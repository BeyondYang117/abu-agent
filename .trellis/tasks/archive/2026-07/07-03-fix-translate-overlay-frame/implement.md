# Implement — 修复选中翻译悬浮窗黑框/标题栏（档 1）

## 执行清单（按序）

- [x] 1. 后端：`src-tauri/src/lens_commands.rs` `lens_set_floating` Windows 分支
      在 `(Some(x),Some(y))` 之外补 size-only 分支：`lens_clear_interactive_region(&window)` + `window.set_size(LogicalSize(width,height))`。
      验证点：`x/y` 存在时仍走 `lens_set_interactive_region`（截图翻译/lens 不回归）。

- [x] 2. 前端：`src/Lens.tsx` 浮动 resize effect（约 1915–1919 行）
      把非 mac 分支按 `mode === 'translateText'` 二分：translateText 与 mac 一样只传 `{width,height}`；其余保持 `{x,y,width,height}`。

- [x] 3. 质量门：typecheck ✅ / lint ✅ / `cargo check` ✅（仅既有无关 warning）。

- [ ] 4. 实机验证：构建并运行，多次触发选中文本翻译热键，确认无黑块/无原生标题栏，卡片随内容尺寸正确、锚定光标；再验证截图翻译不回归。

## 档 2（不透明窗口结构加固）—— 已实现后按用户偏好整体回退

用户实机体验后判断：不透明窗口让选中翻译"像个应用界面"，而截图/翻译这几个功能应保持**悬浮窗**调性。故 档 2 全部回退，只保留档 1。

- [x] 5–8. 档 2 后端/前端/静态门曾全部完成（不透明主题窗 + DWM 圆角 + ResizeObserver 精确贴合）。
- [x] 回退：`git checkout HEAD -- windows.rs`（全档 2）；`lens_position_text_floating` 宽度还原 `card_w+48`；`Lens.tsx` 移除 `isWinOpaqueTextCard`/`cardMeasuredH`/`translateCardRef`/ResizeObserver/opaque 分支/卡片 opaque 条件；**保留档 1**（resize effect 的 `|| mode==='translateText'` 分支）。
- [x] 回退后静态门：typecheck ✅ / lint ✅ / cargo check ✅。回退后 `git diff` 仅剩档 1 两处 hunk。
- 期间发现并修正过一个自引入回归：`ensure_overlay_window` 的 `.shadow(!opaque_text_card)` 取反，误给截图翻译/Lens 透明覆盖窗加了原生阴影——已随 windows.rs 整体回退消除。

## 最终交付 = 档 1（保留悬浮透明外观，去掉 SetWindowRgn 黑框/标题栏触发点）

## 验证命令

```bash
# 前端
npm run typecheck
npm run lint

# Rust 编译（agent loop 无关，做整体 check）
cargo check --manifest-path src-tauri/Cargo.toml

# 实机（手动冒烟）
npm run dev   # 触发选中翻译热键，观察浮窗
```

## 审查门 / 回滚点
- 每步后自查 diff 范围是否越界（只应动 2 个文件）。
- 若实机仍偶发原生标题栏 → 记录，评估升级档 2（design.md），不在本任务强行扩围。
- 回滚：还原 `lens_commands.rs` 与 `Lens.tsx` 两处改动即可。
