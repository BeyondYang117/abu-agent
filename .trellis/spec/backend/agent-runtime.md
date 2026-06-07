# Chat Agent Runtime

## Scenario: Per-Round Tool Scheduling

### 1. Scope / Trigger

- Trigger: changes under `src-tauri/src/chat/agent/**` that alter how model-emitted tool calls are matched, executed, recorded, or replayed.
- The Chat agent loop is a Rust-native model-step loop. Provider adapters may parse multiple tool calls from one assistant response, but local execution concurrency is controlled by the runtime scheduler.

### 2. Signatures

- `run_agent_loop(config, host, executor) -> Result<AgentRunResult, String>`
- `execute_tool_call(host, executor, settings, ctx, tool, call, skill_cache) -> (ToolCallRecord, String)`
- `validate_tool_arguments(tool, arguments) -> Result<(), String>`
- `ToolExecutor::call(ctx, tool, arguments, skill_cache) -> ToolExecutorFuture`
- `skill_cache` is optional so non-skill tools can run without borrowing the per-run `SkillRunCache`.
- `ChatToolDefinition` must carry `input_schema`, optional MCP `annotations`, optional MCP `output_schema`, and `sensitive`.
- `ToolCallRecord` must carry lifecycle fields plus optional `trace_id`, `span_id`, and `structured_content`.

### 3. Contracts

- Record one assistant message containing all requested `tool_calls` before appending any tool result messages.
- Append generated tool result messages as OpenAI-compatible objects:

```json
{ "role": "tool", "tool_call_id": "<original id>", "content": "<tool output>" }
```

- Tool result messages must remain in the same order as the model's original `PendingToolCall` list, even if execution completes out of order.
- Every executable tool still emits lifecycle records through `AgentHost::emit_tool_record`: pending, running, then final success/error/skipped/cancelled.
- Validate every executable tool call against `ChatToolDefinition.input_schema` before approval and before `ToolExecutor::call`. Validation failure returns an error tool result and must not ask for approval or invoke the executor.
- Approval-gated tools must be serial. Do not start execution before `AgentHost::request_tool_approval` resolves.
- The native parallel-safe set is intentionally narrow: `native:web_search`, `native:web_fetch`, and `native:read_file`, and only when `tool_requires_approval` returns false.
- MCP tools are serial by default. A MCP tool may join a parallel batch only when it has explicit `annotations.readOnlyHint == true`, no `destructiveHint == true`, and `tool_requires_approval` returns false.
- MCP approval/sensitivity must prefer tool metadata over name guessing: `destructiveHint == true`, `openWorldHint == true`, or `readOnlyHint == false` imply sensitive/confirmation behavior under confirm policies; `readOnlyHint == true` allows auto-approval for trusted non-sensitive tools. User-selected `approval_policy == "auto"` still bypasses approval prompts but must not make non-read-only MCP tools parallel.
- Preserve MCP metadata across all backend/frontend boundaries: `annotations`, `outputSchema`, and tool result `structuredContent` must not be dropped. When a MCP result includes `structuredContent`, persist it on `ToolCallRecord` and include it in the model-facing tool content unless the text result already contains the same JSON.
- Tool records emitted from the agent loop should include `trace_id = run_id` and a deterministic `span_id` such as `tool_round_<round>_<tool_call_id>` so future tracing/export can correlate events without changing storage shape.
- Serial by default: writes/edits, command execution, `run_python`, Skill runtime tools, Mixer image generation, memory mutation, arbitrary MCP tools, unknown calls, and invalid arguments.
- Keep `SkillRunCache` on the serial path unless it is redesigned as a shared concurrency-safe cache with tests.
- Keep timeout and cancellation inside `execute_tool_call`; schedulers should call this helper rather than duplicating lifecycle logic.
- If generation is cancelled during a tool round, stop launching any unstarted calls in that round. Append ordered `cancelled` tool result messages and records for every unstarted call so provider replay remains valid.
- A cancelled tool round that already produced tool transcript messages should return an `AgentRunResult` with stopped content instead of bubbling `Err("cancelled")`, allowing the assistant message and tool records to persist.

### 4. Validation & Error Matrix

| Condition | Runtime behavior |
|---|---|
| Unknown enabled tool name | Emit an error `ToolCallRecord`; append a matching `role: tool` error message. |
| Disabled built-in requested through fallback markup | Append hidden model feedback; do not emit a visible tool record. |
| Invalid tool argument JSON | Emit an error `ToolCallRecord`; append retry guidance as the tool result; do not request approval or call the executor. |
| Tool arguments violate declared schema | Emit an error `ToolCallRecord`; append schema retry guidance; do not request approval or call the executor. |
| MCP `annotations.readOnlyHint == true` and trusted/non-sensitive | May skip approval under confirm policies and may parallelize if no other risk hints are present. |
| MCP `destructiveHint == true`, `openWorldHint == true`, or `readOnlyHint == false` | Treat as sensitive under confirm policies; keep serial even if approval is skipped by `"auto"`. |
| Tool requires approval | Execute serially after approval; skipped result if denied. |
| MCP result includes `structuredContent` | Preserve it on the tool record, emit it through `chat-tool`, and include it in replay content without duplicating identical text JSON. |
| Generation cancelled while a tool is running | Mark active and unstarted tool records cancelled where possible, append matching tool result messages in original order, and stop launching remaining calls. |
| Tool timeout | Mark the tool record error and return the timeout message as tool content. |

### 5. Good/Base/Bad Cases

- Good: a model emits `read_file` and `web_fetch` in one round; both enter running state before either finishes, but replay messages preserve model order.
- Good: a trusted MCP server exposes two tools with `readOnlyHint: true`; both may overlap, and their `structuredContent` remains visible in records/events/model replay.
- Base: a model emits only `run_python`; calls execute one at a time and keep old lifecycle behavior.
- Bad: a scheduler parallelizes `skill_activate` or an arbitrary MCP stdio tool without explicit read-only annotations and races shared state or external side effects.
- Bad: schema validation happens inside one executor implementation only; other executors can still receive invalid arguments or approval prompts can show invalid payloads.

### 6. Tests Required

- Prove two eligible tools overlap by recording start/finish events.
- Prove explicitly read-only MCP tools overlap while destructive/open-world/non-read-only MCP tools remain serial.
- Prove returned `response_messages` and persisted `tool_records` follow original call order.
- Prove schema-invalid arguments produce error records and never call the executor or approval hook.
- Prove MCP `annotations`, `outputSchema`, and result `structuredContent` survive parse/registry/command/TypeScript boundaries.
- Prove serial-only tools never overlap.
- Prove unknown and invalid calls flush pending parallel batches and preserve result ordering.
- Run `cargo test --manifest-path src-tauri/Cargo.toml chat::agent:: -- --nocapture` for targeted changes.
- Run `cargo test --manifest-path src-tauri/Cargo.toml` before completion when practical.

### 7. Wrong vs Correct

#### Wrong

```rust
for call in tool_calls {
    tokio::spawn(execute_any_tool(call));
}
```

This loses transcript order, approval sequencing, cache safety, and cancellation ownership.

```rust
request_tool_approval(ctx, record).await;
validate_tool_arguments(tool, &call.arguments)?;
```

This can ask the user to approve a payload that will never execute and makes guardrail behavior inconsistent.

#### Correct

```rust
// Validate first, classify next, run only explicitly safe read-only tools together,
// then append all result messages in original model-call order.
```

Keep provider-side multiple tool-call support separate from local execution concurrency.
