# 子系统研究：Agent / 多 agent（subagent 派生、agent 类型、并行执行、结果回传、上下文隔离）

> 对比对象：
> - clawspring（参考实现，Python）：`/Users/zmair/ZM database/keylingo/clawspring`
> - kivio（待重构，Tauri v2 / Rust）：`/Users/zmair/ZM database/keylingo/keylingo`

---

## 1. clawspring 设计精读

clawspring 的多 agent 系统总共约 **800 行 Python**（`multi_agent/subagent.py` 480 行 + `multi_agent/tools.py` 295 行 + `__init__.py` 23 行；根目录 `subagent.py` 仅 11 行向后兼容 shim），却覆盖了 agent 类型定义、并发派生、后台执行、消息收件箱、git worktree 隔离五个能力。核心思路是：**子 agent 不是新的执行引擎，而是"用全新 `AgentState` 再跑一遍主循环 `agent.run()`"**。

### 1.1 数据结构

**`AgentDefinition`**（`multi_agent/subagent.py:17-25`）— agent 类型定义：

```python
@dataclass
class AgentDefinition:
    name: str
    description: str = ""
    system_prompt: str = ""   # 追加在基础 system prompt 之前
    model: str = ""           # 模型覆盖；"" = 继承父 agent
    tools: list = field(default_factory=list)   # 空 = 全部工具
    source: str = "user"      # "built-in" | "user" | "project"
```

**内置 agent 类型**（`subagent.py:30-91`）：5 个 — `general-purpose`（空 prompt）、`coder`、`reviewer`（声明只用 Read/Glob/Grep）、`researcher`（Read/Glob/Grep/WebFetch/WebSearch）、`tester`。每个就是一段几行的 system prompt 前缀。

**`SubAgentTask`**（`subagent.py:189-202`）— 生命周期跟踪：

```python
@dataclass
class SubAgentTask:
    id: str                    # uuid hex[:12]
    prompt: str
    status: str = "pending"    # pending | running | completed | failed | cancelled
    result: Optional[str] = None
    depth: int = 0
    name: str = ""             # 可被 SendMessage 寻址的人类可读名
    worktree_path: str = ""
    worktree_branch: str = ""
    _cancel_flag: bool = False
    _future: Optional[Future] = None
    _inbox: Any = queue.Queue()   # SendMessage 投递队列
```

**`SubAgentManager`**（`subagent.py:278-480`）— 进程级单例（`multi_agent/tools.py:21-26` 懒初始化），持有 `ThreadPoolExecutor(max_workers=5)`、`tasks: Dict[id→task]`、`_by_name: Dict[name→id]`，以及两个上限：`max_concurrent=5`、`max_depth=5`。

### 1.2 agent 类型的三层加载

`load_agent_definitions()`（`subagent.py:150-179`）按 **内置 → 用户级 `~/.clawspring/agents/*.md` → 项目级 `.clawspring/agents/*.md`** 顺序合并，同名后者覆盖前者。`.md` 格式是 YAML frontmatter（description/model/tools）+ 正文作 system prompt（`_parse_agent_md`，`subagent.py:96-147`），与 Claude Code 的 agents 目录约定一致。frontmatter 解析有无 yaml 依赖的降级路径（`subagent.py:123-129` 手工 `key: value` 解析）。

### 1.3 spawn 控制流（核心）

`SubAgentManager.spawn()`（`subagent.py:288-411`）：

1. 生成 task_id，注册到 `tasks` / `_by_name`（`subagent.py:312-317`）。
2. **深度防护**：`depth >= max_depth` 直接置 failed 返回（`subagent.py:319-322`），防无限递归派生。
3. **配置合成**：复制父 config；若有 `agent_def`，model 覆盖、`agent_def.system_prompt + "\n\n" + 基础 system prompt` 拼接（`subagent.py:324-332`）——专用 prompt 在前、通用在后。
4. **worktree 隔离**（可选）：`_create_worktree`（`subagent.py:219-235`）`git worktree add -b nano-agent-<hex8> <tmpdir>`，并向 prompt 末尾注入一段"你在隔离 worktree 工作、完成前请 commit"的提示（`subagent.py:349-355`）。失败路径全部置 task.failed 而非抛异常。
5. **闭包 `_run` 提交线程池**（`subagent.py:361-410`）：
   - `state = AgentState()` —— **全新空消息历史 = 上下文隔离的全部实现**（`subagent.py:369`）。
   - 调 `_agent_run(prompt, state, eff_config, eff_system, depth=depth+1, cancel_check=lambda: task._cancel_flag)`（`subagent.py:370-374`）——直接复用主循环 `agent.run()`，传入取消闭包。
   - **事件丢弃**：`for _event in gen:` 只迭代不处理（`subagent.py:375-377`）——子 agent 的流式输出对用户不可见，这是有意为之的极简设计。
   - 结果回传：`_extract_final_text(state.messages)`（`subagent.py:268-273`）从消息列表倒序取第一条 assistant 文本。
   - **收件箱排水**（`subagent.py:386-401`）：主任务跑完后循环 `task._inbox.get_nowait()`，每条消息**复用同一个 `state`** 再跑一轮 `agent.run()` —— 后台 agent 因此可以多轮对话，且保留上下文。
   - `finally` 中恢复 cwd 并 `_remove_worktree`（worktree 删除但**分支保留**供 review/merge，`subagent.py:238-253` 只在显式调用时删分支；`_run` 的 finally `subagent.py:405-408` 调 `_remove_worktree` 会删 worktree 和分支——注意：实际代码 finally 里两者都删，"commit 后留分支"靠 `git branch -D` 在分支已被 commit 引用时仍会删除，这是参考实现的一个粗糙处)。

### 1.4 主循环侧的最小耦合

`agent.py run()`（`agent.py:55-150`）为多 agent 只做了三件事：

- 签名带 `depth: int = 0, cancel_check=None`（`agent.py:60-61`）。
- 把运行时元数据注入 config：`config = {**config, "_depth": depth, "_system_prompt": system_prompt}`（`agent.py:81`）——工具函数（如 Agent 工具）从 config 读出当前深度和基础 system prompt，**避免了工具注册表对 agent 模块的循环依赖**。
- 循环顶部 `if cancel_check and cancel_check(): return`（`agent.py:84-85`）。

子 agent 还自动继承主循环的全部设施：`maybe_compact` 上下文压缩（`agent.py:90`）、provider 自动检测流式（`agent.py:93-99`）、权限闸（`agent.py:127-142`，子 agent 因事件被丢弃，`PermissionRequest.granted` 保持 False，即子 agent 内的写操作默认被拒——一个隐式安全属性）。

### 1.5 工具注册（5 个工具）

`multi_agent/tools.py` 注册 5 个工具（导入即注册，`tools.py:1054-1055` `import multi_agent.tools` 触发）：

| 工具 | 行号 | 职责 |
|---|---|---|
| `Agent` | tools.py:160-221 | spawn；参数 prompt/subagent_type/name/model/wait/isolation；`wait=True`（默认）阻塞 300s 等结果，`wait=False` 立即返回 task id 转后台 |
| `SendMessage` | tools.py:223-244 | 按 name 或 id 向运行中后台 agent 投递消息（入 `_inbox`） |
| `CheckAgentResult` | tools.py:246-262 | 查询 status/result |
| `ListAgentTasks` | tools.py:264-277 | 表格列出所有任务 |
| `ListAgentTypes` | tools.py:279-295 | 列出可用 agent 类型 |

`_agent_tool`（`tools.py:31-93`）：从 config 取 `_system_prompt`/`_depth`，**剥掉 `_` 开头私有键再传给子 agent**（`tools.py:51`），同步模式返回 `[Agent: name (type)]\n\n结果` 格式化文本，后台模式返回 task id + 使用提示。

### 1.6 REPL 联动

- `/agents` 命令（`clawspring.py:1259`）列出任务。
- `_print_background_notifications()`（`clawspring.py:1277`，在每次用户输入前调用于 `clawspring.py:3016`）：用函数属性 `_seen` 集合去重，对新完成的后台任务打印 ✓/✗ + 结果前 200 字预览——**无轮询、无回调，借 REPL 的天然节拍做结果回传**。

### 1.7 设计意图小结

1. **子 agent = 新 state + 旧循环**：零新执行引擎，多 agent 系统的"内核"只有 `spawn()` 一个函数。
2. **上下文隔离 = 新建空 `AgentState`**；文件系统隔离 = git worktree（可选增量能力）。
3. **结果回传 = 最后一条 assistant 文本**，刻意丢弃中间流式事件，把子 agent 当"函数调用"。
4. **深度上限 + 线程池上限**双保险控制资源。
5. **`_inbox` 队列**让"后台 agent"升级成"可对话的常驻 worker"，复用同一 state 实现多轮。
6. 已知粗糙处：`AgentDefinition.tools` 字段**从未被强制执行**（grep 全库，`agent_def.tools` 只在 ListAgentTypes 展示用，`subagent.py:328-332` 只用了 model 和 system_prompt）——工具限制仅靠 prompt 软约束；worktree 的 `os.chdir` 是进程级的，与线程池并发其实互相污染 cwd（Python 参考实现的妥协，Rust 重写时必须避免）。

---

## 2. kivio 现状

**结论先行：kivio 目前没有任何多 agent / subagent 能力。** 在 `src-tauri/src` 全局搜索 `subagent|sub_agent|spawn.*agent|AgentDefinition|dispatch_agent` 零命中。但 kivio 的单 agent 循环抽象程度很高，已经具备复用为子 agent 引擎的几乎全部前置条件。

### 2.1 主循环已是"宿主无关"的纯函数

`run_agent_loop`（`chat/agent/loop_.rs:190-194`）签名：

```rust
pub async fn run_agent_loop(
    mut config: AgentRunConfig<'_>,
    host: &dyn AgentHost,
    executor: &dyn ToolExecutor,
) -> Result<AgentRunResult, String>
```

这是 kivio 最大的架构资产：**循环对 UI 和工具系统都只通过 trait 说话**。

- `AgentHost` trait（`chat/agent/host.rs:10-58`）：8 个方法 — `emit_stream_delta` / `emit_stream_done` / `emit_tool_record`（事件外发）、`request_tool_approval` / `request_user_response`（人机交互）、`is_generation_active` / `wait_for_generation_inactive`（取消）。生产实现 `ChatAgentHost`（`chat/commands.rs:3315-3319` 起）把它们映射到 Tauri `app.emit("chat-stream"/"chat-tool", ...)`（`chat/commands.rs:3735/3770/3801`）。测试中已有 3 个独立 `TestHost` 实现（`loop_.rs:1900`、`stream.rs:640`、`execute.rs:613`），证明替换 host 是已验证的路径。
- `ToolExecutor` trait（`chat/agent/execute.rs:24-32`）：单方法 `call(ctx, tool, arguments, skill_cache)`。生产实现 `RegistryToolExecutor`（`chat/commands.rs:3417-3460`）分流 todo 工具与 `mcp::registry::call_tool`。
- `AgentRunConfig`（`chat/agent/types.rs:37-63`）：把 provider、model、`runtime_messages`、`tools: Vec<ChatToolDefinition>`、settings、`max_output_tokens` 等全部显式传入——**没有隐藏全局态**，意味着为子 agent 构造一份独立 config 即可获得独立上下文。
- `AgentRunResult`（`types.rs:76-85`）：`content` / `reasoning` / `tool_records` / `segments` / `api_messages` / `steps` / `stream_outcome` —— 比 clawspring 的"最后一条 assistant 文本"丰富得多，子 agent 结果回传可以直接取 `result.content`。

### 2.2 取消与并发设施已就绪

- generation 取消机制（`state.rs:174-210`）：`chat_stream_generations: Mutex<HashMap<conversation_id, u64>>`，`next_chat_generation` / `cancel_chat_generation` / `is_chat_generation_active` 按 conversation 维度工作——**天然支持多个并行运行，只要每个子 agent 用独立的 conversation_id（或派生 id）**。
- 同一轮内并行工具执行已实现：`MAX_PARALLEL_TOOL_CALLS_PER_ROUND = 4`（`loop_.rs:42`），`execute_parallel_chunk`（`loop_.rs:1219-1270`）用 `tokio::join!` 并发（值得注意：因 `&dyn` trait object 不能用 `JoinSet`，目前是手写 1/2/3/4 分支的 join，重构时可借鉴 `futures::future::join_all`）。
- 轮次上限：`tool_round_limit_reached`（`loop_.rs:982-983`），由 `settings.chat_tools.max_tool_rounds` 配置（`settings.rs:748-749`），到限注入 `step_limit_system_message()` 让模型收尾（`loop_.rs:568-573`）——比 clawspring 直接 break 更优雅。

### 2.3 已有"agent 类型"的雏形：ChatAssistant

`ChatAssistantSnapshot`（`chat/types.rs:458-487`）已包含 `system_prompt`、`provider_id`、`model`、`skill_id`、`tool_preset`、`data_connectors` —— 这就是 kivio 版的 `AgentDefinition`，且比 clawspring 强：

- `apply_assistant_tool_preset`（`chat/agent/prepare.rs:76-92`）**真正强制执行工具过滤**（clawspring 的 `tools` 字段是摆设）；测试覆盖 none/skills/inherit/all 四种 preset（`prepare.rs:1031-1072`）。
- skill 级 `allowed_tools` 过滤也已实现（`prepare.rs:58-74`）。
- frontmatter 解析器现成：`skills/parse.rs:5` `split_frontmatter`、`parse.rs:71` `parse_allowed_tools` —— 加载 `.md` agent 定义文件可直接复用。

### 2.4 事件通道已带运行标识

`emit_chat_stream_delta`（`chat/commands.rs:3770-3799`）payload 含 `conversationId` / `runId` / `messageId` / `segmentId` / `phase` / `round` / `stepNumber`——前端（React 侧监听 `chat-stream` / `chat-tool`）已经按 runId/messageId 路由，**为子 agent 事件加一个维度（如 `parentRunId` 或 segment kind = SubAgent）改动面可控**。

### 2.5 缺失项（如实列出）

1. 无 `Agent` / spawn 类工具；`mcp/registry.rs` 的 native 工具列表中没有任何派生入口（`registry.rs:194` `list_enabled_tool_defs`、`registry.rs:511` `call_native_tool`）。
2. 无 depth 概念：`ToolExecutionContext`（`execute.rs:34-40`）只有 conversation_id/run_id/message_id/generation/round，没有嵌套深度。
3. 无后台任务管理器：`AppState`（`state.rs`）没有 task 注册表、收件箱、结果缓存。
4. 无 worktree / cwd 隔离概念（native_tools/files.rs、shell.rs 的工作目录处理是会话级的）。
5. `run_agent_loop` 的调用点只有一个：`chat/commands.rs:1212`，由 `complete_assistant_reply`（`commands.rs:912`）驱动，前置约 300 行 setup（system prompt 组装 `commands.rs:1146-1163`、消息构建 `commands.rs:1165-1171`、fallback prompt `commands.rs:1176-1198`）——**这段 setup 与 Conversation/UI 强耦合，是子 agent 复用主循环的主要障碍**。

---

## 3. 差距分析

### clawspring 有、kivio 缺失

| # | 能力 | clawspring 位置 | kivio 现状 |
|---|---|---|---|
| G1 | `Agent` 工具（spawn 子 agent，同步/后台） | multi_agent/tools.py:160-221 | 完全缺失 |
| G2 | SubAgentManager（并发上限、任务表、按名寻址） | subagent.py:278-480 | 完全缺失 |
| G3 | agent 类型三层加载（内置/用户/项目 .md） | subagent.py:150-179 | 有 ChatAssistant 但不面向"被 spawn"，无目录加载 agents/*.md |
| G4 | 深度防护（max_depth） | subagent.py:319-322 + agent.py:81 | 无 depth 概念 |
| G5 | 后台执行 + 完成通知 | tools.py:86-93 + clawspring.py:1277 | 无 |
| G6 | SendMessage 收件箱（后台 agent 多轮对话） | subagent.py:438-459, 386-401 | 无 |
| G7 | worktree 文件系统隔离 | subagent.py:219-253, 339-359 | 无 |
| G8 | CheckAgentResult / ListAgentTasks / ListAgentTypes 查询工具 | tools.py:246-295 | 无 |

### kivio 有、clawspring 没有（不要丢掉）

| # | 能力 | kivio 位置 | clawspring 对照 |
|---|---|---|---|
| K1 | host/executor 双 trait 抽象，循环可被任意宿主复用 | host.rs:10-58, execute.rs:24-32 | agent.py 直接 import tools 全局注册表，靠 config 私有键传上下文 |
| K2 | **真正强制执行**的工具过滤（assistant preset / skill allowed_tools） | prepare.rs:58-92 | AgentDefinition.tools 从未生效（仅展示） |
| K3 | 同轮并行工具执行（tokio::join，上限 4） | loop_.rs:42, 1219-1270 | 串行 for 循环（agent.py:123） |
| K4 | 结构化结果（segments/steps/tool_records/stream_outcome） | types.rs:66-85 | 只回传最后一条文本 |
| K5 | 按 conversation 的 generation 取消 + 等待（select! 友好） | state.rs:174-210 | 布尔 cancel_flag 轮询 |
| K6 | 工具审批 / ask_user 异步交互闭环 | host.rs:38-49 | 子 agent 内权限事件被静默丢弃 |
| K7 | 到达轮次上限时注入收尾 system message | loop_.rs:568-573 | 直接 break |
| K8 | provider 双协议适配（OpenAI/Anthropic）+ fallback prompt | commands.rs:3491-3510, types.rs:62 | providers.py 单一中立格式 |

**核心判断**：kivio 不缺执行引擎质量，缺的是"把引擎再实例化一次"的那层薄壳（clawspring 用 ~500 行做到了）。kivio 的难点不在循环本身，而在 G5 提到的：`run_agent_loop` 唯一调用点前面那 ~300 行与 Conversation 强耦合的 setup 需要先抽出一个"无 UI 的运行入口"。

---

## 4. 重构建议

### P0 — 抽出无界面的 agent 运行入口（前置工程，~2-3 天）

**目标**：让"跑一次 agent"不依赖 `Conversation` 持久化与主窗口事件。

1. 新建 `chat/agent/runner.rs`，提供：
   ```rust
   pub struct SubAgentRequest {
       pub prompt: String,
       pub system_prompt: String,
       pub provider: ModelProvider,
       pub model: String,
       pub tools: Vec<ChatToolDefinition>,   // 已按 agent 类型过滤
       pub depth: u8,
       pub parent: ParentRunIds,             // conversation_id / run_id / tool_call_id
   }
   pub async fn run_sub_agent(state: &AppState, app: &AppHandle, req: SubAgentRequest)
       -> Result<AgentRunResult, String>
   ```
   内部构造独立 `runtime_messages`（仅 system + user prompt——上下文隔离即此一步，对应 clawspring `AgentState()`，subagent.py:369）、独立 `conversation_id`（如 `subagent-{uuid}`，使 `state.rs:174` 的 generation 机制天然按子 agent 维度工作），然后调既有 `run_agent_loop`。
2. 把 `complete_assistant_reply` 中 system prompt 组装（commands.rs:1146-1163）与 `build_chat_api_messages` 调用收敛成可独立调用的小函数，主路径与 runner 共用。
3. **SubAgentHost**：实现 `AgentHost`，策略与 clawspring 一致但更精细：
   - `emit_stream_delta/done`：v1 直接丢弃（对齐 clawspring `for _event in gen: pass`），或降采样转发为父运行的一个 segment 进度（见 P1-3）。
   - `request_tool_approval`：**深度 >0 时对 sensitive 工具直接拒绝**（把 clawspring 的"隐式拒绝"变成显式策略，execute.rs 已有 `builtin_tool_bypasses_approval` prepare.rs:204 可复用）。
   - `is_generation_active`：委托 `state.is_chat_generation_active(sub_conversation_id, gen)`，并级联检查父 generation——父被取消时子自动失活。

### P0 — `Agent` 工具 + SubAgentManager（~3-4 天）

1. `chat/sub_agent.rs`（或 `chat/agent/manager.rs`）：
   ```rust
   pub struct SubAgentManager {
       tasks: Mutex<HashMap<String, SubAgentTaskRecord>>,
       by_name: Mutex<HashMap<String, String>>,
       semaphore: Arc<tokio::sync::Semaphore>,   // 替代 ThreadPoolExecutor(5)
   }
   pub struct SubAgentTaskRecord {
       pub status: SubAgentStatus,  // Pending/Running/Completed/Failed/Cancelled
       pub result: Option<String>,
       pub inbox: tokio::sync::mpsc::UnboundedSender<String>,
       pub handle: Option<tokio::task::JoinHandle<()>>,
       pub depth: u8, ...
   }
   ```
   挂到 `AppState`（state.rs）。**用 `tokio::task::spawn` + `Semaphore` 取代线程池**：循环本身已是 async，无需线程。注意 `AgentRunConfig` 持有 `&AppState` 借用（types.rs:39），spawn 的 future 须 `'static`——传 `AppHandle`（clone 廉价）并经 `app.state::<AppState>()` 重取，这是 Tauri 惯用法（ChatAgentHost 已持 `AppHandle`，commands.rs:3315-3317）。
2. 在 `mcp/registry.rs` 的 native 工具表注册 `Agent` / `CheckAgentResult` / `ListAgentTasks`（source="native"，参考 todo 工具在 `RegistryToolExecutor::call` 的分流模式，commands.rs:3429-3443）。schema 直接移植 tools.py:172-216。
3. **depth 传递**：在 `ToolExecutionContext`（execute.rs:34-40）加 `pub depth: u8` 字段，`AgentRunConfig` 同步加；`Agent` 工具读 `ctx.depth`，`>= MAX_DEPTH(建议 3)` 返回错误字符串而非 Err（对齐 subagent.py:319-322 的软失败哲学，避免打断父循环）。
4. 同步模式（wait=true）：`tokio::select! { res = run, _ = host.wait_for_generation_inactive(parent...) }` + `tokio::time::timeout(300s)`；结果格式 `[Agent: name (type)]\n\n{result.content}`（tools.py:79-85）。

### P1 — agent 类型定义与加载（~1-2 天）

1. `chat/agent/definitions.rs`：内置 4-5 个 `AgentDefinition`（移植 subagent.py:30-91 的 prompt 文案，提供 zh/en 双语——kivio 全链路有 `language` 参数，types.rs:51）。
2. 目录加载 `~/.kivio/agents/*.md` + 项目级：**复用 `skills/parse.rs:5 split_frontmatter` 与 `parse.rs:56 parse_list_value`**，不要新写解析器。
3. **工具过滤要真正生效**（修复 clawspring 的缺陷）：spawn 时按 `agent_def.tools` 过滤 `Vec<ChatToolDefinition>`，复用 `apply_assistant_tool_preset` 的同款模式（prepare.rs:76-92）；并强制从子 agent 工具表中**剔除 `Agent` 自身**（深度防护之外的第二道闸）。
4. 评估与 `ChatAssistant` 合流：长期看 `AgentDefinition` 可以是 `ChatAssistantSnapshot` 的子集视图（types.rs:458 已有 system_prompt/model/tool_preset），避免两套"人格"模型。

### P1 — 后台执行 + 前端联动（~3 天，含 React 侧）

1. 新增 Tauri 事件 `chat-subagent`：payload 含 `parentConversationId` / `parentRunId` / `taskId` / `name` / `status` / `resultPreview`。前端在父消息的工具卡片（ToolCallBlock）上渲染子 agent 状态——`ToolCallRecord` 已有 `structured_content` 字段（execute.rs:69 附近、commands.rs:3765）可承载 taskId，**后台完成时通过更新同一条 tool record（emit_chat_tool_record 幂等按 toolCallId 路由）回传结果**，不需要 clawspring 那种 REPL 节拍通知。
2. `wait=false` 路径：spawn 后立即返回 taskId 文本；任务完成时若父运行仍 active，可向父 conversation 注入一条 system 提示（下轮可见）；若父已结束，仅更新 tool record + 可选 `PushNotification` 式提醒。
3. 子 agent 流式可见性（可选增强，超越 clawspring）：SubAgentHost 把 delta 降采样（如每 500ms）转成 `chat-subagent` 进度事件，前端折叠展示。v1 可不做。

### P2 — SendMessage 收件箱（~1-2 天）

`tokio::sync::mpsc::unbounded_channel` 替代 `queue.Queue`；任务主体完成后 `while let Ok(msg) = rx.try_recv()` 复用同一 `runtime_messages`（取 `result.api_messages` 追加）再调 `run_sub_agent`，对齐 subagent.py:386-401。注册 `SendMessage` 工具（schema 移植 tools.py:223-244）。

### P2 — worktree 隔离（~2-3 天，谨慎）

- **不要移植 `os.chdir`**（subagent.py:364-367 在多线程下就是错的）；kivio 应给 `SubAgentRequest` 加 `working_dir: Option<PathBuf>`，并让 `native_tools/files.rs` / `shell.rs` 的执行上下文接受显式 cwd（需评估这两个模块当前的目录解析方式，是本项的主要工作量）。
- worktree 创建/清理用 `tokio::process::Command` 执行 git（移植 subagent.py:219-253），修正参考实现的缺陷：**完成后保留分支**、只删 worktree 目录，分支名写进 tool record 供用户 review。
- 桌面 App 场景下该能力价值低于 CLI，故 P2。

### 风险与约束备忘

- `run_agent_loop` 2709 行且为单调用点优化，runner 化时**不要改循环本体**，只在外围构造 config——loop_.rs 的既有测试（TestHost，loop_.rs:1900）是回归保障。
- 子 agent 持久化：v1 不落盘 Conversation（纯内存 task record），避免污染会话列表；后续若要"查看子 agent 完整轨迹"，再考虑写入带 `hidden` 标记的 conversation。
- 并发上限建议 `Semaphore::new(3)`（桌面端 API 配额比 CLI 更敏感，且多 key failover state.rs 的 cooldown 是按 provider 全局的，子 agent 并发会加速烧 key）。

### 工作量汇总

| 项 | 优先级 | 估时 |
|---|---|---|
| runner.rs 无 UI 入口 + SubAgentHost | P0 | 2-3 天 |
| SubAgentManager + Agent/Check/List 工具 + depth | P0 | 3-4 天 |
| agent 类型定义/加载/过滤 | P1 | 1-2 天 |
| 后台执行 + 前端 tool card 联动 | P1 | 3 天 |
| SendMessage 收件箱 | P2 | 1-2 天 |
| worktree / cwd 隔离 | P2 | 2-3 天 |

合计 P0 约 1 周可让 kivio 拥有同步 `Agent` 工具；P0+P1 约 2.5 周达到并超过 clawspring 参考实现（工具过滤真正生效、结构化结果、级联取消）。
