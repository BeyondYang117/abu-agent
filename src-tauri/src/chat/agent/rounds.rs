use serde_json::Value;

use crate::chat::model::PendingToolCall;
use crate::chat::types::{ToolCallRecord, ToolCallStatus};
use crate::mcp::ChatToolDefinition;
use crate::settings::Settings;
use crate::skills;

use super::execute::{
    disabled_tool_content, execute_tool_call, invalid_tool_arguments_record, match_tool_call,
    tool_requires_approval, unknown_tool_record, ToolExecutionContext, ToolExecutor,
};
use super::finalize::cancelled_tool_round_run_result;
use super::host::AgentHost;
use super::loop_::{LoopEnv, RunState};
use super::planning::PlannedToolRound;
use super::stop::{assistant_api_message_for_tool_calls, step_limit_system_message};
use super::types::{AgentPhase, AgentRunResult, AgentStepResult, AgentStopReason};

pub(crate) const MAX_PARALLEL_TOOL_CALLS_PER_ROUND: usize = 4;

pub(crate) enum ToolRoundOutcome {
    /// The round completed; the skeleton continues to the next planning round.
    Continue,
    /// The round limit was reached; the limit system message was injected and
    /// the last step's stop reason rewritten. The skeleton breaks out.
    RoundLimit,
    /// The round was cancelled mid-flight; done("cancelled") was emitted and a
    /// final result built. The skeleton returns it.
    Cancelled(AgentRunResult),
}

pub(crate) async fn run_tool_round(
    env: &LoopEnv<'_>,
    state: &mut RunState,
    round: u32,
    planned: PlannedToolRound,
) -> ToolRoundOutcome {
    let config = env.config;
    let host = env.host;
    let assistant_message =
        assistant_api_message_for_tool_calls(&planned.message, &planned.tool_calls);
    state.runtime_messages.push(assistant_message);
    state
        .generated_api_messages
        .push(state.runtime_messages.last().cloned().unwrap_or(Value::Null));
    let mut step_response_messages =
        vec![state.runtime_messages.last().cloned().unwrap_or(Value::Null)];
    let round_result = execute_tool_round(
        host,
        env.executor,
        &config.settings,
        ToolRoundContext {
            conversation_id: &config.conversation_id,
            run_id: &config.run_id,
            message_id: &config.message_id,
            generation: config.generation,
            round,
            depth: config.depth,
            tool_conversation_id: &config.tool_conversation_id,
        },
        &state.tools,
        &state.blocked_tool_calls,
        planned.tool_calls,
        &mut state.skill_cache,
    )
    .await;
    let round_cancelled = round_result.cancelled;
    state
        .runtime_messages
        .extend(round_result.response_messages.iter().cloned());
    state
        .generated_api_messages
        .extend(round_result.response_messages.iter().cloned());
    step_response_messages.extend(round_result.response_messages);
    let step_tool_records = round_result.tool_records;
    state.tool_records.extend(step_tool_records.iter().cloned());
    state.steps.push(AgentStepResult {
        step_number: state.step_number,
        phase: AgentPhase::ToolLoop,
        response_messages: step_response_messages,
        tool_records: step_tool_records,
        segments: planned.step_segments,
        streamed: config.stream_enabled,
        stop_reason: if round_cancelled {
            Some(AgentStopReason::Cancelled)
        } else {
            None
        },
    });
    if round_cancelled {
        host.emit_stream_done(
            &config.conversation_id,
            &config.run_id,
            &config.message_id,
            "cancelled",
            "",
        );
        return ToolRoundOutcome::Cancelled(cancelled_tool_round_run_result(
            &config.language,
            &state.planning_reasoning_parts,
            std::mem::take(&mut state.tool_records),
            std::mem::take(&mut state.segment_builder).all(),
            std::mem::take(&mut state.generated_api_messages),
            std::mem::take(&mut state.steps),
        ));
    }
    if tool_round_limit_reached(config.effective_chat_tools.max_tool_rounds, round) {
        state.runtime_messages.push(step_limit_system_message());
        if let Some(last_step) = state.steps.last_mut() {
            last_step.stop_reason = Some(AgentStopReason::StepLimit);
        }
        return ToolRoundOutcome::RoundLimit;
    }
    ToolRoundOutcome::Continue
}

pub(crate) struct ToolRoundContext<'a> {
    pub(crate) conversation_id: &'a str,
    pub(crate) run_id: &'a str,
    pub(crate) message_id: &'a str,
    pub(crate) generation: u64,
    pub(crate) round: u32,
    pub(crate) depth: u8,
    pub(crate) tool_conversation_id: &'a str,
}

pub(crate) struct ToolRoundResult {
    pub(crate) response_messages: Vec<Value>,
    pub(crate) tool_records: Vec<ToolCallRecord>,
    pub(crate) cancelled: bool,
}

struct ToolExecutionResult {
    response_message: Value,
    record: Option<ToolCallRecord>,
    cancelled: bool,
}

struct ExecutableToolCall<'a> {
    call: PendingToolCall,
    tool: &'a ChatToolDefinition,
}

pub(crate) fn visible_tool_segment_calls(
    tools: &[ChatToolDefinition],
    blocked_tool_calls: &[ChatToolDefinition],
    tool_calls: &[PendingToolCall],
) -> Vec<PendingToolCall> {
    tool_calls
        .iter()
        .filter(|call| {
            match_tool_call(tools, &call.function_name).is_some()
                || match_tool_call(blocked_tool_calls, &call.function_name).is_some()
                || disabled_tool_content(call).is_none()
        })
        .cloned()
        .collect()
}

pub(crate) fn tool_round_limit_reached(max_tool_rounds: Option<u32>, round: u32) -> bool {
    max_tool_rounds.is_some_and(|limit| round >= limit)
}

pub(crate) async fn execute_tool_round(
    host: &dyn AgentHost,
    executor: &dyn ToolExecutor,
    settings: &Settings,
    ctx: ToolRoundContext<'_>,
    tools: &[ChatToolDefinition],
    blocked_tool_calls: &[ChatToolDefinition],
    tool_calls: Vec<PendingToolCall>,
    skill_cache: &mut skills::SkillRunCache,
) -> ToolRoundResult {
    let mut response_messages = Vec::new();
    let mut tool_records = Vec::new();
    let mut parallel_batch: Vec<ExecutableToolCall<'_>> = Vec::new();
    let mut cancelled = false;

    let mut tool_calls = tool_calls.into_iter();
    while let Some(tool_call) = tool_calls.next() {
        let Some(tool) = match_tool_call(tools, &tool_call.function_name) else {
            if flush_parallel_batch(
                &mut parallel_batch,
                &mut response_messages,
                &mut tool_records,
                host,
                executor,
                settings,
                &ctx,
            )
            .await
            {
                cancelled = true;
                push_cancelled_tool_call(
                    host,
                    &ctx,
                    tools,
                    tool_call,
                    &mut response_messages,
                    &mut tool_records,
                );
                push_cancelled_tool_calls(
                    host,
                    &ctx,
                    tools,
                    tool_calls,
                    &mut response_messages,
                    &mut tool_records,
                );
                break;
            }
            let result = unknown_or_disabled_tool_result(host, &ctx, blocked_tool_calls, tool_call);
            push_tool_execution_result(result, &mut response_messages, &mut tool_records);
            continue;
        };

        if let Some(error) = tool_call.arguments_parse_error.clone() {
            if flush_parallel_batch(
                &mut parallel_batch,
                &mut response_messages,
                &mut tool_records,
                host,
                executor,
                settings,
                &ctx,
            )
            .await
            {
                cancelled = true;
                push_cancelled_tool_call(
                    host,
                    &ctx,
                    tools,
                    tool_call,
                    &mut response_messages,
                    &mut tool_records,
                );
                push_cancelled_tool_calls(
                    host,
                    &ctx,
                    tools,
                    tool_calls,
                    &mut response_messages,
                    &mut tool_records,
                );
                break;
            }
            let result = invalid_tool_arguments_result(host, &ctx, &tool_call, tool, error);
            push_tool_execution_result(result, &mut response_messages, &mut tool_records);
            continue;
        }

        if tool_call_parallel_eligible(settings, tool) {
            parallel_batch.push(ExecutableToolCall {
                call: tool_call,
                tool,
            });
            if parallel_batch.len() >= MAX_PARALLEL_TOOL_CALLS_PER_ROUND {
                if flush_parallel_batch(
                    &mut parallel_batch,
                    &mut response_messages,
                    &mut tool_records,
                    host,
                    executor,
                    settings,
                    &ctx,
                )
                .await
                {
                    cancelled = true;
                    push_cancelled_tool_calls(
                        host,
                        &ctx,
                        tools,
                        tool_calls,
                        &mut response_messages,
                        &mut tool_records,
                    );
                    break;
                }
            }
            continue;
        }

        if flush_parallel_batch(
            &mut parallel_batch,
            &mut response_messages,
            &mut tool_records,
            host,
            executor,
            settings,
            &ctx,
        )
        .await
        {
            cancelled = true;
            push_cancelled_tool_call(
                host,
                &ctx,
                tools,
                tool_call,
                &mut response_messages,
                &mut tool_records,
            );
            push_cancelled_tool_calls(
                host,
                &ctx,
                tools,
                tool_calls,
                &mut response_messages,
                &mut tool_records,
            );
            break;
        }
        let result =
            execute_serial_tool_call(host, executor, settings, &ctx, tool, tool_call, skill_cache)
                .await;
        if push_tool_execution_result(result, &mut response_messages, &mut tool_records) {
            cancelled = true;
            push_cancelled_tool_calls(
                host,
                &ctx,
                tools,
                tool_calls,
                &mut response_messages,
                &mut tool_records,
            );
            break;
        }
    }

    if !cancelled {
        cancelled = flush_parallel_batch(
            &mut parallel_batch,
            &mut response_messages,
            &mut tool_records,
            host,
            executor,
            settings,
            &ctx,
        )
        .await;
    }

    ToolRoundResult {
        response_messages,
        tool_records,
        cancelled,
    }
}

fn push_tool_execution_result(
    result: ToolExecutionResult,
    response_messages: &mut Vec<Value>,
    tool_records: &mut Vec<ToolCallRecord>,
) -> bool {
    let cancelled = result.cancelled;
    if let Some(record) = result.record {
        tool_records.push(record);
    }
    response_messages.push(result.response_message);
    cancelled
}

async fn flush_parallel_batch(
    batch: &mut Vec<ExecutableToolCall<'_>>,
    response_messages: &mut Vec<Value>,
    tool_records: &mut Vec<ToolCallRecord>,
    host: &dyn AgentHost,
    executor: &dyn ToolExecutor,
    settings: &Settings,
    ctx: &ToolRoundContext<'_>,
) -> bool {
    if batch.is_empty() {
        return false;
    }

    let mut cancelled = false;
    while !batch.is_empty() {
        let limit = batch.len().min(MAX_PARALLEL_TOOL_CALLS_PER_ROUND);
        let mut chunk = batch.drain(..limit).collect::<Vec<_>>();
        let results = execute_parallel_chunk(host, executor, settings, ctx, &mut chunk).await;
        for result in results {
            cancelled |= push_tool_execution_result(result, response_messages, tool_records);
        }
        if cancelled {
            for item in batch.drain(..) {
                let result = cancelled_tool_result(host, ctx, &item.call, Some(item.tool));
                push_tool_execution_result(result, response_messages, tool_records);
            }
            break;
        }
    }
    cancelled
}

async fn execute_parallel_chunk(
    host: &dyn AgentHost,
    executor: &dyn ToolExecutor,
    settings: &Settings,
    ctx: &ToolRoundContext<'_>,
    chunk: &mut [ExecutableToolCall<'_>],
) -> Vec<ToolExecutionResult> {
    match chunk.len() {
        0 => Vec::new(),
        1 => {
            let item = &chunk[0];
            vec![execute_parallel_tool_call(host, executor, settings, ctx, item).await]
        }
        2 => {
            let (a, rest) = chunk.split_at(1);
            let (b, _) = rest.split_at(1);
            let (ra, rb) = tokio::join!(
                execute_parallel_tool_call(host, executor, settings, ctx, &a[0]),
                execute_parallel_tool_call(host, executor, settings, ctx, &b[0]),
            );
            vec![ra, rb]
        }
        3 => {
            let (a, rest) = chunk.split_at(1);
            let (b, rest) = rest.split_at(1);
            let (c, _) = rest.split_at(1);
            let (ra, rb, rc) = tokio::join!(
                execute_parallel_tool_call(host, executor, settings, ctx, &a[0]),
                execute_parallel_tool_call(host, executor, settings, ctx, &b[0]),
                execute_parallel_tool_call(host, executor, settings, ctx, &c[0]),
            );
            vec![ra, rb, rc]
        }
        _ => {
            let (a, rest) = chunk.split_at(1);
            let (b, rest) = rest.split_at(1);
            let (c, rest) = rest.split_at(1);
            let (d, _) = rest.split_at(1);
            let (ra, rb, rc, rd) = tokio::join!(
                execute_parallel_tool_call(host, executor, settings, ctx, &a[0]),
                execute_parallel_tool_call(host, executor, settings, ctx, &b[0]),
                execute_parallel_tool_call(host, executor, settings, ctx, &c[0]),
                execute_parallel_tool_call(host, executor, settings, ctx, &d[0]),
            );
            vec![ra, rb, rc, rd]
        }
    }
}

async fn execute_parallel_tool_call(
    host: &dyn AgentHost,
    executor: &dyn ToolExecutor,
    settings: &Settings,
    ctx: &ToolRoundContext<'_>,
    item: &ExecutableToolCall<'_>,
) -> ToolExecutionResult {
    execute_tool_call_result(
        host,
        executor,
        settings,
        ctx,
        item.tool,
        item.call.clone(),
        None,
    )
    .await
}

async fn execute_serial_tool_call(
    host: &dyn AgentHost,
    executor: &dyn ToolExecutor,
    settings: &Settings,
    ctx: &ToolRoundContext<'_>,
    tool: &ChatToolDefinition,
    tool_call: PendingToolCall,
    skill_cache: &mut skills::SkillRunCache,
) -> ToolExecutionResult {
    execute_tool_call_result(
        host,
        executor,
        settings,
        ctx,
        tool,
        tool_call,
        Some(skill_cache),
    )
    .await
}

async fn execute_tool_call_result(
    host: &dyn AgentHost,
    executor: &dyn ToolExecutor,
    settings: &Settings,
    round_ctx: &ToolRoundContext<'_>,
    tool: &ChatToolDefinition,
    tool_call: PendingToolCall,
    skill_cache: Option<&mut skills::SkillRunCache>,
) -> ToolExecutionResult {
    let tool_call_id = tool_call.id.clone();
    let execution_ctx = ToolExecutionContext {
        conversation_id: round_ctx.conversation_id,
        run_id: round_ctx.run_id,
        message_id: round_ctx.message_id,
        generation: round_ctx.generation,
        round: round_ctx.round,
        depth: round_ctx.depth,
        tool_conversation_id: round_ctx.tool_conversation_id,
        tool_call_id: &tool_call_id,
    };
    let (record, tool_content) = execute_tool_call(
        host,
        executor,
        settings,
        &execution_ctx,
        tool,
        tool_call,
        skill_cache,
    )
    .await;
    let cancelled = matches!(record.status, ToolCallStatus::Cancelled);
    ToolExecutionResult {
        response_message: tool_message(tool_call_id, tool_content),
        record: Some(record),
        cancelled,
    }
}

fn push_cancelled_tool_calls(
    host: &dyn AgentHost,
    ctx: &ToolRoundContext<'_>,
    tools: &[ChatToolDefinition],
    tool_calls: impl IntoIterator<Item = PendingToolCall>,
    response_messages: &mut Vec<Value>,
    tool_records: &mut Vec<ToolCallRecord>,
) {
    for tool_call in tool_calls {
        push_cancelled_tool_call(host, ctx, tools, tool_call, response_messages, tool_records);
    }
}

fn push_cancelled_tool_call(
    host: &dyn AgentHost,
    ctx: &ToolRoundContext<'_>,
    tools: &[ChatToolDefinition],
    tool_call: PendingToolCall,
    response_messages: &mut Vec<Value>,
    tool_records: &mut Vec<ToolCallRecord>,
) {
    let tool = match_tool_call(tools, &tool_call.function_name);
    let result = cancelled_tool_result(host, ctx, &tool_call, tool);
    push_tool_execution_result(result, response_messages, tool_records);
}

fn cancelled_tool_result(
    host: &dyn AgentHost,
    ctx: &ToolRoundContext<'_>,
    tool_call: &PendingToolCall,
    tool: Option<&ChatToolDefinition>,
) -> ToolExecutionResult {
    let now = chrono::Local::now().timestamp();
    let record = ToolCallRecord {
        id: tool_call.id.clone(),
        name: tool
            .map(|tool| tool.name.clone())
            .unwrap_or_else(|| tool_call.function_name.clone()),
        source: tool
            .map(|tool| tool.source.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        server_id: tool.and_then(|tool| tool.server_id.clone()),
        arguments: tool_call.arguments_raw.clone(),
        status: ToolCallStatus::Cancelled,
        result_preview: None,
        error: Some("Tool call cancelled".to_string()),
        duration_ms: Some(0),
        started_at: Some(now),
        completed_at: Some(now),
        round: ctx.round,
        sensitive: tool.map(|tool| tool.sensitive).unwrap_or(false),
        artifacts: Vec::new(),
        trace_id: Some(ctx.run_id.to_string()),
        span_id: Some(tool_span_id(ctx, &tool_call.id)),
        structured_content: None,
    };
    host.emit_tool_record(ctx.conversation_id, ctx.run_id, ctx.message_id, &record);
    ToolExecutionResult {
        response_message: tool_message(tool_call.id.clone(), "Tool call cancelled"),
        record: Some(record),
        cancelled: true,
    }
}

fn unknown_or_disabled_tool_result(
    host: &dyn AgentHost,
    ctx: &ToolRoundContext<'_>,
    blocked_tool_calls: &[ChatToolDefinition],
    tool_call: PendingToolCall,
) -> ToolExecutionResult {
    if let Some(tool) = match_tool_call(blocked_tool_calls, &tool_call.function_name) {
        let error = format!(
            "Tool `{}` is blocked in Plan mode. Switch to Act / execute the plan before using side-effecting tools.",
            tool.openai_tool_name()
        );
        let mut record = skipped_tool_record(ctx, &tool_call, tool, error.clone());
        attach_tool_trace(ctx, &mut record);
        host.emit_tool_record(ctx.conversation_id, ctx.run_id, ctx.message_id, &record);
        return ToolExecutionResult {
            response_message: tool_message(tool_call.id, error),
            record: Some(record),
            cancelled: false,
        };
    }

    let disabled = disabled_tool_content(&tool_call);
    let record = if disabled.is_none() {
        let error = format!("Unknown tool requested: {}", tool_call.function_name);
        let mut record = unknown_tool_record(&tool_call, ctx.round, error);
        attach_tool_trace(ctx, &mut record);
        host.emit_tool_record(ctx.conversation_id, ctx.run_id, ctx.message_id, &record);
        Some(record)
    } else {
        None
    };
    let content =
        disabled.unwrap_or_else(|| format!("Unknown tool requested: {}", tool_call.function_name));
    ToolExecutionResult {
        response_message: tool_message(tool_call.id, content),
        record,
        cancelled: false,
    }
}

fn skipped_tool_record(
    ctx: &ToolRoundContext<'_>,
    call: &PendingToolCall,
    tool: &ChatToolDefinition,
    error: String,
) -> ToolCallRecord {
    let now = chrono::Local::now().timestamp();
    ToolCallRecord {
        id: call.id.clone(),
        name: tool.name.clone(),
        source: tool.source.clone(),
        server_id: tool.server_id.clone(),
        arguments: call.arguments_raw.clone(),
        status: ToolCallStatus::Skipped,
        result_preview: None,
        error: Some(error),
        duration_ms: Some(0),
        started_at: Some(now),
        completed_at: Some(now),
        round: ctx.round,
        sensitive: tool.sensitive,
        artifacts: Vec::new(),
        trace_id: None,
        span_id: None,
        structured_content: None,
    }
}

fn invalid_tool_arguments_result(
    host: &dyn AgentHost,
    ctx: &ToolRoundContext<'_>,
    tool_call: &PendingToolCall,
    tool: &ChatToolDefinition,
    error: String,
) -> ToolExecutionResult {
    let mut record = invalid_tool_arguments_record(tool_call, tool, ctx.round, error);
    attach_tool_trace(ctx, &mut record);
    host.emit_tool_record(ctx.conversation_id, ctx.run_id, ctx.message_id, &record);
    ToolExecutionResult {
        response_message: tool_message(
            tool_call.id.clone(),
            "Tool arguments JSON is invalid or incomplete. Retry this tool call with a compact, valid JSON object for arguments.",
        ),
        record: Some(record),
        cancelled: false,
    }
}

fn attach_tool_trace(ctx: &ToolRoundContext<'_>, record: &mut ToolCallRecord) {
    record.trace_id = Some(ctx.run_id.to_string());
    record.span_id = Some(tool_span_id(ctx, &record.id));
}

fn tool_span_id(ctx: &ToolRoundContext<'_>, tool_call_id: &str) -> String {
    format!("tool_round_{}_{}", ctx.round, tool_call_id)
}

pub(crate) fn tool_message(tool_call_id: String, content: impl Into<String>) -> Value {
    serde_json::json!({
        "role": "tool",
        "tool_call_id": tool_call_id,
        "content": content.into(),
    })
}

pub(crate) fn tool_call_parallel_eligible(settings: &Settings, tool: &ChatToolDefinition) -> bool {
    if tool_requires_approval(settings, tool) {
        return false;
    }
    if tool.source == "native" {
        // The native parallel-safe set is intentionally narrow (see
        // agent-runtime spec); it lives in the static registry.
        return crate::mcp::native_registry::find_entry(&tool.name)
            .is_some_and(|entry| entry.parallel_safe);
    }
    tool.source == "mcp" && tool.is_read_only_tool()
}
