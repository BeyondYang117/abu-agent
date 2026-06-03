use serde::{Deserialize, Serialize};

use crate::settings::ChatMcpServer;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatToolDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub server_id: Option<String>,
    pub server_name: Option<String>,
    pub input_schema: serde_json::Value,
    pub sensitive: bool,
}

impl ChatToolDefinition {
    pub fn openai_tool_name(&self) -> String {
        sanitize_openai_tool_name(&self.id)
    }

    pub fn to_openai_tool(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": self.openai_tool_name(),
                "description": self.description,
                "parameters": self.input_schema,
            }
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolCallResult {
    pub content: String,
    pub is_error: bool,
    pub raw: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct McpTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

pub fn tool_definition_from_mcp(server: &ChatMcpServer, tool: McpTool) -> ChatToolDefinition {
    let id = format!("mcp__{}__{}", server.id, tool.name);
    ChatToolDefinition {
        id,
        name: tool.name.clone(),
        description: if tool.description.trim().is_empty() {
            format!("MCP tool {}", tool.name)
        } else {
            tool.description
        },
        source: "mcp".to_string(),
        server_id: Some(server.id.clone()),
        server_name: Some(server.name.clone()),
        input_schema: if tool.input_schema.is_null() {
            serde_json::json!({ "type": "object", "properties": {} })
        } else {
            tool.input_schema
        },
        sensitive: looks_sensitive_tool(&tool.name),
    }
}

pub fn native_web_search_tool() -> ChatToolDefinition {
    ChatToolDefinition {
        id: "native__web_search".to_string(),
        name: "web_search".to_string(),
        description: "Search the web for current facts and return source snippets.".to_string(),
        source: "native".to_string(),
        server_id: None,
        server_name: Some("Kivio".to_string()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query"
                }
            },
            "required": ["query"]
        }),
        sensitive: false,
    }
}

pub fn sanitize_openai_tool_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "tool".to_string()
    } else {
        trimmed.chars().take(64).collect()
    }
}

pub fn looks_sensitive_tool(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "write", "delete", "remove", "exec", "shell", "command", "run", "update", "patch", "move",
        "rename", "create", "save", "upload", "publish", "replace", "modify", "edit", "insert",
        "drop", "truncate", "grant", "revoke", "deploy", "apply",
    ]
    .iter()
    .any(|needle| name.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_tool_detection_covers_common_write_verbs() {
        for name in ["save_file", "uploadAsset", "publish_page", "replace_rows"] {
            assert!(
                looks_sensitive_tool(name),
                "{name} should require confirmation"
            );
        }
        assert!(!looks_sensitive_tool("read_file"));
        assert!(!looks_sensitive_tool("web_search"));
    }
}
