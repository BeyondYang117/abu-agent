//! Cloud 模式的「虚拟供应商」：把 abu-api 的普通 relay 端点包装成一个
//! `ModelProvider`。协议由模型名选择，避免把 Responses/Codex 请求误发成 Anthropic。

use crate::settings::{ModelProvider, ProviderApiFormat};

/// 虚拟供应商的 id / name。会进入 usage 记录与请求调试面板，保持可辨识。
pub const ABU_API_PROVIDER_ID: &str = "abu-api-relay";

/// 构造指向 relay 的虚拟供应商。
///
/// `base_url` 传 abu-api 根地址（如 `https://api.abuai.chat`），这里补上
/// `/api/agent/relay/v1`——Anthropic 适配器会拼成 `{base}/messages`，
/// 正好命中 `agentRelay.POST("/v1/messages")`。
pub fn create_abu_api_virtual_provider(
    base_url: &str,
    api_key: &str,
    model: &str,
) -> ModelProvider {
    let base = base_url.trim_end_matches('/');
    ModelProvider {
        id: ABU_API_PROVIDER_ID.to_string(),
        name: "ABU 云端".to_string(),
        api_keys: vec![api_key.to_string()],
        api_key_legacy: None,
        base_url: format!("{base}/v1"),
        available_models: Vec::new(),
        enabled_models: Vec::new(),
        enabled: true,
        api_format: relay_api_format(model).as_str().to_string(),
        model_overrides: Default::default(),
        // relay 前面挂着 WAF/限流中间件，压缩体反而多一层不确定性，保持明文。
        compress_request_body: false,
        request: Default::default(),
        active_key_index: 0,
    }
}

fn relay_api_format(model: &str) -> ProviderApiFormat {
    let id = model.trim().to_ascii_lowercase();
    if id.contains("claude") || id.contains("anthropic") {
        ProviderApiFormat::AnthropicMessages
    } else if id.contains("codex")
        || id.contains("responses")
        || openai_gpt_major_version(&id).is_some_and(|major| major >= 5)
        || id.starts_with("o1")
        || id.starts_with("o3")
        || id.starts_with("o4")
    {
        ProviderApiFormat::OpenAiResponses
    } else {
        ProviderApiFormat::OpenAiChat
    }
}

fn openai_gpt_major_version(model: &str) -> Option<u32> {
    let version = model.strip_prefix("gpt-")?;
    let major = version
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    (!major.is_empty()).then(|| major.parse().ok()).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_lands_on_relay_messages() {
        let provider =
            create_abu_api_virtual_provider("https://api.abuai.chat", "rk", "claude-sonnet");
        // Anthropic 适配器拼的是 `{base_url}/messages`。
        assert_eq!(
            format!("{}/messages", provider.base_url),
            "https://api.abuai.chat/v1/messages"
        );
    }

    #[test]
    fn trailing_slash_in_base_url_does_not_double_up() {
        let provider =
            create_abu_api_virtual_provider("https://api.abuai.chat/", "rk", "claude-sonnet");
        assert_eq!(provider.base_url, "https://api.abuai.chat/v1");
    }

    #[test]
    fn relay_key_is_the_api_key() {
        let provider = create_abu_api_virtual_provider("https://x", "relay-key-abc", "gpt-4o");
        assert_eq!(provider.api_keys, vec!["relay-key-abc".to_string()]);
        assert!(!provider.api_keys.is_empty(), "reply.rs 会拒绝空 api_keys");
    }

    #[test]
    fn selects_protocol_from_model() {
        let provider = create_abu_api_virtual_provider("https://x", "rk", "claude-3");
        assert_eq!(
            provider.api_format_kind(),
            ProviderApiFormat::AnthropicMessages
        );
        assert_eq!(
            create_abu_api_virtual_provider("https://x", "rk", "codex-mini").api_format_kind(),
            ProviderApiFormat::OpenAiResponses
        );
        for model in [
            "gpt-5",
            "gpt-5.4",
            "gpt-5.5",
            "gpt-5.6",
            "gpt-5.6-sol",
            "gpt-5.6-terra",
            "gpt-5.6-luna",
            "gpt-6",
            "gpt-6-pro",
        ] {
            assert_eq!(
                create_abu_api_virtual_provider("https://x", "rk", model).api_format_kind(),
                ProviderApiFormat::OpenAiResponses,
                "GPT-5+ models must use Responses so function-call arguments stream correctly: {model}"
            );
        }
        assert_eq!(
            create_abu_api_virtual_provider("https://x", "rk", "gpt-4o").api_format_kind(),
            ProviderApiFormat::OpenAiChat
        );
    }
}
