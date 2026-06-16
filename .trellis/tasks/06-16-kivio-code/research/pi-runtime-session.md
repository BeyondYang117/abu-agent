# Research: PI agent runtime / loop / session-state, mapped to Kivio's Rust runtime

- **Query**: Study PI agent's runtime/loop/session/compaction/skills (TypeScript) and compare to Kivio's Rust `chat/agent/` runtime, to plan "Kivio Code"
- **Scope**: mixed (PI internal at `/Users/zmair/ZM database/Kivio agent/pi`, Kivio internal at `/Users/zmair/ZM database/Kivio agent/kivio`)
- **Date**: 2026-06-16

PI is a TypeScript monorepo. The layers relevant here:
- `packages/agent/src/` — the **low-level loop** (`agent-loop.ts`), the stateful `Agent` wrapper (`agent.ts`), and the **`AgentHarness`** (`harness/`) which adds sessions, compaction, skills, prompt templates, queues, hooks.
- `packages/coding-agent/src/core/` — the actual coding-agent app (`agent-session.ts`, `session-manager.ts`, `system-prompt.ts`) that drives the harness with auto-compaction, model registry, settings.

---

## 1. Agent loop shape (PI)

### 1.1 Three layers
1. **`agentLoop()` / `runAgentLoop()`** — `packages/agent/src/agent-loop.ts:31,95`. Pure function over `(prompts, context, config, signal, streamFn)`. Returns an `EventStream<AgentEvent, AgentMessage[]>`. Works on `AgentMessage[]` throughout; only converts to provider `Message[]` at the LLM-call boundary via `config.convertToLlm` (`agent-loop.ts:289`).
2. **`Agent`** — `agent.ts:166`. Stateful wrapper owning the transcript (`MutableAgentState`), a listener set, and two queues (steering, follow-up). `prompt()`/`continue()` start a run; `abort()`, `waitForIdle()`, `reset()` manage lifecycle. Single active run enforced (`agent.ts:328`).
3. **`AgentHarness`** — `harness/agent-harness.ts:174`. Wraps `runAgentLoop` per turn (`executeTurn`, line 553) and adds: session persistence, compaction/branch-summary, skills + prompt-template invocation, per-turn `streamOptions`, model/thinkingLevel/active-tool mutation, hook system (`on(...)`, `subscribe(...)`), and three queues (steer/followUp/nextTurn).

### 1.2 The inner loop (`runLoop`, `agent-loop.ts:155`)
```
outer loop (re-enters when getFollowUpMessages() returns work):
  inner loop while (hasMoreToolCalls || pendingMessages):
    emit turn_start
    inject pending steering messages (message_start/end + push to context)
    message = streamAssistantResponse(...)        # one provider call
    if stopReason in {error, aborted}: emit turn_end+agent_end; RETURN
    toolCalls = message.content.filter(type==toolCall)
    if toolCalls: executeToolCalls(...) -> ToolResultMessage[]; push each to context
    emit turn_end {message, toolResults}
    config.prepareNextTurn?(...)  -> may swap context/model/thinkingLevel
    if config.shouldStopAfterTurn?(...): emit agent_end; RETURN
    pendingMessages = getSteeringMessages()
  followUp = getFollowUpMessages(); if present -> pendingMessages = followUp; continue
  else break
emit agent_end
```
- **Turn = one assistant response + its tool calls/results.** There is **no fixed max-step / max-round counter** in the loop itself. Termination is driven by: (a) the assistant emitting *no* tool calls (`hasMoreToolCalls=false`), (b) `shouldStopAfterTurn` returning true, (c) error/abort, or (d) every tool in a batch setting `result.terminate === true` (`shouldTerminateToolBatch`, `agent-loop.ts:544`). The coding-agent layer adds compaction-driven stop, not a numeric step cap.

### 1.3 Streaming (`streamAssistantResponse`, `agent-loop.ts:275`)
- Applies optional `transformContext` (AgentMessage→AgentMessage), then `convertToLlm`, builds `Context {systemPrompt, messages, tools}`, resolves API key dynamically (`getApiKey`), calls `streamFn` (default `streamSimple` from `@earendil-works/pi-ai`).
- Consumes a typed event stream: `start` / `text_*` / `thinking_*` / `toolcall_*` / `done` / `error`. The **partial** assistant message is mutated in place in `context.messages` and re-emitted as `message_update` each delta; on `done`/`error` the final message replaces the partial.
- **Streaming contract** (`types.ts:24`): `streamFn` must NOT throw for request/model failures — failures are encoded as a final `AssistantMessage` with `stopReason: "error" | "aborted"` and `errorMessage`. This is the same "never throw, encode in result" discipline Kivio's model adapters follow.

### 1.4 Tool-call dispatch (`executeToolCalls`, `agent-loop.ts:373`)
- Mode is `"parallel"` (default) or `"sequential"` (`ToolExecutionMode`, `types.ts:36`). A single tool can force sequential via `tool.executionMode === "sequential"` (`agent-loop.ts:381`).
- Per tool: `prepareToolCall` → resolve tool, `prepareArguments` shim, `validateToolArguments`, `beforeToolCall` hook (can `{block:true}`), abort check → `executePreparedToolCall` (calls `tool.execute(id, args, signal, onUpdate)`, streams `tool_execution_update`) → `afterToolCall` hook (can override content/details/isError/terminate). Errors are caught and turned into an error `ToolResultMessage` — tools "throw on failure", the loop encodes it (`createErrorToolResult`, `agent-loop.ts:716`).
- Parallel mode preflights sequentially, runs concurrently, then emits `tool_execution_end` in completion order but emits the **tool-result message artifacts in assistant source order** (`agent-loop.ts:502`).

### 1.5 Stop / cancel
- `AbortSignal` threaded everywhere. `Agent.abort()` aborts the active run's controller (`agent.ts:300`). `AgentHarness.abort()` (line 1005) also clears steer/followUp queues and `waitForIdle`s.
- Cancellation is checked between tools (`signal?.aborted` breaks the batch loop) and inside the stream. Aborted runs produce a synthetic failure `AssistantMessage` with `stopReason:"aborted"`.

### 1.6 Error / retry handling
- Provider-level retry/backoff lives in `pi-ai`'s `streamSimple` (`maxRetries`, `maxRetryDelayMs` in `SimpleStreamOptions`), not in the loop. The loop only sees the final encoded `error` message.
- The **coding-agent** layer adds an *application* retry: on an error message it strips the failed assistant message and runs auto-compaction (`agent-session.ts:1840-1846`, `_runAutoCompaction("overflow", willRetry=true)`), then continues — i.e. context-overflow recovery is a retry-after-compact, not a transport retry.

---

## 2. Message / state model (PI)

### 2.1 Message union (`packages/agent/src/types.ts:300-309`, `harness/messages.ts`)
`AgentMessage = Message (pi-ai) | CustomAgentMessages[...]`. Base `Message` = `UserMessage | AssistantMessage | ToolResultMessage`. The harness extends the union via TS declaration merging (`harness/messages.ts:54`) with four **custom roles**:
- `bashExecution` — a ran command + output (toggleable `excludeFromContext`).
- `custom` — extension-injected content with `customType`, `display`, `details`.
- `branchSummary` — summary of an abandoned tree branch.
- `compactionSummary` — summary of compacted history.

Content blocks: `TextContent`, `ImageContent {data(base64), mimeType}`, `ThinkingContent {thinking}`, `ToolCall {id,name,arguments}` (session-format.md:44-70).

`AssistantMessage` carries `api, provider, model, usage, stopReason ("stop"|"length"|"toolUse"|"error"|"aborted"), errorMessage?, timestamp`. `ToolResultMessage` = `{toolCallId, toolName, content[], details?, isError, timestamp}`.

### 2.2 Tool-call ↔ result representation
- Tool calls are **content blocks inside the assistant message** (`AgentToolCall`, `types.ts:47`), not separate messages.
- Each result is a standalone `ToolResultMessage` keyed by `toolCallId`/`toolName` (`agent-loop.ts:733`). They are pushed into the transcript right after the assistant message that called them.

### 2.3 How history threads to the model (`convertToLlm`, `harness/messages.ts:120`)
- `user`/`assistant`/`toolResult` pass through unchanged.
- `bashExecution` → a `user` text message via `bashExecutionToText` (or dropped if `excludeFromContext`).
- `custom` → `user` message.
- `branchSummary` / `compactionSummary` → `user` message wrapped in `<summary>…</summary>` with a prefix (`BRANCH_SUMMARY_PREFIX` / `COMPACTION_SUMMARY_PREFIX`).
- Anything else filtered out. **Tool calls stay native blocks** (no flattening), so providers get real tool_use/tool_result pairs.

### 2.4 `Agent` state shape (`AgentState`, `types.ts:317-342`)
`{systemPrompt, model, thinkingLevel, tools (accessor-copied), messages (accessor-copied), isStreaming, streamingMessage?, pendingToolCalls:Set, errorMessage?}`. `processEvents` (`agent.ts:509`) reduces loop events into this state (push on `message_end`, track `pendingToolCalls` on tool start/end, capture `errorMessage` on `turn_end`).

---

## 3. System prompt + prompt templates (PI)

### 3.1 Harness system prompt (`harness/agent-harness.ts:339-351`)
`systemPrompt` is either a string or a callback `(env, session, model, thinkingLevel, activeTools, resources) => string`, evaluated fresh **per turn** in `createTurnState`. Default fallback: `"You are a helpful assistant."`.

### 3.2 Coding-agent system prompt (`coding-agent/src/core/system-prompt.ts:28`)
`buildSystemPrompt({customPrompt?, selectedTools, toolSnippets, promptGuidelines, appendSystemPrompt, cwd, contextFiles, skills})`:
- Default prompt is `"You are an expert coding assistant operating inside pi…"` + an **Available tools** list (only tools that have a one-line `toolSnippets[name]`), **Guidelines** (dedup'd; e.g. "Use bash for file operations…", "Be concise", "Show file paths clearly"), then a pi-docs block.
- `customPrompt` replaces the default body entirely but still gets the appended sections.
- Appends `<project_context>` with each context file as `<project_instructions path="…">…</project_instructions>`, then the **skills block** (only if `read` tool present), then `Current date:` and `Current working directory:` last.

### 3.3 Skills block in prompt (`harness/skills.ts:formatSkillsForSystemPrompt`, line 3)
Emits `<available_skills><skill><name/><description/><location/></skill>…</available_skills>` — name, model-visible description, and **absolute file path** so the model can `read` the full SKILL.md on demand. Skills with `disableModelInvocation` are hidden from the listing but still invocable explicitly.

### 3.4 Prompt templates (`harness/prompt-templates.ts`)
Markdown files (`.md`) with optional frontmatter (`description`, `argument-hint`). `name` = filename. `substituteArgs` supports `$1`, `$@`, `$ARGUMENTS`, `${@:N}`, `${@:N:L}` (`prompt-templates.ts:249`). Invoked via `harness.promptFromTemplate(name, args)` which formats the body into a user prompt.

---

## 4. Session persistence — the JSONL-per-session model (PI)

This is the largest gap vs Kivio. Source: `harness/session/*`, `harness/types.ts:334-490`, `coding-agent/docs/session-format.md`.

### 4.1 On-disk layout
- Path: `~/.pi/agent/sessions/--<cwd-with-slashes-as-dashes>--/<timestamp>_<uuid>.jsonl` (`jsonl-repo.ts:34,65`, `encodeCwd` strips leading slash and replaces `/`,`\`,`:` with `-`; filename = ISO timestamp with `:`/`.`→`-` plus session id).
- **One JSONL file per session.** First line = `SessionHeader`; every subsequent line = one append-only `SessionTreeEntry`.

### 4.2 Header (`jsonl-storage.ts:8`)
```json
{"type":"session","version":3,"id":"<uuid>","timestamp":"<ISO>","cwd":"<dir>","parentSession":"<path?>"}
```
`version:3` (v1 linear, v2 tree, v3 renamed `hookMessage`→`custom`). `parentSession` set when forked/cloned.

### 4.3 Entry tree (`harness/types.ts:334-420`)
All entries (except header) extend `{type, id (8-char), parentId|null, timestamp(ISO)}` and form a **tree via id/parentId** — branching is in-file, no new file. Entry types:
| `type` | payload | in LLM context? |
|---|---|---|
| `message` | `{message: AgentMessage}` | yes |
| `model_change` | `{provider, modelId}` | settings only |
| `thinking_level_change` | `{thinkingLevel}` | settings only |
| `active_tools_change` | `{activeToolNames[]}` | settings only |
| `compaction` | `{summary, firstKeptEntryId, tokensBefore, details?, fromHook?}` | yes (as summary) |
| `branch_summary` | `{fromId, summary, details?, fromHook?}` | yes (as summary) |
| `custom` | `{customType, data?}` | **no** (extension state) |
| `custom_message` | `{customType, content, display, details?}` | yes |
| `label` | `{targetId, label?}` | no (bookmark) |
| `session_info` | `{name?}` | no (display name) |
| `leaf` | `{targetId|null}` | no — records active leaf after branch navigation |

### 4.4 Storage / repo (`jsonl-storage.ts`, `jsonl-repo.ts`)
- `JsonlSessionStorage` keeps in-memory `entries[]`, `byId` map, `labelsById`, `currentLeafId`. `appendEntry` JSON-stringifies one line and `appendFile`s it, then updates caches + `currentLeafId = leafIdAfterEntry(entry)` (a `leaf` entry's target, else its own id). Open = read whole file, parse header + each line.
- `getPathToRoot(leafId)` (`jsonl-storage.ts:275`) walks `parentId` to root and reverses → the **branch** used to build context.
- `JsonlSessionRepo` provides `create / open / list / delete / fork`. **`list`** scans per-cwd dirs (or all), parses just the header of each `.jsonl`, sorts by `createdAt` desc (`jsonl-repo.ts:102`). **`fork`** copies entries up to an `entryId` into a new file with `parentSession` set.
- There's also `MemorySessionStorage` / `MemorySessionRepo` (in-memory, no file) for ephemeral/SDK use.

### 4.5 `Session` API (`session/session.ts:82`)
Append helpers (each writes one entry, returns its id): `appendMessage`, `appendThinkingLevelChange`, `appendModelChange`, `appendActiveToolsChange`, `appendCompaction`, `appendCustomEntry`, `appendCustomMessageEntry`, `appendLabel`, `appendSessionName`. Navigation: `getLeafId`, `getEntry`, `getEntries`, `getBranch(fromId?)`, `moveTo(entryId, summary?)` (sets leaf, optionally appends a branch_summary), `buildContext()`.

### 4.6 Context building (`buildSessionContext`, `session/session.ts:22`)
Walks the path entries and:
- Extracts latest `model`, `thinkingLevel`, `activeToolNames` from change entries (and from assistant messages' provider/model).
- If a `compaction` entry is on the path: emit its summary first, then messages from `firstKeptEntryId`→compaction, then messages after compaction. Otherwise emit all messages in order. (`compaction`/`branch_summary`/`custom_message` are converted to their summary/user message forms.)

### 4.7 Resume / continue / listing (coding-agent)
`SessionManager` (`coding-agent/src/core/session-manager.ts`) wraps this with static `create`, `open(path)`, **`continueRecent(cwd)`** (line 1420 — open most-recent or create new), `inMemory`, `forkFrom`, `list(cwd)`, `listAll()`. The TUI `/resume` selects a session; `/fork`, `/clone`, `/tree`, `/name`, `/compact` map to the above. Deleting a session = deleting its `.jsonl`.

### 4.8 Harness ↔ session write path (`agent-harness.ts`)
- Each turn, `createTurnState` calls `session.buildContext()` to get messages + settings (`agent-harness.ts:332`).
- Loop events are persisted in `handleAgentEvent` (line 510): `message_end` → `session.appendMessage`; `turn_end` → `flushPendingSessionWrites` + emit `save_point`.
- **`pendingSessionWrites`** (line 484) batches non-message mutations (model/thinking/active-tools/custom/label/session-name/leaf) made *during* a run, flushed at turn boundaries via `prepareNextTurn` / `turn_end` / `agent_end`. This is the crash-safety + ordering mechanism.

---

## 5. Compaction & branch summarization (PI)

Source: `harness/compaction/compaction.ts`, `branch-summarization.ts`, `utils.ts`; coding-agent driver in `agent-session.ts:1800-1940`; docs `coding-agent/docs/compaction.md`.

### 5.1 When it triggers
- **Auto** (coding-agent `_maybeAutoCompact`/`_runAutoCompaction`): two cases — **overflow** (last assistant errored on context overflow → strip failed message, compact, retry) and **threshold** (`shouldCompact(contextTokens, contextWindow, settings)` where `contextTokens > contextWindow - reserveTokens`, `compaction.ts:196`). `contextTokens` is taken from the last successful assistant `usage` (real provider tokens), or estimated for error messages.
- **Manual** `/compact [instructions]` → `harness.compact(customInstructions?)` (`agent-harness.ts:708`).
- Defaults (`DEFAULT_COMPACTION_SETTINGS`, `compaction.ts:112`): `enabled:true, reserveTokens:16384, keepRecentTokens:20000`. Configurable in `~/.pi/agent/settings.json` or `<proj>/.pi/settings.json`.

### 5.2 How compaction works (`prepareCompaction` → `compact`)
1. `prepareCompaction(pathEntries, settings)` (`compaction.ts:542`): find previous compaction boundary (`firstKeptEntryId`), compute `tokensBefore` from rebuilt context, `findCutPoint(...)` walks backwards accumulating `estimateTokens` until `keepRecentTokens` reached, snapping to a **valid cut point** (user/assistant/bashExecution/custom_message/branch_summary — **never a toolResult**, `findValidCutPoints` line 261). Splits messages into `messagesToSummarize` and (if a single turn exceeds budget) `turnPrefixMessages` + `isSplitTurn`.
2. `compact(...)` (`compaction.ts:627`): calls `generateSummary` with a strict structured-summary prompt (Goal / Constraints / Progress(Done/InProgress/Blocked) / Key Decisions / Next Steps / Critical Context). On a split turn it *also* generates a turn-prefix summary and merges. Uses iterative update prompt (`UPDATE_SUMMARIZATION_PROMPT`) when a `previousSummary` exists. Appends `<read-files>`/`<modified-files>` blocks from cumulative `fileOps`.
3. `session.appendCompaction(summary, firstKeptEntryId, tokensBefore, details, fromHook)` writes the entry; next `buildContext` uses summary + kept tail.
- **Token estimation** (`estimateTokens`, `compaction.ts:220`): char-heuristic `ceil(chars/4)`, images ≈ 4800 chars. `estimateContextTokens` prefers the last assistant `usage.totalTokens` + estimated trailing tokens (`compaction.ts:165`) — i.e. anchors on real provider counts and only estimates the tail.

### 5.3 Branch summarization (`branch-summarization.ts`)
Triggered by `/tree` navigation (`harness.navigateTree`, `agent-harness.ts:764`). `collectEntriesForBranchSummary` finds the deepest common ancestor between old leaf and target, collects the abandoned-branch entries, `prepareBranchEntries` budgets newest-first, `generateBranchSummary` produces a structured summary with `BRANCH_SUMMARY_PREAMBLE`. Stored as a `branch_summary` entry at the navigation point.

### 5.4 Serialization for summary (`utils.ts:serializeConversation`)
Renders `[User]: …`, `[Assistant thinking]: …`, `[Assistant]: …`, `[Assistant tool calls]: read(path="…"); …`, `[Tool result]: …`. Tool results truncated to 2000 chars. This is fed as `<conversation>…</conversation>` so the summarizer doesn't "continue" the chat (`SUMMARIZATION_SYSTEM_PROMPT`, `compaction.ts:379`).

### 5.5 Hooks
`session_before_compact` / `session_before_tree` can `{cancel:true}` or supply a custom `summary`/`compaction` (`agent-harness.ts:723,787`). Cumulative file tracking accumulates across nested compactions/branch summaries (`extractFileOperations`, prev-entry `details`).

---

## 6. Skills (PI)

- **Discovery** (`harness/skills.ts:loadSkills`, line 49): recursively walk dirs, load `SKILL.md` files (one per dir, stops at first) and root-level `.md`, honoring `.gitignore`/`.ignore`/`.fdignore`. Parse YAML frontmatter (`name`, `description`, `disable-model-invocation`). Validate name (lowercase a-z0-9-, ≤64, matches parent dir) and description (required, ≤1024). `loadSourcedSkills` tags each skill with provenance.
- **Listing** in system prompt: §3.3 above.
- **Activation**: explicit via `harness.skill(name, additionalInstructions?)` (`agent-harness.ts:645`) which formats `<skill name=… location=…>…body…</skill>` (`formatSkillInvocation`) and runs it as a turn. The model can also self-activate by reading the SKILL.md path. There is **no separate runtime re-permitting of tools** at the agent layer — skills are prompt-injection + file access; the coding-agent's `setActiveTools` is the tool-gating knob.

---

## 7. Kivio's existing Rust runtime (current state)

Source: `src-tauri/src/chat/agent/*`, `chat/storage.rs`, `chat/types.rs`, `chat/model/`.

### 7.1 Loop (`agent/loop_.rs:109 run_agent_loop`)
Phase pipeline threaded by `LoopEnv`/`RunState`: **prepare** (tool assembly happens before the loop, `RunState.base_tools`) → per-round **planning_step** → **run_tool_round** → optional skill re-filter → `persist_partial_assistant` checkpoint → repeat; then **synthesis_step** → **finalize**. Decoupled from Tauri via **`AgentHost` trait** (`host.rs`) and tools run via **`ToolExecutor`** (`execute.rs`). Cancellation = `host.is_generation_active(conv, generation)` checked at the top of each round (`loop_.rs:153`); generation counter is the cancel token. Round/step uses a `u32 round` + `u8 step_number` but the *break* conditions are phase outcomes (`PlanningStepOutcome::FinalAnswer`, `ToolRoundOutcome::RoundLimit`) — there **is** a real round limit (`tool_round_limit_reached`) unlike PI. Reasoning + usage accumulation tracked in `RunState` (`merge_usage`).
- **T3 mid-run skill re-filtering** (`loop_.rs:77 apply_activated_tool_filter`): recomputes effective `tools` from `base_tools` narrowed by the union of activated skills' allowed-tools — order-independent, monotonic. PI has no equivalent (PI gates tools via `setActiveTools`, not skill-activation).

### 7.2 Host / executor
`AgentHost` (`host.rs:10`): `emit_stream_delta`, `emit_stream_done`, `emit_tool_record`, `persist_partial_assistant`, `request_tool_approval`, `request_session_consent`, `request_user_response` (ask-user), `is_generation_active`, `wait_for_generation_inactive`. Equivalent to PI's event listeners + hooks, but Kivio's is a **trait with concrete chat/sub-agent impls** rather than PI's `subscribe()`/`on()` event bus.

### 7.3 Message / state model
- **`RunState.runtime_messages: Vec<serde_json::Value>`** — the working transcript is **raw OpenAI-style JSON messages**, not a typed union. `generated_api_messages` is the persisted mirror.
- Persisted message = **`ChatMessage`** (`types.rs:231`): `{id, role(String), content(String), attachments, reasoning?, artifacts, tool_calls: Vec<ToolCallRecord>, segments: Vec<ChatMessageSegment>, api_messages: Vec<Value>, model_messages: Vec<ModelMessage>, active_skill_id?, run_entry?, stream_outcome?, usage?, timestamp}`.
- **Tool calls are metadata** (`ToolCallRecord`, `types.rs:162`) attached to the assistant `ChatMessage`, PLUS the raw OpenAI `tool_calls`/`role:tool` pairs replayed via `api_messages`/`model_messages`. PI instead keeps tool calls as native content blocks and tool results as separate `ToolResultMessage`s in one flat transcript.
- **Segments** (`ChatMessageSegment`, `types.rs:214`) carry the UI-render structure (text/reasoning/tool, phase, order, round) — PI has no equivalent; PI replays the typed message stream directly.

### 7.4 Compaction (`agent/compaction.rs`)
`maybe_compact_send_view` — a **two-layer, send-view** compactor invoked inside the loop:
- **L1 snip** (`snip_old_tool_results`): for non-recent (`KEEP_RECENT_RAW_MESSAGES=8`) `role:tool` messages over `SNIP_THRESHOLD_CHARS=4000`, replace middle with `[… N chars snipped …]` (head½ + tail¼). Affects only the send view.
- **L2 model summary**: if still over `COMPACT_TRIGGER_RATIO=0.85 * context_window`, split into (system-prefix, old-segment, recent-tail), summarize old-segment via a one-shot LLM call, replace with a `user`/`assistant` summary pair, and **write back to `runtime_messages`** so later rounds reuse it. Falls back to L1 on failure/cancel. `estimate_messages_tokens` = same char/4 heuristic.
- Trigger anchors on **estimated** tokens (not provider usage like PI), uses `context_window_for_model` from `model_metadata`.

### 7.5 Storage (`chat/storage.rs`)
- **One JSON file per conversation**: `{app_data}/conversations/{conv_id}.json` holding the **entire `Conversation`** (`types.rs:285`: id, title, provider_id, model, `messages: Vec<ChatMessage>`, assistant snapshot, todo/plan state, context_state, …). Plus `index.json` (list metadata), `projects.json`, `assistants.json`.
- **Save = rewrite the whole conversation file atomically** (`save_conversation`, line 368; temp-file + rename, 3 retries). No append-only log, no per-entry tree, no branching, no leaf/parentId. `save_conversation_without_index` for hot-path saves.
- Crash-safety: `persist_partial_assistant` (host) rewrites a draft of the in-progress assistant message after each tool round; final write replaces same `message_id`.
- Listing rebuilds from `index.json` or by scanning files (`load_conversation_list_from_files`).

---

## 8. PI ↔ Kivio mapping (what's equivalent / missing / to build)

### 8.1 Equivalent — Kivio already has it (reuse directly)
| Capability | PI | Kivio |
|---|---|---|
| Loop decoupled from UI | `AgentEventSink` + `subscribe`/`on` | `AgentHost` trait (`host.rs`) |
| Tool execution abstraction | `tool.execute` + before/after hooks | `ToolExecutor` (`execute.rs`) + `request_tool_approval`/consent |
| Streaming deltas (text/reasoning/tool) | `message_update` events | `emit_stream_delta` / segments |
| Cancellation | `AbortSignal` | `is_generation_active(generation)` / `wait_for_generation_inactive` |
| Provider-agnostic model layer | `streamSimple` + `Model` | `chat/model/` (openai.rs + anthropic.rs peers) |
| Crash-safety checkpoint mid-run | `pendingSessionWrites` + `save_point` | `persist_partial_assistant` |
| Per-call usage accumulation | `AssistantMessage.usage` | `RunState.merge_usage` / `AgentRunResult.usage` |
| Round/step bounding | (none; relies on no-tool-call) | real round limit (`ToolRoundOutcome::RoundLimit`) |
| Mid-run tool gating from skills | `setActiveTools` (manual) | T3 `apply_activated_tool_filter` (automatic) |
| System-prompt skills block (XML, path-based) | `formatSkillsForSystemPrompt` | Kivio skills already inject similarly (`skills/`) |

### 8.2 PI does it, Kivio's loop lacks (candidate to build for Kivio Code)
- **Typed message union with custom roles.** PI threads `AgentMessage` (incl. `bashExecution`, `custom`, `branchSummary`, `compactionSummary`) and only flattens at `convertToLlm`. Kivio threads raw OpenAI `Value` and re-derives `model_messages` separately. A Kivio-Code terminal agent that runs shell heavily would benefit from a first-class `bashExecution`-like representation.
- **Steering / follow-up / next-turn queues.** PI's `Agent`/`AgentHarness` let the user inject messages mid-run (steer), after-stop (followUp), or pre-next-turn (nextTurn) with `"all"`/`"one-at-a-time"` drain modes. Kivio's loop has no in-run message injection.
- **Session-tree branching + `/tree` navigation + branch summarization.** Kivio has no tree, no branch, no branch-summary.
- **Compaction anchored on real provider usage + structured checkpoint summary + split-turn handling + cumulative file tracking.** Kivio's L2 is a single freeform summary on an *estimated* trigger, written into the send-view working copy; it lacks the structured Goal/Progress/NextSteps format, split-turn prefix summaries, `firstKeptEntryId` boundary semantics, and read/modified-file accumulation.
- **`prepareNextTurn` model/thinking swap mid-run** and per-turn `streamOptions` patch hooks (`before_provider_request`/`before_provider_payload`).
- **Prompt templates** with `$1/$@/$ARGUMENTS/${@:N:L}` substitution (PI `harness/prompt-templates.ts`). Kivio has skills but not arg-substituted prompt templates.
- **`cwd`-scoped session store** (sessions keyed by working directory) — essential for a terminal coding agent that operates per-project.

### 8.3 Session-storage gap (Kivio Code's biggest build item)
Kivio persists **one mutable whole-conversation JSON blob rewritten on every save**. PI persists an **append-only JSONL log of a session tree**, one file per session, keyed by `cwd`, with header + typed entries (`message`, `model_change`, `thinking_level_change`, `active_tools_change`, `compaction`, `branch_summary`, `custom`, `custom_message`, `label`, `session_info`, `leaf`) linked by `id`/`parentId`, and context rebuilt by walking leaf→root.

To match PI for Kivio Code, the new runtime needs:
1. A **JSONL append-only session file** format (header line + entry lines), path `<root>/--<cwd>--/<ts>_<id>.jsonl`.
2. A **tree model** (`id`, `parentId`, current `leaf`) supporting in-file branching + `getPathToRoot`.
3. Typed **entry enum** mirroring PI's `SessionTreeEntry`.
4. **`buildContext`** that folds compaction/branch-summary entries into the LLM message list.
5. Repo ops: `create`, `open`, `continueRecent(cwd)`, `list(cwd)`/`listAll`, `delete`, `fork`.
6. Append helpers writing one entry each (cheap, no full-file rewrite) — the inverse of Kivio's current "rewrite the whole conversation".
7. Optional `MemorySessionStorage` analogue for ephemeral/sub-agent runs.

Note: Kivio's current persistence (conversation JSON + index + projects/assistants) is tuned for the **chat product UI** (folders, projects, pinning, assistants). Kivio Code can either (a) introduce a parallel JSONL session store for the terminal agent, or (b) port the existing loop to write through a new session abstraction. PI keeps the loop (`packages/agent`) storage-agnostic and the app (`coding-agent`) chooses JSONL vs memory — that separation (loop ← `Session` trait → JSONL/memory backends) is the recommended shape to copy.

---

## 9. Reuse vs Build — Kivio Code runtime/session layer

| Area | Reuse from Kivio | Build for Kivio Code | Borrow PI design |
|---|---|---|---|
| Loop orchestration | `agent/loop_.rs` phases, `LoopEnv`/`RunState` | — | turn = assistant+tools (already matches) |
| Host/UI decoupling | `AgentHost` trait | terminal-host impl | PI listener/hook split |
| Tool exec + approval | `ToolExecutor`, consent | shell/file tool set for terminal | PI before/afterToolCall, `terminate` flag, sequential/parallel modes |
| Model layer | `chat/model/` (openai+anthropic) | — | PI `streamFn` "never throw, encode error" contract (already followed) |
| Cancellation | generation counter | — | PI AbortSignal-everywhere (parity) |
| Message model | — | typed message enum incl. `bashExecution`-style | PI `AgentMessage` union + `convertToLlm` flattening |
| In-run message injection | — | **steer / followUp / nextTurn queues** | PI `PendingMessageQueue` + `QueueMode` |
| Compaction | `agent/compaction.rs` L1 snip (cheap pre-pass) | **structured checkpoint summary, usage-anchored trigger, split-turn, file tracking, `firstKeptEntryId` semantics** | PI `prepareCompaction`/`compact`/`findCutPoint`, `DEFAULT_COMPACTION_SETTINGS` |
| Branch summarization | — | **`/tree` navigation + branch summary** (only if tree adopted) | PI `collectEntriesForBranchSummary`/`generateBranchSummary` |
| Session persistence | atomic-write helper, attachment externalization | **JSONL append-only, per-cwd, session tree, leaf/parentId, entry enum, buildContext, repo create/open/list/fork/continueRecent** | PI `harness/session/*` (port wholesale conceptually) |
| Crash safety | `persist_partial_assistant` | adapt to append-only (`pendingSessionWrites` batching) | PI `flushPendingSessionWrites` + `save_point` |
| Skills | `skills/` discovery + prompt block + T3 filter | per-cwd/project skill dirs | PI `loadSkills` (.ignore-aware, SKILL.md validation) |
| Prompt templates | — | **arg-substituted prompt templates** | PI `prompt-templates.ts` (`$1/$@/${@:N:L}`) |
| System prompt | reuse Kivio prompt composition | terminal/coding default + project context files + cwd/date | PI `buildSystemPrompt` (tool snippets, guidelines dedup, `<project_context>`) |

## Caveats / Not Found
- Did not exhaustively read `coding-agent/src/core/agent-session.ts` (105 KB) or `session-manager.ts` (48 KB) line-by-line; covered their session/compaction wiring via targeted grep + the relevant ~100-line spans. The harness layer (`packages/agent/src/harness/`) — which is what a fresh Rust port should mirror — was read in full.
- `packages/agent/src/proxy.ts`, `node.ts`, and `harness/env/nodejs.ts` (the Node `ExecutionEnv`/`FileSystem`/`Shell` impl) were listed but not deeply read; they are the Node-specific I/O backend behind the `ExecutionEnv` interface (`harness/types.ts:268-332`), which Kivio would replace with Rust `std::fs`/`tokio::process` — the *interface* (in `types.ts`) is the load-bearing part and was read in full.
- PI's actual numeric round/step behavior: confirmed the *loop* has no max-step cap (`agent-loop.ts`); the coding-agent relies on no-tool-call + compaction. Kivio's round limit is in `rounds.rs` (`tool_round_limit_reached`) — exact value not read here.
