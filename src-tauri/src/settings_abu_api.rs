/**
 * Settings 扩展：支持 ABU API 配置
 */

use serde::{Deserialize, Serialize};

/// 在现有 Settings 结构体中添加 ABU API 相关字段
/// 这些字段应该被添加到 src-tauri/src/settings.rs 的 Settings 结构体中

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    // ... 现有字段 ...

    /// ABU API 配置
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abu_api_base_url: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abu_api_session_token: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abu_api_device_id: Option<String>,

    /// 运行模式：cloud（使用 ABU API）或 local（本地配置）
    #[serde(default = "default_runtime_mode")]
    pub runtime_mode: RuntimeMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeMode {
    /// 云端模式：使用 ABU API 账户
    Cloud,
    /// 本地模式：使用本地配置的 Provider
    Local,
}

fn default_runtime_mode() -> RuntimeMode {
    RuntimeMode::Local
}

impl Default for RuntimeMode {
    fn default() -> Self {
        RuntimeMode::Local
    }
}

/// ABU API 相关的辅助函数

/// 检查是否配置了 ABU API
pub fn has_abu_api_config(settings: &Settings) -> bool {
    settings.abu_api_session_token.is_some() && settings.abu_api_device_id.is_some()
}

/// 获取有效的 ABU API base URL
pub fn get_abu_api_base_url(settings: &Settings) -> String {
    settings
        .abu_api_base_url
        .clone()
        .unwrap_or_else(|| "https://api.abuai.com".to_string())
}

/// 清除 ABU API 会话信息（保留 device_id）
pub fn clear_abu_api_session(settings: &mut Settings) {
    settings.abu_api_session_token = None;
    // device_id 保留，方便下次登录复用
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_mode_serialization() {
        let mode = RuntimeMode::Cloud;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, r#""cloud""#);

        let mode = RuntimeMode::Local;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, r#""local""#);
    }

    #[test]
    fn test_runtime_mode_deserialization() {
        let mode: RuntimeMode = serde_json::from_str(r#""cloud""#).unwrap();
        assert_eq!(mode, RuntimeMode::Cloud);

        let mode: RuntimeMode = serde_json::from_str(r#""local""#).unwrap();
        assert_eq!(mode, RuntimeMode::Local);
    }

    #[test]
    fn test_has_abu_api_config() {
        let mut settings = Settings::default();
        assert!(!has_abu_api_config(&settings));

        settings.abu_api_session_token = Some("token".to_string());
        assert!(!has_abu_api_config(&settings));

        settings.abu_api_device_id = Some("device".to_string());
        assert!(has_abu_api_config(&settings));
    }

    #[test]
    fn test_get_abu_api_base_url() {
        let mut settings = Settings::default();
        assert_eq!(get_abu_api_base_url(&settings), "https://api.abuai.com");

        settings.abu_api_base_url = Some("https://custom.api.com".to_string());
        assert_eq!(get_abu_api_base_url(&settings), "https://custom.api.com");
    }

    #[test]
    fn test_clear_abu_api_session() {
        let mut settings = Settings::default();
        settings.abu_api_session_token = Some("token".to_string());
        settings.abu_api_device_id = Some("device".to_string());

        clear_abu_api_session(&mut settings);

        assert!(settings.abu_api_session_token.is_none());
        assert!(settings.abu_api_device_id.is_some()); // device_id 保留
    }
}
