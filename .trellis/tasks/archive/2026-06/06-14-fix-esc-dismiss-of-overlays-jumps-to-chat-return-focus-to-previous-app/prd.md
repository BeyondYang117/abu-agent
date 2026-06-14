# Fix: 浮窗按 Esc 关闭时有时跳到 AI 客户端（Chat）

## Goal

macOS 上用 lens / 快速翻译 / 输入翻译这些浮窗时，按 Esc 关闭**有时会跳出/创建 Chat（AI 客户端）窗口**。应改为：关闭浮窗后焦点回到**打开浮窗之前的那个前台 App**（Chrome、Finder 等），不再误开 Chat。

## Root cause（高置信度，已用工作流交叉验证 + 逐行核对）

两部分组合，缺一不可：

1. **旧地雷（pre-existing，main.rs:391-394，6/03–6/11 引入）**：
   ```rust
   RunEvent::Reopen { has_visible_windows } =>
       if !has_visible_windows { open_chat_window(app) }   // 无可见 user 窗口 → 创建/聚焦 Chat
   ```
   本意是"点 Dock 图标且无窗口 → 打开 Chat"。`USER_WINDOW_LABELS=[chat,settings,main]` 不含 lens，且非激活 NSPanel 不计入 AppKit 的 `hasVisibleWindows`，所以浮窗在屏时 `has_visible_windows=false`。
2. **新触发条件（本次 NSPanel 改动带来）**：浮窗现在是**非激活 NSPanel**，`makeKeyWindow` 成 key 但**不激活 Kivio**。按 Esc = `orderOut` 一个 key panel，AppKit 必须重选前台，**有时把 Kivio 进程重新激活** → 命中旧分支 → `open_chat_window` 凭空创建并聚焦 Chat。
   - 确证：Chat 关闭即销毁（`win.close()`，main.rs 的 `CloseRequested` 只对 lens 拦截改 hide），所以"凭空冒出 Chat"只可能来自 `open_chat_window` 的**创建**路径。

**"有时候"** = 需同时满足：①浮窗确实成了 key（在里面打过字/点过）②无 chat/main/settings 可见窗 ③关闭瞬间 AppKit 把 Kivio 选成新前台（而非底下 App 夺回前台）。

> 对比旧实现：之前用 `activateIgnoringOtherApps:YES` 每次整 app 激活，关闭后的激活流转很少凭空产生 Reopen。换非激活 panel 后才会踩到这颗旧地雷。

翻译窗补充：**commit 路径已用 `[NSApp hide:]`（commands.rs:182）让回前台**，所以 commit 不复现；只有 Esc/toggle 关闭路径没让前台 → 会复现。quick-translate 走 lens 窗口（被 lens 路径覆盖），只有 input-translate 用 main 窗口。

## Decision：关闭浮窗时把前台交还给"之前那个 App"（Spotlight/Alfred 标准做法，治根不堆兜底）

- **快照**：显示浮窗前记下当前前台 App 的 PID（非激活 panel 不改前台，所以此 PID = 用户原来的 App；若前台就是 Kivio 自己则记空）。
- **交还**：关闭浮窗后把前台激活权还给该 PID → Kivio 不会变成"前台却无窗口" → 那个误触的 Reopen 根本不发生 → 不再冒出 Chat。附带：Esc 后正确回到原位；全屏下回到 Chrome 的 Space 也正好对。
- **交接豁免**：`open_chat_window` 开头清除快照——这样"在 Chat 里追问 / dock 开 Chat"等**故意打开 Chat** 的路径不会被交还逻辑拽回旧 App。

明确**不**采用：在 Reopen 分支里判断 `lens_busy` 就跳过——那是对症状打补丁的防御性 guard，违背"治根"原则。

## Requirements

- lens / 快速翻译（截图+选词，走 lens 窗口）/ 输入翻译（main 窗口）按 Esc 或 toggle 关闭后，焦点回到打开前的前台 App，不再误开/聚焦 Chat。
- 故意打开 Chat 的路径不受影响：lens「在客户端继续」、tray「打开 AI 客户端」、Dock 图标点击（真实 Reopen）。
- 不回归：lens/翻译的键盘输入、commit 粘贴回填、刚修好的全屏浮现（交还用 activate 别的 App / 不得对 panel 用 set_focus/makeKeyAndOrderFront/activateIgnoringOtherApps）。
- Windows/Linux 不受影响（macOS-gated）。

## Implementation

- `state.rs`：AppState 加 `prev_frontmost_pid: AtomicI32`（0 = none）。
- `windows.rs`（macOS）：`macos_frontmost_app_pid()`（NSWorkspace.frontmostApplication.processIdentifier，主线程；==自身 pid 时返回 None）、`macos_activate_app(pid)`（NSRunningApplication.runningApplicationWithProcessIdentifier→activate，主线程）；以及 `remember_frontmost_app(state)` / `restore_previous_frontmost_app(state)`（swap 取出并交还）/ `forget_frontmost_app(state)`。
- `lens_commands.rs`：`lens_request_internal` 开头 `remember_frontmost_app`；`lens_close` 末尾 `restore_previous_frontmost_app`。
- `shortcuts.rs`：`toggle_main_window` 显示分支 + tray `"show"` 调 `remember_frontmost_app`；`open_chat_window` 开头 `forget_frontmost_app`。
- `main.rs`：`WindowEvent::CloseRequested` 中 label=="main" 时 `restore_previous_frontmost_app`（不拦截关闭；覆盖 Esc/toggle/commit 的 main 关闭）。
- spec：`.trellis/spec/backend/window-lifecycle.md` 增「Overlay dismissal focus-return contract」。

## Verification

- `cargo check` 0 错误；`cargo test` 不回归。
- macOS release 手动冒烟：
  - Chrome 全屏 → lens → Esc：回到 Chrome，不出 Chat。
  - 桌面无 Chat → lens/快速翻译/输入翻译 → Esc：回到原 App，不创建 Chat。
  - lens「在客户端继续」→ 正常进入 Chat（不被交还拽走）。
  - Dock 图标点击（无窗口）→ 仍能打开 Chat。
  - 输入翻译 commit → 粘贴回填正常。

## Out of Scope

- 重写 commit_translation 的 `NSApp hide` 路径（工作正常，不动）。
- Windows 焦点行为。
