# Refactor Kivio Agent Architecture (based on clawspring)

> 状态：已定稿（2026-06-12 用户拍板全部开放问题）
> 关联文档：`research/00-architecture-proposal.md`（总体架构方案）、`research/01-07`（子系统研究）

## 背景

对照参考实现 clawspring 完成了 7 份子系统精读（核心循环、内置工具、skill、MCP、multi-agent、task、memory）。结论一致：kivio 的单次执行质量（多层错误恢复、细粒度取消、审批流、路径安全、原子写、前端事件联动）全面领先参考实现；结构性短板集中在四点：

1. `run_agent_loop` 单函数 790 行，6 处重复 fallback 块，改一处需同步多处；
2. 工具体系无统一注册表——新增一个内置工具要同步改 4-5 份硬编码名单（types.rs 构造器、list_native_builtin_tool_defs、call_native_tool 17 臂 match、BUILTIN_NAMES、parallel 白名单）；
3. 循环内无上下文压缩，长工具链任务必然爆窗口报错而非自愈；MCP 每次工具调用重建连接（spawn→握手→杀进程），npx 型服务器每次秒级冷启动；
4. 缺失 multi-agent 能力，而 host/executor trait 抽象已使其只差一层薄壳。

本任务把 clawspring 的四个结构思想（骨架瘦循环、统一注册表、压缩进循环、子 agent = 新 state + 旧循环）移植进 kivio，**保留 kivio 全部既有鲁棒性资产，不重写**。

## 目标

- 主循环拆分为 <150 行骨架 + planning/rounds/synthesis/finalize 单职责模块，6 处 fallback 收敛为一个 RunResultBuilder。
- 建立统一工具注册表：native/skill/mcp/task（以及后续 subagent）工具在一个 registry 下统一 schema 汇集、分发、并行/审批元数据；新增工具只需"一个实现文件 + 一条注册表条目"。
- 循环内上下文治理：廉价 snip（旧工具输出头 1/2+尾 1/4 截断）+ 轮内超限摘要压缩，长 agent 任务自愈不报错。
- 修复 bug 级工具缺陷（edit_file CRLF 匹配、截断只保头部等），补齐模型人体工学（行号、diff 回显、grep）。
- 后续阶段：MCP 持久连接、skill slash 触发、subagent、task/memory 进阶。

### 非目标

- 不重写 provider 层（chat/model/ 已达标）、不引入 Node sidecar 或外部 agent 框架。
- 不照搬 clawspring 的弱项：无路径安全、generator 审批、KeyboardInterrupt 取消、未实施的工具过滤、旧版 SSE 协议。
- 不改变前端事件契约（chat-stream/chat-tool/chat-context/chat-todo payload 字段）与 settings.json/会话 JSON 兼容性。
- MVP 不包含 multi-agent、task 依赖图、memory 固化（见分阶段范围）。
- 不做 ModelMessage 中立格式货币化（列为 P4 可选项，单独评估）。

## 分阶段范围

### MVP = P0：基础设施（行为零变更的结构重构）

1. **回归测试先行**：6 个 fallback 场景（流式失败/取消/空输出、非流式失败/空输出、tool_planning_failed）补成显式集成测试；既有 ~840 行测试迁入 loop_tests.rs。
2. **拆分 run_agent_loop**：loop_.rs 骨架 + planning.rs / rounds.rs / synthesis.rs / finalize.rs。约束：AgentHost/ToolExecutor trait 签名、commands.rs 调用点、前端事件 payload 三不动。
3. **统一工具注册表**：`&'static [NativeToolDef]` 静态表（def/enabled/parallel_safe/bypasses_approval/call 五元组），收敛五份硬编码名单；registry 统一分发 native/skill/mixer/mcp 四源；MCP 工具列表 TTL 缓存机制保留。

### P1：上下文治理 + 工具修复

compaction.rs（snip + 轮内摘要压缩，只动发送视图不动持久化）；usage/成本贯通到前端；watch 取消原语替代 100ms 轮询；edit_file CRLF 归一匹配、read_file 行号、write/edit 回传裁剪 diff、头+尾截断、错误信息修复指南化；grep 工具。

### P2：连接生命周期 + skill/task 增量（三线并行）

MCP 持久连接管理器（pending 表、stderr 尾部缓存、探活重连、**空闲超时自动回收**——默认 10 分钟未用断开子进程、下次调用透明重连、app 退出杀全部进程）+ image artifacts + 状态面板；skill 注册表 run 级缓存 + slash 触发 + `$ARGUMENTS` 替换；**task 系统全量升级**（用户拍板一步到位）：四态状态机（pending/in_progress/completed/cancelled）+ deleted 伪状态、subject/description 分离、blocks/blocked_by 双向依赖边（写侧自动同步反向边）、owner 字段、project 级作用域持久化（复用 projects.json 模式）、字段级变更回执；保留 kivio 既有优势（严格 JSON Schema、系统提示注入、chat-todo 事件、防覆盖重读）。

### P3：Multi-agent

runner.rs 无 UI 运行入口 + SubAgentHost（depth 防护、敏感工具直拒、级联取消）→ SubAgentManager + Agent/CheckAgentResult/ListAgentTasks 工具 → AgentDefinition 三层加载（工具过滤真正生效）→ 后台执行；**前端为 tool card 嵌套实时进度**（用户拍板）：子 agent 流式 delta 降采样转发，父消息 tool card 内折叠展示子 agent 进度与工具调用，复用既有 segments 协议（事件 payload 增加 parent_run_id/depth 标识子流）；skill fork 模式并入此阶段实现。task 的 owner/依赖边在本阶段接通 subagent 认领场景（P2 已落数据模型）。

### P4：Task/Memory 进阶

task 用户侧操作入口（设置页/面板查看与编辑 project 级任务）；memory 会话末固化钩子（默认关、仅手动触发）、memory_search、project scope。

## 验收标准

### MVP（P0）

- [ ] `cargo test --manifest-path src-tauri/Cargo.toml` 全绿，且新增 6 个 fallback 场景集成测试在拆分前后均通过。
- [ ] `npm run typecheck`、`npm run lint` 通过。
- [ ] loop_.rs 骨架函数 ≤200 行；BUILTIN_NAMES / parallel 白名单 / bypasses_approval 名单 / call_native_tool match / list_native_builtin_tool_defs 五处收敛为单一注册表（grep 验证旧名单已删除）。
- [ ] 手工冒烟：① 含 read_file+edit_file 的对话正常流式输出（reasoning 折叠、工具卡片、diff 渲染）；② 审批弹窗出现并可拒绝；③ 中途取消保留已生成文本且工具 record 补齐；④ todo 指示器正常；⑤ 设置页 provider/API key/MCP/skill 配置原样保留（不得清除用户配置）；⑥ 重启后会话与设置正常加载。

### 后续阶段（摘要）

- P1：构造长工具链任务，观察 chat-context 占用条在压缩后回落且不报超限错误；CRLF 文件 edit_file 三组单测（LF/CRLF/混合）通过；消息卡片显示真实 token 用量。
- P2：同一 MCP 服务器连续 10 次调用仅 1 次握手；空闲超时（默认 10 分钟）后子进程被回收、下次调用透明重连；kill 服务器进程后下次调用透明重连；app 退出无孤儿进程；`/skill名 参数` 触发成功；task——四态状态机 + 依赖边写侧自动同步反向边单测通过、cancelled/deleted 前端渲染、换对话后 project 级任务仍在。
- P3：spawn 子 agent 同步返回结果且 tool card 内可见嵌套实时进度（折叠/展开）；depth≥3 被拒；父取消级联子取消；子 agent 工具表不含 Agent 自身；子 agent 认领 task 后 owner 字段更新。

## 决策记录（ADR-lite，2026-06-12 用户拍板）

1. **PR 切分**：P0 拆 3 个 PR（测试先行 / 循环拆分 / 注册表），纯重构与功能严格分离。
2. **subagent 前端形态**：tool card 嵌套实时进度（接受 P3 工期 +3 天，体验优先）。
3. **task 系统**：P2 直接做全量 task 系统（四态 + 依赖边 + owner + project 级作用域），不走渐进式；弱模型 schema 出错率风险通过严格 JSON Schema + 字段级回执缓解。owner/依赖的 subagent 消费方在 P3 接通。
4. **MCP 连接回收**：空闲超时自动回收（默认 10 分钟，可后续做成设置项）。
5. **memory 自动固化**：默认关、仅手动触发（用户未反对推荐项）。

## 技术备注

- 重构红线：测试/调试不得清除 settings 或 providers（用户 API key 不可丢失）。
- 每个 PR 要么纯重构（cargo test 零修改通过为准入）要么纯功能，不混合。
- 风险清单与回归基线详见 `research/00-architecture-proposal.md` §5。
