# Window Lifecycle

> Tauri window lifecycle conventions for Kivio desktop windows.

## Chat Window Restore Contract

Chat is a normal desktop window, unlike Lens and translator floating windows.

When reusing an existing Chat window from any activation entry point, the path must:

1. Reapply Chat chrome with `apply_chat_window_chrome`.
2. Reapply Chat min-size with `apply_chat_window_min_size`.
3. Reapply normal window behavior with `normalize_chat_window_behavior`.
4. Restore the app activation policy to `Regular` on macOS.
5. Explicitly `unminimize()` when the window is minimized.
6. Then call `show()` and `set_focus()`.

Activation entry points include:

- Dock reopen / macOS `RunEvent::Reopen`
- tray menu "Open AI Client"
- single-instance activation
- settings routes that reuse the `chat` webview window

## Anti-Pattern

Do not restore Chat from macOS Dock reopen with only:

```rust
let _ = window.show();
let _ = window.set_focus();
```

That bypasses Chat normalization and can leave a packaged macOS build restoring a miniaturized `NSWindow` into a malformed surface that still renders React content but is not managed like a normal app window.

## Correct Pattern

Route Chat activations through `open_chat_window` / `open_chat_settings_window`, or a shared helper that includes the full restore contract above.

Lens and translator windows are intentionally different: they may be frameless, transparent, always-on-top, or skipped from the taskbar. Do not copy their restore behavior into Chat.

## Tests Required

- Run `cargo check --manifest-path src-tauri/Cargo.toml`.
- Run `cargo test --manifest-path src-tauri/Cargo.toml` when practical.
- For macOS release candidates, manually smoke-test installed-app Dock restore:
  - open Chat
  - minimize to Dock
  - click the Dock icon
  - verify the restored window has normal titlebar/Dock behavior, can close/minimize, and does not remain stuck above other windows
