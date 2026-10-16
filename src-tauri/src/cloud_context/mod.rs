//! CloudContextManager — Rust authority for org / bootstrap / task credentials (C §3).

mod broker;
mod errors;
mod session;
mod tasks;
mod types;

pub use broker::{
    acquire_cloud_run_credentials, ensure_conversation_billing_binding, finalize_cloud_run,
    CloudRunCredentials,
};

pub use errors::{format_agent_error, parse_agent_api_error, AgentApiError};
pub use session::{
    clear_session_token, load_session_token, migrate_from_settings, resolve_session_token,
    store_session_token,
};
pub use tasks::{
    active_relay_from_create, create_agent_task, finalize_agent_task, heartbeat_agent_task,
    map_task_error, refresh_task_relay, relay_near_expiry, urlencoding_encode, CreateTaskRequest,
    CreateTaskResponse,
};
pub use types::*;

use crate::state::AppState;
use serde_json::Value;
use std::sync::Mutex;
use tauri::{command, AppHandle, State};

const AGENT_API_USER_AGENT: &str = "reqwest/0.12";

#[derive(Debug, Default)]
pub struct CloudContextManager {
    pub legacy_mode: bool,
    pub bootstrap: Option<CloudBootstrap>,
    pub active_organization_id: Option<String>,
    pub caches: OrgScopedCaches,
    pub active_relays: Vec<ActiveTaskRelay>,
    pub client_upgrade_required: bool,
    pub client_upgrade_download_url: Option<String>,
    pub last_error: Option<AgentApiError>,
}

impl CloudContextManager {
    pub fn view(&self, authenticated: bool) -> CloudContextView {
        let context = self
            .active_organization_id
            .as_ref()
            .and_then(|id| self.caches.context.get(id).map(|e| e.value.clone()));
        CloudContextView {
            legacy_mode: self.legacy_mode,
            authenticated,
            active_organization_id: self.active_organization_id.clone(),
            bootstrap: self.bootstrap.clone(),
            context,
            minimum_client_version: self
                .bootstrap
                .as_ref()
                .map(|b| b.minimum_client_version.clone())
                .filter(|v| !v.trim().is_empty()),
            client_upgrade_required: self.client_upgrade_required,
            client_upgrade_download_url: self.client_upgrade_download_url.clone(),
        }
    }

    pub fn organizations_enabled(&self) -> bool {
        !self.legacy_mode
            && self
                .bootstrap
                .as_ref()
                .map(|b| b.features.organizations)
                .unwrap_or(false)
    }

    pub fn scoped_relay_enabled(&self) -> bool {
        self.organizations_enabled()
            && self
                .bootstrap
                .as_ref()
                .map(|b| b.features.scoped_relay_credentials)
                .unwrap_or(false)
    }

    pub fn clear_org_caches(&mut self) {
        self.caches = OrgScopedCaches::default();
    }

    pub fn set_active_organization(&mut self, organization_id: Option<String>) {
        let changed = self.active_organization_id != organization_id;
        self.active_organization_id = organization_id;
        if changed {
            // Switching org must not flash previous org models (C §9).
            self.clear_org_caches();
        }
    }

    pub fn remember_relay(&mut self, relay: ActiveTaskRelay) {
        self.active_relays
            .retain(|r| r.task_id != relay.task_id && r.root_task_id != relay.root_task_id);
        self.active_relays.push(relay);
    }

    pub fn take_relay_for_task(&mut self, task_id: &str) -> Option<ActiveTaskRelay> {
        if let Some(idx) = self.active_relays.iter().position(|r| r.task_id == task_id) {
            return Some(self.active_relays.remove(idx));
        }
        None
    }

    pub fn find_relay_for_root(&self, root_task_id: &str) -> Option<&ActiveTaskRelay> {
        self.active_relays
            .iter()
            .find(|r| r.root_task_id == root_task_id || r.task_id == root_task_id)
    }
}

fn agent_http() -> &'static reqwest::Client {
    crate::abu_api::shared_agent_http()
}

async fn get_json(
    base_url: &str,
    session_token: &str,
    path_and_query: &str,
) -> Result<(reqwest::StatusCode, Value), AgentApiError> {
    let url = format!(
        "{}{}",
        base_url.trim_end_matches('/'),
        path_and_query
    );
    let response = agent_http()
        .get(url)
        .header("X-Abu-Session-Token", session_token)
        .header(reqwest::header::USER_AGENT, AGENT_API_USER_AGENT)
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
    read_json_response(response).await
}

async fn post_json(
    base_url: &str,
    session_token: &str,
    path: &str,
    body: &Value,
) -> Result<(reqwest::StatusCode, Value), AgentApiError> {
    let url = format!("{}{}", base_url.trim_end_matches('/'), path);
    let response = agent_http()
        .post(url)
        .header("X-Abu-Session-Token", session_token)
        .header(reqwest::header::USER_AGENT, AGENT_API_USER_AGENT)
        .json(body)
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
    read_json_response(response).await
}

async fn read_json_response(
    response: reqwest::Response,
) -> Result<(reqwest::StatusCode, Value), AgentApiError> {
    let status = response.status();
    let text = response.text().await.map_err(|e| AgentApiError {
        code: "NETWORK".into(),
        message: format!("ABU API 返回内容无效：{e}"),
        http_status: status.as_u16(),
        retryable: true,
        action: None,
        context: None,
    })?;
    let body: Value = serde_json::from_str(&text).unwrap_or_else(|_| {
        Value::Object(serde_json::Map::from_iter([(
            "message".into(),
            Value::String(text.chars().take(200).collect()),
        )]))
    });
    Ok((status, body))
}

/// Persist organization privacy consent on the server (C §8.4 / org privacy).
async fn consent_organization_privacy(
    base_url: &str,
    session_token: &str,
    organization_id: &str,
    version: i64,
) -> Result<(), AgentApiError> {
    if version < 0 {
        return Err(AgentApiError {
            code: "ORG_PRIVACY_CONSENT_REQUIRED".into(),
            message: "隐私说明版本无效".into(),
            http_status: 400,
            retryable: false,
            action: Some("open_privacy_consent".into()),
            context: None,
        });
    }
    let path = format!(
        "/api/organizations/{}/privacy-consent",
        urlencoding_encode(organization_id)
    );
    let payload = serde_json::json!({
        "version": version,
        "accepted": true,
    });
    let (status, body) = post_json(base_url, session_token, &path, &payload).await?;
    if body
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Ok(());
    }
    Err(parse_agent_api_error(status, &body))
}

fn apply_upgrade_from_error(manager: &mut CloudContextManager, err: &AgentApiError) {
    if err.is_client_upgrade_required() {
        manager.client_upgrade_required = true;
        manager.client_upgrade_download_url = err.download_url();
    }
    manager.last_error = Some(err.clone());
}

/// Fetch bootstrap; 404/405 → legacy_mode (C §11.2).
pub async fn refresh_bootstrap(
    manager: &Mutex<CloudContextManager>,
    base_url: &str,
    session_token: &str,
    preferred_org_id: Option<&str>,
) -> Result<CloudContextView, String> {
    let (status, body) = get_json(base_url, session_token, "/api/agent/bootstrap")
        .await
        .map_err(|e| format_agent_error(&e))?;

    if matches!(
        status,
        reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::METHOD_NOT_ALLOWED
    ) {
        let mut guard = manager.lock().map_err(|e| e.to_string())?;
        guard.legacy_mode = true;
        guard.client_upgrade_required = false;
        guard.bootstrap = Some(CloudBootstrap::legacy_personal(AgentBootstrapUser::default()));
        guard.active_organization_id = Some(LEGACY_PERSONAL_ORG_ID.to_string());
        return Ok(guard.view(true));
    }

    if !body
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let err = parse_agent_api_error(status, &body);
        let mut guard = manager.lock().map_err(|e| e.to_string())?;
        apply_upgrade_from_error(&mut guard, &err);
        return Err(format_agent_error(&err));
    }

    let bootstrap: CloudBootstrap = serde_json::from_value(
        body.get("data").cloned().unwrap_or_default(),
    )
    .map_err(|e| format!("Bootstrap 响应格式错误：{e}"))?;

    let legacy_mode = !bootstrap.features.organizations;
    let default_org = if legacy_mode {
        LEGACY_PERSONAL_ORG_ID.to_string()
    } else if !bootstrap.default_organization_id.trim().is_empty() {
        bootstrap.default_organization_id.clone()
    } else {
        bootstrap
            .organizations
            .iter()
            .find(|o| o.org_type == "personal")
            .map(|o| o.id.clone())
            .or_else(|| bootstrap.organizations.first().map(|o| o.id.clone()))
            .unwrap_or_default()
    };

    let active = preferred_org_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .filter(|id| {
            legacy_mode
                || bootstrap.organizations.iter().any(|o| o.id == *id)
                || *id == LEGACY_PERSONAL_ORG_ID
        })
        .unwrap_or(default_org);

    let mut guard = manager.lock().map_err(|e| e.to_string())?;
    guard.legacy_mode = legacy_mode;
    guard.bootstrap = Some(bootstrap);
    guard.client_upgrade_required = false;
    guard.set_active_organization(if active.is_empty() {
        None
    } else {
        Some(active)
    });
    Ok(guard.view(true))
}

pub async fn refresh_organization_context(
    manager: &Mutex<CloudContextManager>,
    base_url: &str,
    session_token: &str,
    organization_id: &str,
) -> Result<AgentOrganizationContext, String> {
    {
        let guard = manager.lock().map_err(|e| e.to_string())?;
        if guard.legacy_mode || organization_id == LEGACY_PERSONAL_ORG_ID {
            return Ok(AgentOrganizationContext {
                schema_version: 1,
                context_version: 0,
                organization: AgentContextOrganization {
                    id: LEGACY_PERSONAL_ORG_ID.to_string(),
                    name: "个人账户".into(),
                    org_type: "personal".into(),
                    edition: "basic".into(),
                    activated: true,
                    member_limit: 1,
                },
                membership: AgentContextMembership {
                    id: "legacy-member".into(),
                    role: "owner".into(),
                    permissions: vec!["model.use".into(), "usage.view.self".into()],
                },
                funding: AgentContextFunding {
                    mode: "personal".into(),
                    billing_account_id: String::new(),
                    currency: "MODEL_CREDIT".into(),
                },
                balances: None,
                budget: None,
            });
        }
    }

    let path = format!(
        "/api/agent/context?organization_id={}",
        tasks::urlencoding_encode(organization_id)
    );
    let (status, body) = get_json(base_url, session_token, &path)
        .await
        .map_err(|e| format_agent_error(&e))?;
    if !body
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let err = parse_agent_api_error(status, &body);
        let mut guard = manager.lock().map_err(|e| e.to_string())?;
        apply_upgrade_from_error(&mut guard, &err);
        return Err(format_agent_error(&err));
    }
    let context: AgentOrganizationContext =
        serde_json::from_value(body.get("data").cloned().unwrap_or_default())
            .map_err(|e| format!("组织上下文响应格式错误：{e}"))?;
    let now = chrono::Local::now().timestamp();
    let mut guard = manager.lock().map_err(|e| e.to_string())?;
    guard.caches.context.insert(
        organization_id.to_string(),
        OrgScopedCacheEntry {
            organization_id: organization_id.to_string(),
            context_version: context.context_version,
            fetched_at: now,
            value: context.clone(),
        },
    );
    Ok(context)
}

fn cloud_manager(state: &AppState) -> &Mutex<CloudContextManager> {
    &state.cloud_context
}

#[command]
pub async fn cloud_context_get_view(
    state: State<'_, AppState>,
) -> Result<CloudContextView, String> {
    let authenticated = {
        let settings = state.settings_read();
        resolve_session_token(settings.abu_api_session_token.as_deref()).is_some()
            && settings.is_cloud_runtime()
    };
    let guard = cloud_manager(&state).lock().map_err(|e| e.to_string())?;
    Ok(guard.view(authenticated))
}

#[command]
pub async fn cloud_context_refresh(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CloudContextView, String> {
    let (base_url, session_token, preferred_org) = {
        let mut settings = state.settings_write();
        let (token, clear_settings) =
            migrate_from_settings(settings.abu_api_session_token.as_deref())?;
        if clear_settings {
            settings.abu_api_session_token = None;
            let snapshot = settings.clone();
            drop(settings);
            let _ = crate::settings::persist_settings(&app, &snapshot);
            let settings = state.settings_read();
            (
                settings.abu_api_base_url_or_default().to_string(),
                token,
                settings.active_organization_id.clone(),
            )
        } else {
            (
                settings.abu_api_base_url_or_default().to_string(),
                token,
                settings.active_organization_id.clone(),
            )
        }
    };
    let session_token = session_token.ok_or_else(|| "尚未登录 ABU 账户".to_string())?;
    let view = refresh_bootstrap(
        cloud_manager(&state),
        &base_url,
        &session_token,
        preferred_org.as_deref(),
    )
    .await?;

    if let Some(org_id) = view.active_organization_id.clone() {
        let _ = refresh_organization_context(
            cloud_manager(&state),
            &base_url,
            &session_token,
            &org_id,
        )
        .await;
        // Persist active org id (non-sensitive).
        let snapshot = {
            let mut settings = state.settings_write();
            settings.active_organization_id = Some(org_id);
            settings.clone()
        };
        let _ = crate::settings::persist_settings(&app, &snapshot);
    }

    let guard = cloud_manager(&state).lock().map_err(|e| e.to_string())?;
    Ok(guard.view(true))
}

#[command]
pub async fn cloud_context_set_organization(
    app: AppHandle,
    state: State<'_, AppState>,
    organization_id: String,
    privacy_consent_accepted: Option<bool>,
) -> Result<CloudContextView, String> {
    let org_id = organization_id.trim().to_string();
    if org_id.is_empty() {
        return Err("organization_id 不能为空".into());
    }

    let (privacy_version, org_type, privacy_required) = {
        let guard = cloud_manager(&state).lock().map_err(|e| e.to_string())?;
        if !guard.legacy_mode {
            if let Some(boot) = &guard.bootstrap {
                if boot.features.organizations
                    && !boot.organizations.iter().any(|o| o.id == org_id)
                {
                    return Err("组织不存在或不属于当前用户".into());
                }
                let org = boot.organizations.iter().find(|o| o.id == org_id);
                (
                    boot.privacy_notice_version,
                    org.map(|o| o.org_type.clone()).unwrap_or_default(),
                    org.map(|o| o.privacy_consent_required).unwrap_or(false),
                )
            } else {
                (0, String::new(), false)
            }
        } else {
            (0, String::new(), false)
        }
    };

    let (base_url, session_token) = {
        let settings = state.settings_read();
        (
            settings.abu_api_base_url_or_default().to_string(),
            resolve_session_token(settings.abu_api_session_token.as_deref()),
        )
    };

    // 决策理由：非个人组织在前端确认后写服务端 consent；服务端幂等，已同意则 no-op。
    let should_consent = org_type != "personal"
        && org_type != LEGACY_PERSONAL_ORG_ID
        && privacy_version > 0
        && (privacy_consent_accepted.unwrap_or(false) || privacy_required);
    if should_consent {
        if !privacy_consent_accepted.unwrap_or(false) && privacy_required {
            return Err(format_agent_error(&AgentApiError {
                code: "ORG_PRIVACY_CONSENT_REQUIRED".into(),
                message: "请先同意当前版本的组织隐私说明".into(),
                http_status: 403,
                retryable: false,
                action: Some("open_privacy_consent".into()),
                context: None,
            }));
        }
        let token = session_token
            .as_deref()
            .ok_or_else(|| "尚未登录 ABU 账户".to_string())?;
        consent_organization_privacy(&base_url, token, &org_id, privacy_version)
            .await
            .map_err(|e| {
                if let Ok(mut guard) = cloud_manager(&state).lock() {
                    apply_upgrade_from_error(&mut guard, &e);
                    guard.last_error = Some(e.clone());
                }
                format_agent_error(&e)
            })?;
        if let Ok(mut guard) = cloud_manager(&state).lock() {
            if let Some(boot) = guard.bootstrap.as_mut() {
                if let Some(org) = boot.organizations.iter_mut().find(|o| o.id == org_id) {
                    org.privacy_consent_required = false;
                }
            }
        }
    }

    {
        let mut guard = cloud_manager(&state).lock().map_err(|e| e.to_string())?;
        guard.set_active_organization(Some(org_id.clone()));
    }

    let snapshot = {
        let mut settings = state.settings_write();
        settings.active_organization_id = Some(org_id.clone());
        settings.clone()
    };
    crate::settings::persist_settings(&app, &snapshot)?;

    if let Some(token) = session_token {
        // 决策理由：组织切换已落盘；context 拉取失败不回滚，前端靠 bootstrap.activated / 下次 refresh 恢复。
        let _ = refresh_organization_context(
            cloud_manager(&state),
            &base_url,
            &token,
            &org_id,
        )
        .await;
    }

    let guard = cloud_manager(&state).lock().map_err(|e| e.to_string())?;
    Ok(guard.view(true))
}

#[command]
pub async fn cloud_context_get_organization_context(
    state: State<'_, AppState>,
    organization_id: Option<String>,
) -> Result<AgentOrganizationContext, String> {
    let (base_url, session_token, active) = {
        let settings = state.settings_read();
        let guard = cloud_manager(&state).lock().map_err(|e| e.to_string())?;
        let org = organization_id
            .filter(|s| !s.trim().is_empty())
            .or_else(|| guard.active_organization_id.clone())
            .ok_or_else(|| "未选择组织".to_string())?;
        (
            settings.abu_api_base_url_or_default().to_string(),
            resolve_session_token(settings.abu_api_session_token.as_deref())
                .ok_or_else(|| "尚未登录 ABU 账户".to_string())?,
            org,
        )
    };
    refresh_organization_context(cloud_manager(&state), &base_url, &session_token, &active).await
}

/// Resolve billing organization for a conversation send (C §5.3 / §6.1).
pub fn resolve_billing_organization_id(
    manager: &CloudContextManager,
    conversation_binding: &ConversationBillingBinding,
    active_is_team: bool,
) -> Result<String, String> {
    if conversation_binding.is_bound() {
        return Ok(conversation_binding.organization_id.clone());
    }
    let active = manager
        .active_organization_id
        .clone()
        .ok_or_else(|| "未选择组织".to_string())?;
    if manager.legacy_mode || active == LEGACY_PERSONAL_ORG_ID {
        return Ok(active);
    }
    let is_personal = manager
        .bootstrap
        .as_ref()
        .and_then(|b| b.organizations.iter().find(|o| o.id == active))
        .map(|o| o.org_type == "personal")
        .unwrap_or(false);
    if is_personal {
        return Ok(active);
    }
    if active_is_team {
        return Err(
            "[ORG_BINDING_REQUIRED] 当前选择了团队组织。未绑定组织的旧对话不能自动升级为团队对话，请新建或复制对话。"
                .into(),
        );
    }
    Ok(active)
}

pub fn org_display_name(manager: &CloudContextManager, organization_id: &str) -> String {
    manager
        .bootstrap
        .as_ref()
        .and_then(|b| {
            b.organizations
                .iter()
                .find(|o| o.id == organization_id)
                .map(|o| o.name.clone())
        })
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| organization_id.to_string())
}

pub fn funding_mode_for(manager: &CloudContextManager, organization_id: &str) -> String {
    manager
        .caches
        .context
        .get(organization_id)
        .map(|e| e.value.funding.mode.clone())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| {
            let is_personal = manager
                .bootstrap
                .as_ref()
                .and_then(|b| b.organizations.iter().find(|o| o.id == organization_id))
                .map(|o| o.org_type == "personal")
                .unwrap_or(organization_id == LEGACY_PERSONAL_ORG_ID);
            if is_personal {
                "personal".into()
            } else {
                "organization_wallet".into()
            }
        })
}
