//! Agent Task + scoped relay broker (C §4.4 / Phase C).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::errors::{format_agent_error, parse_agent_api_error, AgentApiError};
use super::types::ActiveTaskRelay;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateSessionResponse {
    pub id: String,
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateTaskRequest {
    pub session_id: String,
    pub device_id: String,
    #[serde(rename = "type")]
    pub task_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub organization_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub parent_task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soft_cap: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hard_cap: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateTaskResponse {
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub root_task_id: String,
    #[serde(default)]
    pub organization_id: String,
    #[serde(default)]
    pub soft_cap: i64,
    #[serde(default)]
    pub hard_cap: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_version: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_version_expires_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay: Option<TaskRelayView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskRelayView {
    #[serde(default)]
    pub base_url: Option<String>,
    pub credential: String,
    pub expires_at: i64,
}

fn agent_http() -> &'static reqwest::Client {
    crate::abu_api::shared_agent_http()
}

async fn read_json_response(
    response: reqwest::Response,
) -> Result<(reqwest::StatusCode, Value), AgentApiError> {
    let status = response.status();
    let body_text = response.text().await.map_err(|e| AgentApiError {
        code: "NETWORK".into(),
        message: format!("ABU API 返回内容无效：{e}"),
        http_status: status.as_u16(),
        retryable: true,
        action: None,
        context: None,
    })?;
    let body: Value = serde_json::from_str(&body_text).unwrap_or_else(|_| {
        Value::Object(serde_json::Map::from_iter([(
            "message".into(),
            Value::String(body_text.chars().take(200).collect()),
        )]))
    });
    Ok((status, body))
}

fn ensure_success(status: reqwest::StatusCode, body: &Value) -> Result<(), AgentApiError> {
    if body
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Ok(());
    }
    Err(parse_agent_api_error(status, body))
}

/// Create a server-side agent session bound to the registered device (C §4.4).
pub async fn create_agent_session(
    base_url: &str,
    session_token: &str,
    device_id: &str,
) -> Result<CreateSessionResponse, AgentApiError> {
    let device_id = device_id.trim();
    if device_id.is_empty() {
        return Err(AgentApiError {
            code: "INVALID_DEVICE".into(),
            message: "缺少设备 ID，无法创建 Agent Session".into(),
            http_status: 0,
            retryable: false,
            action: None,
            context: None,
        });
    }
    let url = format!(
        "{}/api/agent/sessions?device_id={}",
        base_url.trim_end_matches('/'),
        urlencoding_encode(device_id)
    );
    let response = agent_http()
        .post(url)
        .header("X-Abu-Session-Token", session_token)
        .send()
        .await
        .map_err(|e| AgentApiError {
            code: "NETWORK".into(),
            message: format!("无法连接 ABU API：{e}"),
            http_status: 0,
            retryable: true,
            action: None,
            context: None,
        })?;
    let (status, body) = read_json_response(response).await?;
    ensure_success(status, &body)?;
    let session: CreateSessionResponse =
        serde_json::from_value(body.get("data").cloned().unwrap_or_default()).map_err(|e| {
            AgentApiError {
                code: "PARSE".into(),
                message: format!("Agent Session 响应格式错误：{e}"),
                http_status: status.as_u16(),
                retryable: false,
                action: None,
                context: None,
            }
        })?;
    if session.id.trim().is_empty() {
        return Err(AgentApiError {
            code: "PARSE".into(),
            message: "Agent Session 响应缺少 id".into(),
            http_status: status.as_u16(),
            retryable: false,
            action: None,
            context: None,
        });
    }
    Ok(session)
}

pub async fn create_agent_task(
    base_url: &str,
    session_token: &str,
    request: &CreateTaskRequest,
) -> Result<CreateTaskResponse, AgentApiError> {
    let url = format!("{}/api/agent/tasks", base_url.trim_end_matches('/'));
    let response = agent_http()
        .post(url)
        .header("X-Abu-Session-Token", session_token)
        .json(request)
        .send()
        .await
        .map_err(|e| AgentApiError {
            code: "NETWORK".into(),
            message: format!("无法连接 ABU API：{e}"),
            http_status: 0,
            retryable: true,
            action: None,
            context: None,
        })?;
    let (status, body) = read_json_response(response).await?;
    ensure_success(status, &body)?;
    serde_json::from_value(body.get("data").cloned().unwrap_or_default()).map_err(|e| {
        AgentApiError {
            code: "PARSE".into(),
            message: format!("Agent Task 响应格式错误：{e}"),
            http_status: status.as_u16(),
            retryable: false,
            action: None,
            context: None,
        }
    })
}

/// Server returns this message (no stable code) when session_id is missing/mismatched.
pub fn is_invalid_agent_session_error(err: &AgentApiError) -> bool {
    err.http_status == 400
        && err
            .message
            .to_ascii_lowercase()
            .contains("invalid agent session")
}

pub async fn refresh_task_relay(
    base_url: &str,
    session_token: &str,
    task_id: &str,
    organization_id: &str,
) -> Result<TaskRelayView, AgentApiError> {
    let mut url = format!(
        "{}/api/agent/tasks/{}/relay-credentials",
        base_url.trim_end_matches('/'),
        task_id
    );
    if !organization_id.trim().is_empty() {
        url.push_str(&format!(
            "?organization_id={}",
            urlencoding_encode(organization_id)
        ));
    }
    let response = agent_http()
        .post(url)
        .header("X-Abu-Session-Token", session_token)
        .send()
        .await
        .map_err(|e| AgentApiError {
            code: "NETWORK".into(),
            message: format!("无法连接 ABU API：{e}"),
            http_status: 0,
            retryable: true,
            action: None,
            context: None,
        })?;
    let (status, body) = read_json_response(response).await?;
    ensure_success(status, &body)?;
    serde_json::from_value(body.get("data").cloned().unwrap_or_default()).map_err(|e| {
        AgentApiError {
            code: "PARSE".into(),
            message: format!("Relay 凭证响应格式错误：{e}"),
            http_status: status.as_u16(),
            retryable: false,
            action: None,
            context: None,
        }
    })
}

pub async fn finalize_agent_task(
    base_url: &str,
    session_token: &str,
    task_id: &str,
    organization_id: &str,
    status: &str,
    reason: Option<&str>,
) -> Result<(), AgentApiError> {
    let mut url = format!(
        "{}/api/agent/tasks/{}/finalize",
        base_url.trim_end_matches('/'),
        task_id
    );
    if !organization_id.trim().is_empty() {
        url.push_str(&format!(
            "?organization_id={}",
            urlencoding_encode(organization_id)
        ));
    }
    let mut payload = serde_json::json!({ "status": status });
    if let Some(reason) = reason.filter(|r| !r.trim().is_empty()) {
        payload["reason"] = Value::String(reason.to_string());
    }
    let response = agent_http()
        .post(url)
        .header("X-Abu-Session-Token", session_token)
        .json(&payload)
        .send()
        .await
        .map_err(|e| AgentApiError {
            code: "NETWORK".into(),
            message: format!("无法连接 ABU API：{e}"),
            http_status: 0,
            retryable: true,
            action: None,
            context: None,
        })?;
    let (http_status, body) = read_json_response(response).await?;
    ensure_success(http_status, &body)
}

pub async fn heartbeat_agent_task(
    base_url: &str,
    session_token: &str,
    task_id: &str,
    organization_id: &str,
) -> Result<(), AgentApiError> {
    let mut url = format!(
        "{}/api/agent/tasks/{}/heartbeat",
        base_url.trim_end_matches('/'),
        task_id
    );
    if !organization_id.trim().is_empty() {
        url.push_str(&format!(
            "?organization_id={}",
            urlencoding_encode(organization_id)
        ));
    }
    let response = agent_http()
        .post(url)
        .header("X-Abu-Session-Token", session_token)
        .send()
        .await
        .map_err(|e| AgentApiError {
            code: "NETWORK".into(),
            message: format!("无法连接 ABU API：{e}"),
            http_status: 0,
            retryable: true,
            action: None,
            context: None,
        })?;
    let (http_status, body) = read_json_response(response).await?;
    ensure_success(http_status, &body)
}

/// Minimal percent-encoding for organization ids (alphanumeric + _-).
pub fn urlencoding_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn active_relay_from_create(
    created: &CreateTaskResponse,
    fallback_org: &str,
) -> Option<ActiveTaskRelay> {
    let relay = created.relay.as_ref()?;
    if relay.credential.trim().is_empty() {
        return None;
    }
    Some(ActiveTaskRelay {
        task_id: created.task_id.clone(),
        root_task_id: if created.root_task_id.is_empty() {
            created.task_id.clone()
        } else {
            created.root_task_id.clone()
        },
        organization_id: if created.organization_id.is_empty() {
            fallback_org.to_string()
        } else {
            created.organization_id.clone()
        },
        credential: relay.credential.clone(),
        expires_at: relay.expires_at,
        base_url: relay.base_url.clone(),
        price_version: created.price_version,
        price_version_expires_at: created.price_version_expires_at,
    })
}

pub fn relay_near_expiry(relay: &ActiveTaskRelay, now_unix: i64, skew_secs: i64) -> bool {
    relay.expires_at > 0 && relay.expires_at <= now_unix + skew_secs
}

pub fn map_task_error(err: AgentApiError) -> String {
    format_agent_error(&err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud_context::AgentApiError;

    #[test]
    fn detects_invalid_agent_session_message() {
        let err = AgentApiError {
            code: "UNKNOWN".into(),
            message: "invalid agent session".into(),
            http_status: 400,
            retryable: false,
            action: None,
            context: None,
        };
        assert!(is_invalid_agent_session_error(&err));
        let other = AgentApiError {
            code: "UNKNOWN".into(),
            message: "invalid agent device".into(),
            http_status: 403,
            retryable: false,
            action: None,
            context: None,
        };
        assert!(!is_invalid_agent_session_error(&other));
    }
}
