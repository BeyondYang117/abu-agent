# Research: PI agent 上下文窗口管理 / 压缩 / 工具结果截断

- **Query**: PI agent 如何做上下文窗口管理/压缩/工具结果截断，供 kivio-code(Rust) 对标修复「连续工具调用撑爆上下文 → 总结失败」
- **Scope**: internal（只读 PI 源码 `/Users/zmair/ZM database/Kivio agent/pi/`）
- **Date**: 2026-06-17

---

## 0. 一句话结论（先看这个）

PI 的压缩**不是在 tool round 之间触发的**。它把一次 `prompt()` 的整个「assistant ↔ 多轮工具」循环跑完（直到 `agent_end`），**才在 turn 边界检查 token / overflow** 并压缩；真正能在长 tool round 撑爆时救场的是「**overflow 重试**」这条路 —— 即先让模型调用**自然失败**（provider 报 context-overflow 错误），捕获该错误 → 移除错误消息 → 压缩历史 → **自动重试一次**。同时所有大工具输出（bash / read / grep）在**进入历史之前**就已被 truncate 到 50KB / 2000 行，所以单条工具结果几乎不可能单独撑爆窗口。kivio-code 当前缺的就是这两层：(a) tool-result 入历史前的硬截断；(b) overflow 错误捕获→压缩→重试的兜底。

---

## Findings

### Files Found

| File Path | Description |
|---|---|
| `packages/agent/src/agent-loop.ts` | 纯 agent 循环。**循环本身完全不做任何压缩/截断**，只通过 `prepareNextTurn` / `transformContext` 等回调把控制权交给上层 |
| `packages/agent/src/harness/agent-harness.ts` | 通用 harness。`compact()` 是公开方法，**要求 harness 处于 idle**（`phase !== "idle"` 直接抛 busy），即**不在 loop 内部自动调** |
| `packages/agent/src/harness/compaction/compaction.ts` | 压缩核心：`shouldCompact` / `estimateContextTokens` / `findCutPoint` / `prepareCompaction` / `compact` / `generateSummary` + 阈值常量 |
| `packages/agent/src/harness/compaction/utils.ts` | 文件操作追踪 + `serializeConversation`（喂给摘要模型时把每条 tool result 再截到 2000 字符） |
| `packages/agent/src/harness/compaction/branch-summarization.ts` | 分支切换时的摘要（与本问题关系小，下面只简述） |
| `packages/agent/src/harness/utils/truncate.ts` | **通用截断**：`truncateHead`（保头，用于 read 文件）/ `truncateTail`（保尾，用于 bash）。默认 2000 行 / 50KB |
| `packages/agent/src/harness/utils/shell-output.ts` | bash 执行流式抓输出 + 滚动缓冲 + 落盘全量 + 最终 `truncateTail` |
| `packages/coding-agent/src/core/agent-session.ts` | **真正的自动压缩调度器**（PI 缺的那块逻辑在这里，不在 `packages/agent`）：`_checkCompaction` / `_runAutoCompaction` / `_handlePostAgentRun` |
| `packages/ai/src/utils/overflow.ts` | `isContextOverflow`：把各家 provider 的「超长」错误信息识别成 overflow 信号 |
| `packages/coding-agent/src/core/tools/read.ts` / `output-accumulator.ts` / `grep.ts` / `bash-executor.ts` | 各工具在产出结果时调用截断 |

---

## 核心问题逐条回答

### Q1. 压缩在 agent loop 的哪个时机触发？tool rounds 之间会压缩吗？

**不会在 tool rounds 之间压缩。** 证据链：

1. `agent-loop.ts` 的内层循环 `while (hasMoreToolCalls || pendingMessages.length > 0)`（`agent-loop.ts:174`）连续执行 model→tools→model→tools…，**这段代码里没有任何 token 检查 / 压缩调用**。每轮之间只调 `prepareNextTurn`（`agent-loop.ts:226`）和 `shouldStopAfterTurn`（`agent-loop.ts:242`）回调，但 harness 注入的 `prepareNextTurn`（`agent-harness.ts:457-466`）只做 `flushPendingSessionWrites` + 重建 turnState，**不压缩**。

2. harness 的 `compact()` 显式要求 idle：
   ```ts
   // agent-harness.ts:711
   if (this.phase !== "idle") throw new AgentHarnessError("busy", "compact() requires idle harness");
   ```
   loop 运行期间 `phase === "turn"`，所以根本不可能在 loop 内被调到。

3. 真正的触发点在 coding-agent 层，**在整个 prompt 跑完之后**的驱动循环里：
   ```ts
   // agent-session.ts:936-944  _runAgentPrompt
   await this.agent.prompt(messages);
   while (await this._handlePostAgentRun()) {   // ← 跑完一整轮 agent 才检查
       await this.agent.continue();
   }
   ```
   `_handlePostAgentRun()`（`agent-session.ts:947-975`）里调 `_checkCompaction(msg)`（`agent-session.ts:968`）。它接收的是**最后一条 assistant 消息**，即一次完整 agent 运行结束（`agent_end`）后才检查。注释写得很明确：
   ```ts
   // agent-session.ts:1788-1789
   * Check if compaction is needed and run it.
   * Called after agent_end and before prompt submission.
   ```

4. 另一处检查在**下一次 prompt 提交前**（`agent-session.ts:1066-1068`）：
   ```ts
   const lastAssistant = this._findLastAssistantMessage();
   if (lastAssistant && (await this._checkCompaction(lastAssistant, false))) { ... }
   ```

**结论**：PI 的常规压缩只发生在 **turn 边界**（agent_end 后 / 下一个 prompt 前）。在一个 turn 内连续几十次工具调用时，**唯一能救场的不是阈值压缩，而是 Q5 的 overflow 重试机制**——靠模型调用自然失败来触发。这正是 kivio-code 需要补的关键设计点。

---

### Q2. 触发条件是什么？阈值 / 比例 / token 估算？

**两个独立触发条件**，都在 `_checkCompaction`（`agent-session.ts:1798-1876`）里：

**条件 A — Overflow（硬失败）**：`isContextOverflow(assistantMessage, contextWindow)` 为真（`agent-session.ts:1825`）。即模型这次调用**已经因为超长报错了**（或 z.ai 式静默超长 / MiMo 式 length-stop），见 Q5。

**条件 B — Threshold（阈值，软触发）**：
```ts
// compaction.ts:196-199
export function shouldCompact(contextTokens, contextWindow, settings): boolean {
    if (!settings.enabled) return false;
    return contextTokens > contextWindow - settings.reserveTokens;
}
```
- 是**硬 token 数**，不是百分比：`contextTokens > contextWindow - reserveTokens`。
- `reserveTokens` 默认 **16384**（`compaction.ts:112-116` `DEFAULT_COMPACTION_SETTINGS`）。即一旦上下文超过「窗口 − 16K」就压缩。

**token 怎么估算**（`estimateContextTokens`, `compaction.ts:165-193`）：
- **优先用 provider 真实回报的 usage**：从消息列表里**从后往前**找最近一条成功的 assistant 消息，取它的 `usage`（`calculateContextTokens = totalTokens || input+output+cacheRead+cacheWrite`，`compaction.ts:119-121`）。
- 该 usage 之后新增的消息（trailing）用**字符启发式**补估：`estimateTokens`（`compaction.ts:220-260`）= `Math.ceil(chars / 4)`，图片按固定 `ESTIMATED_IMAGE_CHARS = 4800` 字符算（`compaction.ts:201`）。
- 在 threshold 分支里：正常 stop 用 `calculateContextTokens(assistantMessage.usage)`（`agent-session.ts:1870`）；如果这次是 error（无 usage）则退回 `estimateContextTokens` 用最近成功响应估（`agent-session.ts:1853-1868`），保证「连续 529 错误也能压缩」。

**重要防抖**：`_checkCompaction` 会跳过「时间戳早于最近一次 compaction 边界」的 assistant 消息（`agent-session.ts:1814-1822` 和 `1860-1867`），避免压缩刚做完又被旧 usage 重新触发。

---

### Q3. 压缩 / 截断分几层？每层阈值？

PI 实际是 **3 层防御**，从「最早 / 最便宜」到「最晚 / 最贵」：

**第 1 层 — 工具产出时的硬截断（入历史前）**（见 Q4）
- 常量在 `truncate.ts:11-13`：`DEFAULT_MAX_LINES = 2000`，`DEFAULT_MAX_BYTES = 50*1024`(50KB)，`GREP_MAX_LINE_LENGTH = 500`。
- 任意一个限制先命中即截断（line 或 byte，谁先到谁赢）。
- 这是最关键的一层：保证**单条工具结果天花板 ≈ 50KB ≈ 1.2 万 token**，所以「一次 bash 抓回大段文本」不会单独撑爆窗口。

**第 2 层 — turn 边界的阈值压缩（模型摘要）**
- `shouldCompact`：`contextTokens > contextWindow - 16384`（Q2）。
- 触发后 `prepareCompaction`（`compaction.ts:542`）+ `compact`（`compaction.ts:627`）：用 `findCutPoint` 选切点，把旧历史交给摘要模型生成结构化 summary，**保留最近约 `keepRecentTokens = 20000` token**（`compaction.ts:112-116`）。
- 切点逻辑 `findCutPoint`（`compaction.ts:329-377`）：从尾部往前累加 token，累到 ≥ `keepRecentTokens` 时，在合法切点（user/assistant/bashExecution/summary 边界，**不在 toolResult 处切**，`findValidCutPoints` `compaction.ts:261-299`）切开。若切点落在一个 turn 中间，会把「该 turn 前缀」单独再摘要一份（split-turn，`compaction.ts:577-595` + `653-680`）。
- 摘要 prompt 给模型的 `maxTokens = min(0.8 * reserveTokens, model.maxTokens)`（`compaction.ts:467-470`），即摘要输出预算 ≈ 0.8×16384 ≈ 13K；split-turn 前缀摘要预算 0.5×reserveTokens（`compaction.ts:716-719`）。
- 喂给摘要模型时，历史里每条 tool result 再被截到 `TOOL_RESULT_MAX_CHARS = 2000` 字符（`utils.ts:74` + `serializeConversation` `utils.ts:138`），避免「摘要请求本身」又超长。

**第 3 层 — overflow 兜底压缩 + 重试**（见 Q5）
- 当前两层都没拦住、模型调用真的报 overflow 时，`_checkCompaction` 走 overflow 分支强制压一次并自动重试一次。

---

### Q4. 单条工具结果入历史前会不会先截断？保头还是保尾？上限？

**会，强制截断，这是第一道也是最重要的防线。** 两个方向语义不同：

**`truncateHead`（保头）**（`truncate.ts:125-207`）—— 用于**读文件**（看开头）：
- 限制：默认 2000 行 / 50KB，谁先命中谁赢。
- 永远不返回半行；若第一行就超 byte 限制，返回空 + `firstLineExceedsLimit=true`（`truncate.ts:151-166`）。
- 调用方 `read.ts:299`（`truncateHead(selectedContent)`），并在 description 里告诉模型「超 2000 行/50KB 截断，用 offset/limit 继续读」（`read.ts:212`）。

**`truncateTail`（保尾）**（`truncate.ts:215-289`）—— 用于 **bash 输出**（看结尾错误/结果）：
- 同样 2000 行 / 50KB。
- 从结尾往前收集完整行；边界情况：若最后一行本身超 byte 限制，取该行尾部（部分行，`truncate.ts:255-260` `lastLinePartial`）。

**bash 的流式抓取**（`shell-output.ts:43-143`）做得更细：
- 滚动缓冲上限 `maxOutputBytes = DEFAULT_MAX_BYTES * 2`(100KB)，边收边丢最旧 chunk（`shell-output.ts:50, 95-98`）。
- 超过 50KB 时把**全量**写到临时文件 `bash-*.log`（`shell-output.ts:88-90` `ensureFullOutputFile`），最终返回时对内存缓冲再 `truncateTail`（`shell-output.ts:111-115`）。
- 截断后给模型的文本里附上 `[Output truncated. Full output: <path>]`（`messages.ts:75-77` `bashExecutionToText`），模型可按需再读全量文件。

**grep**：单行用 `truncateLine` 截到 `GREP_MAX_LINE_LENGTH = 500` 字符（`truncate.ts:336-344`），加 `... [truncated]` 后缀。

**结论 Q4**：上限统一 50KB / 2000 行；read 保头、bash 保尾、grep 限行宽 500；超量内容落盘（bash）或提示用 offset 续读（read）。**这是 kivio-code 必须先补的一层** —— 没有它，单次工具结果就能把窗口打爆。

---

### Q5. 压缩失败怎么兜底？整轮失败还是降级继续？

PI **不会因为压缩失败就让整轮硬挂**，而是分级降级，并且有「先撞墙再补救」的 overflow 路径：

**Overflow 恢复路径**（`agent-session.ts:1824-1847`）：
```ts
if (sameModel && isContextOverflow(assistantMessage, contextWindow)) {
    if (this._overflowRecoveryAttempted) {            // 已经试过一次还失败
        this._emit({ type:"compaction_end", reason:"overflow",
            errorMessage:"Context overflow recovery failed after one compact-and-retry attempt..." });
        return false;                                  // 放弃，不再死循环
    }
    this._overflowRecoveryAttempted = true;
    // 把那条报错的 assistant 消息从 agent state 里移除（仍存 session 历史，但不进重试上下文）
    const messages = this.agent.state.messages;
    if (messages.length>0 && messages.at(-1).role==="assistant")
        this.agent.state.messages = messages.slice(0,-1);
    return await this._runAutoCompaction("overflow", true);   // willRetry=true
}
```
- overflow 只重试 **1 次**（`_overflowRecoveryAttempted` 守门），避免「压缩→还是超→再压缩」死循环。
- `willRetry=true` 时，`_runAutoCompaction` 成功后返回 `true`（`agent-session.ts:2028-2035`），驱动循环 `while(await _handlePostAgentRun()) await this.agent.continue()` 就会**自动用压缩后的上下文重发**。
- threshold 触发时 `willRetry=false`（`agent-session.ts:1873`）—— 压完**不自动重试**，等用户继续（除非有排队消息，`agent-session.ts:2037-2039`）。

**压缩自身失败的兜底**（`_runAutoCompaction` 的 try/catch，`agent-session.ts:2040-2056`）：
- 摘要模型调用抛错 → 捕获 → `_emit({type:"compaction_end", aborted:false, willRetry:false, errorMessage:"Auto-compaction failed: ..."})` → **`return false`**。返回 false 意味着驱动循环停止、不重试，agent 干净地停在那一轮，错误以事件形式上报 UI，**不抛异常炸掉整个会话**。
- 被用户 abort：`_autoCompactionAbortController.signal.aborted` → emit `aborted:true` + `return false`（`agent-session.ts:1991-2000`）。
- 无 model / 无 auth / `prepareCompaction` 返回 undefined（没东西可压）→ 都是 emit 事件 + `return false`，优雅降级（`agent-session.ts:1888-1931`）。

**摘要生成层**本身也用 `Result` 类型而非抛异常：`generateSummary` 在 stop=aborted/error 时返回 `err(new CompactionError(...))`（`compaction.ts:501-511`），由上层决定如何处理，不会 panic。

**结论 Q5**：失败 = 发事件 + 返回 false（停在当前轮、保留历史、不死循环）；overflow 走「压一次 + 重试一次」，仍失败则提示换大窗口模型。**没有任何一条路径会让整个 agent 进程崩溃。**

---

### 附：branch-summarization（与本问题关系较小）

`branch-summarization.ts` 处理的是**用户在会话树里跳到别的分支**时，把被离开的分支摘要成一段 `branchSummary` 消息（`generateBranchSummary` `branch-summarization.ts:201`，`maxTokens:2048`）。`prepareBranchEntries`（`branch-summarization.ts:125-164`）用 `tokenBudget = contextWindow - reserveTokens(16384)` 反向选消息。它和「长 turn 撑爆」无关，是树形历史导航功能，kivio-code 若无会话树可忽略。

---

## kivio-code 应如何对标（可操作建议）

按「投入产出比」排序，前两条直接解决「连续工具调用撑爆 → 总结失败」：

1. **【必做·第一优先】给每条工具结果加入历史前的硬截断。** 对标 `truncate.ts` + `shell-output.ts`：
   - 定常量 `MAX_TOOL_RESULT_BYTES ≈ 50KB`、`MAX_LINES ≈ 2000`。
   - bash/run_command **保尾**（看错误/结果），read_file **保头**，search/grep 限单行宽（~500 字符）。
   - 超量内容落临时文件，结果文本里附 `[truncated, full output: <path>]`，让模型按需再读。
   - 这一层让「单次工具调用」永远撑不爆窗口，是 kivio-code 当前最可能缺的一环。

2. **【必做·第二优先】加 overflow 错误捕获 → 压缩 → 重试一次的兜底。** 对标 `overflow.ts` + `_checkCompaction` overflow 分支：
   - 在 Rust 的 provider adapter（`chat/model/openai.rs` / `anthropic.rs`）里识别 context-overflow 错误（参考 `OVERFLOW_PATTERNS` 那张 provider 正则表，尤其 Anthropic 的 `prompt is too long` / `request_too_large`）。
   - 命中后：移除报错的 assistant 消息 → 触发压缩 → **自动重发一次**；用一个 `overflow_recovery_attempted` 布尔守门，**只重试一次**，避免死循环；再失败就给用户「换大窗口模型」提示。
   - 这条专治「最终总结时一次性塞太多 tool 历史 → 模型报超长 → 总结失败」：现在 kivio-code 大概是直接把这个错误当普通失败抛了。

3. **【强烈建议】turn 边界的阈值压缩。** 对标 `shouldCompact` + `_runAutoCompaction`：
   - 在每个完整 turn 结束后（agent_end / 下次 prompt 前，**不要在 tool round 之间**）检查 `contextTokens > contextWindow - RESERVE(16K)`。
   - token 数**优先用 provider 真实回报的 usage**（kivio-code 的 `usage.rs` 已经在记 usage，可复用），trailing 部分用 `chars/4` 估。
   - 触发后保留最近 ~20K token，旧历史交给一个 summary 调用生成结构化 checkpoint（参考 `SUMMARIZATION_PROMPT` 的 Goal/Progress/Next Steps/Critical Context 格式），用 `compactionSummary` 这种特殊消息塞回上下文头部。
   - **切点不要落在 tool_result 上**（对标 `findValidCutPoints`），否则会留下「孤儿 tool result / 缺失 tool call」破坏 provider 校验。

4. **【建议】压缩失败一律降级、不 panic。** 对标 `_runAutoCompaction` 的 try/catch：摘要调用失败 → 发事件/记日志 + 停在当前轮，保留历史，绝不让整个 agent loop 抛错终止。用 `Result` 而非 `panic!`/`?` 直接冒泡。

5. **【建议】喂给摘要模型的历史里，把每条 tool result 再截到 ~2000 字符。** 对标 `serializeConversation` 的 `TOOL_RESULT_MAX_CHARS=2000`：防止「为了压缩而构造的总结请求」本身又超长 —— 这恰好是「总结失败」的另一个隐藏成因。

6. **【可选】加防抖**：压缩刚做完后，忽略「时间戳早于压缩边界」的旧 usage/error，避免压完立刻又被旧数据重新触发（对标 `agent-session.ts:1814-1822`）。

---

## Caveats / Not Found

- PI 的代码分两个包：通用引擎 `packages/agent`（loop + harness + 压缩**算法**），和具体应用 `packages/coding-agent`（压缩**调度策略** `agent-session.ts`）。**「何时触发压缩」的真正逻辑在 coding-agent，不在 agent**——任务列的几个 agent-loop 文件本身不含触发逻辑，必须看 `agent-session.ts`，这点已在上文补齐。
- 没有发现「tool round 之间」的主动压缩；这是 PI 的有意设计（靠 overflow 兜底而非轮间检查）。如果 kivio-code 的单个 turn 工具调用极多且单条结果未截断，会比 PI 更容易爆——所以建议 1 对 kivio-code 比对 PI 还要更重要。
- `RESERVE=16384` / `keepRecent=20000` / `50KB` / `2000行` / `2000字符` 这些都是 PI 的硬编码默认值（`DEFAULT_COMPACTION_SETTINGS`、`truncate.ts` 常量），kivio-code 可按自己常用模型窗口调，但比例关系（reserve ≈ keepRecent ≈ 窗口的 10-20%）值得保留。
- 未深入 `output-accumulator.ts` 全文（仅确认它用 `truncateTail` + `DEFAULT_MAX_*`，滚动缓冲 `maxBytes*2`），如需流式工具输出累积的精确语义可进一步细读。
