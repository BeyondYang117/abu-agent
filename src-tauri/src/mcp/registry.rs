use std::{collections::HashMap, fs, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::oneshot;

use crate::{
    settings::{
        ChatMcpServer, WebSearchProvider, CHAT_TOOL_MAX_TIMEOUT_MS, CHAT_TOOL_MIN_TIMEOUT_MS,
        SKILL_SCRIPT_MIN_TIMEOUT_MS,
    },
    state::AppState,
    web_search,
};

use super::{
    client::{StdioMcpClient, StreamableHttpMcpClient},
    types::{
        list_native_builtin_tool_defs, native_skill_tools, tool_definition_from_mcp,
        ChatToolDefinition, McpToolCallResult,
    },
};

const TOOL_LIST_CACHE_TTL: Duration = Duration::from_secs(5 * 60);

pub(crate) fn effective_skill_script_timeout_ms(
    default_timeout_ms: u64,
    requested_timeout_ms: Option<u64>,
) -> u64 {
    let base_timeout_ms = default_timeout_ms.max(SKILL_SCRIPT_MIN_TIMEOUT_MS);
    requested_timeout_ms
        .unwrap_or(base_timeout_ms)
        .clamp(CHAT_TOOL_MIN_TIMEOUT_MS, CHAT_TOOL_MAX_TIMEOUT_MS)
        .max(base_timeout_ms)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpListToolsResult {
    pub success: bool,
    pub tools: Vec<ChatToolDefinition>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTestResult {
    pub success: bool,
    pub tools: Vec<ChatToolDefinition>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpImportResult {
    pub success: bool,
    pub servers: Vec<ChatMcpServer>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CursorMcpJson {
    #[serde(default, rename = "mcpServers")]
    mcp_servers: HashMap<String, CursorMcpServer>,
}

#[derive(Debug, Deserialize)]
struct CursorMcpServer {
    #[serde(default)]
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    url: String,
    #[serde(default)]
    transport: Option<String>,
    #[serde(default, rename = "type")]
    server_type: Option<String>,
}

#[tauri::command]
pub async fn chat_mcp_list_tools(state: State<'_, AppState>) -> Result<McpListToolsResult, String> {
    Ok(match list_enabled_tool_defs(&state).await {
        Ok(tools) => McpListToolsResult {
            success: true,
            tools,
            error: None,
        },
        Err(err) => McpListToolsResult {
            success: false,
            tools: Vec::new(),
            error: Some(err),
        },
    })
}

pub async fn list_enabled_tool_defs(state: &AppState) -> Result<Vec<ChatToolDefinition>, String> {
    let settings = state.settings_read().clone();
    let cache_key = enabled_tools_cache_key(&settings);
    if let Some(tools) = state.get_cached_chat_tools(&cache_key, TOOL_LIST_CACHE_TTL) {
        return Ok(tools);
    }

    let mut tools = list_native_builtin_tool_defs(
        &settings.chat_tools.native_tools,
        web_search_configured(&settings),
    );

    if settings.chat_tools.enabled {
        for server in settings
            .chat_tools
            .servers
            .iter()
            .filter(|server| server.enabled)
        {
            match list_server_tools(server, settings.chat_tools.tool_timeout_ms).await {
                Ok(mut server_tools) => tools.append(&mut server_tools),
                Err(err) => {
                    eprintln!(
                        "MCP server {} failed while listing tools: {err}",
                        server.name
                    );
                }
            }
        }
    }

    tools.extend(list_skill_tool_defs(&settings));

    state.set_cached_chat_tools(cache_key, tools.clone());
    Ok(tools)
}

#[tauri::command]
pub async fn chat_mcp_test_server(server: ChatMcpServer, timeout_ms: Option<u64>) -> McpTestResult {
    match list_server_tools(&server, timeout_ms.unwrap_or(60_000)).await {
        Ok(tools) => McpTestResult {
            success: true,
            tools,
            error: None,
        },
        Err(err) => McpTestResult {
            success: false,
            tools: Vec::new(),
            error: Some(err),
        },
    }
}

#[tauri::command]
pub fn chat_mcp_import_json(path: String) -> McpImportResult {
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) => {
            return McpImportResult {
                success: false,
                servers: Vec::new(),
                error: Some(format!("Read mcp.json failed: {err}")),
            }
        }
    };
    let parsed: CursorMcpJson = match serde_json::from_str(&raw) {
        Ok(parsed) => parsed,
        Err(err) => {
            return McpImportResult {
                success: false,
                servers: Vec::new(),
                error: Some(format!("Parse mcp.json failed: {err}")),
            }
        }
    };

    let servers = parsed
        .mcp_servers
        .into_iter()
        .map(|(name, server)| ChatMcpServer {
            id: format!("mcp-{}", uuid::Uuid::new_v4()),
            name,
            enabled: false,
            transport: normalize_imported_transport(&server),
            url: server.url,
            command: server.command,
            args: server.args,
            env: server.env,
            headers: server.headers,
            cwd: server.cwd,
            enabled_tools: Vec::new(),
        })
        .collect();

    McpImportResult {
        success: true,
        servers,
        error: None,
    }
}

pub async fn call_tool(
    app: &AppHandle,
    state: &AppState,
    tool: &ChatToolDefinition,
    arguments: Value,
    skill_cache: Option<&mut crate::skills::SkillRunCache>,
) -> Result<McpToolCallResult, String> {
    if tool.source == "native" {
        return call_native_tool(app, state, tool, arguments).await;
    }

    if tool.source == "skill" {
        return call_skill_tool(app, state, tool, arguments, skill_cache).await;
    }

    let server_id = tool
        .server_id
        .as_deref()
        .ok_or_else(|| "MCP tool has no server id".to_string())?;
    let settings = state.settings_read().clone();
    let server = settings
        .chat_tools
        .servers
        .iter()
        .find(|server| server.id == server_id && server.enabled)
        .cloned()
        .ok_or_else(|| "MCP server is disabled or missing".to_string())?;
    match server.transport.as_str() {
        "streamable_http" => {
            StreamableHttpMcpClient::new(
                server,
                settings.chat_tools.tool_timeout_ms,
                state.http.clone(),
            )
            .call_tool(&tool.name, arguments)
            .await
        }
        _ => {
            StdioMcpClient::new(server, settings.chat_tools.tool_timeout_ms)
                .call_tool(&tool.name, arguments)
                .await
        }
    }
}

async fn list_server_tools(
    server: &ChatMcpServer,
    timeout_ms: u64,
) -> Result<Vec<ChatToolDefinition>, String> {
    if !server.enabled && !server.command.trim().is_empty() {
        // Test connection passes in disabled draft configs; listing enabled tools filters elsewhere.
    }
    let tools = match server.transport.as_str() {
        "streamable_http" => {
            StreamableHttpMcpClient::new(server.clone(), timeout_ms, reqwest::Client::new())
                .list_tools()
                .await?
        }
        _ => {
            StdioMcpClient::new(server.clone(), timeout_ms)
                .list_tools()
                .await?
        }
    };
    let allowed = server
        .enabled_tools
        .iter()
        .map(|tool| tool.as_str())
        .collect::<Vec<_>>();
    Ok(tools
        .into_iter()
        .filter(|tool| allowed.is_empty() || allowed.contains(&tool.name.as_str()))
        .map(|tool| tool_definition_from_mcp(server, tool))
        .collect())
}

fn normalize_imported_transport(server: &CursorMcpServer) -> String {
    let raw = server
        .transport
        .as_deref()
        .or(server.server_type.as_deref())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if raw == "http" || raw == "sse" || raw == "streamable_http" || !server.url.trim().is_empty() {
        "streamable_http".to_string()
    } else {
        "stdio".to_string()
    }
}

fn enabled_tools_cache_key(settings: &crate::settings::Settings) -> String {
    serde_json::to_string(&serde_json::json!({
        "chatTools": settings.chat_tools,
        "lensWebSearchProvider": settings.lens.web_search.provider,
        "lensWebSearchMaxResults": settings.lens.web_search.max_results,
        "lensWebSearchDepth": settings.lens.web_search.search_depth,
    }))
    .unwrap_or_else(|_| "chat-tools".to_string())
}

fn web_search_configured(settings: &crate::settings::Settings) -> bool {
    match settings.lens.web_search.provider {
        WebSearchProvider::Tavily => !settings.lens.web_search.tavily_api_key.trim().is_empty(),
        WebSearchProvider::Exa => !settings.lens.web_search.exa_api_key.trim().is_empty(),
    }
}

async fn call_skill_tool(
    app: &AppHandle,
    state: &AppState,
    tool: &ChatToolDefinition,
    arguments: Value,
    skill_cache: Option<&mut crate::skills::SkillRunCache>,
) -> Result<McpToolCallResult, String> {
    let settings = state.settings_read().clone();
    let registry = crate::skills::build_registry(app, &settings.chat_tools.skill_scan_paths)?;
    let skill_name = crate::skills::extract_skill_name(&arguments)?;
    let record = crate::skills::lookup_skill(&registry, &skill_name)
        .ok_or_else(|| format!("Skill not found: {skill_name}"))?;
    if !crate::settings::is_skill_enabled(&settings.chat_tools, &record.meta.id) {
        return Err(format!("Skill is disabled in Settings: {skill_name}"));
    }

    let content = match tool.name.as_str() {
        "skill_activate" => {
            if let Some(cache) = skill_cache {
                cache.activate_with_cache(record)
            } else {
                crate::skills::activate_skill(record)
            }
        }
        "skill_read_file" => {
            let relative_path = crate::skills::extract_relative_path(&arguments)?;
            if let Some(cache) = skill_cache {
                cache.read_file_with_cache(record, &relative_path)?
            } else {
                crate::skills::read_skill_file(record, &relative_path)?
            }
        }
        "skill_run_script" => {
            let relative_path = crate::skills::extract_relative_path(&arguments)?;
            let args = crate::skills::extract_script_args(&arguments);
            let timeout_ms = effective_skill_script_timeout_ms(
                settings.chat_tools.tool_timeout_ms,
                arguments.get("timeout_ms").and_then(|value| value.as_u64()),
            );
            crate::skills::run_skill_script(
                record,
                &relative_path,
                &args,
                timeout_ms,
                &settings.chat_tools.skill_script_allowlist,
            )
            .await?
        }
        other => return Err(format!("Unknown skill tool: {other}")),
    };

    Ok(McpToolCallResult {
        content,
        is_error: false,
        raw: Value::Null,
        artifacts: Vec::new(),
    })
}

pub fn skill_runtime_tools_enabled(settings: &crate::settings::Settings) -> bool {
    settings.chat_tools.native_tools.skill_runtime
}

pub fn list_skill_tool_defs(settings: &crate::settings::Settings) -> Vec<ChatToolDefinition> {
    if skill_runtime_tools_enabled(settings) {
        native_skill_tools()
    } else {
        Vec::new()
    }
}

async fn call_native_tool(
    app: &AppHandle,
    state: &AppState,
    tool: &ChatToolDefinition,
    arguments: Value,
) -> Result<McpToolCallResult, String> {
    let settings = state.settings_read().clone();
    let roots = &settings.chat_tools.native_tools.workspace_roots;

    if tool.name == "run_python" {
        return run_python_via_pyodide(app, state, &settings, &arguments).await;
    }

    let content = match tool.name.as_str() {
        "web_search" => {
            let query = arguments
                .get("query")
                .and_then(|query| query.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            if query.is_empty() {
                return Err("web_search query is empty".to_string());
            }
            let retry_attempts = if settings.retry_enabled {
                settings.retry_attempts as usize
            } else {
                1
            };
            let results =
                web_search::search_web(state, &settings.lens.web_search, &query, retry_attempts)
                    .await?;
            let raw = serde_json::to_value(&results).unwrap_or(Value::Null);
            return Ok(McpToolCallResult {
                content: web_search::format_web_context(&results),
                is_error: false,
                raw,
                artifacts: Vec::new(),
            });
        }
        "web_fetch" => crate::native_tools::web_fetch(&state.http, &arguments).await?,
        "read_file" => crate::native_tools::read_file(&arguments)?,
        "write_file" => crate::native_tools::write_file(roots, &arguments)?,
        "edit_file" => crate::native_tools::edit_file(roots, &arguments)?,
        "run_command" => {
            crate::native_tools::run_command(roots, settings.chat_tools.tool_timeout_ms, &arguments)
                .await?
        }
        other => return Err(format!("Unknown native tool: {other}")),
    };

    Ok(McpToolCallResult {
        content,
        is_error: false,
        raw: Value::Null,
        artifacts: Vec::new(),
    })
}

async fn run_python_via_pyodide(
    app: &AppHandle,
    state: &AppState,
    settings: &crate::settings::Settings,
    arguments: &Value,
) -> Result<McpToolCallResult, String> {
    let code = arguments
        .get("code")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "run_python requires code".to_string())?;

    let run_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel();
    {
        let mut pending = state
            .pending_python_runs
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        pending.insert(run_id.clone(), tx);
    }

    let timeout_ms = arguments
        .get("timeout_ms")
        .and_then(|value| value.as_u64())
        .unwrap_or(settings.chat_tools.tool_timeout_ms)
        .clamp(1_000, 300_000);
    let emit_result = app.emit(
        "chat-run-python",
        serde_json::json!({
            "runId": run_id,
            "code": code,
            "timeoutMs": timeout_ms,
        }),
    );
    if let Err(err) = emit_result {
        let mut pending = state
            .pending_python_runs
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        pending.remove(&run_id);
        return Err(format!("Failed to start Python runner: {err}"));
    }

    let wait =
        tokio::time::timeout(Duration::from_millis(timeout_ms.saturating_add(5_000)), rx).await;

    match wait {
        Ok(Ok(result)) => {
            if result.is_error {
                Err(result.content)
            } else {
                Ok(McpToolCallResult {
                    content: result.content,
                    is_error: false,
                    raw: Value::Null,
                    artifacts: result.artifacts,
                })
            }
        }
        Ok(Err(_)) => Err("Python runner channel closed".to_string()),
        Err(_) => {
            let mut pending = state
                .pending_python_runs
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            pending.remove(&run_id);
            Err(format!("Python execution timed out after {timeout_ms}ms"))
        }
    }
}
