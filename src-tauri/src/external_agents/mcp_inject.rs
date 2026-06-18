use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde_json::json;

use crate::external_agents::types::ExternalMcpInjection;
use crate::settings::ChatMcpServer;

pub fn inject_claude_mcp_json(
    cwd: &Path,
    servers: &[ChatMcpServer],
    can_write: bool,
) -> Result<(), String> {
    if !can_write {
        return Ok(());
    }

    let target = cwd.join(".mcp.json");
    let enabled: Vec<&ChatMcpServer> = servers.iter().filter(|s| s.enabled).collect();
    if enabled.is_empty() {
        let _ = fs::remove_file(&target);
        return Ok(());
    }

    let mut mcp_servers = BTreeMap::new();
    for server in enabled {
        let key = if server.id.trim().is_empty() {
            server.name.clone()
        } else {
            server.id.clone()
        };
        let entry = match server.transport.as_str() {
            "stdio" if !server.command.trim().is_empty() => json!({
                "command": server.command,
                "args": server.args,
                "env": server.env,
            }),
            "http" | "sse" if !server.url.trim().is_empty() => {
                let headers = server.headers.clone();
                json!({
                    "url": server.url.trim(),
                    "headers": headers,
                })
            }
            _ => continue,
        };
        mcp_servers.insert(key, entry);
    }

    if mcp_servers.is_empty() {
        let _ = fs::remove_file(&target);
        return Ok(());
    }

    let payload = json!({ "mcpServers": mcp_servers });
    fs::write(
        &target,
        serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

pub fn build_opencode_config_content(servers: &[ChatMcpServer]) -> Option<String> {
    let enabled: Vec<&ChatMcpServer> = servers.iter().filter(|s| s.enabled).collect();
    if enabled.is_empty() {
        return None;
    }
    let mut mcp = BTreeMap::new();
    for server in enabled {
        let key = if server.id.trim().is_empty() {
            server.name.clone()
        } else {
            server.id.clone()
        };
        match server.transport.as_str() {
            "stdio" if !server.command.trim().is_empty() => {
                let mut entry = json!({
                    "type": "local",
                    "command": std::iter::once(server.command.as_str())
                        .chain(server.args.iter().map(String::as_str))
                        .collect::<Vec<_>>(),
                    "enabled": true,
                });
                if !server.env.is_empty() {
                    entry["environment"] = json!(server.env);
                }
                mcp.insert(key, entry);
            }
            "http" | "sse" if !server.url.trim().is_empty() => {
                let mut entry = json!({
                    "type": "remote",
                    "url": server.url.trim(),
                    "enabled": true,
                });
                if !server.headers.is_empty() {
                    entry["headers"] = json!(server.headers);
                }
                mcp.insert(key, entry);
            }
            _ => {}
        }
    }
    if mcp.is_empty() {
        return None;
    }
    serde_json::to_string(&json!({ "mcp": mcp })).ok()
}

pub fn build_spawn_extra_env(
    injection: Option<ExternalMcpInjection>,
    servers: &[ChatMcpServer],
) -> std::collections::HashMap<String, String> {
    let mut env = std::collections::HashMap::new();
    if injection == Some(ExternalMcpInjection::OpenCodeEnvContent) {
        if let Some(content) = build_opencode_config_content(servers) {
            env.insert("OPENCODE_CONFIG_CONTENT".to_string(), content);
        }
    }
    env
}

pub fn apply_mcp_injection(
    injection: Option<ExternalMcpInjection>,
    cwd: &Path,
    servers: &[ChatMcpServer],
    can_write: bool,
) -> Result<(), String> {
    match injection {
        Some(ExternalMcpInjection::ClaudeMcpJson) => {
            inject_claude_mcp_json(cwd, servers, can_write)
        }
        Some(ExternalMcpInjection::OpenCodeEnvContent) | Some(ExternalMcpInjection::AcpMerge) => {
            Ok(())
        }
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn writes_claude_mcp_json_for_stdio_server() {
        let tmp = std::env::temp_dir().join(format!("kivio-mcp-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&tmp).unwrap();
        let servers = vec![ChatMcpServer {
            id: "local".to_string(),
            name: "Local".to_string(),
            enabled: true,
            command: "node".to_string(),
            args: vec!["server.js".to_string()],
            transport: "stdio".to_string(),
            env: HashMap::new(),
            ..Default::default()
        }];
        inject_claude_mcp_json(&tmp, &servers, true).unwrap();
        let raw = fs::read_to_string(tmp.join(".mcp.json")).unwrap();
        assert!(raw.contains("mcpServers"));
        assert!(raw.contains("node"));
        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn builds_opencode_config_content() {
        let servers = vec![ChatMcpServer {
            id: "local".to_string(),
            name: "Local".to_string(),
            enabled: true,
            command: "node".to_string(),
            args: vec!["server.js".to_string()],
            transport: "stdio".to_string(),
            ..Default::default()
        }];
        let content = build_opencode_config_content(&servers).unwrap();
        assert!(content.contains("\"mcp\""));
        assert!(content.contains("node"));
    }
}
