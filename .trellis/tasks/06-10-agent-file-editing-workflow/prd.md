# Agent File Editing Reliability PRD

## Goal

Redesign Kivio Chat's agent file editing and file writing workflow so large file creation, existing-file edits, and provider interruptions are handled reliably. The agent should not lose an entire large file because the provider stream disconnects once, and a failed file write should not crash or poison the main conversation.

The target behavior is closer to coding-focused agents such as Codex, OpenCode, Aider, and Hermes: existing code edits prefer patch/search-replace operations, long file creation uses an explicit resumable draft transaction, file mutations return inspectable diffs and metadata, and provider/tool failures are represented as scoped tool errors instead of one red failure that makes the user unsure what happened.

## Problem

Kivio currently exposes `write_file(path, content)` as a native Chat tool. The model must generate the complete file body inside one tool-call JSON argument before the backend can execute the tool.

This creates several failure modes:

- For long files, the risky part happens before the write tool starts. If the provider stream times out while generating `content`, the tool never executes and the entire generated file is lost.
- The app has a global HTTP client timeout of 60 seconds. This is reasonable for normal HTTP calls, but unreasonable for long SSE model streams where content is actively arriving.
- Existing-file edits may cause the model to regenerate a whole file even when only a small patch is needed.
- A post-tool provider failure can visually dominate the turn, hiding that file mutations may have succeeded.
- A naive chunk tool that writes directly to the target file would leave partial/corrupted target files if the model disconnects mid-write.
- The frontend can show "waiting" or "writing" without reflecting the true backend phase: generating tool arguments, executing the write, verifying, or failing.

## Current Evidence

- Current Kivio `write_file` requires `path` and full `content`.
- Current backend writes with `fs::write` only after complete arguments are decoded.
- Current `reqwest::Client` is built with a global `.timeout(Duration::from_secs(60))`.
- The latest observed error happened during `Chat tools planning` stream decoding, before a durable file mutation could finish.
- Kivio already has partial improvements for tool-call draft visibility, but not a durable/resumable write protocol.
- A half-built `write_file_chunk(start/append/finish)` exists in the working tree, but it writes/appends directly to the target file and should not be shipped as-is.

## External Reference Summary

- Codex centers code edits around `apply_patch`: the model emits patch hunks, and the runtime parses/applies them as file operations.
- OpenCode exposes `edit`, `write`, and `apply_patch`; its docs describe `edit` as the primary way to modify code and `write` as create/overwrite.
- Aider documents whole-file editing as simple but slow/costly, and uses diff/search-replace/udiff formats for targeted edits.
- Local Hermes keeps `write_file` for full replacement, but directs targeted changes to `patch`; it also uses path locks, stale-write warnings, patch failure hints, and V4A patch parsing.

See: `research/agent-file-editing-patterns.md`.

## Product Principles

- Targeted edits first; whole-file writes only when appropriate.
- Long file writes must be durable, resumable, and atomic.
- Never write partial generated content directly into the target file.
- A provider disconnect should not erase visible progress or invalidate the whole chat turn.
- File mutation state must come from backend runtime events, not frontend guesses.
- Tool success, tool failure, provider stream failure, and final answer failure must be visually separate.
- Prefer small, local Rust/React changes over heavy new dependencies.

## Users And Use Cases

### Primary User

A user asks Kivio Chat to modify a bound local project, create a website/demo, generate source files, update configuration, or produce durable local deliverables.

### Key Use Cases

- Modify a few lines in an existing file.
- Apply coordinated changes across multiple files.
- Create a small new file.
- Generate a large new file without losing all progress on one disconnect.
- Resume or retry a failed large-file write.
- Understand whether failure happened during model generation, tool execution, verification, or final assistant response.

## Proposed Direction

Use three file mutation paths:

1. `edit_file`
   - For small existing-file edits.
   - Exact old/new replacement with uniqueness enforcement.

2. `patch`
   - For multi-file or larger code edits.
   - V4A/Codex-style add/update/delete patch format.

3. `file_write_session`
   - For long new files, long explicit full-file replacement, or generated deliverables.
   - Draft-based, chunked, resumable, and atomically committed.

Keep `write_file` as a compatibility/simple-path tool for small new files and explicit full overwrites, but make prompt guidance and tool descriptions push coding work toward `edit_file`, `patch`, or `file_write_session`.

## Tool Contract

### `edit_file`

Input:

```json
{
  "path": "src/App.tsx",
  "old_string": "old text",
  "new_string": "new text",
  "replace_all": false
}
```

Requirements:

- Reject missing file.
- Reject no-op edits where old and new strings are identical.
- Require exactly one match unless `replace_all` is true.
- Preserve project path boundaries.
- Return diff metadata.

### `patch`

Input:

```json
{
  "patch": "*** Begin Patch\n*** Update File: src/App.tsx\n@@\n-old\n+new\n*** End Patch"
}
```

Supported operations:

- `*** Add File: path`
- `*** Update File: path`
- `*** Delete File: path`

MVP excludes move/rename unless already cheap to support.

Requirements:

- Patch file paths must be project-relative.
- Reject absolute paths, `~`, backslash paths, and `..` traversal.
- Validate all hunks before writing any file.
- Acquire sorted per-path locks.
- Return all affected files, additions/removals, diff, warnings, and diagnostics.

### `file_write_session`

Use a transaction-style protocol instead of direct append to the target.

#### `begin_file_write`

Input:

```json
{
  "path": "src/generated/large.ts",
  "mode": "create_or_overwrite",
  "expected_bytes": 120000,
  "expected_sha256": "optional",
  "description": "optional user-facing purpose"
}
```

Behavior:

- Resolve and validate target path.
- Create a session id.
- Create a draft file under app-managed data, not in arbitrary cache and not directly at the target.
- Capture target pre-state metadata if it exists.
- Emit a pending tool record with session id and target path.

#### `append_file_write`

Input:

```json
{
  "session_id": "fw_...",
  "offset": 32768,
  "content": "next chunk",
  "chunk_sha256": "optional"
}
```

Behavior:

- Append only if `offset` matches current draft length.
- Optionally verify chunk hash.
- Emit real bytes/chunks progress.
- Do not mutate target file.

#### `finish_file_write`

Input:

```json
{
  "session_id": "fw_...",
  "expected_bytes": 120000,
  "expected_sha256": "optional"
}
```

Behavior:

- Verify final byte count/hash when provided.
- Generate diff metadata against target pre-state.
- Atomically replace target with draft using temp file + rename.
- Clean up session draft after success.
- Return `FileMutationResult`.

#### `abort_file_write`

Input:

```json
{
  "session_id": "fw_...",
  "reason": "provider stream interrupted"
}
```

Behavior:

- Mark session aborted.
- Keep draft for a short retention window if partial recovery is possible.
- Do not mutate target file.

## Storage And Retention

Drafts should live in app-managed durable support data, not generic browser/http cache:

- macOS: Tauri app data/support directory.
- Windows: Tauri app data directory.
- Suggested subdir: `agent-file-drafts/<conversation_id>/<session_id>/`.

Retention:

- Successful sessions: delete draft immediately after commit.
- Failed/aborted sessions: retain for a short window, default 24 hours or until app cleanup.
- Include metadata JSON with target path, workspace id, offsets, hashes, created_at, updated_at, and status.

## HTTP Streaming Timeout Policy

Replace the global 60-second total timeout for model streaming calls with role-specific timeouts.

Recommended MVP:

- Non-streaming API calls: keep a finite total timeout, configurable around 60-120 seconds.
- Streaming model calls: use connect timeout plus read-idle timeout, not total 60 seconds.
- Read-idle timeout should mean "no bytes/events received for N seconds", not "stream lasted N seconds".
- Long tool-argument streams should remain cancellable by user generation token.
- Error text should distinguish:
  - provider stream idle timeout;
  - local request total timeout;
  - user cancellation;
  - invalid provider response;
  - tool argument JSON failed before executable call.

## Frontend UX

Tool blocks should show the actual backend phase:

- `generating_arguments`: model is generating tool args; no file mutation has happened.
- `drafting_file`: draft session started, target untouched.
- `writing_draft`: chunk append in progress, show bytes/chunks.
- `applying_file_change`: final atomic commit or patch is running.
- `completed`: committed mutation with diff stats.
- `failed`: scoped failure with reason.
- `aborted`: draft retained or discarded; target untouched.

Large file write UI:

- Show target path.
- Show draft bytes received.
- Show chunk count.
- Show whether target file has been touched.
- On failure, say clearly: "目标文件未被修改" when failure happened before finish.

Patch/edit UI:

- Show operation, affected files, additions/removals.
- Show expandable diff.
- Show warnings/diagnostics.

Main conversation:

- If final synthesis fails after successful tool mutation, preserve the completed tool block and show final-response failure separately.
- Do not clear the assistant preview before persisted conversation state is applied.

## Prompting Rules

Update native tool descriptions and system prompt:

- Existing-file small edit: use `edit_file`.
- Multi-file or larger code change: use `patch`.
- New small file: `write_file` is acceptable.
- Large generated file or explicit full-file replacement: use `begin_file_write` + `append_file_write` + `finish_file_write`.
- Do not repeat full saved file content after successful mutation unless user explicitly asks.
- If a draft write fails, explain the draft/target state and retry/resume if possible.

## Backend Requirements

- Implement structured `FileMutationResult` for all mutation tools.
- Implement V4A-style `patch` with add/update/delete.
- Implement file mutation path locks.
- Implement draft write sessions with metadata and atomic finish.
- Implement stale/pre-state checks for existing target overwrite.
- Keep target file untouched until `finish_file_write`.
- Persist tool records even when provider stream fails during argument generation.
- Convert provider stream interruption during started tool arg generation into a scoped draft/tool error, not a whole-turn invoke crash.
- Split HTTP timeout behavior for streaming vs non-streaming calls.
- Ensure cancellation marks active draft sessions as aborted or resumable.

## Frontend Requirements

- Render `FileMutationResult` metadata in `ToolCallBlock`.
- Render draft-session progress from backend events.
- Avoid marking a draft-generation failure as an applied file change.
- Show target untouched/modified state explicitly for failed large writes.
- Keep completed file mutation blocks visible when final model synthesis fails.

## Verification Requirements

### Rust Tests

- `edit_file` exact replacement success.
- `edit_file` rejects duplicate match unless `replace_all`.
- `patch` add/update/delete success.
- `patch` rejects path traversal and absolute paths.
- `patch` failure validates before write and does not partially modify files.
- `begin_file_write` creates draft but does not touch target.
- `append_file_write` enforces offset.
- `finish_file_write` atomically commits and returns diff stats.
- failed/aborted draft leaves target unchanged.
- stream interruption after tool-call draft start preserves tool record as error.
- streaming timeout helper does not enforce 60-second total timeout on active SSE streams.

### Frontend Checks

- Typecheck for new structured metadata.
- Tool block renders patch/edit/write-session states.
- Failed draft write displays target untouched.
- Completed tool block survives final provider failure.

### Manual Smoke Tests

- Ask agent to edit one line in an existing file.
- Ask agent to apply a multi-file patch.
- Ask agent to create a small file.
- Ask agent to generate a large file, then simulate provider interruption before finish; target must remain unchanged and UI must show draft failure.
- Ask agent to generate a large file successfully; final target must match expected size/hash and UI must show completed mutation.

## Phased Delivery

### Phase 1: Stop Misleading Failures

- Split streaming timeout from global HTTP timeout.
- Keep provider-stream interruptions scoped when tool-call draft already started.
- Ensure frontend does not label generated tool arguments as completed file writes.

### Phase 2: Patch/Edit As Default Coding Path

- Finalize structured mutation metadata.
- Ship `patch` add/update/delete.
- Update prompts and tool descriptions.
- Update `ToolCallBlock` diff rendering.

### Phase 3: Durable Large File Write Sessions

- Add draft session storage.
- Add begin/append/finish/abort tools.
- Add atomic finish and offset/hash validation.
- Add UI progress and failure state.

### Phase 4: Hardening

- Add stale target warnings.
- Add cleanup job for abandoned drafts.
- Add optional pre-apply diff approval.
- Add lightweight diagnostics where cheap.

## Acceptance Criteria

- [ ] Existing-file code edits prefer `edit_file` or `patch`, not whole-file `write_file`.
- [ ] Large generated files use draft write sessions instead of one huge `write_file(content)` call.
- [ ] If provider stream interrupts before `finish_file_write`, target file is unchanged.
- [ ] If append offset is wrong, write session fails safely without target mutation.
- [ ] If final synthesis fails after a successful mutation, the mutation remains visible as completed.
- [ ] Streaming requests are not killed by a 60-second total timeout while bytes/events are still arriving.
- [ ] Tool UI distinguishes argument generation, draft writing, final apply, completed, failed, and aborted.
- [ ] Rust tests for patch/edit/write-session safety pass.
- [ ] `npm run typecheck` passes.
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml chat::agent:: -- --nocapture` passes for agent-loop changes.

## Out Of Scope

- Full IDE editor experience.
- Binary file editing.
- LSP-level semantic edit tools.
- Cross-agent collaborative merge UI.
- Git checkpoint/rollback UI.
- Cloud sync of draft sessions.

## Open Questions

- Should the public tool names be separate (`begin_file_write`, `append_file_write`, `finish_file_write`, `abort_file_write`) or one tool with `mode`? Recommended: separate tools for clearer schemas and better UI labels.
- Should failed draft sessions be resumable by the model in the same conversation only, or across app restart? Recommended MVP: across app restart within retention window.
- Should `write_file` remain enabled for project coding prompts once write sessions exist? Recommended: keep it, but add size/context guidance and prefer write sessions for large content.

## Implementation Notes

Likely backend files:

- `src-tauri/src/api.rs`
- `src-tauri/src/chat/model/openai.rs`
- `src-tauri/src/chat/model/anthropic.rs`
- `src-tauri/src/chat/agent/loop_.rs`
- `src-tauri/src/chat/agent/stream.rs`
- `src-tauri/src/chat/agent/prepare.rs`
- `src-tauri/src/mcp/types.rs`
- `src-tauri/src/mcp/registry.rs`
- `src-tauri/src/native_tools/files.rs`
- `src-tauri/src/native_tools/mod.rs`

Likely frontend files:

- `src/chat/ToolCallBlock.tsx`
- `src/chat/types.ts`
- `src/api/tauri.ts`

Important caution:

- Do not ship a direct target-file `write_file_chunk(start/append/finish)` implementation. It solves progress display but creates a corruption risk. The correct implementation must write to a draft and commit atomically only at finish.
