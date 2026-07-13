# PRD — 修复选中翻译悬浮窗黑框/标题栏

## 背景 / 问题

选中文本翻译（Lens `translateText` 模式，独立 `translate` 浮窗）在 Windows 上**有时**出现两个异常（见用户截图）：

1. 卡片外围出现一大块**不透明纯黑**区域；
2. 窗口顶部冒出**原生标题栏**（"Translate" 标题 + 经典 `_ □ ✕` 按钮）。

正常时它应是一个贴近光标、无边框、透明背景、只显示翻译结果卡的悬浮小窗。

## 根因（已查证）

- 该窗口是 `transparent(true) + decorations(false)` 的 WebView2 窗口。Windows 8+ 的 DWM 合成强制常开，**不存在"合成被关闭"**；真实机制是"透明 + 无边框 + `SetWindowRgn`"三者叠加在一个 WebView2 窗口上天生易碎：
  - `SetWindowRgn` 裁出的 region 在 DWM 下透明窗口上会渲染成**不透明黑**；
  - tao 0.34.5 给无边框窗口的 style **始终保留 `WS_CAPTION`**，仅靠 `WM_NCCALCSIZE` 子类隐藏；透明 + WebView2 在 frame/焦点变化时会让原生标题栏重新画出（上游已知 bug tauri#14764，同 tao 版本）。
- **关键**：`translateText` 路径从创建起就被 `lens_position_text_floating` 缩成卡片大小，**根本不需要 `SetWindowRgn`**；但前端 resize effect 仍对它调 `lensSetFloating({x,y})` → 后端 `lens_set_floating` 在 Windows 上走 `SetWindowRgn`（`lens_commands.rs:2301`）。这个多余的 region 操作正是黑框/标题栏的引爆点。

## 范围

### 本任务（档 1，最小正确修复）
- 让 `translateText` 在 Windows 上**不再使用 `SetWindowRgn`**，改为真实的小窗 `set_size`（与 macOS 行为一致）。
- 顺带修复"复用窗口时残留旧裁剪 region 从不清除"的隐患：size-only 路径先清 region 再 `set_size`。

### 不在本任务（档 2 / 档 3，作为后续，见 design.md）
- 档 2：给选中翻译一个**独立不透明窗口**（对齐 chat 窗口在 Windows 上已验证的 `不透明 + DWM 圆角/描边/阴影` 稳态方案），彻底绕开透明+无边框脆弱性。因 `translate` 窗口被截图翻译共用，此改动需新增独立窗口 label，属较大改动。
- 档 3：将"全屏选区覆盖层"与"浮动结果卡"拆成两个独立窗口的架构级重构。

## 验收标准

1. Windows 上反复触发选中文本翻译（热键）多次，**不再出现**卡片外的黑色块与原生标题栏。
2. 结果卡仍能随内容高度增长/收缩正确调整窗口尺寸，位置保持在开窗时的光标锚点附近。
3. 截图翻译（mode=translate）与截图问答（mode=chat / lens）的全屏覆盖 + 浮动卡行为**不回归**（仍走原 `SetWindowRgn` 路径）。
4. `npm run lint`、`npm run typecheck`、相关 `cargo` 检查通过。
5. 实机运行验证（构建后手动触发选中翻译）确认症状消失。

## 非目标
- 不改 macOS 行为。
- 不动截图翻译/截图问答的全屏→浮动 region 裁剪机制。
- 不在本轮实施档 2/档 3。
