use serde::{Deserialize, Serialize};

use crate::settings::{ChatMcpServer, ChatNativeToolsConfig};

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
        match self.source.as_str() {
            // Native and Skill tools are model-facing APIs owned by Kivio. Keep their names
            // aligned with the system prompt so models can call exactly what we instruct.
            "native" | "skill" => sanitize_openai_tool_name(&self.name),
            _ => sanitize_openai_tool_name(&self.id),
        }
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
    #[serde(default)]
    pub artifacts: Vec<ChatToolArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatToolArtifact {
    pub name: String,
    #[serde(alias = "mimeType")]
    pub mime_type: String,
    #[serde(alias = "dataUrl")]
    pub data_url: String,
    #[serde(default, alias = "sizeBytes")]
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonRunResult {
    pub content: String,
    #[serde(alias = "isError")]
    pub is_error: bool,
    #[serde(default)]
    pub artifacts: Vec<ChatToolArtifact>,
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

pub fn native_skill_activate_tool() -> ChatToolDefinition {
    ChatToolDefinition {
        id: "skill__activate".to_string(),
        name: "skill_activate".to_string(),
        description: "Activate an Agent Skill by name. Always call this first when a task matches a skill. Loads SKILL.md instructions and lists bundled scripts and reference files.".to_string(),
        source: "skill".to_string(),
        server_id: None,
        server_name: Some("Skill".to_string()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Skill name from available_skills"
                }
            },
            "required": ["name"]
        }),
        sensitive: false,
    }
}

pub fn native_skill_read_file_tool() -> ChatToolDefinition {
    ChatToolDefinition {
        id: "skill__read_file".to_string(),
        name: "skill_read_file".to_string(),
        description: "Read a file from a skill directory (references/, secrets/, etc.) using a path relative to the skill root. Call skill_activate first.".to_string(),
        source: "skill".to_string(),
        server_id: None,
        server_name: Some("Skill".to_string()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Skill name"
                },
                "relative_path": {
                    "type": "string",
                    "description": "Path relative to the skill root, e.g. references/guide.md"
                }
            },
            "required": ["name", "relative_path"]
        }),
        sensitive: false,
    }
}

pub fn native_skill_run_script_tool() -> ChatToolDefinition {
    ChatToolDefinition {
        id: "skill__run_script".to_string(),
        name: "skill_run_script".to_string(),
        description: "Execute a bundled script under scripts/ (e.g. scripts/tavily_cli.py). Pass CLI args via args. Use this instead of describing commands when a skill provides scripts.".to_string(),
        source: "skill".to_string(),
        server_id: None,
        server_name: Some("Skill".to_string()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Skill name"
                },
                "relative_path": {
                    "type": "string",
                    "description": "Script path relative to the skill root, must start with scripts/"
                },
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional script arguments passed after the script path"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Timeout in ms (optional, max 300000)"
                }
            },
            "required": ["name", "relative_path"]
        }),
        sensitive: false,
    }
}

pub fn native_skill_tools() -> Vec<ChatToolDefinition> {
    vec![
        native_skill_activate_tool(),
        native_skill_read_file_tool(),
        native_skill_run_script_tool(),
    ]
}

pub fn native_read_file_tool() -> ChatToolDefinition {
    ChatToolDefinition {
        id: "native__read_file".to_string(),
        name: "read_file".to_string(),
        description: "Read any local text file that Kivio can access. Optional offset/limit are 1-based line numbers.".to_string(),
        source: "native".to_string(),
        server_id: None,
        server_name: Some("Kivio".to_string()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute or home-relative file path" },
                "offset": { "type": "integer", "description": "1-based start line (optional)" },
                "limit": { "type": "integer", "description": "Max lines to return (optional)" }
            },
            "required": ["path"]
        }),
        sensitive: false,
    }
}

pub fn native_write_file_tool() -> ChatToolDefinition {
    ChatToolDefinition {
        id: "native__write_file".to_string(),
        name: "write_file".to_string(),
        description: "Write or overwrite a text file under the user home directory.".to_string(),
        source: "native".to_string(),
        server_id: None,
        server_name: Some("Kivio".to_string()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["path", "content"]
        }),
        sensitive: true,
    }
}

pub fn native_edit_file_tool() -> ChatToolDefinition {
    ChatToolDefinition {
        id: "native__edit_file".to_string(),
        name: "edit_file".to_string(),
        description: "Replace old_string with new_string in a text file. Fails if old_string is missing or appears multiple times unless replace_all is true.".to_string(),
        source: "native".to_string(),
        server_id: None,
        server_name: Some("Kivio".to_string()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "old_string": { "type": "string" },
                "new_string": { "type": "string" },
                "replace_all": { "type": "boolean" }
            },
            "required": ["path", "old_string", "new_string"]
        }),
        sensitive: true,
    }
}

pub fn native_run_command_tool() -> ChatToolDefinition {
    ChatToolDefinition {
        id: "native__run_command".to_string(),
        name: "run_command".to_string(),
        description: "Run a shell command (build, test, etc.) in a working directory under the user home. Requires user approval. A non-zero exit code is returned as a tool error with stdout/stderr. Do not use this to run Skill scripts; use skill_run_script for bundled Skill scripts. Do not use pip to bypass run_python sandbox failures; host Python package installs require an explicit user request and allow_host_python_package_install=true.".to_string(),
        source: "native".to_string(),
        server_id: None,
        server_name: Some("Kivio".to_string()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Shell command" },
                "cwd": { "type": "string", "description": "Working directory (optional)" },
                "timeout_ms": { "type": "integer", "description": "Timeout in ms (optional)" },
                "allow_host_python_package_install": { "type": "boolean", "description": "Only true when the user explicitly asked to modify the host Python environment; installs must use --user or a virtual environment." }
            },
            "required": ["command"]
        }),
        sensitive: true,
    }
}

pub fn native_run_python_tool() -> ChatToolDefinition {
    ChatToolDefinition {
        id: "native__run_python".to_string(),
        name: "run_python".to_string(),
        description: "Execute Python code in a Pyodide sandbox (no host filesystem or network). Use for calculation, statistics, basic ML, and chart/data code. Do not use network/API client packages such as tavily, requests, httpx, urllib3, or aiohttp; use web_search/web_fetch or Skill scripts for live web/API access. Common Pyodide packages such as numpy, matplotlib, pandas, scipy, sympy, scikit-learn, statsmodels, pillow, seaborn, and micropip are auto-loaded when imported. stdout/stderr are returned.".to_string(),
        source: "native".to_string(),
        server_id: None,
        server_name: Some("Kivio".to_string()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "code": { "type": "string", "description": "Python source code" },
                "timeout_ms": { "type": "integer", "description": "Timeout in ms (optional, max 300000)" }
            },
            "required": ["code"]
        }),
        sensitive: false,
    }
}

pub fn native_web_fetch_tool() -> ChatToolDefinition {
    ChatToolDefinition {
        id: "native__web_fetch".to_string(),
        name: "web_fetch".to_string(),
        description: "Fetch readable text from an HTTPS URL (HTML is stripped to plain text)."
            .to_string(),
        source: "native".to_string(),
        server_id: None,
        server_name: Some("Kivio".to_string()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "HTTPS URL" }
            },
            "required": ["url"]
        }),
        sensitive: false,
    }
}

pub fn list_native_builtin_tool_defs(
    native: &ChatNativeToolsConfig,
    web_search_configured: bool,
) -> Vec<ChatToolDefinition> {
    let mut tools = Vec::new();
    if native.web_search && web_search_configured {
        tools.push(native_web_search_tool());
    }
    if native.web_fetch {
        tools.push(native_web_fetch_tool());
    }
    if native.read_file {
        tools.push(native_read_file_tool());
    }
    if native.write_file {
        tools.push(native_write_file_tool());
    }
    if native.edit_file {
        tools.push(native_edit_file_tool());
    }
    if native.run_command {
        tools.push(native_run_command_tool());
    }
    if native.run_python {
        tools.push(native_run_python_tool());
    }
    tools
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

    #[test]
    fn skill_and_native_tools_use_prompt_facing_names() {
        assert_eq!(
            native_skill_activate_tool().openai_tool_name(),
            "skill_activate"
        );
        assert_eq!(
            native_skill_read_file_tool().openai_tool_name(),
            "skill_read_file"
        );
        assert_eq!(native_web_search_tool().openai_tool_name(), "web_search");
    }

    #[test]
    fn builtin_skill_tools_are_not_marked_sensitive() {
        assert!(!native_skill_activate_tool().sensitive);
        assert!(!native_skill_read_file_tool().sensitive);
        assert!(!native_skill_run_script_tool().sensitive);
    }

    #[test]
    fn native_file_and_web_tools_have_expected_sensitivity() {
        assert!(!native_read_file_tool().sensitive);
        assert!(!native_web_fetch_tool().sensitive);
        assert!(!native_run_python_tool().sensitive);
        assert!(native_write_file_tool().sensitive);
        assert!(native_edit_file_tool().sensitive);
        assert!(native_run_command_tool().sensitive);
    }

    #[test]
    fn mcp_tools_keep_namespaced_openai_names() {
        let server = ChatMcpServer {
            id: "server.one".to_string(),
            name: "Server One".to_string(),
            enabled: true,
            transport: "stdio".to_string(),
            url: String::new(),
            command: "demo".to_string(),
            args: Vec::new(),
            env: std::collections::HashMap::new(),
            headers: std::collections::HashMap::new(),
            cwd: None,
            enabled_tools: Vec::new(),
        };
        let tool = tool_definition_from_mcp(
            &server,
            McpTool {
                name: "search.web".to_string(),
                description: String::new(),
                input_schema: serde_json::json!({ "type": "object" }),
            },
        );

        assert_eq!(tool.openai_tool_name(), "mcp__server_one__search_web");
        assert_ne!(tool.openai_tool_name(), tool.name);
    }
}
