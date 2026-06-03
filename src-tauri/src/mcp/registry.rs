use std::{collections::HashMap, fs, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::{
    settings::{ChatMcpServer, WebSearchProvider},
    state::AppState,
    web_search,
};

use super::{
    client::{StdioMcpClient, StreamableHttpMcpClient},
    types::{
        native_web_search_tool, tool_definition_from_mcp, ChatToolDefinition, McpToolCallResult,
    },
};

const TOOL_LIST_CACHE_TTL: Duration = Duration::from_secs(5 * 60);

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

    let mut tools = Vec::new();

    if settings.chat_tools.native_tools.web_search && web_search_configured(&settings) {
        tools.push(native_web_search_tool());
    }

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
    state: &AppState,
    tool: &ChatToolDefinition,
    arguments: Value,
) -> Result<McpToolCallResult, String> {
    if tool.source == "native" && tool.name == "web_search" {
        let settings = state.settings_read().clone();
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
        let content = web_search::format_web_context(&results);
        return Ok(McpToolCallResult {
            content,
            is_error: false,
            raw: serde_json::to_value(results).unwrap_or(Value::Null),
        });
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
