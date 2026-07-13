# Export conversations as Markdown

## Goal

Let users export an individual Kivio conversation from its sidebar context menu as a readable Markdown document.

## Background

- Conversations are currently persisted locally as one internal JSON file per conversation under the app data `conversations/` directory.
- The existing sidebar context menu is implemented by `src/chat/ConversationContextMenu.tsx` and currently offers rename, project/set movement, and delete actions.
- The frontend already uses `@tauri-apps/plugin-dialog`; export should use a native save dialog rather than silently writing to a fixed folder.
- Internal conversation JSON includes runtime-only state and may reference separately stored attachments, so a deliberate export contract is required instead of blindly copying the persistence file.

## Requirements

- Add a localized `导出` / `Export` entry to the individual conversation context menu shown in the user-provided screenshot.
- Clicking the export entry opens a native save dialog directly for Markdown; no format picker is needed in the first version.
- Present a native save dialog with a filename derived from the conversation title and the `.md` extension.
- Export the selected conversation only.
- Report export failures without closing or corrupting the conversation.
- Keep existing rename, move, and delete behavior unchanged.
- Define a readable Markdown representation suitable for use outside Kivio.
- Export only user-authored message content and assistant final-answer content.
- Exclude reasoning/thinking, tool calls, tool outputs, command logs, and internal runtime state from Markdown.
- Keep export as a single Markdown file. Represent images and attachments in message order using concise placeholders such as `[图片]` and `[附件：filename.ext]`; do not copy binary files.
- Begin the document with the conversation title, creation time, last-updated time, and model name. Do not export API keys, base URLs, provider credentials, or other connection configuration.
- Render messages chronologically with clear second-level headings for the user and assistant.
- Localize Markdown labels and attachment placeholders according to the current Kivio UI language (`zh` or `en`).

## Acceptance Criteria

- [x] Every individual-conversation context menu exposes a localized export action.
- [x] Clicking export opens a native save dialog defaulting to a `.md` filename.
- [x] Cancelling the native save dialog produces no file and no error.
- [x] Markdown export writes valid UTF-8 Markdown with conversation metadata and messages in chronological order.
- [x] Exported Markdown contains no reasoning/thinking sections, tool-call arguments, tool outputs, or internal runtime state.
- [x] Images and attachments are represented by readable placeholders in their original message position; no binary sidecar files are created.
- [x] The document header contains title, creation time, update time, and model, but no provider secrets or endpoint configuration.
- [x] Chinese UI exports Chinese metadata/message labels; English UI exports English labels.
- [x] Export handles filenames containing characters invalid on macOS/Windows.
- [x] Export failures are surfaced in the UI and do not mutate persisted conversation data.
- [x] Existing context-menu actions continue to work.
- [x] Relevant frontend/backend tests and project checks pass.

## Out of Scope

- Bulk export of all conversations.
- Importing an exported conversation.
- JSON export; it is deferred until after the Markdown version.
- Cloud sync or sharing links.

## Technical Notes

- `ConversationContextMenu` is shared by recent/project/set views, so adding the action there can cover all individual conversation locations if callers provide the export callback.
- Existing storage serialization is atomic and should remain untouched; export is a read-only projection of a loaded conversation.
