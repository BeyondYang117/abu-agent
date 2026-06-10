# Agent File Editing Patterns Research

## Purpose

Summarize how established coding agents handle file creation and code edits, then map those patterns to Kivio's current Chat native tools.

## Sources Reviewed

- OpenCode Go write tool: https://github.com/opencode-ai/opencode/blob/73ee4932/internal/llm/tools/write.go
- OpenCode TypeScript edit tool: https://github.com/anomalyco/opencode/blob/5c5069b6/packages/opencode/src/tool/edit.ts
- OpenAI Codex apply_patch runtime: https://github.com/openai/codex/blob/35aaa5d9/codex-rs/apply-patch/src/lib.rs
- Aider edit formats documentation: https://github.com/Aider-AI/aider/blob/main/aider/website/docs/more/edit-formats.md
- Local Hermes Agent:
  - `/Users/zmair/.hermes/hermes-agent/tools/file_tools.py`
  - `/Users/zmair/.hermes/hermes-agent/tools/file_operations.py`
  - `/Users/zmair/.hermes/hermes-agent/tools/patch_parser.py`
- Current Kivio:
  - `src-tauri/src/mcp/types.rs`
  - `src-tauri/src/native_tools/files.rs`
  - `src-tauri/src/chat/agent/prepare.rs`

## Current Kivio Behavior

- `write_file` requires `path` and full `content`.
- Backend writes with a single `fs::write(&full, content)`.
- `edit_file` exists, but it is a minimal exact `old_string` -> `new_string` replacement.
- Tool UI currently shows a tool block lifecycle, but does not expose diff, per-file verification, or a separate "generating file content" state.
- If provider synthesis/planning fails after the tool ran, the user may see a final red provider error even though file mutation may already have landed.

## Patterns From Other Coding Agents

### Whole-file write exists, but is not the preferred code-edit primitive

- OpenCode has a write tool that accepts full file content. It adds safety around it: read-before-write checks, diff generation, permission request with diff metadata, history versions, and LSP diagnostics.
- Hermes also keeps `write_file`, but its schema explicitly says it completely replaces existing content and directs targeted edits to `patch`.
- Aider documents whole-file editing as the simplest approach, but slow and expensive because the model must return the entire file even for small changes.

### Patch/search-replace is the preferred targeted-edit primitive

- Codex uses an `apply_patch` flow where the model emits add/update/delete patch hunks, then the runtime parses and applies them.
- OpenCode TypeScript uses an `edit` tool with `oldString`, `newString`, `replaceAll`, locking, diff metadata, formatting, file watcher events, and diagnostics.
- Hermes uses a `patch` tool with:
  - replace mode: `path`, `old_string`, `new_string`, `replace_all`
  - patch mode: V4A multi-file patch text
  - fuzzy matching strategies
  - unified diff return
  - lint/LSP diagnostics
  - path locks
  - stale file and wrong-cwd warnings
  - file mutation verifier when writes/patches fail
- Aider supports multiple edit formats: whole file, search/replace diff blocks, unified diff, and architect/editor separation.

### Common safety and UX practices

- Generate a diff before applying or immediately after applying.
- Ask for approval on sensitive file mutations, ideally showing the diff.
- Preserve line endings and BOM where practical.
- Prevent stale writes by tracking whether the file has changed since the agent read it.
- Lock per path for concurrent edits.
- Run syntax/lint/LSP diagnostics after writes/patches, and surface only relevant new errors where possible.
- Report the actual path modified.
- Do not let a post-mutation provider error erase or obscure the fact that a file operation succeeded.

## Recommended Direction For Kivio

- Keep `write_file` for new files and explicit full-file generation.
- Make targeted edits first-class:
  - either add a `patch` tool or significantly enhance `edit_file`;
  - support exact replace first, then V4A patch mode;
  - return unified diff and metadata to the UI.
- Update prompts so coding edits prefer patch/edit over whole-file write.
- Update frontend tool UI so file mutations show file path, operation type, status, diff summary, and diagnostics.
- Treat post-tool synthesis/provider failures as final-response failures, not as if the file mutation itself failed.
