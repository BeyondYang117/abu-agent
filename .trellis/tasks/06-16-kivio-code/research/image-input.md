# Research: Image input UX for terminal coding agents (and kivio-code design)

- **Query**: How do Claude Code / Codex CLI / opencode let users ADD images to a terminal coding agent (clipboard paste, drag-drop path, `[Image #N]` placeholder, submit-time vision handling)? Then design the same for kivio-code reusing its existing vision pipeline.
- **Scope**: mixed (external UX/docs from knowledge + internal kivio code references)
- **Date**: 2026-06-17

> Sourcing note: live web/exa search tools were not available in this session. The external-UX sections below are reconstructed from knowledge of these CLIs (Claude Code, Codex CLI, opencode) and their public docs/changelogs; treat the exact gesture/version details as "verify against current docs before locking the UX." Each external claim is tagged `[verify]` where the precise behavior may have drifted. The kivio-code internal sections are verified against the repo (file:line cited) and are authoritative.

---

## Findings

### 1. Claude Code image input

**Clipboard paste (the gesture).**
- Claude Code accepts an image pasted from the OS clipboard directly into the prompt input. `[verify]` The documented gesture is the platform paste shortcut: **Ctrl+V on macOS inside Claude Code** (notably *not* Cmd+V — Cmd+V is intercepted by the terminal emulator to paste text, so Claude Code binds Ctrl+V for image paste). On Linux/Windows terminals it is also Ctrl+V.
- Terminals cannot deliver binary clipboard data as keystrokes. So a paste keypress triggers a **native read of the system clipboard image** (NSPasteboard on macOS), not a parse of pasted bytes from stdin. When an image is on the clipboard, Claude Code grabs the raw image, base64-encodes it, and attaches it; if there is no image, the same key falls back to pasting clipboard *text*.
- Platform support: works best on macOS; Linux support depends on the terminal/clipboard backend (Wayland vs X11) being readable. `[verify]` Some terminals/SSH sessions cannot expose a clipboard image, in which case only the drag/path route works.

**Drag-and-drop / file path.**
- Dragging an image file onto a terminal does NOT transfer bytes; the terminal inserts the **file path as text**, typically quoted and/or with spaces backslash-escaped (e.g. `/Users/me/Desktop/My\ Shot.png` or `'/Users/me/My Shot.png'`). This is terminal behavior (macOS Terminal.app and iTerm2 both do this), not something the app controls.
- `[verify]` Claude Code's documented trick: **drag the file while holding a modifier (Shift on macOS in some terminals)** so the terminal inserts the path rather than trying other behavior; then Claude Code detects that the typed/pasted token resolves to an existing local image file and converts it into an attachment.
- Detection: on submit (or as you type), Claude Code recognizes tokens that look like local file paths with image extensions (`.png/.jpg/.jpeg/.gif/.webp`), **de-quotes / un-escapes** them, checks the file exists, and turns each into an attachment, replacing the path text with the placeholder.
- `@path` reference: Claude Code supports `@`-mentions for files generally (its file autocomplete uses `@`), and `@`-mentioning an image file attaches it as an image. So both bare drag-dropped paths and `@relative/img.png` work as input routes. `[verify]` exact `@`-vs-bare-path handling.

**Placeholder shown in input / transcript.**
- Confirmed pattern: a compact bracketed token, **`[Image #1]`, `[Image #2]`, …** numbered per message. Multiple images each get their own incrementing number. The placeholder is what's shown in the input line and is also what appears in the persisted/transcript user message in place of the binary. `[verify]` whether deleting the `[Image #N]` text in the editor removes that attachment (the common implementation: the placeholder token IS the handle, so backspacing it out drops the image; numbering is recomputed on submit).
- The image is **never rendered in the terminal** — only the placeholder text.

**On submit (how images reach the model).**
- Claude (the API) is natively vision-capable, so Claude Code attaches images **directly to the request** as image content blocks (base64) alongside the text — no separate vision pre-analysis step. This is the key difference from kivio-code's situation (kivio's main coding model is often a text-only OpenAI-compatible model).

### 2. Codex CLI / opencode (brief)

- **OpenAI Codex CLI**: supports image inputs. `[verify]` It accepts images via a CLI flag (`codex -i image.png` / `--image`) and, in the interactive TUI, via pasted/drag'd file paths; recent versions added clipboard-image paste in the TUI. Placeholder shown is a short attachment marker in the composer. Codex sends images to vision-capable GPT models directly.
- **opencode** (sst/opencode TUI): supports image attachments by referencing a file path / `@file` mention in the prompt; `[verify]` clipboard-image paste support has been added in recent releases. Images are forwarded to the configured vision-capable model. Placeholder is a file-chip/marker in the editor.
- Common conventions across all three: (a) clipboard paste reads the OS clipboard image natively, (b) drag-drop relies on the terminal inserting a path which the app then resolves, (c) a short non-rendered placeholder/chip stands in for the image in the composer and transcript, (d) images go to a vision-capable model.

### 3. macOS terminal specifics (implementation path)

**Reading an image from the system clipboard programmatically:**
- Native: `NSPasteboard.general` → read `NSPasteboard.PasteboardType.png`/`.tiff` data.
- Tools people use: `pngpaste` (`pngpaste out.png`) or AppleScript (`the clipboard as «class PNGf»`). These are external-process approaches.
- **Rust path (what kivio already has): `arboard`.** `arboard::Clipboard::get_image()` returns an `ImageData { width, height, bytes }` where `bytes` is **RGBA8**. Re-encode that to PNG with the `image` crate before saving/attaching. This is the cross-platform implementation path — no `pngpaste`/AppleScript shell-out needed. kivio's `Cargo.toml` already has `arboard = "3"` and `image = "0.25"` (png/jpeg features) — `src-tauri/Cargo.toml:71-72`.
- Note: `arboard` also has `get().file_list()` (already used at `src-tauri/src/chat/commands.rs:1132`) to read Finder-copied file paths off the clipboard — a third input route besides image bytes and typed paths.

**Drag-drop path delivery on macOS Terminal/iTerm2:**
- The terminal inserts the dropped file's absolute path as text. Terminal.app and iTerm2 quote/escape: spaces become `\ ` (backslash-escaped) or the whole path is single-quoted. The parser must handle both: strip surrounding single/double quotes, and unescape `\<space>` (and other backslash-escapes) before `Path::exists()`.

---

## Existing kivio-code building blocks (verified, repo-authoritative)

All of the GUI vision/mixer pipeline already exists and is reusable. Most pieces are private `fn` in `chat/commands.rs` and would need `pub(crate)` exposure for the CLI to call them.

| Item | Location | Notes |
|---|---|---|
| `save_pasted_image(name, mime_type, data_base64) -> PastedImageSave` | `src-tauri/src/chat/attachments.rs:36` | `pub(crate)`. Decodes base64, size-caps (`MAX_PASTED_IMAGE_BYTES = 12 MiB`, `attachments.rs:17`), writes to conversation attachments dir, returns saved `path`/`name`/`mime_type`. Saves to `conversation_attachments_dir` — CLI may want a different/temp dir. |
| `PastedImageSave` enum | `attachments.rs:20` | `Saved { path, name, mime_type }` / `Failed { error }`. |
| `is_attachable_file_name(name)` | `attachments.rs:127` | `pub(crate)` — extension allow-list check (covers image + doc types). |
| `call_vision_api(...)` | `src-tauri/src/api.rs:814` | `pub async`. Lens-oriented (takes `image_id`, `ExplainMessage`s, stream/event args). Heavier than needed for mixer; the chat mixer uses `call_chat_completion_message` instead. |
| `auxiliary_vision_model_for_images(settings, main_provider, main_model, image_paths) -> Option<AuxiliaryVisionModel>` | `chat/commands.rs:2790` | **private fn.** Decides whether to route images through a separate vision model: uses explicit `effective_vision_model()` (`settings.rs:908`) if set, else if the main model is known text-only it auto-picks an enabled vision-capable, non-image-gen model. Returns `None` when main model can see images itself. |
| `analyze_chat_images_with_auxiliary_model(...) -> AuxiliaryVisionResult` | `chat/commands.rs:3566` | **private async fn.** Builds `[{system}, {user: image parts + prompt}]`, calls `call_chat_completion_message`, returns objective textual observations. Uses `image_content_part` (`commands.rs:4388`) to base64-embed each image. |
| `AuxiliaryVisionResult { provider_name, model, content }` | `chat/commands.rs:3508` | **private struct.** |
| `user_content_with_auxiliary_vision_result(last_user, result, lang) -> String` | `chat/commands.rs:3663` | **private fn.** Appends a `[Mixer vision auxiliary result] ... answer using the visual observations below: <content>` block to the user message so a text-only main model "sees" the screenshot. |
| `auxiliary_vision_system_prompt` / `auxiliary_vision_user_prompt` | `chat/commands.rs:3633` / `3641` | Prompts for the aux step. |
| `auxiliary_vision_tool_record` / `finish_auxiliary_vision_tool_record` | `chat/commands.rs:3514` / `3552` | Build a `mixer_vision` tool-call record (for UI display of the aux step). CLI can render this as a tool card too, or skip. |
| `has_explicit_vision_model` / `effective_vision_model` | `src-tauri/src/settings.rs:904` / `908` | Settings accessors the mixer logic depends on. |
| GUI mixer wiring (reference flow) | `chat/commands.rs:2929-2946` | Computes `route_images_through_auxiliary_vision`; when true, sends `&[]` images to the main model and budgets `AUXILIARY_VISION_RESULT_TOKEN_ESTIMATE` per image; aux result is injected at `commands.rs:1329` via `user_content_with_auxiliary_vision_result`. |

### kivio-code interactive layout (where new code lands)

- `src-tauri/src/kivio_code/interactive/app.rs` (2937 lines) — TUI App state + key handling. `handle_key(&str) -> AppEffect` at `app.rs:822` is the app-level key dispatcher (Ctrl+L, Shift+Tab, Esc, Ctrl+D, Ctrl+C, Enter→`AppEffect::Submitted`). Editor is `self.editor: Editor` (`app.rs:169`). `matches_key(data, "ctrl+v", self.kitty_active)` is the pattern to add. Existing clipboard use: `copy_to_clipboard` at `app.rs:1396-1398` already uses `arboard::Clipboard`.
- `InputEvent::Paste(String)` already exists (`mod.rs:63`) and is fed from bracketed-paste (`mod.rs:969`, `1098`, `1138`) into the editor — this is the text-paste route; image paste is a separate keypress.
- `src-tauri/src/kivio_code/interactive/mod.rs` (2089 lines) — `begin_turn(text, agent_tx, plan_mode)` at `mod.rs:157`. This is where the user message becomes `runtime_messages.push({role:user, content:text})` (`mod.rs:168`) and the agent loop is spawned. **This is the injection point** for vision pre-analysis: before pushing the user message, if there are attached images, run the aux vision model and rewrite `content`.
- `AppEffect::Submitted(text)` handled at `mod.rs:865` → `turn.begin_turn(...)` (`mod.rs:870`).

---

## Proposed kivio-code design

### A. Attachment state (app.rs)
- Add to `App`: `pending_images: Vec<PendingImage>` where `PendingImage { number: usize, path: PathBuf, name: String, mime: String }`.
- `[Image #N]` numbering = index+1, recomputed on submit. The placeholder token in the editor text is the user-visible handle. On submit, parse `[Image #k]` tokens to know which attachments survived (if the user backspaced one out, drop it).

### B. Clipboard paste — Ctrl+V (app.rs `handle_key`)
1. In `handle_key`, before the Enter branch, add `if matches_key(data, "ctrl+v", self.kitty_active) { ... }`.
2. Try `arboard::Clipboard::new()?.get_image()`:
   - **If an image is present**: it's RGBA (`ImageData { width, height, bytes }`). Encode to PNG via the `image` crate (`image::RgbaImage::from_raw(w, h, bytes)` → `DynamicImage` → write PNG to a `Vec<u8>` via `ImageOutputFormat::Png`), base64-encode, call `save_pasted_image("pasted.png", "image/png", &b64)`. On `Saved`, push a `PendingImage`, compute `N`, insert `[Image #N]` into the editor at the cursor, push a notice ("Added [Image #N]"). Consume the key (return `AppEffect::None`).
   - **If no image** (`get_image()` errors): fall through to normal text paste (`get_text()` → editor insert), so Ctrl+V still pastes text. Mirror the GUI's "image-first, text-fallback" behavior.
3. Reuse `save_pasted_image` (already `pub(crate)`); consider a CLI-specific save dir (the GUI one targets a conversation dir) — either pass through the kivio-code session/temp dir or add a sibling helper.

### C. Drag / typed path → attachment (on submit, or on a token boundary)
- On `AppEffect::Submitted` handling (in `mod.rs`, or pre-process in `app.rs` before emitting `Submitted`):
  1. Tokenize the editor text; for each token that looks like a path:
     - strip surrounding `'`/`"`; unescape `\<space>` and other `\<char>`.
     - check extension ∈ {png,jpg,jpeg,gif,webp} (reuse the spirit of `is_attachable_file_name`) AND `Path::exists()` AND `is_file()`.
  2. For each match: read bytes, base64, `save_pasted_image` (or just record the existing path directly — no copy needed since it's already a local file; simplest is to keep the original path as the attachment), assign `[Image #N]`, and **replace the path token in the text with `[Image #N]`**.
  3. Optionally support `@img.png` (relative to cwd) the same way.
- Also optionally read Finder-copied files via `clipboard.get().file_list()` (pattern already at `commands.rs:1132`) on Ctrl+V when no raw image is present.

### D. Placeholder
- `[Image #N]` shown in the editor and persisted verbatim in the user message text. Never render the image bytes in the terminal. Numbering per submitted message, sequential, starting at 1. Deleting the placeholder text drops that attachment at submit time.

### E. On submit — mixer vision pre-analysis (mod.rs `begin_turn`)
The coding model is usually text-only, so reuse the GUI auxiliary-vision flow rather than attaching images to the main request.

In `begin_turn` (or just before it), when `pending_images` is non-empty:
1. Resolve settings (already have `self.state` → settings) and the main provider/model (`self.assembly`).
2. Call `auxiliary_vision_model_for_images(&settings, main_provider, main_model, &image_paths)`:
   - `Some(aux)` → main model is text-only (or explicit vision model configured): run `analyze_chat_images_with_auxiliary_model(...)` → `AuxiliaryVisionResult`, then set the message content to `user_content_with_auxiliary_vision_result(Some(&text), &result, &language)`. Push THAT as the `user` content into `runtime_messages` (so the main coding model sees the textual visual observations).
   - `None` → main model can see images itself: attach image content parts directly to the user message (build a multimodal content array like `image_content_part` does), if/when the CLI's model layer supports image parts. (If the CLI path can't yet send image parts, fall back to always routing through the aux model.)
3. Render a `mixer_vision` tool card (reuse `auxiliary_vision_tool_record` / `finish_...`) so the user sees the pre-analysis step, mirroring the GUI.
4. Clear `pending_images` after the turn starts.

Because `begin_turn` is currently sync and spawns the loop, the aux-vision call (async) should run either (a) inside the spawned task before constructing messages, or (b) in a small pre-step that awaits the aux result and then calls `begin_turn` with the rewritten text. Option (b) keeps `begin_turn` simple; option (a) avoids blocking the UI thread — prefer (a): move the image→text resolution into the spawned async block (it already builds `messages` from `runtime_messages.clone()` at `mod.rs:185`), appending the aux result before the loop runs, and emit the tool card via `agent_tx`.

### F. Visibility / `pub(crate)` exposures needed
These are currently private in `chat/commands.rs` and must be exposed (or duplicated) for the CLI:
- `auxiliary_vision_model_for_images` (`commands.rs:2790`)
- `analyze_chat_images_with_auxiliary_model` (`commands.rs:3566`)
- `AuxiliaryVisionResult` (`commands.rs:3508`)
- `user_content_with_auxiliary_vision_result` (`commands.rs:3663`)
- `image_content_part` (`commands.rs:4388`) — if attaching directly for vision-capable main models
- `auxiliary_vision_tool_record` / `finish_auxiliary_vision_tool_record` (`commands.rs:3514`/`3552`) — optional, for the tool card
Already public enough: `save_pasted_image`, `is_attachable_file_name`, `PastedImageSave` (`pub(crate)` in `attachments.rs`); `effective_vision_model` / `has_explicit_vision_model` (`pub` in `settings.rs`).

Note: `analyze_chat_images_with_auxiliary_model` takes a `State<'_, AppState>` (Tauri). The CLI has `self.state` (an `AppState`/Arc, per `mod.rs`); if it's not a Tauri `State`, refactor the aux fn to accept `&AppState` (or the `http` client + settings) rather than `State<'_, AppState>` so it's callable outside the Tauri command context.

### Implementation checklist (module map)
- [ ] `interactive/app.rs`: add `pending_images` state + `PendingImage`; add `[Image #N]` insertion helper; add Ctrl+V branch in `handle_key` (arboard `get_image` → encode PNG → `save_pasted_image` → insert `[Image #N]`; text fallback on no image).
- [ ] `interactive/app.rs` (or a parse helper): drag/typed path detection — de-quote/unescape, extension+exists check, replace token with `[Image #N]`; optional `@img.png` and `file_list()` clipboard route.
- [ ] `interactive/mod.rs` `begin_turn` / submit path: when images attached, run aux-vision pre-analysis inside the spawned async block and inject the result text into the turn's `messages`; emit `mixer_vision` tool card via `agent_tx`; clear pending images.
- [ ] `chat/commands.rs`: bump `auxiliary_vision_model_for_images`, `analyze_chat_images_with_auxiliary_model`, `AuxiliaryVisionResult`, `user_content_with_auxiliary_vision_result`, `image_content_part`, (opt) tool-record fns to `pub(crate)`; if needed, change the aux fn signature to accept `&AppState` instead of Tauri `State`.
- [ ] Reuse as-is: `attachments::save_pasted_image`, `is_attachable_file_name`, `settings::{has_explicit_vision_model, effective_vision_model}`.

---

## Related specs
- None found under `.trellis/spec/` for image input (searched: no spec dir for this CLI feature).
- Sibling research file: `.trellis/tasks/06-16-kivio-code/research/context-init-commands.md` (pre-existing).

## Caveats / Not Found
- External UX details (exact Claude Code paste shortcut Ctrl+V vs Cmd+V, the drag modifier, `@`-vs-bare-path handling, Codex/opencode current image support) are from model knowledge, NOT live docs — flagged `[verify]`. Confirm against current Claude Code / Codex CLI / opencode docs+changelogs before locking the UX, since these CLIs iterate fast.
- The `[Image #N]` numbering/delete semantics in Claude Code are inferred from the visible placeholder convention; exact delete behavior `[verify]`.
- `arboard::ImageData.bytes` is RGBA (unverified against the exact `arboard 3.x` API in-repo beyond the documented contract) — confirm the `get_image()` return shape when implementing (it returns `Cow<[u8]>` RGBA in arboard 3).
- The CLI's main-model adapter ability to send native image content parts (for vision-capable main models, design path E.2) was not confirmed; if absent, always route through the aux vision model.
