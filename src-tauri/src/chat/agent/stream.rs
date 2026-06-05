use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::chat::model::{GenerateOutput, ModelError, PendingToolCall, StreamPart, StreamSink};

use super::host::AgentHost;
use super::stop::{
    empty_assistant_response_error, final_assistant_api_message, pending_tool_calls_from_dsml,
    sanitize_assistant_text_response,
};
use super::types::AgentStreamPolicy;

#[derive(Default)]
struct ChatStreamAccumulator {
    content: String,
    reasoning: String,
}

struct ChatStreamSnapshot {
    content: String,
    reasoning: String,
}

fn chat_stream_snapshot(accumulator: &Arc<Mutex<ChatStreamAccumulator>>) -> ChatStreamSnapshot {
    let guard = accumulator.lock().unwrap_or_else(|err| err.into_inner());
    ChatStreamSnapshot {
        content: guard.content.clone(),
        reasoning: guard.reasoning.clone(),
    }
}

pub struct AgentStreamSink<'a> {
    host: &'a dyn AgentHost,
    conversation_id: String,
    run_id: String,
    message_id: String,
    accumulator: Arc<Mutex<ChatStreamAccumulator>>,
    buffer_tool_planning_text: bool,
    text_buffer: String,
    text_suppressed: bool,
}

impl<'a> AgentStreamSink<'a> {
    pub fn new(
        host: &'a dyn AgentHost,
        conversation_id: &str,
        run_id: &str,
        message_id: &str,
        buffer_tool_planning_text: bool,
    ) -> Self {
        Self {
            host,
            conversation_id: conversation_id.to_string(),
            run_id: run_id.to_string(),
            message_id: message_id.to_string(),
            accumulator: Arc::new(Mutex::new(ChatStreamAccumulator::default())),
            buffer_tool_planning_text,
            text_buffer: String::new(),
            text_suppressed: false,
        }
    }

    pub fn snapshot(&self) -> (String, String) {
        let snapshot = chat_stream_snapshot(&self.accumulator);
        (snapshot.content, snapshot.reasoning)
    }

    fn emit_text_delta(&self, delta: &str) {
        self.host.emit_stream_delta(
            &self.conversation_id,
            &self.run_id,
            &self.message_id,
            delta,
            None,
        );
    }

    fn handle_text_delta(&mut self, delta: String) {
        self.accumulator
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .content
            .push_str(&delta);

        if self.text_suppressed {
            return;
        }
        if !self.buffer_tool_planning_text {
            self.emit_text_delta(&delta);
            return;
        }

        self.text_buffer.push_str(&delta);
        if crate::chat::dsml_tools::contains_dsml_tool_markup(&self.text_buffer) {
            self.text_buffer.clear();
            self.text_suppressed = true;
            return;
        }
        if should_flush_tool_planning_text_buffer(&self.text_buffer) {
            self.flush_pending_text();
        }
    }

    pub fn flush_pending_text(&mut self) {
        if self.text_suppressed || self.text_buffer.is_empty() {
            return;
        }
        let delta = std::mem::take(&mut self.text_buffer);
        self.emit_text_delta(&delta);
    }
}

impl StreamSink for AgentStreamSink<'_> {
    fn emit(&mut self, part: StreamPart) -> Result<(), ModelError> {
        match part {
            StreamPart::TextDelta { delta } => {
                self.handle_text_delta(delta);
            }
            StreamPart::ReasoningDelta { delta } => {
                self.accumulator
                    .lock()
                    .unwrap_or_else(|err| err.into_inner())
                    .reasoning
                    .push_str(&delta);
                self.host.emit_stream_delta(
                    &self.conversation_id,
                    &self.run_id,
                    &self.message_id,
                    "",
                    Some(&delta),
                );
            }
            StreamPart::Error { message } => return Err(ModelError::new(message)),
            StreamPart::Finish { .. }
            | StreamPart::ToolCallStart { .. }
            | StreamPart::ToolCallDelta { .. }
            | StreamPart::ToolCallDone { .. }
            | StreamPart::ToolResult { .. } => {}
        }
        Ok(())
    }
}

pub fn should_flush_tool_planning_text_buffer(buffer: &str) -> bool {
    let trimmed = buffer.trim_start();
    if trimmed.starts_with('<') && trimmed.len() < 64 {
        return false;
    }
    buffer.chars().count() >= 12 || buffer.contains('\n')
}

pub struct ChatStreamOutput {
    pub content: String,
    pub raw_content: String,
    pub reasoning: Option<String>,
    pub tool_calls: Vec<PendingToolCall>,
    pub finish_reason: Option<String>,
    pub cancelled: bool,
}

impl ChatStreamOutput {
    pub fn new(content: String, reasoning: String, cancelled: bool) -> Self {
        Self::from_generate_output(
            content.clone(),
            content,
            reasoning,
            Vec::new(),
            None,
            cancelled,
        )
    }

    pub fn from_generate_output(
        content: String,
        raw_content: String,
        reasoning: String,
        tool_calls: Vec<PendingToolCall>,
        finish_reason: Option<String>,
        cancelled: bool,
    ) -> Self {
        Self {
            content,
            raw_content,
            reasoning: if reasoning.trim().is_empty() {
                None
            } else {
                Some(reasoning)
            },
            tool_calls,
            finish_reason,
            cancelled,
        }
    }

    pub fn from_generate_output_with_snapshot(
        output: GenerateOutput,
        snapshot_content: String,
        snapshot_reasoning: String,
    ) -> Self {
        let raw_content = if output.text.trim().is_empty() {
            snapshot_content
        } else {
            output.text
        };
        let cleaned = sanitize_assistant_text_response(raw_content.trim());
        let reasoning = output.reasoning.unwrap_or(snapshot_reasoning);
        Self::from_generate_output(
            cleaned,
            raw_content,
            reasoning,
            output.tool_calls,
            output.finish_reason,
            false,
        )
    }

    pub fn to_openai_compatible_message(&self) -> Value {
        let content = if self.raw_content.trim().is_empty() {
            self.content.clone()
        } else {
            self.raw_content.clone()
        };
        let mut message = final_assistant_api_message(&content, self.reasoning.as_deref());
        if !self.tool_calls.is_empty() {
            message["tool_calls"] = Value::Array(
                self.tool_calls
                    .iter()
                    .map(|call| {
                        serde_json::json!({
                            "id": call.id,
                            "type": "function",
                            "function": {
                                "name": call.function_name,
                                "arguments": call.arguments_raw,
                            }
                        })
                    })
                    .collect(),
            );
        }
        if let Some(finish_reason) = self
            .finish_reason
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            message["finish_reason"] = Value::String(finish_reason.to_string());
        }
        message
    }
}

pub fn should_emit_done(policy: AgentStreamPolicy, output: &ChatStreamOutput) -> bool {
    match policy {
        AgentStreamPolicy::SynthesisAlwaysDone => true,
        AgentStreamPolicy::SynthesisDeferEmpty => !output.content.trim().is_empty(),
        AgentStreamPolicy::PlanningNoDoneUntilNoTools => {
            output.tool_calls.is_empty()
                && pending_tool_calls_from_dsml(&output.raw_content).is_empty()
        }
    }
}

pub fn validate_stream_output(
    label: &str,
    policy: AgentStreamPolicy,
    output: &ChatStreamOutput,
) -> Result<(), String> {
    let tool_calls_from_stream = !output.tool_calls.is_empty()
        || !pending_tool_calls_from_dsml(&output.raw_content).is_empty();
    if output.content.trim().is_empty() {
        match policy {
            AgentStreamPolicy::SynthesisAlwaysDone => {
                return Err(empty_assistant_response_error(label));
            }
            AgentStreamPolicy::SynthesisDeferEmpty => return Ok(()),
            AgentStreamPolicy::PlanningNoDoneUntilNoTools if !tool_calls_from_stream => {
                return Err(empty_assistant_response_error(label));
            }
            AgentStreamPolicy::PlanningNoDoneUntilNoTools => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_planning_text_buffer_delays_possible_dsml_prefix() {
        assert!(!should_flush_tool_planning_text_buffer("<|DSML|"));
        assert!(!should_flush_tool_planning_text_buffer("   <invoke"));
        assert!(should_flush_tool_planning_text_buffer(
            "普通回答已经足够长，可以开始流式显示了"
        ));
        assert!(should_flush_tool_planning_text_buffer("first line\n"));
    }

    #[test]
    fn synthesis_defer_empty_allows_agent_fallback_without_done() {
        let output = ChatStreamOutput::from_generate_output(
            String::new(),
            String::new(),
            String::new(),
            Vec::new(),
            Some("done".to_string()),
            false,
        );

        assert!(!should_emit_done(
            AgentStreamPolicy::SynthesisDeferEmpty,
            &output
        ));
        assert!(validate_stream_output(
            "Chat stream",
            AgentStreamPolicy::SynthesisDeferEmpty,
            &output
        )
        .is_ok());
        assert_eq!(
            validate_stream_output(
                "Chat stream",
                AgentStreamPolicy::SynthesisAlwaysDone,
                &output
            )
            .expect_err("strict synthesis should still reject empty output"),
            "Chat stream returned an empty assistant response"
        );
    }

    #[test]
    fn synthesis_defer_empty_emits_done_for_non_empty_output() {
        let output = ChatStreamOutput::from_generate_output(
            "final".to_string(),
            "final".to_string(),
            String::new(),
            Vec::new(),
            Some("done".to_string()),
            false,
        );

        assert!(should_emit_done(
            AgentStreamPolicy::SynthesisDeferEmpty,
            &output
        ));
        validate_stream_output(
            "Chat stream",
            AgentStreamPolicy::SynthesisDeferEmpty,
            &output,
        )
        .expect("non-empty synthesis should validate");
    }
}
