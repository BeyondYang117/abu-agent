# Type Safety

> Type safety patterns in this project.

---

## Overview

<!--
Document your project's type safety conventions here.

Questions to answer:
- What type system do you use?
- How are types organized?
- What validation library do you use?
- How do you handle type inference?
-->

(To be filled by the team)

---

## Type Organization

<!-- Where types are defined, shared types vs local types -->

(To be filled by the team)

---

## Validation

<!-- Runtime validation patterns (Zod, Yup, io-ts, etc.) -->

(To be filled by the team)

---

## Common Patterns

<!-- Type utilities, generics, type guards -->

(To be filled by the team)

---

## Forbidden Patterns

<!-- any, type assertions, etc. -->

(To be filled by the team)

---

## Scenario: Chat MVP Cross-Layer Contract

### 1. Scope / Trigger
- Trigger: Chat features span React state, the centralized Tauri bridge, Rust Tauri commands, settings persistence, and JSON file storage.
- Apply this contract whenever changing `src/chat/**`, `src/api/tauri.ts`, or `src-tauri/src/chat/**`.

### 2. Signatures
- `chat_get_conversations(offset: usize, limit: usize, folder?: string) -> { success: true, conversations: ConversationListItem[] }`
- `chat_get_conversation(conversationId: string) -> { success: true, conversation: Conversation }`
- `chat_create_conversation(providerId?: string, model?: string, folder?: string) -> { success: true, conversation: Conversation }`
- `chat_send_message(conversationId: string, content: string, attachments: string[]) -> { success: boolean, conversation?: Conversation, error?: string }`
- `chat_update_conversation(conversationId: string, title?: string, pinned?: boolean, folder?: string, providerId?: string, model?: string) -> { success: true, conversation: Conversation }`
- `chat-stream` event payload is `ChatStreamPayload` in `src/api/tauri.ts`.

### 3. Contracts
- Conversation IDs are backend-owned and must match `conv_*`; frontend must pass IDs returned by chat commands or parsed from `#chat/{conversation_id}`.
- Stored Rust fields use snake_case (`provider_id`, `created_at`); Tauri command argument names from TypeScript use camelCase (`conversationId`, `providerId`).
- Chat model defaults live in settings as `defaultModels.chat`; legacy `chatProviderId` and `chatModel` are kept as a compatibility mirror. When the structured chat default is unset, backend sanitization falls back to Lens provider/model, then translator provider/model.
- Default model slots for title summary and compression live under `defaultModels.titleSummary` and `defaultModels.compression`. If those slots are unset, backend effective-model helpers inherit the effective Chat default.
- New Chat conversation titles are generated after the first assistant reply using `defaultModels.titleSummary` when configured, or the effective Chat default when unset. Title generation is best effort: provider errors, timeouts, missing keys, or invalid title output must fall back to local first-user-message truncation and must not fail `chat_send_message`.
- Model providers expose `enabledModels` and `availableModels`; do not read a `models` array in Chat UI.
- Streaming payload fields: `{ imageId, kind: 'answer', delta, reasoningDelta?, done?, reason?, full? }`.

### 4. Validation & Error Matrix
- Invalid conversation ID -> backend returns `Err("Invalid conversation id: ...")`; do not construct file paths directly in frontend.
- Missing conversation file -> backend returns a failed invoke; frontend shows a visible load/send error.
- Pagination offset beyond loaded index length -> backend returns an empty list, never panics.
- Stream `reason: 'error'` -> frontend clears streaming state, shows an error, and re-enables input.
- Stream `reason: 'cancelled'` -> frontend clears streaming state without forcing a conversation reload.

### 5. Good/Base/Bad Cases
- Good: `ModelSelector` uses `enabledModels.length > 0 ? enabledModels : availableModels`, then persists through `chat_update_conversation`.
- Base: `#chat` shows the empty Chat shell; `#chat/{id}` loads the referenced conversation.
- Bad: listening for `{ kind: 'chunk', text }` on `chat-stream`; backend emits `delta` and `done`, not `text`.

### 6. Tests Required
- `npm run typecheck` must catch mismatches between bridge types and Chat UI props.
- `npm run lint` must pass without `any` stream payload listeners.
- `cargo test --manifest-path src-tauri/Cargo.toml` must include settings fallback assertions when changing chat defaults, and title-generation regression tests when changing title summary behavior.
- `npm run build:ui` must pass after route or lazy-loaded Chat component changes.

### 7. Wrong vs Correct

#### Wrong
```tsx
listen<any>('chat-stream', (event) => {
  if (event.payload.kind === 'chunk') {
    setStreamingContent((prev) => prev + event.payload.text)
  }
})
```

#### Correct
```tsx
api.onChatStream((payload) => {
  if (payload.delta) {
    setStreamingContent((prev) => prev + payload.delta)
  }
  if (payload.done) {
    setStreaming(false)
  }
})
```

## Scenario: Chat MCP + Skill Cross-Layer Contract

### 1. Scope / Trigger
- Trigger: Chat MCP + Skill work changes model request construction, Tauri command payloads, streaming events, conversation JSON persistence, settings JSON persistence, and Chat UI rendering.
- Apply this contract whenever implementing Chat Tool Runtime, MCP server settings, Skill selection/loading, or tool-call persistence.

### 2. Signatures
- `chat_send_message(conversationId: string, content: string, attachments: string[], activeSkillId?: string) -> { success: boolean, conversation?: Conversation, error?: string }`
- `chat_mcp_list_tools() -> { success: boolean, tools?: ChatToolDefinition[], error?: string }`
- `chat_mcp_test_server(server: ChatMcpServer) -> { success: boolean, tools?: ChatToolDefinition[], error?: string }`
- `chat_skills_list() -> { success: boolean, skills?: SkillMeta[], error?: string }`
- `chat_skills_read(skillId: string) -> { success: boolean, skill?: SkillDetail, error?: string }`
- `chat-stream` event payload: `{ conversationId, runId, messageId?, kind, delta, reasoningDelta?, done?, reason?, full? }`.
- `chat-tool` event payload: `{ conversationId, runId, messageId?, toolCallId, name, source, serverId?, status, argumentsPreview?, resultPreview?, error?, startedAt?, completedAt?, durationMs? }`.

### 3. Contracts
- Settings JSON remains camelCase. Add new settings under one Chat tools block such as `chatTools`, not as many unrelated top-level fields.
- Conversation JSON remains Rust snake_case. Persist conversation-level Skill as `active_skill_id`; persist assistant tool traces as message `tool_calls`.
- Strict tool-calling provider transcript is stored separately from the visible timeline. When an assistant response includes tool calls, persist message `api_messages` with the raw assistant `tool_calls` message, each matching `role: "tool"` result, and the final assistant message.
- Preserve provider reasoning payloads only in the hidden transcript when protocol replay requires them. DeepSeek thinking-mode tool calls need `reasoning_content` replayed with the assistant tool-call message; plain no-tool answers should not create hidden `api_messages`.
- Frontend TypeScript converts persisted snake_case fields at the bridge/type boundary or explicitly models existing snake_case fields, matching current Chat conventions.
- Tool traces are metadata on the related assistant message, not standalone `role: 'tool'` timeline messages in the UI.
- Skill selection is conversation-pinned and user-switchable. Sending a message snapshots the active Skill ID onto the generated assistant message.
- Claude-style Skills use progressive loading: list metadata in the catalog, load `SKILL.md` through `skill_activate`, read supporting files through `skill_read_file`, and run bundled scripts through `skill_run_script`.
- Native and Skill runtime tools must expose prompt-facing OpenAI names (`web_search`, `skill_activate`, `skill_read_file`, `skill_run_script`). MCP tools must keep namespaced OpenAI names such as `mcp__server_id__tool_name` to avoid collisions.
- Tool approval policy defaults to read-only tools auto-running and sensitive tools requiring confirmation.
- `skill_run_script` is sensitive by default, only accepts paths under `scripts/`, and must run through the Rust-owned Skill runtime. `skill_activate`, `skill_read_file`, and `web_search` are read-like and may run automatically under the default policy.
- MCP stdio process control is Rust-owned. Frontend must not use `@tauri-apps/plugin-shell` to spawn MCP servers or require `shell:*` capability for this feature.
- MCP discovery and invocation follow the protocol boundary: list tools with `tools/list`, invoke with `tools/call`, and return MCP `isError` tool results back to the model as tool content rather than converting them into visible timeline messages.
- Streamable HTTP MCP responses may be `text/event-stream`; backend must read chunks under timeout and accept only the JSON-RPC event whose `id` matches the request, skipping notifications/progress/mismatched responses.
- Native `web_search` is a native tool, not an MCP server, but uses the same UI/event/status model as MCP tools.
- Cancelling a stream or switching conversations must stop the backend generation, ignore stale `chat-stream` / `chat-tool` events by `{ conversationId, runId }`, and keep optimistic user messages scoped to the conversation that sent them.

### 4. Validation & Error Matrix
- Missing or disabled MCP config -> no `tools` are sent to the model; Skill-only prompt injection still works.
- Provider lacks or rejects `tools` -> surface a user-visible Chat tool status and disable tool-dependent sends when a selected Skill recommends unavailable tools; do not break plain Chat or prompt-only Skill use.
- Provider returns an assistant message with no `tool_calls` during the tool loop -> finalize that exact assistant response immediately. Do not discard it and make a second plain request.
- Active Skill `allowed-tools` filtering -> always retain the internal Skill runtime tools (`skill_activate`, `skill_read_file`, `skill_run_script`) so the selected Skill can load itself.
- Old conversation JSON missing `active_skill_id`, `tool_calls`, or `api_messages` -> deserialize with defaults and render normally.
- MCP server imported from config -> keep disabled until the user explicitly enables it.
- MCP env values shown in UI/logs -> redact secret-looking values; never include full env secrets in tool previews.
- Tool run exceeds max rounds, timeout, or cancellation -> emit final `chat-tool` status and a `chat-stream` completion/error reason with the same `runId`.
- User cancels while request startup or chunk read is pending -> emit `chat-stream` done with `reason: 'cancelled'` promptly and do not rely on the HTTP stream producing another chunk.

### 5. Good/Base/Bad Cases
- Good: `MessageBubble` renders `ToolCallBlock` above assistant content by reading `message.toolCalls`.
- Good: `chat-tool` events patch only the matching `{ conversationId, runId }`, preventing stale events from another run updating the visible conversation.
- Good: stream cancellation is checked while opening the streaming request and again while awaiting each `response.chunk()`; polling only between already-received chunks can leave the UI waiting on a blocked network read.
- Good: pending optimistic user messages are keyed to the conversation that sent them; switching routes during send must not render conversation A's pending user message in conversation B.
- Good: after a tool-assisted response, later model requests replay hidden `api_messages` so OpenAI-compatible providers see the exact `assistant tool_calls -> tool -> assistant` sequence.
- Base: a conversation with no Skill and MCP disabled behaves exactly like current Chat.
- Bad: inserting tool results as visible user/assistant messages; this corrupts previews, editing, deletion, and regeneration semantics.
- Bad: exposing Skill tools as `skill__activate` in the OpenAI schema while the system prompt instructs the model to call `skill_activate`.
- Bad: adding frontend shell permissions so the webview can spawn arbitrary user-configured MCP commands.
- Bad: clearing streaming state on conversation switch but leaving a global `pendingUserMessage`; that leaks the old conversation's optimistic user row into the newly selected conversation.

### 6. Tests Required
- TypeScript typecheck must cover the new `ChatStreamPayload`, `ChatToolProgressPayload`, `ToolCallRecord`, `SkillMeta`, and settings types.
- Rust tests must cover settings defaults/sanitization, old conversation deserialization with missing new fields, tool max-round stopping, timeout/cancel behavior, hidden tool transcript replay, Skill runtime tool naming, sensitive script approval, and tool-result message construction.
- Rust tests for Streamable HTTP MCP must cover notification/progress events before the response and mismatched JSON-RPC ids before the matching id.
- UI smoke tests must cover live tool progress, persisted tool trace rendering after reload, Skill switch/clear persistence, and provider-without-tools fallback.
- Capability review must confirm no frontend `shell:*` permission is added when MCP stdio stays Rust-owned.

### 7. Wrong vs Correct

#### Wrong
```tsx
// Treating tool calls as timeline messages makes reload/edit/regenerate drift.
const messages = [
  ...conversation.messages,
  { id: call.id, role: 'assistant', content: `Called ${call.name}`, timestamp: now },
]
```

#### Correct
```tsx
// Tool calls belong to the assistant response they helped produce.
const nextAssistant: ChatMessage = {
  ...assistant,
  toolCalls: [...(assistant.toolCalls ?? []), toolCallRecord],
}
```

#### Wrong
```rust
// Dropping the provider transcript breaks strict tool callers on the next turn.
messages.push(json!({ "role": "assistant", "content": visible_answer }));
```

#### Correct
```rust
// UI stays clean, while the next API request can replay the strict transcript.
assistant.tool_calls = tool_records;
assistant.api_messages = vec![
    assistant_tool_call_message,
    tool_result_message,
    final_assistant_message,
];
```

## Scenario: Chat Context Management Contract

### 1. Scope / Trigger
- Trigger: Chat context management spans Rust request construction, conversation JSON persistence, Tauri command/event payloads, and React header UI.
- Apply this contract whenever changing context usage indicators, compression summaries, `build_chat_api_messages`, or Chat context popover rendering.

### 2. Signatures
- `chat_get_context_stats(conversationId: string) -> { success: true, contextState: ConversationContextState, conversation: Conversation }`
- `chat_compress_context(conversationId: string) -> { success: true, contextState: ConversationContextState, conversation: Conversation }`
- `chat-context` event payload: `{ conversationId: string, contextState: ConversationContextState }`
- Conversation JSON persists `context_state: ConversationContextState`.

### 3. Contracts
- Context stats are backend-owned because Rust assembles the real prompt with system prompt, runtime context, skills, tool schemas, hidden `api_messages`, attachments, and summary injection.
- Frontend may render persisted `context_state` immediately, but refreshes through `chat_get_context_stats` when the active conversation/model/skill changes.
- Stored Rust fields remain snake_case (`estimated_input_tokens`, `context_window_tokens`, `context_state`); frontend types may include camelCase aliases only for compatibility.
- `ConversationContextState.segments` drives the segmented bar; React must not recompute source buckets from visible messages except in browser-only mocks.
- Compression preserves raw visible messages. It stores `summary`, `source_message_ids`, and `source_until_message_id`, then future requests inject a synthetic summary system message and skip old raw messages before the boundary.
- Compression model selection uses `Settings::effective_compression_model()`, which inherits the effective Chat model when `defaultModels.compression` is unset.
- Auto-compression runs before send when estimated usage crosses the backend threshold and there is no active fresh summary. Manual compression uses the same backend path.
- Context states may include `warning` when auto-compression failed but the uncompressed request is still estimated to fit; the Context popover must display this warning without hiding the visible conversation.
- Editing, deleting, or regenerating a message at or before the summary boundary must mark the summary stale and ignore it in future request construction until recompressed.

### 4. Validation & Error Matrix
- Missing conversation file -> command invoke fails; frontend surfaces the error in the Chat error path.
- Not enough old complete messages to compress -> `chat_compress_context` returns a user-visible error and leaves raw messages unchanged.
- Compression provider missing key/model -> compression returns a clear model/key error; normal sends should continue when the original request can still be sent.
- Auto-compression failure before send while still under the estimated window -> backend saves/emits the current context state with `warning` and continues with the uncompressed request.
- Auto-compression failure before send when estimated usage is at/over the model window -> backend removes the newly persisted user message, recomputes/emits `context_state`, and returns a clear error suggesting manual compression or a larger-context model.
- Model call failure after optimistic user message persistence -> backend removes that user message, recomputes `context_state`, emits `chat-context`, and saves the corrected conversation.
- Stale summary present -> request builders ignore the summary and include raw history.

### 5. Good/Base/Bad Cases
- Good: `ContextIndicator` reads `contextState.segments` and renders the backend-provided source labels, colors, and token estimates.
- Good: `build_chat_api_messages` injects the summary only when it is non-stale and its boundary message still exists.
- Good: visible history stays intact after compression; only future request payloads are shortened.
- Base: a new conversation with no messages shows an estimated/unknown context state and can refresh without failing.
- Bad: deleting old visible messages as the compression mechanism.
- Bad: computing production context usage only from React-visible messages; this misses tool schemas, hidden transcript replay, runtime prompt, and skills.
- Bad: continuing to show a context state that includes a user message after `chat_send_message` rolls that message back on provider failure.

### 6. Tests Required
- TypeScript typecheck must cover `ConversationContextState`, `ContextUsageSegment`, and `chat-context` event payload usage.
- Rust tests must cover token estimation, summary injection, stale-summary ignore behavior, boundary invalidation, and hidden tool transcript replay.
- `npm run build:ui` must pass after context indicator layout or Chat header changes.
- `cargo test --manifest-path src-tauri/Cargo.toml` must pass after any request-builder or compression-boundary change.

### 7. Wrong vs Correct

#### Wrong
```tsx
const tokens = conversation.messages.reduce((sum, message) => {
  return sum + estimateTokens(message.content)
}, 0)
```

#### Correct
```tsx
const result = await chatApi.getContextStats(conversation.id)
setContextState(result.contextState)
```

#### Wrong
```rust
conversation.messages.drain(..boundary_index);
```

#### Correct
```rust
conversation.context_state.summary = Some(ConversationContextSummary {
    source_until_message_id,
    source_message_ids,
    content: summary_content,
    stale: false,
    ..summary_metadata
});
```
