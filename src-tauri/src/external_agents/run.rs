use std::collections::HashMap;
use std::time::Instant;

use chrono::Local;
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::chat::agent::AgentRunEntry;
use crate::chat::commands::{
    emit_chat_stream_delta, emit_chat_stream_done, emit_chat_tool_record, push_assistant_message,
};
use crate::chat::memory::l1_prompt_block;
use crate::chat::model::ModelUsage;
use crate::chat::storage::save_conversation;
use crate::chat::types::{
    ChatMessageSegment, ChatMessageSegmentKind, ChatMessageSegmentPhase, ToolCallRecord,
    ToolCallStatus,
};
use crate::chat::Conversation;
use crate::external_agents::detection::detect_single_agent;
use crate::external_agents::mcp_inject::apply_mcp_injection;
use crate::external_agents::prompt::{compose_external_prompt, cwd_hint};
use crate::external_agents::registry::get_agent_def;
use crate::external_agents::session::{persist_delivered_session, resolve_agent_resume_context};
use crate::external_agents::skill_stage::{skill_cwd_alias_segment, stage_active_skill};
use crate::external_agents::spawn::{read_stdout_lines, resolve_binary, spawn_agent, write_prompt_stdin};
use crate::external_agents::stream::create_stream_handler;
use crate::external_agents::types::{
    RuntimeBuildOptions, RuntimeContext, UnifiedAgentEvent,
};
use crate::external_agents::workspace::{
    can_write_mcp_json, extra_allowed_dirs_for_agent, resolve_effective_cwd,
};
use crate::skills::read_skill_detail;
use crate::state::AppState;

pub struct ExternalCliRunOutcome {
    pub stream_outcome: String,
}

pub async fn run_external_cli_reply(
    app: &AppHandle,
    state: &State<'_, AppState>,
    conversation: &mut Conversation,
    title_from_first_user: Option<&str>,
    latest_user_message: &str,
    active_skill_id: Option<&str>,
    entry: AgentRunEntry,
) -> Result<(), String> {
    let settings = state.settings_read().clone();
    let agent_id = conversation
        .agent_runtime
        .external_agent_id
        .clone()
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| "未选择外部 Agent".to_string())?;

    let def = get_agent_def(&agent_id).ok_or_else(|| format!("未知外部 Agent: {agent_id}"))?;

    let detected = detect_single_agent(def).await;
    if !detected.available {
        return Err(format!(
            "{} 未安装或不可用，请确认 CLI 在 PATH 中。",
            def.name
        ));
    }

    let resolved_bin = resolve_binary(def).await.ok_or_else(|| {
        format!("无法定位 {} 可执行文件", def.bin)
    })?;

    let workspace = resolve_effective_cwd(
        app,
        &conversation.id,
        conversation.project_id.as_deref(),
    )?;
    let cwd = workspace.cwd.clone();

    let skill_detail = if let Some(skill_id) = active_skill_id.filter(|s| !s.is_empty()) {
        read_skill_detail(app, &settings.chat_tools.skill_scan_paths, skill_id).ok()
    } else {
        None
    };

    let memory_body = if settings.chat_memory.enabled {
        l1_prompt_block(app).unwrap_or(None).unwrap_or_default()
    } else {
        String::new()
    };

    let mut daemon_instructions = String::new();
    if !settings.chat.system_prompt.trim().is_empty() {
        daemon_instructions.push_str(settings.chat.system_prompt.trim());
        daemon_instructions.push_str("\n\n");
    }
    if !memory_body.trim().is_empty() {
        daemon_instructions.push_str("## Memory\n\n");
        daemon_instructions.push_str(memory_body.trim());
        daemon_instructions.push('\n');
    }
    daemon_instructions.push_str(&cwd_hint(cwd.to_string_lossy().as_ref()));

    let resume_ctx = resolve_agent_resume_context(
        app,
        &conversation.id,
        def.id,
        def.resumes_session_via_cli,
        &daemon_instructions,
    );

    let skill_dir = skill_detail
        .as_ref()
        .and_then(|d| d.meta.path.clone());
    let skill_body = skill_detail.as_ref().map(|d| d.body.clone());
    let skill_folder = skill_dir.as_deref().map(skill_cwd_alias_segment);

    if let (Some(dir), Some(folder)) = (skill_dir.as_deref(), skill_folder.as_deref()) {
        let _ = stage_active_skill(&cwd, folder, std::path::Path::new(dir));
    }

    let composed = compose_external_prompt(
        conversation,
        &daemon_instructions,
        skill_body.as_deref(),
        skill_dir.as_deref(),
        skill_folder.as_deref(),
        resume_ctx.skip_instructions,
        resume_ctx.is_resuming,
        latest_user_message,
    );

    let can_write_mcp = can_write_mcp_json(&workspace, settings.chat.external_allow_mcp_in_project);
    apply_mcp_injection(
        def.external_mcp_injection,
        &cwd,
        &settings.chat_tools.servers,
        can_write_mcp,
    )?;

    let extra_dirs = extra_allowed_dirs_for_agent(def, &settings.chat_tools.skill_scan_paths);
    let runtime_ctx = RuntimeContext {
        cwd: Some(cwd.to_string_lossy().into_owned()),
        extra_allowed_dirs: extra_dirs,
        resume_session_id: resume_ctx.resume_session_id.clone(),
        new_session_id: resume_ctx.new_session_id.clone(),
        include_partial_messages: true,
    };

    let build_options = RuntimeBuildOptions {
        model: conversation.agent_runtime.external_model.clone(),
        reasoning: conversation.agent_runtime.external_reasoning.clone(),
    };

    let args = (def.build_args)(&runtime_ctx, &build_options);

    let run_generation = state.next_chat_generation(&conversation.id);
    let run_id = format!("ext-run-{}-{}", run_generation, Uuid::new_v4());
    let assistant_message_id = format!("msg_{}", Uuid::new_v4());

    let mut spawned = spawn_agent(def, &resolved_bin, &args, &cwd).await?;
    write_prompt_stdin(&mut spawned.child, def, &composed.full_prompt).await?;

    let mut handler = create_stream_handler(def.stream_format, def.json_event_parser);
    let mut content = String::new();
    let mut reasoning = String::new();
    let mut tool_calls: Vec<ToolCallRecord> = Vec::new();
    let mut tool_map: HashMap<String, usize> = HashMap::new();
    let mut usage: Option<ModelUsage> = None;
    let mut stream_outcome = "completed".to_string();
    let mut segment_order = 0u32;
    let mut segments: Vec<ChatMessageSegment> = Vec::new();
    let mut segment_tracker = StreamSegmentTracker::default();
    let conversation_id = conversation.id.clone();
    let started_at = Instant::now();

    let read_result = read_stdout_lines(
        &mut spawned.child,
        |line| {
            handler.handle_line(
                line,
                &mut |event| {
                    apply_unified_event(
                        app,
                        &conversation_id,
                        &run_id,
                        &assistant_message_id,
                        &mut content,
                        &mut reasoning,
                        &mut tool_calls,
                        &mut tool_map,
                        &mut usage,
                        &mut segments,
                        &mut segment_order,
                        &mut segment_tracker,
                        event,
                    );
                },
            );
            Ok(())
        },
        || !state.is_chat_generation_active(&conversation_id, run_generation),
    )
    .await;

    let status = spawned.child.wait().await.map_err(|e| e.to_string())?;
    if read_result.is_err() {
        stream_outcome = "cancelled".to_string();
    } else if !status.success() {
        if content.trim().is_empty() {
            stream_outcome = "error".to_string();
        }
    }

    if content.trim().is_empty() && stream_outcome == "completed" {
        stream_outcome = "error".to_string();
        content = format!(
            "{} 未产生输出（exit={:?}，耗时 {}ms）",
            def.name,
            status.code(),
            started_at.elapsed().as_millis()
        );
    }

    emit_chat_stream_done(
        app,
        &conversation_id,
        &run_id,
        &assistant_message_id,
        &stream_outcome,
        &content,
    );

    persist_delivered_session(
        app,
        &conversation_id,
        def.id,
        &resume_ctx,
        &composed.instructions_block,
    )?;

    push_assistant_message(
        app,
        state,
        &settings,
        conversation,
        assistant_message_id,
        content,
        if reasoning.is_empty() {
            None
        } else {
            Some(reasoning)
        },
        vec![],
        tool_calls,
        vec![],
        segments,
        active_skill_id,
        title_from_first_user,
        Some(match entry {
            AgentRunEntry::Send => "send",
            AgentRunEntry::Regenerate => "regenerate",
        }),
        Some(&stream_outcome),
        usage,
    )
    .await?;

    save_conversation(app, conversation)?;
    Ok(())
}

#[derive(Default)]
struct StreamSegmentTracker {
    active_text_idx: Option<usize>,
    active_reasoning_idx: Option<usize>,
}

impl StreamSegmentTracker {
    fn reset_text(&mut self) {
        self.active_text_idx = None;
    }

    fn reset_reasoning(&mut self) {
        self.active_reasoning_idx = None;
    }

    fn append_text(
        &mut self,
        segments: &mut Vec<ChatMessageSegment>,
        segment_order: &mut u32,
        tool_calls_len: usize,
        delta: &str,
    ) -> ChatMessageSegment {
        let phase = text_phase_for_tool_count(tool_calls_len);
        if let Some(idx) = self.active_text_idx {
            if let Some(segment) = segments.get_mut(idx) {
                if segment.kind == ChatMessageSegmentKind::Text && segment.phase == phase {
                    let merged = format!("{}{}", segment.text.as_deref().unwrap_or(""), delta);
                    segment.text = Some(merged);
                    return segment.clone();
                }
            }
        }

        *segment_order += 1;
        let segment = ChatMessageSegment {
            id: format!("seg_{}", Uuid::new_v4()),
            kind: ChatMessageSegmentKind::Text,
            phase,
            order: *segment_order,
            step_number: None,
            round: if tool_calls_len == 0 {
                None
            } else {
                Some(1)
            },
            text: Some(delta.to_string()),
            tool_call_id: None,
        };
        self.active_text_idx = Some(segments.len());
        segments.push(segment.clone());
        segment
    }

    fn append_reasoning(
        &mut self,
        segments: &mut Vec<ChatMessageSegment>,
        segment_order: &mut u32,
        tool_calls_len: usize,
        delta: &str,
    ) -> ChatMessageSegment {
        let phase = text_phase_for_tool_count(tool_calls_len);
        if let Some(idx) = self.active_reasoning_idx {
            if let Some(segment) = segments.get_mut(idx) {
                if segment.kind == ChatMessageSegmentKind::Reasoning && segment.phase == phase {
                    let merged = format!("{}{}", segment.text.as_deref().unwrap_or(""), delta);
                    segment.text = Some(merged);
                    return segment.clone();
                }
            }
        }

        *segment_order += 1;
        let segment = ChatMessageSegment {
            id: format!("seg_{}", Uuid::new_v4()),
            kind: ChatMessageSegmentKind::Reasoning,
            phase,
            order: *segment_order,
            step_number: None,
            round: if tool_calls_len == 0 {
                None
            } else {
                Some(1)
            },
            text: Some(delta.to_string()),
            tool_call_id: None,
        };
        self.active_reasoning_idx = Some(segments.len());
        segments.push(segment.clone());
        segment
    }
}

fn text_phase_for_tool_count(tool_calls_len: usize) -> ChatMessageSegmentPhase {
    if tool_calls_len == 0 {
        ChatMessageSegmentPhase::Plain
    } else {
        ChatMessageSegmentPhase::ToolLoop
    }
}

fn apply_unified_event(
    app: &AppHandle,
    conversation_id: &str,
    run_id: &str,
    message_id: &str,
    content: &mut String,
    reasoning: &mut String,
    tool_calls: &mut Vec<ToolCallRecord>,
    tool_map: &mut HashMap<String, usize>,
    usage: &mut Option<ModelUsage>,
    segments: &mut Vec<ChatMessageSegment>,
    segment_order: &mut u32,
    segment_tracker: &mut StreamSegmentTracker,
    event: UnifiedAgentEvent,
) {
    let now = Local::now().timestamp();
    match event {
        UnifiedAgentEvent::TextDelta { delta } => {
            content.push_str(&delta);
            let segment = segment_tracker.append_text(
                segments,
                segment_order,
                tool_calls.len(),
                &delta,
            );
            emit_chat_stream_delta(
                app,
                conversation_id,
                run_id,
                message_id,
                &delta,
                None,
                Some(&segment),
            );
        }
        UnifiedAgentEvent::ThinkingDelta { delta } => {
            reasoning.push_str(&delta);
            let segment = segment_tracker.append_reasoning(
                segments,
                segment_order,
                tool_calls.len(),
                &delta,
            );
            emit_chat_stream_delta(
                app,
                conversation_id,
                run_id,
                message_id,
                "",
                Some(&delta),
                Some(&segment),
            );
        }
        UnifiedAgentEvent::ToolUse { id, name, input } => {
            segment_tracker.reset_text();
            segment_tracker.reset_reasoning();
            let record = ToolCallRecord {
                id: id.clone(),
                name: name.clone(),
                source: "external_cli".to_string(),
                server_id: None,
                arguments: input.to_string(),
                status: ToolCallStatus::Running,
                result_preview: None,
                error: None,
                duration_ms: None,
                started_at: Some(now),
                completed_at: None,
                round: 1,
                sensitive: false,
                artifacts: vec![],
                trace_id: None,
                span_id: None,
                structured_content: Some(input),
            };
            tool_map.insert(id.clone(), tool_calls.len());
            tool_calls.push(record.clone());
            emit_chat_tool_record(app, conversation_id, run_id, message_id, &record);
        }
        UnifiedAgentEvent::ToolResult {
            tool_use_id,
            content: result_content,
            is_error,
        } => {
            if let Some(idx) = tool_map.get(&tool_use_id).copied() {
                if let Some(record) = tool_calls.get_mut(idx) {
                    record.status = if is_error {
                        ToolCallStatus::Error
                    } else {
                        ToolCallStatus::Success
                    };
                    record.result_preview = Some(truncate_for_preview(&result_content, 800));
                    record.completed_at = Some(now);
                    emit_chat_tool_record(app, conversation_id, run_id, message_id, record);
                }
            }
        }
        UnifiedAgentEvent::Usage { usage: u } => {
            *usage = Some(u);
        }
        UnifiedAgentEvent::Error { message, .. } => {
            eprintln!("[external-agent] stream error: {message}");
        }
        _ => {}
    }
}

fn truncate_for_preview(value: &str, max_chars: usize) -> String {
    let mut out: String = value.chars().take(max_chars).collect();
    if value.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_segment_tracker_reuses_text_segment_for_deltas() {
        let mut segments = Vec::new();
        let mut order = 0u32;
        let mut tracker = StreamSegmentTracker::default();

        let first = tracker.append_text(&mut segments, &mut order, 0, "你");
        let second = tracker.append_text(&mut segments, &mut order, 0, "好");

        assert_eq!(segments.len(), 1);
        assert_eq!(first.id, second.id);
        assert_eq!(segments[0].text.as_deref(), Some("你好"));
        assert_eq!(segments[0].phase, ChatMessageSegmentPhase::Plain);
    }

    #[test]
    fn stream_segment_tracker_starts_new_text_segment_after_tool_use() {
        let mut segments = Vec::new();
        let mut order = 0u32;
        let mut tracker = StreamSegmentTracker::default();

        tracker.append_text(&mut segments, &mut order, 0, "before");
        tracker.reset_text();
        let after = tracker.append_text(&mut segments, &mut order, 1, "after");

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text.as_deref(), Some("before"));
        assert_eq!(segments[1].text.as_deref(), Some("after"));
        assert_eq!(after.phase, ChatMessageSegmentPhase::ToolLoop);
    }
}
