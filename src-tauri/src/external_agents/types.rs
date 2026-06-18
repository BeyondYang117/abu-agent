use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::chat::model::ModelUsage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamFormat {
    ClaudeStreamJson,
    JsonEventStream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JsonEventParser {
    Codex,
    CursorAgent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalMcpInjection {
    ClaudeMcpJson,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptInputFormat {
    Text,
    StreamJson,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeModelOption {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedAgent {
    pub id: String,
    pub name: String,
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub models: Vec<RuntimeModelOption>,
    pub reasoning_options: Vec<RuntimeModelOption>,
    pub auth_status: Option<String>,
    pub external_mcp_injection: Option<ExternalMcpInjection>,
}

#[derive(Debug, Clone)]
pub struct RuntimeBuildOptions {
    pub model: Option<String>,
    pub reasoning: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RuntimeContext {
    pub cwd: Option<String>,
    pub extra_allowed_dirs: Vec<String>,
    pub resume_session_id: Option<String>,
    pub new_session_id: Option<String>,
    pub include_partial_messages: bool,
}

#[derive(Debug, Clone)]
pub struct RuntimeAgentDef {
    pub id: &'static str,
    pub name: &'static str,
    pub bin: &'static str,
    pub fallback_bins: &'static [&'static str],
    pub version_args: &'static [&'static str],
    pub auth_probe_args: Option<&'static [&'static str]>,
    pub fallback_models: &'static [(&'static str, &'static str)],
    pub reasoning_options: &'static [(&'static str, &'static str)],
    pub list_models_args: Option<&'static [&'static str]>,
    pub prompt_via_stdin: bool,
    pub prompt_input_format: PromptInputFormat,
    pub stream_format: StreamFormat,
    pub json_event_parser: Option<JsonEventParser>,
    pub external_mcp_injection: Option<ExternalMcpInjection>,
    pub resumes_session_via_cli: bool,
    pub build_args: fn(&RuntimeContext, &RuntimeBuildOptions) -> Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UnifiedAgentEvent {
    Status {
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    TextDelta {
        delta: String,
    },
    ThinkingDelta {
        delta: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
    Usage {
        usage: ModelUsage,
    },
    TurnEnd {
        stop_reason: String,
    },
    Error {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<String>,
    },
    Raw {
        line: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAgentSession {
    pub conversation_id: String,
    pub agent_id: String,
    pub session_id: String,
    pub stable_prompt_hash: Option<String>,
}

pub fn default_model_option() -> RuntimeModelOption {
    RuntimeModelOption {
        id: "default".to_string(),
        label: "Default".to_string(),
    }
}

pub fn fallback_models_from_pairs(pairs: &[(&str, &str)]) -> Vec<RuntimeModelOption> {
    let mut out = vec![default_model_option()];
    for (id, label) in pairs {
        if *id == "default" {
            continue;
        }
        out.push(RuntimeModelOption {
            id: (*id).to_string(),
            label: (*label).to_string(),
        });
    }
    out
}

pub fn reasoning_options_from_pairs(pairs: &[(&str, &str)]) -> Vec<RuntimeModelOption> {
    pairs
        .iter()
        .map(|(id, label)| RuntimeModelOption {
            id: (*id).to_string(),
            label: (*label).to_string(),
        })
        .collect()
}
