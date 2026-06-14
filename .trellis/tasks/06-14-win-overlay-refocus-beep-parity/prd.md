# Windows 适配：overlay 重新聚焦 + 无选区消音对齐

## Goal

本轮 macOS overlay 改动在 Windows 上留下两处需对齐的点（macOS 单平台开发引入）：
1. **overlay 重新聚焦回归**：为治 macOS 跨屏激活跳屏，删了前端 `getCurrentWindow().setFocus()`，改走 `api.lensFocusWebview()`——但该命令在 Windows 是 no-op。导致 Windows 上 lens/translate overlay 的「重新聚焦」路径（切模式/关历史/onFocusChanged 等只调 focusLensSurface、不重走 lens_request 的场景）失效。
2. **无选区提示音 Windows 仍响**：消音的三态 `AxSelection` 短路只在 macOS；Windows UIA 路径「无选区」仍返回 `Unavailable` → Ctrl+C → 系统提示音。

## What I already know（代码事实）

- `lens_focus_webview`（lens_commands.rs:1251）：macOS 调 `windows::focus_overlay_webview`，非 macOS 分支 `let _ = window`（no-op）。
- Windows overlay 初次显示靠 `lens_request_internal` not-macos 分支 `window.show()` + `window.set_focus()`（lens_commands.rs:504-508）——初次 OK，缺的是「重新聚焦」。
- 前端 `focusLensSurface`（Lens.tsx ~280）现在 run() 只 `api.lensFocusWebview()` + DOM focus。
- Windows UIA `read_accessibility_selected_text`（shortcuts.rs:186-247）：closure 用 `?` 把「TextPattern 不支持」和「支持但选区空」都塌成 `None` → `Unavailable`。
- Windows 上 `window.set_focus()` 无 macOS 的 NSApp 跨屏激活问题，安全。

## Requirements

### Fix 1（强烈建议，安全）：lens_focus_webview Windows 恢复窗口聚焦
- `lens_focus_webview` 非 macOS 分支由 `let _ = window` 改为 `let _ = window.set_focus();`。
- 这样前端 `focusLensSurface` 的重试在 Windows 上恢复窗口级重新聚焦，与 macOS 行为对齐；macOS 仍走非激活 `focus_overlay_webview`（不变）。

### Fix 2（保守，需 Windows 实测）：Windows 无选区返回 Empty
- 改写 Windows `read_accessibility_selected_text` 的 closure，区分两种当前都塌成 `None` 的情况：
  - **拿到 IUIAutomationTextPattern + GetSelection 成功 + 收集到的非空文本为空** → 返回 `AxSelection::Empty`（确认无选区 → 上层跳过 Ctrl+C → 不响）。
  - **TextPattern 获取失败 / cast 失败 / GetSelection 失败 / 其它错误** → 返回 `AxSelection::Unavailable`（保持兜底 Ctrl+C，**不碰浏览器/Electron 等不支持 TextPattern 的路径**）。
- 保守原则：任何不确定/失败都归 `Unavailable`，只在「TextPattern 明确可用且选区确为空」时才 `Empty`，避免漏抓真实选区。

## Acceptance Criteria

- [ ] Fix1：Windows 上 overlay 切模式/关历史等重新聚焦正常（需 Windows 冒烟）。
- [ ] Fix2：Windows 原生输入框（记事本/文本框）无选区起 lens 不再响；**原生应用里有真实选区时仍能抓到**（需 Windows 冒烟重点验这条）；浏览器/Electron 行为不变（仍 Ctrl+C 兜底）。
- [ ] macOS 行为完全不变（lens_focus_webview macOS 分支、macOS AxSelection 不动）。
- [ ] `cargo check`（macOS 本机，确认非 Windows 分支至少语法/类型正确）。Windows 分支代码因 cfg 在 macOS 下不编译，需仔细审 `::windows` API 用法正确性。
- [ ] `npm run typecheck` / `lint` 通过（若动到前端；本任务可能不动前端）。

## Out of Scope

- macOS overlay/Ax 逻辑（不动）。
- 既存非本轮问题：skill `.sh` 需 bash、MCP npx PATH、shell 超时进程组（另议）。

## Technical Notes

- Fix2 的 Windows 代码我（macOS）无法本机编译验证，需用 `::windows` crate 的 IUIAutomationTextPattern/GetSelection API 正确写法；实测在 Windows 机器上跑。
- 两处改动都不影响 macOS 编译/行为。
