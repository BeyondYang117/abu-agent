# MCP Tool-Result Images → Model (Vision Feedback Loop)

> **Purpose**: Contract for how images returned by MCP tools reach the main model (or an auxiliary vision model), and the plugin-adaptation principle this mechanism enforces.
> **Origin**: Task `07-10-plugin-runtime-robustness` — OfficeCLI screenshots were flattened to `[image: image/png]` placeholders, so the model "reviewed" slides it never saw (fake Gate-3 PASS).

---

## Scenario: MCP tool returns image content blocks

### 1. Scope / Trigger

Any MCP server whose tool result contains `{ "type": "image", "data": <base64>, "mimeType": ... }` blocks. Generic — NOT officecli-specific. Applies at the MCP execution point in `mcp/registry.rs::call_tool`, after `state.mcp_call_tool` succeeds, only when `native_ctx` (conversation/message context) is present.

### 2. Signatures

```rust
// chat/commands.rs — compatibility entrypoint kept for mcp/registry.rs
pub(crate) async fn attach_image_artifacts_for_model(
    app: &AppHandle,
    settings: &Settings,
    conversation_id: &str,
    message_id: &str,
    result: &mut mcp::types::McpToolCallResult,
)

// chat/vision.rs — crate-internal implementation behind the compatibility entrypoint
pub(super) async fn attach_image_artifacts_for_model(
    app: &AppHandle,
    settings: &Settings,
    conversation_id: &str,
    message_id: &str,
    result: &mut mcp::types::McpToolCallResult,
)

// chat/mcp_image_feedback.rs
// Pure helper (unit-tested guardrails): filters image artifacts, enforces caps
fn select_image_artifacts_for_attach(artifacts, MAX_IMAGE_BYTES, MAX_IMAGES)
    -> (Vec<(ChatToolArtifact, Vec<u8>)>, Option<String /*guard note*/>)

// Sibling of image_content_part(&Path) — takes a data: URL, no disk read
fn data_url_image_part(data_url: &str) -> Result<Value, String>
```

### 3. Contracts

- **Parse layer stays dumb**: `mcp/client.rs::parse_tool_result` converts image blocks to `ChatToolArtifact { mime_type, data_url, ... }` + text placeholder `[image: <mime>]`, `follow_up_user_messages` empty. It has no settings/conversation access — vision decisions do NOT belong there.
- **Vision main model** (`model_supports_vision(provider, model) == Some(true)`): ONE follow-up user message `{"role":"user","content":[<image parts>]}` pushed to `result.follow_up_user_messages` — the exact pipe implemented in `chat/vision.rs::read_image_as_tool_result` and exposed through `chat::commands::read_image_as_tool_result` already proved across all four protocol adapters (Anthropic merges it into the same user turn as the tool_result).
- **Non-vision main model**: images written to `temp_dir()/kivio-mcpimg-<uuid>.<ext>`, analyzed via `auxiliary_vision_model_for_images` + `analyze_chat_images_with_auxiliary_model` (review-oriented prompt, see below), analysis text appended to `result.content`, temp files removed on every exit path. `kivio-mcpimg-` prefix is GC'd at startup by `screenshot.rs::cleanup_orphan_temp_files`.
- **Guardrails**: max 4 images per result; single image ≤ 8MB after base64 decode (true byte length, not estimate). Skipped images append an explanatory note to `result.content`.
- **Review material is not a deliverable**: after images are successfully fed to the model (either branch), `result.artifacts` is **cleared** so the chat gallery does not display review screenshots. Final-product preview is the live-preview channel / delivery directory. The frontend "last-round gallery" logic (`MessageBubble.tsx::selectGalleryImageArtifacts`) stays as generic fallback for images that did NOT go through this pipe.
- **Graceful no-op**: missing conversation, no vision anywhere, aux failure → original placeholder preserved, artifacts kept (user still sees the image), never an error.

### 4. Validation & Error Matrix

| Condition | Behavior |
|---|---|
| No image artifacts | Unchanged result |
| `native_ctx` absent (context-less caller) | Skipped entirely |
| Image > 8MB | Skipped + note in content |
| > 4 images | First 4 used + overflow note |
| Conversation load fails | Silent no-op, placeholder kept |
| Aux vision fails | Placeholder kept, guard note only |

### 5. Tests Required

In `chat/mcp_image_feedback.rs`: artifact filtering (non-image / empty data_url), oversize skip + note, cap + overflow note, no-image passthrough, and `data_url_image_part` validation. In `chat/vision.rs`: auxiliary-model auto/explicit selection. Keep the commands-level message-builder regression test in `chat/commands.rs`. Run via `scripts/win-cargo-test.ps1` (plain cargo test binaries fail on Windows, 0xC0000139).

### 6. Wrong vs Correct

#### Wrong (the bug this replaces)
Model told to screenshot with `-o` then `read` the file back — a prompt-level detour around a runtime gap; and MCP images shown in chat but never given to the model (fake visual review).

#### Correct
Runtime feeds tool-result images to whichever vision path exists; prompts need no workaround; review images are hidden from chat.

---

## Module boundary after `commands.rs` extraction

- `chat/vision.rs` owns auxiliary vision selection, prompts, provider-backed image analysis, `read_image_as_tool_result`, and the MCP artifact attachment implementation.
- `chat/model_call.rs` owns the provider dispatch shared by normal chat orchestration and auxiliary vision; this prevents `vision -> commands` coupling.
- `chat/mcp_image_feedback.rs` remains pure and owns only artifact filtering, guard-note composition, MIME extension mapping, and data-URL content-part construction.
- `chat/commands.rs` keeps the two crate-visible compatibility entrypoints because `mcp/native_registry.rs` and `mcp/registry.rs` intentionally retain their existing paths. Do not bypass these paths during unrelated refactors.

Allowed dependency direction:

```text
commands -> vision -> model_call
commands ------------> model_call
vision -> storage / model_metadata / mcp_image_feedback
```

Forbidden: `vision -> commands` or `model_call -> commands`.

---

## Auxiliary vision prompt is REVIEW-oriented

`auxiliary_vision_system_prompt` (`chat/vision.rs`, zh+en) must demand explicit defect reporting — truncated/overflowing text, overlapping elements, literal escape sequences visible as text (`\n`, `\t`), misalignment, low-contrast — item by item, and only claim "no visual defects" when none found. A describe-only prompt loses defect information and re-creates the fake-PASS bug for every non-vision main model.

---

## Design Decision: plugin adaptation principle

**Context**: OfficeCLI initially got a ~35-line runtime hint teaching batch usage, screenshot-to-file detours, absolute paths, cleanup — compensating for runtime gaps.

**Decision**: **Generic capabilities go into the runtime (or the generic prompt segments in `chat/agent/prepare.rs`); a plugin's `system_hint` in `plugins/catalog.rs` keeps ONLY constraints that are truly specific to that plugin.** For officecli that is: MCP-tool-first (persistent warm process + Kivio live preview), no `officecli watch`/`unwatch`, no `officecli mcp <ide>`, plus the skill-routing table.

**Why**: thick per-plugin hints rot, fight the official skills, and hide runtime gaps that hurt every other plugin too. Claude Code / opencode / pi run officecli with zero adaptation because their runtimes already provide bash, image feedback, and vision reads.

**Generic prompt segment (prepare.rs, `tools_available`-gated)**: intermediate files → system temp dir; clean up before finishing; absolute paths for stdio MCP tools (server cwd unpredictable).
