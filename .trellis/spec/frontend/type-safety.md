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
- Chat model defaults live in settings as `chatProviderId` and `chatModel`; when absent, backend sanitization falls back to Lens provider/model, then translator provider/model.
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
- `cargo test --manifest-path src-tauri/Cargo.toml` must include settings fallback assertions when changing chat defaults.
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
- Frontend TypeScript converts persisted snake_case fields at the bridge/type boundary or explicitly models existing snake_case fields, matching current Chat conventions.
- Tool traces are metadata on the related assistant message, not standalone `role: 'tool'` timeline messages in the UI.
- Skill selection is conversation-pinned and user-switchable. Sending a message snapshots the active Skill ID onto the generated assistant message.
- Tool approval policy defaults to read-only tools auto-running and sensitive tools requiring confirmation.
- MCP stdio process control is Rust-owned. Frontend must not use `@tauri-apps/plugin-shell` to spawn MCP servers or require `shell:*` capability for this feature.
- Native `web_search` is a native tool, not an MCP server, but uses the same UI/event/status model as MCP tools.

### 4. Validation & Error Matrix
- Missing or disabled MCP config -> no `tools` are sent to the model; Skill-only prompt injection still works.
- Provider rejects `tools` -> surface a user-visible Chat error or disable MCP for that provider; do not break plain Chat.
- Old conversation JSON missing `active_skill_id` or `tool_calls` -> deserialize with defaults and render normally.
- MCP server imported from config -> keep disabled until the user explicitly enables it.
- MCP env values shown in UI/logs -> redact secret-looking values; never include full env secrets in tool previews.
- Tool run exceeds max rounds, timeout, or cancellation -> emit final `chat-tool` status and a `chat-stream` completion/error reason with the same `runId`.

### 5. Good/Base/Bad Cases
- Good: `MessageBubble` renders `ToolCallBlock` above assistant content by reading `message.toolCalls`.
- Good: `chat-tool` events patch only the matching `{ conversationId, runId }`, preventing stale events from another run updating the visible conversation.
- Base: a conversation with no Skill and MCP disabled behaves exactly like current Chat.
- Bad: inserting tool results as visible user/assistant messages; this corrupts previews, editing, deletion, and regeneration semantics.
- Bad: adding frontend shell permissions so the webview can spawn arbitrary user-configured MCP commands.

### 6. Tests Required
- TypeScript typecheck must cover the new `ChatStreamPayload`, `ChatToolProgressPayload`, `ToolCallRecord`, `SkillMeta`, and settings types.
- Rust tests must cover settings defaults/sanitization, old conversation deserialization with missing new fields, tool max-round stopping, timeout/cancel behavior, and tool-result message construction.
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
