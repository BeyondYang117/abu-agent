//! Cloud 模式的「虚拟供应商」：把 abu-api 的 relay 端点包装成一个普通
//! `ModelProvider`，复用现成的 OpenAI Chat 适配器，不新写一套流解析。
//!
//! ## 为什么统一用 openai_chat
//!
//! abu-api 的 relay 按**模型名**选渠道（`middleware.Distribute()`），请求格式与上游
//! 渠道类型解耦：Claude / Gemini 渠道各自实现了 `ConvertOpenAIRequest`
//! （`RequestOpenAI2ClaudeMessage` / `CovertOpenAI2Gemini`），网关会把 OpenAI 格式的
//! 请求体翻成上游原生协议。所以云端模型不管上游是谁，一律走
//! `/api/agent/relay/v1/chat/completions` 即可。
//!
//! 这条选择顺带绕开了一个死结：relay 鉴权只认 `Authorization: Bearer` 或
//! `x-api-key`（`middleware.AgentRelayAuth`），而本地 Gemini 适配器发的是
//! `x-goog-api-key`——它在 `provider_request::RESERVED_HEADER_KEYS` 里，
//! 用户级自定义头覆盖不了。走 openai_chat 就不会有 Gemini 适配器参与。

use crate::settings::{ModelProvider, ProviderApiFormat, ProviderCustomHeader};

/// 虚拟供应商的 id / name。会进入 usage 记录与请求调试面板，保持可辨识。
pub const ABU_API_PROVIDER_ID: &str = "abu-api-relay";

/// relay 鉴权强制要求的任务头。缺失 → 400，与凭证里的 task 不一致 → 409。
/// 走 `provider.request.custom_headers` 注入：该 key 不在保留名单里，
/// 且 `provider_request::apply` / `header_pairs` 是所有适配器的统一装配入口。
const TASK_ID_HEADER: &str = "X-Abu-Agent-Task-ID";

/// 构造指向 relay 的虚拟供应商。
///
/// `base_url` 传 abu-api 根地址（如 `https://api.abuai.chat`），这里补上
/// `/api/agent/relay/v1`——OpenAI 适配器会拼成 `{base}/chat/completions`，
/// 正好命中 `agentRelay.POST("/v1/chat/completions")`。
pub fn create_abu_api_virtual_provider(
    base_url: &str,
    relay_key: &str,
    task_id: &str,
) -> ModelProvider {
    let base = base_url.trim_end_matches('/');
    ModelProvider {
        id: ABU_API_PROVIDER_ID.to_string(),
        name: "ABU 云端".to_string(),
        api_keys: vec![relay_key.to_string()],
        api_key_legacy: None,
        base_url: format!("{base}/api/agent/relay/v1"),
        available_models: Vec::new(),
        enabled_models: Vec::new(),
        enabled: true,
        api_format: ProviderApiFormat::OpenAiChat.as_str().to_string(),
        model_overrides: Default::default(),
        // relay 前面挂着 WAF/限流中间件，压缩体反而多一层不确定性，保持明文。
        compress_request_body: false,
        request: crate::settings::ProviderRequestConfig {
            custom_headers: vec![ProviderCustomHeader {
                key: TASK_ID_HEADER.to_string(),
                value: task_id.to_string(),
            }],
            ..Default::default()
        },
        active_key_index: 0,
    }
}

/// Task 配额快照。
#[derive(Debug, Clone)]
pub struct TaskQuotaStatus {
    pub soft_cap_exceeded: bool,
    pub hard_cap_exceeded: bool,
    pub consumed_quota: i64,
    pub soft_cap: i64,
    pub hard_cap: i64,
}

#[derive(Debug, serde::Deserialize)]
struct UsageEnvelope {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    data: Option<UsageData>,
}

#[derive(Debug, serde::Deserialize)]
struct UsageData {
    #[serde(default)]
    soft_cap_exceeded: bool,
    #[serde(default)]
    hard_cap_exceeded: bool,
    #[serde(default)]
    consumed_quota: i64,
    #[serde(default)]
    soft_cap: i64,
    #[serde(default)]
    hard_cap: i64,
}

/// `GET /api/agent/tasks/{id}/usage`。用 session token 鉴权（不是 relay_key）。
pub async fn check_task_quota(
    base_url: &str,
    session_token: &str,
    task_id: &str,
) -> Result<TaskQuotaStatus, String> {
    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/agent/tasks/{}/usage",
            base_url.trim_end_matches('/'),
            task_id
        ))
        .header("X-Abu-Session-Token", session_token)
        .send()
        .await
        .map_err(|err| format!("查询配额失败：{err}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| format!("查询配额失败：读取响应出错（HTTP {status}）：{err}"))?;
    let envelope: UsageEnvelope = serde_json::from_str(&body)
        .map_err(|_| format!("查询配额失败：响应无法解析（HTTP {status}）"))?;
    if !envelope.success {
        let message = envelope
            .message
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| format!("HTTP {status}"));
        return Err(format!("查询配额失败：{message}"));
    }
    let data = envelope
        .data
        .ok_or_else(|| "查询配额失败：响应缺少 data 字段".to_string())?;
    Ok(TaskQuotaStatus {
        soft_cap_exceeded: data.soft_cap_exceeded,
        hard_cap_exceeded: data.hard_cap_exceeded,
        consumed_quota: data.consumed_quota,
        soft_cap: data.soft_cap,
        hard_cap: data.hard_cap,
    })
}

/// `POST /api/agent/tasks/{id}/heartbeat`。只刷 `updated_at`，防僵尸回收。
/// 调用方应忽略失败——心跳掉一次不该打断正在进行的对话。
pub async fn send_task_heartbeat(
    base_url: &str,
    session_token: &str,
    task_id: &str,
) -> Result<(), String> {
    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/agent/tasks/{}/heartbeat",
            base_url.trim_end_matches('/'),
            task_id
        ))
        .header("X-Abu-Session-Token", session_token)
        .send()
        .await
        .map_err(|err| format!("心跳失败：{err}"))?;
    if !response.status().is_success() {
        return Err(format!("心跳失败：HTTP {}", response.status()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_lands_on_relay_chat_completions() {
        let provider = create_abu_api_virtual_provider("https://api.abuai.chat", "rk", "task-1");
        // OpenAI 适配器拼的是 `{base_url}/chat/completions`。
        assert_eq!(
            format!("{}/chat/completions", provider.base_url),
            "https://api.abuai.chat/api/agent/relay/v1/chat/completions"
        );
    }

    #[test]
    fn trailing_slash_in_base_url_does_not_double_up() {
        let provider = create_abu_api_virtual_provider("https://api.abuai.chat/", "rk", "task-1");
        assert_eq!(
            provider.base_url,
            "https://api.abuai.chat/api/agent/relay/v1"
        );
    }

    #[test]
    fn relay_key_is_the_api_key() {
        let provider = create_abu_api_virtual_provider("https://x", "relay-key-abc", "task-1");
        assert_eq!(provider.api_keys, vec!["relay-key-abc".to_string()]);
        assert!(!provider.api_keys.is_empty(), "reply.rs 会拒绝空 api_keys");
    }

    #[test]
    fn forces_openai_chat_format() {
        let provider = create_abu_api_virtual_provider("https://x", "rk", "task-1");
        assert_eq!(provider.api_format_kind(), ProviderApiFormat::OpenAiChat);
    }

    /// 任务头必须能真的发出去：不在保留名单里、且键值合法。
    #[test]
    fn task_id_header_survives_sanitization() {
        let provider = create_abu_api_virtual_provider("https://x", "rk", "task-42");
        let header = provider
            .request
            .custom_headers
            .iter()
            .find(|h| h.key.eq_ignore_ascii_case(TASK_ID_HEADER))
            .expect("task header must be present");
        assert_eq!(header.value, "task-42");
        assert!(
            crate::provider_request::is_usable_header(header),
            "task 头被判为非法或保留，relay 会因缺头返回 400"
        );
    }

    #[test]
    fn task_id_header_reaches_wire() {
        let provider = create_abu_api_virtual_provider("https://x", "rk", "task-99");
        let pairs = crate::provider_request::header_pairs(&provider, None);
        assert!(
            pairs
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case(TASK_ID_HEADER)
                    && value == "task-99"),
            "header_pairs 没带上 task 头：{pairs:?}"
        );
    }
}
