#[cfg(test)]
use std::path::Path;

#[cfg(test)]
use crate::chat::agent::prepare as agent_prepare;
#[cfg(test)]
use crate::chat::attachments::compose_user_content_for_api;
#[cfg(test)]
use crate::chat::model::{
    openai_messages_from_model_messages, MessagePart, ModelMessage, ModelRole,
};
#[cfg(test)]
use crate::mcp::ChatToolDefinition;
#[cfg(test)]
use crate::settings::Settings;
#[cfg(test)]
use crate::settings::{ModelProvider, SessionModel};
#[cfg(test)]
use crate::skills;
#[cfg(test)]
use super::vision::AuxiliaryVisionResult;
use super::vision::image_content_part;
#[cfg(test)]
use super::vision::user_content_with_auxiliary_vision_result;
use super::storage::save_conversation;
use super::{
    AgentPlanState, ChatMessage, ChatMessageSegment, ChatMessageSegmentKind,
    ChatMessageSegmentPhase, Conversation, ToolCallRecord,
    ToolCallStatus,
};
#[cfg(test)]
use super::{AgentTodoState, CompactionBoundaryRecord, ConversationContextState};

mod agent_host;

pub(crate) mod attachments;

pub(crate) mod catalog;

pub(crate) use catalog::create_assistant_via_builder;
#[cfg(test)]
use catalog::strip_transcripts_for_frontend;
#[cfg(test)]
use catalog::assistant_from_builder_args;

pub(crate) mod context;

pub(crate) mod interaction;

mod title;

mod tooling;

mod messages;

mod sanitization;

mod reply_runtime;
use reply_runtime::{ChatSendReservation, CHAT_REPLY_BUSY_ERROR, MAX_REPLY_MODELS};
#[cfg(test)]
use reply_runtime::resolve_reply_arms;

mod fan_out;

pub(crate) mod send;

pub(crate) mod reasoning;
use reasoning::resolve_thinking;

mod vision_compat;
pub(crate) use vision_compat::{attach_image_artifacts_for_model, read_image_as_tool_result};

mod reply;
use reply::{agent_run_entry_label, complete_assistant_reply, complete_assistant_reply_inner};

mod direct_image;

#[cfg(debug_assertions)]
mod probe_runtime;
#[cfg(debug_assertions)]
pub(crate) use probe_runtime::run_chat_probe;

pub(crate) mod mutations;

pub(crate) use interaction::{
    emit_chat_stream_delta, emit_chat_stream_done, emit_chat_tool_record,
};
#[cfg(test)]
use title::generate_title;
pub(crate) use messages::push_assistant_message;
#[cfg(test)]
use mutations::{apply_regenerate_truncation, build_fork_messages};
#[cfg(test)]
use sanitization::sanitize_image_payloads_for_model;
#[cfg(test)]
use messages::build_assistant_message;
#[cfg(test)]
use messages::{
    assistant_model_messages_for_storage, build_error_arm_message, content_from_segments,
    normalize_assistant_segments,
    reasoning_from_segments, reconcile_orphan_tool_segments, replace_final_text_segments_for_edit,
};
use tooling::{
    append_agent_ask_user_tools, append_agent_todo_tools, apply_agent_plan_tool_filter,
    apply_inline_code_request_tool_filter, list_tools_for_chat, resolve_forced_skill_id,
};
#[cfg(test)]
use tooling::try_apply_skill_slash_trigger;
#[cfg(test)]
use tooling::should_answer_inline_without_file_write;
#[cfg(test)]
use title::{build_title_summary_prompt, sanitize_generated_title};

#[cfg(test)]
use interaction::{approve_agent_plan_for_execution, format_tool_approval_summary};

#[cfg(test)]
use context::{build_chat_api_messages, resolve_usage_anchor};
#[cfg(test)]
use context::{
    count_tokens_in_value, estimate_image_tokens_for_dimensions,
    group_answer_excluded_from_context, mark_summary_stale_if_needed,
    should_auto_compress_context,
};
#[cfg(test)]
use super::ConversationContextSummary;

#[cfg(test)]
mod tests;
