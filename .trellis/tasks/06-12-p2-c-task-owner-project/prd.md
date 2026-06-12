# P2-C：Task 系统增强（四态 / 依赖边 / owner / 删除）

> 状态：实现中（已交付数据模型 + 前端；project 级共享已废弃）
> 关联：archive/2026-06/06-12-refactor-kivio-agent-architecture-based-on-clawspring（P2-C 来源）、研究文档 06-task-management.md

## 设计修订（2026-06-13 用户复核后推翻）

**原计划的"project 级共享持久化"已废弃。** 实测发现：todo 是某次对话/任务的 agent 工作状态，跨同一 project 的所有对话共享会让无关对话互相串扰——用户在 project 下另开一个对话问别的事，却看到上一个对话的 todo，是错的。**正确模型是 per-conversation 隔离**：每个对话维护自己的 todo，存在自己的 Conversation 上、重开仍在、切换对话各看各的。这本就满足"换对话不丢"（只是看到的是当前对话自己的列表）。

因此 project 路由层（resolve/with_resolved_todo/ChatProject.agent_todo_state）整个撤掉，保留与作用域无关的**数据模型增强**与**前端渲染**。

## 已交付范围（对话级）

1. **四态 + 删除**：`AgentTodoStatus` 加 `Cancelled`（不参与单 in_progress 不变量）；`todo_update` 支持 `delete:true` 删除条目。
2. **subject/description 分离**：`content` 保留为一行 subject，新增可选 `description`。
3. **依赖边（仅数据模型 + 写侧同步）**：`blocks`/`blocked_by`，写侧自动同步反向边、丢弃自指/无效/重复边、删除时清理对端边；不强约束执行。
4. **owner 字段**：P3 subagent 认领预留，本期不接消费方。
5. **字段级变更回执**：工具结果带 `changed`。
6. **前端**：todo 面板渲染 cancelled（Skip/划线）、description、blocked-by。

### 非目标
- ~~project 级共享持久化~~（已废弃，改为 per-conversation 隔离）。
- 依赖边执行约束、owner 消费方 → P3。
- 用户侧编辑入口 → P4。
- 不重命名 todo→task 线协议。

## 验收标准
- [x] `cargo test` 全绿（341）；新增单测：四态/删除/反向边同步/无效边丢弃/description+owner round-trip+清除/老 JSON serde 兼容。
- [x] `npm run typecheck`/`lint`/`test` 通过；前端渲染 cancelled/description/blocked-by。
- [ ] 手工冒烟：① 对话 A 建 todo，新开对话 B 看不到 A 的 todo（隔离）；切回 A 仍在；② cancelled/delete 前端实时更新；③ 设置页 provider/API key 原样。
- [x] spec 更新：agent-runtime.md「Agent Todo Runtime State」标注 per-conversation 隔离 + 四态/依赖边/owner/删除/changed 回执。

## 数据模型
```rust
pub enum AgentTodoStatus { Pending, InProgress, Completed, Cancelled }
pub struct AgentTodoItem {
    pub id: String,
    pub content: String,                 // 一行 subject（保留名向后兼容）
    #[serde(default)] pub description: Option<String>,
    #[serde(default)] pub status: AgentTodoStatus,
    #[serde(default)] pub blocks: Vec<String>,
    #[serde(default)] pub blocked_by: Vec<String>,
    #[serde(default)] pub owner: Option<String>,
}
```
全部新字段 serde default → 老对话 JSON 零破坏。

## 技术备注
- 红线：测试/调试不得清除 settings / providers。
- 保留全部既有 todo 不变量（单 in_progress、严格 schema、防覆盖重读、chat-todo 只读契约）。

