# Chat Agent Runtime

> Module layout (since 2026-06-12 split, zero behavior change): `loop_.rs` is a thin orchestration skeleton (`LoopEnv`/`RunState` + prepare → planning → tool rounds → synthesis → finalize). Planning + provider-call plumbing live in `planning.rs`, tool round scheduling in `rounds.rs`, synthesis paths in `synthesis.rs`, and `RunResultBuilder` + fallback copy + `SegmentBuilder` in `finalize.rs`. Loop tests live in `loop_tests.rs` (mounted via `#[path]` from `loop_.rs`). All contracts below are unchanged by the split; `run_agent_loop` remains the single public entry. Fallback behavior (synthesis failure/cancel/empty, planning draft failure) is pinned by regression tests in `loop_tests.rs` — keep them green and assertion-identical through refactors.

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
- Provider-streamed tool call chunks are real runtime state. When `ToolCallStart` arrives during tool planning, the backend must emit a backend-owned tool segment plus a pending `ToolCallRecord`; as argument deltas arrive it may update the same record with a compact progress preview, and when `ToolCallDone` arrives normal tool execution should continue with the same `tool_call_id`.
- Serial by default: writes/edits, command execution, `run_python`, Skill runtime tools, Mixer image generation, memory mutation, arbitrary MCP tools, unknown calls, and invalid arguments.
- `ask_user` is a native blocking clarification tool. It is allowed in Plan mode, bypasses sensitive tool approval, must stay serial, and must flush any pending parallel batch before waiting for the user's answer.
- `ask_user` must persist `ToolCallRecord.structured_content.askUser` with `phase`, original `questions`, and final `answers`; emit `chat-user-prompt` while awaiting; resolve through `chat_submit_user_choice`; and append a stable matching `role: tool` result using the original `tool_call_id`.
- Cancelling or timing out an awaiting `ask_user` must remove the pending prompt entry and produce a deterministic `cancelled` or `timeout` tool result so provider replay remains valid.
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
| `ask_user` awaiting answer | Emit inline prompt state, block the same run until answer/skip/cancel/timeout, then append the answer JSON as the tool result. |
| Generation cancelled while a tool is running | Mark active and unstarted tool records cancelled where possible, append matching tool result messages in original order, and stop launching remaining calls. |
| Tool timeout | Mark the tool record error and return the timeout message as tool content. |
| Provider stream fails after a visible tool-call draft starts but before executable arguments are complete | Mark the draft `ToolCallRecord` error, preserve its tool segment, and return an `AgentRunResult` with `stream_outcome = "error"` instead of bubbling an invoke error that clears the conversation turn. |

### 5. Good/Base/Bad Cases

- Good: a model emits `read_file` and `web_fetch` in one round; both enter running state before either finishes, but replay messages preserve model order.
- Good: a trusted MCP server exposes two tools with `readOnlyHint: true`; both may overlap, and their `structuredContent` remains visible in records/events/model replay.
- Good: a model emits `read_file`, `ask_user`, then `web_fetch`; the read finishes first, `ask_user` waits inline for the user's answer, and `web_fetch` starts only after the answer tool result is ready.
- Good: a model starts streaming `write_file` arguments for a long generated file; the UI shows a backend-emitted pending tool record while arguments are being generated, and if the provider stream times out before `ToolCallDone`, the tool record becomes error without losing the user/assistant turn.
- Base: a model emits only `run_python`; calls execute one at a time and keep old lifecycle behavior.
- Bad: a scheduler parallelizes `skill_activate` or an arbitrary MCP stdio tool without explicit read-only annotations and races shared state or external side effects.
- Bad: a scheduler includes `ask_user` in a parallel batch, allowing later tools to run before the user's answer has entered the transcript.
- Bad: schema validation happens inside one executor implementation only; other executors can still receive invalid arguments or approval prompts can show invalid payloads.

### 6. Tests Required

- Prove two eligible tools overlap by recording start/finish events.
- Prove explicitly read-only MCP tools overlap while destructive/open-world/non-read-only MCP tools remain serial.
- Prove returned `response_messages` and persisted `tool_records` follow original call order.
- Prove schema-invalid arguments produce error records and never call the executor or approval hook.
- Prove streamed tool-call start/delta/done events emit backend-owned tool draft records and tool segments before execution.
- Prove MCP `annotations`, `outputSchema`, and result `structuredContent` survive parse/registry/command/TypeScript boundaries.
- Prove serial-only tools never overlap.
- Prove `ask_user` remains allowed in Plan mode and remains serial between parallel-safe tools.
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

## Scenario: Assistant Timeline Segments

### 1. Scope / Trigger

- Trigger: changes that alter Chat assistant message storage, `chat-stream` payloads, streaming preview state, final assistant replay, or frontend assistant rendering.
- Assistant output is a time-ordered timeline, not three independent buckets. Runtime text, reasoning, and tool calls must be representable as interleaved `segments`.

### 2. Signatures

- Persistent model: `ChatMessage.segments: Vec<ChatMessageSegment>` with `#[serde(default)]` for old conversations.
- Segment shape: `ChatMessageSegment { id, kind, phase, order, step_number?, round?, text?, tool_call_id? }`.
- Segment enums:
  - `kind`: `text | reasoning | tool`
  - `phase`: `auxiliary | plain | tool_loop | synthesis`
- Runtime result: `AgentRunResult.segments` and `AgentStepResult.segments`.
- Stream host: `AgentHost::emit_stream_delta(..., segment: Option<&ChatMessageSegment>)`.
- Tauri event: `chat-stream` includes `segmentId`, `segmentKind`, `phase`, `order`, `stepNumber`, `round`, `toolCallId`, and full `segment`.

### 3. Contracts

- The backend is the source of truth for segment order. The frontend must not synthesize guessed tool segments from `chat-tool` progress events.
- Streaming tool-call preparation segments are also backend-owned. Frontend rendering may update the matching tool record from `chat-tool`, but it must not create a separate inferred segment when provider tool-call deltas arrive.
- `SegmentBuilder` starts assistant-runtime segment ordering at `1000`. Auxiliary tool segments may use lower orders such as `100`.
- Planning/tool-loop narration uses `kind = text`, `phase = tool_loop`. Final answer text uses `phase = synthesis` when tools are involved, otherwise `plain`.
- Reasoning is represented by `kind = reasoning` segments and may appear during tool loop or synthesis. Within the same model step, reasoning must display before that step's text; reserve reasoning segments before text segments and keep frontend rendering compatible with older persisted messages that had the reverse order.
- Tool calls must get `kind = tool` segments before or alongside visible tool progress. Visible skipped, cancelled, blocked, auxiliary, unknown, and invalid tool calls still need tool segments so the timeline has no holes.
- Long-running tool argument generation is visible before execution: `ToolCallStart` should create a `kind = tool` segment and a pending tool record even though the tool has not executed yet. The label should distinguish argument/content generation from actual file mutation execution.
- Hidden disabled built-in feedback such as disabled `web_search` retry hints must not create visible tool segments when it does not create a visible `ToolCallRecord`.
- Persisted legacy fields are derived by `push_assistant_message` / edit helpers:
  - `content` is the `\n\n` join of all non-empty `kind = text` segments whose phase is `plain` or `synthesis`.
  - `reasoning` is the `\n\n` join of all non-empty `kind = reasoning` segments.
  - `tool_calls` remains the full record list in runtime/model order.
- `model_messages` for storage must use the same final legacy content/reasoning and preserve the tool transcript. Editing an assistant final answer must replace only final `plain`/`synthesis` text segments and rewrite replay messages to the edited final answer.
- Streaming and persisted rendering share the same segment shape. `chat-tool` events update tool record state only; `chat-stream` segment fields update timeline layout.
- Finish must not blank the assistant preview before persisted content is available. If `done` is delayed until after the invoke response, the frontend should patch from the returned conversation or await the pending done handler before clearing the snapshot.

### 4. Validation & Error Matrix

| Condition | Runtime behavior |
|---|---|
| Old assistant message has no `segments` | Deserialize successfully; synthesize fallback segments from `content`, `reasoning`, and `tool_calls` when rewriting or normalizing. |
| Runtime result has content/reasoning but no matching final segment | `normalize_assistant_segments` appends a final text/reasoning segment before save. |
| Tool record has no matching tool segment | `normalize_assistant_segments` appends a tool segment with `auxiliary` for round `0`/Mixer or `tool_loop` otherwise. |
| Plan-mode blocked or approval-denied tool is skipped/cancelled | Persist and stream a visible tool segment plus the matching `ToolCallRecord`. |
| Disabled built-in fallback feedback is hidden | Append model-facing feedback only; do not emit a visible segment or record. |
| Assistant final answer is edited | Preserve prior tool-loop text/reasoning/tool segments, replace final text segments, clear stale `api_messages`, and rewrite `model_messages`. |
| Stream `done` arrives after invoke success | Do not create an empty preview; finish from the returned persisted conversation or the pending done handler. |

### 5. Good/Base/Bad Cases

- Good: model narrates a plan, calls `read_file`, narrates another step, then writes a synthesis. The UI renders text -> tool -> text -> final in the same order during streaming and after reload.
- Good: auxiliary vision analysis runs before the main model. It appears as an auxiliary tool segment before the main assistant timeline.
- Base: old messages without `segments` still render with the legacy tool/reasoning/content fallback.
- Bad: frontend receives `chat-tool` and inserts a guessed tool segment based on arrival time, causing stream order to differ from persisted order.
- Bad: save writes only `segments` and leaves legacy `content` stale, so copy/export/replay uses the wrong final answer.
- Bad: send/regenerate clears streaming preview in `finally` before reload or returned conversation has been applied, causing a blank frame.

### 6. Tests Required

- Serde compatibility: old assistant JSON without `segments` loads successfully.
- Legacy derivation: `content_from_segments` joins only `plain`/`synthesis` text; `reasoning_from_segments` joins reasoning segments.
- Segment normalization: auxiliary and skipped/cancelled tool records receive matching tool segments.
- Stream order: visible tool segments are produced from model-call order and hidden disabled built-ins are excluded.
- Edit replay: editing an assistant reply rewrites replay to the edited final answer while preserving tool transcript output.
- Frontend type/build: `npm run typecheck` verifies `chat-stream` segment payloads and `MessageBubble` timeline rendering props.

### 7. Wrong vs Correct

#### Wrong

```typescript
api.onChatTool((payload) => {
  snapshot.segments.push(makeToolSegmentFromArrivalTime(payload))
})
```

This makes streaming layout depend on event timing instead of the model/tool-call order saved by the backend.

```rust
message.segments = segments;
message.content = content;
```

This can leave legacy fields, replay, copy/export, and old UI paths inconsistent with the timeline.

#### Correct

```rust
let segments = normalize_assistant_segments(&content, reasoning.as_deref(), &tool_calls, segments);
let stored_content = content_from_segments(&segments).unwrap_or_else(|| content.clone());
let stored_reasoning = reasoning_from_segments(&segments).or(reasoning);
```

```typescript
const segment = streamPayloadToSegment(payload)
if (segment) {
  snapshot.segments = upsertStreamSegment(snapshot.segments, segment, delta)
}
```

Backend-owned segments define the timeline; legacy fields are deterministic projections of that same timeline.

## Scenario: Agent Todo Runtime State

### 1. Scope / Trigger

- Trigger: changes that add or modify agent-owned conversation state maintained by model tools, especially `agent_todo_state`, `todo_write`, `todo_update`, prompt injection, or `chat-todo` events.
- This is agent runtime state, not a user task manager. Users may observe it in the Chat UI, but they must not manually edit it.

### 2. Signatures

- Persistent model: `Conversation.agent_todo_state: AgentTodoState` with `#[serde(default)]` for old conversation JSON.
- State shape: `AgentTodoState { items: Vec<AgentTodoItem>, updated_at: i64 }`.
- Item shape: `AgentTodoItem { id: String, content: String, status: AgentTodoStatus }`.
- Status enum: `pending | in_progress | completed`.
- Native tools: `todo_write({ todos })` replaces the full list; `todo_update({ id, content?, status? })` updates one existing item.
- Tauri event: `chat-todo` payload `{ conversationId, todoState }`.

### 3. Contracts

- Canonical todo state lives on `Conversation`, not only in tool records or message metadata.
- Current todo state must be injected into the model system/runtime prompt before `build_chat_api_messages`, and `compute_context_state` must include the same prompt segment for token estimates.
- Todo tools are appended by the Chat runtime when the provider supports tool calls; they are not governed by user MCP/native-tool settings, assistant tool presets, or data connector filters.
- Todo tools are serial state writes. They bypass approval but must not be added to the native parallel-safe set.
- Tool results must include `structured_content` with the latest `todoState`.
- The frontend treats todo state as read-only conversation data and updates it from `chat-todo`.
- If a tool writes conversation state during `run_agent_loop`, `complete_assistant_reply` must reload/merge the latest todo state before saving the assistant message, otherwise the older in-memory `Conversation` can overwrite the tool update.

### 4. Validation & Error Matrix

| Condition | Runtime behavior |
|---|---|
| Old conversation lacks `agent_todo_state` | Deserialize to an empty default state. |
| `todo_write.todos` contains empty `id` or `content` | Return a tool error; do not save state. |
| `todo_write.todos` contains duplicate ids | Return a tool error; do not save state. |
| More than one item is `in_progress` | Normalize to at most one `in_progress`; demote extras to `pending`. |
| `todo_update.id` is missing or unknown | Return a tool error; do not save state. |
| `todo_update` provides neither `content` nor `status` | Return a tool error; do not save state. |
| Provider does not support tools or is Apple local | Inject todo context as read-only; do not expose todo tools. |

### 5. Good/Base/Bad Cases

- Good: model calls `todo_write` at the start of a multi-step task, then `todo_update` as work advances; UI receives `chat-todo` and later turns see the persisted state in context.
- Base: conversation has no todos; prompt says there are no current todos and UI renders no panel.
- Bad: storing todo only in `ToolCallRecord.structured_content`; next turn loses the working state after reload or compaction.
- Bad: appending todo tools before assistant/data-connector filters; `tool_preset: none` can accidentally remove agent housekeeping.
- Bad: saving the old in-memory conversation after tool execution without merging latest `agent_todo_state`.

### 6. Tests Required

- Serde compatibility: old conversation JSON without `agent_todo_state` loads with an empty state.
- Normalization: multiple `in_progress` items collapse to one.
- Update behavior: setting a new item to `in_progress` demotes the previous active item.
- Prompt/context: todo prompt segment appears in both request construction and context estimates.
- Tool trace: successful todo tools persist `structured_content.todoState`.
- Frontend type/build: `npm run typecheck` verifies `chat-todo` payload and read-only panel wiring.

### 7. Wrong vs Correct

#### Wrong

```rust
let result = run_agent_loop(...).await?;
push_assistant_message(app, state, settings, conversation, ..., result.tool_records, ...).await?;
```

This can overwrite todo changes that a tool already saved to disk during the run.

#### Correct

```rust
let result = run_agent_loop(...).await?;
merge_latest_agent_todo_state(app, conversation);
push_assistant_message(app, state, settings, conversation, ..., result.tool_records, ...).await?;
```

Tool-owned conversation state must be merged back before the final conversation save.

## Scenario: Agent Plan Mode Runtime State

### 1. Scope / Trigger

- Trigger: changes that add or modify Plan/Act behavior, especially `agent_plan_state`, plan prompt injection, plan approval commands, Plan-mode tool filtering, or `chat-plan` events.
- Plan mode is an agent runtime permission mode, not a user task manager. It lets the assistant investigate and draft an implementation plan before side-effecting actions are allowed.
- The persisted plan is read-only from the user's perspective. Do not add user-editable plan fields without a product decision.

### 2. Signatures

- Persistent model: `Conversation.agent_plan_state: AgentPlanState` with `#[serde(default)]` for old conversation JSON.
- State shape: `AgentPlanState { mode: AgentPlanMode, status: AgentPlanStatus, plan: Option<String>, updated_at: i64 }`.
- Mode enum: `act | plan`.
- Status enum: `empty | draft | approved`.
- Tauri commands:
  - `chat_set_agent_plan_mode(conversation_id: String, mode: String) -> { success, conversation, planState }`
  - `chat_execute_agent_plan(conversation_id: String) -> { success, conversation, planState }`
- Tauri event: `chat-plan` payload `{ conversationId, planState }`.
- Prompt segment id: `agent_plan`.

### 3. Contracts

- New conversations start in `mode = act`, `status = empty`, `plan = None`.
- `chat_set_agent_plan_mode` only accepts `act` or `plan`; it preserves the saved plan text and status while changing mode.
- `chat_execute_agent_plan` switches to `act`; if a non-empty plan exists, status becomes `approved`, otherwise it remains `empty`.
- In Plan mode, the final assistant reply is captured as a draft plan only when the original turn started in Plan mode and the latest saved state is still Plan mode.
- Current plan state must be injected into the system/runtime prompt before `build_chat_api_messages`, and `compute_context_state` must include the same `agent_plan` segment for token estimates.
- In Act mode, approved or draft plan text remains contextual; if the user asks to execute/continue, the model should use it unless the latest user message changes requirements.
- Plan mode must filter side-effecting tools before model invocation. Allowed tools are:
  - native read-only tools: `web_search`, `web_fetch`, `read_file`, `memory_read`
  - MCP tools with explicit `readOnlyHint == true` and no destructive/open-world hints
  - skill discovery/read tools: `skill_activate`, `skill_read_file`
  - agent todo tools: `todo_write`, `todo_update`
- Plan mode must not expose writes/edits, command execution, `run_python`, memory mutation, Mixer image generation, `skill_run_script`, or arbitrary/non-read-only MCP tools.
- Tools removed by the Plan filter must be kept as blocked metadata for the current run. If the model still requests one through fallback markup or stale provider state, emit a visible `ToolCallRecord` with `status = skipped` and return model-facing feedback that the tool is blocked in Plan mode.
- If plan state is updated while a reply is being completed, `complete_assistant_reply` must reload/merge the latest plan state before saving the assistant message, otherwise an older in-memory `Conversation` can overwrite the plan update.
- The frontend treats plan state as read-only conversation data and updates it from `chat-plan`.

### 4. Validation & Error Matrix

| Condition | Runtime behavior |
|---|---|
| Old conversation lacks `agent_plan_state` | Deserialize to default Act/Empty state. |
| `chat_set_agent_plan_mode` receives an unknown mode | Return an error; do not save conversation state. |
| Plan mode assistant reply is blank | Do not replace the current plan. |
| User executes with no saved plan | Switch to Act and keep status `empty`. |
| User executes with a saved plan | Switch to Act and mark status `approved`. |
| Plan mode tool list includes write/command/Python/memory mutation/image/script tools | Remove them before provider invocation. |
| Model requests a tool removed by Plan filtering | Emit a visible skipped tool record and return a tool message explaining it is blocked in Plan mode. |
| Non-read-only MCP tool has `readOnlyHint == false`, missing read-only metadata, `destructiveHint == true`, or `openWorldHint == true` | Remove it in Plan mode. |
| Provider does not support tools or is Apple local | Inject plan context as prompt text; do not expose unavailable tools. |

### 5. Good/Base/Bad Cases

- Good: user switches to Plan, asks for implementation analysis, the agent reads files/searches web, returns a plan, and the draft is visible and persisted on the conversation.
- Good: user clicks Execute Plan, runtime switches to Act, sends a continuation request, and the saved plan is injected into the next model turn.
- Base: no plan exists; UI shows no plan panel, prompt says there is no saved plan, and Act behavior is unchanged.
- Bad: Plan mode only changes the system prompt while leaving `write_file`, `run_command`, or `run_python` in the tool schema.
- Bad: storing the plan only in assistant message text; next turn loses the accepted plan after reload, compaction, or route changes.
- Bad: treating the plan as a calendar/reminder/task-management object or allowing manual user edits in the MVP.

### 6. Tests Required

- Serde compatibility: old conversation JSON without `agent_plan_state` loads as Act/Empty.
- State helpers: draft capture trims non-empty assistant replies and approval marks saved plans as `approved`.
- Prompt/context: `agent_plan` prompt segment appears in both request construction and context estimates.
- Tool filter: Plan mode keeps read-only native/MCP, skill read tools, and todo tools while removing writes, commands, Python, memory mutation, image generation, script execution, and non-read-only MCP tools.
- Blocked-tool trace: a model request for a Plan-filtered tool yields a `skipped` record rather than silently disappearing.
- Command registration/API: new Tauri commands are registered and frontend types mirror the payload.
- Frontend type/build: `npm run typecheck` verifies `chat-plan` event wiring, Plan/Act controls, Execute Plan flow, and read-only plan panel props.

### 7. Wrong vs Correct

#### Wrong

```rust
let system_prompt = format!("{base}\nPlan mode: don't edit files");
let tools = list_tools_for_chat(...).await;
```

This asks the model not to act while still exposing side-effecting tools.

```rust
let result = run_agent_loop(...).await?;
capture_plan_from_reply(conversation, &result.content);
push_assistant_message(..., conversation, ...).await?;
```

This can overwrite a concurrently saved plan/mode change with stale in-memory conversation state.

#### Correct

```rust
apply_agent_plan_tool_filter(&mut tools, is_plan_mode);
let agent_plan_prompt = plan::format_prompt(&conversation.agent_plan_state, &language);
```

```rust
let result = run_agent_loop(...).await?;
merge_latest_agent_plan_state(app, conversation);
capture_agent_plan_draft_if_needed(app, conversation, is_plan_mode, &result.content);
push_assistant_message(..., conversation, ...).await?;
```

Plan mode must be enforced by both prompt context and backend tool availability, and persisted state must be merged before the final save.

## Scenario: Project Workspace Filesystem

### 1. Scope / Trigger

- Trigger: changes that connect Chat projects to local folders, alter `Conversation.project_id`, `ChatProject.root_path`, native filesystem tools, command cwd behavior, or model-facing tool descriptions.
- Project workspace support is a workspace permission system, not an OS sandbox. Native filesystem tools are project-scoped in project conversations; `run_command` is a sensitive host-shell capability that starts from the project root by default and is governed by approval/user-intent semantics rather than a chroot-like guarantee.

### 2. Signatures

- Persistent project model: `ChatProject { id, name, description?, color?, root_path?, created_at, updated_at }`.
- Persistent conversation model: `Conversation.project_id: Option<String>` with `folder: Option<String>` retained for legacy display/fallback.
- Tauri commands:
  - `chat_get_conversations(offset, limit, folder?, project_id?) -> { success, conversations }`
  - `chat_create_conversation(provider_id?, model?, folder?, project_id?, assistant_id?) -> { success, conversation }`
  - `chat_create_project(name, description?, color?, root_path?) -> { success, project }`
  - `chat_update_project(project_id, name?, description?, color?, root_path?) -> { success, project }`
  - `chat_update_conversation(conversation_id, ..., folder?, project_id?, ...) -> { success, conversation }`
- Workspace resolver: `NativeToolWorkspace::{global(workspace_roots), project(project_id, project_name, root_path)}`.
- Native project filesystem tools:
  - Read/list/search/stat: `read_file`, `list_dir`, `search_files`, `glob_files`, `stat_path`
  - Mutations: `write_file`, `edit_file`, `create_dir`, `delete_path`, `move_path`, `copy_path`
  - Commands: `run_command`
  - Python file inputs: `run_python.files`

### 3. Contracts

- Project membership uses `Conversation.project_id` as the durable link. `folder` is compatibility/display data and may be used only as a fallback for legacy conversations without `project_id`.
- Project roots are normalized to canonical absolute directories before storage. Empty `root_path` means the project is chat-only until the user binds a folder. Project update commands must distinguish an omitted nullable field from an explicit `null`; explicit `null` clears `description`, `color`, or `root_path`, while omitted fields preserve the current value.
- Tool execution resolves `conversation_id -> Conversation -> ChatProject -> root_path` at native tool call time.
- In project mode, native file-tool relative paths resolve under the project root. Absolute paths for native file tools are accepted only if their canonical target stays inside the same root.
- Missing write targets are checked by canonicalizing the nearest existing parent, then joining missing path components. This prevents parent symlink escapes.
- Paths containing `..` are rejected before filesystem access.
- In project mode, `run_command` defaults its startup `cwd` to the project root. Explicit `cwd` is validated as a workspace-local startup directory and must be an existing directory. The shell command itself remains a host-shell command; do not describe it as sharing the exact native file-tool boundary.
- `run_command` must remain sensitive/approval-gated under the default policy. Model-facing instructions must say to honor explicit user constraints such as “do not use shell” or “do not access project-outside paths”, and to explain or ask before cross-directory, destructive, network, or environment-changing shell commands.
- In project mode without a bound `root_path`, filesystem tools and command tools return a clear bind-folder-first error.
- Outside project mode, existing global native-tool behavior remains the fallback: read paths use readable local path resolution, write paths use `workspaceRoots`/home constraints, and command cwd falls back to first workspace root or home.
- Model-facing prompt/tool descriptions must distinguish native file tools from host shell behavior: project file paths are project-relative by default and backend validation enforces the native file-tool boundary; shell is a sensitive host capability with default project cwd and approval/user-intent controls.
- Read-only project tools may join the native parallel-safe set only when they do not require approval: `read_file`, `list_dir`, `search_files`, `glob_files`, `stat_path`.
- Mutation tools and `run_command` remain approval-sensitive and serial.
- `copy_path` must reject copying a directory to itself or any descendant path before creating the destination, otherwise recursive copy can grow without bound.
- Deleting a symlink inside the project deletes the link entry itself. Boundary checks for delete/move-source operations must not follow the final symlink target; parent directories are still resolved canonically so parent symlinks cannot escape the project.
- `glob_files.pattern` is a pattern relative to the search `path`; absolute or `..`-containing path-like patterns must return a clear argument error instead of a silent empty match set.

### 4. Validation & Error Matrix

| Condition | Runtime behavior |
|---|---|
| Project id is provided when creating/updating a conversation | Resolve by id, set `conversation.project_id`, and mirror project name into `folder`. |
| Only legacy `folder` is provided | Resolve project by name when possible; otherwise preserve folder as legacy grouping. |
| Project root is empty | Store `root_path = None`; project remains chat-only for filesystem operations. |
| Project update omits `root_path` | Preserve the existing project root. |
| Project update explicitly clears `root_path` | Store `root_path = None`; project becomes chat-only for filesystem operations. |
| Project root is relative, missing, or not a directory | Reject project create/update with a user-facing error. |
| Project native file-tool path contains `..` | Reject before touching the filesystem. |
| Project native file-tool absolute path resolves outside root | Reject with a project-root boundary error. |
| Project write target does not exist and parent symlink points outside root | Reject after canonical parent resolution. |
| `copy_path` directory destination equals or is inside source | Reject before creating the destination directory. |
| Project `run_command.cwd` is omitted | Use project root. |
| Project `run_command.cwd` resolves outside root or is not a directory | Reject before spawning the process. This validates only the startup directory, not every path the shell may touch after launch. |
| User explicitly says not to use shell | Do not call `run_command`; if shell is required for verification, ask first. |
| Project-internal symlink points outside root and `delete_path` targets the symlink | Delete the link entry without deleting or reading the outside target. |
| `glob_files.pattern` is absolute or contains `..` | Return an argument error explaining that `pattern` is relative and `path` selects the search root. |
| Legacy/no-root project uses file or command tool | Return an error telling the user to bind a local folder first. |
| Non-project conversation uses tools | Preserve global workspace-root fallback behavior. |

### 5. Good/Base/Bad Cases

- Good: user selects a sidebar project bound to `/repo/app`, asks to inspect `src/App.tsx`, and the model calls `read_file({ path: "src/App.tsx" })`.
- Good: user asks to run tests in a project conversation; `run_command({ command: "npm test" })` runs from the project root without needing an explicit cwd.
- Good: user asks for a repo-wide shell command that touches a sibling repo; the agent explains that shell is a host capability and asks for confirmation instead of claiming the project is a hard sandbox.
- Base: old name-only projects continue showing conversations, but file tools explain that a folder must be bound first.
- Bad: tool execution relies on the project name alone after rename; use `project_id` instead.
- Bad: prompt says paths should stay in the project, but backend still accepts `/Users/...` outside root.
- Bad: prompt implies `run_command` is sandboxed the same way as `read_file`, causing misleading safety claims.
- Bad: user says “do not run shell”, but the model calls `run_command` for convenience.

### 6. Tests Required

- Storage compatibility: old conversations without `project_id` still list under the matching project name.
- Conversation create/update: project id sets both `project_id` and legacy `folder`.
- Project create/update: root path is canonicalized and invalid roots are rejected.
- Project update: explicit `null` clears `root_path`, while omitted `root_path` leaves it unchanged.
- Native path resolver: rejects `..`, absolute outside-root paths, and symlink parent escapes for missing write targets.
- Native tools: read/list/search/stat succeed on project-relative paths; write/edit/delete/move/copy cannot escape root.
- Native tools: `delete_path` removes a project-internal symlink whose target is outside root, without touching the target.
- Native tools: `glob_files` rejects absolute or `..` path-like patterns with an explicit error.
- Native tools: `copy_path` rejects directory copies into itself or descendants.
- Command cwd: omitted cwd uses root; outside-root explicit cwd is rejected as a startup directory.
- Prompt/tool definitions: new tool names appear in schemas, disabled-tool feedback, project-relative file-tool wording, and non-sandbox shell wording.
- Frontend type/build: `npm run typecheck` verifies project id/root path propagation.
- Backend checks: targeted `cargo test --manifest-path src-tauri/Cargo.toml native_tools mcp::types chat::agent::prepare -- --nocapture` or equivalent split filters, plus full `cargo test` when practical.

### 7. Wrong vs Correct

#### Wrong

```rust
let full = resolve_workspace_path(path, workspace_roots)?;
```

This ignores the active conversation project and keeps writes scoped to global settings instead of the selected workspace.

```rust
let cwd = arguments.get("cwd").unwrap_or("~/");
```

This ignores the project default startup directory and approval semantics.

#### Correct

```rust
let workspace = resolve_native_workspace(app, workspace_roots, native_ctx.as_ref())?;
let full = resolve_tool_write_path(&workspace, path)?;
```

```rust
let cwd = resolve_tool_existing_dir(&workspace, arguments.get("cwd").and_then(|v| v.as_str()))?;
```

Resolve the current project at tool-call time, use shared project-aware resolvers for native file paths and command startup cwd, and describe shell as a sensitive host capability rather than a project sandbox.

## Scenario: Agent File Mutation Tools

### 1. Scope / Trigger

- Trigger: changes to native file mutation tools, tool-call result records, MCP structured content handling, agent file-editing prompts, or Chat UI rendering of file changes.
- Agent coding edits should prefer targeted mutations with visible diff metadata. Whole-file writes remain available for new files, explicit full replacement, or requested deliverable files.

### 2. Signatures

- Native tools:
  - `write_file({ path, content }) -> FileMutationResult`
  - `write_file_chunk({ path, mode: start|append|finish, content? }) -> FileMutationResult`
  - `edit_file({ path, old_string, new_string, replace_all? }) -> FileMutationResult`
  - `patch({ patch }) -> FileMutationResult`
- Patch grammar:

```text
*** Begin Patch
*** Add File: path/to/new.ts
+content
*** Update File: path/to/existing.ts
@@
 context
-old
+new
*** Delete File: path/to/old.ts
*** End Patch
```

- Structured result:

```rust
FileMutationResult {
    operation,
    resolved_path?,
    files: Vec<FileMutationFile>,
    bytes_written,
    additions,
    removals,
    diff,
    warnings,
    diagnostics,
}
```

- Per-file result:

```rust
FileMutationFile {
    path,
    operation,
    bytes_written,
    additions,
    removals,
    diff,
}
```

### 3. Contracts

- `write_file`, `write_file_chunk`, `edit_file`, and `patch` must return `McpToolCallResult.structured_content = FileMutationResult` and a concise text `content` summary for model replay.
- `ToolCallRecord.structured_content` must preserve file mutation metadata across backend events, persisted messages, and frontend rendering. Do not rely on raw JSON previews for file mutation UX.
- Tool content sent back to the model may include `structuredContent` when the text summary does not already contain the same JSON, following the shared MCP structured-content contract.
- `write_file` is for new files, explicit whole-file replacement, or requested deliverables. Existing-file small edits should use `edit_file`; multi-file or larger code edits should use `patch`.
- `write_file_chunk` is for long file content (roughly > 200 lines or > 8 KB) instead of one giant `write_file` argument: `mode=start` creates/overwrites the file with the first portion, `mode=append` requires an existing file and appends the next portion, `mode=finish` verifies the file and returns its final state. Every call persists immediately, so an interrupted generation keeps all prior chunks on disk.
- `write_file_chunk` is serial, sensitive, and approval-gated like `write_file`: never in the native parallel-safe set, blocked in Plan mode, and it acquires the per-resolved-path in-process lock on every call.
- Inline code/demo requests that do not ask to save locally must hide `write_file`, `write_file_chunk`, and `patch` from the model. `edit_file` may remain available because project-edit requests still need targeted existing-file edits.
- `patch` file headers must be project-relative: no absolute path, `~`, backslashes, roots, or `..`.
- A completely empty line inside an Update File hunk is tolerated as an empty context line (equivalent to `" "`), matching `git apply` behavior, not a parse error.
- In project mode, all file mutations must use project-aware resolvers and stay inside the bound project root. Outside project mode, they must preserve global workspace/home write constraints.
- File mutation tools must be serial, approval-sensitive tools. They must not join the native parallel-safe read-only batch.
- File mutation tools must acquire per-resolved-path in-process locks before reading current contents and before applying writes/deletes. `delete_path`, `move_path` (source + destination), and `copy_path` (destination) acquire the same per-path mutation locks. Multi-file `patch` locks all resolved targets in sorted order to avoid deadlocks.
- `move_path` resolves its source without following the final symlink (same entry semantics as `delete_path`): moving a project-internal symlink moves the link entry, not its target.
- `patch` must fully validate and build all planned file results before applying any filesystem changes. A failed hunk, missing target, duplicate target, traversal path, or existing Add File target must leave every involved file unchanged.
- `edit_file` requires exactly one `old_string` match unless `replace_all` is true.
- Diff metadata may use lightweight unified diffs, but additions/removals must match the per-file changed lines shown in the diff.
- Frontend file mutation blocks should show operation, target path or file count, `+/-` stats, warnings/diagnostics, and an expandable diff.

### 4. Validation & Error Matrix

| Condition | Runtime behavior |
|---|---|
| `write_file.path` missing or content missing | Return a tool argument error before writing. |
| `write_file_chunk` mode=append or mode=finish targets a missing file | Return a tool error telling the model to call `mode=start` first; do not write. |
| `write_file_chunk.mode` is not `start`/`append`/`finish` | Return a tool argument error before touching the filesystem. |
| `write_file` overwrite or `write_file_chunk` mode=start hits an existing non-UTF-8 file | Succeed; report `overwrite` with diff omitted plus a warning explaining why. |
| `edit_file.old_string` is missing in the file | Return an error and do not write. |
| `edit_file.old_string` appears multiple times and `replace_all` is false | Return an error telling the model to use a unique old string or `replace_all=true`. |
| `edit_file.old_string == new_string` | Return a structured noop result with a warning and do not rewrite content. |
| `patch` does not start/end with the required markers | Return a parse error and do not write. |
| `patch` path is absolute, uses `~`, contains backslashes, roots, or `..` | Reject before resolving/writing. |
| `patch` Add File target already exists | Return an error and do not write any file. |
| `patch` Update/Delete target is not an existing file | Return an error and do not write any file. |
| `patch` hunk content is not an exact unique match | Return an error and do not write any file. |
| Two patch headers refer to the same textual or resolved file path | Return an error before applying. |
| Concurrent runs mutate the same resolved path | Later mutation waits on the in-process path lock before reading/applying. |
| Final provider synthesis fails after a successful mutation | Keep the completed tool record and surface provider failure separately. |

### 5. Good/Base/Bad Cases

- Good: user asks to change one label in `src/App.tsx`; the model reads the file and calls `edit_file` with a unique old/new replacement.
- Good: user asks for coordinated frontend/backend edits; the model emits a single `patch` that updates several files and the UI shows affected files plus diff stats.
- Good: a provider fails after `patch` succeeds; the chat timeline still shows the patch block as completed with its files and diff metadata.
- Good: the model writes a 1000-line file in 4 `write_file_chunk` calls (start + 3 appends); the stream dies after chunk 2, chunks 1-2 are already on disk, and the next turn appends the rest instead of regenerating everything.
- Base: user asks to create a new `index.html`; the model calls `write_file` and receives a structured create result.
- Bad: regenerating an entire existing source file through `write_file` for a two-line change.
- Bad: applying the first file in a multi-file patch before verifying later hunks; failures would leave a half-applied repo.
- Bad: storing file mutation details only in model-facing text; the frontend cannot reliably render paths, stats, or diff.
- Bad: a scheduler parallelizes `write_file_chunk` calls or reorders an append before its start; chunk order is the file content, so the calls must run serially in model order.

### 6. Tests Required

- Native tools: `write_file` returns structured diff stats for create/overwrite.
- Native tools: `write_file_chunk` start/append/finish persists each chunk immediately, append/finish on a missing file error with mode=start guidance, and the tool stays serial and blocked in Plan mode.
- Native tools: `edit_file` enforces uniqueness and returns a structured noop warning when old/new match.
- Patch parser/apply: add, update, and delete files in one patch.
- Patch parser: an empty line inside an Update File hunk applies as an empty context line.
- Patch safety: failed hunk does not partially modify earlier planned files.
- Patch safety: traversal/absolute/backslash/tilde paths are rejected.
- Patch safety: duplicate textual or resolved paths are rejected.
- Runtime: `write_file`, `edit_file`, and `patch` preserve `structured_content` on `ToolCallRecord` and model replay content.
- Prompt/filter: inline code-block requests remove `write_file`, `write_file_chunk`, and `patch`, while save/edit intents keep them.
- Frontend type/build: `npm run typecheck` verifies file mutation structured content parsing and `ToolCallBlock` rendering.
- Backend checks: run targeted native/MCP/agent prompt tests plus full `cargo test --manifest-path src-tauri/Cargo.toml` when practical.

### 7. Wrong vs Correct

#### Wrong

```rust
let after = apply_first_hunk_and_write(path)?;
let second = apply_second_file()?; // can fail after the first file changed
```

This leaves the project half-mutated when a later file fails.

```tsx
const preview = toolCall.resultPreview || JSON.stringify(toolCall.raw)
```

This treats file changes as opaque text and loses reliable diff/status rendering.

#### Correct

```rust
let plans = build_all_patch_plans(&ops)?;
let _guard = acquire_file_mutation_locks(plans.iter().map(|p| p.path.clone()))?;
let file_results = build_file_results(&plans)?;
apply_plans(plans)?;
```

```tsx
const mutation = structuredFileMutation(toolCall)
return mutation ? <FileMutationDetails mutation={mutation} /> : <GenericToolDetails />
```

Build and validate all mutation plans first, lock resolved target paths before reading/applying, persist structured metadata, and render file changes from that structured contract.
