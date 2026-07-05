# PRD — 合并 skill 运行时工具为单个 skill 工具

## Goal / User Value

把 skill 运行时的三个工具(`skill_activate` / `skill_read_file` / `skill_run_script`)收敛为**单个** skill 工具,对齐 opencode / Claude Code 的 skill 模型。减小工具面、降低弱模型困惑,消除实际无人使用的冗余能力。

## Background / Confirmed Facts

- 三工具定义在 `src-tauri/src/mcp/types.rs:293-387`(`native_skill_activate_tool` / `native_skill_read_file_tool` / `native_skill_run_script_tool`,由 `native_skill_tools()` 聚合)。
- `activate_skill`(`src-tauri/src/skills/runtime.rs:238`)已返回:SKILL.md 正文 + **skill 绝对目录** + `<skill_resources>` 文件清单 + "相对路径相对 skill 目录"说明 —— 已等价于 opencode 的单一 `skill` 工具。
- `skill_read_file`(runtime.rs:263)基本冗余:activate 给了绝对目录,通用 `read` 工具即可读取,且 `read` 具备分页/图片/OCR。唯一独有行为是超大文件截断标记。
- `skill_run_script`(runtime.rs:288)有独立安全边界:`scripts/` 限定 + `skill_script_allowlist` 解释器白名单 + 专属超时(`effective_skill_script_timeout_ms`, execute.rs:574)。**但没有任何内置 skill 在正文里驱动它**(`grep skill_run_script` 内置 skill 命中 0)。
- 内置 skill 的真实执行方式:pdf/docx/xlsx 走 `run_python`(Pyodide);himalaya 走 `run_command`/bash;仅 mcp-builder / skill-creator 有 `scripts/`,且是参考/开发产物,未经 skill_run_script 调用。
- 两个 agent 运行时都派发这三工具,均需处理:
  - Chat GUI:`src-tauri/src/mcp/registry.rs:527 call_skill_tool`
  - kivio_code(headless 终端 coding agent,无 run_python):`src-tauri/src/kivio_code/executor.rs:247` + `skill_setup.rs` + `interactive/{app,tool_card}.rs`
- `skill_script_allowlist` 是用户可编辑设置:后端 `settings.rs:776/853/1901`,前端 `SkillCenter.tsx:525` 与 `SettingsShell.tsx:3888`,类型 `api/tauri.ts:504`。
- `settings.rs` 未用 `deny_unknown_fields` → 移除该字段对旧 settings.json 反序列化安全(未知键被忽略)。
- 前端工具卡片/标签:`src/chat/segments.ts:151-153`、`src/chat/ToolCallBlock.tsx:181-183/803-811/1023`。
- 提示词/目录引用:`skills/catalog.rs:34-36`、`prepare.rs:609/632/640/877/903`、`attachments.rs:480`、`types.rs:552`(run_command 描述里"用 skill_run_script")。
- DSML/解析测试:`dsml_tools.rs`、`stop.rs`;超时/匹配测试:`execute.rs`、`commands.rs`。

## Decision — 方案 A(已定)

只保留 activate 单工具;删除 read_file / run_script 及其后端实现、cache 方法、allowlist 设置与超时逻辑。读文件交给通用 `read`;跑脚本交给 `run_python`(沙箱)/ `run_command`(宿主)。

- **命名(Q1 已定)**:单工具改名为 `skill`(opencode/Claude 对齐)。经 `LEGACY_TOOL_ALIASES` 加 `("skill_activate","skill")`,覆盖①模型旧名调用(`match_tool_call`)②persona/skill 白名单(`tool_matches_recommended_name`)。
- **白名单设置(Q2 已定)**:`skill_script_allowlist` 及其 Settings UI 整块删除(后端字段 + sanitize、前端 SkillCenter/SettingsShell 输入块、api/tauri.ts 类型)。旧 settings.json 残留键被 serde 忽略,无迁移风险。
- **被删工具不做 alias**:`skill_read_file`/`skill_run_script` 参数语义与 `read`/`run_python` 不同,做 alias 会误路由;直接移除,模型收到未知工具错误后自行改用通用工具。
- **无 persona/skill frontmatter 依赖**这三个工具名(已 grep 确认),删除安全。

## Requirements

- R1 skill 运行时只暴露一个工具(activate 语义)。
- R2 删除 `skill_read_file` / `skill_run_script` 的工具定义、注册、两个运行时的派发分支。
- R3 删除 `read_skill_file` / `run_skill_script` 及其 cache 方法、`effective_skill_script_timeout_ms`、`skill_script_allowlist` 设置(后端 + 前端 UI + 类型)。
- R4 更新 activate 输出措辞与所有提示词/目录/附件文案:引导改用 `read` / `run_python` / `run_command`,不再提及被删工具。
- R5 更新 run_command 工具描述,移除"用 skill_run_script 跑 skill 脚本"的指引。
- R6 清理/迁移所有相关测试(Rust + 前端),保持绿。
- R7 前端工具卡片对被删工具名不再有专属分支(或安全降级)。

## Acceptance Criteria

- AC1 `native_skill_tools()` 只返回一个工具;`cargo test` 与 `npm test` / `lint` / `typecheck` 全绿。
- AC2 内置 skill(pdf/docx/xlsx/himalaya 等)激活后仍能完成既有流程(read + run_python/run_command)。
- AC3 Settings 中不再出现 skill 脚本白名单输入;旧 settings.json 正常加载不报错。
- AC4 chat 与 kivio_code 两个运行时行为一致,均无对已删工具的残留派发。
- AC5 grep 全仓无残留的 `skill_read_file` / `skill_run_script` / `skill_script_allowlist`(测试/注释一并清理)。

## Out of Scope

- 不改变 skill 发现/激活的白名单硬 gate 逻辑(persona/assistant skill 允许集)。
- 不新增 skill 能力;不改 run_python / Pyodide 打包。
