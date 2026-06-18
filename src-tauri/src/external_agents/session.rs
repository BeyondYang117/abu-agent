use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use tauri::AppHandle;
use uuid::Uuid;

use tauri::Manager;

use crate::external_agents::types::ExternalAgentSession;

fn sessions_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir unavailable: {e}"))?;
    let dir = base.join("external-agent-sessions");
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| format!("create sessions dir: {e}"))?;
    }
    Ok(dir)
}

fn session_path(app: &AppHandle, conversation_id: &str) -> Result<PathBuf, String> {
    Ok(sessions_dir(app)?.join(format!("{conversation_id}.json")))
}

pub fn load_session(app: &AppHandle, conversation_id: &str) -> Option<ExternalAgentSession> {
    let path = session_path(app, conversation_id).ok()?;
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn save_session(app: &AppHandle, session: &ExternalAgentSession) -> Result<(), String> {
    let path = session_path(app, &session.conversation_id)?;
    let raw = serde_json::to_string_pretty(session).map_err(|e| e.to_string())?;
    fs::write(path, raw).map_err(|e| e.to_string())
}

pub fn stable_prompt_hash(instructions: &str) -> String {
    let mut hasher = DefaultHasher::new();
    instructions.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub struct AgentResumeContext {
    pub resume_session_id: Option<String>,
    pub new_session_id: Option<String>,
    pub is_resuming: bool,
    pub stored_stable_prompt_hash: Option<String>,
    pub skip_instructions: bool,
}

pub fn resolve_agent_resume_context(
    app: &AppHandle,
    conversation_id: &str,
    agent_id: &str,
    resumes_via_cli: bool,
    instructions: &str,
) -> AgentResumeContext {
    if !resumes_via_cli {
        return AgentResumeContext {
            resume_session_id: None,
            new_session_id: None,
            is_resuming: false,
            stored_stable_prompt_hash: None,
            skip_instructions: false,
        };
    }

    let hash = stable_prompt_hash(instructions);
    if let Some(stored) = load_session(app, conversation_id).filter(|s| s.agent_id == agent_id) {
        let skip = stored
            .stable_prompt_hash
            .as_ref()
            .is_some_and(|h| h == &hash);
        return AgentResumeContext {
            resume_session_id: Some(stored.session_id.clone()),
            new_session_id: None,
            is_resuming: true,
            stored_stable_prompt_hash: stored.stable_prompt_hash.clone(),
            skip_instructions: skip,
        };
    }

    AgentResumeContext {
        resume_session_id: None,
        new_session_id: Some(Uuid::new_v4().to_string()),
        is_resuming: false,
        stored_stable_prompt_hash: None,
        skip_instructions: false,
    }
}

pub fn persist_delivered_session(
    app: &AppHandle,
    conversation_id: &str,
    agent_id: &str,
    resume_ctx: &AgentResumeContext,
    instructions: &str,
) -> Result<(), String> {
    if !resume_ctx.is_resuming {
        if let Some(session_id) = resume_ctx.new_session_id.as_ref() {
            save_session(
                app,
                &ExternalAgentSession {
                    conversation_id: conversation_id.to_string(),
                    agent_id: agent_id.to_string(),
                    session_id: session_id.clone(),
                    stable_prompt_hash: Some(stable_prompt_hash(instructions)),
                },
            )?;
        }
    } else if resume_ctx.stored_stable_prompt_hash.as_deref()
        != Some(stable_prompt_hash(instructions).as_str())
    {
        if let Some(mut stored) = load_session(app, conversation_id) {
            stored.stable_prompt_hash = Some(stable_prompt_hash(instructions));
            save_session(app, &stored)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::stable_prompt_hash;

    #[test]
    fn stable_prompt_hash_is_deterministic() {
        assert_eq!(stable_prompt_hash("a"), stable_prompt_hash("a"));
        assert_ne!(stable_prompt_hash("a"), stable_prompt_hash("b"));
    }
}

pub fn sessions_root(app: &AppHandle) -> Result<PathBuf, String> {
    sessions_dir(app)
}

pub fn session_file_exists(app: &AppHandle, conversation_id: &str) -> bool {
    session_path(app, conversation_id)
        .ok()
        .is_some_and(|p| p.is_file())
}

pub fn managed_session_path(_app: &AppHandle, _conversation_id: &str) -> Option<PathBuf> {
    None
}

pub fn is_managed_session_file(_path: &Path) -> bool {
    false
}
