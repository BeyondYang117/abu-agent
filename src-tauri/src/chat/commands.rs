use std::{
    fs,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose, Engine as _};
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::api::{call_vision_api, send_with_failover, stream_chat_call};
use crate::apple_intelligence::APPLE_INTELLIGENCE_BASE_URL;
use crate::settings::{
    default_system_prompt, no_think_instruction, persist_settings, ExplainMessage,
};
use crate::state::AppState;

use super::storage::{
    conversation_attachments_dir, delete_conversation as delete_conv,
    get_conversations as get_convs, load_conversation, save_conversation,
};
use super::{Attachment, ChatMessage, Conversation};

/// 获取对话列表
#[tauri::command]
pub(crate) fn chat_get_conversations(
    app: AppHandle,
    offset: usize,
    limit: usize,
    folder: Option<String>,
) -> Result<serde_json::Value, String> {
    let conversations = get_convs(&app, offset, limit, folder)?;
    Ok(serde_json::json!({
        "success": true,
        "conversations": conversations,
    }))
}

/// 获取对话详情
#[tauri::command]
pub(crate) fn chat_get_conversation(
    app: AppHandle,
    conversation_id: String,
) -> Result<serde_json::Value, String> {
    let conversation = load_conversation(&app, &conversation_id)?;
    Ok(serde_json::json!({
        "success": true,
        "conversation": conversation,
    }))
}

/// 创建新对话
#[tauri::command]
pub(crate) fn chat_create_conversation(
    app: AppHandle,
    state: State<AppState>,
    provider_id: Option<String>,
    model: Option<String>,
    folder: Option<String>,
) -> Result<serde_json::Value, String> {
    let settings = state.settings_read().clone();

    // 使用提供的 provider/model，或者回退到默认配置
    let provider_id = provider_id.unwrap_or_else(|| {
        if !settings.chat_provider_id.is_empty() {
            settings.chat_provider_id.clone()
        } else if !settings.lens.provider_id.is_empty() {
            settings.lens.provider_id.clone()
        } else {
            settings.translator_provider_id.clone()
        }
    });

    let model = model.unwrap_or_else(|| {
        if !settings.chat_model.is_empty() {
            settings.chat_model.clone()
        } else if !settings.lens.model.is_empty() {
            settings.lens.model.clone()
        } else {
            settings.translator_model.clone()
        }
    });

    let now = chrono::Local::now().timestamp();
    let conversation = Conversation {
        id: format!("conv_{}", Uuid::new_v4()),
        title: "新对话".to_string(),
        provider_id,
        model,
        messages: vec![],
        created_at: now,
        updated_at: now,
        pinned: false,
        folder,
    };

    save_conversation(&app, &conversation)?;

    Ok(serde_json::json!({
        "success": true,
        "conversation": conversation,
    }))
}

/// 发送消息
#[tauri::command]
pub(crate) async fn chat_send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    conversation_id: String,
    content: String,
    attachments: Vec<String>,
) -> Result<serde_json::Value, String> {
    let mut conversation = load_conversation(&app, &conversation_id)?;
    let message_attachments = save_message_attachments(&app, &conversation_id, attachments)?;
    let api_content = compose_user_content_for_api(&content, &message_attachments);
    let title_source = title_source_for_user_message(&content, &message_attachments);
    let last_user_image_paths =
        stored_image_paths_for_attachments(&app, &conversation_id, &message_attachments)?;

    // 创建用户消息
    let user_message = ChatMessage {
        id: format!("msg_{}", Uuid::new_v4()),
        role: "user".to_string(),
        content: content.clone(),
        attachments: message_attachments,
        reasoning: None,
        timestamp: chrono::Local::now().timestamp(),
    };

    conversation.messages.push(user_message.clone());
    conversation.updated_at = chrono::Local::now().timestamp();
    save_conversation(&app, &conversation)?;

    match complete_assistant_reply(
        &app,
        &state,
        &mut conversation,
        Some(title_source.as_str()),
        Some(api_content.as_str()),
        &last_user_image_paths,
    )
    .await
    {
        Ok(()) => Ok(serde_json::json!({
            "success": true,
            "conversation": conversation,
        })),
        Err(err) => Ok(serde_json::json!({
            "success": false,
            "error": err,
        })),
    }
}

fn save_message_attachments(
    app: &AppHandle,
    conversation_id: &str,
    attachment_paths: Vec<String>,
) -> Result<Vec<Attachment>, String> {
    let mut attachments = Vec::new();
    if attachment_paths.is_empty() {
        return Ok(attachments);
    }

    let dir = conversation_attachments_dir(app, conversation_id)?;
    for source in attachment_paths {
        let source_path = Path::new(&source);
        if !source_path.is_file() {
            return Err(format!("附件不存在或不是文件: {source}"));
        }

        let id = format!("att_{}", Uuid::new_v4());
        let original_name = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("attachment");
        let safe_name = sanitize_attachment_name(original_name);
        let stored_name = format!("{}-{}", id, safe_name);
        let dest = dir.join(&stored_name);
        fs::copy(source_path, &dest).map_err(|e| format!("保存附件失败: {e}"))?;

        attachments.push(Attachment {
            id,
            attachment_type: attachment_type_for_name(original_name).to_string(),
            name: original_name.to_string(),
            path: stored_name,
        });
    }

    Ok(attachments)
}

fn sanitize_attachment_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches(['.', ' ', '_']).trim();
    if trimmed.is_empty() {
        "attachment".to_string()
    } else {
        trimmed.to_string()
    }
}

fn attachment_type_for_name(name: &str) -> &'static str {
    let ext = Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tiff" | "tif" | "heic" | "heif" => {
            "image"
        }
        _ => "file",
    }
}

fn attachment_type_label(attachment_type: &str) -> &'static str {
    match attachment_type {
        "image" => "图片",
        _ => "文件",
    }
}

fn compose_user_content_for_api(content: &str, attachments: &[Attachment]) -> String {
    let trimmed = content.trim();
    if attachments.is_empty() {
        return trimmed.to_string();
    }

    let has_images = attachments
        .iter()
        .any(|attachment| attachment.attachment_type == "image");
    let has_files = attachments
        .iter()
        .any(|attachment| attachment.attachment_type != "image");
    let attachment_lines = attachments
        .iter()
        .map(|attachment| {
            format!(
                "- {} ({})",
                attachment.name,
                attachment_type_label(&attachment.attachment_type)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let capability_note = match (has_images, has_files) {
        (true, true) => {
            "图片附件会随本轮请求发送给视觉模型；普通文件当前只保存元数据，模型不会读取文件正文。"
        }
        (true, false) => "图片附件会随本轮请求发送给视觉模型。",
        (false, true) => "当前聊天只保存文件元数据，模型不会读取文件正文。",
        (false, false) => "",
    };
    let attachment_note = format!(
        "[已添加附件]\n{}\n\n注意：{}",
        attachment_lines, capability_note
    );

    if trimmed.is_empty() {
        attachment_note
    } else {
        format!("{trimmed}\n\n{attachment_note}")
    }
}

fn title_source_for_user_message(content: &str, attachments: &[Attachment]) -> String {
    let trimmed = content.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }

    let names = attachments
        .iter()
        .map(|attachment| attachment.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if names.is_empty() {
        "新对话".to_string()
    } else {
        format!("附件: {names}")
    }
}

fn stored_image_paths_for_attachments(
    app: &AppHandle,
    conversation_id: &str,
    attachments: &[Attachment],
) -> Result<Vec<PathBuf>, String> {
    let image_attachments = attachments
        .iter()
        .filter(|attachment| attachment.attachment_type == "image")
        .collect::<Vec<_>>();
    if image_attachments.is_empty() {
        return Ok(Vec::new());
    }

    let dir = conversation_attachments_dir(app, conversation_id)?;
    image_attachments
        .into_iter()
        .map(|attachment| {
            let stored = Path::new(&attachment.path);
            if stored.components().count() != 1 {
                return Err(format!("Invalid attachment path: {}", attachment.path));
            }
            let path = dir.join(stored);
            if !path.is_file() {
                return Err(format!("图片附件不存在: {}", attachment.name));
            }
            Ok(path)
        })
        .collect()
}

async fn complete_assistant_reply(
    app: &AppHandle,
    state: &State<'_, AppState>,
    conversation: &mut Conversation,
    title_from_first_user: Option<&str>,
    last_user_api_content: Option<&str>,
    last_user_image_paths: &[PathBuf],
) -> Result<(), String> {
    let settings = state.settings_read().clone();
    let last_user_idx = conversation.messages.iter().rposition(|m| m.role == "user");

    let api_messages: Vec<ExplainMessage> = conversation
        .messages
        .iter()
        .enumerate()
        .map(|(idx, m)| ExplainMessage {
            role: m.role.clone(),
            content: if Some(idx) == last_user_idx {
                last_user_api_content
                    .unwrap_or(m.content.as_str())
                    .to_string()
            } else {
                m.content.clone()
            },
        })
        .collect();

    let language = if !settings.lens.default_language.is_empty() {
        settings.lens.default_language.clone()
    } else {
        "zh".to_string()
    };

    let stream_enabled = settings.lens.stream_enabled;
    let thinking_enabled = settings.lens.thinking_enabled;
    let retry_attempts = if settings.retry_enabled {
        settings.retry_attempts as usize
    } else {
        0
    };

    let image_id = String::new();

    let response = if last_user_image_paths.is_empty() {
        call_vision_api(
            app,
            state,
            &image_id,
            api_messages,
            &language,
            retry_attempts,
            stream_enabled,
            "answer",
            "chat-stream",
            Some(&conversation.provider_id),
            Some(&conversation.model),
            None,
            thinking_enabled,
        )
        .await?
    } else {
        call_chat_api_with_images(
            app,
            state,
            api_messages,
            &language,
            retry_attempts,
            stream_enabled,
            "answer",
            "chat-stream",
            Some(&conversation.provider_id),
            Some(&conversation.model),
            thinking_enabled,
            last_user_image_paths,
        )
        .await?
    };

    let assistant_message = ChatMessage {
        id: format!("msg_{}", Uuid::new_v4()),
        role: "assistant".to_string(),
        content: response,
        attachments: vec![],
        reasoning: None,
        timestamp: chrono::Local::now().timestamp(),
    };

    conversation.messages.push(assistant_message);

    if let Some(user_content) = title_from_first_user {
        if conversation.messages.len() == 2 && conversation.title == "新对话" {
            conversation.title = generate_title(user_content);
        }
    }

    conversation.updated_at = chrono::Local::now().timestamp();
    save_conversation(app, conversation)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn call_chat_api_with_images(
    app: &AppHandle,
    state: &State<'_, AppState>,
    messages: Vec<ExplainMessage>,
    language: &str,
    retry_attempts: usize,
    stream: bool,
    stream_kind: &str,
    event_name: &str,
    provider_id_override: Option<&str>,
    model_override: Option<&str>,
    thinking_enabled: bool,
    image_paths: &[PathBuf],
) -> Result<String, String> {
    let settings = state.settings_read().clone();
    let provider_id = provider_id_override
        .filter(|s| !s.is_empty())
        .unwrap_or(&settings.translator_provider_id);
    let provider = settings
        .get_provider(provider_id)
        .ok_or_else(|| "Vision provider not found".to_string())?;

    if provider.base_url == APPLE_INTELLIGENCE_BASE_URL {
        return Err(
            "Apple Intelligence 暂不支持图片附件，请为 AI 对话配置云端视觉 provider".into(),
        );
    }

    let model = model_override
        .filter(|s| !s.is_empty())
        .unwrap_or(&settings.translator_model);
    if model.trim().is_empty() {
        return Err("Please select a model first".to_string());
    }

    let system_prompt = {
        let base = default_system_prompt(language, true);
        if thinking_enabled {
            base
        } else {
            format!("{}{}", base, no_think_instruction(language))
        }
    };
    let last_user_idx = messages.iter().rposition(|message| message.role == "user");
    let mut api_messages = vec![serde_json::json!({
        "role": "system",
        "content": system_prompt,
    })];

    for (idx, message) in messages.iter().enumerate() {
        if Some(idx) == last_user_idx {
            let mut content = image_paths
                .iter()
                .map(image_content_part)
                .collect::<Result<Vec<_>, _>>()?;
            content.push(serde_json::json!({
                "type": "text",
                "text": message.content,
            }));
            api_messages.push(serde_json::json!({
                "role": message.role,
                "content": content,
            }));
        } else {
            api_messages.push(serde_json::json!({
                "role": message.role,
                "content": message.content,
            }));
        }
    }

    let mut body = serde_json::json!({
        "model": model,
        "messages": api_messages,
        "temperature": 0.7,
        "max_tokens": 2000,
    });
    if stream {
        body["stream"] = serde_json::json!(true);
    }
    if !thinking_enabled {
        body["thinking"] = serde_json::json!({ "type": "disabled" });
    }

    if stream {
        return stream_chat_call(
            app,
            state,
            provider,
            model,
            body,
            retry_attempts,
            "",
            stream_kind,
            event_name,
        )
        .await;
    }

    let url = format!(
        "{}/chat/completions",
        provider.base_url.trim_end_matches('/')
    );
    let response = send_with_failover(
        state,
        "Chat image API",
        retry_attempts,
        &provider.id,
        &provider.api_keys,
        |key| {
            state
                .http
                .post(url.clone())
                .bearer_auth(key)
                .json(&body)
                .send()
        },
    )
    .await?;
    let raw = response
        .text()
        .await
        .map_err(|e| format!("Chat image API read body: {e}"))?;
    let value: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
        format!(
            "Chat image API parse JSON: {} (body: {})",
            e,
            raw.chars().take(500).collect::<String>()
        )
    })?;
    let content = value
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .ok_or_else(|| {
            format!(
                "Invalid chat image response: {}",
                raw.chars().take(500).collect::<String>()
            )
        })?;

    Ok(content.trim().to_string())
}

fn image_content_part(path: &PathBuf) -> Result<serde_json::Value, String> {
    let bytes = fs::read(path).map_err(|e| format!("读取图片附件失败: {e}"))?;
    let base64 = general_purpose::STANDARD.encode(bytes);
    let mime = image_mime_for_path(path);
    Ok(serde_json::json!({
        "type": "image_url",
        "image_url": { "url": format!("data:{mime};base64,{base64}") },
    }))
}

fn image_mime_for_path(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tiff" | "tif" => "image/tiff",
        "heic" => "image/heic",
        "heif" => "image/heif",
        _ => "image/png",
    }
}

fn find_message_index(conversation: &Conversation, message_id: &str) -> Result<usize, String> {
    conversation
        .messages
        .iter()
        .position(|m| m.id == message_id)
        .ok_or_else(|| "消息不存在".to_string())
}

/// 更新单条消息（仅助手回复）
#[tauri::command]
pub(crate) fn chat_update_message(
    app: AppHandle,
    conversation_id: String,
    message_id: String,
    content: String,
) -> Result<serde_json::Value, String> {
    let mut conversation = load_conversation(&app, &conversation_id)?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err("消息内容不能为空".to_string());
    }

    let idx = find_message_index(&conversation, &message_id)?;
    if conversation.messages[idx].role != "assistant" {
        return Err("仅支持编辑助手回复".to_string());
    }

    conversation.messages[idx].content = trimmed.to_string();
    conversation.messages[idx].timestamp = chrono::Local::now().timestamp();
    conversation.updated_at = chrono::Local::now().timestamp();
    save_conversation(&app, &conversation)?;

    Ok(serde_json::json!({
        "success": true,
        "conversation": conversation,
    }))
}

/// 重新生成助手回复（移除该条及之后的消息，再基于此前上下文请求新回复）
#[tauri::command]
pub(crate) async fn chat_regenerate_message(
    app: AppHandle,
    state: State<'_, AppState>,
    conversation_id: String,
    message_id: String,
) -> Result<serde_json::Value, String> {
    let mut conversation = load_conversation(&app, &conversation_id)?;
    let idx = find_message_index(&conversation, &message_id)?;
    if conversation.messages[idx].role != "assistant" {
        return Err("仅支持重新生成助手回复".to_string());
    }

    conversation.messages.truncate(idx);
    if conversation.messages.last().map(|m| m.role.as_str()) != Some("user") {
        return Err("缺少对应的用户消息，无法重新生成".to_string());
    }

    conversation.updated_at = chrono::Local::now().timestamp();
    save_conversation(&app, &conversation)?;

    let last_user_api_content = conversation
        .messages
        .last()
        .filter(|message| message.role == "user")
        .map(|message| compose_user_content_for_api(&message.content, &message.attachments));
    let last_user_image_paths = conversation
        .messages
        .last()
        .filter(|message| message.role == "user")
        .map(|message| {
            stored_image_paths_for_attachments(&app, &conversation_id, &message.attachments)
        })
        .transpose()?
        .unwrap_or_default();

    match complete_assistant_reply(
        &app,
        &state,
        &mut conversation,
        None,
        last_user_api_content.as_deref(),
        &last_user_image_paths,
    )
    .await
    {
        Ok(()) => Ok(serde_json::json!({
            "success": true,
            "conversation": conversation,
        })),
        Err(err) => Ok(serde_json::json!({
            "success": false,
            "error": err,
        })),
    }
}

/// 删除单条消息
#[tauri::command]
pub(crate) fn chat_delete_message(
    app: AppHandle,
    conversation_id: String,
    message_id: String,
) -> Result<serde_json::Value, String> {
    let mut conversation = load_conversation(&app, &conversation_id)?;
    let idx = find_message_index(&conversation, &message_id)?;
    if conversation.messages[idx].role != "assistant" {
        return Err("仅支持删除助手回复".to_string());
    }

    conversation.messages.remove(idx);
    conversation.updated_at = chrono::Local::now().timestamp();
    save_conversation(&app, &conversation)?;

    Ok(serde_json::json!({
        "success": true,
        "conversation": conversation,
    }))
}

/// 删除对话
#[tauri::command]
pub(crate) fn chat_delete_conversation(
    app: AppHandle,
    conversation_id: String,
) -> Result<serde_json::Value, String> {
    delete_conv(&app, &conversation_id)?;
    Ok(serde_json::json!({
        "success": true,
    }))
}

/// 更新对话（标题、置顶、文件夹等）
#[tauri::command]
pub(crate) fn chat_update_conversation(
    app: AppHandle,
    state: State<AppState>,
    conversation_id: String,
    title: Option<String>,
    pinned: Option<bool>,
    folder: Option<String>,
    provider_id: Option<String>,
    model: Option<String>,
) -> Result<serde_json::Value, String> {
    let mut conversation = load_conversation(&app, &conversation_id)?;

    if let Some(t) = title {
        conversation.title = t;
    }
    if let Some(p) = pinned {
        conversation.pinned = p;
    }
    if folder.is_some() {
        conversation.folder = folder;
    }
    let provider_model_changed = provider_id.is_some() || model.is_some();
    if let Some(provider_id) = provider_id {
        conversation.provider_id = provider_id;
    }
    if let Some(model) = model {
        conversation.model = model;
    }

    conversation.updated_at = chrono::Local::now().timestamp();
    save_conversation(&app, &conversation)?;

    if provider_model_changed {
        let updated_settings = {
            let mut settings = state.settings_write();
            settings.chat_provider_id = conversation.provider_id.clone();
            settings.chat_model = conversation.model.clone();
            settings.clone()
        };
        persist_settings(&app, &updated_settings)?;
    }

    Ok(serde_json::json!({
        "success": true,
        "conversation": conversation,
    }))
}

/// 生成对话标题（从第一条用户消息）
fn generate_title(content: &str) -> String {
    let trimmed = content.trim();
    let title = trimmed.chars().take(30).collect::<String>();
    if trimmed.chars().count() > 30 {
        format!("{title}...")
    } else {
        title
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_type_detects_images_case_insensitively() {
        assert_eq!(attachment_type_for_name("screenshot.PNG"), "image");
        assert_eq!(attachment_type_for_name("scan.tif"), "image");
        assert_eq!(attachment_type_for_name("photo.heic"), "image");
        assert_eq!(attachment_type_for_name("notes.pdf"), "file");
    }

    #[test]
    fn sanitize_attachment_name_removes_path_like_characters() {
        assert_eq!(sanitize_attachment_name("../secret?.png"), "secret_.png");
        assert_eq!(sanitize_attachment_name("   "), "attachment");
    }

    #[test]
    fn compose_user_content_for_api_mentions_attachment_names() {
        let content = compose_user_content_for_api(
            "看看这个",
            &[Attachment {
                id: "att_1".to_string(),
                attachment_type: "image".to_string(),
                name: "screen.png".to_string(),
                path: "att_1-screen.png".to_string(),
            }],
        );

        assert!(content.contains("看看这个"));
        assert!(content.contains("screen.png"));
        assert!(content.contains("图片附件会随本轮请求发送给视觉模型"));
    }

    #[test]
    fn title_source_uses_attachment_name_when_content_empty() {
        let title = title_source_for_user_message(
            "",
            &[Attachment {
                id: "att_1".to_string(),
                attachment_type: "file".to_string(),
                name: "notes.pdf".to_string(),
                path: "att_1-notes.pdf".to_string(),
            }],
        );

        assert_eq!(title, "附件: notes.pdf");
    }

    #[test]
    fn generate_title_truncates_unicode_safely() {
        let title = generate_title("附件: 这是一张非常非常非常非常非常非常非常长的图片文件名.png");

        assert!(title.ends_with("..."));
        assert!(title.chars().count() <= 33);
    }
}
