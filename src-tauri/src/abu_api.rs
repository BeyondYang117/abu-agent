// ABU API 相关的 Tauri 命令

use crate::state::AppState;
use serde::{Deserialize, Serialize};
use tauri::{command, AppHandle, State};

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceAuthResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_at: i64,
    pub interval: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceAuthExchangeResponse {
    pub status: String,
    pub session_token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentDeviceResponse {
    pub id: String,
    pub platform: String,
    pub client_version: String,
    pub device_name: String,
    pub capabilities: String,
    pub status: String,
    pub last_seen_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub revoked_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AbuApiConfig {
    pub base_url: String,
    pub session_token: Option<String>,
    pub device_id: Option<String>,
    pub runtime_mode: String, // "cloud" | "local"
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceRegistration {
    pub fingerprint: String,
    pub platform: String,
    pub client_version: String,
    pub device_name: String,
    pub capabilities: Option<String>,
}

/// 通过 Rust 网络层创建设备授权请求，避免 WebView 跨域/CORS 导致的 "Load failed"。
#[command]
pub async fn abu_api_create_device_authorization(
    base_url: String,
    device_name: String,
) -> Result<DeviceAuthResponse, String> {
    let url = format!("{}/api/agent/auth/device", base_url.trim_end_matches('/'));
    let response = reqwest::Client::new()
        .post(url)
        .json(&serde_json::json!({ "device_name": device_name }))
        .send()
        .await
        .map_err(|e| format!("无法连接 ABU API：{e}"))?;
    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("ABU API 返回内容无效（HTTP {status}）：{e}"))?;
    if !body
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Err(body
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("ABU API 授权请求失败")
            .to_string());
    }
    serde_json::from_value(body.get("data").cloned().unwrap_or_default())
        .map_err(|e| format!("ABU API 授权响应格式错误：{e}"))
}

/// 通过 Rust 网络层轮询设备授权状态。
#[command]
pub async fn abu_api_exchange_device_authorization(
    base_url: String,
    device_code: String,
) -> Result<DeviceAuthExchangeResponse, String> {
    let url = format!(
        "{}/api/agent/auth/device/exchange",
        base_url.trim_end_matches('/')
    );
    let response = reqwest::Client::new()
        .post(url)
        .json(&serde_json::json!({ "device_code": device_code }))
        .send()
        .await
        .map_err(|e| format!("无法连接 ABU API：{e}"))?;
    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("ABU API 返回内容无效（HTTP {status}）：{e}"))?;
    if !body
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        // 服务端用 409/410 表示授权已拒绝/过期，但仍在 data.status 中返回终态，
        // 这不是网络故障，交给前端展示对应提示并停止轮询。
        if let Some(data) = body.get("data") {
            if let Ok(terminal) = serde_json::from_value::<DeviceAuthExchangeResponse>(data.clone())
            {
                if matches!(terminal.status.as_str(), "denied" | "expired") {
                    return Ok(terminal);
                }
            }
        }
        return Err(body
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("ABU API 授权轮询失败")
            .to_string());
    }
    serde_json::from_value(body.get("data").cloned().unwrap_or_default())
        .map_err(|e| format!("ABU API 授权响应格式错误：{e}"))
}

/// 通过 Rust 网络层注册登录设备，避免授权完成后的设备请求再次触发 WebView CORS 错误。
#[command]
pub async fn abu_api_register_device(
    base_url: String,
    session_token: String,
    fingerprint: String,
    platform: String,
    client_version: String,
    device_name: String,
    capabilities: Option<String>,
) -> Result<AgentDeviceResponse, String> {
    let url = format!("{}/api/agent/devices", base_url.trim_end_matches('/'));
    let response = reqwest::Client::new()
        .post(url)
        .header("X-Abu-Session-Token", session_token)
        .json(&serde_json::json!({
            "fingerprint": fingerprint,
            "platform": platform,
            "client_version": client_version,
            "device_name": device_name,
            "capabilities": capabilities.unwrap_or_default(),
        }))
        .send()
        .await
        .map_err(|e| format!("无法连接 ABU API：{e}"))?;
    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("ABU API 返回内容无效（HTTP {status}）：{e}"))?;
    if !body
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Err(body
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("ABU API 设备注册失败")
            .to_string());
    }
    serde_json::from_value(body.get("data").cloned().unwrap_or_default())
        .map_err(|e| format!("ABU API 设备响应格式错误：{e}"))
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct UserInfoResponse {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub quota: i64,
    #[serde(default)]
    pub used_quota: i64,
    #[serde(default)]
    pub group: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AgentModelsResponse {
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub recommended: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AgentEntitlementResponse {
    pub id: i64,
    pub plan_id: i64,
    pub plan_name: String,
    pub group_id: i64,
    #[serde(default)]
    pub supported_models: Vec<String>,
    #[serde(default)]
    pub bound_groups: Vec<String>,
    pub daily_limit_usd: Option<f64>,
    pub weekly_limit_usd: Option<f64>,
    pub monthly_limit_usd: Option<f64>,
    pub daily_usage_usd: f64,
    pub weekly_usage_usd: f64,
    pub monthly_usage_usd: f64,
    pub extra_quota_remaining_usd: f64,
    pub start_date: String,
    pub end_date: String,
}

fn agent_user_info_url(base_url: &str) -> String {
    format!(
        "{}/api/agent/devices?include_account=1",
        base_url.trim_end_matches('/')
    )
}

/// 通过 Rust 网络层获取当前登录用户信息，避免 WebView CORS 导致的请求失败。
#[command]
pub async fn abu_api_get_user_info(state: State<'_, AppState>) -> Result<UserInfoResponse, String> {
    let (base_url, session_token) = {
        let settings = state.settings_read();
        let base_url = settings
            .abu_api_base_url
            .clone()
            .unwrap_or_else(|| crate::settings::DEFAULT_ABU_API_BASE_URL.to_string());
        let session_token = settings
            .abu_api_session_token
            .clone()
            .ok_or_else(|| "尚未登录 ABU 账户".to_string())?;
        (base_url, session_token)
    };

    let url = agent_user_info_url(&base_url);
    let response = reqwest::Client::new()
        .get(url)
        .header("X-Abu-Session-Token", &session_token)
        .send()
        .await
        .map_err(|e| format!("无法连接 ABU API：{e}"))?;
    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("ABU API 返回内容无效（HTTP {status}）：{e}"))?;
    if !body
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Err(format!(
            "HTTP {}: {}",
            status.as_u16(),
            body.get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("获取用户信息失败")
        ));
    }
    serde_json::from_value(body.get("data").cloned().unwrap_or_default())
        .map_err(|e| format!("ABU API 用户信息格式错误：{e}"))
}

fn agent_models_url(base_url: &str) -> String {
    format!("{}/api/agent/models", base_url.trim_end_matches('/'))
}

/// 通过 Rust 网络层获取可用模型，避免 Tauri WebView 的跨域请求失败。
#[command]
pub async fn abu_api_list_models(
    state: State<'_, AppState>,
) -> Result<AgentModelsResponse, String> {
    let (base_url, session_token) = {
        let settings = state.settings_read();
        let base_url = settings
            .abu_api_base_url
            .clone()
            .unwrap_or_else(|| crate::settings::DEFAULT_ABU_API_BASE_URL.to_string());
        let session_token = settings
            .abu_api_session_token
            .clone()
            .ok_or_else(|| "尚未登录 ABU 账户".to_string())?;
        (base_url, session_token)
    };

    let response = reqwest::Client::new()
        .get(agent_models_url(&base_url))
        .header("X-Abu-Session-Token", &session_token)
        .send()
        .await
        .map_err(|e| format!("无法连接 ABU API：{e}"))?;
    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("ABU API 返回内容无效（HTTP {status}）：{e}"))?;
    if !body
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Err(format!(
            "HTTP {}: {}",
            status.as_u16(),
            body.get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("获取模型列表失败")
        ));
    }
    serde_json::from_value(body.get("data").cloned().unwrap_or_default())
        .map_err(|e| format!("ABU API 模型列表格式错误：{e}"))
}

/// 通过 Rust 网络层获取有效套餐权益，避免 WebView 跨域请求失败。
#[command]
pub async fn abu_api_list_entitlements(
    state: State<'_, AppState>,
) -> Result<Vec<AgentEntitlementResponse>, String> {
    let (base_url, session_token) = {
        let settings = state.settings_read();
        let base_url = settings
            .abu_api_base_url
            .clone()
            .unwrap_or_else(|| crate::settings::DEFAULT_ABU_API_BASE_URL.to_string());
        let session_token = settings
            .abu_api_session_token
            .clone()
            .ok_or_else(|| "尚未登录 ABU 账户".to_string())?;
        (base_url, session_token)
    };
    let response = reqwest::Client::new()
        .get(format!("{}/api/agent/entitlements", base_url.trim_end_matches('/')))
        .header("X-Abu-Session-Token", &session_token)
        .send()
        .await
        .map_err(|e| format!("无法连接 ABU API：{e}"))?;
    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("ABU API 返回内容无效（HTTP {status}）：{e}"))?;
    if !body.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
        return Err(format!(
            "HTTP {}: {}",
            status.as_u16(),
            body.get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("获取用户权益失败")
        ));
    }
    serde_json::from_value(body.get("data").cloned().unwrap_or_default())
        .map_err(|e| format!("ABU API 用户权益格式错误：{e}"))
}

/// 获取设备指纹（稳定唯一标识符）。
///
/// 薄封装：真正的计算在 `compute_device_fingerprint()` 里，
/// 那是个不依赖 `AppHandle` 的纯函数，便于单测直接调用。
#[command]
pub fn get_device_fingerprint(_app: AppHandle) -> Result<String, String> {
    Ok(compute_device_fingerprint())
}

/// 组合机器 ID / 主机名 / 用户名算出 16 位十六进制指纹。
/// 同一台机器上多次调用必须稳定——设备注册靠它去重。
pub fn compute_device_fingerprint() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // 组合多个硬件/系统特征生成指纹
    let mut hasher = DefaultHasher::new();

    // 1. 机器 ID（优先）
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("ioreg")
            .args(["-rd1", "-c", "IOPlatformExpertDevice"])
            .output()
        {
            if let Ok(text) = String::from_utf8(output.stdout) {
                if let Some(uuid_line) = text.lines().find(|l| l.contains("IOPlatformUUID")) {
                    uuid_line.hash(&mut hasher);
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = std::process::Command::new("wmic")
            .args(["csproduct", "get", "UUID"])
            .output()
        {
            if let Ok(text) = String::from_utf8(output.stdout) {
                text.hash(&mut hasher);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(machine_id) = std::fs::read_to_string("/etc/machine-id") {
            machine_id.hash(&mut hasher);
        } else if let Ok(dbus_id) = std::fs::read_to_string("/var/lib/dbus/machine-id") {
            dbus_id.hash(&mut hasher);
        }
    }

    // 2. 主机名作为补充
    #[cfg(unix)]
    {
        use std::ffi::CStr;
        let mut buf = [0u8; 256];
        unsafe {
            if libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) == 0 {
                if let Ok(cstr) = CStr::from_bytes_until_nul(&buf) {
                    cstr.to_bytes().hash(&mut hasher);
                }
            }
        }
    }
    #[cfg(windows)]
    {
        if let Ok(name) = std::env::var("COMPUTERNAME") {
            name.hash(&mut hasher);
        }
    }

    // 3. 用户名作为补充
    if let Ok(username) = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .or_else(|_| std::env::var("LOGNAME"))
    {
        username.hash(&mut hasher);
    }

    let hash = hasher.finish();
    format!("{:016x}", hash)
}

/// 获取主机名
#[command]
pub fn get_hostname() -> Result<String, String> {
    #[cfg(unix)]
    {
        use std::ffi::CStr;
        let mut buf = [0u8; 256];
        unsafe {
            if libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) == 0 {
                if let Ok(cstr) = CStr::from_bytes_until_nul(&buf) {
                    return Ok(cstr.to_string_lossy().into_owned());
                }
            }
        }
        Err("Failed to get hostname".to_string())
    }
    #[cfg(windows)]
    {
        std::env::var("COMPUTERNAME").map_err(|_| "Failed to get hostname".to_string())
    }
}

/// 获取平台标识
#[command]
pub fn get_platform() -> String {
    std::env::consts::OS.to_string()
}

/// 获取客户端版本
#[command]
pub fn get_client_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

/// 生成默认设备名称
#[command]
pub fn get_default_device_name() -> Result<String, String> {
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "Unknown".to_string());

    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
        .unwrap_or_else(|| "User".to_string());

    Ok(format!("{} - {}", hostname, username))
}

/// 保存 ABU API 配置到 settings
#[command]
pub async fn save_abu_api_config(
    app: AppHandle,
    state: State<'_, AppState>,
    config: AbuApiConfig,
) -> Result<(), String> {
    let snapshot = {
        let mut guard = state.settings_write();
        guard.abu_api_base_url = Some(config.base_url);
        guard.abu_api_session_token = config.session_token;
        guard.abu_api_device_id = config.device_id;
        guard.runtime_mode = config.runtime_mode;
        guard.clone()
    };

    crate::settings::persist_settings(&app, &snapshot)?;

    Ok(())
}

/// 加载 ABU API 配置
#[command]
pub async fn load_abu_api_config(
    _app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AbuApiConfig, String> {
    let settings = state.settings_read();

    Ok(AbuApiConfig {
        base_url: settings
            .abu_api_base_url
            .clone()
            .unwrap_or_else(|| crate::settings::DEFAULT_ABU_API_BASE_URL.to_string()),
        session_token: settings.abu_api_session_token.clone(),
        device_id: settings.abu_api_device_id.clone(),
        runtime_mode: settings.runtime_mode.clone(),
    })
}

/// 清除 ABU API 会话（登出）
#[command]
pub async fn clear_abu_api_session(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let snapshot = {
        let mut guard = state.settings_write();
        guard.abu_api_session_token = None;
        guard.clone()
    };

    crate::settings::persist_settings(&app, &snapshot)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_info_request_uses_agent_endpoint() {
        assert_eq!(
            agent_user_info_url("https://api.example.com/"),
            "https://api.example.com/api/agent/devices?include_account=1"
        );
    }

    #[test]
    fn models_request_uses_agent_endpoint() {
        assert_eq!(
            agent_models_url("https://api.example.com/"),
            "https://api.example.com/api/agent/models"
        );
    }

    #[test]
    fn test_device_fingerprint_stable() {
        // 同一台机器多次调用必须返回相同指纹（设备注册靠它去重）。
        let fp1 = compute_device_fingerprint();
        let fp2 = compute_device_fingerprint();

        assert_eq!(fp1, fp2, "Device fingerprint should be stable");
        assert_eq!(fp1.len(), 16, "Fingerprint should be 16 hex chars");
        assert!(
            fp1.chars().all(|c| c.is_ascii_hexdigit()),
            "Fingerprint should be lowercase hex, got: {fp1}"
        );
    }

    #[test]
    fn test_platform() {
        let platform = get_platform();
        assert!(
            ["macos", "windows", "linux"].contains(&platform.as_str()),
            "Platform should be one of: macos, windows, linux"
        );
    }
}
