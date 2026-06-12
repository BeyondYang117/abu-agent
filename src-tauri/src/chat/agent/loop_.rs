use serde_json::Value;

use crate::chat::types::ToolCallRecord;
use crate::mcp::ChatToolDefinition;
use crate::skills;

use super::execute::ToolExecutor;
use super::finalize::{finalize_completed, finalize_planning_final, SegmentBuilder};
use super::host::AgentHost;
use super::planning::{planning_step, PlanningStepOutcome};
use super::rounds::{run_tool_round, ToolRoundOutcome};
use super::stop::patch_system_message;
use super::synthesis::{synthesis_step, SynthesisFlow};
use super::types::{AgentRunConfig, AgentRunResult, AgentStepResult};

/// Immutable per-run environment shared by every loop phase.
pub(crate) struct LoopEnv<'a> {
    pub(crate) config: &'a AgentRunConfig<'a>,
    pub(crate) host: &'a dyn AgentHost,
    pub(crate) executor: &'a dyn ToolExecutor,
}

impl LoopEnv<'_> {
    pub(crate) fn ids(&self) -> RunIds<'_> {
        RunIds {
            conversation_id: &self.config.conversation_id,
            run_id: &self.config.run_id,
            message_id: &self.config.message_id,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct RunIds<'a> {
    pub(crate) conversation_id: &'a str,
    pub(crate) run_id: &'a str,
    pub(crate) message_id: &'a str,
}

/// Mutable accumulators threaded through the loop phases.
pub(crate) struct RunState {
    pub(crate) runtime_messages: Vec<Value>,
    pub(crate) tools: Vec<ChatToolDefinition>,
    pub(crate) blocked_tool_calls: Vec<ChatToolDefinition>,
    pub(crate) generated_api_messages: Vec<Value>,
    pub(crate) tool_records: Vec<ToolCallRecord>,
    pub(crate) planning_reasoning_parts: Vec<String>,
    pub(crate) steps: Vec<AgentStepResult>,
    pub(crate) segment_builder: SegmentBuilder,
    pub(crate) step_number: u8,
    pub(crate) provider_tools_unsupported: bool,
    pub(crate) tried_skill_only_tools: bool,
    pub(crate) planning_final_message: Option<Value>,
    pub(crate) planning_final_streamed: bool,
    pub(crate) skill_cache: skills::SkillRunCache,
    /// 本轮全部模型调用（规划/合成/压缩摘要）的 usage 累计；provider 不报则保持 None。
    pub(crate) usage: Option<crate::chat::model::ModelUsage>,
}

impl RunState {
    /// 把单次模型调用的 usage 累加进本轮总账（None 入参不改变现状）。
    pub(crate) fn merge_usage(&mut self, next: Option<crate::chat::model::ModelUsage>) {
        let Some(next) = next else { return };
        let total = self.usage.get_or_insert_with(Default::default);
        let add = |slot: &mut Option<u64>, value: Option<u64>| {
            if let Some(value) = value {
                *slot = Some(slot.unwrap_or(0).saturating_add(value));
            }
        };
        add(&mut total.input_tokens, next.input_tokens);
        add(&mut total.output_tokens, next.output_tokens);
        add(&mut total.total_tokens, next.total_tokens);
        add(&mut total.cached_input_tokens, next.cached_input_tokens);
        add(
            &mut total.cache_creation_input_tokens,
            next.cache_creation_input_tokens,
        );
        add(&mut total.reasoning_tokens, next.reasoning_tokens);
    }
}

pub async fn run_agent_loop(
    mut config: AgentRunConfig<'_>,
    host: &dyn AgentHost,
    executor: &dyn ToolExecutor,
) -> Result<AgentRunResult, String> {
    let mut state = RunState {
        runtime_messages: std::mem::take(&mut config.runtime_messages),
        tools: std::mem::take(&mut config.tools),
        blocked_tool_calls: std::mem::take(&mut config.blocked_tool_calls),
        generated_api_messages: Vec::new(),
        tool_records: Vec::new(),
        planning_reasoning_parts: Vec::new(),
        steps: Vec::new(),
        segment_builder: SegmentBuilder::new(),
        step_number: 0,
        provider_tools_unsupported: false,
        tried_skill_only_tools: false,
        planning_final_message: None,
        planning_final_streamed: false,
        skill_cache: skills::SkillRunCache::default(),
        usage: None,
    };
    let env = LoopEnv {
        config: &config,
        host,
        executor,
    };

    if !state.tools.is_empty() {
        let mut round = 0u32;
        loop {
            round = round.saturating_add(1);
            state.step_number = state.step_number.saturating_add(1);
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

            let planned = match planning_step(&env, &mut state, round).await? {
                PlanningStepOutcome::FinalAnswer => break,
                PlanningStepOutcome::ToolsUnsupported => break,
                PlanningStepOutcome::RetryWithSkillTools => continue,
                PlanningStepOutcome::DraftFailed(result) => {
                    return Ok(attach_usage(result, &mut state))
                }
                PlanningStepOutcome::Cancelled(result) => {
                    return Ok(attach_usage(result, &mut state))
                }
                PlanningStepOutcome::ToolCalls(planned) => planned,
            };

            match run_tool_round(&env, &mut state, round, planned).await {
                ToolRoundOutcome::Continue => {}
                ToolRoundOutcome::RoundLimit => break,
                ToolRoundOutcome::Cancelled(result) => {
                    return Ok(attach_usage(result, &mut state))
                }
            }
        }
    }

    if state.provider_tools_unsupported {
        patch_system_message(
            &mut state.runtime_messages,
            &config.provider_tools_fallback_system_prompt,
        );
    }

    if let Some(message) = state.planning_final_message.take() {
        return finalize_planning_final(&env, &mut state, message)
            .map(|result| attach_usage(result, &mut state));
    }

    match synthesis_step(&env, &mut state).await? {
        SynthesisFlow::Early(result) => Ok(attach_usage(result, &mut state)),
        SynthesisFlow::Completed(completed) => {
            let result = finalize_completed(&env, &mut state, completed);
            Ok(attach_usage(result, &mut state))
        }
    }
}

/// 把本轮累计的 provider usage 挂到运行结果上（finalize 构造器们不感知 usage）。
fn attach_usage(mut result: AgentRunResult, state: &mut RunState) -> AgentRunResult {
    result.usage = state.usage.take();
    result
}

#[cfg(test)]
pub(crate) use super::execute::ToolExecutionContext;
#[cfg(test)]
pub(crate) use super::finalize::{
    cancelled_tool_round_run_result, empty_synthesis_fallback_response,
    stopped_generation_content, synthesis_failed_fallback_response,
    tool_planning_failed_fallback_response, tool_planning_failed_run_result,
};
#[cfg(test)]
pub(crate) use super::rounds::{
    execute_tool_round, tool_call_parallel_eligible, tool_message, tool_round_limit_reached,
    visible_tool_segment_calls, ToolRoundContext,
};
#[cfg(test)]
pub(crate) use super::stream::{AgentStreamSink, ToolCallDraftTracker};
#[cfg(test)]
pub(crate) use super::types::{AgentPhase, AgentStopReason};
#[cfg(test)]
pub(crate) use crate::chat::model::PendingToolCall;
#[cfg(test)]
pub(crate) use crate::chat::types::{
    ChatMessageSegment, ChatMessageSegmentKind, ChatMessageSegmentPhase,
};
#[cfg(test)]
pub(crate) use crate::settings::Settings;

#[cfg(test)]
#[path = "loop_tests.rs"]
mod tests;
