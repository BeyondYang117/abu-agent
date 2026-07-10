use std::time::Duration;

use tauri::State;
use tokio::time::timeout;

use crate::chat::agent::{execute::truncate_chars, stop as agent_stop};
use crate::chat::model_metadata::model_can_generate_images_directly;
use crate::chat::Conversation;
use crate::settings::{SessionModel, Settings};
use crate::state::AppState;

use super::super::model_call::call_chat_completion_message;

pub(super) async fn resolve_conversation_title(
    settings: &Settings,
    state: &State<'_, AppState>,
    conversation: &Conversation,
    user_content: &str,
    assistant_content: &str,
) -> String {
    let session = SessionModel {
        provider_id: conversation.provider_id.as_str(),
        model: conversation.model.as_str(),
    };
    match timeout(
        Duration::from_secs(8),
        generate_title_with_model(
            settings,
            state,
            &conversation.id,
            Some(session),
            user_content,
            assistant_content,
        ),
    )
    .await
    {
        Ok(Some(title)) => title,
        Ok(None) => generate_title(user_content),
        Err(_) => generate_title(user_content),
    }
}

async fn generate_title_with_model(
    settings: &Settings,
    state: &State<'_, AppState>,
    conversation_id: &str,
    session: Option<SessionModel<'_>>,
    user_content: &str,
    assistant_content: &str,
) -> Option<String> {
    let (provider_id, model) = settings.effective_title_summary_model_for_session(session);
    let provider = settings.get_provider(&provider_id)?.clone();
    if provider.api_keys.is_empty() || model.trim().is_empty() {
        return None;
    }
    if model_can_generate_images_directly(&provider, &model) {
        return None;
    }

    let language = crate::settings::resolve_chat_language(settings);
    let prompt = build_title_summary_prompt(user_content, assistant_content, &language);
    let retry_attempts = if settings.retry_enabled {
        settings.retry_attempts as usize
    } else {
        1
    };
    let messages = vec![
        serde_json::json!({
            "role": "system",
            "content": title_summary_system_prompt(&language),
        }),
        serde_json::json!({
            "role": "user",
            "content": prompt,
        }),
    ];
    let message = call_chat_completion_message(
        state,
        &provider,
        &model,
        messages,
        None,
        retry_attempts,
        false,
        Some(conversation_id),
        None,
        "Chat title summary",
    )
    .await
    .ok()?;
    let raw = agent_stop::assistant_content_from_api_message(&message);

    sanitize_generated_title(&raw)
}

fn title_summary_system_prompt(language: &str) -> &'static str {
    if language.starts_with("zh") {
        "你只负责为对话生成简洁标题。只输出标题本身，不要解释。"
    } else {
        "You only generate concise conversation titles. Output only the title, with no explanation."
    }
}

pub(super) fn build_title_summary_prompt(
    user_content: &str,
    assistant_content: &str,
    language: &str,
) -> String {
    let user = truncate_chars(user_content.trim(), 1200);
    let assistant = truncate_chars(assistant_content.trim(), 1200);
    if language.starts_with("zh") {
        format!(
            "请根据下面的首轮对话生成一个简洁中文标题。\n要求：只输出标题本身；不要引号；不要句号；不超过 14 个汉字，最多 20 个字符。\n\n用户：{user}\n\n助手：{assistant}"
        )
    } else {
        format!(
            "Create a concise English title for this first chat turn.\nRules: output only the title; no quotes; no period; 3-6 words.\n\nUser: {user}\n\nAssistant: {assistant}"
        )
    }
}

pub(super) fn sanitize_generated_title(raw: &str) -> Option<String> {
    let mut title = raw
        .trim()
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?
        .to_string();

    title = title
        .trim_start_matches(['-', '*', '•', ' '])
        .trim_matches(['"', '\'', '`', '“', '”', '‘', '’', '。', '.', ' '])
        .to_string();
    for prefix in ["标题：", "标题:", "Title:", "Title：", "title:", "title："] {
        if let Some(rest) = title.strip_prefix(prefix) {
            title = rest.trim().to_string();
        }
    }
    title = title
        .trim_matches(['"', '\'', '`', '“', '”', '‘', '’', '。', '.', ' '])
        .to_string();
    if title.is_empty() {
        return None;
    }
    Some(generate_title(&title))
}

/// 生成对话标题（本地兜底截断）
pub(super) fn generate_title(content: &str) -> String {
    let trimmed = content.trim();
    let title = trimmed.chars().take(30).collect::<String>();
    if trimmed.chars().count() > 30 {
        format!("{title}...")
    } else {
        title
    }
}
