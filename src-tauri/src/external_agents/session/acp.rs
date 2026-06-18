use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::timeout;

use crate::external_agents::stream::usage_from_numbers;
use crate::external_agents::types::{RuntimeModelOption, UnifiedAgentEvent, default_model_option};
use crate::settings::ChatMcpServer;

const ACP_PROTOCOL_VERSION: i64 = 1;

#[derive(Debug, Clone)]
pub struct AcpMcpServer {
    pub server_type: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

pub fn build_acp_mcp_servers(servers: &[ChatMcpServer]) -> Vec<AcpMcpServer> {
    servers
        .iter()
        .filter(|s| s.enabled && s.transport == "stdio" && !s.command.trim().is_empty())
        .map(|s| {
            let name = if s.id.trim().is_empty() {
                s.name.clone()
            } else {
                s.id.clone()
            };
            let env: Vec<(String, String)> = s
                .env
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            AcpMcpServer {
                server_type: "stdio".to_string(),
                name,
                command: s.command.clone(),
                args: s.args.clone(),
                env,
            }
        })
        .collect()
}

fn build_session_new_params(cwd: &Path, mcp_servers: &[AcpMcpServer]) -> Value {
    let servers: Vec<Value> = mcp_servers
        .iter()
        .map(|s| {
            json!({
                "type": s.server_type,
                "name": s.name,
                "command": s.command,
                "args": s.args,
                "env": s.env.iter().map(|(name, value)| json!({ "name": name, "value": value })).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "cwd": cwd.to_string_lossy(),
        "mcpServers": servers,
    })
}

async fn write_rpc(
    stdin: &mut tokio::process::ChildStdin,
    id: u64,
    method: &str,
    params: Value,
) -> Result<(), String> {
    let payload = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    let mut line = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    line.push('\n');
    stdin.write_all(line.as_bytes()).await.map_err(|e| e.to_string())
}

async fn write_rpc_result(
    stdin: &mut tokio::process::ChildStdin,
    id: &Value,
    result: Value,
) -> Result<(), String> {
    let payload = json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    });
    let mut line = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    line.push('\n');
    stdin.write_all(line.as_bytes()).await.map_err(|e| e.to_string())
}

fn rpc_error_message(value: &Value) -> Option<String> {
    let error = value.get("error")?;
    if let Some(message) = error.get("message").and_then(|v| v.as_str()) {
        return Some(message.to_string());
    }
    error
        .get("code")
        .map(|c| c.to_string())
}

fn normalize_models(result: &Value) -> Vec<RuntimeModelOption> {
    let mut out = vec![default_model_option()];
    let mut seen = HashSet::from(["default".to_string()]);

    if let Some(config_options) = result.get("configOptions").and_then(|v| v.as_array()) {
        for raw_option in config_options {
            let option = match raw_option.as_object() {
                Some(o) => o,
                None => continue,
            };
            let config_id = option.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if config_id != "model" && option.get("category").and_then(|v| v.as_str()) != Some("model") {
                continue;
            }
            if let Some(values) = option.get("options").and_then(|v| v.as_array()) {
                for raw_value in values {
                    let value = match raw_value.as_object() {
                        Some(o) => o,
                        None => continue,
                    };
                    let id = value
                        .get("value")
                        .or_else(|| value.get("id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if id.is_empty() || !seen.insert(id.to_string()) {
                        continue;
                    }
                    let name = value.get("name").and_then(|v| v.as_str()).unwrap_or(id);
                    out.push(RuntimeModelOption {
                        id: id.to_string(),
                        label: if name != id {
                            format!("{name} ({id})")
                        } else {
                            id.to_string()
                        },
                    });
                }
            }
            if out.len() > 1 {
                return out;
            }
        }
    }

    if let Some(models) = result.get("models").and_then(|v| v.as_object()) {
        if let Some(available) = models.get("availableModels").and_then(|v| v.as_array()) {
            for model in available {
                let id = model
                    .get("modelId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if id.is_empty() || !seen.insert(id.to_string()) {
                    continue;
                }
                let name = model.get("name").and_then(|v| v.as_str()).unwrap_or(id);
                out.push(RuntimeModelOption {
                    id: id.to_string(),
                    label: if name != id {
                        format!("{name} ({id})")
                    } else {
                        id.to_string()
                    },
                });
            }
        }
    }

    out
}

pub async fn detect_acp_models(
    bin: &Path,
    args: &[&str],
    cwd: &Path,
    timeout_secs: u64,
) -> Option<Vec<RuntimeModelOption>> {
    let mut child = Command::new(bin)
        .args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .ok()?;

    let mut stdin = child.stdin.take()?;
    let stdout = child.stdout.take()?;
    let mut reader = BufReader::new(stdout).lines();

    let mut expected_id: u64 = 1;
    let mut next_id: u64 = 2;
    let mut models: Option<Vec<RuntimeModelOption>> = None;
    let deadline = Duration::from_secs(timeout_secs);

    write_rpc(&mut stdin, 1, "initialize", json!({
        "protocolVersion": ACP_PROTOCOL_VERSION,
        "clientCapabilities": { "terminal": false },
        "clientInfo": { "name": "kivio", "version": "external-agents" },
    }))
    .await
    .ok()?;

    let started = std::time::Instant::now();
    loop {
        if started.elapsed() > deadline {
            let _ = child.start_kill();
            break;
        }
        let line = match timeout(Duration::from_millis(200), reader.next_line()).await {
            Ok(Ok(Some(line))) => line,
            Ok(Ok(None)) => break,
            Ok(Err(_)) => break,
            Err(_) => continue,
        };
        let value: Value = serde_json::from_str(line.trim()).ok()?;
        if rpc_error_message(&value).is_some() {
            if value.get("id").and_then(|v| v.as_u64()) != Some(expected_id) {
                continue;
            }
            let _ = child.start_kill();
            return None;
        }
        if value.get("id").and_then(|v| v.as_u64()) != Some(expected_id) {
            continue;
        }
        let result = value.get("result")?;
        if expected_id == 1 {
            expected_id = next_id;
            write_rpc(
                &mut stdin,
                next_id,
                "session/new",
                build_session_new_params(cwd, &[]),
            )
            .await
            .ok()?;
            next_id += 1;
            continue;
        }
        if expected_id == 2 {
            models = Some(normalize_models(result));
            let _ = child.start_kill();
            break;
        }
    }

    models.filter(|m| m.len() > 1)
}

fn choose_permission_outcome(options: Option<&Value>) -> Option<String> {
    let list = options.and_then(|v| v.as_array())?;
    for item in list {
        if item.get("optionId").and_then(|v| v.as_str()) == Some("approve_for_session") {
            return Some("approve_for_session".to_string());
        }
    }
    for item in list {
        if item.get("kind").and_then(|v| v.as_str()) == Some("allow_always") {
            if let Some(id) = item.get("optionId").and_then(|v| v.as_str()) {
                return Some(id.to_string());
            }
        }
    }
    for item in list {
        if item.get("kind").and_then(|v| v.as_str()) == Some("allow_once") {
            if let Some(id) = item.get("optionId").and_then(|v| v.as_str()) {
                return Some(id.to_string());
            }
        }
    }
    None
}

fn format_acp_usage(usage: &Value) -> Option<crate::chat::model::ModelUsage> {
    let input = usage.get("inputTokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let output = usage.get("outputTokens").and_then(|v| v.as_u64()).unwrap_or(0);
    if input == 0 && output == 0 {
        None
    } else {
        Some(usage_from_numbers(input, output))
    }
}

pub async fn run_acp_session(
    child: &mut Child,
    prompt: &str,
    cwd: &Path,
    model: Option<&str>,
    mcp_servers: &[AcpMcpServer],
    mut sink: impl FnMut(UnifiedAgentEvent),
    cancel_check: impl Fn() -> bool,
) -> Result<(), String> {
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "stdin unavailable".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "stdout unavailable".to_string())?;

    let mut expected_id: u64 = 1;
    let mut next_id: u64 = 2;
    let mut session_id: Option<String> = None;
    let mut prompt_request_id: Option<u64> = None;
    let mut set_model_request_id: Option<u64> = None;
    let mut model_config_id: Option<String> = None;
    let mut emitted_text = String::new();
    let mut finished = false;

    write_rpc(
        &mut stdin,
        1,
        "initialize",
        json!({
            "protocolVersion": ACP_PROTOCOL_VERSION,
            "clientCapabilities": { "terminal": false },
            "clientInfo": { "name": "kivio", "version": "external-agents" },
        }),
    )
    .await?;

    let mut reader = BufReader::new(stdout).lines();

    while !finished {
        if cancel_check() {
            if let Some(ref sid) = session_id {
                let _ = write_rpc(&mut stdin, next_id, "session/cancel", json!({ "sessionId": sid })).await;
            }
            let _ = stdin.shutdown().await;
            let _ = child.start_kill();
            return Err("cancelled".to_string());
        }

        let line = match timeout(Duration::from_millis(200), reader.next_line()).await {
            Ok(Ok(Some(line))) => line,
            Ok(Ok(None)) => {
                if !finished {
                    return Err("ACP session exited before completion".to_string());
                }
                break;
            }
            Ok(Err(e)) => return Err(e.to_string()),
            Err(_) => continue,
        };
        if line.trim().is_empty() {
            continue;
        }

        let value: Value = serde_json::from_str(line.trim())
            .map_err(|e| format!("invalid ACP json: {e}"))?;

        if let Some(method) = value.get("method").and_then(|v| v.as_str()) {
            if method == "session/request_permission" {
                let option_id = choose_permission_outcome(value.get("params").and_then(|p| p.get("options")));
                if let (Some(id), Some(option_id)) = (value.get("id"), option_id) {
                    write_rpc_result(
                        &mut stdin,
                        id,
                        json!({ "outcome": { "outcome": "selected", "optionId": option_id } }),
                    )
                    .await?;
                }
                continue;
            }
            if method == "session/update" {
                if let Some(update) = value
                    .get("params")
                    .and_then(|p| p.get("update"))
                    .and_then(|v| v.as_object())
                {
                    let session_update = update
                        .get("sessionUpdate")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if session_update == "agent_thought_chunk" {
                        if let Some(text) = update
                            .get("content")
                            .and_then(|c| c.get("text"))
                            .and_then(|v| v.as_str())
                        {
                            if !text.is_empty() {
                                sink(UnifiedAgentEvent::ThinkingDelta {
                                    delta: text.to_string(),
                                });
                            }
                        }
                        continue;
                    }
                    if session_update == "agent_message_chunk" {
                        if let Some(text) = update
                            .get("content")
                            .and_then(|c| c.get("text"))
                            .and_then(|v| v.as_str())
                        {
                            if !text.is_empty() {
                                let delta = if text.starts_with(&emitted_text) {
                                    text[emitted_text.len()..].to_string()
                                } else {
                                    text.to_string()
                                };
                                if !delta.is_empty() {
                                    emitted_text.push_str(&delta);
                                    sink(UnifiedAgentEvent::TextDelta { delta });
                                }
                            }
                        }
                        continue;
                    }
                    if session_update != "agent_message_chunk"
                        && session_update != "agent_thought_chunk"
                    {
                        sink(UnifiedAgentEvent::Status {
                            label: session_update.to_string(),
                            model: None,
                        });
                    }
                }
                continue;
            }
        }

        if let Some(err) = rpc_error_message(&value) {
            if value.get("id").and_then(|v| v.as_u64()) != Some(expected_id) {
                continue;
            }
            return Err(err);
        }

        if value.get("id").and_then(|v| v.as_u64()) != Some(expected_id) {
            continue;
        }

        let result = match value.get("result") {
            Some(r) => r,
            None => continue,
        };

        if expected_id == 1 {
            expected_id = next_id;
            write_rpc(
                &mut stdin,
                next_id,
                "session/new",
                build_session_new_params(cwd, mcp_servers),
            )
            .await?;
            next_id += 1;
            continue;
        }

        if expected_id == 2 {
            session_id = result
                .get("sessionId")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            if let Some(config_options) = result.get("configOptions").and_then(|v| v.as_array()) {
                for raw_option in config_options {
                    if let Some(option) = raw_option.as_object() {
                        let id = option.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        if id == "model" || option.get("category").and_then(|v| v.as_str()) == Some("model") {
                            model_config_id = Some(id.to_string());
                            break;
                        }
                    }
                }
            }

            let chosen = model.filter(|m| !m.is_empty() && *m != "default");
            if session_id.is_some() && chosen.is_some() {
                set_model_request_id = Some(next_id);
                expected_id = next_id;
                let sid = session_id.clone().unwrap();
                let chosen = chosen.unwrap();
                if model_config_id.is_some() {
                    write_rpc(
                        &mut stdin,
                        next_id,
                        "session/set_config_option",
                        json!({ "sessionId": sid, "configId": model_config_id, "value": chosen }),
                    )
                    .await?;
                } else {
                    write_rpc(
                        &mut stdin,
                        next_id,
                        "session/set_model",
                        json!({ "sessionId": sid, "modelId": chosen }),
                    )
                    .await?;
                }
                next_id += 1;
                continue;
            }

            if session_id.is_none() {
                return Err("invalid session/new response".to_string());
            }

            prompt_request_id = Some(next_id);
            expected_id = next_id;
            write_rpc(
                &mut stdin,
                next_id,
                "session/prompt",
                json!({
                    "sessionId": session_id,
                    "prompt": [{ "type": "text", "text": prompt }],
                }),
            )
            .await?;
            next_id += 1;
            sink(UnifiedAgentEvent::Status {
                label: "waiting_for_first_output".to_string(),
                model: None,
            });
            continue;
        }

        if set_model_request_id.is_some() && value.get("id").and_then(|v| v.as_u64()) == set_model_request_id {
            set_model_request_id = None;
            prompt_request_id = Some(next_id);
            expected_id = next_id;
            write_rpc(
                &mut stdin,
                next_id,
                "session/prompt",
                json!({
                    "sessionId": session_id,
                    "prompt": [{ "type": "text", "text": prompt }],
                }),
            )
            .await?;
            next_id += 1;
            sink(UnifiedAgentEvent::Status {
                label: "waiting_for_first_output".to_string(),
                model: None,
            });
            continue;
        }

        if prompt_request_id.is_some() && value.get("id").and_then(|v| v.as_u64()) == prompt_request_id {
            if let Some(usage) = result.get("usage").and_then(format_acp_usage) {
                sink(UnifiedAgentEvent::Usage { usage });
            }
            finished = true;
            let _ = stdin.shutdown().await;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_models_from_available() {
        let result = json!({
            "models": {
                "availableModels": [
                    { "modelId": "grok-4.3", "name": "Grok 4.3" }
                ]
            }
        });
        let models = normalize_models(&result);
        assert!(models.iter().any(|m| m.id == "grok-4.3"));
    }

    #[test]
    fn build_acp_mcp_servers_stdio_only() {
        let servers = vec![ChatMcpServer {
            id: "local".to_string(),
            name: "Local".to_string(),
            enabled: true,
            command: "node".to_string(),
            args: vec!["server.js".to_string()],
            transport: "stdio".to_string(),
            ..Default::default()
        }];
        let out = build_acp_mcp_servers(&servers);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].command, "node");
    }
}
