# 输入框工具栏可见 mode 胶囊（含切换入口）

## Goal
当前 agent mode（Act/Plan/Orchestrate）只用输入框边框色区分，**无文字标签、无可见切换入口**（只能 /命令 或 Shift+Tab）。在输入框底部工具栏加一个**可见 mode 胶囊**：常驻显示当前模式（图标+文字），点击=切换（下拉菜单选 3 个模式）。解决"看不出当前模式 + 切换不可发现"两个痛点。边框色保留作辅助。

## Requirements
- 输入框底部那行（与模型选择器/「进入项目」同排，src/chat/InputBar.tsx ~line 1569 区域）加一个 mode 胶囊按钮。
- 显示当前模式 icon+label：Act / Plan（ListChecks）/ Orchestrate（Network）；Act 给个轻图标（如 Zap 或 Sparkles，自选）。颜色随模式：Act=neutral、Plan=其现有色、Orchestrate=violet（和边框色呼应）。
- 点击展开下拉菜单（**复用现有 project 菜单的 toggle/open 模式** toggleProjectMenu/projectMenuOpen 那套样式），列 3 个模式 + 一句简短说明（复用 slash 命令里的描述文案：plan=Enter plan mode、orchestrate=Enter orchestrate mode (proactive sub-agents)、act=普通模式），选中即调 `setAgentPlanMode(mode)`（已存在）。当前模式打勾/高亮。
- 保留现有 Shift+Tab 循环 + /plan /orchestrate 命令 + 边框色。
- 中英双语（跟现有 UI 文案风格一致）。

## Acceptance Criteria
- [ ] 工具栏可见当前 mode（图标+文字），切换对话/重开后与持久化的 mode 一致。
- [ ] 点胶囊出下拉，选模式即切换并持久化（走 onAgentPlanModeChange）。
- [ ] Shift+Tab、/命令、边框色仍工作。
- [ ] typecheck + lint 全绿。

## Out of Scope
- 暂不做"每条 assistant 消息打 mode 标签"（可选后续）。
- 不改后端 mode 逻辑（已就绪：AgentPlanMode 三态 + chat_set_agent_plan_mode）。

## Technical Notes
- InputBar.tsx：`agentPlanMode`(399)、`setAgentPlanMode`(656)、`toggleAgentPlanMode`(669)、project 菜单模式 `toggleProjectMenu/projectMenuOpen`(~1571) 可复用为下拉模板；图标 ListChecks/Network 已 import。
- 模式→icon 映射已有雏形：line 243-246 的 slash icon switch。
