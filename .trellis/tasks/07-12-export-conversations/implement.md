# Implementation Plan: Export Conversation as Markdown

1. Add a focused Rust Markdown export module.
   - Define localized labels for `zh` and `en`.
   - Render header metadata, chronological user/assistant content, timestamps, and attachment placeholders.
   - Ensure reasoning, tools, artifacts, runtime state, provider configuration, and attachment paths are never emitted.
   - Add renderer unit tests.

2. Add and register the backend export command.
   - Load by conversation id using the existing storage API.
   - Write rendered UTF-8 Markdown to the native-dialog-selected path.
   - Return contextual load/write errors.
   - Expose the command through the frontend Tauri API wrapper.

3. Wire the sidebar export interaction.
   - Add a reusable safe Markdown filename helper and tests.
   - Pass current UI language from `Chat` to `Sidebar`.
   - Add `onExportConversation` through `Sidebar` → `ConversationList` → `ConversationContextMenu`.
   - Add localized menu label and icon.
   - Open the native save dialog with `.md` default/filter, no-op on cancel, invoke export on selection, and show localized errors.

4. Verify behavior and regressions.
   - Run targeted Rust export tests.
   - Run frontend unit tests for filename handling/menu behavior if added.
   - Run TypeScript type-check/build and `cargo check --bin kivio`.
   - Manually verify export from recent, project, and set conversation lists; verify Chinese/English labels, cancel behavior, saved content, and absence of reasoning/tool output.

## Risky Files / Review Points

- `src/chat/ConversationContextMenu.tsx`: preserve submenu hover and delete separator layout.
- `src/chat/ConversationList.tsx` and `src/chat/Sidebar.tsx`: the list is reused in recent/project/set views, so callback wiring must cover every caller.
- `src-tauri/src/lib.rs`: command registration must match the frontend invoke name exactly.
- Markdown renderer: only use explicit allowlisted fields to prevent leaking internal content.

## Validation Commands

```bash
npm run test -- --run
npm run build:ui
cargo test chat::export
cargo check --bin kivio
git diff --check
```
