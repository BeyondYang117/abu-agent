# Orchestrate Mode — 第三种 agent 模式（主动用子 agent）

## Goal
在现有 Act / Plan 两种 mode 之外加第三种 **Orchestrate** 模式：模型主动 fan-out 派子 agent、先规划再分派、更高自主预算。同时**删除** `chat_tools.sub_agents` 设置开关——子 agent 工具改由模式控制：Act（被动可用）/ Orchestrate（主动）/ Plan（不可用）。

## 决策（已与用户敲定）
- 名字 **Orchestrate**；默认仍 Act。
- **删除 `chat_tools.sub_agents` 设置 + 设置页开关**（用模式取代）。
- 子 agent 工具暴露：**Act + Orchestrate 都暴露**（被动/主动），**Plan 不暴露**（只读、spawn 是副作用）。
- Orchestrate MVP = 三层：①主动 fan-out 委派（prompt）②先规划再分派（prompt，复用 todo/owner：todo_write 列计划 → 委派子 agent 设 owner → 子返回标 completed → 汇总）③更高自主预算（抬高 max_tool_rounds，取 max(用户配置, 常量~40)，不完全 unlimited）。
- 护栏不变：Semaphore(3)、depth≤3、子 agent 内敏感工具自动拒。

## Requirements
1. `AgentPlanMode` 加 `Orchestrate`（types.rs，Rust + 前端 TS 同步）；为减 churn 保留现有命名（AgentPlanMode/AgentPlanState/chat_set_agent_plan_mode/chat-plan 事件），仅加第三值。
2. `chat/plan.rs::mode_from_str` 接受 `"orchestrate"`；新增 `is_orchestrate_mode` 辅助；`with_mode` 兼容。
3. 子 agent 工具 gating（commands.rs:1173）：从 `settings.chat_tools.sub_agents && supports_tools && !plan_mode` 改为 `supports_tools && !is_plan_mode`（Act+Orchestrate 暴露，Plan 不暴露）。
4. Orchestrate system-prompt 段：mode==Orchestrate 时注入"主动委派 + 先规划(todo)再分派(owner)再汇总"的引导；并入 `compute_context_state`（commands.rs:2570）保证 token 估算一致（对齐 plan 段做法）。建议放进 `chat/plan.rs::format_prompt`（按 mode 产出不同文案：plan / orchestrate / act-空）或新增 orchestrate 段函数。
5. **更高自主预算**：mode==Orchestrate 时把 `effective_chat_tools.max_tool_rounds` 抬到 `max(configured, ORCHESTRATE_MIN_TOOL_ROUNDS≈40)`。
6. 删除 `chat_tools.sub_agents`：settings.rs（删字段 + Default；serde 对旧 json 多余字段默认忽略，无需迁移）、SettingsShell.tsx（删开关 UI + default + updateChatTools 调用）、api/tauri.ts（删 `subAgents` 类型 + default）。
7. 前端：InputBar mode 切换器加第三档「Orchestrate」；`chat_set_agent_plan_mode` 命令 + 前端调用接受 `orchestrate`；AgentPlanMode TS 类型加 `'orchestrate'`。
8. Plan-mode 工具过滤不变（仍过滤副作用工具，含 agent spawn）。

## Acceptance Criteria
- [ ] InputBar 能切到 Orchestrate；切换持久化到 conversation.agent_plan_state.mode，重开仍在。
- [ ] Orchestrate 下模型**主动**派子 agent（无需用户明说）；Act 下子 agent 工具在但模型不主动用。
- [ ] Plan 下无 agent spawn 工具（被过滤）。
- [ ] `chat_tools.sub_agents` 设置 + 设置页开关已删除；旧 settings.json 不报错。
- [ ] Orchestrate 注入的 prompt 段同时出现在请求构造与 compute_context_state 的 token 估算里。
- [ ] Orchestrate 下 max_tool_rounds 被抬高。
- [ ] cargo test + typecheck + lint 全绿；spec 更新。

## Out of Scope
- 不改 Semaphore(3) 并发上限 / depth 上限。
- 不做更激进的自主（无人值守长跑、自动重试规划等）。
- 不重命名 AgentPlanMode→AgentMode（保留命名减 churn）。

## Technical Notes（锚点）
- mode: `src-tauri/src/chat/types.rs::AgentPlanMode{Act,Plan}`；`src-tauri/src/chat/plan.rs`（mode_from_str/is_plan_mode/with_mode/format_prompt）。
- gating + prompt 组装: `commands.rs:1173`(sub_agents gate)、`:1185/:1201/:1231`(agent_plan_prompt)、`:2570`(compute_context_state)、`:3053`(apply_agent_plan_tool_filter)、`:506`(chat_set_agent_plan_mode)。
- 子 agent append: `chat/sub_agent.rs::append_tool_definitions(&mut tools, allow_spawn=true)`。
- 要删的 sub_agents 引用：settings.rs:772/791、SettingsShell.tsx:294/3200-3206、api/tauri.ts:452/816、commands.rs:1173。（state.rs/main.rs 的 `sub_agents` 是 SubAgentManager 字段，**不要删**。）
- 前端：InputBar mode 切换器、`AgentPlanMode` TS 类型、`api.chatSetAgentPlanMode`。
