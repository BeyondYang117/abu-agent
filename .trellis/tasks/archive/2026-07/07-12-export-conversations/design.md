# Design: Export Conversation as Markdown

## Scope

Add a read-only Markdown export path for one conversation from every individual conversation context menu. JSON export, bulk export, import, and binary attachment copying are excluded.

## Architecture

### Frontend interaction

1. `ConversationContextMenu` adds an `导出` / `Export` menu item with a download icon.
2. `ConversationList` receives an `onExportConversation(id, title)` callback and forwards the selected conversation.
3. `Sidebar` owns the export interaction because it already owns conversation-level menu mutations:
   - derive a filesystem-safe default filename from the conversation title;
   - call `@tauri-apps/plugin-dialog.save` with a Markdown filter;
   - treat a cancelled dialog as a no-op;
   - invoke the backend export command with conversation id, selected path, and current UI language;
   - surface failures with a localized user-visible alert, consistent with the Sidebar's existing project-folder error path.
4. `Chat` passes its existing `uiLang` state into `Sidebar`; this keeps menu labels and document labels aligned with the active UI language.

### Backend export

Add a Tauri command in the chat command layer:

```text
chat_export_conversation_markdown(conversation_id, path, language) -> Result<(), String>
```

The command:

1. loads the authoritative persisted `Conversation` through `storage::load_conversation`;
2. renders it through a pure Markdown renderer;
3. writes UTF-8 text to the user-selected path;
4. never mutates the conversation or index.

The renderer belongs in a small focused chat export module so formatting can be unit tested without a Tauri window or native dialog.

## Markdown Contract

Document header:

```md
# <conversation title>

- 创建时间 / Created: <localized timestamp>
- 更新时间 / Updated: <localized timestamp>
- 模型 / Model: <conversation model>
```

Messages remain in persisted chronological order. Each message has a localized level-two heading (`用户` / `User`, `助手` / `Assistant`) and its timestamp. The body uses only `ChatMessage.content` plus attachment placeholders.

Excluded fields:

- `reasoning`
- `segments` other than the already-materialized final `content`
- `tool_calls`, tool artifacts, API/model replay messages
- plan/todo/context/runtime state
- provider id, API endpoint, credentials

Attachment rendering:

- image: `[图片：name]` / `[Image: name]`
- file: `[附件：name]` / `[Attachment: name]`
- no binary files are copied and no internal attachment path is exposed

## Filename Handling

The default filename is derived from the conversation title, replacing Windows/macOS-invalid filename characters and control characters, trimming trailing spaces/dots, applying a reasonable length cap, and falling back to `conversation.md` when empty. The native save dialog remains authoritative: users may choose another name and overwrite behavior is handled by the OS dialog.

## Error and Cancellation Behavior

- Native save-dialog cancellation returns without writing or showing an error.
- Load/render/write errors are returned by the backend and shown in a localized alert.
- No success dialog is required; successful completion returns silently after the native save action.

## Compatibility

- Existing stored conversations require no migration.
- Old conversations missing optional fields remain exportable through existing serde defaults.
- The implementation is desktop-only in practice but uses existing cross-platform Tauri dialog and filesystem paths.

## Testing

- Rust unit tests cover Chinese/English rendering, exclusion of private/internal fields, attachment placeholders, chronological ordering, and empty-content messages.
- Frontend tests cover safe default filename generation where practical.
- Existing type-check/build and targeted Rust tests verify command wiring.

## Rollback

Remove the new command/module/API method and context-menu callback chain. No persisted data or schema changes need rollback.
