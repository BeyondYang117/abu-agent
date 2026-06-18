//! OpenAI **Responses API** adapter (`POST /v1/responses`).
//!
//! Peer to `openai.rs` (Chat Completions) and `anthropic.rs` (Messages). Codex /
//! Responses-native models (and proxies wrapping them) often emit tool-call arguments
//! ONLY over this protocol's streaming events (`response.function_call_arguments.*`) —
//! on Chat Completions they return empty `arguments`. This adapter speaks the Responses
//! wire format while presenting the same provider-agnostic `LanguageModelProvider`
//! surface, so the agent loop is unchanged.
//!
//! Conversation state is modelled as a flat `input` item list rather than chat messages:
//! tool calls are `function_call` items and tool results are `function_call_output`
//! items (see `responses_input_from_model_messages` in `types.rs`).

use reqwest::header::ACCEPT_ENCODING;
use serde_json::Value;

use crate::api::{send_with_failover, with_standard_request_timeout};
use crate::settings::ModelProvider;
use crate::state::AppState;
use crate::usage::{
    chat_usage_source_for_label, error_kind_from_message, model_usage_from_openai_value,
    operation_from_label, record_model_call, UsageRecordInput,
};

use super::{
    parse_tool_arguments, responses_input_from_model_messages, stream_read_error, GenerateOutput,
    GenerateRequest, LanguageModelProvider, ModelError, ModelFuture, ModelUsage, PendingToolCall,
    ProviderCapabilities, StreamPart, StreamSink,
};

pub struct OpenAiResponsesProvider<'a> {
    state: &'a AppState,
    provider: &'a ModelProvider,
    retry_attempts: usize,
}

impl<'a> OpenAiResponsesProvider<'a> {
    pub fn new(state: &'a AppState, provider: &'a ModelProvider, retry_attempts: usize) -> Self {
        Self {
            state,
            provider,
            retry_attempts,
        }
    }
}

impl LanguageModelProvider for OpenAiResponsesProvider<'_> {
    fn generate<'a>(&'a self, request: GenerateRequest) -> ModelFuture<'a, GenerateOutput> {
        Box::pin(async move { self.generate_inner(request).await })
    }

    fn stream<'a>(
        &'a self,
        request: GenerateRequest,
        sink: &'a mut (dyn StreamSink + Send),
    ) -> ModelFuture<'a, GenerateOutput> {
        Box::pin(async move { self.stream_inner(request, sink).await })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            tool_calling: self.provider.supports_tools,
            vision: true,
            streaming: true,
            reasoning: true,
        }
    }
}

impl OpenAiResponsesProvider<'_> {
    async fn generate_inner(&self, request: GenerateRequest) -> Result<GenerateOutput, ModelError> {
        let label = request_label(&request, "Responses API");
        let started_at = chrono::Local::now().timestamp();
        let started = std::time::Instant::now();
        let body = self.request_body(&request, false);
        let response = send_with_failover(
            self.state,
            &label,
            self.retry_attempts,
            &self.provider.id,
            &self.provider.api_keys,
            |key| {
                with_standard_request_timeout(
                    self.state
                        .http
                        .post(self.responses_url())
                        .bearer_auth(key)
                        .header(ACCEPT_ENCODING, "identity")
                        .json(&body),
                )
                .send()
            },
        )
        .await
        .map_err(|err| {
            self.record_usage_failure(&request, &label, started_at, started.elapsed(), &err);
            ModelError::new(err)
        })?;

        let raw = response.text().await.map_err(|err| {
            let message = format!("{label} read body: {err}");
            self.record_usage_failure(&request, &label, started_at, started.elapsed(), &message);
            ModelError::new(message)
        })?;
        let value: Value = serde_json::from_str(&raw).map_err(|err| {
            let message = format!(
                "{label} parse JSON: {} (body: {})",
                err,
                raw.chars().take(500).collect::<String>()
            );
            self.record_usage_failure(&request, &label, started_at, started.elapsed(), &message);
            ModelError::new(message)
        })?;
        let output = output_from_responses(&value, &raw, &label)?;
        self.record_usage_success(
            &request,
            &label,
            started_at,
            started.elapsed(),
            output.usage.clone(),
        );
        Ok(output)
    }

    async fn stream_inner(
        &self,
        request: GenerateRequest,
        sink: &mut (dyn StreamSink + Send),
    ) -> Result<GenerateOutput, ModelError> {
        let label = request_label(&request, "Responses stream");
        let started_at = chrono::Local::now().timestamp();
        let started = std::time::Instant::now();
        let body = self.request_body(&request, true);
        let mut response = send_with_failover(
            self.state,
            &label,
            self.retry_attempts,
            &self.provider.id,
            &self.provider.api_keys,
            |key| {
                self.state
                    .http
                    .post(self.responses_url())
                    .bearer_auth(key)
                    .header(ACCEPT_ENCODING, "identity")
                    .json(&body)
                    .send()
            },
        )
        .await
        .map_err(|err| {
            self.record_usage_failure(&request, &label, started_at, started.elapsed(), &err);
            ModelError::new(err)
        })?;

        let mut buffer = String::new();
        let mut state = ResponsesStreamState::default();

        loop {
            let chunk = response.chunk().await.map_err(|err| {
                let model_error = stream_read_error(&label, &err);
                self.record_usage_failure(
                    &request,
                    &label,
                    started_at,
                    started.elapsed(),
                    &model_error.to_string(),
                );
                model_error
            })?;
            let Some(chunk) = chunk else {
                break;
            };
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(pos) = buffer.find('\n') {
                let line: String = buffer.drain(..=pos).collect();
                let line = line.trim();
                // Responses SSE carries `event:` and `data:` lines; the `data:` JSON
                // already includes a `type` field mirroring the event name, so we only
                // need the data payload.
                if !line.starts_with("data:") {
                    continue;
                }
                let data = line.trim_start_matches("data:").trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                let value: Value = match serde_json::from_str(data) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                if let Some(err) = handle_responses_stream_event(&value, &mut state, sink)? {
                    self.record_usage_failure(
                        &request,
                        &label,
                        started_at,
                        started.elapsed(),
                        &err,
                    );
                    return Err(ModelError::new(err));
                }
            }
        }

        let output = state.finish(sink)?;
        self.record_usage_success(
            &request,
            &label,
            started_at,
            started.elapsed(),
            output.usage.clone(),
        );
        Ok(output)
    }

    fn responses_url(&self) -> String {
        format!("{}/responses", self.provider.base_url.trim_end_matches('/'))
    }

    fn request_body(&self, request: &GenerateRequest, stream: bool) -> Value {
        let mut body = serde_json::json!({
            "model": request.model,
            "input": responses_input_from_model_messages(&request.messages),
        });
        if !request.system.trim().is_empty() {
            body["instructions"] = Value::String(request.system.clone());
        }
        if request.options.max_tokens > 0 {
            body["max_output_tokens"] = Value::from(request.options.max_tokens);
        }
        if stream {
            body["stream"] = Value::Bool(true);
        }
        if !request.tools.is_empty() {
            body["tools"] = Value::Array(
                request
                    .tools
                    .iter()
                    .map(|tool| tool.to_openai_responses_tool())
                    .collect(),
            );
            body["tool_choice"] = Value::String("auto".to_string());
        }
        if let Some(overrides) = request.options.provider_options.as_object() {
            for (key, value) in overrides {
                body[key] = value.clone();
            }
        }
        body
    }

    fn record_usage_success(
        &self,
        request: &GenerateRequest,
        label: &str,
        started_at: i64,
        duration: std::time::Duration,
        usage: Option<ModelUsage>,
    ) {
        let source = request
            .metadata
            .usage_source
            .clone()
            .unwrap_or_else(|| chat_usage_source_for_label(label));
        let operation = request
            .metadata
            .usage_operation
            .clone()
            .unwrap_or_else(|| operation_from_label(label));
        record_model_call(
            self.state,
            UsageRecordInput {
                provider: self.provider,
                model: &request.model,
                source: &source,
                operation: &operation,
                status: "success",
                status_code: Some(200),
                usage,
                usage_source: "provider_reported",
                started_at,
                duration_ms: duration.as_millis() as u64,
                conversation_id: request.metadata.conversation_id.clone(),
                message_id: request.metadata.message_id.clone(),
                error_kind: None,
            },
        );
    }

    fn record_usage_failure(
        &self,
        request: &GenerateRequest,
        label: &str,
        started_at: i64,
        duration: std::time::Duration,
        error: &str,
    ) {
        let source = request
            .metadata
            .usage_source
            .clone()
            .unwrap_or_else(|| chat_usage_source_for_label(label));
        let operation = request
            .metadata
            .usage_operation
            .clone()
            .unwrap_or_else(|| operation_from_label(label));
        record_model_call(
            self.state,
            UsageRecordInput {
                provider: self.provider,
                model: &request.model,
                source: &source,
                operation: &operation,
                status: "error",
                status_code: crate::api::extract_status_code(error),
                usage: None,
                usage_source: "missing",
                started_at,
                duration_ms: duration.as_millis() as u64,
                conversation_id: request.metadata.conversation_id.clone(),
                message_id: request.metadata.message_id.clone(),
                error_kind: Some(error_kind_from_message(error)),
            },
        );
    }
}

/// A function-call item being assembled across streaming events. Keyed internally by
/// `item_id` (the `fc_…` id the delta/done events reference); `call_id` (the `call_…`
/// id) is what the model expects echoed back as `function_call_output.call_id`.
struct ResponsesToolPartial {
    item_id: String,
    call_id: String,
    name: String,
    arguments: String,
    done: bool,
}

#[derive(Default)]
struct ResponsesStreamState {
    text: String,
    reasoning: String,
    tool_calls: Vec<ResponsesToolPartial>,
    finish_reason: Option<String>,
    usage: Option<ModelUsage>,
}

impl ResponsesStreamState {
    fn partial_mut(&mut self, item_id: &str) -> Option<&mut ResponsesToolPartial> {
        self.tool_calls
            .iter_mut()
            .find(|partial| partial.item_id == item_id)
    }

    fn finalize_tool_call(
        &mut self,
        item_id: &str,
        sink: &mut (dyn StreamSink + Send),
    ) -> Result<(), ModelError> {
        if let Some(partial) = self.tool_calls.iter_mut().find(|p| p.item_id == item_id) {
            if partial.done {
                return Ok(());
            }
            partial.done = true;
            let call = pending_tool_call_from_partial(partial);
            sink.emit(StreamPart::ToolCallDone { call })?;
        }
        Ok(())
    }

    fn finish(mut self, sink: &mut (dyn StreamSink + Send)) -> Result<GenerateOutput, ModelError> {
        // Flush any function calls that never received an explicit output_item.done.
        let pending_ids: Vec<String> = self
            .tool_calls
            .iter()
            .filter(|partial| !partial.done)
            .map(|partial| partial.item_id.clone())
            .collect();
        for item_id in pending_ids {
            self.finalize_tool_call(&item_id, sink)?;
        }
        let tool_calls: Vec<PendingToolCall> = self
            .tool_calls
            .iter()
            .map(pending_tool_call_from_partial)
            .collect();
        let reason = self.finish_reason.clone().unwrap_or_else(|| {
            if tool_calls.is_empty() {
                "stop".to_string()
            } else {
                "tool_calls".to_string()
            }
        });
        sink.emit(StreamPart::Finish {
            reason: reason.clone(),
            full: self.text.clone(),
        })?;
        Ok(GenerateOutput {
            text: self.text,
            reasoning: non_empty(self.reasoning),
            tool_calls,
            usage: self.usage,
            finish_reason: Some(reason),
            provider_messages: Vec::new(),
            cancelled: false,
        })
    }
}

/// Dispatch a single streaming Responses event into the accumulating state, emitting
/// `StreamPart`s as content arrives. Returns `Ok(Some(err))` for a terminal provider
/// error (caller records the failure and aborts), `Ok(None)` otherwise. Free function
/// (no `&self`) so the stream loop and unit tests share one code path.
fn handle_responses_stream_event(
    value: &Value,
    state: &mut ResponsesStreamState,
    sink: &mut (dyn StreamSink + Send),
) -> Result<Option<String>, ModelError> {
    let event_type = value.get("type").and_then(Value::as_str).unwrap_or("");
    match event_type {
        "response.output_text.delta" => {
            if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                if !delta.is_empty() {
                    state.text.push_str(delta);
                    sink.emit(StreamPart::TextDelta {
                        delta: delta.to_string(),
                    })?;
                }
            }
        }
        "response.reasoning_summary_text.delta"
        | "response.reasoning_text.delta"
        | "response.reasoning_summary.delta" => {
            if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                if !delta.is_empty() {
                    state.reasoning.push_str(delta);
                    sink.emit(StreamPart::ReasoningDelta {
                        delta: delta.to_string(),
                    })?;
                }
            }
        }
        "response.output_item.added" => {
            if let Some(item) = value.get("item") {
                if item.get("type").and_then(Value::as_str) == Some("function_call") {
                    let item_id = item
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let call_id = item
                        .get("call_id")
                        .and_then(Value::as_str)
                        .unwrap_or(&item_id)
                        .to_string();
                    let name = item
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    sink.emit(StreamPart::ToolCallStart {
                        id: call_id.clone(),
                        name: name.clone(),
                    })?;
                    state.tool_calls.push(ResponsesToolPartial {
                        item_id,
                        call_id,
                        name,
                        arguments: String::new(),
                        done: false,
                    });
                }
            }
        }
        "response.function_call_arguments.delta" => {
            let item_id = value.get("item_id").and_then(Value::as_str).unwrap_or("");
            if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                if let Some(partial) = state.partial_mut(item_id) {
                    partial.arguments.push_str(delta);
                    sink.emit(StreamPart::ToolCallDelta {
                        id: partial.call_id.clone(),
                        delta: delta.to_string(),
                    })?;
                }
            }
        }
        "response.function_call_arguments.done" => {
            let item_id = value.get("item_id").and_then(Value::as_str).unwrap_or("");
            if let Some(arguments) = value.get("arguments").and_then(Value::as_str) {
                if let Some(partial) = state.partial_mut(item_id) {
                    // The `done` event carries the full argument string — prefer it over
                    // the accumulated deltas to avoid any drift.
                    partial.arguments = arguments.to_string();
                }
            }
        }
        "response.output_item.done" => {
            if let Some(item) = value.get("item") {
                if item.get("type").and_then(Value::as_str) == Some("function_call") {
                    let item_id = item.get("id").and_then(Value::as_str).unwrap_or("");
                    if let Some(arguments) = item.get("arguments").and_then(Value::as_str) {
                        if let Some(partial) = state.partial_mut(item_id) {
                            if !arguments.is_empty() {
                                partial.arguments = arguments.to_string();
                            }
                        }
                    }
                    state.finalize_tool_call(item_id, sink)?;
                }
            }
        }
        "response.completed" | "response.incomplete" => {
            if let Some(response) = value.get("response") {
                if let Some(usage) = model_usage_from_openai_value(response) {
                    state.usage = Some(usage);
                }
                if let Some(status) = response.get("status").and_then(Value::as_str) {
                    state.finish_reason = Some(responses_finish_reason(status, state));
                }
            }
        }
        "response.failed" | "error" => {
            let message = value
                .get("response")
                .and_then(|response| response.get("error"))
                .or_else(|| value.get("error"))
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("Responses stream failed")
                .to_string();
            return Ok(Some(message));
        }
        _ => {}
    }
    Ok(None)
}

fn pending_tool_call_from_partial(partial: &ResponsesToolPartial) -> PendingToolCall {
    let raw = if partial.arguments.trim().is_empty() {
        "{}".to_string()
    } else {
        partial.arguments.clone()
    };
    let (arguments, arguments_parse_error) = parse_tool_arguments(&raw);
    PendingToolCall {
        id: partial.call_id.clone(),
        function_name: partial.name.clone(),
        arguments,
        arguments_raw: raw,
        arguments_parse_error,
    }
}

/// Parse a non-streaming `/v1/responses` body into a `GenerateOutput`.
pub fn output_from_responses(
    value: &Value,
    raw: &str,
    label: &str,
) -> Result<GenerateOutput, ModelError> {
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Unknown Responses API error");
        return Err(ModelError::new(format!("{label}: {message}")));
    }
    let output = value
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_response(label, raw))?;

    let mut text = String::new();
    let mut tool_calls = Vec::new();
    for item in output {
        match item.get("type").and_then(Value::as_str) {
            Some("message") => {
                if let Some(content) = item.get("content").and_then(Value::as_array) {
                    for part in content {
                        if part.get("type").and_then(Value::as_str) == Some("output_text") {
                            if let Some(part_text) = part.get("text").and_then(Value::as_str) {
                                text.push_str(part_text);
                            }
                        }
                    }
                }
            }
            Some("function_call") => {
                let call_id = item
                    .get("call_id")
                    .or_else(|| item.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let raw_args = item
                    .get("arguments")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or("{}")
                    .to_string();
                let (arguments, arguments_parse_error) = parse_tool_arguments(&raw_args);
                tool_calls.push(PendingToolCall {
                    id: call_id,
                    function_name: name,
                    arguments,
                    arguments_raw: raw_args,
                    arguments_parse_error,
                });
            }
            _ => {}
        }
    }

    let usage = model_usage_from_openai_value(value);
    let finish_reason = value
        .get("status")
        .and_then(Value::as_str)
        .map(|status| {
            if status == "completed" && !tool_calls.is_empty() {
                "tool_calls".to_string()
            } else if status == "completed" {
                "stop".to_string()
            } else {
                status.to_string()
            }
        });

    Ok(GenerateOutput {
        text,
        reasoning: None,
        tool_calls,
        usage,
        finish_reason,
        provider_messages: Vec::new(),
        cancelled: false,
    })
}

fn responses_finish_reason(status: &str, state: &ResponsesStreamState) -> String {
    match status {
        "completed" if !state.tool_calls.is_empty() => "tool_calls".to_string(),
        "completed" => "stop".to_string(),
        other => other.to_string(),
    }
}

fn request_label(request: &GenerateRequest, fallback: &str) -> String {
    request
        .metadata
        .label
        .trim()
        .is_empty()
        .then(|| fallback.to_string())
        .unwrap_or_else(|| request.metadata.label.clone())
}

fn invalid_response(label: &str, raw: &str) -> ModelError {
    ModelError::new(format!(
        "Invalid {label} response: {}",
        raw.chars().take(500).collect::<String>()
    ))
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive the streaming event handler with the exact `data:` JSON I captured from the
    /// live `gpt-5.3-codex-spark` Responses stream, asserting the tool call's arguments
    /// (which Chat Completions dropped) come through.
    fn run_events(events: &[Value]) -> (Vec<StreamPart>, GenerateOutput) {
        let mut parts = Vec::new();
        let mut state = ResponsesStreamState::default();
        let mut sink = |part: StreamPart| {
            parts.push(part);
            Ok(())
        };
        for event in events {
            handle_responses_stream_event(event, &mut state, &mut sink).expect("event");
        }
        let output = state.finish(&mut sink).expect("finish");
        (parts, output)
    }

    #[test]
    fn stream_function_call_arguments_are_captured() {
        let events = vec![
            serde_json::json!({
                "type": "response.output_item.added",
                "output_index": 1,
                "item": { "id": "fc_1", "type": "function_call", "status": "in_progress", "arguments": "", "call_id": "call_abc", "name": "web_search" }
            }),
            serde_json::json!({
                "type": "response.function_call_arguments.done",
                "arguments": "{\"query\":\"吉林市 明天 天气\"}",
                "item_id": "fc_1",
                "output_index": 1
            }),
            serde_json::json!({
                "type": "response.output_item.done",
                "output_index": 1,
                "item": { "id": "fc_1", "type": "function_call", "status": "completed", "arguments": "{\"query\":\"吉林市 明天 天气\"}", "call_id": "call_abc", "name": "web_search" }
            }),
            serde_json::json!({
                "type": "response.completed",
                "response": { "status": "completed", "usage": { "input_tokens": 10, "output_tokens": 5, "total_tokens": 15 } }
            }),
        ];
        let (parts, output) = run_events(&events);

        assert_eq!(output.tool_calls.len(), 1);
        let call = &output.tool_calls[0];
        assert_eq!(call.id, "call_abc");
        assert_eq!(call.function_name, "web_search");
        assert_eq!(call.arguments["query"], "吉林市 明天 天气");
        assert!(call.arguments_parse_error.is_none());
        assert_eq!(output.finish_reason.as_deref(), Some("tool_calls"));
        assert_eq!(output.usage.and_then(|u| u.total_tokens), Some(15));
        assert!(parts.iter().any(|p| matches!(p, StreamPart::ToolCallStart { .. })));
        assert!(parts.iter().any(|p| matches!(p, StreamPart::ToolCallDone { .. })));
    }

    #[test]
    fn stream_text_deltas_accumulate() {
        let events = vec![
            serde_json::json!({ "type": "response.output_text.delta", "delta": "你好" }),
            serde_json::json!({ "type": "response.output_text.delta", "delta": "，世界" }),
            serde_json::json!({ "type": "response.completed", "response": { "status": "completed" } }),
        ];
        let (parts, output) = run_events(&events);
        assert_eq!(output.text, "你好，世界");
        assert_eq!(output.finish_reason.as_deref(), Some("stop"));
        assert_eq!(
            parts.iter().filter(|p| matches!(p, StreamPart::TextDelta { .. })).count(),
            2
        );
    }

    #[test]
    fn non_stream_output_parses_text_and_tool_call() {
        let value = serde_json::json!({
            "status": "completed",
            "output": [
                { "type": "function_call", "call_id": "call_x", "name": "web_search", "arguments": "{\"query\":\"a\"}" }
            ],
            "usage": { "input_tokens": 3, "output_tokens": 2, "total_tokens": 5 }
        });
        let output = output_from_responses(&value, "{}", "test").expect("output");
        assert_eq!(output.tool_calls.len(), 1);
        assert_eq!(output.tool_calls[0].arguments["query"], "a");
        assert_eq!(output.finish_reason.as_deref(), Some("tool_calls"));

        let text_value = serde_json::json!({
            "status": "completed",
            "output": [ { "type": "message", "content": [ { "type": "output_text", "text": "hi" } ] } ]
        });
        let out = output_from_responses(&text_value, "{}", "test").expect("output");
        assert_eq!(out.text, "hi");
        assert_eq!(out.finish_reason.as_deref(), Some("stop"));
    }
}
