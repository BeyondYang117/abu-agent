// ABU API Provider Adapter
//
// 将 ABU API relay 端点包装为虚拟 Provider，复用现有的 OpenAI/Anthropic/Gemini 适配器

use crate::settings::{ModelProvider, ProviderApiFormat};

/// 从 ABU API Task 上下文创建虚拟 Provider
///
/// ABU API 的 /api/agent/relay/* 端点实际上是 OpenAI/Anthropic/Gemini 的代理，
/// 所以我们不需要重新实现流解析，只需要：
/// 1. 构造一个虚拟的 ModelProvider，指向 relay 端点
/// 2. 使用 relay_key 作为 API Key
/// 3. 调用现有的 OpenAI/Anthropic/Gemini Provider
pub fn create_abu_api_virtual_provider(
    base_url: &str,
    relay_key: &str,
    api_format: ProviderApiFormat,
) -> ModelProvider {
    let relay_base_url = format!("{}/api/agent/relay", base_url);

    ModelProvider {
        id: "abu-api-relay".to_string(),
        name: "abu-api-relay".to_string(),
        api_keys: vec![relay_key.to_string()],
        api_key_legacy: None,
        base_url: relay_base_url,
        available_models: Vec::new(),
        enabled_models: Vec::new(),
        enabled: true,
        api_format: api_format.as_str().to_string(),
        model_overrides: Default::default(),
        compress_request_body: false,
        request: Default::default(),
        active_key_index: 0,
    }
}

/// 检查 Task 配额状态
pub async fn check_task_quota(
    base_url: &str,
    session_token: &str,
    task_id: &str,
) -> Result<TaskQuotaStatus, String> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/agent/tasks/{}/usage", base_url, task_id);

    let response = client
        .get(&url)
        .header("X-Abu-Session-Token", session_token)
        .send()
        .await
        .map_err(|e| format!("Failed to check quota: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Failed to check quota: HTTP {}", response.status()));
    }

    #[derive(serde::Deserialize)]
    struct ApiResponse {
        success: bool,
        message: Option<String>,
        data: Option<TaskUsageData>,
    }

    #[derive(serde::Deserialize)]
    struct TaskUsageData {
        soft_cap_exceeded: bool,
        hard_cap_exceeded: bool,
        consumed_quota: i64,
        soft_cap: i64,
        hard_cap: i64,
    }

    let resp: ApiResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse quota response: {}", e))?;

    if !resp.success {
        return Err(resp.message.unwrap_or_else(|| "Unknown error".to_string()));
    }

    let data = resp.data.ok_or_else(|| "No usage data".to_string())?;

    Ok(TaskQuotaStatus {
        soft_cap_exceeded: data.soft_cap_exceeded,
        hard_cap_exceeded: data.hard_cap_exceeded,
        consumed_quota: data.consumed_quota,
        soft_cap: data.soft_cap,
        hard_cap: data.hard_cap,
    })
}

#[derive(Debug, Clone)]
pub struct TaskQuotaStatus {
    pub soft_cap_exceeded: bool,
    pub hard_cap_exceeded: bool,
    pub consumed_quota: i64,
    pub soft_cap: i64,
    pub hard_cap: i64,
}

/// 发送心跳
pub async fn send_task_heartbeat(
    base_url: &str,
    session_token: &str,
    task_id: &str,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/agent/tasks/{}/heartbeat", base_url, task_id);

    let response = client
        .post(&url)
        .header("X-Abu-Session-Token", session_token)
        .send()
        .await
        .map_err(|e| format!("Failed to send heartbeat: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Heartbeat failed: HTTP {}", response.status()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_virtual_provider() {
        let provider = create_abu_api_virtual_provider(
            "https://api.abuai.com",
            "test_relay_key",
            ProviderApiFormat::OpenAiChat,
        );

        assert_eq!(provider.id, "abu-api-relay");
        assert_eq!(
            provider.base_url,
            "https://api.abuai.com/api/agent/relay"
        );
        assert_eq!(provider.api_keys, vec!["test_relay_key".to_string()]);
    }
}
