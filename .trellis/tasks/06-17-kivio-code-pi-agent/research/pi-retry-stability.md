# Research: PI agent 的模型调用重试 / 错误恢复 / 稳定性保证

- **Query**: PI agent 如何做模型调用失败的重试、错误恢复、稳定性保证，用于让 kivio-code（Rust）对标修复「报错一次就停下」的问题
- **Scope**: internal（只读 PI 源码 `/Users/zmair/ZM database/Kivio agent/pi/`）
- **Date**: 2026-06-17

---

## 0. 架构总览（关键结论先行）

PI 把"稳定性"分成**四层**，各司其职，单次模型失败几乎不会让 agent 直接停下：

| 层 | 文件 | 职责 |
|---|---|---|
| L1 Provider 适配器 | `ai/src/providers/anthropic.ts`、`openai-responses.ts`、`openai-completions.ts` | 把错误**转成 `stopReason:"error"` 的 message**（不向上抛异常）；默认 SDK 自带重试关闭（`maxRetries ?? 0`） |
| L1b 特殊 Provider 自带重试 | `ai/src/providers/openai-codex-responses.ts` | codex websocket/responses provider 自己带 `for attempt` 重试循环 + `Retry-After` 解析 |
| L2 Loop | `agent/src/agent-loop.ts` | 只跑一轮；遇到 `stopReason:"error"` 直接 `agent_end` 返回（**自己不重试**） |
| L3 Harness | `agent/src/harness/agent-harness.ts` | 把异常转成 `createFailureMessage`（降级输出），把 `maxRetries`/`maxRetryDelayMs` 透传给 provider |
| **L4 编排层（核心重试/恢复在这里）** | `coding-agent/src/core/agent-session.ts` | **应用级指数退避重试** + **上下文超限→压缩后重试** + 阈值自动压缩。是 PI 不"报错就停"的真正原因 |

**核心机制**：模型失败不是异常向上冒泡终止流程，而是变成一条 `stopReason:"error"` 的 assistant message。编排层 `agent-session.ts` 在每轮 agent 结束后（`_handlePostAgentRun`）检查这条 message，决定：**应用级重试** / **压缩后重试** / **放弃**。重试/压缩成功后调用 `agent.continue()` 把循环续上。

---

## 1. 模型调用失败时会重试吗？重试几次？退避参数？

**会。应用级重试默认开启，默认 3 次，指数退避（base 2000ms，×2）。**

证据 —— `coding-agent/src/core/settings-manager.ts:799-804`：
```ts
getRetrySettings(): { enabled: boolean; maxRetries: number; baseDelayMs: number } {
  return {
    enabled: this.getRetryEnabled(),          // 默认 true (line 787: retry?.enabled ?? true)
    maxRetries: this.settings.retry?.maxRetries ?? 3,
    baseDelayMs: this.settings.retry?.baseDelayMs ?? 2000,  // 退避: 2s, 4s, 8s
  };
}
```

退避计算 —— `agent-session.ts:2506`：
```ts
const delayMs = settings.baseDelayMs * 2 ** (this._retryAttempt - 1);  // 2000 * 2^(n-1)
```

退避执行（**可中断的 sleep**）—— `agent-session.ts:2522-2539`：
```ts
this._retryAbortController = new AbortController();
try {
  await sleep(delayMs, this._retryAbortController.signal);
} catch { /* 用户取消，emit auto_retry_end success:false，重置 _retryAttempt=0 */ }
```

重试入口 —— `agent-session.ts:947-975`（`_handlePostAgentRun`，每轮 agent_end 后被 `_runAgentPrompt` 的 `while` 调用）：
```ts
if (this._isRetryableError(msg) && (await this._prepareRetry(msg))) {
  return true;   // → 外层 while 会调用 agent.continue() 续跑
}
```

`_prepareRetry`（`agent-session.ts:2492-2542`）做的事：
1. `_retryAttempt++`；超过 `maxRetries` 则回退计数并 `return false`（放弃）。
2. **把失败的 error message 从 agent 上下文里删掉**（`messages.slice(0,-1)`，但仍保留在 session 历史里）—— 这样重试时不会把"上次报错"喂回模型。
3. 指数退避 sleep。
4. `return true`，让 `_runAgentPrompt` 的 `while` 调 `agent.continue()` 重新发起请求。

> **两层重试**：除了 L4 应用级重试，还有 provider/SDK 级重试（`retry.provider.maxRetries`，默认未设即 0；`maxRetryDelayMs ?? 60000`），透传到 provider 的 `maxRetries` 参数（`agent-harness.ts:388-389`）。标准 OpenAI/Anthropic provider 默认 `maxRetries ?? 0`（`anthropic.ts:520`、`openai-responses.ts:125`、`openai-completions.ts:154`），所以**默认真正生效的是 L4 应用级重试**。

---

## 2. 哪些错误重试，哪些不重试？上下文超限怎么处理？

分三类，由 `agent-session.ts` 三个判定函数区分：

### (a) 可重试错误 —— `_isRetryableError` (`agent-session.ts:2473-2486`)
```ts
private _isRetryableError(message: AssistantMessage): boolean {
  if (message.stopReason !== "error" || !message.errorMessage) return false;
  // 上下文超限走压缩，不走重试
  const contextWindow = this.model?.contextWindow ?? 0;
  if (isContextOverflow(message, contextWindow)) return false;
  const err = message.errorMessage;
  if (this._isNonRetryableProviderLimitError(err)) return false;
  return /overloaded|provider.?returned.?error|rate.?limit|too many requests|429|500|502|503|504|
         service.?unavailable|server.?error|internal.?error|network.?error|connection.?error|
         connection.?refused|connection.?lost|websocket.?closed|websocket.?error|other side closed|
         fetch failed|upstream.?connect|reset before headers|socket hang up|ended without|
         stream ended before message_stop|http2 request did not get a response|
         timed? out|timeout|terminated|retry delay/i.test(err);
}
```
→ 覆盖 **429 / 5xx / 服务过载 / 各类网络与连接错误 / WebSocket 断开 / 流提前结束（`stream ended before message_stop`）/ 超时**。

### (b) 永不重试的额度/计费错误 —— `_isNonRetryableProviderLimitError` (`agent-session.ts:2463-2467`)
```ts
return /GoUsageLimitError|FreeUsageLimitError|Monthly usage limit reached|available balance|
        insufficient_quota|out of budget|quota exceeded|billing/i.test(errorMessage);
```
→ 余额/配额/计费类错误**不重试**（重试也没用）。

### (c) 上下文超限 → 压缩后重试（不是直接失败）

**会触发"压缩后重试"而非直接失败。** 在 `_isRetryableError` 里被显式排除（line 2477-2478），改由 `_checkCompaction` 处理。

判定 —— `ai/src/utils/overflow.ts` 的 `isContextOverflow`（`overflow.ts:126-155`），用 `OVERFLOW_PATTERNS`（`overflow.ts:35-59`，覆盖 Anthropic/OpenAI/Gemini/xAI/Groq/Mistral/OpenRouter/llama.cpp 等几十种 provider 的超限文案）+ `NON_OVERFLOW_PATTERNS`（line 70-74，把限流误判排除）。还能检测"静默超限"（z.ai：用 `usage.input > contextWindow`）和"截断到满后无输出"（小米 MiMo：`stopReason:"length"` + `output===0`）。

恢复逻辑 —— `agent-session.ts:1824-1847`（`_checkCompaction` Case 1）：
```ts
if (sameModel && isContextOverflow(assistantMessage, contextWindow)) {
  if (this._overflowRecoveryAttempted) {           // 只压缩-重试一次
    this._emit({ type: "compaction_end", reason: "overflow", willRetry: false,
      errorMessage: "Context overflow recovery failed after one compact-and-retry attempt..." });
    return false;
  }
  this._overflowRecoveryAttempted = true;
  // 删掉 error message，不喂回上下文
  this.agent.state.messages = messages.slice(0, -1);
  return await this._runAutoCompaction("overflow", true);   // willRetry=true
}
```
`_runAutoCompaction(reason, willRetry=true)` 压缩成功后（`agent-session.ts:2028-2034`）：
```ts
if (willRetry) {
  // 删掉残留的 error assistant message
  this.agent.state.messages = messages.slice(0, -1);
  return true;   // → agent.continue() 用压缩后的更短上下文重新请求
}
```
**保护**：`_overflowRecoveryAttempted` 标志保证超限只"压缩+重试一次"，避免压缩后仍超限造成死循环；成功响应后该标志被重置（`agent-session.ts:480, 531-532`）。

此外还有**阈值自动压缩**（`_checkCompaction` Case 2，`agent-session.ts:1849-1874`）：上下文接近窗口时主动压缩（`shouldCompact`）。对 error message（无 usage）会用 `estimateContextTokens` 估算，保证"持续 API 报错（如 529）的会话仍能压缩"。

---

## 3. 流式（streaming）中途断开/出错怎么办？有 resume/reconnect 吗？

**标准 provider（Anthropic / OpenAI）：没有流内 resume；流中途出错 → 整个流标记为 `stopReason:"error"` → 交给 L4 重新发起整个请求（不是断点续传）。**

证据 —— `anthropic.ts:444-446`（流提前结束被显式检测为错误）：
```ts
if (sawMessageStart && !sawMessageEnd) {
  throw new Error("Anthropic stream ended before message_stop");
}
```
此 throw 被 `streamAnthropic` 的 try/catch 捕获（`anthropic.ts:702-712`），转成 `{type:"error"}` 事件，`output.stopReason="error"`。该错误文案 `"stream ended before message_stop"` 正好命中 `_isRetryableError` 的正则 → **整轮请求重试**。

流早停被设计成"可恢复"的关键点：`message_start` 一到就先把 input token 写进 usage（`anthropic.ts:533-534` 注释："This ensures we have input token counts even if the stream is aborted early"），中断的 tool-call 也会清掉 `partialJson` 残留（`anthropic.ts:703-707`、`654`）。

**例外：codex websocket provider 有自带的连接级重试**（`openai-codex-responses.ts:303-368`）：
```ts
for (let attempt = 0; attempt <= maxRetries; attempt++) {
  ...
  if (attempt < maxRetries && isRetryableError(response.status, errorText)) {
    const retryAfterDelayMs = getRetryAfterDelayMs(response.headers);  // 解析 Retry-After / retry-after-ms
    const delayMs = retryAfterDelayMs === undefined
      ? BASE_DELAY_MS * 2 ** attempt                       // 指数退避
      : capRetryDelayMs(retryAfterDelayMs, options);       // 上限 maxRetryDelayMs (默认 60000)
    ...
  }
  // 网络错误也重试（除"usage limit"外，line 367-368）
}
```
即 codex 这一 provider 在连接层做"reconnect + 退避 + Retry-After 遵从"，其余 provider 把重连交给 L4。

---

## 4. 「一轮里产生过工具结果就不能空手而归」这类不变式？失败时降级输出什么？

PI **没有** "产生过工具结果就不能空手而归" 这种显式不变式。它的不变式是另一种形式：**任何失败都必须落地成一条结构化的 assistant message（带 `stopReason` 与 `errorMessage`），绝不让流程因未捕获异常静默死掉。**

降级输出（graceful failure message）—— `agent-harness.ts:49-58`：
```ts
function createFailureMessage(model: Model<any>, error: unknown, aborted: boolean): AssistantMessage {
  return {
    role: "assistant", content: [], ...,
    stopReason: aborted ? "aborted" : "error",
    errorMessage: error instanceof Error ? error.message : String(error),
  };
}
```
当 loop 抛异常时，harness 用 `emitRunFailure`（`agent-harness.ts:540-549`）把它转成完整事件序列，让 UI/session 像收到正常一轮那样处理：
```ts
const failureMessage = createFailureMessage(model, error, aborted);
await this.handleAgentEvent({ type: "message_start", message: failureMessage }, signal);
await this.handleAgentEvent({ type: "message_end", message: failureMessage }, signal);
await this.handleAgentEvent({ type: "turn_end", message: failureMessage, toolResults: [] }, signal);
await this.handleAgentEvent({ type: "agent_end", messages: [failureMessage] }, signal);
```

loop 层对应保证 —— `agent-loop.ts:196-200`：error/aborted 时也会完整 emit `turn_end` + `agent_end` 再 return，不会半途消失：
```ts
if (message.stopReason === "error" || message.stopReason === "aborted") {
  await emit({ type: "turn_end", message, toolResults: [] });
  await emit({ type: "agent_end", messages: newMessages });
  return;
}
```
工具执行同理 —— 工具异常被 catch 成 `createErrorToolResult` + `isError:true`（`agent-loop.ts:619-623, 659-664, 703-705`），而不是抛出终止整轮。

---

## 5. 整体稳定性策略：PI 怎么保证 agent 不"报错一次就停"？

把上面串起来，PI 的设计是 **"错误即数据，而非异常"** + **编排层统一恢复**：

1. **Provider 层吞异常转 message**：所有 wire 错误、解析错误、流早停都被 try/catch 转成 `stopReason:"error"` 的 message（`anthropic.ts:702-712`），不向上抛 → 上层永远拿到结构化结果而非崩溃。
2. **Loop 层纯净、单轮**：loop 不做重试，遇错就干净地 `agent_end`（`agent-loop.ts:196`），把"要不要重试/恢复"的决策权完全上交编排层。
3. **Harness 兜底降级**：任何漏网异常都被 `createFailureMessage` + `emitRunFailure` 转成"一条失败 message + 完整事件序列"（`agent-harness.ts:540-549`），保证 session/UI 状态机不卡死。
4. **编排层（agent-session）是恢复中枢**：在 `_runAgentPrompt` 的 `while (await this._handlePostAgentRun()) { await this.agent.continue(); }`（`agent-session.ts:939-941`）里，每轮结束后分类处理：
   - 可重试错误 → 指数退避后 `agent.continue()`（最多 3 次）。
   - 上下文超限 → 压缩后 `agent.continue()`（一次）。
   - 阈值临近 → 主动压缩。
   - 不可恢复（额度/计费/重试用尽）→ emit `auto_retry_end success:false` / `compaction_end willRetry:false`，**向用户报告错误**，干净停止。
5. **状态自愈**：重试/压缩前都把失败的 error message 从**活动上下文**里删掉（但保留在 session 历史），避免把错误喂回模型；成功响应后重置 `_retryAttempt` 与 `_overflowRecoveryAttempted`（`agent-session.ts:531-543`），保证多轮会话里每次失败都从干净计数重新开始。
6. **全程可中断**：退避 sleep（`_retryAbortController`）、压缩（`_autoCompactionAbortController`）都挂在 AbortController 上，用户随时能取消（`abortRetry` / `abortCompaction`，`agent-session.ts:2547-2548`、`719-720`）。

**关键答案**：PI 确实把"单次模型失败"转成了"向用户报告但 agent 可继续/可恢复"——靠的是 **(a) 错误转 message 不抛异常**，**(b) 编排层 `_handlePostAgentRun` 循环里的应用级指数退避重试（3 次）**，以及 **(c) 上下文超限的压缩-重试通道**。只有额度类错误或重试耗尽才真正停止，且停止时一定有结构化错误反馈给用户。

---

## 关键文件与行号速查

| 文件 | 行 | 内容 |
|---|---|---|
| `coding-agent/src/core/agent-session.ts` | 936-945 | `_runAgentPrompt` 的 `while + agent.continue()` 续跑循环（恢复主回路） |
| 同上 | 947-975 | `_handlePostAgentRun`：重试 / 压缩 / 续跑分类 |
| 同上 | 2463-2467 | `_isNonRetryableProviderLimitError`（额度/计费不重试） |
| 同上 | 2473-2486 | `_isRetryableError`（429/5xx/网络/流早停的重试白名单正则） |
| 同上 | 2492-2542 | `_prepareRetry`（指数退避 + 删错误 message + 可中断 sleep） |
| 同上 | 1798-1876 | `_checkCompaction`（超限压缩-重试 / 阈值压缩 / 估算 token） |
| 同上 | 1881-2057 | `_runAutoCompaction`（压缩并在 `willRetry` 时 return true 续跑） |
| `coding-agent/src/core/settings-manager.ts` | 27-32, 786-824 | RetrySettings（enabled=true, maxRetries=3, baseDelayMs=2000, provider.maxRetryDelayMs=60000） |
| `ai/src/utils/overflow.ts` | 35-74, 126-155 | `isContextOverflow` + OVERFLOW/NON_OVERFLOW 正则库 |
| `ai/src/providers/anthropic.ts` | 444-446, 520, 692-712 | 流早停检测 / `maxRetries??0` / 错误转 `{type:"error"}` message |
| `ai/src/providers/openai-codex-responses.ts` | 105-153, 303-368 | provider 自带重试循环 + `Retry-After` 解析 + 退避封顶 |
| `agent/src/agent-loop.ts` | 196-200, 619-664 | loop 遇错干净 `agent_end`；工具异常转 errorResult |
| `agent/src/harness/agent-harness.ts` | 49-58, 388-389, 540-549 | `createFailureMessage` 降级；透传 maxRetries；`emitRunFailure` |

---

## kivio-code 应如何对标（可操作建议）

kivio-code 当前症状是「模型调用报错一次就停」。对标 PI，建议按以下顺序改造 `src-tauri/src/chat/agent/`（`loop_.rs` / `stream.rs` / `stop.rs`），从最小改动到完整方案：

1. **错误即数据，不要异常向上终止循环。** 让 provider 适配器（`chat/model/openai.rs` / `anthropic.rs`）把网络/解析/流早停错误转成一个带 `stop_reason = Error` + `error_message` 的结果，而不是 `Err(...)` 直接冒泡终止 `loop_.rs`。这是 PI 全部稳定性的地基（对应 `anthropic.ts:702-712`）。

2. **在 loop 外（或 round 边界）加一层应用级重试，默认开、指数退避。** 参数直接抄 PI：`max_retries=3`、`base_delay=2000ms`、`delay = base * 2^(attempt-1)`（2s/4s/8s）。重试前**把上一条 error 消息从发给模型的上下文里移除**（保留在持久化历史），避免把报错喂回模型（对应 `_prepareRetry`，`agent-session.ts:2492-2542`）。退避用可被 cancellation token 中断的 sleep（kivio 已有 `explain_stream_generation` 风格的取消机制，可复用）。

3. **区分可重试 vs 不可重试错误。** 移植两条正则白/黑名单：
   - 重试：`overloaded|429|5xx|service unavailable|timeout|connection|websocket closed|stream ended before ...|fetch failed`（`agent-session.ts:2483`）。
   - 不重试：`insufficient_quota|quota exceeded|billing|usage limit|available balance`（`agent-session.ts:2464`）—— 这类直接报告用户、停。
   kivio 已有 `is_failover_error`（`src-tauri/src/api.rs`，按 401/402/403/429 判定）——可把它扩展/复用为"是否应用级重试"的判定，注意 5xx/网络/超时也要算可重试（PI 把它们都纳入了，而 kivio 现有 `is_failover_error` 只认 4xx）。

4. **上下文超限单独走"压缩后重试"，不要当普通错误。** kivio 已有 `chat/agent/compaction.rs`。对标 PI：检测到 overflow（移植 `overflow.ts` 的 `OVERFLOW_PATTERNS`，覆盖 OpenAI/Anthropic/各代理文案）时，触发一次压缩然后重发，用 `overflow_recovery_attempted` 单次标志防死循环（`agent-session.ts:1824-1847`）。注意排除限流文案被误判为 overflow（`NON_OVERFLOW_PATTERNS`）。

5. **失败也要"完整收尾"，给前端结构化错误。** 不论重试耗尽还是不可恢复，都要 emit 一条 `stop_reason=error` 的 assistant message + 完整的 turn/round 结束事件（对应 `createFailureMessage` + `emitRunFailure`，`agent-harness.ts:540-549`），让 `chat-stream`/`chat-tool` 事件流不卡死、UI 能显示「失败原因 + 可重试」。

6. **成功后重置计数器。** 一旦某轮成功（`stop_reason != error`），把 retry 计数和 overflow 标志清零（`agent-session.ts:531-543`），保证长会话里每次新失败都从干净状态退避，而不是累积。

7. **（可选，provider 级）尊重 `Retry-After`。** 若某 provider 返回 `Retry-After` / `retry-after-ms` 头，按它的值退避并封顶（默认 60s 上限），优先于本地指数退避（对应 `openai-codex-responses.ts:124-153, 340-347`）。

最小可用版本 = 第 1+2+3+5 步（错误转数据 + 3 次指数退避 + 错误分类 + 失败结构化收尾），即可消除「报错一次就停」。第 4 步解决"上下文撑爆后直接死"的相邻问题，第 6/7 步是健壮性加固。
