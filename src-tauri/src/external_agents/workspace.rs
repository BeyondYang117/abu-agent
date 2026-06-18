use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tauri::AppHandle;

use crate::chat::storage::{find_project_by_id, conversations_dir};
use crate::external_agents::types::RuntimeAgentDef;

#[derive(Debug, Clone)]
pub struct ResolvedWorkspace {
    pub cwd: PathBuf,
    pub is_managed_sandbox: bool,
}

pub fn resolve_effective_cwd(
    app: &AppHandle,
    conversation_id: &str,
    project_id: Option<&str>,
) -> Result<ResolvedWorkspace, String> {
    if let Some(project_id) = project_id.filter(|id| !id.trim().is_empty()) {
        if let Ok(project) = find_project_by_id(app, project_id) {
            if let Some(root) = project.root_path.filter(|p| !p.trim().is_empty()) {
                let path = PathBuf::from(root);
                if path.is_dir() {
                    return Ok(ResolvedWorkspace {
                        cwd: path,
                        is_managed_sandbox: false,
                    });
                }
            }
        }
    }

    let base = conversations_dir(app)?
        .parent()
        .ok_or_else(|| "chat data root unavailable".to_string())?
        .join("chat-workspaces")
        .join(conversation_id);
    std::fs::create_dir_all(&base).map_err(|e| format!("create workspace: {e}"))?;
    Ok(ResolvedWorkspace {
        cwd: base,
        is_managed_sandbox: true,
    })
}

pub fn can_write_mcp_json(workspace: &ResolvedWorkspace, allow_in_project: bool) -> bool {
    workspace.is_managed_sandbox || allow_in_project
}

pub fn is_managed_sandbox_path(cwd: &Path, app: &AppHandle) -> bool {
    conversations_dir(app)
        .ok()
        .and_then(|dir| dir.parent().map(|p| p.join("chat-workspaces")))
        .and_then(|workspaces| {
            cwd.canonicalize()
                .ok()
                .and_then(|canonical| {
                    workspaces.canonicalize().ok().map(|root| canonical.starts_with(root))
                })
        })
        .unwrap_or(false)
}

pub fn detection_cache_ttl() -> Duration {
    Duration::from_secs(24 * 60 * 60)
}

pub fn detection_cache_fresh(stored_at: Instant) -> bool {
    stored_at.elapsed() < detection_cache_ttl()
}

pub fn extra_allowed_dirs_for_agent(
    def: &RuntimeAgentDef,
    skill_scan_paths: &[String],
) -> Vec<String> {
    if def.id == "codex" {
        return Vec::new();
    }
    skill_scan_paths
        .iter()
        .filter(|p| !p.trim().is_empty() && Path::new(p).is_dir())
        .cloned()
        .collect()
}
