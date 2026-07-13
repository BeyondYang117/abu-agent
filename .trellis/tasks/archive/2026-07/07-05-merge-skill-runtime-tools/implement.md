# Implement — 合并 skill 运行时工具为单个 `skill`

## 执行顺序(自底向上:定义 → 派发 → 提示/文案 → 前端 → 测试 → 验证)

### 1. 工具定义与 alias(`mcp/types.rs`)
- [ ] 删 `native_skill_read_file_tool` / `native_skill_run_script_tool`。
- [ ] `native_skill_activate_tool`:`name` → `"skill"`,更新 description(opencode 风格);id 保留 `skill__activate`。
- [ ] `native_skill_tools()` → 只返 activate。
- [ ] `LEGACY_TOOL_ALIASES` 加 `("skill_activate", "skill")`。
- [ ] run_command 描述(types.rs:552)删 skill_run_script 指引。
- [ ] 更新本文件内测试(945-960、1072 附近)。

### 2. skills 底层实现(`skills/runtime.rs`, `catalog.rs`, `discover.rs`)
- [ ] 删 `read_skill_file` / `run_skill_script` / cache 的读/跑方法 / `build_script_command` / 仅供其用的 helper。
- [ ] `activate_skill` 截断标记(282)指向 `read`。
- [ ] `catalog.rs` 34/36 文案:skill_activate→skill,删 read_file/run_script 提及;更新 124 测。
- [ ] `discover.rs:165` 注释更新。
- [ ] 删 runtime.rs 内 run_script/read_file 相关测(526/551/611 等)。

### 3. Chat 运行时派发(`mcp/registry.rs`, `chat/agent/execute.rs`)
- [ ] `call_skill_tool` match 收敛为单 `"skill"` 分支;删 read_file/run_script。
- [ ] 删 `effective_skill_script_timeout_ms`。
- [ ] `execute.rs:574` 删 skill_run_script 超时分支;删 1173/1186/1245 相关测。

### 4. 提示/判定/文案(`prepare.rs`, `commands.rs`, `attachments.rs`)
- [ ] `prepare.rs::is_native_skill_tool_name` → `matches!(name, "skill")`;提示词 609/632/640/877/903 改名 + 脚本指引改 run_python/run_command;更新 953-954、961+、1187+ 测。
- [ ] `commands.rs:4428` read-only 判定 → `name == "skill"`;更新 6632-6670 测(工具列表/block/read-only)、7841/7907。
- [ ] `attachments.rs:480` 文案 → `skill(name=...)`;更新 650 测。
- [ ] `stop.rs` / `dsml_tools.rs` DSML 测:skill_run_script → skill_activate 或删(alias 会规整,保留一条覆盖 alias 更佳)。

### 5. kivio_code 运行时(`kivio_code/`)
- [ ] `executor.rs:247-327` dispatch 收敛为 `"skill"` 单分支;删 read_file/run_script 分支 + `skill_script_allowlist` 引用;更新 68-73 注释、678-751 测。
- [ ] `skill_setup.rs`:工具清单三名→单名;更新 19/108 注释、294-296 测。
- [ ] `interactive/app.rs:1578-1611`:标签/代表键收敛。
- [ ] `interactive/tool_card.rs`:`ToolKind::SkillActivate`/映射(184/210/218)收敛;更新 902-919 测。
- [ ] `mod.rs`:251/283 注释、826/857/860 测中 skill_activate→skill。

### 6. Settings(`settings.rs`)
- [ ] 删 `default_skill_script_allowlist`(776)、字段(853)、Default(887)、sanitize(1901-1909)。

### 7. 前端(`src/`)
- [ ] `api/tauri.ts`:删 `skillScriptAllowlist`(504、1048-1049)。
- [ ] `chat/SkillCenter.tsx`(44、525-529)、`settings/SettingsShell.tsx`(312、3888-3890):删白名单输入块 + 默认值键。
- [ ] `chat/segments.ts:151-153`、`chat/ToolCallBlock.tsx:181-183/803-811/1023`:三 case 收敛为 `skill`;删 pdf_extract_digest 特判。

### 8. 验证
- [ ] `powershell -File scripts/win-cargo-test.ps1`(Windows Rust 测走脚本,见 memory;对照 --lib 既有基线)。
- [ ] `npm run lint`、`npm run typecheck`、`npm test`。
- [ ] `grep -rniE "skill_read_file|skill_run_script|skill_script_allowlist"` 全仓仅剩 alias 说明(应为 0 功能残留)。
- [ ] 手动冒烟:chat 里激活 pdf/docx skill,确认能 read + run_python 完成;kivio_code 激活一个 skill 正常。

## 风险 / 回滚点
- 高波及文件:两运行时派发(registry.rs / executor.rs)、prepare.rs 提示词。改错会导致 skill 无法激活或提示词误导。
- 每完成一节先本地编译(`cargo check`)再继续,避免测试阶段一次性暴雷。
- 回滚:本任务为独立 commit,`git revert` 即可;无数据迁移、无 settings 破坏性变更。

## 验证命令
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `powershell -File scripts/win-cargo-test.ps1`
- `npm run lint && npm run typecheck && npm test`
