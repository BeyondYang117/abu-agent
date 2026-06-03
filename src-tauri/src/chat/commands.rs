use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use base64::{engine::general_purpose, Engine as _};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};
use tokio::time::{sleep, timeout};
use uuid::Uuid;

use crate::api::{extract_status_code, send_with_failover};
use crate::apple_intelligence::APPLE_INTELLIGENCE_BASE_URL;
use crate::mcp::{self, ChatToolDefinition};
use crate::settings::{default_system_prompt, no_think_instruction, persist_settings};
use crate::skills;
use crate::state::AppState;

use super::storage::{
    conversation_attachments_dir, delete_conversation as delete_conv,
    get_conversations as get_convs, load_conversation, save_conversation,
};
use super::{Attachment, ChatMessage, Conversation, ToolCallRecord, ToolCallStatus};

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
        active_skill_id: None,
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
    active_skill_id: Option<String>,
) -> Result<serde_json::Value, String> {
    let mut conversation = load_conversation(&app, &conversation_id)?;
    if let Some(skill_id) = active_skill_id.as_deref() {
        let trimmed = skill_id.trim();
        conversation.active_skill_id = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
    }
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
        tool_calls: Vec::new(),
        active_skill_id: None,
        timestamp: chrono::Local::now().timestamp(),
    };

    conversation.messages.push(user_message.clone());
    conversation.updated_at = chrono::Local::now().timestamp();
    save_conversation(&app, &conversation)?;

    let selected_skill_id = active_skill_id
        .as_deref()
        .or(conversation.active_skill_id.as_deref())
        .map(str::to_string);

    match complete_assistant_reply(
        &app,
        &state,
        &mut conversation,
        Some(title_source.as_str()),
        Some(api_content.as_str()),
        &last_user_image_paths,
        selected_skill_id.as_deref(),
    )
    .await
    {
        Ok(()) => Ok(serde_json::json!({
            "success": true,
            "conversation": conversation,
        })),
        Err(err) if err == "cancelled" => Ok(serde_json::json!({
            "success": true,
            "conversation": conversation,
        })),
        Err(err) => {
            conversation
                .messages
                .retain(|message| message.id != user_message.id);
            conversation.updated_at = chrono::Local::now().timestamp();
            save_conversation(&app, &conversation)?;
            Ok(serde_json::json!({
                "success": false,
                "conversation": conversation,
                "error": err,
            }))
        }
    }
}

/// 取消指定对话的当前 Chat 生成或工具执行。
#[tauri::command]
pub(crate) fn chat_cancel_stream(
    state: State<AppState>,
    conversation_id: String,
) -> Result<(), String> {
    state.cancel_chat_generation(&conversation_id);
    Ok(())
}

/// 响应敏感工具调用确认。
#[tauri::command]
pub(crate) fn chat_confirm_tool_call(
    state: State<AppState>,
    tool_call_id: String,
    approved: bool,
) -> Result<(), String> {
    let sender = state
        .pending_chat_tool_approvals
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&tool_call_id);
    if let Some(sender) = sender {
        let _ = sender.send(approved);
    }
    Ok(())
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
    active_skill_id: Option<&str>,
) -> Result<(), String> {
    let settings = state.settings_read().clone();
    let provider = settings
        .get_provider(&conversation.provider_id)
        .ok_or_else(|| "Chat provider not found".to_string())?
        .clone();
    let provider_is_apple = provider.base_url == APPLE_INTELLIGENCE_BASE_URL;
    if !provider_is_apple && provider.api_keys.is_empty() {
        return Err("Missing API Key".to_string());
    }
    if conversation.model.trim().is_empty() {
        return Err("Please select a model first".to_string());
    }

    let last_user_idx = conversation.messages.iter().rposition(|m| m.role == "user");
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
        1
    };
    let run_generation = state.next_chat_generation(&conversation.id);
    let run_id = format!("chat-run-{}-{}", run_generation, Uuid::new_v4());
    let assistant_message_id = format!("msg_{}", Uuid::new_v4());
    let skill_id = active_skill_id.filter(|id| !id.trim().is_empty());
    let skill_registry =
        skills::build_registry(app, &settings.chat_tools.skill_scan_paths).unwrap_or_default();
    let active_skill_record = skill_id
        .and_then(|id| skill_registry.find(id))
        .cloned();
    let active_skill_detail = skill_id.and_then(|id| {
        skills::read_skill_detail(app, &settings.chat_tools.skill_scan_paths, id).ok()
    });
    let mut effective_chat_tools = settings.chat_tools.clone();
    let tools_capable = chat_tools_capable(&provider, &effective_chat_tools);
    if !tools_capable && effective_chat_tools.skill_fallback_mode == "progressive" {
        if skill_id.is_some() {
            effective_chat_tools.skill_fallback_mode = "skill_md_only".to_string();
        }
    }
    let system_prompt = build_chat_system_prompt(
        &language,
        !last_user_image_paths.is_empty(),
        thinking_enabled,
        &skill_registry,
        &effective_chat_tools,
        tools_capable,
        skill_id,
        active_skill_detail.as_ref(),
    );

    if provider_is_apple {
        if !last_user_image_paths.is_empty() {
            return Err(
                "Apple Intelligence 暂不支持图片附件，请为 AI 对话配置云端视觉 provider".into(),
            );
        }
        let prompt = build_apple_chat_prompt(
            &system_prompt,
            conversation,
            last_user_idx,
            last_user_api_content,
        );
        let response = tokio::select! {
            result = state.apple_intelligence.call_text(&prompt) => result?,
            _ = wait_for_chat_cancel(state.inner(), &conversation.id, run_generation) => {
                emit_chat_stream_done(
                    app,
                    &conversation.id,
                    &run_id,
                    &assistant_message_id,
                    "cancelled",
                    "",
                );
                return Err("cancelled".to_string());
            }
        };
        emit_chat_stream_delta(
            app,
            &conversation.id,
            &run_id,
            &assistant_message_id,
            &response,
            None,
        );
        emit_chat_stream_done(
            app,
            &conversation.id,
            &run_id,
            &assistant_message_id,
            "done",
            &response,
        );
        push_assistant_message(
            app,
            conversation,
            assistant_message_id,
            response,
            None,
            Vec::new(),
            skill_id,
            title_from_first_user,
        )?;
        return Ok(());
    }

    let mut runtime_messages = build_chat_api_messages(
        &system_prompt,
        conversation,
        last_user_idx,
        last_user_api_content,
        last_user_image_paths,
    )?;
    let mut tool_records = Vec::new();
    let mut planning_reasoning_parts: Vec<String> = Vec::new();
    let mut tools = if provider.supports_tools
        && (settings.chat_tools.enabled
            || settings.chat_tools.native_tools.web_search
            || settings.chat_tools.native_tools.skill_runtime)
    {
        let tools = mcp::registry::list_enabled_tool_defs(state.inner())
            .await
            .unwrap_or_default();
        tools
    } else {
        Vec::new()
    };
    if let Some(skill) = active_skill_record.as_ref() {
        if !skill.allowed_tools.is_empty() {
            tools.retain(|tool| {
                tool.source == "skill"
                    || skill
                        .allowed_tools
                        .iter()
                        .any(|recommended| tool_matches_recommended_name(tool, recommended))
            });
        }
    }
    let max_rounds = settings.chat_tools.max_tool_rounds.max(1);

    if !tools.is_empty() {
        let mut finalize_after_tool_loop = false;
        let mut provider_tools_unsupported = false;
        let mut skill_cache = skills::SkillRunCache::default();
        for round in 0..max_rounds {
            if !state.is_chat_generation_active(&conversation.id, run_generation) {
                emit_chat_stream_done(
                    app,
                    &conversation.id,
                    &run_id,
                    &assistant_message_id,
                    "cancelled",
                    "",
                );
                if !tool_records.is_empty() {
                    push_assistant_message(
                        app,
                        conversation,
                        assistant_message_id,
                        String::new(),
                        None,
                        tool_records,
                        skill_id,
                        title_from_first_user,
                    )?;
                }
                return Err("cancelled".to_string());
            }
            let planning_result = tokio::select! {
                result = call_chat_completion_message(
                    state,
                    &provider,
                    &conversation.model,
                    runtime_messages.clone(),
                    Some(&tools),
                    retry_attempts,
                    thinking_enabled,
                    "Chat tools planning",
                ) => result,
                _ = wait_for_chat_cancel(state.inner(), &conversation.id, run_generation) => {
                    emit_chat_stream_done(
                        app,
                        &conversation.id,
                        &run_id,
                        &assistant_message_id,
                        "cancelled",
                        "",
                    );
                    if !tool_records.is_empty() {
                        push_assistant_message(
                            app,
                            conversation,
                            assistant_message_id,
                            String::new(),
                            None,
                            tool_records,
                            skill_id,
                            title_from_first_user,
                        )?;
                    }
                    return Err("cancelled".to_string());
                }
            };
            let message = match planning_result {
                Ok(message) => message,
                Err(err) if is_tools_unsupported_error(&err) => {
                    eprintln!(
                        "Chat provider {} rejected tools; falling back to skill-only plain chat",
                        provider.id
                    );
                    provider_tools_unsupported = true;
                    break;
                }
                Err(err) => return Err(err),
            };
            if let Some(reasoning) = extract_reasoning_content(&message) {
                emit_chat_stream_delta(
                    app,
                    &conversation.id,
                    &run_id,
                    &assistant_message_id,
                    "",
                    Some(&reasoning),
                );
                planning_reasoning_parts.push(reasoning);
            }
            let tool_calls = extract_tool_calls(&message);
            if tool_calls.is_empty() {
                finalize_after_tool_loop = true;
                break;
            }

            runtime_messages.push(message);
            for tool_call in tool_calls {
                let Some(tool) = match_tool_call(&tools, &tool_call.function_name) else {
                    let error = format!("Unknown tool requested: {}", tool_call.function_name);
                    let record = unknown_tool_record(&tool_call, round + 1, error);
                    emit_chat_tool_record(
                        app,
                        &conversation.id,
                        &run_id,
                        &assistant_message_id,
                        &record,
                    );
                    runtime_messages.push(serde_json::json!({
                        "role": "tool",
                        "tool_call_id": tool_call.id,
                        "content": record.error.clone().unwrap_or_default(),
                    }));
                    tool_records.push(record);
                    continue;
                };
                let tool_call_id = tool_call.id.clone();
                let record = execute_chat_tool_call(
                    app,
                    state.inner(),
                    &conversation.id,
                    &run_id,
                    &assistant_message_id,
                    run_generation,
                    round + 1,
                    tool,
                    tool_call,
                    &mut skill_cache,
                )
                .await;
                runtime_messages.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": tool_call_id,
                    "content": record.result_preview.clone().or(record.error.clone()).unwrap_or_default(),
                }));
                tool_records.push(record);
            }
        }
        if !finalize_after_tool_loop && !provider_tools_unsupported {
            let message = "工具调用达到最大轮次，已停止。".to_string();
            emit_chat_stream_done(
                app,
                &conversation.id,
                &run_id,
                &assistant_message_id,
                "error",
                &message,
            );
            push_assistant_message(
                app,
                conversation,
                assistant_message_id,
                message,
                None,
                tool_records,
                skill_id,
                title_from_first_user,
            )?;
            return Ok(());
        }
    }

    let (response, reasoning) = if stream_enabled {
        let stream = stream_scoped_chat_completion(
            app,
            state,
            &provider,
            &conversation.model,
            runtime_messages,
            retry_attempts,
            thinking_enabled,
            &conversation.id,
            &run_id,
            &assistant_message_id,
            run_generation,
        )
        .await?;
        if stream.cancelled {
            if !tool_records.is_empty() {
                push_assistant_message(
                    app,
                    conversation,
                    assistant_message_id,
                    if stream.content.trim().is_empty() {
                        "已停止生成。".to_string()
                    } else {
                        stream.content.clone()
                    },
                    merge_reasoning(&planning_reasoning_parts, stream.reasoning),
                    tool_records,
                    skill_id,
                    title_from_first_user,
                )?;
            }
            return Err("cancelled".to_string());
        }
        (
            stream.content,
            merge_reasoning(&planning_reasoning_parts, stream.reasoning),
        )
    } else {
        let message = tokio::select! {
            result = call_chat_completion_message(
                state,
                &provider,
                &conversation.model,
                runtime_messages,
                None,
                retry_attempts,
                thinking_enabled,
                "Chat API",
            ) => result?,
            _ = wait_for_chat_cancel(state.inner(), &conversation.id, run_generation) => {
                emit_chat_stream_done(
                    app,
                    &conversation.id,
                    &run_id,
                    &assistant_message_id,
                    "cancelled",
                    "",
                );
                return Err("cancelled".to_string());
            }
        };
        let response = message
            .get("content")
            .and_then(|content| content.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        let reasoning = merge_reasoning(
            &planning_reasoning_parts,
            extract_reasoning_content(&message),
        );
        emit_chat_stream_delta(
            app,
            &conversation.id,
            &run_id,
            &assistant_message_id,
            &response,
            None,
        );
        emit_chat_stream_done(
            app,
            &conversation.id,
            &run_id,
            &assistant_message_id,
            "done",
            &response,
        );
        (response, reasoning)
    };

    push_assistant_message(
        app,
        conversation,
        assistant_message_id,
        response,
        reasoning,
        tool_records,
        skill_id,
        title_from_first_user,
    )?;
    Ok(())
}

fn push_assistant_message(
    app: &AppHandle,
    conversation: &mut Conversation,
    message_id: String,
    content: String,
    reasoning: Option<String>,
    tool_calls: Vec<ToolCallRecord>,
    active_skill_id: Option<&str>,
    title_from_first_user: Option<&str>,
) -> Result<(), String> {
    conversation.messages.push(ChatMessage {
        id: message_id,
        role: "assistant".to_string(),
        content,
        attachments: vec![],
        reasoning,
        tool_calls,
        active_skill_id: active_skill_id.map(|id| id.to_string()),
        timestamp: chrono::Local::now().timestamp(),
    });

    if let Some(user_content) = title_from_first_user {
        if conversation.messages.len() == 2 && conversation.title == "新对话" {
            conversation.title = generate_title(user_content);
        }
    }

    conversation.updated_at = chrono::Local::now().timestamp();
    save_conversation(app, conversation)?;
    Ok(())
}

fn chat_tools_capable(
    provider: &crate::settings::ModelProvider,
    chat_tools: &crate::settings::ChatToolsConfig,
) -> bool {
    provider.supports_tools
        && (chat_tools.enabled
            || chat_tools.native_tools.web_search
            || chat_tools.native_tools.skill_runtime)
}

fn build_chat_system_prompt(
    language: &str,
    has_image: bool,
    thinking_enabled: bool,
    registry: &skills::SkillRegistry,
    chat_tools: &crate::settings::ChatToolsConfig,
    tools_available: bool,
    active_skill_id: Option<&str>,
    active_skill_detail: Option<&skills::SkillDetail>,
) -> String {
    let mut prompt = default_system_prompt(language, has_image);

    let include_catalog = chat_tools.skill_auto_match
        || active_skill_id.is_some()
        || chat_tools.skill_fallback_mode != "legacy_full_body";
    if include_catalog {
        let catalog = skills::format_catalog(registry, active_skill_id, tools_available);
        if !catalog.is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(&catalog);
        }
    }

    if !chat_tools.skill_auto_match {
        prompt.push_str("\n\nOnly activate a skill when the user explicitly selected one in the UI.");
    }

    let fallback = chat_tools.skill_fallback_mode.as_str();
    if let Some(skill_id) = active_skill_id.filter(|id| !id.trim().is_empty()) {
        prompt.push_str("\n\nUser explicitly selected skill: ");
        prompt.push_str(skill_id);
        if tools_available {
            prompt.push_str(". Activate it with skill_activate before proceeding.");
        } else if matches!(fallback, "skill_md_only" | "legacy_full_body") {
            prompt.push_str(". Follow the Active Skill instructions below.");
        } else {
            prompt.push_str(
                ". Progressive skill loading requires tool support; switch provider or set fallback to SKILL.md only.",
            );
        }
    }

    if matches!(fallback, "skill_md_only" | "legacy_full_body") {
        if let Some(skill) = active_skill_detail {
            if !skill.body.trim().is_empty() {
                prompt.push_str("\n\nActive Skill:\n");
                prompt.push_str(&skill.body);
            }
        }
    }

    if !thinking_enabled {
        prompt.push_str(no_think_instruction(language));
    }
    prompt
}

fn tool_matches_recommended_name(tool: &ChatToolDefinition, recommended: &str) -> bool {
    let recommended = recommended.trim();
    if recommended.is_empty() {
        return false;
    }
    tool.name == recommended
        || tool.id == recommended
        || tool.openai_tool_name() == recommended
        || tool
            .server_id
            .as_deref()
            .map(|server_id| format!("{server_id}:{}", tool.name) == recommended)
            .unwrap_or(false)
}

fn build_chat_api_messages(
    system_prompt: &str,
    conversation: &Conversation,
    last_user_idx: Option<usize>,
    last_user_api_content: Option<&str>,
    last_user_image_paths: &[PathBuf],
) -> Result<Vec<Value>, String> {
    let mut messages = vec![serde_json::json!({
        "role": "system",
        "content": system_prompt,
    })];

    for (idx, message) in conversation.messages.iter().enumerate() {
        let content = if Some(idx) == last_user_idx {
            last_user_api_content.unwrap_or(message.content.as_str())
        } else {
            message.content.as_str()
        };
        if Some(idx) == last_user_idx && !last_user_image_paths.is_empty() {
            let mut parts = last_user_image_paths
                .iter()
                .map(image_content_part)
                .collect::<Result<Vec<_>, _>>()?;
            parts.push(serde_json::json!({ "type": "text", "text": content }));
            messages.push(serde_json::json!({
                "role": message.role,
                "content": parts,
            }));
        } else {
            messages.push(serde_json::json!({
                "role": message.role,
                "content": content,
            }));
        }
    }

    Ok(messages)
}

fn build_apple_chat_prompt(
    system_prompt: &str,
    conversation: &Conversation,
    last_user_idx: Option<usize>,
    last_user_api_content: Option<&str>,
) -> String {
    let mut parts = Vec::new();
    if !system_prompt.trim().is_empty() {
        parts.push(format!("System:\n{}", system_prompt.trim()));
    }
    for (idx, message) in conversation.messages.iter().enumerate() {
        let role = match message.role.as_str() {
            "assistant" => "Assistant",
            _ => "User",
        };
        let content = if Some(idx) == last_user_idx {
            last_user_api_content.unwrap_or(message.content.as_str())
        } else {
            message.content.as_str()
        };
        if !content.trim().is_empty() {
            parts.push(format!("{role}:\n{}", content.trim()));
        }
    }
    parts.push("Assistant:".to_string());
    parts.join("\n\n")
}

async fn call_chat_completion_message(
    state: &State<'_, AppState>,
    provider: &crate::settings::ModelProvider,
    model: &str,
    messages: Vec<Value>,
    tools: Option<&[ChatToolDefinition]>,
    retry_attempts: usize,
    thinking_enabled: bool,
    label: &str,
) -> Result<Value, String> {
    let url = format!(
        "{}/chat/completions",
        provider.base_url.trim_end_matches('/')
    );
    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "temperature": 0.7,
        "max_tokens": 2000,
    });
    if let Some(tools) = tools.filter(|tools| !tools.is_empty()) {
        body["tools"] = Value::Array(
            tools
                .iter()
                .map(ChatToolDefinition::to_openai_tool)
                .collect(),
        );
        body["tool_choice"] = serde_json::json!("auto");
    }
    if !thinking_enabled {
        body["thinking"] = serde_json::json!({ "type": "disabled" });
    }

    let response = send_with_failover(
        state,
        label,
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
        .map_err(|err| format!("{label} read body: {err}"))?;
    let value: Value = serde_json::from_str(&raw).map_err(|err| {
        format!(
            "{label} parse JSON: {} (body: {})",
            err,
            raw.chars().take(500).collect::<String>()
        )
    })?;
    value
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .cloned()
        .ok_or_else(|| {
            format!(
                "Invalid {label} response: {}",
                raw.chars().take(500).collect::<String>()
            )
        })
}

#[allow(clippy::too_many_arguments)]
async fn stream_scoped_chat_completion(
    app: &AppHandle,
    state: &State<'_, AppState>,
    provider: &crate::settings::ModelProvider,
    model: &str,
    messages: Vec<Value>,
    retry_attempts: usize,
    thinking_enabled: bool,
    conversation_id: &str,
    run_id: &str,
    message_id: &str,
    generation: u64,
) -> Result<ChatStreamOutput, String> {
    let url = format!(
        "{}/chat/completions",
        provider.base_url.trim_end_matches('/')
    );
    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "temperature": 0.7,
        "max_tokens": 2000,
        "stream": true,
    });
    if !thinking_enabled {
        body["thinking"] = serde_json::json!({ "type": "disabled" });
    }
    let mut response = send_with_failover(
        state,
        "Chat stream",
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
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!(
            "Chat stream HTTP {}: {}",
            status.as_u16(),
            text.chars().take(500).collect::<String>()
        ));
    }

    let mut buffer = String::new();
    let mut full = String::new();
    let mut reasoning_full = String::new();
    loop {
        if !state.is_chat_generation_active(conversation_id, generation) {
            emit_chat_stream_done(
                app,
                conversation_id,
                run_id,
                message_id,
                "cancelled",
                full.trim(),
            );
            return Ok(ChatStreamOutput::new(
                full.trim().to_string(),
                reasoning_full.trim().to_string(),
                true,
            ));
        }
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(err) => {
                emit_chat_stream_done(
                    app,
                    conversation_id,
                    run_id,
                    message_id,
                    "error",
                    full.trim(),
                );
                return Err(err.to_string());
            }
        };
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = buffer.find('\n') {
            let line: String = buffer.drain(..=pos).collect();
            let line = line.trim();
            if !line.starts_with("data:") {
                continue;
            }
            let data = line.trim_start_matches("data:").trim();
            if data.is_empty() {
                continue;
            }
            if data == "[DONE]" {
                emit_chat_stream_done(
                    app,
                    conversation_id,
                    run_id,
                    message_id,
                    "done",
                    full.trim(),
                );
                return Ok(ChatStreamOutput::new(
                    full.trim().to_string(),
                    reasoning_full.trim().to_string(),
                    false,
                ));
            }
            let value: Value = match serde_json::from_str(data) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let delta = value
                .get("choices")
                .and_then(|choices| choices.get(0))
                .and_then(|choice| choice.get("delta"));
            if let Some(reasoning) = delta
                .and_then(|delta| {
                    delta
                        .get("reasoning_content")
                        .or_else(|| delta.get("reasoning"))
                })
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
            {
                reasoning_full.push_str(reasoning);
                emit_chat_stream_delta(
                    app,
                    conversation_id,
                    run_id,
                    message_id,
                    "",
                    Some(reasoning),
                );
            }
            if let Some(content) = delta
                .and_then(|delta| delta.get("content"))
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
            {
                full.push_str(content);
                emit_chat_stream_delta(app, conversation_id, run_id, message_id, content, None);
            }
        }
    }
    emit_chat_stream_done(
        app,
        conversation_id,
        run_id,
        message_id,
        "done",
        full.trim(),
    );
    Ok(ChatStreamOutput::new(
        full.trim().to_string(),
        reasoning_full.trim().to_string(),
        false,
    ))
}

struct ChatStreamOutput {
    content: String,
    reasoning: Option<String>,
    cancelled: bool,
}

impl ChatStreamOutput {
    fn new(content: String, reasoning: String, cancelled: bool) -> Self {
        Self {
            content,
            reasoning: if reasoning.trim().is_empty() {
                None
            } else {
                Some(reasoning)
            },
            cancelled,
        }
    }
}

fn extract_reasoning_content(message: &Value) -> Option<String> {
    message
        .get("reasoning_content")
        .or_else(|| message.get("reasoning"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn merge_reasoning(planning_parts: &[String], final_reasoning: Option<String>) -> Option<String> {
    let mut parts = planning_parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if let Some(reasoning) = final_reasoning
        .as_deref()
        .map(str::trim)
        .filter(|reasoning| !reasoning.is_empty())
    {
        parts.push(reasoning.to_string());
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

#[derive(Debug, Clone)]
struct PendingToolCall {
    id: String,
    function_name: String,
    arguments: Value,
    arguments_raw: String,
}

fn extract_tool_calls(message: &Value) -> Vec<PendingToolCall> {
    message
        .get("tool_calls")
        .and_then(|value| value.as_array())
        .map(|calls| {
            calls
                .iter()
                .filter_map(|call| {
                    let function = call.get("function")?;
                    let name = function.get("name")?.as_str()?.to_string();
                    let arguments_raw = function
                        .get("arguments")
                        .and_then(|value| value.as_str())
                        .unwrap_or("{}")
                        .to_string();
                    let arguments = serde_json::from_str(&arguments_raw).unwrap_or(Value::Null);
                    Some(PendingToolCall {
                        id: call
                            .get("id")
                            .and_then(|value| value.as_str())
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| format!("tool_{}", Uuid::new_v4())),
                        function_name: name,
                        arguments,
                        arguments_raw,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn match_tool_call<'a>(
    tools: &'a [ChatToolDefinition],
    function_name: &str,
) -> Option<&'a ChatToolDefinition> {
    tools
        .iter()
        .find(|tool| tool.openai_tool_name() == function_name || tool.name == function_name)
}

fn is_tools_unsupported_error(err: &str) -> bool {
    if !matches!(extract_status_code(err), Some(400 | 404 | 422)) {
        return false;
    }
    let lower = err.to_ascii_lowercase();
    lower.contains("tools")
        || lower.contains("tool_choice")
        || lower.contains("tool_calls")
        || lower.contains("function calling")
        || lower.contains("function_call")
}

fn unknown_tool_record(call: &PendingToolCall, round: u8, error: String) -> ToolCallRecord {
    let now = chrono::Local::now().timestamp();
    ToolCallRecord {
        id: call.id.clone(),
        name: call.function_name.clone(),
        source: "unknown".to_string(),
        server_id: None,
        arguments: call.arguments_raw.clone(),
        status: ToolCallStatus::Error,
        result_preview: None,
        error: Some(error),
        duration_ms: Some(0),
        started_at: Some(now),
        completed_at: Some(now),
        round,
        sensitive: false,
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute_chat_tool_call(
    app: &AppHandle,
    state: &AppState,
    conversation_id: &str,
    run_id: &str,
    message_id: &str,
    generation: u64,
    round: u8,
    tool: &ChatToolDefinition,
    call: PendingToolCall,
    skill_cache: &mut skills::SkillRunCache,
) -> ToolCallRecord {
    let now = chrono::Local::now().timestamp();
    let mut record = ToolCallRecord {
        id: call.id.clone(),
        name: tool.name.clone(),
        source: tool.source.clone(),
        server_id: tool.server_id.clone(),
        arguments: call.arguments_raw.clone(),
        status: ToolCallStatus::Pending,
        result_preview: None,
        error: None,
        duration_ms: None,
        started_at: Some(now),
        completed_at: None,
        round,
        sensitive: tool.sensitive,
    };
    emit_chat_tool_record(app, conversation_id, run_id, message_id, &record);

    let settings = state.settings_read().clone();
    let requires_approval = match settings.chat_tools.approval_policy.as_str() {
        "auto" => false,
        "always_confirm" => true,
        _ => tool.sensitive,
    };
    if requires_approval {
        let approved = request_tool_approval(
            app,
            state,
            conversation_id,
            run_id,
            message_id,
            generation,
            &record,
        )
        .await;
        if !approved {
            record.status = ToolCallStatus::Skipped;
            record.completed_at = Some(chrono::Local::now().timestamp());
            record.error = Some("Tool call was not approved".to_string());
            emit_chat_tool_record(app, conversation_id, run_id, message_id, &record);
            return record;
        }
    }

    record.status = ToolCallStatus::Running;
    emit_chat_tool_record(app, conversation_id, run_id, message_id, &record);
    let started = Instant::now();
    let timeout_ms = settings.chat_tools.tool_timeout_ms;
    let result = tokio::select! {
        result = timeout(
            Duration::from_millis(timeout_ms),
            mcp::registry::call_tool(app, state, tool, call.arguments.clone(), Some(skill_cache)),
        ) => result,
        _ = wait_for_chat_cancel(state, conversation_id, generation) => {
            record.status = ToolCallStatus::Cancelled;
            record.duration_ms = Some(started.elapsed().as_millis() as u64);
            record.completed_at = Some(chrono::Local::now().timestamp());
            record.error = Some("Tool call cancelled".to_string());
            emit_chat_tool_record(app, conversation_id, run_id, message_id, &record);
            return record;
        }
    };
    record.duration_ms = Some(started.elapsed().as_millis() as u64);
    record.completed_at = Some(chrono::Local::now().timestamp());
    match result {
        Ok(Ok(output)) if !output.is_error => {
            record.status = ToolCallStatus::Success;
            record.result_preview = Some(truncate_chars(
                &output.content,
                settings.chat_tools.max_tool_output_chars,
            ));
        }
        Ok(Ok(output)) => {
            record.status = ToolCallStatus::Error;
            record.error = Some(truncate_chars(&output.content, 1000));
        }
        Ok(Err(err)) => {
            record.status = ToolCallStatus::Error;
            record.error = Some(err);
        }
        Err(_) => {
            record.status = ToolCallStatus::Error;
            record.error = Some("Tool call timed out".to_string());
        }
    }
    emit_chat_tool_record(app, conversation_id, run_id, message_id, &record);
    record
}

async fn request_tool_approval(
    app: &AppHandle,
    state: &AppState,
    conversation_id: &str,
    run_id: &str,
    message_id: &str,
    generation: u64,
    record: &ToolCallRecord,
) -> bool {
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut pending = state
            .pending_chat_tool_approvals
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        pending.insert(record.id.clone(), tx);
    }
    let _ = app.emit(
        "chat-tool-confirm",
        serde_json::json!({
            "conversationId": conversation_id,
            "runId": run_id,
            "messageId": message_id,
            "toolCallId": record.id,
            "name": record.name,
            "source": record.source,
            "serverId": record.server_id,
            "argumentsPreview": truncate_chars(&record.arguments, 800),
            "sensitivity": "sensitive",
        }),
    );
    let result = tokio::select! {
        result = timeout(Duration::from_secs(60), rx) => result,
        _ = wait_for_chat_cancel(state, conversation_id, generation) => {
            let mut pending = state
                .pending_chat_tool_approvals
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            pending.remove(&record.id);
            return false;
        }
    };
    match result {
        Ok(Ok(value)) => value,
        _ => {
            let mut pending = state
                .pending_chat_tool_approvals
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            pending.remove(&record.id);
            false
        }
    }
}

async fn wait_for_chat_cancel(state: &AppState, conversation_id: &str, generation: u64) {
    while state.is_chat_generation_active(conversation_id, generation) {
        sleep(Duration::from_millis(100)).await;
    }
}

fn emit_chat_tool_record(
    app: &AppHandle,
    conversation_id: &str,
    run_id: &str,
    message_id: &str,
    record: &ToolCallRecord,
) {
    let _ = app.emit(
        "chat-tool",
        serde_json::json!({
            "conversationId": conversation_id,
            "runId": run_id,
            "messageId": message_id,
            "toolCallId": record.id,
            "id": record.id,
            "name": record.name,
            "source": record.source,
            "serverId": record.server_id,
            "status": record.status,
            "argumentsPreview": truncate_chars(&record.arguments, 800),
            "resultPreview": record.result_preview,
            "error": record.error,
            "startedAt": record.started_at,
            "completedAt": record.completed_at,
            "durationMs": record.duration_ms,
            "round": record.round,
            "sensitive": record.sensitive,
        }),
    );
}

fn emit_chat_stream_delta(
    app: &AppHandle,
    conversation_id: &str,
    run_id: &str,
    message_id: &str,
    delta: &str,
    reasoning_delta: Option<&str>,
) {
    let _ = app.emit(
        "chat-stream",
        serde_json::json!({
            "conversationId": conversation_id,
            "runId": run_id,
            "messageId": message_id,
            "imageId": "",
            "kind": "answer",
            "delta": delta,
            "reasoningDelta": reasoning_delta,
        }),
    );
}

fn emit_chat_stream_done(
    app: &AppHandle,
    conversation_id: &str,
    run_id: &str,
    message_id: &str,
    reason: &str,
    full: &str,
) {
    let _ = app.emit(
        "chat-stream",
        serde_json::json!({
            "conversationId": conversation_id,
            "runId": run_id,
            "messageId": message_id,
            "imageId": "",
            "kind": "answer",
            "delta": "",
            "done": true,
            "reason": reason,
            "full": full,
        }),
    );
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut out = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        out.push_str("...");
    }
    out
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
    let selected_skill_id = conversation.active_skill_id.clone();

    match complete_assistant_reply(
        &app,
        &state,
        &mut conversation,
        None,
        last_user_api_content.as_deref(),
        &last_user_image_paths,
        selected_skill_id.as_deref(),
    )
    .await
    {
        Ok(()) => Ok(serde_json::json!({
            "success": true,
            "conversation": conversation,
        })),
        Err(err) if err == "cancelled" => Ok(serde_json::json!({
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
    active_skill_id: Option<String>,
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
    if let Some(skill_id) = active_skill_id {
        let trimmed = skill_id.trim();
        conversation.active_skill_id = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
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
    fn merge_reasoning_keeps_planning_and_final_sections() {
        let reasoning = merge_reasoning(
            &[
                "  planning round one  ".to_string(),
                String::new(),
                "planning round two".to_string(),
            ],
            Some(" final answer reasoning ".to_string()),
        )
        .expect("reasoning should be merged");

        assert_eq!(
            reasoning,
            "planning round one\n\nplanning round two\n\nfinal answer reasoning"
        );
    }

    #[test]
    fn generate_title_truncates_unicode_safely() {
        let title = generate_title("附件: 这是一张非常非常非常非常非常非常非常长的图片文件名.png");

        assert!(title.ends_with("..."));
        assert!(title.chars().count() <= 33);
    }
}
