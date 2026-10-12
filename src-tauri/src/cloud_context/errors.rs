//! Stable agent API error parsing. Branch on `code`, never on message text (C §8.4).

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentApiError {
    pub code: String,
    pub message: String,
    pub http_status: u16,
    #[serde(default)]
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<Value>,
}

impl AgentApiError {
    pub fn code_eq(&self, expected: &str) -> bool {
        self.code.eq_ignore_ascii_case(expected)
    }

    pub fn is_client_upgrade_required(&self) -> bool {
        self.code_eq("CLIENT_UPGRADE_REQUIRED") || self.http_status == 426
    }

    pub fn download_url(&self) -> Option<String> {
        self.context
            .as_ref()
            .and_then(|ctx| ctx.get("download_url"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }

    pub fn minimum_version(&self) -> Option<String> {
        self.context
            .as_ref()
            .and_then(|ctx| ctx.get("minimum_version"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }
}

impl std::fmt::Display for AgentApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.message.trim().is_empty() {
            write!(f, "{}", self.code)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for AgentApiError {}

/// Parse abu-api error bodies:
/// - nested `{ success, message, error: { code, action, context } }` (426 / relay)
/// - flat `{ success, code, message }` (organization errors)
pub fn parse_agent_api_error(status: reqwest::StatusCode, body: &Value) -> AgentApiError {
    let message = body
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("ABU API 请求失败")
        .to_string();

    if let Some(error) = body.get("error").filter(|v| v.is_object()) {
        let code = error
            .get("code")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| body.get("code").and_then(|v| v.as_str()).unwrap_or("UNKNOWN"))
            .to_string();
        return AgentApiError {
            code,
            message,
            http_status: status.as_u16(),
            retryable: error
                .get("retryable")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            action: error
                .get("action")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            context: error.get("context").cloned(),
        };
    }

    let code = body
        .get("code")
        .and_then(|v| v.as_str())
        .unwrap_or(if status.as_u16() == 426 {
            "CLIENT_UPGRADE_REQUIRED"
        } else {
            "UNKNOWN"
        })
        .to_string();

    AgentApiError {
        code,
        message,
        http_status: status.as_u16(),
        retryable: false,
        action: None,
        context: body.get("context").cloned(),
    }
}

/// Format a typed error as a string that frontend can still show, while embedding
/// a machine-readable prefix for action routing when needed.
pub fn format_agent_error(err: &AgentApiError) -> String {
    if err.code.is_empty() || err.code == "UNKNOWN" {
        return err.message.clone();
    }
    format!("[{}] {}", err.code, err.message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_nested_426() {
        let body = json!({
            "success": false,
            "message": "请升级",
            "error": {
                "code": "CLIENT_UPGRADE_REQUIRED",
                "retryable": false,
                "action": "open_update",
                "context": { "minimum_version": "0.1.12", "download_url": "https://example.com" }
            }
        });
        let err = parse_agent_api_error(reqwest::StatusCode::from_u16(426).unwrap(), &body);
        assert!(err.is_client_upgrade_required());
        assert_eq!(err.action.as_deref(), Some("open_update"));
        assert_eq!(err.download_url().as_deref(), Some("https://example.com"));
    }

    #[test]
    fn parses_flat_org_error() {
        let body = json!({
            "success": false,
            "code": "ORG_PRIVACY_CONSENT_REQUIRED",
            "message": "请先同意组织隐私说明"
        });
        let err = parse_agent_api_error(reqwest::StatusCode::FORBIDDEN, &body);
        assert!(err.code_eq("ORG_PRIVACY_CONSENT_REQUIRED"));
    }
}
