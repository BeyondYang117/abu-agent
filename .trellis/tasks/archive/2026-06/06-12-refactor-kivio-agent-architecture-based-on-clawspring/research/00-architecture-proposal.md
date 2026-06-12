# 00 — Kivio Agent 架构重构总体方案（基于 clawspring 对照研究）

> 输入：本目录 01–07 共 7 份子系统研究文档（核心循环 / 内置工具 / skill / MCP / multi-agent / task / memory）。
> 总体判断（各文档一致结论）：**kivio 不是落后实现**——单次执行质量（错误恢复、取消、审批、安全、前端联动）全面领先 clawspring；落后的是**代码组织（790 行单函数循环、4-5 处硬编码工具名单）、上下文窗口治理（循环内无压缩）、连接生命周期（MCP 每调用重建连接）、以及缺失的 multi-agent 薄壳**。
> 重构方针：**移植 clawspring 的"骨架瘦、注册表统一、压缩进循环、子 agent = 新 state + 旧循环"四个结构思想，原样保留 kivio 的全部鲁棒性资产，不重写。**

---

## 1. 目标分层与 Rust 模块归属

参照 clawspring 的分层（REPL → agent 循环 → providers/compaction/tool_registry/context），kivio 重构后的目标分层如下。依赖方向严格单向（上层依赖下层，下层不知上层）：

```
┌─ 编排层（Tauri command / Conversation 持久化 / 事件外发）
│    chat/commands.rs（瘦身后）、chat/storage.rs、ChatAgentHost
│
├─ Multi-agent 层（后续阶段）
│    chat/agent/runner.rs   —— 无 UI 的 agent 运行入口（SubAgentRequest → run_agent_loop）
│    chat/agent/manager.rs  —— SubAgentManager（任务表、Semaphore 并发上限、收件箱）
│    chat/agent/definitions.rs —— AgentDefinition 三层加载（内置/用户/项目 .md）
│
├─ 核心循环层（chat/agent/，P0 拆分目标）
│    loop_.rs       骨架 <150 行：loop { prepare → planning → tool_round } → synthesis → finalize
│    planning.rs    规划请求（流式/非流式 + tools-unsupported 三段降级 + planning 即终稿）
│    rounds.rs      execute_tool_round（并行批次 / 取消补齐 / 阻断反馈）
│    synthesis.rs   合成请求（合并现有流式/非流式两套重复 fallback）
│    finalize.rs    RunResultBuilder（收敛 6 处 "emit + 构造 AgentRunResult" 重复块）+ 会话末钩子位
│    compaction.rs  循环内上下文治理（snip 旧 tool 结果 + 轮内超限摘要压缩）
│    prepare.rs / stream.rs / stop.rs / execute.rs / host.rs / types.rs（保留，职责不变）
│
├─ 统一工具注册表层（P0 新建，详见 §2）
│    tools/registry.rs（由 mcp/registry.rs 演化）—— 唯一的 schema 汇集 + 分发 + 权限元数据入口
│    native_tools/  —— 静态注册表 &'static [NativeToolDef]，文件/shell/fetch 实现不动
│
├─ 能力子系统层（互相独立，都只通过注册表暴露工具）
│    skills/        —— 发现/解析/catalog/runtime（已成熟，增量改进）
│    mcp/           —— client.rs + 新增 manager.rs（持久连接生命周期）
│    chat/todo.rs   —— task 工具（增量升级，不重写）
│    chat/memory.rs —— memory 工具（增量升级）+ 新增 memory_consolidate.rs
│
└─ 模型层（chat/model/，已达标，仅小改）
     types.rs（ModelMessage 中立格式 / LanguageModelProvider trait）
     openai.rs / anthropic.rs、api.rs 多 key failover
```

与 clawspring 的对应关系：`agent.py` ↔ 拆分后的 loop_.rs 骨架；`tool_registry.py` ↔ tools/registry.rs；`compaction.py` ↔ chat/agent/compaction.rs；`providers.py` ↔ chat/model/（kivio 已更好）；`multi_agent/subagent.py` ↔ runner.rs + manager.rs；`task/` ↔ todo.rs；`memory/` ↔ memory.rs。

**核心架构资产盘点（重构红线，全部保留）**：
- `AgentHost` / `ToolExecutor` 双 trait 抽象（host.rs:10-58、execute.rs:24-32）——循环不依赖 Tauri，已有 3 个 TestHost 验证可替换性；
- 多层错误恢复（tools-unsupported 三段降级、流中断降非流式、合成失败保工具结果、参数损坏结构化回喂）；
- 代际取消（state.rs:174-210）+ 协议完整性（取消时补齐全部 pending tool record）；
- 注解驱动安全模型（readOnly/destructive/openWorld → 审批/并行/敏感三套策略）；
- JSON Schema 入参校验、路径安全模型、原子写、占位符拒绝、DSML 回退、多 key failover；
- segments 结构化流式 UI 协议与 chat-stream/chat-tool/chat-context/chat-todo 事件契约。

---

## 2. 统一工具注册表设计

### 2.1 现状问题（文档 02 §2.1）

新增一个内置工具要同步改 4-5 处：`mcp/types.rs` 构造函数、`list_native_builtin_tool_defs`（types.rs:755）、`call_native_tool` 17 臂字符串 match（registry.rs:511）、`BUILTIN_NAMES`（prepare.rs:159）、`tool_call_parallel_eligible` 白名单（loop_.rs:1516）、`builtin_tool_bypasses_approval`（prepare.rs:204）。五份名单彼此漂移是结构性 bug 来源。

### 2.2 设计

`ChatToolDefinition`（types.rs:7-21）已经是统一 schema 载体（native/skill/mixer/mcp 四源共用，含 input_schema/annotations/output_schema/sensitive），**保留不动**。注册表在它之上补"行为元数据 + 分发函数"：

```rust
// tools/registry.rs
pub enum ToolSource { Native, Skill, Mixer, Mcp { server_id: String }, SubAgent, Task }

pub struct NativeToolDef {                       // 静态注册表条目（native/task/subagent/memory 工具）
    pub name: &'static str,
    pub def: fn(&Settings) -> ChatToolDefinition, // schema 构造（替代 types.rs 散装构造函数）
    pub enabled: fn(&Settings) -> bool,           // 替代 list_native_builtin_tool_defs 的 if 链
    pub parallel_safe: bool,                      // 替代 loop_.rs:1516 白名单
    pub bypasses_approval: bool,                  // 替代 prepare.rs:204 名单
    pub call: NativeToolCallFn,                   // 替代 call_native_tool 17 臂 match
}
pub static NATIVE_TOOLS: &[NativeToolDef] = &[ /* read_file, write_file, ..., todo_write, memory_read, agent_spawn(后续) */ ];
```

要点：
1. **静态表而非运行时注册**：Rust 桌面应用没有"import 副作用注册"的需求，`&'static [NativeToolDef]` 比 `OnceLock<RwLock<HashMap>>` 更直接、可测试、无锁。MCP/skill 工具仍是动态来源，由 registry 的 list 阶段叠加（保留现有 TTL 缓存与 cache-key-绑定设置的失效机制，registry.rs:194-245）。
2. **统一分发入口**：`registry.call(ctx, tool, args)` 按 `ToolSource` 分流——Native 查静态表、Skill 走 skills/runtime、Mcp 走 manager（阶段 3 后为持久连接）。`RegistryToolExecutor` 中 todo 工具的特判分流（commands.rs:3431-3442）收编为静态表中带 conversation 上下文的 native 条目。
3. **权限/并行元数据单源化**：`execute.rs::tool_requires_approval` 与 `tool_call_parallel_eligible` 改为读注册表元数据；MCP 工具继续由 annotations 推导（is_read_only_tool / mcp_tool_requires_confirmation 逻辑不变，只是挂到 registry 条目上）。
4. **未来扩展即加行**：subagent 的 `agent_spawn/agent_check/agent_list` 工具、task 升级工具、memory_search、grep 工具，全部只在静态表加一项 + 一个实现文件，循环与 commands.rs 零改动——这是把 clawspring "注册表 = 唯一扩展点" 的核心价值落到 Rust。
5. **DSML / openai_tool_name sanitize / schema 校验**保持在现有位置不动，注册表只管"有哪些工具、谁能并行、谁免审批、怎么调"。

### 2.3 迁移策略

机械等价重构：先建静态表并让旧函数（list_native_builtin_tool_defs 等）改为查表实现，跑全量测试；再逐个调用点切到 registry API；最后删旧名单。每步 `cargo test` 守护，行为零变更。

---

## 3. 各子系统重构要点汇总（去重排序）

按"基础设施 → 治理 → 能力补齐 → 新能力"排序；标注来源文档与建议阶段：

| # | 要点 | 来源 | 阶段 |
|---|------|------|------|
| 1 | 拆分 `run_agent_loop`（790 行 → 骨架 <150 行 + planning/rounds/synthesis/finalize），先补 6 个 fallback 场景集成测试再动刀 | 01 P0-1 | P0 |
| 2 | 统一工具注册表（§2，收敛五份名单） | 02 P2-1（提级） | P0 |
| 3 | 循环内上下文治理：`compaction.rs` —— Layer1 snip 旧 tool 结果（头 1/2+尾 1/4）+ 轮内超限触发摘要压缩；压缩函数从 commands.rs 抽为与 Conversation 解耦的纯函数；memory_l1 段列为不可压缩固定开销 | 01 P0-2、07 P0-B | P1 |
| 4 | usage/成本贯通：AgentStepResult/AgentRunResult 携带 ModelUsage，前端显示真实 token，校准 chars 启发式 | 01 P1-2 | P1 |
| 5 | 取消原语：100ms 轮询 → `tokio::sync::watch` | 01 P1-1 | P1 |
| 6 | bug 级工具修复：edit_file CRLF 归一匹配（Windows 高频 0 命中）、错误信息升级为修复指南、read_file 给模型加行号、write/edit 回传裁剪 diff、截断改头+尾保留 | 02 P0-1/2/3、P1-2/3 | P1 |
| 7 | grep/regex 搜索工具（rg 探测 + regex crate 回落），search_files 上限治理 | 02 P1-1 | P1 |
| 8 | MCP 持久连接管理器：McpSession（pending 表 + stderr 尾部缓存 + 探活透明重连 + session-id 复用）、退出钩子杀进程 | 04 P0 | P2 |
| 9 | MCP image content 进 artifacts（现被静默丢弃）、命名冲突治理（OpenAI 适配器去重、裸名回退限非 MCP）、状态事件 + 设置页状态面板、启动期并行预热 | 04 P0/P1 | P2 |
| 10 | skill 注册表缓存（每次工具调用全盘扫描 → run 级缓存）、slash 触发 + `$ARGUMENTS` 参数替换、run 中途激活的 allowed_tools 动态过滤、skill_read_file 大小上限 | 03 P0/P1 | P2 |
| 11 | todo 模型增强：cancelled 状态 + deleted 伪状态删除、可选 description、幂等反馈（changed 字段） | 06 P0 | P2 |
| 12 | subagent：runner.rs 无 UI 入口 + SubAgentHost（depth>0 敏感工具直拒、级联取消）→ SubAgentManager + Agent/Check/List 工具 + depth 防护 → agent 类型三层加载（工具过滤真正生效）→ 后台执行 + tool card 联动 | 05 P0/P1 | P3 |
| 13 | skill fork 执行 / Skill-as-tool 函数式调用——与 subagent 合并实现，避免两套子 agent 机制 | 03 P1-2 | P3 |
| 14 | task 作用域升级 Project 级 + 用户侧操作入口；依赖边 blocks/blocked_by + owner **仅当 subagent 落地后**引入 | 06 P1/P2 | P4 |
| 15 | memory：会话末 AI 固化钩子（finalize 阶段、默认关）、L2 条目化 + memory_search 关键词检索 + 陈旧度提示、project scope 记忆 | 07 P0-A/P1 | P4 |
| 16 | 中立格式 ModelMessage 作为循环货币（替换 runtime_messages: Vec<Value>），风险较高，在拆分稳定后独立做 | 01 P2-1 | P4（可选） |
| 17 | 杂项：join_all 替换手写 1/2/3/4 并发分支、registry.rs 死代码清理、capabilities/serverInfo 落地、`.mcp.json` 项目级支持（默认 disabled） | 01/04 P2 | 顺带 |

**明确不移植**（各文档一致）：clawspring 的无路径安全、generator 可变对象审批、KeyboardInterrupt 取消、`os.chdir` 线程污染、未实施的 `_allowed_tools`/`concurrent_safe` 空字段、旧版 SSE 双端点协议、DuckDuckGo 爬虫搜索。

---

## 4. 分阶段实施路线

每阶段可独立交付、独立验证、独立成 PR（或 PR 组）。依赖关系：P0 → P1 → （P2 各项可并行）→ P3 → P4。

### P0 — 基础设施（必须先行，约 1-1.5 周）

> 纯结构重构，**行为零变更**。是后续一切阶段的地基。

- **P0.a 回归测试先行**：把 loop_.rs 的 6 个 fallback 场景（流式失败/取消/空输出、非流式失败/空输出、tool_planning_failed）补成显式集成测试；现有 ~840 行测试迁入 loop_tests.rs。
- **P0.b 拆分 run_agent_loop**：planning.rs / rounds.rs / synthesis.rs / finalize.rs（要点 1）。约束：AgentHost/ToolExecutor trait 签名不动、commands.rs:1212 调用点不动、前端事件 payload 不动。
- **P0.c 统一工具注册表**：静态 NativeToolDef 表 + registry 分发收敛（要点 2）。
- **验证**：`cargo test` 全绿（含新增 fallback 测试）、`npm run typecheck`、手工冒烟（一轮含工具调用的对话、审批弹窗、取消、todo 指示器）。
- 依赖：无。

### P1 — 上下文治理 + 工具人体工学（约 1-1.5 周）

> 用户可感知收益最高的阶段：长任务不再爆窗口、Windows edit 不再失败、模型自我纠错能力提升。

- compaction.rs 循环内治理（要点 3）；usage 贯通（要点 4）；watch 取消原语（要点 5）；
- 工具 bug 修复五连（要点 6）+ grep 工具（要点 7）。
- **验证**：cargo test（compaction/snip/CRLF 单测）+ 手工冒烟：构造长工具链任务观察 chat-context 占用条自动回落；Windows（或 CRLF 测试文件）edit_file 成功。
- 依赖：P0（compaction 挂 prepare 钩子、grep 进注册表）。

### P2 — 连接生命周期 + skill/task 增量（约 1.5-2 周，三条线可并行）

- **P2.a MCP**：持久连接管理器 + image artifacts + 状态面板 + 预热 + 命名冲突（要点 8/9）。
- **P2.b Skill**：注册表缓存 + slash 触发 + 动态 allowed_tools + read 上限（要点 10）。
- **P2.c Task**：todo 模型增强（要点 11）。
- **验证**：MCP——同一服务器连续 10 次工具调用仅 1 次握手（日志断言）、kill 服务器进程后下次调用透明重连、app 退出无孤儿进程；Skill——`/commit xxx` 触发与参数替换；todo——cancelled/deleted 前端渲染。
- 依赖：P0（注册表）；P2 三条线互不依赖。

### P3 — Multi-agent（约 2-2.5 周）

- runner.rs + SubAgentHost → SubAgentManager + Agent/CheckAgentResult/ListAgentTasks 工具（经静态注册表注册）→ AgentDefinition 加载与工具过滤 → 后台执行 + 前端 tool card 状态联动（要点 12/13）。
- 设计决策：子 agent 共享只读 L1 memory 注入、不注册 memory_modify（文档 07 §与 05 交叉决策）；并发 Semaphore(3)；v1 不落盘 Conversation。
- **验证**：cargo test（depth 防护/取消级联/工具过滤单测）+ 冒烟：模型 spawn 一个 researcher 子 agent 同步返回结果；父取消时子终止；后台任务完成后 tool card 更新。
- 依赖：P0（注册表 + 循环拆分）、强烈建议 P1 先行（子 agent 更易触发上下文超限）。

### P4 — Task/Memory 进阶 + 可选深水区（约 2 周，按需取舍）

- project 级 todo + 用户操作入口；依赖边 + owner（前置：P3 已交付）；
- memory 固化钩子（挂 finalize.rs）、memory_search、project scope（要点 14/15）；
- 可选：ModelMessage 中立格式货币化（要点 16，独立 PR，风险自担）。
- 依赖：P3（owner/依赖边、子 agent 记忆策略）、P0（finalize 钩子位）。

---

## 5. 风险清单与回归验证

| # | 风险 | 触发阶段 | 缓解与回归手段 |
|---|------|---------|---------------|
| R1 | **流式事件契约破坏**：chat-stream/chat-tool/chat-context/chat-todo 的 payload 字段（conversationId/runId/messageId/segmentId/phase/round）被拆分改动，前端渲染错乱 | P0/P1 | 拆分约束"事件 payload 不动"写进 PR checklist；为 emit 路径补 payload 快照测试；冒烟必查 reasoning 折叠、工具卡片、todo 指示器、上下文占用条四个前端联动点 |
| R2 | **6 处 fallback 行为回退**：合成失败/取消/空输出的双语兜底文案与"保住工具结果"语义在合并重复代码时丢失 | P0.b | P0.a 先补集成测试再动刀（硬性顺序）；synthesis.rs 合并后逐场景断言 stream_outcome 与 content |
| R3 | **压缩破坏持久化与回放**：compaction 修改 runtime_messages 时误伤 generated_api_messages，破坏工具卡片回放或 assistant 消息重放链 | P1 | 红线：snip/摘要只作用于"发送视图"副本，持久化结构原样；补"压缩后 save→reload→再发送"round-trip 测试 |
| R4 | **取消原语替换引入死锁/丢取消**：watch 替换轮询后 select! 分支语义变化 | P1 | host trait 不变，仅 state.rs/commands.rs 内部改；保留既有取消集成测试（流中途取消保文本、工具轮取消补 record） |
| R5 | **设置持久化兼容**：注册表化引入新 settings 字段或改变 ChatNativeToolsConfig 语义，老 settings.json 升级失败 | P0/P2 | 新字段一律 `#[serde(default)]`；sanitize_settings 补迁移测试；测试时不清除用户 providers/API key（项目 memory 红线） |
| R6 | **MCP 持久连接的进程泄漏与状态陈旧**：常驻子进程随 app 退出未杀、配置变更后旧 session 未重建 | P2.a | kill_on_drop(true) + 退出钩子 disconnect_all；config_fingerprint 变更即重建；保留 client.rs 假服务器测试并补 reconnect 用例 |
| R7 | **edit_file CRLF 修复改变既有匹配行为**：归一化使原本"恰好匹配"的混合行尾用例行为变化 | P1 | 补 LF/CRLF/混合三组单测；写回侧维持 atomic_write_text 既有还原逻辑 |
| R8 | **subagent 烧 key/配额**：并发子 agent 加速触发 provider 限流，与多 key failover 冷却互相放大 | P3 | Semaphore(3) + depth≤3 + 子 agent 工具表剔除 Agent 自身；usage 贯通（P1）先行使消耗可见 |
| R9 | **工具 schema 变更影响弱模型**：行号/diff/description 升级改变模型可见文本，部分 OpenAI 兼容小模型调用错误率波动 | P1/P2 | 逐项独立提交可单独回滚；schema 保持 additionalProperties 等严格性不放松 |
| R10 | **重构与功能改动交织**：P0 机械重构期间夹带行为修改导致 diff 不可审 | 全程 | 纪律：每个 PR 要么纯重构要么纯功能；重构 PR 以"cargo test 零修改通过"为准入 |

**统一回归基线**（每阶段交付必跑）：
1. `cargo test --manifest-path src-tauri/Cargo.toml` 全绿；
2. `npm run typecheck` + `npm run lint`；
3. 手工冒烟脚本（固定清单）：发一条触发 read_file+edit_file 的消息 → 观察流式 reasoning/text/工具卡片 → 触发审批并拒绝一次 → 中途取消一次 → 验证 todo 指示器 → 验证设置页 provider/模型/MCP 配置原样保留 → 重启 app 验证会话与 settings 加载。
