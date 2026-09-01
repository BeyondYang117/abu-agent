//! ABU API Task 生命周期管理（Cloud 模式）。
//!
//! 一次对话发送 = 一条 Agent Task。流程：
//!   `POST /api/agent/sessions?device_id=..`            → session.id
//!   `POST /api/agent/tasks`      {session_id,device_id,type,caps} → task.id
//!   `POST /api/agent/relay-session?device_id=..&task_id=..`       → relay_key
//! 之后模型请求走 `/api/agent/relay/v1/*`，用 relay_key 作 Bearer，
//! 并**必须**带 `X-Abu-Agent-Task-ID`（`middleware.AgentRelayAuth` 强制校验，
//! 缺失 400、与凭证不匹配 409）。
//!
//! 本文件的每个请求形状都对齐 abu-api 的 Go 源码（`controller/agent.go`、
//! `controller/agent_relay.go`、`router/api-router.go`）。改动前请先核对那边：
//! session / relay-session 用 **query 参数**，不是 JSON body；
//! 状态收尾是 **POST** `{from,to}` 的状态机，不是 PATCH `{status}`。

use serde::{Deserialize, Serialize};

use crate::settings::{Settings, DEFAULT_ABU_API_BASE_URL};

/// Task 的 `type`（服务端 `binding:"required,max=32"`）。
const TASK_TYPE_CHAT: &str = "chat";

/// 单条 Task 的客户端配额上限（单位：quota，与服务端 `consumed_quota` 同量纲）。
///
/// 服务端 `AgentTaskDefaultSoftCap` / `AgentTaskDefaultHardCap` 目前都是 `0`
/// （= 不限），且只有在平台默认 > 0 时才会下调客户端传入的 hard_cap。
/// 也就是说**这两个常量是当前唯一生效的超支防线**，调整前想清楚。
const TASK_SOFT_CAP: i64 = 500;
const TASK_HARD_CAP: i64 = 2000;

/// 一次 Cloud 模式对话所需的全部服务端句柄。
#[derive(Debug, Clone)]
pub struct AbuApiTaskContext {
    pub base_url: String,
    pub session_token: String,
    pub device_id: String,
    pub session_id: String,
    pub task_id: String,
    /// relay 请求的 Bearer 凭证。短期有效，不落盘。
    pub relay_key: String,
    /// relay_key 过期时间（Unix 秒）。
    pub relay_expires_at: i64,
}

/// 服务端统一响应包封。`data` 的形状按端点不同。
#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    data: Option<T>,
}

/// `AgentSession` / `AgentTask` 都以 `id` 返回主键（不是 `session_id` / `task_id`）。
#[derive(Debug, Deserialize)]
struct IdOnly {
    id: String,
}

#[derive(Debug, Serialize)]
struct CreateTaskRequest {
    session_id: String,
    device_id: String,
    #[serde(rename = "type")]
    task_type: String,
    soft_cap: i64,
    hard_cap: i64,
}

#[derive(Debug, Deserialize)]
struct RelaySessionData {
    relay_key: String,
    #[serde(default)]
    expires_at: i64,
}

/// 状态机收尾请求。服务端只接受 `from == "running"` 且 `to != "running"`。
#[derive(Debug, Serialize)]
struct UpdateTaskStatusRequest {
    from: String,
    to: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    reason: String,
}

/// Task 的终态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskOutcome {
    Succeeded,
    Failed,
    Cancelled,
}

impl TaskOutcome {
    fn as_str(self) -> &'static str {
        match self {
            TaskOutcome::Succeeded => "succeeded",
            TaskOutcome::Failed => "failed",
            TaskOutcome::Cancelled => "cancelled",
        }
    }
}

/// 从 settings 取出 Cloud 模式所需的三件套。任一缺失都说明没登录 / 没注册设备。
fn cloud_credentials(settings: &Settings) -> Result<(String, String, String), String> {
    let base_url = settings
        .abu_api_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_ABU_API_BASE_URL)
        .trim_end_matches('/')
        .to_string();
    let session_token = settings
        .abu_api_session_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "尚未登录 ABU 账户，请到设置 > 账户重新登录。".to_string())?
        .to_string();
    let device_id = settings
        .abu_api_device_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "本机尚未注册为 ABU 设备，请到设置 > 账户重新登录。".to_string())?
        .to_string();
    Ok((base_url, session_token, device_id))
}

/// 解析统一响应包封。HTTP 状态码与 `success` 都要看：4xx 时 body 里带的
/// `message` 才是有用信息（如「Agent task 无效、设备不匹配或已结束」）。
async fn parse_envelope<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
    what: &str,
) -> Result<T, String> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| format!("{what}：读取响应失败（HTTP {status}）：{err}"))?;
    let envelope: ApiEnvelope<T> = serde_json::from_str(&body).map_err(|_| {
        let preview: String = body.chars().take(200).collect();
        format!("{what}：响应无法解析（HTTP {status}）：{preview}")
    })?;
    if !envelope.success {
        let message = envelope
            .message
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| format!("HTTP {status}"));
        return Err(format!("{what}：{message}"));
    }
    envelope
        .data
        .ok_or_else(|| format!("{what}：响应缺少 data 字段（HTTP {status}）"))
}

/// 准备一次 Cloud 模式对话：Session → Task → Relay Session。
///
/// 三步都在服务端建了状态，所以这里不做重试：任何一步失败都直接把错误抛给调用方，
/// 由用户重发触发新的一轮（残留的 running task 由服务端僵尸任务回收处理）。
pub async fn prepare_conversation(settings: &Settings) -> Result<AbuApiTaskContext, String> {
    let (base_url, session_token, device_id) = cloud_credentials(settings)?;
    let client = reqwest::Client::new();

    let session_id = create_session(&client, &base_url, &session_token, &device_id).await?;
    let task_id = create_task(
        &client,
        &base_url,
        &session_token,
        &session_id,
        &device_id,
    )
    .await?;
    let (relay_key, relay_expires_at) =
        create_relay_session(&client, &base_url, &session_token, &device_id, &task_id).await?;

    Ok(AbuApiTaskContext {
        base_url,
        session_token,
        device_id,
        session_id,
        task_id,
        relay_key,
        relay_expires_at,
    })
}

/// `POST /api/agent/sessions?device_id=..` —— device_id 是 **query 参数**。
async fn create_session(
    client: &reqwest::Client,
    base_url: &str,
    session_token: &str,
    device_id: &str,
) -> Result<String, String> {
    let response = client
        .post(format!("{base_url}/api/agent/sessions"))
        .query(&[("device_id", device_id)])
        .header("X-Abu-Session-Token", session_token)
        .send()
        .await
        .map_err(|err| format!("创建 Agent Session 失败：{err}"))?;
    let data: IdOnly = parse_envelope(response, "创建 Agent Session 失败").await?;
    Ok(data.id)
}

/// `POST /api/agent/tasks` —— JSON body，`device_id` 与 `type` 都是必填。
async fn create_task(
    client: &reqwest::Client,
    base_url: &str,
    session_token: &str,
    session_id: &str,
    device_id: &str,
) -> Result<String, String> {
    let body = CreateTaskRequest {
        session_id: session_id.to_string(),
        device_id: device_id.to_string(),
        task_type: TASK_TYPE_CHAT.to_string(),
        soft_cap: TASK_SOFT_CAP,
        hard_cap: TASK_HARD_CAP,
    };
    let response = client
        .post(format!("{base_url}/api/agent/tasks"))
        .header("X-Abu-Session-Token", session_token)
        .json(&body)
        .send()
        .await
        .map_err(|err| format!("创建 Agent Task 失败：{err}"))?;
    let data: IdOnly = parse_envelope(response, "创建 Agent Task 失败").await?;
    Ok(data.id)
}

/// `POST /api/agent/relay-session?device_id=..&task_id=..` —— 注意路径是
/// `relay-session`（单数、连字符），且两个参数都在 query 上。
async fn create_relay_session(
    client: &reqwest::Client,
    base_url: &str,
    session_token: &str,
    device_id: &str,
    task_id: &str,
) -> Result<(String, i64), String> {
    let response = client
        .post(format!("{base_url}/api/agent/relay-session"))
        .query(&[("device_id", device_id), ("task_id", task_id)])
        .header("X-Abu-Session-Token", session_token)
        .send()
        .await
        .map_err(|err| format!("创建中转会话失败：{err}"))?;
    let data: RelaySessionData = parse_envelope(response, "创建中转会话失败").await?;
    Ok((data.relay_key, data.expires_at))
}

/// 收尾 Task：`POST /api/agent/tasks/{id}/status`，body 是 `{from,to,reason}`。
///
/// 服务端 `AdvanceAgentTaskStatus` 是 CAS：`from` 不匹配时返回 409，
/// 说明这条 task 已经被别的路径收尾（如取消、僵尸回收），不是错误。
pub async fn finalize_task(
    context: &AbuApiTaskContext,
    outcome: TaskOutcome,
    reason: &str,
) -> Result<(), String> {
    let body = UpdateTaskStatusRequest {
        from: "running".to_string(),
        to: outcome.as_str().to_string(),
        // 服务端 `fail_reason` 是 varchar(64)，超长会被数据库截断/报错，这里先裁。
        reason: reason.chars().take(64).collect(),
    };
    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/agent/tasks/{}/status",
            context.base_url, context.task_id
        ))
        .header("X-Abu-Session-Token", &context.session_token)
        .json(&body)
        .send()
        .await
        .map_err(|err| format!("更新 Task 状态失败：{err}"))?;
    if response.status() == reqwest::StatusCode::CONFLICT {
        // 已被收尾，幂等返回。
        return Ok(());
    }
    let _: serde_json::Value = parse_envelope(response, "更新 Task 状态失败")
        .await
        .unwrap_or(serde_json::Value::Null);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with(
        base_url: Option<&str>,
        token: Option<&str>,
        device: Option<&str>,
    ) -> Settings {
        let mut settings = Settings::default();
        settings.abu_api_base_url = base_url.map(str::to_string);
        settings.abu_api_session_token = token.map(str::to_string);
        settings.abu_api_device_id = device.map(str::to_string);
        settings
    }

    #[test]
    fn credentials_fall_back_to_default_base_url() {
        let settings = settings_with(None, Some("tok"), Some("dev"));
        let (base_url, token, device) = cloud_credentials(&settings).expect("should resolve");
        assert_eq!(base_url, DEFAULT_ABU_API_BASE_URL);
        assert_eq!(token, "tok");
        assert_eq!(device, "dev");
    }

    #[test]
    fn credentials_strip_trailing_slash() {
        let settings = settings_with(Some("https://api.example.com/"), Some("tok"), Some("dev"));
        let (base_url, _, _) = cloud_credentials(&settings).expect("should resolve");
        assert_eq!(base_url, "https://api.example.com");
    }

    #[test]
    fn credentials_reject_missing_token_and_device() {
        let no_token = settings_with(None, None, Some("dev"));
        assert!(cloud_credentials(&no_token).is_err());

        // 空串等同缺失（settings.json 可被手改）。
        let blank_device = settings_with(None, Some("tok"), Some("   "));
        assert!(cloud_credentials(&blank_device).is_err());
    }

    #[test]
    fn task_outcome_strings_match_server_constants() {
        assert_eq!(TaskOutcome::Succeeded.as_str(), "succeeded");
        assert_eq!(TaskOutcome::Failed.as_str(), "failed");
        assert_eq!(TaskOutcome::Cancelled.as_str(), "cancelled");
    }

    #[test]
    fn soft_cap_stays_below_hard_cap() {
        // 服务端在 soft > hard 时会把 soft 压到 hard，等于软上限失效。
        assert!(TASK_SOFT_CAP < TASK_HARD_CAP);
    }
}
