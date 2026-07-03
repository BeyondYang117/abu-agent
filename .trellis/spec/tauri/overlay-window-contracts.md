# Overlay Window Contracts

Rules for Kivio's native windows, especially the Windows-only transparency pitfalls that
cost real debugging time. Read before changing window config in `src-tauri/src/windows.rs`
or the overlay sizing paths in `src-tauri/src/lens_commands.rs` + `src/Lens.tsx`.

## Two window archetypes — do not mix them

| Archetype | Windows | Windows-side chrome | Why |
|-----------|---------|---------------------|-----|
| **Floating overlay** | `main` (translator), `lens`, `translate` (screenshot + selected-text translate) | **Transparent + undecorated**, no native shadow (CSS draws the surface) | These are lightweight screen utilities. They must *feel like floating overlays*, not app windows. |
| **Application window** | `chat` | **Opaque + undecorated**, native DWM rounded corners / border / shadow (`apply_windows_chat_window_frame` + `apply_chat_window_theme_background`) | A real desktop app window. The opaque + DWM approach is the robust Windows pattern and avoids the transparent-layer seam bug. |

**Hard rule (product + verified 2026-07-03):** do **not** "fix" an overlay's Windows
rendering bug by converting it to the opaque application-window pattern. That was tried for
the selected-text translate window and rejected by the user — it reads as "an application
interface" and loses the floating-overlay feel. Solve overlay bugs while *keeping the window
transparent + undecorated*.

## Windows transparency fragility (tauri#14764)

Transparent + undecorated WebView2 windows are intrinsically fragile on Windows (upstream
[tauri#14764](https://github.com/tauri-apps/tauri/issues/14764), tao 0.34.5):

- tao's `to_window_styles` **always keeps `WS_CAPTION`** for undecorated windows; the native
  title bar is only hidden by the `WM_NCCALCSIZE` subclass. On some machines a frame/focus
  change lets the native caption ("Translate" + `_ □ ✕`) paint through.
- DWM composition on Windows 8+ is always on and **cannot be disabled** — so "region turned
  black because DWM composition was lost" is *not* the mechanism. The real trigger is any
  extra frame/region operation on the transparent surface.

**Do not** rely on `SetWindowRgn` / `set_resizable` / frame-changing calls on these windows
more than necessary; each is a potential trigger for the black-region + caption glitch.

## `SetWindowRgn` is only for the fullscreen→floating clip path

`lens_set_floating` (`lens_commands.rs`) intentionally keeps a fullscreen window and clips the
visible area to a card rect via `SetWindowRgn` — but **only** for windows that start
fullscreen and shrink to a floating card (screenshot-translate `mode=translate`, lens
`mode=chat`). This avoids WebView2 jitter from repeatedly moving a fullscreen webview.

Windows that are **born card-sized** must never use `SetWindowRgn`:

- Selected-text translate (`mode=translateText`) is sized to the card at open by
  `lens_position_text_floating`. It must resize with plain `set_size`, matching macOS.
- Contract: `lens_set_floating` treats **`x/y` present → `SetWindowRgn`** (clip path) and
  **`x/y` absent → clear any stale region + `set_size`** (born-small path). The frontend
  resize effect must send width/height only (no x/y) for such windows.

Regression fixed in commit `8ab8309`: the selected-text translate window was incorrectly
routed through the `SetWindowRgn` clip path, producing the black-region + native-caption
glitch on a tester's machine. Fix = route it through the `set_size` path.

## Overlay lifecycle notes (relied upon by the above)

- Both platforms **destroy** the overlay window on close (Windows `hide + destroy`; macOS
  `destroy_overlay_window` after restoring the original NS class). So each `ensure_*_window`
  call creates a fresh window.
- `lens` and `translate` overlays are **mutually exclusive** (`lens_is_active` toggle) — only
  one is visible at a time; opening one requires the other to be closed (destroyed) first.
