use std::{future::Future, pin::Pin};

use crate::chat::types::ToolCallRecord;

use super::execute::ToolExecutionContext;

pub type AgentHostFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait AgentHost: Send + Sync {
    fn emit_stream_delta(
        &self,
        conversation_id: &str,
        run_id: &str,
        message_id: &str,
        delta: &str,
        reasoning_delta: Option<&str>,
    );

    fn emit_stream_done(
        &self,
        conversation_id: &str,
        run_id: &str,
        message_id: &str,
        reason: &str,
        full: &str,
    );

    fn emit_tool_record(
        &self,
        conversation_id: &str,
        run_id: &str,
        message_id: &str,
        record: &ToolCallRecord,
    );

    fn request_tool_approval<'a>(
        &'a self,
        ctx: &'a ToolExecutionContext<'a>,
        record: &'a ToolCallRecord,
    ) -> AgentHostFuture<'a, bool>;

    fn is_generation_active(&self, conversation_id: &str, generation: u64) -> bool;

    fn wait_for_generation_inactive<'a>(
        &'a self,
        conversation_id: &'a str,
        generation: u64,
    ) -> AgentHostFuture<'a, ()>;
}
