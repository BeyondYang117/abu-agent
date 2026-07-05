# Design — 合并 skill 运行时工具为单个 `skill`

## 架构边界

skill 运行时工具由**两个 agent 运行时**并行派发,二者必须同步改动:

1. **Chat GUI 运行时** — `mcp/registry.rs::call_skill_tool`(依赖 Tauri `AppHandle`/`AppState`)。
2. **kivio_code 运行时** — `kivio_code/executor.rs`(headless 终端 coding agent,**无 run_python**;自带 host shell)。

工具定义单一来源:`mcp/types.rs::native_skill_tools()`。两个运行时都消费同一批 `ChatToolDefinition`,派发时按 `tool.name` 分支。合并后 `native_skill_tools()` 只返回一个工具,两处派发分支各自坍缩为单分支。

## 目标形态

- 工具名 `skill`,id `skill__activate`(id 保持不变以免影响持久化/日志键;仅改对外 `name`)。参数 `{ name }`。语义 = 现 `activate_skill`。
- 描述对齐 opencode:`Load a specialized skill when the task at hand matches one of the skills listed in the system prompt. Injects the skill's instructions and resources (including references to scripts/files in the skill directory) into the conversation.`
- `read_file` / `run_script` 工具定义、派发、底层实现、cache 方法、超时逻辑、allowlist 设置全部删除。

## 数据流(合并后)

```
模型发出 skill(name) / 旧名 skill_activate(name)
  → canonical_tool_name: skill_activate → skill (LEGACY_TOOL_ALIASES)
  → match_tool_call 命中唯一 skill 工具
  → call_skill_tool / executor.dispatch_skill: 解析 SkillRecord → 白名单硬 gate
  → activate_with_cache(record) 返回 <skill_content>(正文 + 绝对目录 + <skill_resources>)
  → 记录 activated allowed_tools(T3 收窄)
后续:模型用通用 read(绝对路径) 读资源;用 run_python(沙箱)/run_command(宿主) 跑脚本
```

## 契约变更

### 后端 Rust

- `mcp/types.rs`
  - `native_skill_read_file_tool` / `native_skill_run_script_tool`:删除。
  - `native_skill_activate_tool`:改 `name` 为 `"skill"`,更新 description;id 保留 `skill__activate`。
  - `native_skill_tools()`:只返回 `[native_skill_activate_tool()]`。
  - `LEGACY_TOOL_ALIASES`:加 `("skill_activate", "skill")`。
  - run_command 描述(types.rs:552):删除"Do not use this to run Skill scripts; use skill_run_script..."一句(改为不特殊对待 skill 脚本)。
- `mcp/registry.rs`
  - `call_skill_tool`:match 只留 `"skill" => activate`;删 read_file/run_script 分支。
  - 删 `effective_skill_script_timeout_ms`。
- `skills/runtime.rs`
  - 删 `read_skill_file` / `run_skill_script` / cache 的 `read_file_with_cache` / `run_script_with_cache`、`build_script_command`、相关 helper(`extract_relative_path`/`extract_script_args` 若无其他调用者)。
  - `activate_skill` 输出:`<skill_resources>` 保留(仍告知模型有哪些文件),超大文件截断标记(runtime.rs:282)改为指向 `read`(分页)而非 skill_run_script。
- `skills/catalog.rs`:两处提示文案(34/36)删除 read_file/run_script 提及;把"call skill_activate"改为"call skill";no-tools 回退文案同步。
- `chat/agent/prepare.rs`
  - `is_native_skill_tool_name` → `matches!(name, "skill")`。
  - 提示词(609/632/640/877/903):`skill_activate` → `skill`;删"Skill 脚本走 skill_run_script"一句,改为"skill 脚本用 run_python(沙箱)或 run_command(宿主)"。
- `chat/agent/execute.rs`:删 skill_run_script 超时分支(574)。
- `chat/commands.rs`:`is_read_only_tool` 判定(4428)改为 `tool.name == "skill"`。
- `chat/attachments.rs`:文案(480)`skill_activate(name="{skill}")` → `skill(name="{skill}")`。
- `kivio_code/executor.rs`:`dispatch_skill` match 只留 `"skill"`;删 read_file/run_script 分支与 `skill_script_allowlist` 引用。
- `kivio_code/skill_setup.rs` / `interactive/app.rs` / `interactive/tool_card.rs`:工具名清单/卡片标签/`ToolKind` 映射从三名收敛为 `skill` 单名(`SkillActivate` → 保留/改名 `Skill`)。
- `settings.rs`:删 `default_skill_script_allowlist`、`skill_script_allowlist` 字段、Default 赋值、sanitize(1901-1909)。

### 前端 TS

- `api/tauri.ts`:删 `skillScriptAllowlist` 类型(504)与两处默认合并(1048)。
- `chat/SkillCenter.tsx` / `settings/SettingsShell.tsx`:删白名单输入块 + 默认值里的 `skillScriptAllowlist`。
- `chat/segments.ts` / `chat/ToolCallBlock.tsx`:三个 case 收敛为 `skill`;删 pdf_extract_digest.py 的 skill_run_script 特判(1023);标签"Activate skill"→保留或"Skill"。

## 兼容性 / 迁移

- 旧 settings.json 的 `skillScriptAllowlist` 键:serde 无 `deny_unknown_fields`,读取时忽略,无迁移。
- 模型/persona/skill 白名单里的旧名 `skill_activate`:经 alias 映射到 `skill`,不破。
- 旧名 `skill_read_file`/`skill_run_script`:不做 alias → 未知工具错误(可接受,无内置依赖)。

## 取舍

- **牺牲脚本白名单快速通道**:run_script 消失后,skill 脚本走 run_command(host,敏感、需审批)或 run_python(沙箱)。因无内置 skill 使用,实际影响≈0,换来工具面收敛与两运行时一致。
- **id 保留 `skill__activate`**:避免动持久化/usage 日志键;仅对外 `name` 变。

## 测试影响

- 删/改:`execute.rs`(run_script 超时两测)、`commands.rs`(6632-6670 三工具列表/block 测)、`dsml_tools.rs`(skill_run_script 提取测→改 skill_activate 或删)、`stop.rs`(DSML skill_run_script 测)、`skill_setup.rs`(294-296 三名断言)、`tool_card.rs`(skill_activate 卡片测→skill)、`skills/runtime.rs`(run_script/read_file 相关测删)、`catalog.rs`(124)、`prepare.rs`(953-954)、`attachments.rs`(650)、`types.rs`(945-960)。
- 加:alias 测(`canonical_tool_name("skill_activate") == "skill"`);`native_skill_tools().len() == 1`。
