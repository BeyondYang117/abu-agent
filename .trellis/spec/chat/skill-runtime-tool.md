# Skill 运行时工具契约

> 来源:07-05-merge-skill-runtime-tools。skill 运行时对模型只暴露**一个**工具。

## 契约

- **单工具**:`native_skill_tools()`(`src-tauri/src/mcp/types.rs`)只返回一个工具。
  - 对模型可见名 `name = "skill"`,内部 `id = "skill__activate"`(id 不变以稳住持久化/usage 日志键)。
  - 语义 = 激活一个 skill:加载 `SKILL.md` 正文 + skill 绝对目录 + `<skill_resources>` 文件清单到上下文(`skills::activate_skill`)。
  - `source == "skill"`;`sensitive == false`(只读)。
- **旧名兼容**:`LEGACY_TOOL_ALIASES`(types.rs)含 `("skill_activate", "skill")`。经 `canonical_tool_name` 覆盖两条输入路径:①模型按旧名 `skill_activate` 出牌(`match_tool_call`)②persona/skill 白名单存的旧名。`is_native_skill_tool_name` 同时认 `"skill"` 与旧名 `"skill_activate"`。
- **无读文件/跑脚本专用工具**:历史上的 `skill_read_file` / `skill_run_script` 已删除。激活后:
  - 读 skill 目录内文件 → 用通用 `read`(activate 输出已给绝对目录)。
  - 跑 skill 附带脚本 → 用 `run_python`(Pyodide 沙箱)或 `run_command`(宿主)。
  - 内置文档 skill(pdf/docx/xlsx)本就走 `run_python`;himalaya 走 `run_command`。无 bundled skill 依赖已删工具。
- **无脚本解释器白名单**:`skill_script_allowlist` 设置(后端字段 + sanitize + 前端 UI)已整体移除。旧 `settings.json` 里残留键被 serde 忽略(无 `deny_unknown_fields`),无迁移。

## 两个运行时都要一致

skill 工具由两处派发,改一处必须同步另一处:
- Chat GUI:`mcp::registry::call_skill_tool`(match `"skill"` 单分支)。
- kivio_code(headless CLI,无 run_python):`kivio_code::executor::CliToolExecutor::dispatch_skill`(同样单分支)。
两处都按 `tool.source == "skill"` 路由,按 `tool.name == "skill"` 分派。

## 激活的副作用(T3 收窄,未变)

激活时 `SkillRunCache::record_activated_allowed_tools(&record.allowed_tools)` 把该 skill 的 `allowed_tools`(来自 frontmatter `recommended-tools`/`allowed-tools`)并入运行期允许集,loop 在后续轮次单调收窄工具面。助手 skill 白名单硬 gate(`skill_id_allowed`)在派发前拦截越权激活。

## 验证入口

- 单测:`mcp::types::tests`(单工具 + alias)、`skills::runtime::tests`、`kivio_code::executor::tests`、`prepare.rs`/`commands.rs` skill 相关测。
- E2E:chat-probe(`docs/chat-probe.md`)——写 `request.json` 触发,`result.json.toolCalls` 应出现 `name:"skill"`。已实测 pdf/xlsx/docx 激活 + docx→run_python 全链路 `completed`。
