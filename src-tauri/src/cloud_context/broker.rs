//! Acquire cloud relay credentials for a chat round (legacy or scoped task).

use crate::state::AppState;

use super::session::resolve_session_token;
use super::tasks::{
    active_relay_from_create, create_agent_task, finalize_agent_task, map_task_error,
    refresh_task_relay, CreateTaskRequest,
};
use super::types::{
    ConversationBillingBinding, ActiveTaskRelay, LEGACY_PERSONAL_ORG_ID,
};
use super::{funding_mode_for, org_display_name, resolve_billing_organization_id};

pub struct CloudRunCredentials {
    pub api_key: String,
    pub base_url: String,
    pub task_id: Option<String>,
    pub root_task_id: Option<String>,
    pub organization_id: Option<String>,
    pub scoped: bool,
}

/// Bind conversation billing on first cloud send when allowed (C §5.3).
pub fn ensure_conversation_billing_binding(
    state: &AppState,
    billing: &mut ConversationBillingBinding,
) -> Result<String, String> {
    if billing.is_bound() {
        return Ok(billing.organization_id.clone());
    }
    let cloud = state
        .cloud_context
        .lock()
        .map_err(|e| e.to_string())?;
    let active = cloud
        .active_organization_id
        .clone()
        .unwrap_or_else(|| LEGACY_PERSONAL_ORG_ID.to_string());
    let is_team = cloud
        .bootstrap
        .as_ref()
        .and_then(|b| b.organizations.iter().find(|o| o.id == active))
        .map(|o| o.org_type != "personal")
        .unwrap_or(false);
    let org_id = resolve_billing_organization_id(&cloud, billing, is_team)?;
    let name = org_display_name(&cloud, &org_id);
    let funding = funding_mode_for(&cloud, &org_id);
    drop(cloud);

    billing.organization_id = org_id.clone();
    billing.organization_name_snapshot = name;
    billing.funding_mode = funding;
    billing.bound_at = chrono::Local::now().timestamp();
    Ok(org_id)
}

pub async fn acquire_cloud_run_credentials(
    state: &AppState,
    model: &str,
    conversation_id: &str,
    billing: &mut ConversationBillingBinding,
    parent_task_id: Option<&str>,
) -> Result<CloudRunCredentials, String> {
    let (base_url, session_token, device_id) = {
        let settings = state.settings_read();
        let base_url = settings.abu_api_base_url_or_default().to_string();
        let session_token = resolve_session_token(settings.abu_api_session_token.as_deref())
            .ok_or_else(|| "尚未登录 ABU 账户".to_string())?;
        let device_id = settings
            .abu_api_device_id
            .clone()
            .unwrap_or_else(|| "unknown-device".to_string());
        (base_url, session_token, device_id)
    };

    let scoped = {
        let cloud = state.cloud_context.lock().map_err(|e| e.to_string())?;
        cloud.scoped_relay_enabled()
    };

    if !scoped {
        let credentials =
            crate::abu_api::fetch_agent_relay_credentials(&base_url, &session_token, model).await?;
        return Ok(CloudRunCredentials {
            api_key: credentials.api_key,
            base_url,
            task_id: None,
            root_task_id: None,
            organization_id: None,
            scoped: false,
        });
    }

    let organization_id = ensure_conversation_billing_binding(state, billing)?;
    // legacy-personal is client-only; server personal org uses empty / personal id from bootstrap.
    let server_org_id = if organization_id == LEGACY_PERSONAL_ORG_ID {
        String::new()
    } else {
        organization_id.clone()
    };

    let request = CreateTaskRequest {
        session_id: format!("conv:{conversation_id}"),
        device_id,
        task_type: "chat".into(),
        organization_id: server_org_id.clone(),
        parent_task_id: parent_task_id.unwrap_or("").to_string(),
        soft_cap: None,
        hard_cap: None,
    };

    let created = create_agent_task(&base_url, &session_token, &request)
        .await
        .map_err(map_task_error)?;

    let mut relay = active_relay_from_create(&created, &server_org_id);
    if relay.is_none() {
        let view = refresh_task_relay(
            &base_url,
            &session_token,
            &created.task_id,
            &server_org_id,
        )
        .await
        .map_err(map_task_error)?;
        relay = Some(ActiveTaskRelay {
            task_id: created.task_id.clone(),
            root_task_id: if created.root_task_id.is_empty() {
                created.task_id.clone()
            } else {
                created.root_task_id.clone()
            },
            organization_id: server_org_id.clone(),
            credential: view.credential,
            expires_at: view.expires_at,
            base_url: view.base_url,
            price_version: created.price_version,
            price_version_expires_at: created.price_version_expires_at,
        });
    }

    let relay = relay.ok_or_else(|| "服务端未返回任务 Relay 凭证".to_string())?;
    let api_key = relay.credential.clone();
    let relay_base = relay
        .base_url
        .clone()
        .unwrap_or_else(|| format!("{}/v1", base_url.trim_end_matches('/')));
    // Store strip /v1 for create_abu_api_virtual_provider which appends /v1.
    let provider_base = relay_base
        .trim_end_matches('/')
        .trim_end_matches("/v1")
        .to_string();

    if let Ok(mut cloud) = state.cloud_context.lock() {
        cloud.remember_relay(relay.clone());
    }

    Ok(CloudRunCredentials {
        api_key,
        base_url: if provider_base.is_empty() {
            base_url
        } else {
            provider_base
        },
        task_id: Some(relay.task_id),
        root_task_id: Some(relay.root_task_id),
        organization_id: Some(organization_id),
        scoped: true,
    })
}

pub async fn finalize_cloud_run(
    state: &AppState,
    credentials: &CloudRunCredentials,
    succeeded: bool,
    reason: Option<&str>,
) {
    let Some(task_id) = credentials.task_id.as_deref() else {
        return;
    };
    let (base_url, session_token) = {
        let settings = state.settings_read();
        let token = resolve_session_token(settings.abu_api_session_token.as_deref());
        (settings.abu_api_base_url_or_default().to_string(), token)
    };
    let Some(session_token) = session_token else {
        return;
    };
    let org = credentials
        .organization_id
        .as_deref()
        .filter(|id| *id != LEGACY_PERSONAL_ORG_ID)
        .unwrap_or("");
    let status = if succeeded { "succeeded" } else { "failed" };
    if let Err(err) =
        finalize_agent_task(&base_url, &session_token, task_id, org, status, reason).await
    {
        eprintln!(
            "[cloud-context] finalize task {task_id} failed: {}",
            map_task_error(err)
        );
    }
    if let Ok(mut cloud) = state.cloud_context.lock() {
        cloud.take_relay_for_task(task_id);
    }
}
