use tauri::AppHandle;

use crate::chat::storage::{load_conversation, save_conversation};
use crate::chat::types::AgentRuntimeConfig;
use crate::external_agents::detection::detect_all_agents;
use crate::external_agents::slash::list_external_cli_slash_commands;
use crate::state::AppState;

#[tauri::command]
pub async fn chat_detect_external_agents(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let agents = detect_all_agents().await;
    for agent in &agents {
        if !agent.models.is_empty() {
            state.set_cached_external_agent_models(agent.id.clone(), agent.models.clone());
        }
    }
    Ok(serde_json::json!({
        "success": true,
        "agents": agents,
    }))
}

#[tauri::command]
pub async fn chat_list_external_cli_slash_commands(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    agent_id: String,
    conversation_id: Option<String>,
) -> Result<serde_json::Value, String> {
    let (supports, commands, message) =
        list_external_cli_slash_commands(&app, &state, &agent_id, conversation_id.as_deref()).await?;
    Ok(serde_json::json!({
        "success": true,
        "supportsSlashCommands": supports,
        "commands": commands,
        "message": message,
    }))
}

#[tauri::command]
pub fn chat_set_agent_runtime(
    app: AppHandle,
    conversation_id: String,
    agent_runtime: AgentRuntimeConfig,
) -> Result<serde_json::Value, String> {
    let mut conversation = load_conversation(&app, &conversation_id)?;
    conversation.agent_runtime = agent_runtime;
    conversation.updated_at = chrono::Local::now().timestamp();
    save_conversation(&app, &conversation)?;
    Ok(serde_json::json!({
        "success": true,
        "conversation": conversation,
    }))
}
