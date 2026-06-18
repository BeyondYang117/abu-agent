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
                let mut headers = server.headers.clone();
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
    fn skips_write_when_not_allowed() {
        let tmp = std::env::temp_dir().join(format!("kivio-mcp-skip-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&tmp).unwrap();
        let servers = vec![ChatMcpServer {
            id: "x".to_string(),
            name: "X".to_string(),
            enabled: true,
            command: "node".to_string(),
            transport: "stdio".to_string(),
            ..Default::default()
        }];
        inject_claude_mcp_json(&tmp, &servers, false).unwrap();
        assert!(!tmp.join(".mcp.json").exists());
        let _ = fs::remove_dir_all(tmp);
    }
}
