// ABU API 相关的 Tauri 命令

use crate::state::AppState;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::{command, AppHandle, State};

const MODEL_ROUTING_CACHE_FILE: &str = "model-routing-policy-cache.json";
// The API gateway rejects requests with a missing/browser-like signature.
// Keep native desktop requests identifiable as the reqwest client used by the app.
const AGENT_API_USER_AGENT: &str = "reqwest/0.12";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelRoutingPolicyCache {
    pub version: i64,
    pub fetched_at: i64,
    pub payload: serde_json::Value,
}

fn model_routing_cache_path() -> Result<std::path::PathBuf, String> {
    crate::app_data::app_data_dir()
        .map(|dir| dir.join(MODEL_ROUTING_CACHE_FILE))
        .ok_or_else(|| "无法确定应用数据目录".to_string())
}

fn read_model_routing_cache() -> Option<ModelRoutingPolicyCache> {
    let raw = std::fs::read_to_string(model_routing_cache_path().ok()?).ok()?;
    let cache: ModelRoutingPolicyCache = serde_json::from_str(&raw).ok()?;
    (cache.version > 0 && cache.payload.get("rules").is_some()).then_some(cache)
}

#[derive(Debug, Clone, Deserialize, Default)]
struct CachedRoutingCapabilities {
    #[serde(default)]
    vision: bool,
    #[serde(default)]
    embedding: bool,
    #[serde(default)]
    image_generation: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct CachedRoutingRule {
    model: String,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    healthy: bool,
    #[serde(default)]
    tiers: Vec<String>,
    #[serde(default)]
    task_scores: HashMap<String, i64>,
    #[serde(default)]
    capabilities: CachedRoutingCapabilities,
    #[serde(default)]
    priority: i64,
    #[serde(default)]
    fallback_priority: i64,
    #[serde(default = "default_rollout_percent")]
    rollout_percent: u8,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct CachedTaskMapping {
    task: String,
    #[serde(default)]
    keywords: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct CachedRoutingPayload {
    #[serde(default)]
    rules: Vec<CachedRoutingRule>,
    #[serde(default)]
    fallbacks: HashMap<String, Vec<String>>,
    #[serde(default)]
    task_mappings: Vec<CachedTaskMapping>,
}

fn default_rollout_percent() -> u8 {
    100
}

#[derive(Debug, Clone)]
pub struct CachedModelRoute {
    pub provider_id: String,
    pub model: String,
    pub version: i64,
    pub fallbacks: Vec<(String, String)>,
}

fn rollout_eligible(key: &str, model_id: &str, percent: u8) -> bool {
    if percent >= 100 {
        return true;
    }
    if percent == 0 {
        return false;
    }
    let mut hash = 0x811c9dc5_u32;
    for byte in format!("{key}\0{model_id}").as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    hash % 100 < u32::from(percent)
}

fn cached_subtask_kind(prompt: &str, mappings: &[CachedTaskMapping]) -> &'static str {
    let lower = prompt.to_lowercase();
    for mapping in mappings {
        if mapping.keywords.iter().any(|keyword| {
            let keyword = keyword.trim().to_lowercase();
            !keyword.is_empty() && lower.contains(&keyword)
        }) {
            return match mapping.task.as_str() {
                "coding" => "coding",
                "creative" => "creative",
                "reasoning" => "reasoning",
                "vision" => "vision",
                _ => "general",
            };
        }
    }
    if ["代码", "编程", "调试", "bug", "code", "typescript", "rust", "python"]
        .iter()
        .any(|keyword| lower.contains(keyword))
    {
        "coding"
    } else if ["小说", "故事", "文案", "写作", "润色", "story", "creative"]
        .iter()
        .any(|keyword| lower.contains(keyword))
    {
        "creative"
    } else if ["分析", "推理", "证明", "研究", "reason", "prove", "research"]
        .iter()
        .any(|keyword| lower.contains(keyword))
    {
        "reasoning"
    } else {
        "general"
    }
}

pub fn select_cached_model_for_subtask(
    settings: &crate::settings::Settings,
    prompt: &str,
    parent_provider_id: &str,
    parent_model: &str,
    rollout_key: &str,
) -> Option<CachedModelRoute> {
    let cache = read_model_routing_cache()?;
    let payload: CachedRoutingPayload = serde_json::from_value(cache.payload.clone()).ok()?;
    let task = cached_subtask_kind(prompt, &payload.task_mappings);
    let tier = "balanced";
    let mut candidates: Vec<(String, String, i64, i64)> = Vec::new();
    for rule in &payload.rules {
        if rule.model.trim().is_empty()
            || !rule.enabled
            || !rule.healthy
            || !rule.tiers.iter().any(|item| item == tier)
            || rule.capabilities.embedding
            || rule.capabilities.image_generation
            || (task == "vision" && !rule.capabilities.vision)
        {
            continue;
        }
        if settings.is_cloud_runtime() {
            let route_id = format!("{}:{}", rule.provider, rule.model);
            if rollout_eligible(rollout_key, &route_id, rule.rollout_percent) {
                let score = rule.task_scores.get(task).copied().unwrap_or_default()
                    + rule.priority
                    + i64::from(parent_model == rule.model) * 3;
                candidates.push((
                    crate::chat::model::ABU_API_PROVIDER_ID.to_string(),
                    rule.model.clone(),
                    score,
                    rule.fallback_priority,
                ));
            }
            continue;
        }
        for provider in settings.providers.iter().filter(|provider| {
            provider.enabled && (rule.provider.is_empty() || rule.provider == provider.id)
        }) {
            let configured = if provider.enabled_models.is_empty() {
                &provider.available_models
            } else {
                &provider.enabled_models
            };
            if !configured.iter().any(|item| item == &rule.model) {
                continue;
            }
            let route_id = format!("{}:{}", provider.id, rule.model);
            if !rollout_eligible(rollout_key, &route_id, rule.rollout_percent) {
                continue;
            }
            let score = rule.task_scores.get(task).copied().unwrap_or_default()
                + rule.priority
                + i64::from(provider.id == parent_provider_id && rule.model == parent_model) * 3;
            candidates.push((
                provider.id.clone(),
                rule.model.clone(),
                score,
                rule.fallback_priority,
            ));
        }
    }
    candidates.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| b.3.cmp(&a.3)));
    let selected = candidates.first()?.clone();
    let configured_fallbacks = payload.fallbacks.get(tier).cloned().unwrap_or_default();
    candidates.sort_by(|a, b| {
        let a_index = configured_fallbacks.iter().position(|model| model == &a.1);
        let b_index = configured_fallbacks.iter().position(|model| model == &b.1);
        a_index
            .unwrap_or(usize::MAX)
            .cmp(&b_index.unwrap_or(usize::MAX))
            .then_with(|| b.3.cmp(&a.3))
    });
    Some(CachedModelRoute {
        provider_id: selected.0.clone(),
        model: selected.1.clone(),
        version: cache.version,
        fallbacks: candidates
            .into_iter()
            .filter(|candidate| candidate.0 != selected.0 || candidate.1 != selected.1)
            .map(|candidate| (candidate.0, candidate.1))
            .collect(),
    })
}

fn write_model_routing_cache(cache: &ModelRoutingPolicyCache) -> Result<(), String> {
    let path = model_routing_cache_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建策略缓存目录失败：{e}"))?;
    }
    let raw = serde_json::to_string(cache).map_err(|e| format!("序列化策略缓存失败：{e}"))?;
    crate::chat::storage::atomic_write(&path, &raw, "model routing policy cache")
}

#[command]
pub fn abu_api_get_cached_model_routing_policy() -> Option<ModelRoutingPolicyCache> {
    read_model_routing_cache()
}

#[command]
pub async fn abu_api_sync_model_routing_policy(
    state: State<'_, AppState>,
) -> Result<ModelRoutingPolicyCache, String> {
    let cached = read_model_routing_cache();
    let (base_url, session_token) = {
        let settings = state.settings_read();
        let base_url = settings
            .abu_api_base_url
            .clone()
            .unwrap_or_else(|| crate::settings::DEFAULT_ABU_API_BASE_URL.to_string());
        (base_url, settings.abu_api_session_token.clone())
    };
    let session_token = match session_token {
        Some(token) if !token.trim().is_empty() => token,
        _ => return cached.ok_or_else(|| "尚未登录且没有可用的路由策略缓存".to_string()),
    };
    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/agent/model-routing-policy",
            base_url.trim_end_matches('/'),
        ))
        .header("X-Abu-Session-Token", session_token)
        .header(reqwest::header::USER_AGENT, AGENT_API_USER_AGENT)
        .timeout(std::time::Duration::from_secs(12))
        .send()
        .await
        .map_err(|e| format!("路由策略同步失败：{e}"));
    let response = match response {
        Ok(response) => response,
        Err(error) => return cached.ok_or(error),
    };
    let data: serde_json::Value = match parse_agent_data(response, "同步路由策略").await {
        Ok(data) => data,
        Err(error) => return cached.ok_or(error),
    };
    if data.get("changed").and_then(|value| value.as_bool()) == Some(false) {
        let mut cache = cached.ok_or_else(|| "服务端返回未变化，但本地没有策略缓存".to_string())?;
        cache.fetched_at = chrono::Utc::now().timestamp();
        write_model_routing_cache(&cache)?;
        return Ok(cache);
    }
    let version = data.get("version").and_then(|value| value.as_i64()).unwrap_or_default();
    if version <= 0 || !data.get("rules").is_some_and(|value| value.is_array()) {
        return cached.ok_or_else(|| "服务端路由策略不完整".to_string());
    }
    let cache = ModelRoutingPolicyCache {
        version,
        fetched_at: chrono::Utc::now().timestamp(),
        payload: data,
    };
    write_model_routing_cache(&cache)?;
    Ok(cache)
}

#[cfg(test)]
mod model_routing_tests {
    use super::{cached_subtask_kind, rollout_eligible, CachedTaskMapping};

    #[test]
    fn rollout_hash_is_stable_for_utf8_identifiers() {
        let first = rollout_eligible("设备-a", "渠道:模型", 37);
        for _ in 0..10 {
            assert_eq!(first, rollout_eligible("设备-a", "渠道:模型", 37));
        }
        assert!(rollout_eligible("设备-a", "渠道:模型", 100));
        assert!(!rollout_eligible("设备-a", "渠道:模型", 0));
    }

    #[test]
    fn server_task_mapping_precedes_local_keywords() {
        let mappings = vec![CachedTaskMapping {
            task: "creative".to_string(),
            keywords: vec!["产品代码".to_string()],
        }];
        assert_eq!(cached_subtask_kind("请写产品代码介绍", &mappings), "creative");
        assert_eq!(cached_subtask_kind("修复 Rust bug", &[]), "coding");
    }
}

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
    #[serde(default)]
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
pub struct AgentCliCredentialsResponse {
    pub agent: String,
    pub api_key: String,
    pub group: String,
    pub groups: Vec<String>,
    pub models: Vec<String>,
    pub recommended_model: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentRelayCredentialsResponse {
    pub api_key: String,
    pub groups: Vec<String>,
    pub models: Vec<String>,
    pub recommended_model: String,
}

#[derive(Debug, Clone)]
pub struct PlatformSearchCredentials {
    pub base_url: String,
    pub relay_key: String,
}

#[derive(Debug, Deserialize)]
struct PlatformSearchEnvelope {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    data: Vec<crate::web_search::WebSearchResult>,
}

pub async fn search_platform_web(
    client: &reqwest::Client,
    credentials: &PlatformSearchCredentials,
    query: &str,
    max_results: u8,
) -> Result<Vec<crate::web_search::WebSearchResult>, String> {
    let response = client
        .post(format!(
            "{}/v1/web-search",
            credentials.base_url.trim_end_matches('/')
        ))
        .bearer_auth(&credentials.relay_key)
        .timeout(std::time::Duration::from_secs(15))
        .json(&serde_json::json!({
            "query": query,
            "max_results": max_results.clamp(1, 12),
        }))
        .send()
        .await
        .map_err(|err| format!("平台搜索请求失败：{err}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| format!("平台搜索响应读取失败（HTTP {status}）：{err}"))?;
    let envelope: PlatformSearchEnvelope = serde_json::from_str(&body)
        .map_err(|_| format!("平台搜索响应无法解析（HTTP {status}）"))?;
    if !status.is_success() || !envelope.success {
        return Err(envelope
            .message
            .filter(|message| !message.trim().is_empty())
            .unwrap_or_else(|| format!("平台搜索失败（HTTP {status}）")));
    }
    Ok(envelope.data)
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
        // A network outage must return control to the onboarding UI instead of
        // leaving the button in its loading state indefinitely (especially on Windows).
        .timeout(std::time::Duration::from_secs(15))
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
        .timeout(std::time::Duration::from_secs(15))
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
        .timeout(std::time::Duration::from_secs(15))
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

/// 获取用于本机 Claude/Codex CLI 的用户级 relay token。该请求只使用桌面
/// session token，返回的 token 不与某个 Agent Task 绑定，可直接用于标准
/// `/v1/messages` 与 `/v1/responses` 端点。
#[command]
pub async fn abu_api_get_cli_credentials(
    base_url: String,
    session_token: String,
    agent: String,
) -> Result<AgentCliCredentialsResponse, String> {
    let url = format!(
        "{}/api/agent/cli-credentials",
        base_url.trim_end_matches('/')
    );
    let response = reqwest::Client::new()
        .post(url)
        .header("X-Abu-Session-Token", session_token)
        .json(&serde_json::json!({ "agent": agent }))
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
            .unwrap_or("获取 CLI 凭证失败")
            .to_string());
    }
    serde_json::from_value(body.get("data").cloned().unwrap_or_default())
        .map_err(|e| format!("ABU API CLI 凭证响应格式错误：{e}"))
}

pub async fn fetch_agent_relay_credentials(
    base_url: &str,
    session_token: &str,
    model: &str,
) -> Result<AgentRelayCredentialsResponse, String> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/agent/relay-credentials", base_url.trim_end_matches('/')))
        .header("X-Abu-Session-Token", session_token)
        .send()
        .await
        .map_err(|e| format!("无法连接 ABU API：{e}"))?;
    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|e| format!("ABU API 返回内容无效（HTTP {status}）：{e}"))?;
    if matches!(status, reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::METHOD_NOT_ALLOWED) {
        return fetch_agent_cli_credentials(&client, base_url, session_token, model).await;
    }
    let body: serde_json::Value = serde_json::from_str(&body_text)
        .map_err(|e| format!("ABU API 返回内容无效（HTTP {status}）：{e}"))?;
    // During a rolling deployment the session endpoint can be live on one
    // instance while the new relay-credentials route is still absent on
    // another. The legacy CLI credential endpoint returns the same ordinary
    // user token and is safe to use as a compatibility fallback.
    if !body.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
        return Err(body
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("获取云端凭证失败")
            .to_string());
    }
    serde_json::from_value(body.get("data").cloned().unwrap_or_default())
        .map_err(|e| format!("ABU API 云端凭证响应格式错误：{e}"))
}

async fn fetch_agent_cli_credentials(
    client: &reqwest::Client,
    base_url: &str,
    session_token: &str,
    model: &str,
) -> Result<AgentRelayCredentialsResponse, String> {
    let agent = if model.trim().to_ascii_lowercase().contains("claude")
        || model.trim().to_ascii_lowercase().contains("anthropic")
    {
        "claude"
    } else {
        "codex"
    };
    let response = client
        .post(format!("{}/api/agent/cli-credentials", base_url.trim_end_matches('/')))
        .header("X-Abu-Session-Token", session_token)
        .json(&serde_json::json!({ "agent": agent }))
        .send()
        .await
        .map_err(|e| format!("无法连接 ABU API：{e}"))?;
    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("ABU API 返回内容无效（HTTP {status}）：{e}"))?;
    if !body.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
        return Err(body
            .get("message")
            .and_then(|v| v.as_str())
            .map(|message| format!("获取云端凭证失败（兼容接口）：{message}"))
            .unwrap_or_else(|| format!("获取云端凭证失败（兼容接口，HTTP {status}）")));
    }
    let data = body.get("data").cloned().unwrap_or_default();
    let credentials: AgentCliCredentialsResponse = serde_json::from_value(data)
        .map_err(|e| format!("ABU API CLI 凭证响应格式错误：{e}"))?;
    Ok(AgentRelayCredentialsResponse {
        api_key: credentials.api_key,
        groups: credentials.groups,
        models: credentials.models,
        recommended_model: credentials.recommended_model,
    })
}

#[command]
pub async fn abu_api_get_relay_credentials(
    base_url: String,
    session_token: String,
    model: String,
) -> Result<AgentRelayCredentialsResponse, String> {
    fetch_agent_relay_credentials(&base_url, &session_token, &model).await
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
    #[serde(default)]
    pub model_access: Vec<AgentModelAccessResponse>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AgentModelAccessResponse {
    pub model: String,
    pub status: String,
    #[serde(default)]
    pub recommended_plan_ids: Vec<i64>,
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

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    message: Option<String>,
    data: Option<T>,
}

fn agent_api_credentials(state: &AppState) -> Result<(String, String), String> {
    let settings = state.settings_read();
    let base_url = settings
        .abu_api_base_url_or_default()
        .trim_end_matches('/')
        .to_string();
    let session_token = settings
        .abu_api_session_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "尚未登录 ABU 账户".to_string())?
        .to_string();
    Ok((base_url, session_token))
}

fn agent_path_id(value: &str, what: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 128 || value.contains(['/', '?', '#']) {
        return Err(format!("{what}无效"));
    }
    Ok(value.to_string())
}

async fn parse_agent_data<T: DeserializeOwned + Default>(
    response: reqwest::Response,
    what: &str,
) -> Result<T, String> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("{what}响应读取失败（HTTP {status}）：{e}"))?;
    let envelope: ApiEnvelope<T> = serde_json::from_str(&body).map_err(|e| {
        let preview: String = body.chars().take(200).collect();
        format!("{what}响应格式错误（HTTP {status}）：{e}；{preview}")
    })?;
    if !status.is_success() || !envelope.success {
        return Err(format!(
            "{what}失败（HTTP {}）：{}",
            status.as_u16(),
            envelope
                .message
                .filter(|message| !message.trim().is_empty())
                .unwrap_or_else(|| "服务器返回失败".to_string())
        ));
    }
    Ok(envelope.data.unwrap_or_default())
}

async fn parse_agent_success(response: reqwest::Response, what: &str) -> Result<(), String> {
    let _: serde_json::Value = parse_agent_data(response, what).await?;
    Ok(())
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
        .header(reqwest::header::USER_AGENT, AGENT_API_USER_AGENT)
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
        .get(format!(
            "{}/api/agent/entitlements",
            base_url.trim_end_matches('/')
        ))
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
                .unwrap_or("获取用户权益失败")
        ));
    }
    serde_json::from_value(body.get("data").cloned().unwrap_or_default())
        .map_err(|e| format!("ABU API 用户权益格式错误：{e}"))
}

/// 通过 Rust 网络层列出当前账户的设备，避免桌面 WebView 的 CORS 限制。
#[command]
pub async fn abu_api_list_devices(
    state: State<'_, AppState>,
) -> Result<Vec<AgentDeviceResponse>, String> {
    let (base_url, session_token) = agent_api_credentials(state.inner())?;
    let response = reqwest::Client::new()
        .get(format!("{base_url}/api/agent/devices"))
        .header("X-Abu-Session-Token", session_token)
        .send()
        .await
        .map_err(|e| format!("无法连接 ABU API：{e}"))?;
    parse_agent_data(response, "获取设备列表").await
}

/// 通过 Rust 网络层注销指定设备。
#[command]
pub async fn abu_api_revoke_device(
    state: State<'_, AppState>,
    device_id: String,
) -> Result<(), String> {
    let device_id = agent_path_id(&device_id, "设备 ID")?;
    let (base_url, session_token) = agent_api_credentials(state.inner())?;
    let response = reqwest::Client::new()
        .delete(format!("{base_url}/api/agent/devices/{device_id}"))
        .header("X-Abu-Session-Token", session_token)
        .send()
        .await
        .map_err(|e| format!("无法连接 ABU API：{e}"))?;
    parse_agent_success(response, "注销设备").await
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

    #[tokio::test]
    async fn platform_search_uses_standard_relay_token() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            loop {
                let mut chunk = [0_u8; 2048];
                let read = stream.read(&mut chunk).unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n")
                    && request
                        .windows(15)
                        .any(|window| window == b"\"max_results\":4")
                {
                    break;
                }
            }
            let request = String::from_utf8_lossy(&request);
            assert!(request.starts_with("POST /v1/web-search HTTP/1.1"));
            assert!(request
                .to_ascii_lowercase()
                .contains("authorization: bearer relay-secret"));
            assert!(!request.to_ascii_lowercase().contains("x-abu-agent-task-id"));
            let body = r#"{"success":true,"data":[{"title":"Release","url":"https://example.com/release","content":"Current","publishedDate":null,"score":null}]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });

        let credentials = PlatformSearchCredentials {
            base_url,
            relay_key: "relay-secret".to_string(),
        };
        let results =
            search_platform_web(&reqwest::Client::new(), &credentials, "current release", 4)
                .await
                .unwrap();
        server.join().unwrap();
        assert_eq!(results[0].title, "Release");
    }

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
