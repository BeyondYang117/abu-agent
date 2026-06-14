# Fix: Lens 与快速翻译在 macOS 别的 App 原生全屏下无法唤起

## Goal

macOS 上当其它 App（如 Chrome）处于**原生全屏**（独占 Space）时，按全局热键唤起 Lens / 快速翻译没有任何反应。窗口要能浮现在该全屏 App 上方，让两个功能在全屏场景下可用。

## Root cause (corrected 06-14, cross-verified)

> 之前几轮一直在调 `collectionBehavior` 的位和 window level —— **那部分本来就对**（`CanJoinAllSpaces | FullScreenAuxiliary` + status level + `orderFrontRegardless` 已经全齐）。真正没修好的原因是**前提搞错了**：以为 app 是 Accessory，其实不是。

**真正的卡点 = 激活策略 + 窗口类型，不是 collectionBehavior 的位。**

- App **默认 `ActivationPolicy::Regular`**（`main.rs:129-134`，仅 autostart 才 Accessory；`open_chat_window` 等也反复设 Regular）。这是为了满足 `window-lifecycle.md` 对 Chat 必须有 Dock 图标的硬要求。
- macOS 10.14 / Big Sur 起：**只有 Accessory(LSUIElement) 策略的 app，或 NSPanel 类型窗口**，才被允许把浮窗画进**别的 App 的原生全屏 Space**。Regular app 的**普通 NSWindow** 无论 collectionBehavior / level / orderFrontRegardless 怎么设都**不会浮现** → 这就是「没反应」。
  - Electron 铁证：`setVisibleOnAllWorkspaces(true,{visibleOnFullScreen:true})` 内部**强制 `app.dock.hide()`（=Accessory）**（PR #24956），明说就是绕过这条限制。
- tao 的窗口是普通 NSWindow（`TaoWindow`），**不是 NSPanel**。这正是 Tauri 官方 issue #9556 / #11488 / #3326 里维护者反复说的：Big Sur 后只有 NSPanel 能画在全屏上方，必须把窗口转 NSPanel。
- 「跳到新桌面」那个变体 = app 被激活（Regular app 变前台）把用户从全屏 Space 拽走。之前提交版用过 `makeKeyAndOrderFront`/`activateIgnoringOtherApps` 必然触发；工作区版去掉了，但只要路径上还有任何「激活 app」动作（如 `set_focus`）仍会复现。

证据来源：Apple AppKit docs（canJoinAllSpaces / fullScreenAuxiliary / moveToActiveSpace 语义、NSWindow.Level）、Electron `native_window_mac.mm` + PR #24956、SO #73518709 / #23503943、tao/tauri issues #9556/#11488/#3326/#4620。

## Decision: 非激活 NSPanel 改造（裸 objc，不加依赖）

把 **lens** 与 **main(翻译)** 浮窗在 macOS 上重分类为**非激活 NSPanel**：
- `object_setClass` 重分类到运行时 NSPanel 子类（`canBecomeKeyWindow=YES`，`canBecomeMainWindow=NO`）。
- `styleMask |= NSWindowStyleMaskNonactivatingPanel` —— 点击/聚焦浮窗**不激活 app** → 结构性消除「跳 Space」。
- `collectionBehavior = CanJoinAllSpaces | FullScreenAuxiliary | Stationary | IgnoresCycle`（清 MoveToActiveSpace/Transient）。
- `level = NSStatusWindowLevel(25)`（在菜单栏之上、避开 1000 的错屏闪烁带）。
- `hidesOnDeactivate = NO`（**关键**：NSPanel 默认在 app 失活时自动隐藏；浮窗显示时前台是 Chrome，不设 NO 会立刻消失）。
- 显示走 `orderFrontRegardless`（+ 需要键盘输入时 `makeKeyWindow`），**绝不** `activateIgnoringOtherApps` / `makeKeyAndOrderFront` / `set_focus`。

为什么不选「临时切 Accessory」：那会让每次用 lens/翻译（含非全屏日常）Dock 图标闪一下，且与 Chat 的 Regular 身份切换耦合、有回归风险。NSPanel 全程保持 Regular → Chat 零影响。

为什么不选 tauri-nspanel 插件：避免新依赖 + 版本兼容核验；项目已大量用裸 objc，自洽。

## Requirements

- lens / main 浮窗在 macOS 上转为非激活 NSPanel，能浮现在别的 App 原生全屏 Space 上方，且不触发 Space 切换。
- app 全程保持 `ActivationPolicy::Regular`；**Chat 窗口绝不转 panel**，保留 Dock/Cmd+Tab 身份。
- 不破坏非全屏（普通桌面 Space）下的既有行为（位置、键盘输入、粘贴回填）。
- Windows / Linux 不受影响（改动 macOS-gated）。

## Acceptance Criteria (evolving)

- [ ] Chrome 原生全屏下按 Lens 热键，Lens overlay 浮现在全屏上方。
- [ ] Chrome 原生全屏下按快速翻译热键，翻译小窗浮现在全屏上方。
- [ ] 普通桌面 Space 下两个功能行为不变（位置、聚焦、自动隐藏照常）。
- [ ] `cargo build` 通过；macOS 手动冒烟通过。

## Definition of Done

- macOS 手动冒烟（全屏 + 非全屏各验一遍）。
- `cargo build` / 现有测试不回归。
- 改动集中、macOS cfg-gated，不影响 Windows。

## Out of Scope

- Windows 全屏行为（Windows 全屏模型不同，不在本次范围）。
- Lens 截图坐标 / 多显示器定位的既有逻辑（不改）。
- 全屏下唤起后焦点归还策略（保持现状）。

## Technical Notes

- `windows.rs` 新增 macOS-only 助手（裸 objc，复用项目既有 `msg_send` 风格）：
  - `kivio_panel_class()`：`OnceLock` 注册一个 NSPanel 运行时子类（`canBecomeKeyWindow=YES` / `canBecomeMainWindow=NO`，因为浮窗是 borderless，否则不能成为 key window 接收键盘）。
  - `ensure_overlay_panel(window)`：`object_setClass` 重分类（幂等，已是 panel 则跳过）→ `setStyleMask |= NonactivatingPanel` → 设 collectionBehavior / level / `hidesOnDeactivate=NO`。每次 show 前可再调一次重申（防 tao `set_resizable`/`set_always_on_top` 改 mask/level 时的漂移）。
  - `show_overlay_panel(window, make_key)`：`orderFrontRegardless`（+ 需键盘时 `makeKeyWindow`）。
- 调用点：
  - lens：`ensure_lens_window` 创建后 `ensure_overlay_panel`；`lens_request_internal` 用 `ensure_overlay_panel + show_overlay_panel(true)`；`main.rs` `Focused(true)` 自愈改调 `ensure_overlay_panel`。
  - main(翻译)：`toggle_main_window` / tray "show" 用 `ensure_overlay_panel + show_overlay_panel(true)`，移除 macOS 上的 `set_always_on_top`（level 由 panel 管，避免 tao 把 level 重置成 floating）。
- `object_setClass` 把 `TaoWindow`→NSPanel 子类的安全性：tauri-nspanel 生产环境同款做法（NSPanel 与 TaoWindow 都是 NSWindow 子类，实例布局兼容）。
- 删除旧的 `apply_macos_workspace_behavior` / `show_macos_auxiliary_window` / `apply_macos_auxiliary_window_behavior` / `order_front_macos_auxiliary_window`。

## Verification

- `cargo check --manifest-path src-tauri/Cargo.toml` 通过；`cargo test` 相关不回归。
- macOS **release 构建**下手动冒烟（dev 与 release 行为可能不同，必须验 release）：
  - Chrome 原生全屏 → 按 lens 热键 → overlay 浮现在全屏上方、可框选、可输入，不跳 Space。
  - Chrome 原生全屏 → 按快速翻译 / 输入翻译热键 → 浮窗浮现、可输入、粘贴回填正常。
  - 普通桌面 Space 下三个功能行为不变。
  - Chat 仍是正常带 Dock 图标窗口（Cmd+Tab / Dock 还原正常）。
