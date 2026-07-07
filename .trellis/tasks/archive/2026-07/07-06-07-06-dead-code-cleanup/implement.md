# Implement — 死代码清理执行计划

## 前置：基线

- [ ] 记录 Rust 测试基线：`powershell -File scripts/win-cargo-test.ps1`（对照 memory 中已知的 ~14 个 --lib env/locale 失败 + web_search_empty_query 失败，作为「无新增失败」判据）。
- [ ] 前端基线：`npm run lint && npm run typecheck && npm test` 当前应全绿（若有既有红，先记录）。

## 批次 1 — 前端（commit: `refactor(frontend): drop verified dead code + orphan scripts`）

- [ ] 1a 整文件删除：`App.css`、`SkillSelector.tsx`、两个孤儿脚本。
- [ ] 1b 死 i18n key：逐 key bare-substring grep 复验零命中后删（zh + en 两侧）。
- [ ] 1c 零调用导出/组件/prop/字段（见 design 1c 清单）。
- [ ] 1d 近重复合并（见 design 1d 清单）；测试专用导出改造对应 `.test.ts(x)`。
- [ ] 验证：`npm run lint && npm run typecheck && npm test` 全绿。
- [ ] Review gate → commit。

## 批次 2 — Rust core（commit: `refactor(core): dedupe api/state, drop dead command attrs`）

- [ ] 4 个 lens fn 去 `#[tauri::command]` + invoke_handler 行（保留 fn）。
- [ ] api.rs：failure 记录闭包收敛 + 删三 wrapper + 内联 `send_with_retry_for_failover`。
- [ ] state.rs：AppState 基构造器；`chat_stream_generations`→AtomicU64；detected-agents 缓存复用。
- [ ] rapidocr `pipeline()` 提取；windows_ocr async 壳合并。
- [ ] 验证：`cargo build`；`scripts/win-cargo-test.ps1` 无新增失败。
- [ ] Review gate → commit。

## 批次 3 — chat 后端（commit: `refactor(chat): remove dead agent config/steps machinery`）

- [ ] 删 steps 累积机制 + 改 loop_tests 断言。
- [ ] 删 AgentRunConfig 六字段 + 4 构造点。
- [ ] 删 capabilities()/ProviderCapabilities（4 adapter）。
- [ ] 内联 prepare_agent_step/PreparedStep。
- [ ] memory 复用 storage::atomic_write。
- [ ] compaction 三 fn 移 `#[cfg(test)]`；stop.rs 删 `evaluate_stop_after_model_step`。
- [ ] todo.rs 简化；小件（truncate_chars/stream/text_content/temperature/filter marker/plan wrappers/storage allow）。
- [ ] 验证：`cargo build`；loop_tests + chat 相关测试无新增失败。
- [ ] Review gate → commit。

## 批次 4 — kivio_code / external_agents（commit: `refactor(cli): drop dead tui components + cli parsers`）

- [ ] 删 input.rs / autocomplete.rs / loader CancellableLoader / text BoxView+TruncatedText。
- [ ] 删 json_events 四 handler + 4 变体 + 不可达臂（保留 kimi）。
- [ ] acp_def 帮助函数 + 数据行替换四 def。
- [ ] keys.rs / keybindings.rs / text_width.rs 死 API。
- [ ] UnifiedAgentEvent 三变体 + 构造点；run append 合并；slash RuntimeContext 上提；detection 不可达臂；codex 沙箱探测。
- [ ] 零调用小件批（见 design 批 4 末尾清单）；只写不读字段；connectors slugify 复用。
- [ ] 验证：`cargo build`；相关测试无新增失败。
- [ ] Review gate → commit。

## 批次 5 — 依赖（commit: `chore(deps): remove unused deps + trim windows features`）

- [ ] **先** `cargo check`（Windows）确认 windows-future / windows features 传递关系，再删。
- [ ] 删 clipboard-manager（Cargo + lib.rs init）、ndarray、windows-future。
- [ ] windows features 去 Globalization/Foundation_Collections；tar 移 macOS target。
- [ ] package.json 删 @types/katex。
- [ ] 验证：`cargo check` + `npm run typecheck` 通过。
- [ ] Review gate → commit。

## 收尾

- [ ] 全量 last-iteration check（trellis-check 或 lint+typecheck+test+cargo build 全跑）。
- [ ] 确认保留项未被触碰：`git log -p` 中不含 mockChatApi / ChatDotGridBackground 变更。
- [ ] 更新 spec（若删除影响 .trellis/spec 中记载的符号）。
- [ ] 统计实际净减行数。

## 验证命令

- 前端：`npm run lint`、`npm run typecheck`、`npm test`
- Rust 编译：`cargo build --manifest-path src-tauri/Cargo.toml`
- Rust 测试：`powershell -ExecutionPolicy Bypass -File scripts/win-cargo-test.ps1`
- 依赖检查：`cargo check --manifest-path src-tauri/Cargo.toml`

## 回滚点

每批独立 commit。任一批验证失败且无法快速修复 → `git revert` 该批 commit，不影响其余批次。
