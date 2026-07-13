# Design — 修复选中翻译悬浮窗黑框/标题栏

## 组件边界

- 前端：`src/Lens.tsx` 浮动 resize effect（约 1886–1920 行）+ `src/api/tauri.ts` 的 `lensSetFloating`（契约不变，已支持 `x?/y?` 可选）。
- 后端：`src-tauri/src/lens_commands.rs` 的 `lens_set_floating`（Windows 分支）与既有 `lens_clear_interactive_region`。

## 现状数据流（Windows / translateText）

1. `lens_request_internal(mode="translateText")` → `lens_position_text_floating` 用 `set_size(card+48, 320)` + 贴光标 `set_position`。**从不 SetWindowRgn**。
2. 前端进入 `stage=translating`，resize effect 触发：
   - 非 mac 分支调用 `api.lensSetFloating({ x, y, width, height })`（x=y=0，因 `translateText` 的 `barRect` 恒为 `FLOATING_PADDING`）。
3. 后端 `lens_set_floating` Windows 分支：`(Some(x),Some(y))` → `lens_set_interactive_region` → **SetWindowRgn**。
   - 该 region 裁剪叠加在透明无边框 WebView2 窗口上 → 触发黑块 + 原生标题栏（tauri#14764）。

> 对比：截图翻译 / 截图问答走全屏窗口，**必须**用 SetWindowRgn 把可见区裁成卡片（刻意避开 WebView2 反复搬窗的抖动，见 `Lens.tsx:1154` 注释）。此路径保持不变。

## 档 1 设计（本任务实施）

核心：`translateText` 是天生的小窗，用真实 `set_size` 即可，不需要 region。

### 前端（`Lens.tsx` resize effect）
把非 mac 分支按 mode 二分：
```ts
if (isMacPlatform || mode === 'translateText') {
  // 真实小窗：仅改尺寸，保持开窗时锚定的 origin，绝不 SetWindowRgn
  api.lensSetFloating({ width: w, height: h }).catch(...)
} else {
  // 截图翻译 / lens 全屏 → 浮动：保持 SetWindowRgn 裁剪
  api.lensSetFloating({ x, y, width: w, height: h }).catch(...)
}
```
- `translateText` 传 `width/height`（不带 x/y），命中后端 size-only 分支。
- macOS 行为不变（原本就是仅 width/height）。

### 后端（`lens_set_floating` Windows 分支）
当前 x/y 缺省时 Windows 分支**什么都不做**（既不 region 也不 set_size），是个空洞。补成 size-only：
```rust
#[cfg(target_os = "windows")]
{
    if let (Some(x), Some(y)) = (rect.x, rect.y) {
        lens_set_interactive_region(&window, x, y, rect.width, rect.height)?;
    } else {
        // size-only（选中翻译等天生小窗）：先清掉可能残留的旧裁剪 region，再真实改尺寸。
        // 避开"透明+无边框+SetWindowRgn"的黑块/原生标题栏脆弱性（tauri#14764）。
        lens_clear_interactive_region(&window);
        let _ = window.set_size(tauri::LogicalSize::new(rect.width, rect.height));
    }
}
```
- `lens_clear_interactive_region` 已存在（`SetWindowRgn(hwnd, None)`），顺带修掉"复用窗口残留 region 从不清除"的隐患。
- `set_size` 只改尺寸、保持左上角 origin → 卡片继续锚定在开窗光标位置。

### 兼容性 / 回归面
- 仅影响 Windows + `translateText`。截图翻译（`mode=translate`）与 lens（`mode=chat`）仍带 x/y → 走 region 原路径，零改动。
- macOS 全平台不变。
- `lens_set_floating` 开头的 `set_resizable(true)`：`translate` 窗口创建即 `resizable(true)`，此调用为 no-op diff，不触发 frame change，保留即可。

## 档 2 方向（后续，不在本任务实施）

**背景更新（本轮决策依据）**：该 bug 只在**测试者机器**上出现，开发/用户本机从不复现。因此档 1「本机正常」只证明无回归、不能证明在故障机上已修复；档 2 同样无法本机验证「bug 没了」，其可信度来自「结构上不再有透明分层窗口」。用户已确认：**现在就做档 2 结构加固**，接受视觉变化（圆角 16px → DWM ~8px）与「只能验证长得对、验不了 bug 没了」。

**方案（复用 `translate` label，按 mode 建不同配置——不新增 label）**

生命周期已确认：两平台关闭均 destroy 该窗口（Win `hide+destroy`；mac `destroy_overlay_window`），且两 overlay 模式互斥（`lens_is_active` toggle，开一个就不能开另一个，必须先关=销毁）。故每次 `ensure_translate_window(mode)` 都是全新创建，**同一 label 按 mode 建不同配置是安全的**，无需改 lib.rs / shortcuts.rs / active_overlay_window 等硬编码 `"translate"/"lens"` 的触点。

仅 **Windows + `mode==translateText`** 走不透明；macOS（透明 NSPanel 稳、且需要）与其它 mode 全部不变。

### 后端

1. `windows.rs::ensure_overlay_window`：新增 `opaque_text_card = cfg!(windows) && mode=="translateText"`。
   - opaque：`transparent(false) + shadow(true) + background_color(白占位)`；build 后 `apply_chat_window_theme_background(&w, chat_window_is_dark(&w))` + `apply_windows_chat_window_frame(&w)`（DWM 圆角/描边/阴影，与 chat 同）。窗口建时 hidden，show 前完成上色 → 无闪。
   - 非 opaque：保持 `transparent(true)+shadow(false)+bg(0,0,0,0)`。
2. `lens_commands.rs::lens_position_text_floating`：Windows 下窗口宽 = `card_w`（去掉 +48 留白，卡片铺满窗口）；macOS 仍 `card_w+48`。
3. `lens_set_floating` size-only 分支（档 1 已加）沿用。

### 前端（`Lens.tsx`）

`isWinOpaqueTextCard = mode==='translateText' && !isMacPlatform`（本项目仅 mac/Win，`!isMacPlatform` 即 Windows）。

1. 新增 `translateCardRef` + `cardMeasuredH`（ResizeObserver 测卡片实际 offsetHeight）。
2. resize effect：opaque 分支窗口尺寸 = `{ width: barRect.width, height: cardMeasuredH }`（不 +padding），驱动 `lensSetFloating`（size-only）。
3. 卡片渲染：opaque 时 `left:0, top:0, width:'100%'`、高度 auto 铺满；className 去掉 `rounded-2xl / border / lens-floating-surface`（圆角/描边/阴影交给 DWM，卡 bg=窗口 bg），关掉 scale intro（只留 opacity），避免边缘露窗底。

### 回归面 / 验证
- 全部 gated 于 `translateText + Windows`；截图翻译 / lens / macOS 零改动。
- 可本机验证：不透明卡片尺寸贴合、无黑边、DWM 圆角+阴影、内容随流式增长正确、截图翻译不回归。
- 不可本机验证：#14764 是否根除（结构性论证：无透明分层面）→ 交测试者确认。

## 回滚

改动集中在 `windows.rs` / `lens_commands.rs` / `Lens.tsx`，均 gated；`git revert` 或还原对应块即可，无数据/存储迁移。
