# 删除审计确证的死代码与孤儿脚本

## Goal

根据 2026-07-06 的 ponytail-audit 全库审计，删除**确证的死代码**（零调用方的函数/组件/字段、死 i18n key、死依赖）、合并近重复逻辑、清理孤儿开发脚本。目标是在**不改变任何用户可见行为**的前提下缩减代码量。

## Scope（用户已确认）

包含：
- **确证死代码**（~4100 行）：零调用方的函数/组件/字段、死 i18n key、死 Cargo/npm 依赖、近重复逻辑合并。
- **孤儿开发脚本**（~1130 行）：`scripts/chat-katex-perf-smoke.mjs`、`scripts/chat_skill_e2e.py`。

明确**排除**（本次不动）：
- `mockChatApi` 及浏览器预览分支（保留浏览器预览能力）。
- `ChatDotGridBackground` 装饰花纹简化（纯视觉，非死代码）。

## Constraints

- 零行为变更：所有删除项必须是审计已 grep 验证「零生产调用方」或「逐字重复可合并」。
- 删除后 `npm run lint`、`npm run typecheck`、`npm test` 必须全绿。
- Rust 侧删除后 `cargo build` 通过；测试基线对照既有已知失败（见 memory：windows-cargo-lib-preexisting-failures / windows-rust-test-manifest）。
- 依赖删除（`tauri-plugin-clipboard-manager`/`ndarray`/`windows-future`/`@types/katex`、windows features、tar 移 target）**必须**先 `cargo check` 验证再删——windows-future / windows features 依赖 Windows 平台传递关系。
- 测试专用的出货导出被删时，需同步改造对应测试（改为从生产 API 或 localStorage seed 驱动），不得留下坏测试。
- 分批提交，每批可独立回滚。

## Acceptance Criteria

- [ ] 前端：所有确证死代码/死 i18n/近重复合并完成，`npm run lint && npm run typecheck && npm test` 全绿。
- [ ] Rust：所有确证死代码/近重复合并完成，`cargo build` 通过，测试无新增失败（对照基线）。
- [ ] 依赖：4 个死依赖删除，windows features/tar 调整，`cargo check`（Windows）通过。
- [ ] 孤儿脚本删除。
- [ ] 保留项（mockChatApi、ChatDotGridBackground）未被触碰。
- [ ] 无行为变更（不引入功能改动，仅删除/合并）。

## Notes

审计原始清单见本任务 `design.md` 的分组条目。
