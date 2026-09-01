// ABU API Task 生命周期管理
//
// 负责在 Cloud 模式下创建和管理 Agent Session/Task

use crate::settings::Settings;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct AbuApiTaskContext {
    pub base_url: String,
    pub session_token: String,
    pub device_id: String,
    pub session_id: String,
    pub task_id: String,
    pub relay_session_id: String,
    pub relay_key: String,
}

#[derive(Debug, Serialize)]
struct CreateSessionRequest {
    device_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct CreateSessionResponse {
    success: bool,
    message: Option<String>,
    data: Option<SessionData>,
}

#[derive(Debug, Deserialize)]
struct SessionData {
    session_id: String,
}

#[derive(Debug, Serialize)]
struct CreateTaskRequest {
    session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    soft_cap: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hard_cap: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct CreateTaskResponse {
    success: bool,
    message: Option<String>,
    data: Option<TaskData>,
}

#[derive(Debug, Deserialize)]
struct TaskData {
    task_id: String,
}

#[derive(Debug, Serialize)]
struct CreateRelaySessionRequest {
    task_id: String,
}

#[derive(Debug, Deserialize)]
struct CreateRelaySessionResponse {
    success: bool,
    message: Option<String>,
    data: Option<RelaySessionData>,
}

#[derive(Debug, Deserialize)]
struct RelaySessionData {
    relay_session_id: String,
    relay_key: String,
    expires_at: String,
}

/// 准备对话：创建 Session + Task + Relay Session
pub async fn prepare_conversation(
    settings: &Settings,
    conversation_id: &str,
) -> Result<AbuApiTaskContext, String> {
    let base_url = settings
        .abu_api_base_url
        .as_ref()
        .ok_or_else(|| "ABU API base URL not configured".to_string())?;
    let session_token = settings
        .abu_api_session_token
        .as_ref()
        .ok_or_else(|| "Not logged in to ABU API".to_string())?;
    let device_id = settings
        .abu_api_device_id
        .as_ref()
        .ok_or_else(|| "Device ID not found".to_string())?;

    let client = Client::new();

    // 1. 创建 Agent Session
    let session_id = create_session(&client, base_url, session_token, device_id).await?;

    // 2. 创建 Agent Task（设置配额）
    let task_id = create_task(
        &client,
        base_url,
        session_token,
        &session_id,
        Some(500),  // ¥5 软上限
        Some(2000), // ¥20 硬上限
        conversation_id,
    )
    .await?;

    // 3. 创建 Relay Session（获取临时 token）
    let (relay_session_id, relay_key) =
        create_relay_session(&client, base_url, session_token, &task_id).await?;

    Ok(AbuApiTaskContext {
        base_url: base_url.clone(),
        session_token: session_token.clone(),
        device_id: device_id.clone(),
        session_id,
        task_id,
        relay_session_id,
        relay_key,
    })
}

async fn create_session(
    client: &Client,
    base_url: &str,
    session_token: &str,
    device_id: &str,
) -> Result<String, String> {
    let url = format!("{}/api/agent/sessions", base_url);
    let body = CreateSessionRequest {
        device_id: device_id.to_string(),
        metadata: None,
    };

    let response = client
        .post(&url)
        .header("X-Abu-Session-Token", session_token)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to create agent session: {}", e))?;

    let status = response.status();
    let resp: CreateSessionResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse session response: {}", e))?;

    if !resp.success {
        return Err(resp
            .message
            .unwrap_or_else(|| format!("HTTP {}", status)));
    }

    resp.data
        .map(|d| d.session_id)
        .ok_or_else(|| "No session_id in response".to_string())
}

async fn create_task(
    client: &Client,
    base_url: &str,
    session_token: &str,
    session_id: &str,
    soft_cap: Option<i64>,
    hard_cap: Option<i64>,
    conversation_id: &str,
) -> Result<String, String> {
    let url = format!("{}/api/agent/tasks", base_url);
    let mut metadata = serde_json::Map::new();
    metadata.insert("conversation_id".to_string(), conversation_id.into());

    let body = CreateTaskRequest {
        session_id: session_id.to_string(),
        soft_cap,
        hard_cap,
        metadata: Some(metadata.into()),
    };

    let response = client
        .post(&url)
        .header("X-Abu-Session-Token", session_token)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to create task: {}", e))?;

    let status = response.status();
    let resp: CreateTaskResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse task response: {}", e))?;

    if !resp.success {
        return Err(resp
            .message
            .unwrap_or_else(|| format!("HTTP {}", status)));
    }

    resp.data
        .map(|d| d.task_id)
        .ok_or_else(|| "No task_id in response".to_string())
}

async fn create_relay_session(
    client: &Client,
    base_url: &str,
    session_token: &str,
    task_id: &str,
) -> Result<(String, String), String> {
    let url = format!("{}/api/agent/relay/sessions", base_url);
    let body = CreateRelaySessionRequest {
        task_id: task_id.to_string(),
    };

    let response = client
        .post(&url)
        .header("X-Abu-Session-Token", session_token)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to create relay session: {}", e))?;

    let status = response.status();
    let resp: CreateRelaySessionResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse relay session response: {}", e))?;

    if !resp.success {
        return Err(resp
            .message
            .unwrap_or_else(|| format!("HTTP {}", status)));
    }

    resp.data
        .map(|d| (d.relay_session_id, d.relay_key))
        .ok_or_else(|| "No relay session data in response".to_string())
}

/// 更新 Task 状态（对话结束时调用）
pub async fn finalize_task(
    base_url: &str,
    session_token: &str,
    task_id: &str,
    status: &str, // "succeeded" | "failed" | "cancelled"
) -> Result<(), String> {
    let client = Client::new();
    let url = format!("{}/api/agent/tasks/{}/status", base_url, task_id);

    #[derive(Serialize)]
    struct UpdateStatusRequest {
        status: String,
    }

    let body = UpdateStatusRequest {
        status: status.to_string(),
    };

    let response = client
        .patch(&url)
        .header("X-Abu-Session-Token", session_token)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to update task status: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Failed to finalize task: HTTP {}", response.status()));
    }

    Ok(())
}
