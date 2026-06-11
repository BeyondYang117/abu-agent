use crate::chat::types::{ChatMessageSegment, ChatMessageSegmentKind};

use super::finalize::{
    empty_synthesis_fallback_response, segment_phase_for_agent_phase, stopped_generation_content,
    synthesis_failed_fallback_response, RunResultBuilder,
};
use super::loop_::{LoopEnv, RunState};
use super::planning::{call_chat_completion_message, stream_scoped_chat_completion_inner};
use super::prepare::{prepare_agent_step, PrepareStepInput};
use super::stop::{
    empty_assistant_response_error, extract_reasoning_content, final_assistant_api_message,
    merge_reasoning, sanitize_assistant_text_response,
};
use super::stream::ChatStreamOutput;
use super::types::{AgentPhase, AgentRunConfig, AgentRunResult, AgentStreamPolicy};

pub(crate) struct SynthesisCompleted {
    pub(crate) response: String,
    pub(crate) reasoning: Option<String>,
    pub(crate) response_reasoning: Option<String>,
    pub(crate) phase: AgentPhase,
    pub(crate) response_segment: ChatMessageSegment,
    pub(crate) response_reasoning_segment: ChatMessageSegment,
}

pub(crate) enum SynthesisFlow {
    Completed(SynthesisCompleted),
    Early(AgentRunResult),
}

pub(crate) async fn synthesis_step(
    env: &LoopEnv<'_>,
    state: &mut RunState,
) -> Result<SynthesisFlow, String> {
    let config = env.config;
    let host = env.host;
    state.step_number = state.step_number.saturating_add(1);
    let step_number = state.step_number;
    let phase = if state.tool_records.is_empty() && !state.provider_tools_unsupported {
        AgentPhase::Plain
    } else {
        AgentPhase::Synthesis
    };
    let prepared = prepare_agent_step(PrepareStepInput {
        step_number,
        previous_steps: &state.steps,
        runtime_messages: &state.runtime_messages,
        tools: &[],
        phase,
    });
    let synthesis_stream_policy = if state.tool_records.is_empty() {
        AgentStreamPolicy::SynthesisAlwaysDone
    } else {
        AgentStreamPolicy::SynthesisDeferEmpty
    };
    let response_phase = segment_phase_for_agent_phase(phase);
    let response_reasoning_segment = state.segment_builder.reserve(
        ChatMessageSegmentKind::Reasoning,
        response_phase.clone(),
        Some(step_number),
        None,
        &format!("step_{step_number}_reasoning"),
    );
    let response_segment = state.segment_builder.reserve(
        ChatMessageSegmentKind::Text,
        response_phase.clone(),
        Some(step_number),
        None,
        &format!("step_{step_number}_text"),
    );

    let (response, reasoning, response_reasoning) = if config.stream_enabled {
        let stream = stream_scoped_chat_completion_inner(
            config.state,
            host,
            &config.provider,
            &config.model,
            prepared.runtime_messages,
            None,
            config.retry_attempts,
            config.thinking_enabled,
            config.max_output_tokens,
            &config.conversation_id,
            &config.run_id,
            &config.message_id,
            config.generation,
            "Chat stream",
            synthesis_stream_policy,
            Some(response_segment.clone()),
            Some(response_reasoning_segment.clone()),
            None,
        )
        .await
        .map_err(|err| {
            if state.tool_records.is_empty() {
                err.to_string()
            } else {
                eprintln!(
                    "Chat synthesis stream failed after tool records; preserving tool results with fallback: {}",
                    err
                );
                String::new()
            }
        });
        let stream = match stream {
            Ok(stream) => stream,
            Err(err) if err.is_empty() && !state.tool_records.is_empty() => {
                let fallback = synthesis_failed_fallback_response(&config.language);
                return Ok(SynthesisFlow::Early(
                    RunResultBuilder::new(host, env.ids(), fallback)
                        .segment(&response_segment)
                        .emit_done("done")
                        .outcome("error")
                        .finish(
                            std::mem::take(&mut state.segment_builder),
                            &state.planning_reasoning_parts,
                            std::mem::take(&mut state.tool_records),
                            std::mem::take(&mut state.generated_api_messages),
                            std::mem::take(&mut state.steps),
                        ),
                ));
            }
            Err(err) => return Err(err),
        };
        if stream.cancelled {
            if !state.tool_records.is_empty() {
                let stored_content = if stream.content.trim().is_empty() {
                    stopped_generation_content(&config.language)
                } else {
                    stream.content.clone()
                };
                return Ok(SynthesisFlow::Early(
                    RunResultBuilder::new(host, env.ids(), stored_content)
                        .segment(&response_segment)
                        .api_reasoning(stream.reasoning.clone())
                        .reasoning_tail(stream.reasoning)
                        .outcome("cancelled")
                        .finish(
                            std::mem::take(&mut state.segment_builder),
                            &state.planning_reasoning_parts,
                            std::mem::take(&mut state.tool_records),
                            std::mem::take(&mut state.generated_api_messages),
                            std::mem::take(&mut state.steps),
                        ),
                ));
            }
            return Err("cancelled".to_string());
        }
        let final_reasoning_for_api = stream.reasoning.clone();
        let reasoning = merge_reasoning(&state.planning_reasoning_parts, stream.reasoning.clone());
        let response = sanitize_assistant_text_response(&stream.content);
        if response.trim().is_empty() {
            if !state.tool_records.is_empty() {
                log_empty_synthesis_output(config, phase, &stream, state.tool_records.len());
                let fallback = RunResultBuilder::new(
                    host,
                    env.ids(),
                    empty_synthesis_fallback_response(&config.language),
                )
                .emit_segment_opt(Some(response_segment.clone()))
                .api_reasoning(final_reasoning_for_api.clone())
                .emit_done("done")
                .emit_and_record(&mut state.generated_api_messages);
                (fallback, reasoning, final_reasoning_for_api)
            } else {
                return Err(empty_assistant_response_error("Chat stream"));
            }
        } else {
            if !state.generated_api_messages.is_empty() {
                state.generated_api_messages.push(final_assistant_api_message(
                    &response,
                    final_reasoning_for_api.as_deref(),
                ));
            }
            (response, reasoning, final_reasoning_for_api)
        }
    } else {
        let runtime_messages = std::mem::take(&mut state.runtime_messages);
        let message_result = tokio::select! {
            result = call_chat_completion_message(
                config.state,
                &config.provider,
                &config.model,
                runtime_messages,
                None,
                config.retry_attempts,
                config.thinking_enabled,
                config.max_output_tokens,
                &config.conversation_id,
                &config.message_id,
                "Chat API",
            ) => result,
            _ = host.wait_for_generation_inactive(&config.conversation_id, config.generation) => {
                host.emit_stream_done(
                    &config.conversation_id,
                    &config.run_id,
                    &config.message_id,
                    "cancelled",
                    "",
                );
                return Err("cancelled".to_string());
            }
        };
        let message = match message_result {
            Ok(message) => message,
            Err(err) if !state.tool_records.is_empty() => {
                eprintln!(
                    "Chat synthesis request failed after tool records; preserving tool results with fallback: {}",
                    err
                );
                let fallback = synthesis_failed_fallback_response(&config.language);
                return Ok(SynthesisFlow::Early(
                    RunResultBuilder::new(host, env.ids(), fallback)
                        .segment(&response_segment)
                        .emit_done("done")
                        .outcome("error")
                        .finish(
                            std::mem::take(&mut state.segment_builder),
                            &state.planning_reasoning_parts,
                            std::mem::take(&mut state.tool_records),
                            std::mem::take(&mut state.generated_api_messages),
                            std::mem::take(&mut state.steps),
                        ),
                ));
            }
            Err(err) => return Err(err),
        };
        let response = sanitize_assistant_text_response(
            message
                .get("content")
                .and_then(|content| content.as_str())
                .unwrap_or_default(),
        );
        let reasoning = merge_reasoning(
            &state.planning_reasoning_parts,
            extract_reasoning_content(&message),
        );
        let response_reasoning = extract_reasoning_content(&message);
        if response.trim().is_empty() && !state.tool_records.is_empty() {
            eprintln!(
                "Chat agent empty synthesis fallback: conversation_id={} run_id={} provider_id={} model={} phase={:?} stream=false tool_records={} finish_reason={}",
                config.conversation_id,
                config.run_id,
                config.provider.id,
                config.model,
                phase,
                state.tool_records.len(),
                message
                    .get("finish_reason")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown"),
            );
            let fallback = RunResultBuilder::new(
                host,
                env.ids(),
                empty_synthesis_fallback_response(&config.language),
            )
            .emit_segment_opt(Some(response_segment.clone()))
            .api_reasoning(extract_reasoning_content(&message))
            .emit_done("done")
            .emit_and_record(&mut state.generated_api_messages);
            (fallback, reasoning, response_reasoning)
        } else if response.trim().is_empty() {
            host.emit_stream_done(
                &config.conversation_id,
                &config.run_id,
                &config.message_id,
                "error",
                "",
            );
            return Err(empty_assistant_response_error("Chat API"));
        } else {
            host.emit_stream_delta(
                &config.conversation_id,
                &config.run_id,
                &config.message_id,
                &response,
                None,
                Some(&response_segment),
            );
            host.emit_stream_done(
                &config.conversation_id,
                &config.run_id,
                &config.message_id,
                "done",
                &response,
            );
            if !state.generated_api_messages.is_empty() {
                state.generated_api_messages.push(message);
            }
            (response, reasoning, response_reasoning)
        }
    };

    Ok(SynthesisFlow::Completed(SynthesisCompleted {
        response,
        reasoning,
        response_reasoning,
        phase,
        response_segment,
        response_reasoning_segment,
    }))
}

fn log_empty_synthesis_output(
    config: &AgentRunConfig<'_>,
    phase: AgentPhase,
    stream: &ChatStreamOutput,
    tool_record_count: usize,
) {
    eprintln!(
        "Chat agent empty synthesis fallback: conversation_id={} run_id={} provider_id={} model={} phase={:?} stream=true tool_records={} finish_reason={} raw_chars={} cleaned_chars={} reasoning_chars={} stream_tool_calls={}",
        config.conversation_id,
        config.run_id,
        config.provider.id,
        config.model,
        phase,
        tool_record_count,
        stream.finish_reason.as_deref().unwrap_or("unknown"),
        stream.raw_content.chars().count(),
        stream.content.chars().count(),
        stream.reasoning.as_deref().map(|value| value.chars().count()).unwrap_or(0),
        stream.tool_calls.len(),
    );
}
