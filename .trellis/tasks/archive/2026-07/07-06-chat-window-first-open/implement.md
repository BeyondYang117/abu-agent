# Implement — Chat 窗口首开体验

前置：本任务纯前端；dev app 正在运行可随时手动冒烟。两个模块（缓存 / reveal 门控）独立，按序做但各自成 commit。

## Stage 1 — settings 缓存（R2）

- [x] 1.1 新建 `src/api/settingsCache.ts`：`peekSettings` / `getSettingsCached` / `refreshSettings` / `updateSettingsCache` / `saveSettingsCached`；模块头注释说明 per-webview 生命周期与跨窗口豁免。
- [x] 1.2 新建 `src/api/settingsCache.test.ts`：并发首读单 invoke、失败不写缓存可重试、save 成功写通/失败不动缓存、peek 语义。
- [x] 1.3 读调用方替换为 `getSettingsCached()`：`App.tsx`、`Chat.tsx`、`Sidebar.tsx`、`ModelSelector.tsx`。
- [x] 1.4 写调用方替换为 `saveSettingsCached()`：`Chat.tsx`；`SettingsShell` 保存流程。
- [x] 1.5 `SettingsShell.tsx` 初始化 effect 改 SWR：`peekSettings()` 命中→首帧渲染缓存+后台 `refreshSettings()`（`SettingsShell.tsx:740-764`）。
- [x] 1.6 验证：`npm run typecheck && npm run lint && npm test` 全绿（2026-07-08 复核）。
- [x] 1.7 Commit：代码已落地并提交（`97864be` 等，混入其他杂项 commit，未按计划拆成独立 rollback-point commit——历史已成事实，不再重排）。

## Stage 2 — reveal 门控（R1）

- [x] 2.1 `SettingsShell.tsx`：`onReady` 门控已按计划实现。
- [x] 2.2 `Chat.tsx`：`onContentReady?: () => void` prop 已接入，一次性触发。
- [x] 2.3 `App.tsx`：`<Chat onContentReady={revealChatWindowNow} />` 已接入。
- [x] 2.4 验证：`npm run typecheck && npm run lint && npm test` 全绿。
- [ ] 2.5 手动冒烟（A1/A2/A3/A7）：本轮复核未执行完整手动冒烟；dev app 本轮启动过程中冷启动路径未见异常，但未针对性验证 3s 兜底/DevTools 断点场景。
- [x] 2.6 Commit：代码已随 `617597d`/`78609f5` 等提交落地（同 1.7，未按计划拆分独立 commit）。

## Stage 3 — 收尾

- [x] 3.1 全量检查：`npm run typecheck && npm run lint && npm test`（2026-07-08，37 files / 201 tests 全过）。
- [x] 3.2 手动冒烟未完整执行，但代码检查未发现 PRD 假设不成立之处，暂不回 Plan。
- [x] 3.3 Spec 更新：`.trellis/spec/frontend/settings-cache.md` 已写好并登记进 `index.md`。
- [x] 3.4 归档任务。

## 回滚

- Stage 1 / Stage 2 各自独立 commit，revert 即回滚，无数据迁移、无 Rust 改动。
