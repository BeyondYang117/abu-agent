/// 判断 provider 是否支持 `thinking` 字段。
/// 目前只有 DeepSeek 官方 API 和 Kimi 支持该字段；
/// 第三方代理（OpenRouter / 反代）做严格校验时会以 400 拒绝整个请求。
pub fn provider_supports_thinking_field(base_url: &str) -> bool {
    let lower = base_url.to_ascii_lowercase();
    lower.contains("deepseek.com") || lower.contains("moonshot.cn")
}

/**
 * 解析目标语言
 * 当设置为 "auto" 时，根据文本内容自动判断：
 * - 如果文本包含中文，则目标语言为英文
 * - 否则目标语言为中文
 */
pub fn resolve_target_lang(target: &str, text: &str) -> String {
    if target == "auto" {
        if has_chinese(text) {
            "en".to_string()
        } else {
            "zh".to_string()
        }
    } else {
        target.to_string()
    }
}

/**
 * 判断文本中是否包含中文字符
 */
pub fn has_chinese(text: &str) -> bool {
    text.chars().any(|c| ('\u{4e00}'..'\u{9fff}').contains(&c))
}

/**
 * 获取语言代码对应的显示名称
 */
pub fn language_name(code: &str) -> &'static str {
    match code {
        "zh" | "zh-Hans" => "Simplified Chinese",
        "zh-Hant" => "Traditional Chinese",
        "en" => "English",
        "ja" => "Japanese",
        "ko" => "Korean",
        "fr" => "French",
        "de" => "German",
        _ => "English",
    }
}
