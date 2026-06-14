# Window Lifecycle

> Tauri window lifecycle conventions for Kivio desktop windows.

## Chat Window Restore Contract

Chat is a normal desktop window, unlike Lens and translator floating windows.

On macOS, packaged builds must preserve that normal desktop identity: do not
declare `LSUIElement=true` for the app bundle just to support tray behavior.
Tray and global shortcuts may remain as auxiliary entry points, but Chat should
stay visible to Dock, Cmd+Tab, and window-capture tools during normal launches.
Only the explicit autostart path may temporarily use `ActivationPolicy::Accessory`
to stay quiet before the user opens Chat.

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

## macOS Auxiliary Floating Window Contract

Lens and translator (`main`) floating windows must work while another app, such
as Chrome, is in macOS **native fullscreen** (its own Space).

**Root cause of the long-standing breakage (do not regress):** since macOS
10.14/Big Sur, only an **NSPanel**, or a window owned by an **Accessory
(LSUIElement)** app, may be drawn into *another* app's fullscreen Space. Kivio
runs as `ActivationPolicy::Regular` (Chat needs Dock/Cmd+Tab identity — see
above), so a plain tao **NSWindow** can NOT appear over another app's fullscreen
Space — no matter what `collectionBehavior`, window level, or `orderFrontRegardless`
is set. `visible_on_all_workspaces(true)` / `CanJoinAllSpaces` alone is therefore
insufficient. Any path that *activates* the app (`makeKeyAndOrderFront`,
`activateIgnoringOtherApps`, `set_focus`) additionally yanks the user off the
fullscreen Space.

**The fix:** convert the lens/translator windows into **non-activating NSPanels**
on macOS, via `windows::ensure_overlay_panel`. It is idempotent and:

1. `object_setClass`-es the window into a runtime `NSPanel` subclass
   (`canBecomeKeyWindow=YES` — borderless windows default to NO, so this is
   required for the translator input / lens question box to receive keyboard;
   `canBecomeMainWindow=NO`).
2. Sets `NSWindowStyleMaskNonactivatingPanel` — clicking/focusing the panel does
   not activate Kivio, so it never switches away from the fullscreen Space.
3. Sets `collectionBehavior = CanJoinAllSpaces | FullScreenAuxiliary | Stationary
   | IgnoresCycle` (clears `MoveToActiveSpace` and `Transient`, which are
   mutually exclusive with those).
4. Sets window level to `NSStatusWindowLevel` (25) — above the menu bar /
   fullscreen content, but below the `screenSaver` (1000) band that causes a
   wrong-Space blink.
5. Sets `hidesOnDeactivate = NO` — **critical**: NSPanel defaults to hiding when
   its owning app deactivates, and the overlay is shown while another app (e.g.
   fullscreen Chrome) is frontmost, so without this the panel vanishes instantly.

Then show with `windows::show_overlay_panel(&window, need_key)`, which calls
`orderFrontRegardless` (and `makeKeyWindow` when `need_key`). Never use
`window.show()` / `set_focus()` on the macOS overlay path.

Apply `ensure_overlay_panel` at lens-window creation (`ensure_lens_window`), in
the `Focused(true)` self-heal (`main.rs`), and once per show in
`lens_request_internal` / `toggle_main_window` / tray `"show"`. Drop
`set_always_on_top(true)` on macOS for these windows — the panel owns its level,
and tao's `set_always_on_top` resets the level to `NSFloatingWindowLevel` (and to
`Normal` on toggle-off), which would clobber the status level.

Keep this behavior out of Chat. **Chat must never be converted to an NSPanel** —
it stays a normal desktop NSWindow through `normalize_chat_window_behavior`,
preserving Dock/Cmd+Tab/window-management identity.

### Wrong

```rust
let _ = window.set_always_on_top(true);
let _ = window.show();
let _ = window.set_focus();
```

A plain NSWindow from a Regular-policy app fails silently over another app's
fullscreen Space (no overlay), and `set_focus`/activation switches Spaces.

### Correct

```rust
#[cfg(not(target_os = "macos"))]
let _ = window.set_always_on_top(true);
#[cfg(target_os = "macos")]
windows::ensure_overlay_panel(&window); // idempotent: reclass to non-activating NSPanel
#[cfg(target_os = "macos")]
windows::show_overlay_panel(&window, /* need_key */ true);
#[cfg(not(target_os = "macos"))]
let _ = window.show();
#[cfg(not(target_os = "macos"))]
let _ = window.set_focus();
```

> Validate in a **release** build (`tauri build`), not only `tauri dev` —
> `setLevel`/`setCollectionBehavior` on a plain NSWindow are widely reported to
> work in dev but fail in release; the NSPanel conversion is the path that
> survives release mode.

## Lens Window Selection Contract

Lens window selection must treat Chat as a selectable app window. On macOS,
`CGWindowListCopyWindowInfo` reports Chat, Lens, and legacy translator helper
surfaces with the same Kivio / KeyLingo owner name, so do not filter all
Kivio-owned windows as "self".

Correct filtering is:

- Keep the general filters for invalid ids, non-zero layers, near-transparent windows, and tiny windows.
- Filter Kivio-owned auxiliary surfaces such as Lens (`title == "Lens"`), legacy empty-title floating bars, and the small translator window.
- Allow the Kivio / KeyLingo primary Chat window when its title is `Kivio` / `KeyLingo` and its bounds are large enough to be a desktop window.

### Wrong

```rust
if owner == "Kivio" || owner == "KeyLingo" {
    continue;
}
```

That made Lens unable to hover/select Chat after Chat became the primary
desktop window.

### Correct

Route self-filtering through a helper such as `is_kivio_auxiliary_window`, and
cover the boundary with tests:

- Chat-sized `owner=kivio title=Kivio` is selectable.
- Lens-sized/title `owner=kivio title=Lens` is filtered.
- Small translator-sized Kivio windows are filtered.

## Lens Overlay Close Contract

The Lens window is **reused, not destroyed** — `lens_close` / the frontend close path `hide()`s it and re-positions it fullscreen for next time. Because it is reused and borderless (no rounded corners, no traffic lights), if any close path fails to hide it, the window lingers visible on screen and reappears during later Chat use — looking like a malformed "duplicate" window even when Lens was never re-triggered.

Therefore the close path must hide the Lens window **deterministically**, never conditionally on animation state:

- The frontend `closeAfterReset` plays an exit animation (`resetBeforeHide` → `setStage('select')`) before calling `api.lensClose()`. `resetBeforeHide`'s `setStage` fires the stage motion effect, which bumps the animation-level `motionSeqRef`.
- The "should I still hide?" guard MUST distinguish **a genuine new Lens session opening** from the close's own animation side effects. Guard on an open-generation counter (`lensOpenSeqRef`, incremented only by `enterSelect`), NOT on `motionSeqRef`. Guarding on `motionSeqRef` self-trips: the close's own `setStage` bump makes the guard think a new session started, so it skips `lensClose` and leaks the window.

Do not paper over a leaked Lens window with a backend "hide it whenever Chat opens" backstop — fix the close path so the window is always hidden at its source.

### Anti-Pattern

```ts
resetBeforeHide()                                   // bumps motionSeqRef via setStage('select')
const seq = motionSeqRef.current
await waitForFrames(2)
if (seq !== motionSeqRef.current) return            // self-trips on its own animation bump → window leaks
await api.lensClose()
```

### Correct Pattern

```ts
const openSeq = lensOpenSeqRef.current              // only enterSelect() bumps this
resetBeforeHide()
await waitForFrames(2)
if (openSeq !== lensOpenSeqRef.current) return       // only a real new session aborts the hide
await api.lensClose()
```

## Tests Required

- Run `cargo check --manifest-path src-tauri/Cargo.toml`.
- Run `cargo test --manifest-path src-tauri/Cargo.toml` when practical.
- For macOS release candidates, manually smoke-test installed-app Dock restore:
  - open Chat
  - minimize to Dock
  - click the Dock icon
  - verify the restored window has normal titlebar/Dock behavior, can close/minimize, and does not remain stuck above other windows
- For the Lens overlay close path, manually verify the Lens window never lingers: repeatedly open Lens → ask (handoff to Chat) and open Lens → Esc-close, then switch back to Chat; no second borderless window should remain.
