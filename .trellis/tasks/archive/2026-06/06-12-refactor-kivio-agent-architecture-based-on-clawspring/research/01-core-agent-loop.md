# 研究文档 01：核心 Agent 循环（clawspring → kivio）

> 范围：模型调用循环、上下文管理、压缩/compaction、provider 抽象、流式处理、取消/停止。
> clawspring 侧：`clawspring.py`（3348 行）、`agent.py`（179 行）、`context.py`（165 行）、`compaction.py`（196 行）、`config.py`（80 行）、`providers.py`（628 行）。
> kivio 侧：`src-tauri/src/chat/agent/*`（loop_.rs 2709 行等）、`chat/model/*`、`chat/types.rs`、`chat/commands.rs`（5332 行，agent 入口与上下文压缩在此）。

---

## 1. clawspring 设计精读

### 1.1 总体分层

clawspring 把"agent 框架"拆成五个职责单一的模块，依赖方向严格单向：

```
clawspring.py (REPL/渲染/slash 命令, ~3300 行 —— 全是 UI/编排)
    └── agent.py (核心循环, 179 行)
          ├── providers.py (多 provider 流式适配, 628 行)
          ├── compaction.py (上下文压缩, 196 行)
          ├── tool_registry.py / tools.py (工具注册与执行)
          └── context.py (system prompt 组装, 165 行)
```

核心循环只有 **95 行**（`agent.py:55-150`），这是整个设计最值得学的点：**循环本体不做任何渲染、不做任何 I/O 决策，只产出事件流**。

### 1.2 数据结构

**会话状态**（`agent.py:22-28`）：

```python
@dataclass
class AgentState:
    messages: list                # 中立格式消息
    total_input_tokens: int       # 累计 token 用量
    total_output_tokens: int
    turn_count: int
```

**中立消息格式**（`providers.py:224-231` 注释处定义），provider 无关：

```python
{"role": "user",      "content": "text"}
{"role": "assistant", "content": "text", "tool_calls": [{"id","name","input"}]}
{"role": "tool", "tool_call_id": "...", "name": "...", "content": "..."}
```

进入具体 provider 时才转换：`messages_to_anthropic`（`providers.py:233-276`，把连续 tool 消息合并为一个 user/tool_result 块）、`messages_to_openai`（`providers.py:279-324`，保留 Gemini `thought_signature` 等 `extra_content` 透传）。

**事件类型**（generator yield 的"协议"）：
- `TextChunk` / `ThinkingChunk` / `AssistantTurn`（`providers.py:329-341`）—— provider 层事件；`AssistantTurn` 携带 `text + tool_calls + in_tokens + out_tokens`，是一次模型调用的终结事件。
- `ToolStart` / `ToolEnd` / `TurnDone` / `PermissionRequest`（`agent.py:31-50`）—— agent 层事件。`PermissionRequest` 是个**可变对象**：循环 yield 它出去，外层 REPL 设置 `req.granted` 后循环继续读取（`agent.py:129-131`），用 generator 的暂停语义实现了"循环中途等待用户审批"，零回调、零 channel。

### 1.3 主循环控制流（`agent.py:55-150`）

```
run(user_message, state, config, system_prompt, depth, cancel_check):
  1. 追加 user 消息（附带 /image 命令暂存的图片, :74-78）
  2. while True:
     a. cancel_check() → 提前 return            (:84-85)
     b. maybe_compact(state, config)             (:90)  ← 每轮模型调用前检查压缩
     c. for event in providers.stream(...):      (:93-103)
          TextChunk/ThinkingChunk → 透传 yield
          AssistantTurn → 留存
     d. 记录 assistant 消息（中立格式）、累计 token、yield TurnDone (:109-117)
     e. 无 tool_calls → break（自然结束）        (:119-120)
     f. 逐个执行工具：
          yield ToolStart
          权限三态判定 _check_permission (:155-170: accept-all/manual/auto，
            auto 下只读工具直接放行、Bash 走 _is_safe_bash 白名单、写操作要问)
          未放行 → yield PermissionRequest，读回 granted
          execute_tool(...)（registry 分发 + 输出截断, tool_registry.py:57-93）
          yield ToolEnd；追加 tool 消息          (:123-150)
```

要点：
- **压缩时机**：在 while 循环顶部、每次模型调用前（`agent.py:90`），意味着长工具链运行中途也会触发压缩，而不是只在用户发消息时。
- **错误恢复**：循环内几乎没有 —— 异常直接抛给 REPL。REPL 唯一的定制恢复是 Ollama 404 → 弹模型选择器 → 弹出刚加的 user 消息后整体重试（`clawspring.py:2812-2823`）。这是 clawspring 较弱的一面。
- **取消**：`cancel_check` 闭包每轮检查一次（`agent.py:84`）；交互层用 KeyboardInterrupt + "2 秒内 3 次 Ctrl+C 强退"（`clawspring.py:2887-2899`）。粒度粗（无法打断单次流式读取），但语义简单。

### 1.4 上下文管理与压缩（`compaction.py`）

两层结构，入口 `maybe_compact`（`compaction.py:170-196`）：

1. **token 估算**：`estimate_tokens`（`:9-35`）—— 全部 content + tool_calls 字符数 / 3.5。粗但够用，且把 `tool_calls` 参数也计入（很多实现漏掉这块）。
2. **窗口上限**：`get_context_limit`（`:38-48`）从 `PROVIDERS[provider]["context_limit"]` 查（anthropic 200k、gemini/qwen 1M、deepseek 64k...，`providers.py:29-128`）。
3. **触发阈值**：估算 > limit × 0.7（`:183`）。
4. **Layer 1 —— snip_old_tool_results**（`:53-83`）：保留最后 6 条消息不动，更早的 `role=="tool"` 消息若超过 2000 字符，保留前 1/2 + 后 1/4，中间替换为 `[... N chars snipped ...]`。**廉价、无模型调用、保留头尾语义**。做完再测一次，若已达标则不进 Layer 2。
5. **Layer 2 —— compact_messages**（`:110-165`）：`find_split_point`（`:88-107`）从尾部反向累计 token，找到"最近 30% token"的分界点；旧段拼成文本（每条截 500 字符）发给模型做摘要；返回 `[summary_user_msg, assistant_ack_msg, *recent]`。**摘要 + 假 ack 消息对**保证消息序列 role 交替合法。

设计意图：先用零成本的结构化截断兜住大多数情况（工具输出是上下文膨胀的主因），只有仍然超限才花一次模型调用做摘要；摘要后保留的"最近 30%"是按 token 而非按条数切，避免一条巨型消息打破预算。

### 1.5 provider 抽象（`providers.py`）

- **注册表驱动**：`PROVIDERS` dict（`:29-128`）每项含 `type`（anthropic/openai/ollama 三种协议族）、`api_key_env`、`base_url`、`context_limit`、`max_completion_tokens`、`models`。新增 OpenAI 兼容 provider = 加一条 dict。
- **模型名路由**：`detect_provider`（`:171-179`）支持 `provider/model` 显式前缀或 `_PREFIXES` 自动检测（`:152-168`）。
- **统一入口** `stream()`（`:580-616`）：检测 provider → 分发到 `stream_anthropic`（`:344-397`，SDK 流式 + thinking 预算）/ `stream_openai_compat`（`:400-493`，手写 delta 聚合：`tool_buf[idx]` 累积 name/arguments 分片，`:459-474`；JSON 解析失败时降级为 `{"_raw": args}` 而不是丢弃，`:484-487`）/ `stream_ollama`（`:496-577`，原生 /api/chat，工具参数 dict 化、native thinking 字段）。
- **成本核算**：`COSTS` 每百万 token 单价表（`:131-149`）+ `calc_cost`（`:202-204`），REPL `/cost` 直接读 `AgentState` 累计值（`clawspring.py:979-987`）。

### 1.6 system prompt 组装（`context.py`）

`build_system_prompt`（`:153-165`）每个用户回合重建一次（`clawspring.py:2707`，"picks up cwd changes"）：模板（`:9-95`）+ 当前日期/cwd/platform + `get_git_info`（branch/status/log，`:98-118`）+ `get_claude_md`（全局 + 向上 10 级目录查找项目 CLAUDE.md，`:121-150`）+ 持久记忆。

### 1.7 REPL 编排（`clawspring.py`）

`run_query`（`:2700-2839`）消费 agent 事件流做渲染：spinner 管理、thinking 灰显、**post-tool 文本去重**（`:2740-2758`，模型在工具调用后复读 pre-tool 文本时缓冲比对并吞掉 —— 处理部分模型复读问题的脏但有效手法）、`query_lock`（RLock）串行化前台与后台（Telegram/proactive）查询。3348 行中约 95% 是 REPL/slash 命令/Telegram/voice/brainstorm 等外围功能，**核心循环本身从未膨胀**。

---

## 2. kivio 现状

### 2.1 模块结构

```
chat/agent/
  loop_.rs    2709 行  run_agent_loop 主循环 + 工具轮执行 + 流式封装 + fallback 文案 + ~840 行测试
  execute.rs   932 行  execute_tool_call（审批/超时/取消/schema 校验/截断）
  prepare.rs  1158 行  prepare_agent_step + system prompt 组装 + 工具过滤 + token 估算
  stream.rs    911 行  AgentStreamSink / ToolCallDraftTracker / 流策略
  stop.rs      414 行  tool_calls 提取（含 DSML 回退）/ reasoning 合并 / 停止判定
  host.rs       58 行  AgentHost trait（事件外发 + 审批 + 取消等待的抽象边界）
  types.rs      85 行  AgentRunConfig / AgentPhase / AgentStopReason / AgentStreamPolicy
chat/model/
  types.rs     767 行  ModelMessage/MessagePart 中立格式、LanguageModelProvider trait、双向转换
  openai.rs    835 行  OpenAiChatProvider（generate + 手写 SSE 流解析）
  anthropic.rs 1066 行 AnthropicMessagesProvider（SSE + thinking + 角色合并）
chat/commands.rs 5332 行  Tauri command 入口、上下文压缩、消息组装、Host/Executor 实现
```

### 2.2 主循环（`loop_.rs:190-980`，单函数 790 行）

`run_agent_loop(config, host: &dyn AgentHost, executor: &dyn ToolExecutor)`：

1. **工具轮循环**（`:208-576`）：每轮 `round+=1`，先查 `host.is_generation_active`（`:215`）；`prepare_agent_step`（`prepare.rs:28-43`，目前只是按 phase 决定 active_tools 和流策略，消息原样克隆）；为本步预留 reasoning/text segment（`SegmentBuilder`，`:69-164`）；发起规划请求：
   - 流式：`stream_scoped_chat_completion_inner`（`:1563-1657`），三层错误恢复：工具参数生成中断 → 直接落败并产出 `tool_planning_failed_run_result`（`:285-301`，避免半截写文件参数重试造成重复写入）；`is_stream_read_interrupted` → 降级非流式重试一次（`:302-325`）；其他错误上抛。
   - 非流式：`tokio::select!` 同时等待请求与 `host.wait_for_generation_inactive`（`:329-356`）。
2. **provider 不支持 tools 的降级**（`:363-396`）：`is_tools_unsupported_error`（`stop.rs:155-171`，按 400/404/422/501 + 关键词判定）→ 先尝试只保留 skill 工具重试，再失败则 `provider_tools_unsupported = true` 落到纯聊天，并用 `patch_system_message` 替换 system prompt（`:578-583`）。
3. **无 tool_calls → planning 即终稿**（`:400-453`）：直接把规划消息作为最终回复（省一次合成请求），segment phase 改写为 Plain。
4. **有 tool_calls → `execute_tool_round`**（`:986-1172`）：逐个匹配工具；未知/被 Plan 模式阻断/参数 JSON 损坏分别产出结构化错误 record 回喂模型（`:1407-1493`）；只读工具进并行批（`tool_call_parallel_eligible`，`:1512-1529`：native 只读白名单 + MCP read-only annotation），`MAX_PARALLEL_TOOL_CALLS_PER_ROUND = 4`（`:42`），用 `tokio::join!` 按 1/2/3/4 手工分支并发（`execute_parallel_chunk`，`:1219-1266`）；写类工具串行。取消时把剩余 tool_calls 全部补成 Cancelled record（`:1343-1405`），保证 OpenAI 协议中每个 tool_call 都有响应消息。
5. **轮次上限**：`tool_round_limit_reached`（`:982-984`，`max_tool_rounds` 来自 `settings.rs:722,748-749`）→ 注入中文"必须直接回答"的 system 消息（`stop.rs:16-21`）后跳出。
6. **合成阶段**（`:619-940`）：tools 清空再调一次模型；流式/非流式两条路径**各自完整实现**了一套"合成失败但已有工具结果 → 双语 fallback 文案 + emit + 提前 return"的逻辑（`:686-720` 流式失败、`:723-752` 流式取消、`:757-785` 流式空输出、`:823-859` 非流式失败、`:873-918` 非流式空输出），fallback 文案在 `:1659-1689`。
7. 组装 `AgentRunResult { content, reasoning, tool_records, segments, api_messages, steps, stream_outcome }`（`types.rs:77-85`）。

### 2.3 上下文管理（在 `commands.rs`，不在 agent 模块内）

- **估算**：`estimate_tokens`（`prepare.rs:769`，chars/比例启发式）+ `count_tokens_in_value`（`commands.rs:2108-2126`）+ 图片附件估算（`commands.rs:802-803`，固定 1600 token/图）。
- **窗口**：`context_window_for_model`（`model_metadata.rs`，内嵌 `modelDatabase.json` 数据库 + provider `ModelInfo` 覆盖，fallback 200k）。
- **压缩**：`should_auto_compress_context`（`commands.rs:2675-2689`，ratio ≥ `AUTO_COMPRESS_RATIO = 0.85`，`:799`）→ `compress_conversation_context`（`:2691-2783`）：`compression_boundary_index`（`:2785-2796`）保留最近 `KEEP_RECENT_RAW_MESSAGES = 8` 条、边界对齐到 assistant 消息；可配独立压缩模型（`effective_compression_model`）；摘要存为 `ConversationContextSummary`（`types.rs:21-36`，含 before/after token 估算、stale 标记）持久化在会话里。
- **重放**：`build_chat_api_messages`（`commands.rs:3060-3126`）—— system + summary 消息 + 边界后原始消息；assistant 消息优先重放 `model_messages`/`api_messages`（保留完整 tool_calls 链）。
- **触发时机**：仅在 `chat_send_message` 用户发消息时（`commands.rs:608-652`），压缩失败且预估超限则**回滚用户消息**并报错（`:626-641`），否则带 warning 继续发送。
- **前端联动**：`ConversationContextState`（`types.rs:39-62`）带分段 token 占比（system/skills/MCP/conversation/attachments 各一段，`commands.rs:2330-2391`），经 `chat-context` 事件推给 React（`:2866-2878`）。

### 2.4 provider 抽象（`chat/model/`）

- `LanguageModelProvider` trait（`model/types.rs:409-417`）：`generate` / `stream(sink)` / `capabilities`。两个实现按 `ProviderApiFormat` 分发（`loop_.rs:1829-1868`）。
- 中立格式 `ModelMessage { role, content: Vec<MessagePart> }`（`model/types.rs:26-60`）：Text/Image/ImageUrl/ToolCall/ToolResult/Reasoning 六种 part —— 比 clawspring 的 dict 更精细（多模态、reasoning 一等公民）。
- **但运行时主循环不用它**：`runtime_messages` 是 `Vec<serde_json::Value>`（OpenAI 格式），每次请求经 `generate_request_from_openai_messages`（`model/types.rs:436-478`）转成 ModelMessage，Anthropic 侧再转一次（`anthropic.rs:401-421` + 角色合并 `:764`）。中立格式成了"中转格式"而非"存储格式"。
- 流式协议 `StreamPart`（`model/types.rs:291-322`）：TextDelta/ReasoningDelta/ToolCallStart/Delta/Done/Finish/Error，经 `StreamSink` trait 回调进 `AgentStreamSink`（`stream.rs:134-418`），后者负责 DSML 抑制缓冲（`:190-214`）、工具草稿实时进度（`ToolCallDraftTracker`，每 2048 字符 emit 一次参数进度，`:309-346`）。
- 多 key failover：`send_with_failover`（`api.rs:230-286`）+ 60s 冷却 + `is_failover_error` 严格 401/402/403/429（`api.rs:215-217`），两个 provider 实现都走它（`openai.rs:64,122`；`anthropic.rs:65,123`）。

### 2.5 取消/停止

- 代际计数：`AppState.chat_stream_generations: Mutex<HashMap<String, u64>>`，`next_chat_generation`/`cancel_chat_generation`/`is_chat_generation_active`（`state.rs:173-210`）。每对话独立、新一轮自动作废旧轮。
- 等待原语：`wait_for_chat_cancel` 是 **100ms 轮询 sleep 循环**（`commands.rs:3729-3733`），通过 `host.wait_for_generation_inactive` 进入各 `tokio::select!`（模型请求 `loop_.rs:1606-1629`、工具执行 `execute.rs:166-180`）。
- 取消语义完整：流中途取消保留已生成文本（`loop_.rs:1614-1628`）；工具轮取消补齐全部 pending tool 的 Cancelled record；有工具结果时取消仍返回 `Ok(AgentRunResult{stream_outcome:"cancelled"})` 保住已有成果（`:723-752`）。

### 2.6 kivio 已经做对的部分（明确列出）

1. **Host/Executor trait 抽象边界**（`host.rs:10-58`）—— 主循环不依赖 Tauri，测试用 `TestHost`/`RecordingExecutor` 直接驱动（`loop_.rs:1885-2025`），这正是 clawspring generator 模式在 Rust 异步环境下的正确等价物。
2. **错误恢复远强于 clawspring**：tools-unsupported 三段降级、流中断降级非流式、合成失败保工具结果、参数损坏结构化回喂、空输出 fallback —— clawspring 全部没有。
3. **取消粒度远细于 clawspring**：每个 await 点都可打断，工具协议完整性有保证。
4. **并行工具执行 + 只读判定**（clawspring 的 `concurrent_safe` 字段定义了但循环里没用，仍是串行执行）。
5. **多 key failover、用量记录（`record_usage_success/failure`）、DSML 文本协议回退**（让不支持 function calling 的模型也能用工具，`stop.rs:89-114`）。
6. **压缩结果持久化 + 前端可视化分段**，比 clawspring 的一次性内存压缩更产品化。
7. 工具输出截断双层：执行期 `max_tool_output_chars`（`execute.rs:183-219,518-535`）+ preview 截断，等价于 clawspring `tool_registry.py:83-92`。

---

## 3. 差距分析

### 3.1 clawspring 有、kivio 缺失或粗糙

| # | 能力 | clawspring | kivio 现状 | 影响 |
|---|------|-----------|-----------|------|
| G1 | **循环中途压缩** | `maybe_compact` 在每次模型调用前（`agent.py:90`） | 压缩只在 `chat_send_message` 入口（`commands.rs:608`）；`run_agent_loop` 内 `runtime_messages` 随工具轮无限增长，无任何检查 | 长 agent 任务（几十轮工具、大输出）中途必然爆窗口，报错而非自愈 |
| G2 | **Layer-1 廉价截断（snip 旧 tool 结果）** | `snip_old_tool_results`（`compaction.py:53-83`）保留头 1/2 尾 1/4 | 无对应物。历史重放时旧 tool 消息原样带上（`build_chat_api_messages` 重放完整 `api_messages`，`commands.rs:3107-3122`）；只有执行期一次性截断 | 多轮会话里旧工具输出持续占窗口，过早触发昂贵的 LLM 摘要 |
| G3 | **主循环代码体量/可读性** | `run()` 95 行，事件驱动，单一职责 | `run_agent_loop` 单函数 790 行；流式/非流式合成两套 fallback 重复 ~250 行；"emit fallback + return AgentRunResult" 模式重复 6 处（`loop_.rs:686-720, 723-752, 757-785, 823-859, 873-918, 1691-1766`） | 改一处 fallback 要同步 2-3 处；新人无法追踪控制流 |
| G4 | **token 用量/成本累计** | `AgentState.total_*_tokens` + `TurnDone` 事件 + `/cost`（`agent.py:115-117`、`clawspring.py:979-987`） | provider 返回 `ModelUsage`（`model/types.rs:198-208`）且有 usage 记录通道，但 `run_agent_loop` 把 `GenerateOutput.usage` 丢弃，`AgentRunResult` 无 usage 字段，前端无法展示本轮真实 token/成本 | 上下文估算只能靠 chars 启发式；用户看不到花费 |
| G5 | **取消等待原语** | N/A（同步模型）| 100ms 轮询 sleep（`commands.rs:3729-3733`），每个活动请求/工具一个轮询任务 | 最多 100ms 取消延迟 + 无谓唤醒；tokio 有现成 `watch`/`Notify`/`CancellationToken` |
| G6 | **压缩与 agent 循环解耦复用** | `maybe_compact(state, config)` 一个入口，任何调用方可用 | 压缩逻辑（boundary/prompt/sanitize/summary）散在 `commands.rs:2675-2864`，与 Conversation 持久化结构强耦合，循环内无法调用 | G1 修复的前置障碍 |
| G7 | **中立消息格式作为运行时货币** | 全程中立 dict，进 provider 才转 | `ModelMessage` 设计完善但循环用 OpenAI `Value`；每步 `Value→ModelMessage→(Anthropic)Value` 双重转换（`loop_.rs:1544-1555` + `anthropic.rs:401`） | 转换开销小，但语义损耗大：reasoning/图片在 Value 层靠字段名约定（`reasoning_content`），易碎 |
| G8 | 模型调用前消息再加工钩子 | `prepare` 即 compact + system 重建 | `prepare_agent_step`（`prepare.rs:28-43`）只复制消息、选工具，没有任何窗口治理 | 是放置 G1/G2 的天然位置，目前是空壳 |

### 3.2 kivio 有、clawspring 没有（重构时必须保住）

1. **结构化流式 UI 协议**：segments（Reasoning/Text/Tool 分段 + phase + order，`types.rs:199-214`）、工具参数生成实时进度、`chat-stream`/`chat-tool`/`chat-context` 事件 → React。clawspring 只有终端打印。
2. **多层错误恢复**（§2.6.2）—— clawspring 异常直接炸到 REPL。
3. **细粒度协作式取消 + 协议完整性**（§2.6.3）。
4. **审批流/ask_user/Plan 模式工具阻断/todo**：`request_tool_approval`、`execute_ask_user_call`（`execute.rs:145-160, 224-296`）、blocked_tool_calls（`loop_.rs:1407-1426`）。
5. **并行只读工具执行**（`loop_.rs:1075-1105`）。
6. **多 key failover + 冷却**（`api.rs:230-286`）。
7. **工具参数 JSON Schema 校验**（`execute.rs:329-465`），clawspring 直接 `tool.func(**params)` 任由炸裂。
8. **DSML 文本工具协议回退**、**skill-only 工具降级**、**辅助视觉模型混编**（`commands.rs:966-1078`）。
9. **压缩摘要持久化 + stale 失效追踪 + 可配压缩专用模型**。

结论：kivio 不是"落后实现"，而是**鲁棒性/产品化远超参考实现、但代码组织和上下文窗口治理明显落后**。重构目标是把 clawspring 的"循环骨架瘦、压缩进循环"移植过来，而不是重写。

---

## 4. 重构建议

### P0-1 拆分 `run_agent_loop`（工作量：2-3 天，纯结构重构无行为变更）

现状 790 行单函数 + 6 处重复 fallback。按 clawspring 的"循环骨架只做编排"原则拆为：

```
chat/agent/
  loop_.rs        仅保留 run_agent_loop 骨架（目标 <150 行）：
                  loop { prepare → planning_step → 无工具? finalize : tool_round → 限轮? break } → synthesis_step → finalize
  planning.rs     新增：planning_step()——封装 loop_.rs:253-357 的流式/非流式规划请求
                  + :363-396 tools-unsupported 降级 + :400-453 planning 即终稿判定，
                  返回 enum PlanningOutcome { Final(Message), ToolCalls(Vec<PendingToolCall>, Message), ToolsUnsupported }
  synthesis.rs    新增：synthesis_step()——合并 loop_.rs:653-794（流式）与 :795-940（非流式）两套重复逻辑，
                  内部统一产出 SynthesisOutcome { Ok, EmptyFallback, FailedFallback, Cancelled }
  finalize.rs     新增：RunResultBuilder——收敛 6 处 "emit_delta + emit_done + push api_message + 构造 AgentRunResult"
                  重复块（loop_.rs:686-720, 723-752, 757-785, 823-859, 873-918, 1691-1766）为
                  builder.fallback(kind, language).emit(host).build()
  rounds.rs       迁移 execute_tool_round 及其辅助（loop_.rs:986-1529）
  loop_tests.rs   迁移 ~840 行测试（loop_.rs:1870-2709）
```

约束：`AgentHost`/`ToolExecutor` trait 签名不动，`commands.rs:1212` 调用点不动，前端事件协议不动。先迁测试再拆，每步 `cargo test --manifest-path src-tauri/Cargo.toml` 守护。

### P0-2 循环内上下文治理（G1+G2+G6+G8，工作量：3-4 天）

仿 `compaction.py` 建 `chat/agent/compaction.rs`，在 `prepare_agent_step`（`prepare.rs:28`）里真正干活：

1. **Layer 1 移植 `snip_old_tool_results`**：对 `runtime_messages` 中倒数 N 条（建议 N=8，对齐 `KEEP_RECENT_RAW_MESSAGES`）之外的 `role=="tool"` 消息，超过阈值（如 4000 chars）时头 1/2 + 尾 1/4 截断。纯函数、可单测、零额外请求。注意：只修改发给模型的副本，不动 `generated_api_messages`（持久化保持原样，避免破坏前端工具卡片回放）。
2. **轮内超限检查**：每轮规划请求前用 `estimate_tokens` × `context_window_for_model` 估 ratio；> 0.85 先 Layer 1；仍超 → 调用从 `commands.rs:2691-2783` 抽出的纯函数版 `summarize_messages(state, provider, model, msgs) -> Summary`（与 `Conversation` 解耦，`commands.rs` 的会话级压缩改为调它），把 `runtime_messages` 中旧工具轮替换为 summary+ack 消息对（对齐 `compaction.py:157-165`）。
3. 压缩发生时通过新增 `AgentHost::emit_context_event`（或复用现有 `chat-context` 事件）通知前端刷新占用条。
4. Tauri/tokio 注意点：压缩用模型请求同样要包进 `tokio::select!` + `wait_for_generation_inactive`，并计入 usage 记录。

### P1-1 取消原语替换轮询（工作量：1 天）

`AppState.chat_stream_generations` 的 `HashMap<String, u64>` 改为 `HashMap<String, tokio::sync::watch::Sender<u64>>`；`wait_for_chat_cancel`（`commands.rs:3729`）改为 `rx.changed().await` 直至代际不匹配。`is_chat_generation_active` 读 `watch::Receiver::borrow`。Host trait 不变，仅 `commands.rs` + `state.rs:173-210` 改动；顺带消除 100ms 取消延迟。

### P1-2 usage/成本贯通（G4，工作量：1-2 天）

- `AgentStepResult` 增加 `usage: Option<ModelUsage>`；`stream_scoped_chat_completion_inner` / `call_chat_completion_message` 已拿到 `GenerateOutput.usage`（`loop_.rs:1556-1559` 当前丢弃），透传出来。
- `AgentRunResult` 增加累计 `total_usage`，`push_assistant_message` 存入 `ChatMessage`，前端消息卡片显示真实 in/out tokens；`compute_context_state` 在有真实 usage 时用它校准 chars 启发式（仿 clawspring `TurnDone` 的角色）。

### P2-1 中立格式作为循环货币（G7，工作量：5+ 天，风险较高，可缓行）

把 `runtime_messages: Vec<Value>` 换成 `Vec<ModelMessage>`，`extract_tool_calls`/`assistant_content_from_api_message` 等 `stop.rs` 中基于 Value 字段名的提取函数改为模式匹配 `MessagePart`；OpenAI/Anthropic 各只在出口转换一次。建议在 P0-1 拆分稳定后再做，否则 diff 互相干扰。

### P2-2 杂项

- `execute_parallel_chunk` 的 1/2/3/4 手工分支（`loop_.rs:1219-1266`）改 `futures::future::join_all`（已在依赖树中），删 ~40 行。
- `prepare_agent_step` 的 `runtime_messages.to_vec()` 每轮全量克隆（`prepare.rs:39`），消息多时是 O(n) 深拷贝；P0-2 引入"发送视图"后可顺带改为按需构造。
- 借鉴 clawspring `COSTS` 表（`providers.py:131-149`）：`modelDatabase.json` 已含 pricing（`settings.rs ModelPricing`），P1-2 落地后即可算钱。

### 不建议照搬的 clawspring 设计

- generator + 可变 `PermissionRequest` 的审批模式 —— kivio 的 oneshot channel + host trait 在多窗口/异步环境下是更正确的形态。
- KeyboardInterrupt 取消 —— kivio 代际方案全面占优。
- 无错误恢复的"裸循环" —— kivio 的分层 fallback 是其核心产品价值，拆分时必须逐条保留（建议拆分 PR 中把 6 个 fallback 场景先补成集成测试再动刀）。
