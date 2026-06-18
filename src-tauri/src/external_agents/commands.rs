use tauri::AppHandle;

use crate::chat::storage::{load_conversation, save_conversation};
use crate::chat::types::AgentRuntimeConfig;
use crate::external_agents::detection::detect_all_agents;

#[tauri::command]
pub async fn chat_detect_external_agents() -> Result<serde_json::Value, String> {
    let agents = detect_all_agents().await;
    Ok(serde_json::json!({
        "success": true,
        "agents": agents,
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
