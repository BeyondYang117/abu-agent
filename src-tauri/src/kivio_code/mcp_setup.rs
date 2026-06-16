//! MCP integration seam for kivio-code (stub — implemented in a later pass).
//!
//! This module owns ALL MCP wiring for the headless CLI so MCP support can be
//! built out without touching `kivio_code/mod.rs`. `mod.rs` only calls
//! [`collect_mcp_tools`] at startup and [`dispatch_mcp_tool`] from the executor;
//! everything else (server connection, lifecycle, tool naming) lives here.
use crate::mcp::types::McpToolCallResult;
use crate::mcp::ChatToolDefinition;
use crate::settings::Settings;
use crate::state::AppState;
use serde_json::Value;

/// Connect configured MCP servers and collect their tool definitions. STUB:
/// returns empty until MCP is wired up.
pub async fn collect_mcp_tools(
    _state: &AppState,
    _settings: &Settings,
) -> Vec<ChatToolDefinition> {
    Vec::new()
}

/// Dispatch a tool call to its MCP server. STUB: returns `Err` (no MCP wired
/// yet). Returns `Ok(result)` when the tool was handled by an MCP server,
/// `Err(msg)` so the executor can fall through to its unknown-tool error.
pub async fn dispatch_mcp_tool(
    _state: &AppState,
    _tool: &ChatToolDefinition,
    _arguments: &Value,
) -> Result<McpToolCallResult, String> {
    Err("MCP not yet integrated".to_string())
}
