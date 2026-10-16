//! Cloud context types matching abu-api agent bootstrap / context contracts (C §4–§5).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const LEGACY_PERSONAL_ORG_ID: &str = "legacy-personal";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AgentBootstrapFeatures {
    #[serde(default)]
    pub organizations: bool,
    #[serde(default)]
    pub scoped_relay_credentials: bool,
    #[serde(default)]
    pub price_version: bool,
    #[serde(default)]
    pub managed_cli_session: bool,
    #[serde(default)]
    pub wallet_breakdown: bool,
    #[serde(default)]
    pub member_budgets: bool,
    #[serde(default)]
    pub commission: bool,
    #[serde(default)]
    pub missions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AgentBootstrapUser {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AgentBootstrapOrganization {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "type")]
    pub org_type: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub status: String,
    /// Shared-org activation; personal orgs are always true (C §4.1).
    #[serde(default)]
    pub activated: bool,
    #[serde(default)]
    pub privacy_consent_required: bool,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CloudBootstrap {
    #[serde(default)]
    pub schema_version: i64,
    #[serde(default)]
    pub api_version: i64,
    #[serde(default)]
    pub features: AgentBootstrapFeatures,
    #[serde(default)]
    pub minimum_client_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_sunset_at: Option<i64>,
    #[serde(default)]
    pub privacy_notice_version: i64,
    #[serde(default)]
    pub user: AgentBootstrapUser,
    #[serde(default)]
    pub organizations: Vec<AgentBootstrapOrganization>,
    #[serde(default)]
    pub default_organization_id: String,
}

impl CloudBootstrap {
    /// Synthetic personal-only view when bootstrap is missing (C §11.2).
    pub fn legacy_personal(user: AgentBootstrapUser) -> Self {
        Self {
            schema_version: 1,
            api_version: 1,
            features: AgentBootstrapFeatures::default(),
            minimum_client_version: String::new(),
            legacy_sunset_at: None,
            user,
            privacy_notice_version: 0,
            organizations: vec![AgentBootstrapOrganization {
                id: LEGACY_PERSONAL_ORG_ID.to_string(),
                name: "个人账户".to_string(),
                org_type: "personal".to_string(),
                role: "owner".to_string(),
                status: "active".to_string(),
                activated: true,
                privacy_consent_required: false,
                permissions: vec!["model.use".into(), "usage.view.self".into()],
            }],
            default_organization_id: LEGACY_PERSONAL_ORG_ID.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AgentContextOrganization {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "type")]
    pub org_type: String,
    #[serde(default)]
    pub edition: String,
    #[serde(default)]
    pub activated: bool,
    #[serde(default)]
    pub member_limit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AgentContextMembership {
    pub id: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AgentContextFunding {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub billing_account_id: String,
    #[serde(default)]
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AgentContextBalances {
    #[serde(default)]
    pub permanent: i64,
    #[serde(default)]
    pub temporary: i64,
    #[serde(default)]
    pub reserved: i64,
    #[serde(default)]
    pub available: i64,
    #[serde(default)]
    pub debt: i64,
    #[serde(default)]
    pub overdraft_limit: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nearest_expiry_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AgentContextBudget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_limit: Option<i64>,
    #[serde(default)]
    pub daily_used: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monthly_limit: Option<i64>,
    #[serde(default)]
    pub monthly_used: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct AgentOrganizationContext {
    #[serde(default)]
    pub schema_version: i64,
    #[serde(default)]
    pub context_version: i64,
    #[serde(default)]
    pub organization: AgentContextOrganization,
    #[serde(default)]
    pub membership: AgentContextMembership,
    #[serde(default)]
    pub funding: AgentContextFunding,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balances: Option<AgentContextBalances>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<AgentContextBudget>,
}

/// Desensitized view for React (no session / relay tokens).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CloudContextView {
    pub legacy_mode: bool,
    pub authenticated: bool,
    pub active_organization_id: Option<String>,
    pub bootstrap: Option<CloudBootstrap>,
    pub context: Option<AgentOrganizationContext>,
    pub minimum_client_version: Option<String>,
    pub client_upgrade_required: bool,
    pub client_upgrade_download_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ConversationBillingBinding {
    #[serde(default)]
    pub organization_id: String,
    #[serde(default)]
    pub organization_name_snapshot: String,
    #[serde(default)]
    pub funding_mode: String,
    #[serde(default)]
    pub bound_at: i64,
}

impl ConversationBillingBinding {
    pub fn is_bound(&self) -> bool {
        !self.organization_id.trim().is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct OrgScopedCacheEntry<T> {
    pub organization_id: String,
    pub context_version: i64,
    pub fetched_at: i64,
    pub value: T,
}

#[derive(Debug, Default)]
pub struct OrgScopedCaches {
    pub models: HashMap<String, OrgScopedCacheEntry<serde_json::Value>>,
    pub entitlements: HashMap<String, OrgScopedCacheEntry<serde_json::Value>>,
    pub context: HashMap<String, OrgScopedCacheEntry<AgentOrganizationContext>>,
}

#[derive(Debug, Clone)]
pub struct ActiveTaskRelay {
    pub task_id: String,
    pub root_task_id: String,
    pub organization_id: String,
    pub credential: String,
    pub expires_at: i64,
    pub base_url: Option<String>,
    pub price_version: Option<i64>,
    pub price_version_expires_at: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_personal_hides_org_features() {
        let boot = CloudBootstrap::legacy_personal(AgentBootstrapUser {
            id: 1,
            username: "abu".into(),
            display_name: "ABU".into(),
        });
        assert!(!boot.features.organizations);
        assert_eq!(boot.default_organization_id, LEGACY_PERSONAL_ORG_ID);
        assert_eq!(boot.organizations.len(), 1);
    }
}
