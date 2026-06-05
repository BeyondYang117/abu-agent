use serde_json::Value;

use crate::chat::model::{
    generate_request_from_openai_messages, AnthropicMessagesProvider, AppleLocalProvider,
    GenerateOptions, GenerateOutput, LanguageModelProvider, OpenAiChatProvider,
};
use crate::mcp::ChatToolDefinition;
use crate::settings::ProviderApiFormat;
use crate::skills;

use super::execute::{
    disabled_tool_content, execute_tool_call, invalid_tool_arguments_record, match_tool_call,
    unknown_tool_record, ToolExecutionContext, ToolExecutor,
};
use super::host::AgentHost;
use super::prepare::{prepare_agent_step, PrepareStepInput};
use super::stop::{
    assistant_api_message_for_tool_calls, empty_assistant_response_error,
    extract_reasoning_content, extract_tool_calls, final_assistant_api_message,
    final_response_from_planning_message, is_tools_unsupported_error, merge_reasoning,
    patch_system_message, sanitize_assistant_text_response, step_limit_system_message,
};
use super::stream::{should_emit_done, validate_stream_output, AgentStreamSink, ChatStreamOutput};
use super::types::{
    AgentPhase, AgentRunConfig, AgentRunResult, AgentStepResult, AgentStopReason, AgentStreamPolicy,
};

struct ChatPlanningStep {
    message: Value,
    streamed: bool,
}

pub async fn run_agent_loop(
    mut config: AgentRunConfig<'_>,
    host: &dyn AgentHost,
    executor: &dyn ToolExecutor,
) -> Result<AgentRunResult, String> {
    let mut runtime_messages = std::mem::take(&mut config.runtime_messages);
    let mut tools = std::mem::take(&mut config.tools);
    let mut generated_api_messages = Vec::new();
    let mut tool_records = Vec::new();
    let mut planning_reasoning_parts: Vec<String> = Vec::new();
    let max_rounds = config.settings.chat_tools.max_tool_rounds.max(1);
    let mut provider_tools_unsupported = false;
    let mut tool_planning_finished = false;
    let mut planning_final_message: Option<Value> = None;
    let mut planning_final_already_streamed = false;
    let mut steps = Vec::new();
    let mut step_number = 0u8;

    if !tools.is_empty() {
        let mut tried_skill_only_tools = false;
        let mut skill_cache = skills::SkillRunCache::default();
        for round in 0..max_rounds {
            step_number = step_number.saturating_add(1);
            if !host.is_generation_active(&config.conversation_id, config.generation) {
                host.emit_stream_done(
                    &config.conversation_id,
                    &config.run_id,
                    &config.message_id,
                    "cancelled",
                    "",
                );
                return Err("cancelled".to_string());
            }

            let prepared = prepare_agent_step(PrepareStepInput {
                step_number,
                previous_steps: &steps,
                runtime_messages: &runtime_messages,
                tools: &tools,
                phase: AgentPhase::ToolLoop,
            });
            let planning_result = if config.stream_enabled {
                match stream_scoped_chat_completion_inner(
                    config.state,
                    host,
                    &config.provider,
                    &config.model,
                    prepared.runtime_messages.clone(),
                    Some(&prepared.active_tools),
                    config.retry_attempts,
                    config.thinking_enabled,
                    &config.conversation_id,
                    &config.run_id,
                    &config.message_id,
                    config.generation,
                    "Chat tools planning",
                    prepared.stream_policy,
                )
                .await
                {
                    Ok(stream) => {
                        if stream.cancelled {
                            return Err("cancelled".to_string());
                        }
                        Ok(ChatPlanningStep {
                            message: stream.to_openai_compatible_message(),
                            streamed: true,
                        })
                    }
                    Err(err) => Err(err),
                }
            } else {
                tokio::select! {
                    result = call_chat_completion_message(
                        config.state,
                        &config.provider,
                        &config.model,
                        prepared.runtime_messages.clone(),
                        Some(&prepared.active_tools),
                        config.retry_attempts,
                        config.thinking_enabled,
                        "Chat tools planning",
                    ) => result.map(|message| ChatPlanningStep {
                        message,
                        streamed: false,
                    }),
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
                }
            };
            let message = match planning_result {
                Ok(step) => {
                    planning_final_already_streamed = step.streamed;
                    step.message
                }
                Err(err) if is_tools_unsupported_error(&err) => {
                    let skill_only: Vec<ChatToolDefinition> = tools
                        .iter()
                        .filter(|tool| tool.source == "skill")
                        .cloned()
                        .collect();
                    if !tried_skill_only_tools
                        && skill_only.len() < tools.len()
                        && !skill_only.is_empty()
                    {
                        eprintln!(
                            "Chat provider {} rejected tools; retrying with skill-native tools only",
                            config.provider.id
                        );
                        tools = skill_only;
                        tried_skill_only_tools = true;
                        continue;
                    }
                    eprintln!(
                        "Chat provider {} rejected tools; falling back to plain chat",
                        config.provider.id
                    );
                    provider_tools_unsupported = true;
                    steps.push(AgentStepResult {
                        step_number,
                        phase: AgentPhase::ToolLoop,
                        response_messages: Vec::new(),
                        tool_records: Vec::new(),
                        streamed: false,
                        stop_reason: Some(AgentStopReason::ProviderToolsUnsupported),
                    });
                    break;
                }
                Err(err) => return Err(err),
            };
            let tool_calls = extract_tool_calls(&message);
            if tool_calls.is_empty() {
                tool_planning_finished = true;
                planning_final_message = Some(message.clone());
                steps.push(AgentStepResult {
                    step_number,
                    phase: AgentPhase::ToolLoop,
                    response_messages: vec![message],
                    tool_records: Vec::new(),
                    streamed: planning_final_already_streamed,
                    stop_reason: Some(AgentStopReason::Natural),
                });
                break;
            }
            planning_final_already_streamed = false;
            if let Some(reasoning) = extract_reasoning_content(&message) {
                if !config.stream_enabled {
                    host.emit_stream_delta(
                        &config.conversation_id,
                        &config.run_id,
                        &config.message_id,
                        "",
                        Some(&reasoning),
                    );
                }
                planning_reasoning_parts.push(reasoning);
            }

            let assistant_message = assistant_api_message_for_tool_calls(&message, &tool_calls);
            runtime_messages.push(assistant_message);
            generated_api_messages.push(runtime_messages.last().cloned().unwrap_or(Value::Null));
            let mut step_response_messages =
                vec![runtime_messages.last().cloned().unwrap_or(Value::Null)];
            let mut step_tool_records = Vec::new();
            for tool_call in tool_calls {
                let Some(tool) = match_tool_call(&tools, &tool_call.function_name) else {
                    let disabled = disabled_tool_content(&tool_call);
                    if disabled.is_none() {
                        let error = format!("Unknown tool requested: {}", tool_call.function_name);
                        let record = unknown_tool_record(&tool_call, round + 1, error);
                        host.emit_tool_record(
                            &config.conversation_id,
                            &config.run_id,
                            &config.message_id,
                            &record,
                        );
                        step_tool_records.push(record.clone());
                        tool_records.push(record);
                    }
                    let content = disabled.unwrap_or_else(|| {
                        format!("Unknown tool requested: {}", tool_call.function_name)
                    });
                    let tool_message = serde_json::json!({
                        "role": "tool",
                        "tool_call_id": tool_call.id,
                        "content": content,
                    });
                    runtime_messages.push(tool_message.clone());
                    generated_api_messages.push(tool_message.clone());
                    step_response_messages.push(tool_message);
                    continue;
                };
                let tool_call_id = tool_call.id.clone();
                if let Some(error) = tool_call.arguments_parse_error.clone() {
                    let record = invalid_tool_arguments_record(&tool_call, tool, round + 1, error);
                    host.emit_tool_record(
                        &config.conversation_id,
                        &config.run_id,
                        &config.message_id,
                        &record,
                    );
                    let tool_message = serde_json::json!({
                        "role": "tool",
                        "tool_call_id": tool_call_id,
                        "content": "Tool arguments JSON is invalid or incomplete. Retry this tool call with a compact, valid JSON object for arguments.",
                    });
                    runtime_messages.push(tool_message.clone());
                    generated_api_messages.push(tool_message.clone());
                    step_response_messages.push(tool_message);
                    step_tool_records.push(record.clone());
                    tool_records.push(record);
                    continue;
                }
                let ctx = ToolExecutionContext {
                    conversation_id: &config.conversation_id,
                    run_id: &config.run_id,
                    message_id: &config.message_id,
                    generation: config.generation,
                    round: round + 1,
                };
                let (record, tool_content) = execute_tool_call(
                    host,
                    executor,
                    &config.settings,
                    &ctx,
                    tool,
                    tool_call,
                    &mut skill_cache,
                )
                .await;
                let tool_message = serde_json::json!({
                    "role": "tool",
                    "tool_call_id": tool_call_id,
                    "content": tool_content,
                });
                runtime_messages.push(tool_message.clone());
                generated_api_messages.push(tool_message.clone());
                step_response_messages.push(tool_message);
                step_tool_records.push(record.clone());
                tool_records.push(record);
            }
            steps.push(AgentStepResult {
                step_number,
                phase: AgentPhase::ToolLoop,
                response_messages: step_response_messages,
                tool_records: step_tool_records,
                streamed: config.stream_enabled,
                stop_reason: None,
            });
        }
        if !provider_tools_unsupported && !tool_planning_finished {
            runtime_messages.push(step_limit_system_message());
            steps.push(AgentStepResult {
                step_number: step_number.saturating_add(1),
                phase: AgentPhase::ToolLoop,
                response_messages: vec![runtime_messages.last().cloned().unwrap_or(Value::Null)],
                tool_records: Vec::new(),
                streamed: false,
                stop_reason: Some(AgentStopReason::StepLimit),
            });
        }
    }

    if provider_tools_unsupported {
        patch_system_message(
            &mut runtime_messages,
            &config.provider_tools_fallback_system_prompt,
        );
    }

    if let Some(message) = planning_final_message {
        let (response, reasoning) =
            final_response_from_planning_message(&message, &planning_reasoning_parts)?;
        if !planning_final_already_streamed {
            host.emit_stream_delta(
                &config.conversation_id,
                &config.run_id,
                &config.message_id,
                &response,
                None,
            );
            host.emit_stream_done(
                &config.conversation_id,
                &config.run_id,
                &config.message_id,
                "done",
                &response,
            );
        }
        if !generated_api_messages.is_empty() {
            generated_api_messages.push(message);
        }
        return Ok(AgentRunResult {
            content: response,
            reasoning,
            tool_records,
            api_messages: generated_api_messages,
            steps,
        });
    }

    step_number = step_number.saturating_add(1);
    let phase = if tool_records.is_empty() && !provider_tools_unsupported {
        AgentPhase::Plain
    } else {
        AgentPhase::Synthesis
    };
    let prepared = prepare_agent_step(PrepareStepInput {
        step_number,
        previous_steps: &steps,
        runtime_messages: &runtime_messages,
        tools: &[],
        phase,
    });
    let synthesis_stream_policy = if tool_records.is_empty() {
        AgentStreamPolicy::SynthesisAlwaysDone
    } else {
        AgentStreamPolicy::SynthesisDeferEmpty
    };

    let (response, reasoning) = if config.stream_enabled {
        let stream = stream_scoped_chat_completion_inner(
            config.state,
            host,
            &config.provider,
            &config.model,
            prepared.runtime_messages,
            None,
            config.retry_attempts,
            config.thinking_enabled,
            &config.conversation_id,
            &config.run_id,
            &config.message_id,
            config.generation,
            "Chat stream",
            synthesis_stream_policy,
        )
        .await?;
        if stream.cancelled {
            if !tool_records.is_empty() {
                let stored_content = if stream.content.trim().is_empty() {
                    "已停止生成。".to_string()
                } else {
                    stream.content.clone()
                };
                let final_reasoning_for_api = stream.reasoning.clone();
                let reasoning = merge_reasoning(&planning_reasoning_parts, stream.reasoning);
                if !generated_api_messages.is_empty() {
                    generated_api_messages.push(final_assistant_api_message(
                        &stored_content,
                        final_reasoning_for_api.as_deref(),
                    ));
                }
                return Ok(AgentRunResult {
                    content: stored_content,
                    reasoning,
                    tool_records,
                    api_messages: generated_api_messages,
                    steps,
                });
            }
            return Err("cancelled".to_string());
        }
        let final_reasoning_for_api = stream.reasoning.clone();
        let reasoning = merge_reasoning(&planning_reasoning_parts, stream.reasoning.clone());
        let response = sanitize_assistant_text_response(&stream.content);
        if response.trim().is_empty() {
            if !tool_records.is_empty() {
                log_empty_synthesis_output(&config, phase, &stream, tool_records.len());
                let fallback = empty_synthesis_fallback_response(&config.language);
                host.emit_stream_delta(
                    &config.conversation_id,
                    &config.run_id,
                    &config.message_id,
                    &fallback,
                    None,
                );
                host.emit_stream_done(
                    &config.conversation_id,
                    &config.run_id,
                    &config.message_id,
                    "done",
                    &fallback,
                );
                if !generated_api_messages.is_empty() {
                    generated_api_messages.push(final_assistant_api_message(
                        &fallback,
                        final_reasoning_for_api.as_deref(),
                    ));
                }
                (fallback, reasoning)
            } else {
                return Err(empty_assistant_response_error("Chat stream"));
            }
        } else {
            if !generated_api_messages.is_empty() {
                generated_api_messages.push(final_assistant_api_message(
                    &response,
                    final_reasoning_for_api.as_deref(),
                ));
            }
            (response, reasoning)
        }
    } else {
        let message = tokio::select! {
            result = call_chat_completion_message(
                config.state,
                &config.provider,
                &config.model,
                runtime_messages,
                None,
                config.retry_attempts,
                config.thinking_enabled,
                "Chat API",
            ) => result?,
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
        let response = sanitize_assistant_text_response(
            message
                .get("content")
                .and_then(|content| content.as_str())
                .unwrap_or_default(),
        );
        let reasoning = merge_reasoning(
            &planning_reasoning_parts,
            extract_reasoning_content(&message),
        );
        if response.trim().is_empty() && !tool_records.is_empty() {
            eprintln!(
                "Chat agent empty synthesis fallback: conversation_id={} run_id={} provider_id={} model={} phase={:?} stream=false tool_records={} finish_reason={}",
                config.conversation_id,
                config.run_id,
                config.provider.id,
                config.model,
                phase,
                tool_records.len(),
                message
                    .get("finish_reason")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown"),
            );
            let fallback = empty_synthesis_fallback_response(&config.language);
            host.emit_stream_delta(
                &config.conversation_id,
                &config.run_id,
                &config.message_id,
                &fallback,
                None,
            );
            host.emit_stream_done(
                &config.conversation_id,
                &config.run_id,
                &config.message_id,
                "done",
                &fallback,
            );
            if !generated_api_messages.is_empty() {
                generated_api_messages.push(final_assistant_api_message(
                    &fallback,
                    extract_reasoning_content(&message).as_deref(),
                ));
            }
            (fallback, reasoning)
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
            );
            host.emit_stream_done(
                &config.conversation_id,
                &config.run_id,
                &config.message_id,
                "done",
                &response,
            );
            if !generated_api_messages.is_empty() {
                generated_api_messages.push(message);
            }
            (response, reasoning)
        }
    };

    steps.push(AgentStepResult {
        step_number,
        phase,
        response_messages: Vec::new(),
        tool_records: Vec::new(),
        streamed: config.stream_enabled,
        stop_reason: Some(AgentStopReason::Natural),
    });

    Ok(AgentRunResult {
        content: response,
        reasoning,
        tool_records,
        api_messages: generated_api_messages,
        steps,
    })
}

async fn call_chat_completion_message(
    state: &crate::state::AppState,
    provider: &crate::settings::ModelProvider,
    model: &str,
    messages: Vec<Value>,
    tools: Option<&[ChatToolDefinition]>,
    retry_attempts: usize,
    thinking_enabled: bool,
    label: &str,
) -> Result<Value, String> {
    let request = generate_request_from_openai_messages(
        model,
        messages,
        tools,
        GenerateOptions {
            thinking_enabled,
            ..GenerateOptions::default()
        },
        label,
    );
    let output = generate_with_chat_provider(state, provider, retry_attempts, request).await?;
    Ok(output.to_openai_compatible_message())
}

#[allow(clippy::too_many_arguments)]
async fn stream_scoped_chat_completion_inner(
    state: &crate::state::AppState,
    host: &dyn AgentHost,
    provider: &crate::settings::ModelProvider,
    model: &str,
    messages: Vec<Value>,
    tools: Option<&[ChatToolDefinition]>,
    retry_attempts: usize,
    thinking_enabled: bool,
    conversation_id: &str,
    run_id: &str,
    message_id: &str,
    generation: u64,
    label: &str,
    policy: AgentStreamPolicy,
) -> Result<ChatStreamOutput, String> {
    let request = generate_request_from_openai_messages(
        model,
        messages,
        tools,
        GenerateOptions {
            stream: true,
            thinking_enabled,
            ..GenerateOptions::default()
        },
        label,
    );
    let mut sink = AgentStreamSink::new(
        host,
        conversation_id,
        run_id,
        message_id,
        matches!(policy, AgentStreamPolicy::PlanningNoDoneUntilNoTools),
    );
    let output = tokio::select! {
        result = stream_with_chat_provider(
            state,
            provider,
            retry_attempts,
            request,
            &mut sink,
        ) => result?,
        _ = host.wait_for_generation_inactive(conversation_id, generation) => {
            let (content, reasoning) = sink.snapshot();
            host.emit_stream_done(
                conversation_id,
                run_id,
                message_id,
                "cancelled",
                content.trim(),
            );
            return Ok(ChatStreamOutput::new(
                content.trim().to_string(),
                reasoning.trim().to_string(),
                true,
            ));
        }
    };
    let (snapshot_content, snapshot_reasoning) = sink.snapshot();
    let stream_output = ChatStreamOutput::from_generate_output_with_snapshot(
        output,
        snapshot_content,
        snapshot_reasoning,
    );
    validate_stream_output(label, policy, &stream_output).map_err(|err| {
        host.emit_stream_done(conversation_id, run_id, message_id, "error", "");
        err
    })?;
    if should_emit_done(policy, &stream_output) {
        sink.flush_pending_text();
        host.emit_stream_done(
            conversation_id,
            run_id,
            message_id,
            "done",
            &stream_output.content,
        );
    }
    Ok(stream_output)
}

fn empty_synthesis_fallback_response(language: &str) -> String {
    if language.starts_with("zh") {
        "工具调用已经完成，但模型没有返回最终总结。上方工具结果已保存在本轮回复中，你可以继续追问，或让我重新生成总结。".to_string()
    } else {
        "The tool calls completed, but the model did not return a final summary. The tool results above were saved with this reply; you can continue from them or regenerate the summary.".to_string()
    }
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

async fn generate_with_chat_provider(
    state: &crate::state::AppState,
    provider: &crate::settings::ModelProvider,
    retry_attempts: usize,
    request: crate::chat::model::GenerateRequest,
) -> Result<GenerateOutput, String> {
    match provider.api_format_kind() {
        ProviderApiFormat::OpenAiChat => {
            OpenAiChatProvider::new(state, provider, retry_attempts)
                .generate(request)
                .await
        }
        ProviderApiFormat::AnthropicMessages => {
            AnthropicMessagesProvider::new(state, provider, retry_attempts)
                .generate(request)
                .await
        }
        ProviderApiFormat::AppleLocal => {
            AppleLocalProvider::new(state.apple_intelligence.clone())
                .generate(request)
                .await
        }
    }
    .map_err(|err| err.to_string())
}

async fn stream_with_chat_provider(
    state: &crate::state::AppState,
    provider: &crate::settings::ModelProvider,
    retry_attempts: usize,
    request: crate::chat::model::GenerateRequest,
    sink: &mut (dyn crate::chat::model::StreamSink + Send),
) -> Result<GenerateOutput, String> {
    match provider.api_format_kind() {
        ProviderApiFormat::OpenAiChat => {
            OpenAiChatProvider::new(state, provider, retry_attempts)
                .stream(request, sink)
                .await
        }
        ProviderApiFormat::AnthropicMessages => {
            AnthropicMessagesProvider::new(state, provider, retry_attempts)
                .stream(request, sink)
                .await
        }
        ProviderApiFormat::AppleLocal => {
            AppleLocalProvider::new(state.apple_intelligence.clone())
                .stream(request, sink)
                .await
        }
    }
    .map_err(|err| err.to_string())
}
