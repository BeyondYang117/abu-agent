# PRD: kivio-code 稳定性 — 上下文超限自动恢复 + 模型调用失败重试（对标 PI agent）

- **任务**: 06-17-kivio-code-pi-agent
- **状态**: planning
- **日期**: 2026-06-17
- **基准**: PI agent (`/Users/zmair/ZM database/Kivio agent/pi/`)，调研见 `research/pi-context-compaction.md`、`research/pi-retry-stability.md`

---

## 1. 背景 / 问题

用户在 kivio-code（DeepSeek-V4-Flash，小上下文窗口）上连续跑多个 bash 抓大段代码后，最终总结阶段报：
`Could not produce a summary (provider content moderation or call failure); here is what was gathered:`
然后 agent 停下。用户核心诉求两条：

1. **编码工具必须稳定，不能「报错一次就直接停」**——当前模型调用失败没有应用级重试。
2. **上下文超限要能自动恢复**，对标 PI 的做法。

## 2. 现状（kivio-code）

- **工具结果截断**：bash 已做（`native_tools/shell.rs`：`TAIL_MAX_BYTES=50KB` / `TAIL_MAX_LINES=2000`，超量落盘 + inline 16KB cap），与 PI 一致。read/grep 截断待确认补齐。
- **压缩**：`chat/agent/compaction.rs::maybe_compact_send_view` 两层（snip 旧 tool result → 模型摘要），挂在 `planning` 和 `synthesis`。主循环 `loop { planning_step → run_tool_round }`，planning 内先压缩 → **rounds 之间其实每轮都压**。
- **统一模型调用入口**：`chat/agent/planning.rs::call_chat_completion_message_with_usage`（planning 草稿 / synthesis / compaction 摘要都走它）→ `provider.generate()`。
- **网络层重试**：`api.rs::send_with_retry`（指数退避）+ `send_with_failover`（换 key），但只处理 4xx/5xx/网络；**overflow / 内容审核不触发它**。
- **失败兜底**：`recovery.rs` + `synthesis.rs::recover_synthesis`：`classify` → `Remediate`（去敏重试一次）/ `DegradeToGathered`（拼工具结果）/ `Surface`。

## 3. 差距（kivio 缺 / PI 有）

| # | 差距 | PI 怎么做 | 影响 |
|---|---|---|---|
| G1 | **模型调用无应用级指数退避重试** | `agent-session.ts` 每轮结束后重试 3 次，base 2000ms ×2，可中断 sleep | 「报错一次就停」根因 |
| G2 | **overflow 不走「压缩后重试」专用通道** | `_checkCompaction` 识别 overflow → 删错误消息 → 压缩 → 重发一次（`_overflowRecoveryAttempted` 守门） | 撑爆后直接死，而非自救 |
| G3 | **错误未统一分类为可重试/不可重试** | `_isRetryableError`(429/5xx/网络/流早停) vs `_isNonRetryableProviderLimitError`(额度/计费) | 不该重试的也可能重试、该重试的没重试 |
| G4 | overflow 信号识别不全 | `overflow.ts` 的 `OVERFLOW_PATTERNS`（覆盖各 provider 文案）+ `NON_OVERFLOW_PATTERNS` 排除限流误判 | 超长错误被当普通失败 |
| G5 | read/grep 工具结果入历史前截断待确认 | `truncateHead`(read 保头) / grep 限行宽 500 | 单条结果撑爆（bash 已防住） |

## 4. 方案（按优先级，对标 PI）

### P0 — 应用级重试（治「报错一次就停」，G1+G3）
在 `call_chat_completion_message_with_usage`（统一入口）外包一层重试：
- 默认开，`max_retries=3`，`delay = base(2000ms) * 2^(attempt-1)` → 2s/4s/8s。
- 复用 settings 已有 `retry_enabled` / `retry_attempts`（与项目现有失败重试配置对齐，不新增 UI）。
- 退避 sleep 可被取消（复用 generation/cancel 机制）。
- 错误分类（移植 PI 两条正则）：
  - 可重试：`overloaded|429|5xx|service unavailable|timeout|connection|websocket closed|stream ended before|fetch failed`
  - 不可重试：`insufficient_quota|quota exceeded|billing|usage limit|available balance`
- 重试前不把上一次 error 当模型输入（本来就不会，确认即可）。

> **⚠️ 修正（见 `research/kivio-existing-retry-correction.md`）**：上面这段 P0 作废。
> kivio 底层 `api.rs::send_with_retry` **已覆盖** 429/5xx/网络/超时的退避重试，
> 再加一层通用重试会重复退避、放大延迟。**P0 不做通用重试层。**
> 真正缺的是「overflow（多以 400 返回）被当确定性错误快速失败、不重试」——见 P1。

### P0(修正) — 重试逻辑规范化（用户明确要求「别搞得到处都是」）
当前重试/恢复散落在多处：`api.rs`（网络层 send_with_retry/failover）、`recovery.rs`（classify/decide）、
`synthesis.rs::recover_synthesis`（Remediate/Degrade）、`compaction.rs`（压缩）。
**目标：恢复策略集中到一个中枢**，对标 PI 的 `agent-session.ts`。具体：
- **`recovery.rs` 升格为唯一的「恢复策略中枢」**：所有「模型调用失败后怎么办」的决策只在这里表达
  （classify 分类 + decide 策略），不再在 synthesis/planning 里散写 if-else。
- 网络层（api.rs send_with_retry）保持现状、**只管传输层退避**，职责边界写清楚注释：
  「传输层重试（429/5xx/网络）在此；语义级恢复（overflow 压缩重试 / 去敏 / 兜底）在 recovery.rs」。
- synthesis/planning 的失败路径**统一调用 recovery 中枢的一个入口函数**（如 `recover_model_call`），
  不各自判断 overflow / 去敏 / 兜底。
- 结果：一处分类、一处策略、一处兜底；新增 overflow 重试也只动 recovery 中枢。

### P1 — overflow 压缩后重试（核心，治「撑爆后直接死」，G2+G4）
- 在 recovery 中枢新增 overflow 识别（移植 `overflow.ts` 的 `OVERFLOW_PATTERNS` / `NON_OVERFLOW_PATTERNS` 子集，
  覆盖 OpenAI/Anthropic/常见代理 + DeepSeek/国内供应商文案；排除限流误判）。
- `recovery.rs::classify` 的 `ContextOverflow` 分支：`decide` 改为返回新动作 `CompactAndRetry`
  （而非现在的 `Remediate`）。
- `recover_synthesis`（或统一的 `recover_model_call`）执行 `CompactAndRetry`：调 `maybe_compact_send_view`
  压缩一次 → 用压缩后的消息重发一次；`overflow_recovery_attempted` 单次守门防死循环；
  仍失败 → 降级到 `DegradeToGathered` + 提示「换更大上下文模型」。

### P2 — 兜底文案 + 失败结构化收尾（小修，可顺带）
- `recovery.rs:139` 兜底文案补「可换模型 / 重新生成 / 精简上下文后重试」引导，与 `finalize.rs:389` 对齐。
- 确认失败时仍 emit 完整的 stream/tool done 事件，UI 不卡死。

### P3 — read/grep 截断补齐（G5，确认后按需）
- 确认 read/grep 是否已截断；未做则对标 PI 补 `truncateHead` / grep 限行宽。
  （bash 已做：shell.rs `TAIL_MAX_BYTES=50KB`/`TAIL_MAX_LINES=2000`。）

## 5. 不做 / 范围外
- 不引入会话树分支摘要（PI 的 branch-summarization，kivio 无会话树）。
- 不改 GUI chat 的既有行为（改动在共享 agent loop，需保证 GUI 回归不破）。
- 不新增重试相关的 Settings UI（复用现有 `retry_*`）。

## 6. 验收
- 连续多个大 bash 后总结不再「报错即停」：失败先重试，overflow 先压缩再重试。
- `cargo test`（尤其 `loop_tests.rs`）全绿；新增重试/overflow 单测。
- GUI chat 行为无回归。
- 真机：在 DeepSeek-V4-Flash 上复跑用户场景，不再出现裸 `Could not produce a summary` 即停。

## 7. 风险
- 改动在 GUI/CLI 共享的 agent loop 核心路径，回归面大 → 必须扩 `loop_tests.rs`。
- 重试叠加底层 `send_with_retry` 可能放大延迟 → 应用级重试只对「非网络层」的 overflow/特定错误生效，避免与网络层重试重复退避。
- overflow 正则误判（把限流当 overflow）→ 移植 `NON_OVERFLOW_PATTERNS` 排除。
