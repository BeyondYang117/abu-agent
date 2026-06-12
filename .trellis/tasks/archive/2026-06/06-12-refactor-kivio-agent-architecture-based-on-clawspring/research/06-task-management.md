# 06 — Task 管理子系统研究（clawspring vs kivio）

> 研究范围：任务存储、任务工具（TaskCreate/Update/Get/List vs todo_write/todo_update）、依赖关系、与 agent 循环 / 前端 UI 的联动。
> 引用格式：`文件:行号`。clawspring 根目录 = `/Users/zmair/ZM database/keylingo/clawspring`，kivio 根目录 = `/Users/zmair/ZM database/keylingo/keylingo`。

---

## 1. clawspring 设计精读

### 1.1 模块布局

`task/` 目录共 4 个文件，职责切分非常干净：

| 文件 | 行数 | 职责 |
|---|---|---|
| `task/types.py` | 92 | 数据模型（`Task` dataclass、`TaskStatus` 枚举、序列化、单行渲染） |
| `task/store.py` | 199 | 线程安全的内存 + JSON 文件持久化 store |
| `task/tools.py` | 265 | 4 个工具的 JSON schema、实现函数、注册到 tool_registry |
| `task/__init__.py` | 12 | re-export 公共 API |

### 1.2 数据模型（types.py）

`TaskStatus`（`task/types.py:10-14`）是四态状态机：`pending / in_progress / completed / cancelled`（工具层额外接受伪状态 `deleted`，见 1.4）。

`Task` dataclass（`task/types.py:20-32`）字段：

```python
id: str                      # 短的递增数字 ID（"1", "2", ...）
subject: str                 # 标题
description: str             # 详细描述（与标题分离）
status: TaskStatus
active_form: str             # in_progress 时的进行时标签，如 "Running tests"
owner: str                   # 负责的 agent/用户（为多 agent 协作预留）
blocks: list[str]            # 本任务阻塞的任务 ID（正向边）
blocked_by: list[str]        # 阻塞本任务的任务 ID（反向边）
metadata: dict[str, Any]     # 任意扩展元数据
created_at / updated_at: str # ISO 时间戳
```

设计要点：

- **双向依赖边**：`blocks` 与 `blocked_by` 同时存储，是一张显式 DAG。
- **`from_dict` 容错**（`task/types.py:53-57`）：非法 status 静默降级为 `PENDING`，旧数据永远能加载。
- **"pending blocker" 渲染逻辑**（`task/types.py:82-92`）：`one_line()` 接收一个 `resolved_ids` 集合，只显示**尚未完成**的 blocker——已完成的依赖在列表视图自动消失。这是个巧妙的设计：依赖边永久保留（可追溯），但展示层只暴露仍然生效的阻塞，模型读 `TaskList` 输出时天然知道"哪个任务现在可以做"。
- **状态图标**（`task/types.py:74-80`）：`○ ● ✓ ✗` 直接编码进工具文本输出，给模型提供视觉密度高的状态扫描。

### 1.3 存储层（store.py）

- **存储位置**：`<cwd>/.clawspring/tasks.json`（`task/store.py:24-25`）——**按项目目录持久化、跨会话存活**，与对话历史解耦。
- **并发模型**：模块级 `threading.Lock`（`task/store.py:13`）+ 进程内 dict 缓存 `_tasks` + 懒加载标志 `_loaded`（`task/store.py:18-19`、`_load` 在 `store.py:28-41`）。每次变更全量回写 JSON（`_save`，`store.py:44-48`）。
- **短递增 ID**：`_next_id`（`task/store.py:51-56`）取现存最大数字 ID +1。模型引用 `#3` 比 UUID 友好得多（省 token、不易抄错）。
- **`update_task` 是核心**（`task/store.py:93-172`）：
  - 返回 `(task, updated_fields)` 二元组——调用方能告诉模型"实际改了哪些字段"，没有变化时返回空列表（幂等反馈，`tools.py:167-168` 据此回 "no changes"）。
  - **双向边自动同步**（`store.py:146-166`）：`add_blocks` 时自动在目标任务上补 `blocked_by` 反向边，反之亦然。模型只需声明单向关系，图的一致性由 store 保证。
  - **metadata 合并语义**（`store.py:138-144`）：传入的 key 合并进现有 metadata，value 为 `None` 时删除该 key——部分更新而非整体替换。
  - 只有真正发生变化才更新 `updated_at` 并落盘（`store.py:168-170`）。

### 1.4 工具层（tools.py）

四个工具：`TaskCreate` / `TaskUpdate` / `TaskGet` / `TaskList`，schema 与实现分离（schema 在 `tools.py:11-126`，实现在 `tools.py:131-210`）。

- **删除复用 TaskUpdate**：`status="deleted"` 是 schema enum 里的伪状态（`tools.py:70`），实现层拦截后调 `delete_task`（`tools.py:148-152`）——少注册一个工具，减少模型的选择负担。
- **工具输出全部是给模型读的紧凑纯文本**：`_task_list`（`tools.py:196-210`）每任务一行，含状态图标、owner、未解决的 blocker；`_task_get`（`tools.py:172-193`）输出对齐的字段卡片。无 JSON 包裹，token 高效。
- **注册机制**（`tools.py:215-265`）：构造 `ToolDef`（定义见 `tool_registry.py:13-27`），声明 `read_only`（Get/List 为 True）与 `concurrent_safe=True`（4 个全部并发安全，因为 store 自带锁）。模块 import 即自注册（`tools.py:265` 顶层调用 `_register()`），由 `tools.py:1082-1083`（根目录）的 `import task.tools` 触发。

### 1.5 与 agent 循环 / 系统提示的联动

- **系统提示引导**（`context.py:56-64`）：在 "Task Management & Background Jobs" 小节逐一列出 4 个工具，并给出一句话工作流（`context.py:64`）：*"Break multi-step plans into tasks at the start → mark in_progress when starting each → mark completed when done → use TaskList to review remaining work."* 这是唯一的"循环联动"——clawspring 的 agent 主循环（`agent.py:55-150`）对 task 完全无感知，task 只是注册表里的普通工具。
- **proactive watcher**（`clawspring.py:345-366`）：后台 idle 守护线程在静默超时后注入自动 prompt，提示模型"检查是否有 pending tasks 要执行"——这是任务系统与自治执行之间的弱耦合桥。
- **REPL 斜杠命令** `/tasks`（`clawspring.py:1554-1640`）：用户可在终端直接 list/create/start/done/cancel/delete/get/clear，与模型共享同一个 store——**人和模型操作同一份任务数据**。

### 1.6 与 memory/ 的持久化模式对比（顺带）

memory 子系统用的是**另一套**模式：每条记忆一个 `.md` 文件 + 自动重建的 `MEMORY.md` 索引（`memory/store.py:1-26`），有 user/project 双 scope、confidence 分数、来源标记（`memory/store.py:46-75`），并有 AI 驱动的会话末 consolidator（`memory/consolidator.py:1-12`，每会话上限 3 条、0.8 起始置信度）。task 与 memory **不共享持久化代码**，但共享同一设计哲学：纯文件存储（`.clawspring/` 目录）、人类可读、跨会话、与对话记录解耦。

---

## 2. kivio 现状

### 2.1 数据模型（types.rs）

- `AgentTodoStatus`（`src-tauri/src/chat/types.rs:64-70`）：三态 `pending / in_progress / completed`，**无 cancelled**。
- `AgentTodoItem`（`types.rs:78-84`）：仅 `id`（字符串，模型自取）、`content`（≤240 字符）、`status` 三个字段。**无 description、owner、依赖边、metadata、时间戳**。
- `AgentTodoState`（`types.rs:86-91`）：`items: Vec<AgentTodoItem>` + 整表级 `updated_at: i64`。
- **挂在 Conversation 上**：`Conversation.agent_todo_state`（`types.rs:289`，`#[serde(default)]` 保证旧对话 JSON 兼容，有测试 `todo.rs:331-346`）。持久化即对话文件本身（`{app_data_dir}/conversations/{id}.json`，`storage.rs:182`、`save_conversation` 在 `storage.rs:279`）。**作用域是单对话，跨对话/跨项目不可见**。

另有独立的 `AgentPlanState`（`types.rs:122-130`：mode=act/plan、status=empty/draft/approved、plan 文本）——plan 是一段自由文本草稿而非结构化任务，与 todo 互不相通（`plan.rs:36-47` 的 `capture_draft_from_reply` 只是把 plan 模式下的最终回复全文存为 plan）。

### 2.2 工具层（todo.rs，381 行）

两个工具：`todo_write`（整表替换，`todo.rs:47-75`）与 `todo_update`（按 id 改单条 content/status，`todo.rs:77-112`）。

kivio 已经做对、甚至比 clawspring 更好的部分：

- **严格 JSON Schema 校验**：`input_schema` 带 `maxItems:50`、`minLength/maxLength`、`additionalProperties:false`（`todo.rs:55-66, 85-103`），还提供 `output_schema`（`todo.rs:301-323`）——clawspring 的 schema 远没这么严。
- **单 in_progress 不变量**：`normalized_state`（`todo.rs:196-242`）强制最多一个 in_progress；`todo_update` 把新激活项设为优先（`todo.rs:157-159`），其余自动降级为 pending，并有单测覆盖（`todo.rs:349-380`）。clawspring 完全没有这个约束。
- **id 去重、空值校验**（`todo.rs:200-219`），错误信息精确到条目。
- **structured_content 双通道结果**（`tool_result`，`todo.rs:185-194`）：文本给模型，`{"todoState": ...}` 结构化给前端渲染。
- **双语 system prompt 注入**（`format_prompt`，`todo.rs:162-183`）：每次请求把当前 todo 状态 + 维护守则注入系统提示（调用点 `commands.rs:1136-1140`，经 `prepare.rs:380-384` 的 `append_context_segment` 进入 `agent_todo` 上下文段）。**这等价于 clawspring 的 TaskList 读工具——模型每轮都"免费"看到全量状态，无需主动调用读工具**。
- **工具执行路径短路 + 免审批**：`RegistryToolExecutor::call` 在进 MCP registry 之前拦截 todo 工具（`commands.rs:3431-3442`）：load conversation → `todo::apply_tool` → save → `emit_chat_todo_state`。`builtin_tool_bypasses_approval`（`prepare.rs:204-217`）让 todo 工具跳过用户审批；plan 模式下 todo 工具仍被放行（`agent_plan_allows_tool`，`commands.rs:2980-2998`）。
- **并发安全的状态合并**：agent run 结束保存回复前 `merge_latest_agent_todo_state`（`commands.rs:1245`、定义 `commands.rs:1802-1811`）重新从磁盘读最新 todo state，防止 run 期间工具写入被内存里的旧 conversation 副本覆盖。
- **前端实时联动**：Rust 侧 `emit_chat_todo_state` 发 `chat-todo` 事件（`commands.rs:2880-2888`）→ React 侧 `api.onChatTodo` 监听（`src/chat/Chat.tsx:1145-1171`）→ `patchAgentTodoState`（`Chat.tsx:509-514`）→ 标题栏 `AgentTodoIndicator`（`Chat.tsx:2327`，组件 `src/chat/AgentTodoIndicator.tsx`，144 行，含 N/M 进度、当前项、弹出全列表）；`ToolCallBlock.tsx:130-199` 还能从工具结果的 structuredContent 内联渲染 todo 快照。clawspring 是纯 TUI，没有任何等价物。

### 2.3 与 agent 循环的关系

与 clawspring 相同的架构选择：`run_agent_loop`（`src-tauri/src/chat/agent/loop_.rs:190`）对 todo 无感知，todo 只是 executor 拦截的一类工具 + prompt 的一个上下文段。工具注册经 `append_agent_todo_tools`（`commands.rs:2940-2948` → `todo.rs:32-41` 去重追加），provider 不支持工具时 prompt 明确降级提示（`todo.rs:167-177`）。`prepare.rs:179-180` 把 `todo_write/todo_update` 列入 builtin 名单用于"工具未启用"的纠错回复。

### 2.4 kivio 缺的部分

- 模型**不能删除单条** todo（只能 `todo_write` 全表重写实现等效删除）、不能标记 cancelled。
- 条目无 description——`content` 240 字符上限，复杂任务的上下文装不下，模型在长 run 中可能忘记条目的具体含义。
- **无依赖关系**：纯顺序列表，模型无法表达"任务 C 等 A、B 完成后才能做"，并行工具调用场景下无法推理可执行集。
- **无 owner**：kivio 目前没有 subagent（全仓 grep `subagent|sub_agent|SubAgent` 无命中），owner 暂无消费者。
- **无跨对话/项目级任务**：换一个对话，todo 清零；clawspring 的任务按项目目录跨会话存活。
- 无用户侧操作入口（clawspring 有 `/tasks` 斜杠命令；kivio 的 UI 是只读 indicator，prompt 里明确"用户不能编辑"，`todo.rs:171`）。
- 无 metadata 扩展位、无按条目时间戳。

---

## 3. 差距分析

### clawspring 有、kivio 缺失/粗糙

| # | 能力 | clawspring 出处 | kivio 现状 |
|---|---|---|---|
| G1 | 四态状态机（含 cancelled）+ deleted 伪状态删除 | `task/types.py:10-14`、`task/tools.py:148-152` | 三态，无取消/删除单条（`types.rs:64-70`） |
| G2 | subject/description 分离 | `task/types.py:23-24` | 仅 content ≤240 字符（`types.rs:78-84`） |
| G3 | 双向依赖边 + 自动反向同步 + pending-blocker 渲染 | `task/store.py:146-166`、`types.py:82-92` | 完全没有 |
| G4 | owner 字段（多 agent 认领） | `task/types.py:27` | 没有（但 kivio 也没有 subagent，暂无消费者） |
| G5 | metadata 任意扩展 + null 删 key 合并语义 | `task/store.py:138-144` | 没有 |
| G6 | 跨会话、项目级持久化（`.clawspring/tasks.json` 按 cwd） | `task/store.py:24-25` | todo 锁死在单 Conversation JSON 里（`types.rs:289`） |
| G7 | 用户与模型共操作同一 store（`/tasks` REPL 命令） | `clawspring.py:1554-1640` | UI 只读 |
| G8 | `updated_fields` 幂等反馈（"no changes"） | `task/store.py:104-172`、`tools.py:167-168` | apply 后整表返回，无字段级 diff |
| G9 | idle watcher 提示模型继续 pending tasks | `clawspring.py:345-366` | 无等价机制 |

### kivio 有、clawspring 没有

| # | 能力 | kivio 出处 |
|---|---|---|
| K1 | 单 in_progress 不变量 + 归一化 + 单测 | `todo.rs:196-242, 349-380` |
| K2 | 严格 input/output JSON Schema（maxItems、长度上限、additionalProperties:false） | `todo.rs:55-66, 271-323` |
| K3 | 状态每轮注入系统提示（免去 TaskList 调用开销），双语、工具不可用降级 | `todo.rs:162-183`、`prepare.rs:380-384` |
| K4 | 前端实时事件联动（`chat-todo` 事件 → 标题栏指示器 → 弹出列表 → 工具卡内联快照） | `commands.rs:2880-2888`、`Chat.tsx:509,1159,2327`、`AgentTodoIndicator.tsx`、`ToolCallBlock.tsx:130-199` |
| K5 | 保存前重读磁盘合并最新 todo state（防写覆盖） | `commands.rs:1802-1811, 1245` |
| K6 | plan 模式工具过滤中对 todo 的精确放行 + 免审批 | `commands.rs:2980-2998`、`prepare.rs:204-217` |
| K7 | 旧数据 serde 兼容测试 | `todo.rs:331-346` |

### 核心判断：是否值得升级为带依赖的 task 系统？

**不建议照搬，建议选择性升级。**理由：

1. clawspring 的 `blocks/blocked_by` + `owner` 的真正受益场景是**多 agent 并行认领任务**。kivio 当前是单 agent 顺序循环、无 subagent，DAG 的可执行集推理没有消费者；单 in_progress 不变量（K1）本身就隐含了顺序执行模型。引入依赖图会增加 schema 复杂度（模型调用出错率上升）却换不来执行收益。
2. 但 G1（cancelled/删除）、G2（description）、G6（跨对话作用域）是**单 agent 也立刻受益**的：长 run 中条目语义丢失、用户中断后任务无法标记放弃、换对话续做工作时 todo 蒸发，都是现实痛点。
3. 若 kivio 路线图里有 subagent / 并行工具执行，再把依赖边 + owner 作为该 feature 的一部分引入（届时 kivio 的 `normalized_state` 单 in_progress 约束也要改为 per-owner）。

---

## 4. 重构建议

### P0 — todo 条目模型增强（≈1 天，纯后端 + 少量前端）

1. **加 `cancelled` 状态 + 单条删除**：
   - `types.rs:64-70` `AgentTodoStatus` 加 `Cancelled`（serde `snake_case` 自动得 `"cancelled"`，旧 JSON 兼容不受影响）。
   - `todo.rs:77-112` `todo_update` 的 `status_schema()`（`todo.rs:294-299`）enum 加 `"cancelled"`；学 clawspring 的伪状态（`tools.py:148-152`），再加 `"deleted"`——`apply_todo_update` 里拦截后从 `items` 中移除该条，省一个新工具。
   - 前端：`src/chat/types.ts:378` 联合类型加 `'cancelled'`；`AgentTodoIndicator.tsx:11-42` 的 `statusLabel/dotClass/textClass` 各加一臂；`ToolCallBlock.tsx:143` 同步。
2. **加可选 `description` 字段**：`AgentTodoItem`（`types.rs:78-84`）加 `#[serde(default)] pub description: Option<String>`；`todo_item_schema()`（`todo.rs:271-292`）加可选属性（maxLength 建议 2000）；`format_state_lines`（`todo.rs:244-261`）保持单行不变（description 不进 system prompt，避免膨胀），但 `tool_result` 的 structured_content 自动携带，前端弹出面板可展开显示。
3. **幂等反馈**：`apply_todo_update` 返回值附带 changed 字段列表（学 `task/store.py:104-172`），`tool_result` 文本首行从固定 "Todo list updated." 改为 "changed: status" / "no changes"，给模型更准的回执。

### P1 — todo 状态作用域升级到 Project（≈2-3 天，后端 + 前端 + 迁移）

- 现状 todo 绑死 Conversation（`types.rs:289`），而 kivio 已有 Project 概念（`storage.rs:169-171` 的 `projects.json`、`Conversation.project_id` 在 `types.rs:285`）。建议：当对话属于某 project 时，todo 读写重定向到 `conversations/projects/{project_id}_todo.json`（沿用 `storage.rs` 的文件持久化模式，新增 `load_project_todo/save_project_todo`，用 `std::sync::Mutex<HashMap<String, AgentTodoState>>` 或直接每次读写文件——量小，文件级即可，等价 clawspring `store.py:44-48` 的全量回写）。
- 改动点集中：`commands.rs:1136`（format_prompt 取数）、`commands.rs:3431-3442`（executor 写回）、`commands.rs:1802`（merge 函数）、`emit_chat_todo_state` 的 payload 加 `projectId`，前端 `Chat.tsx:1159` 的过滤条件从 conversationId 扩展为 conversation/project 二选一。无 project 的对话保持现行为，零迁移成本。
- Tauri 约束注意：executor 的 todo 短路路径目前每次 `load_conversation + save_conversation` 全量读写对话 JSON（`commands.rs:3432-3440`），对话很长时这是放大写；project 级 todo 文件顺带解决该问题。

### P1 — 用户侧操作入口（≈1-2 天，主要前端）

- `AgentTodoIndicator.tsx` 弹出面板加"标记完成/取消/删除"按钮 → 新增 Tauri command `chat_user_update_todo(conversation_id, id, status)`（放 `commands.rs`，复用 `todo::apply_todo_update` + `emit_chat_todo_state`）。注意同步修改 `todo.rs:171` 的 prompt 文案（删掉"用户不能编辑"），并在 todo state 里给用户改动加来源标记（metadata 或新字段），让模型知道是用户改的。对标 clawspring `/tasks`（`clawspring.py:1554-1640`）的人机共写。

### P2 — 依赖边 + owner（仅当 subagent 立项时一并做，≈3-5 天）

- 数据面照抄 clawspring：`AgentTodoItem` 加 `blocks/blocked_by: Vec<String>` + `owner: Option<String>`，在 `apply_todo_update` 中实现双向边自动同步（移植 `task/store.py:146-166` 的逻辑，Rust 侧注意同一 Vec 内借用，建议两遍循环：先收集再写）。
- `format_state_lines` 移植 pending-blocker 渲染（`task/types.py:82-92`）：只显示未完成的 blocker。
- `normalized_state` 的单 in_progress 约束改为 per-owner。
- 在此之前**不要做**：schema 变复杂会提高小模型调用错误率，kivio 支持任意 OpenAI 兼容 provider，工具 schema 的简洁是现实约束。

### P2 — plan 与 todo 打通（≈1 天）

- `plan.rs:36-47` 的 `capture_draft_from_reply` 只存纯文本计划。可在用户点击"执行计划"（`chat_execute_agent_plan`，`commands.rs:527-541`）时，在注入的下一轮 prompt 中明确引导模型"先把已批准计划拆成 todo_write 条目再执行"（改 `plan.rs:format_prompt` 的 act 分支文案即可，零结构改动），对齐 clawspring `context.py:64` 的工作流引导句。

### 总结

kivio 的 todo 在**工程质量**（schema 严格性、不变量、并发合并、前端联动）上优于 clawspring task；clawspring 优在**数据模型表达力**（四态 + description + 依赖 + owner + metadata）与**作用域**（项目级跨会话）。重构主线应是"补模型表达力 + 提升作用域"，而非重写存储或引入完整 DAG。
