# 03 — Skill 系统：clawspring vs kivio

> 范围：skill 发现/加载、SKILL.md 解析、按需注入、内置 skill、skill 执行器。
> clawspring 侧：`skill/`（loader.py、builtin.py、executor.py、tools.py、__init__.py）、`skills.py`、`clawspring.py` 接入点。
> kivio 侧：`src-tauri/src/skills/`（types.rs、discover.rs、parse.rs、catalog.rs、runtime.rs、mod.rs）、`chat/agent/prepare.rs`、`mcp/registry.rs`、`mcp/types.rs`、`chat/commands.rs`、`chat/agent/loop_.rs`。

---

## 1. clawspring 设计精读

clawspring 的 "skill" 本质是**可复用的 prompt 模板**（更接近 Claude Code 的 slash command），而不是 Anthropic Agent Skills 那种"目录 + 资源 + 脚本"的多文件包。整个子系统约 16KB Python，分四层：数据结构（loader）、内置注册（builtin）、执行器（executor）、模型可见工具（tools）。

### 1.1 数据结构：`SkillDef`

`skill/loader.py:9-24`：

```python
@dataclass
class SkillDef:
    name: str
    description: str
    triggers: list[str]          # ["/commit", "commit changes"]
    tools: list[str]             # allowed-tools
    prompt: str                  # frontmatter 之后的完整 prompt 正文
    file_path: str
    when_to_use: str = ""        # 提示模型何时自动调用
    argument_hint: str = ""      # 例如 "[branch] [description]"
    arguments: list[str] = ...   # 具名参数名列表（位置式）
    model: str = ""              # 模型覆盖
    user_invocable: bool = True  # 是否出现在 /skills 列表
    context: str = "inline"      # "inline" 或 "fork"（fork = 子 agent）
    source: str = "user"         # "user" / "project" / "builtin"
```

关键字段设计意图：
- **`triggers`**：一个 skill 可以有多个触发词（`/review` 和 `/review-pr`），缺省自动派生为 `/{name}`（loader.py:88）。
- **`context: inline | fork`**：执行隔离级别声明在 skill 自身的 frontmatter 里，而不是调用方决定。
- **`model`**：fork 模式下允许 skill 指定专属模型（如便宜模型跑 review）。
- **`when_to_use`**：区别于 `description`（给人看），`when_to_use` 是给模型的自动调用提示。

### 1.2 发现与加载：单文件 .md，三级优先级

- 扫描路径只有两个固定目录（loader.py:29-33）：`{cwd}/.clawspring/skills/`（项目级）和 `~/.clawspring/skills/`（用户级），**平铺 glob `*.md`**（loader.py:143），不递归、无子目录概念。
- `load_skills()`（loader.py:128-148）按 builtin → user → project 顺序写入同一个 `dict[name, SkillDef]`，后写覆盖先写，实现 **project > user > builtin** 的同名遮蔽。每次调用都重新读盘，无缓存（保证热加载，代价是每次 IO）。
- 内置 skill 通过 `register_builtin_skill()`（loader.py:122-123）在 import 时注册：`builtin.py:68-100` 注册了 `commit` 和 `review` 两个纯 prompt skill（`file_path="<builtin>"`，`source="builtin"`）。`skill/__init__.py:14` 通过 `from . import builtin` 触发注册——典型的 Python import 副作用注册模式。
- 插件系统有 `load_plugin_skills()`（plugin/loader.py:55-65）返回已启用插件的 skill .md 路径，**但 `load_skills()` 并没有消费它**——插件 skill 实际上是断链的（clawspring 自身的未完成点）。

### 1.3 解析：极简 frontmatter 解析器

`_parse_skill_file`（loader.py:48-114）：以 `text.split("---", 2)` 切 frontmatter；逐行 `key: value`，不支持嵌套 YAML；`_parse_list_field`（loader.py:38-43）同时接受 `[a, b]` 和 `"a, b"` 两种写法。`allowed-tools` 优先于 `tools`（loader.py:84）。`name` 缺失即返回 None（静默丢弃）。

### 1.4 触发与参数替换

- `find_skill(query)`（loader.py:151-164）：取输入的**第一个词**与每个 trigger 做精确匹配（或 trigger 前缀匹配），命中即返回。
- `substitute_arguments`（loader.py:169-184）：两层替换——`$ARGUMENTS` 替换为完整参数串；具名参数按**位置**切分（第 1 个词 → 第 1 个名字），替换 `$ARG_NAME` 大写占位符。

### 1.5 执行模型：inline / fork 双轨 + Skill 工具

三条调用路径，全部收敛到"渲染 prompt → 注入消息"：

**路径 A：用户 slash 触发（主路径）**。`clawspring.py:2502-2526` 的 `handle_slash` 先查内置命令表，**fall-through 到 `find_skill(line)`**（clawspring.py:2520-2525），命中则返回 `(skill, args)` 元组；REPL 在 `clawspring.py:3251-3257` 接住该元组，渲染后以 `run_query(f"[Skill: {skill.name}]\n\n{rendered}")` 把整段 prompt **作为用户消息注入当前会话**。`[Skill: name]` 前缀是给模型的来源标记。

**路径 B：`execute_skill` 执行器**（executor.py:9-36）。按 `skill.context` 分流：
- `inline`（executor.py:39-42）：直接 `agent.run(message, state, ...)`，共享当前会话历史。
- `fork`（executor.py:45-66）：构造 `sub_config = {**config, "_depth": depth+1}`，支持 `skill.model` 覆盖模型、`skill.tools` 写入 `_allowed_tools`，用**全新 `AgentState()`**（无共享历史）跑子 agent，yield 其事件流。
  - ⚠️ 设计缺陷：`_allowed_tools` 只在 executor.py:62 写入，全代码库**没有任何读取方**（agent.py 的工具循环用 `get_tool_schemas()` 返回全量注册表）——工具限制是声明了但未执行的。

**路径 C：模型主动调用 `Skill` / `SkillList` 工具**（skill/tools.py）。
- `SkillList`（tools.py:80-90）：列出全部 skill 的 name/triggers/argument_hint/when_to_use——这是 clawspring 的"渐进式披露"：系统提示里只有一行静态说明（context.py:47-49 只写了 *"Skill: Invoke a named skill… Use SkillList to see available skills"*），**skill 目录本身不进 system prompt**，模型需要先调 `SkillList` 才知道有什么。
- `Skill`（tools.py:42-77）：按 name（再退化按 trigger）查找，渲染参数后**起一个全新 `AgentState` 的子 agent 同步跑完**，把所有 TextChunk 拼接成字符串作为 tool result 返回（tools.py:67-77）。即"skill 即函数"——调用方拿到的是文本产出而非上下文注入。
- 两个工具在 `tools.py:1061-1063`（顶层 tools.py）通过 import 副作用注册进 `tool_registry`（tool_registry.py:37-39 的全局 dict）。

### 1.6 设计巧妙之处小结

1. **同一份 SkillDef 三种消费方式**（slash 注入 / inline-fork 执行 / 工具化函数调用），数据与执行解耦。
2. **`context: fork` + `model` 字段**让 skill 自带隔离与成本声明。
3. **触发词 fall-through**：slash 命令查不到就查 skill，用户体验上 skill 与内置命令无缝。
4. 弱点也明显：无目录式资源/脚本、无安全边界（prompt 模板而已无需沙箱）、`_allowed_tools` 未实施、catalog 不进 system prompt（模型自发用 skill 的概率低，依赖 SkillList 这步额外调用）、插件 skill 断链。

---

## 2. kivio 现状

kivio 实现的是 **Anthropic Agent Skills 规范**（SKILL.md + references/ + scripts/ + assets/ 多文件包），并且有一条完整的"catalog 进 system prompt → skill_activate 按需加载 → skill_read_file / skill_run_script 渐进取资源"的链路。总体成熟度高于 clawspring 的 skill 子系统。

### 2.1 数据结构（skills/types.rs）

- `SkillMeta`（types.rs:25-36）：id/name/description/source/path/`recommended_tools`/`disable_model_invocation`/`files`。
- `SkillFileEntry` + `SkillFileKind`（types.rs:5-21）：skill 包内文件索引，按 `scripts/`、`references/`（或 `.md`）、`assets/` 分类（discover.rs:232-244）。
- `SkillRecord`（types.rs:38-45）：meta + `location`（SKILL.md 路径）+ `base_dir` + `body`（正文全文）+ `allowed_tools`。
- `SkillRegistry::find`（types.rs:96-108）：按 id、slug(id)、name、slug(name) 四路匹配；`slugify`（types.rs:118-135）。

### 2.2 发现（skills/discover.rs）

三类扫描根（discover.rs:47-69）：
1. **builtin**：app bundle `resources/skills/`（discover.rs:32-38）——随安装包分发 pdf/docx/xlsx/doc-coauthoring/skill-creator/frontend-design/mcp-builder 等内置 skill（`src-tauri/resources/skills/`，`tauri.conf.json:42` 打包映射）。
2. **user**：`app_data_dir/skills/`（discover.rs:22-30，自动建目录）。
3. **external**：设置项 `chat_tools.skill_scan_paths`（settings.rs:738）。

递归扫描深度 6（discover.rs:13），跳过 `.git/node_modules` 等（discover.rs:15）；找到 `SKILL.md` 即收录并**停止深入**该目录（discover.rs:130-139）。同 id 去重产出 warning（discover.rs:101-117）。`build_registry_metadata` vs `build_registry`（discover.rs:71-80）区分"只要元数据（不索引文件，UI 列表用）"与"全量（含文件索引）"，避免列表场景的多余 IO。

### 2.3 解析（skills/parse.rs）

`split_frontmatter`（parse.rs:5-54）支持标量与 YAML 列表（`- item` 续行收集为 csv）；`parse_allowed_tools`（parse.rs:71-84）合并 `recommended-tools` + `mcp-tools` + `allowed-tools`（空白分隔）三个别名字段并排序去重；`parse_skill_markdown`（parse.rs:86-125）**强制要求 name 与 description**（报错而非静默丢弃，比 clawspring 严格）；`parse_skill_record`（parse.rs:127-165）校验目录名与 frontmatter name 不一致时产出 warning。支持 `disable-model-invocation`（parse.rs:105-109）。

### 2.4 按需注入：catalog → activate（渐进式披露的真实现）

- `format_catalog`（catalog.rs:3-59）：把已启用 skill 渲染为 `<available_skills><skill><name/><description/><location/></skill>…` XML 块，仅含元数据不含 body；`disable_model_invocation` 的 skill 只在被用户显式 pin 时出现（catalog.rs:14-24）；模型不支持工具时换用提示降级的 header（catalog.rs:33-37）。
- 注入点 `prepare.rs:458-469`：catalog 作为 `"skills"` context segment 进 system prompt（且参与 token 占用统计 `ContextUsageSegment`，prepare.rs:558-579）。
- pinned skill（用户在 UI 选定）：`prepare.rs:481-501` 注入 "User pinned skill … Call skill_activate with this name"；非 pin 场景注入"目录仅供参考，优先内置工具"的反滥用提示（prepare.rs:502-530，中英双语）。
- **三档降级链**（settings.rs:698-700 默认 `progressive`）：
  - `progressive`：catalog + skill_activate 按需加载；
  - `skill_md_only`：模型不支持工具时，`apply_skill_fallback_when_tools_unavailable`（prepare.rs:132-145）自动切到此档，直接把 active skill 的 body 全文注入（prepare.rs:532-544）；
  - `legacy_full_body`：兼容旧行为。
  - commands.rs:1172-1175 还为"provider 中途拒绝 tools"准备了 fallback system prompt（progressive → skill_md_only）。

### 2.5 执行器（skills/runtime.rs + mcp/registry.rs + mcp/types.rs）

三个模型可见工具（mcp/types.rs:211-306，`source: "skill"`）：
- `skill_activate`：返回 `<skill_content>`（body + skill 目录绝对路径 + `<skill_resources>` 文件清单，runtime.rs:95-118）。
- `skill_read_file`：读取相对路径文件；`resolve_skill_path`（runtime.rs:54-93）拒绝 `..`、canonicalize 后校验不逃出 base_dir——**真实的路径穿越防护**。
- `skill_run_script`：仅允许 `scripts/` 下文件（runtime.rs:135-138）；按扩展名映射解释器（py→python3 / js→node / sh→bash，runtime.rs:200-214）并校验**解释器 allowlist**（settings.rs:702-709 默认 python3/bash/sh/node；runtime.rs:216-218）；tokio 子进程 + 超时 kill（runtime.rs:157-165）；超时经 `effective_skill_script_timeout_ms` 三层 clamp（mcp/registry.rs:117-126，execute.rs:493-498 接入统一工具超时）。

分发在 `call_skill_tool`（mcp/registry.rs:439-498）：每次调用**重新 `build_registry` 全盘扫描**（registry.rs:447），查 record、校验 `is_skill_enabled`（settings.rs:979-988，disabled_skill_ids 黑名单）后执行。

`SkillRunCache`（runtime.rs:18-52）：每个 agent run 一个实例（loop_.rs:210 创建，loop_.rs:529→execute.rs:109→registry.rs:316 贯穿传递），对重复 `skill_activate` 返回"already active"短文本、对重复 `skill_read_file` 返回 `[cached]` 前缀内容——**防止模型反复激活/重读浪费上下文**，这是 clawspring 完全没有的 token 工程。

### 2.6 与主循环/前端的其他联动

- skill 工具免审批：`builtin_tool_bypasses_approval`（prepare.rs:204-215）。
- provider 拒绝 tools 时先做 **skill-only 工具重试**再降级纯聊天（loop_.rs:364-378）。
- pinned skill 的 `allowed_tools` 会真实过滤工具列表：`apply_active_skill_tool_filter`（prepare.rs:58-74，调用点 commands.rs:1121-1123 与 2524）——clawspring 声明未实施的事 kivio 做到了。
- pin 链路：assistant snapshot / conversation 级 `active_skill_id` → `resolve_forced_skill_id` 校验启用状态（commands.rs:1084-1100、2037-2057）。
- DSML 容错：deepseek 风格 `<|DSML|invoke name="skill_activate">` 文本也能解析为 skill 工具调用（chat/dsml_tools.rs:170-214 测试）。
- 管理命令：`chat_skills_list/read/open_folder/import`（skills/mod.rs:30-148），import 支持目录与 zip（zip 解包带 `..` 过滤，mod.rs:238-240），前端 `src/api/tauri.ts:1096-1104` 暴露。
- 设置面：`skill_auto_match`（默认 true）、`skill_fallback_mode`、`skill_script_allowlist`、`disabled_skill_ids`、`native_tools.skill_runtime` 总开关（settings.rs:650、738-747；sanitize 校验 settings.rs:1344-1357）。

---

## 3. 差距分析

### 3.1 clawspring 有、kivio 缺失/粗糙

| # | 能力 | clawspring 出处 | kivio 现状 |
|---|------|----------------|-----------|
| G1 | **用户 slash 触发 + 参数替换**：`/commit fix typo` → trigger 匹配 → `$ARGUMENTS`/`$ARG_NAME` 渲染 → 注入用户消息 | loader.py:151-184；clawspring.py:2520-2525、3251-3257 | 无。kivio 只有 UI 级 pin（skill_id），无触发词、无参数占位符；SKILL.md body 是给模型的指令而非可参数化模板 |
| G2 | **fork 执行模式**：skill 在全新会话状态跑子 agent，支持 `model` 覆盖 | executor.py:45-66；SkillDef.context/model（loader.py:22-23） | 无。skill 一律 inline 注入当前会话；无 per-skill 模型覆盖 |
| G3 | **Skill-as-tool（函数式调用）**：模型调 `Skill(name, args)`，子 agent 跑完返回文本作为 tool result，不污染主上下文 | skill/tools.py:42-77 | 无对应物。`skill_activate` 是上下文注入式，激活后所有指令常驻 runtime_messages |
| G4 | `when_to_use` / `user_invocable` 元数据字段（自动调用提示与可见性控制分离） | loader.py:18,22 | 仅 `description` + `disable_model_invocation`（语义上覆盖了 user_invocable 的反面，但缺少独立的 when_to_use 引导字段） |
| G5 | 程序内注册 builtin skill 的轻量机制（无需打包资源文件即可加 prompt 型 skill） | loader.py:119-123；builtin.py:68-100 | builtin 必须是 `resources/skills/` 下的完整 SKILL.md 目录；没有 Rust 侧代码注册纯 prompt skill 的入口 |

注：G5 影响小（资源目录方案对桌面应用更合适），列出仅为完整性。

### 3.2 kivio 有、clawspring 没有（kivio 领先项）

| # | 能力 | kivio 出处 |
|---|------|-----------|
| K1 | **真·渐进式披露**：catalog 元数据进 system prompt + `skill_activate` 按需加载 body + 资源文件清单。clawspring 的 catalog 不进 prompt，依赖模型先调 SkillList，自发使用率必然低 | catalog.rs:3-59；prepare.rs:458-469；runtime.rs:95-118 |
| K2 | 多文件 skill 包（scripts/references/assets 分类索引）+ zip/目录导入 | discover.rs:182-244；mod.rs:110-252 |
| K3 | **脚本执行安全边界**：scripts/ 限定、路径穿越防护、解释器 allowlist、超时 clamp + kill_on_drop | runtime.rs:54-93、128-223 |
| K4 | `SkillRunCache` 防重复激活/重读的 token 工程 | runtime.rs:18-52 |
| K5 | 工具能力降级链（progressive→skill_md_only→legacy_full_body；skill-only 工具重试） | prepare.rs:132-145；loop_.rs:364-378；commands.rs:1172-1175 |
| K6 | `allowed-tools` 真实过滤工具列表（clawspring 写了 `_allowed_tools` 但无消费方） | prepare.rs:58-74 vs executor.py:62 |
| K7 | skill 启用/禁用设置、disable-model-invocation、反滥用 prompt 引导 | settings.rs:979-988；catalog.rs:14-24；prepare.rs:502-530 |
| K8 | DSML 文本工具调用容错（非标准 provider 兼容） | chat/dsml_tools.rs |
| K9 | catalog 作为 context segment 参与 token 占用可视化 | prepare.rs:558-579 |

### 3.3 kivio 自身的粗糙点（与 clawspring 无关但精读中发现）

- **R1：每次 skill 工具调用全盘重扫**。`call_skill_tool` 每次 `build_registry`（mcp/registry.rs:447），一次 run 内模型连续调 activate→read_file→run_script 会扫盘 3 次+；commands.rs:1082 在请求准备阶段也建了一次 registry，但没有传给工具执行层复用。
- **R2：模型自主 `skill_activate` 后不收紧工具列表**。`apply_active_skill_tool_filter` 只在请求准备阶段对 pinned skill 生效（commands.rs:1121-1123）；run 中途模型激活某 skill 后，其 `allowed_tools` 不会影响后续轮次的工具集（工具列表在 loop_.rs:196 取定后除 skill-only 重试外不变）。
- **R3：`skill_read_file` 无大小上限**。runtime.rs:120-126 直接 `fs::read_to_string`，超大 reference 文件会撑爆上下文（native read_file 有行数/字节约束，skill 版没有）。
- **R4：缓存命中文案弱**。`activate_with_cache` 第二次只回 "already active"（runtime.rs:28-33），若首次激活内容已被上下文压缩裁掉，模型将无法恢复 skill 指令。

---

## 4. 重构建议

### P0-1：Skill 注册表缓存（修 R1）— 工作量：小（0.5 天）

- 在 `AppState` 增加 `skill_registry_cache: RwLock<Option<(Instant, SkillRegistry)>>`（或挂在 `SkillRunCache` 上按 run 缓存）。
- `call_skill_tool`（mcp/registry.rs:439）优先取缓存；失效策略：run 开始时刷新一次（commands.rs:1082 已建好的 registry 直接塞进 run 级缓存传下去），run 内不再扫盘。`SkillRunCache`（runtime.rs:18）已贯穿整条调用链（loop_.rs:529 → execute.rs:109 → registry.rs:316），加一个 `registry: Option<SkillRegistry>` 字段即可，无需新管道。
- Tauri 约束：`SkillRegistry` 已是 `Clone`，跨 await 传递无碍；注意 `chat_skills_import` 后需失效缓存（mod.rs:111）。

### P0-2：slash 触发 + 参数替换（补 G1）— 工作量：中（2-3 天）

- `parse.rs`：frontmatter 增加 `triggers`、`argument-hint`、`arguments` 字段（复用 `parse_list_value`，parse.rs:56-69）；`SkillMeta` 加对应字段（types.rs:25），缺省 trigger = `/{id}`（对齐 loader.py:88）。
- 后端入口：在 `chat_send_message` 系命令（chat/commands.rs）的用户消息预处理处加 `find_skill_by_trigger(&registry, first_word)`；命中后做 `$ARGUMENTS`/`$ARG_NAME` 替换（新函数 `skills::substitute_arguments`，逻辑照搬 loader.py:169-184，~30 行），将渲染结果以 `[Skill: name]` 前缀替换用户消息内容，同时把该 skill 设为本条消息的 active_skill_id（走既有 pinned 链路 commands.rs:1084-1100，自动获得 allowed-tools 过滤与 catalog 标注）。
- 前端联动：`src/api/tauri.ts` 的 `SkillMeta` 类型加 `triggers/argumentHint`；聊天输入框 `/` 自动补全可后做（P2），先保证后端纯文本触发可用。

### P1-1：run 中途激活 skill 的动态约束 + read_file 上限（修 R2/R3）— 工作量：中（1-2 天）

- `SkillRunCache` 记录"本 run 已激活的 skill 的 allowed_tools 并集"；`execute_tool_round` 前（loop_.rs:515 附近）若该集合非空且发生变化，对 `tools` 应用一次 `apply_active_skill_tool_filter` 等价过滤（prepare.rs:58-74 已有现成逻辑，需提为可在 loop 内复用的形式）。注意：过滤后需保留 skill 三件套与 builtin（现有函数已处理）。
- `read_skill_file`（runtime.rs:120-126）加字节上限（建议复用 native read_file 的截断策略），超限返回前半 + 截断标记，提示模型用 `skill_run_script` 处理大文件。

### P1-2：Skill 函数式调用 / fork 子 agent（补 G2/G3）— 工作量：大（1-2 周，建议与子 agent 专项合并）

- 这是 clawspring 最有价值的差异能力，但在 kivio 中不应按 skill 维度单独造：`run_agent_loop`（loop_.rs:190）已经是 host/executor 抽象，可新增 `skill_run_forked` 工具（或 `context: fork` frontmatter 字段），实现为：构造独立 `runtime_messages`（system prompt = 主 prompt + skill body）、独立 generation、可选 per-skill provider/model 覆盖（settings 已有多 provider 基建），跑完把最终文本作为 tool result 返回。
- Tauri 约束：子 agent 的流事件需要独立 run_id 并决定是否向前端转发（建议折叠为父工具卡片内的嵌套进度，React 侧 ToolCallBlock 扩展）；取消传播复用 `explain_stream_generation` 同款 generation 机制。
- 在子 agent 内 **真正实施 allowed_tools**（kivio 已有过滤函数，等于把 clawspring 没做完的事做对）。

### P2：增量改进

- `when_to_use` frontmatter 字段进 catalog 的 `<skill>` 节点（catalog.rs:41-56 加一行），提升 auto-match 精度（补 G4）。工作量：极小。
- 缓存命中文案改为返回 body 摘要或在检测到上下文压缩后允许重新注入（修 R4）。
- 聊天输入框 `/` 触发的 skill 自动补全 UI（依赖 P0-2）。
- 插件/商店式 skill 分发（clawspring plugin 体系的对应物）——kivio 已有 import 基建（mod.rs:110-252），缺的是远端索引与更新检查，独立立项。

### 优先级理由

P0 两项一个是纯性能修复（每次工具调用全盘扫描在 skill 多 + 外部扫描路径深时是可感知卡顿），一个是用户可感知的高频交互能力（slash skill 是 Claude Code 用户的肌肉记忆）且实现成本低、完全复用既有 pinned 链路。fork/函数式调用价值最高但牵涉子 agent 事件流与前端展示，应等多 agent 架构专项一起做，避免两套子 agent 机制。
