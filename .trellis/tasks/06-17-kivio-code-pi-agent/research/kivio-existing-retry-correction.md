# Research: kivio 现有重试层 — 关键修正（避免做重复功能）

- **Date**: 2026-06-17
- **Scope**: internal（kivio `src-tauri/src/api.rs`）

## 关键发现（修正 PRD 的 P0 假设）

kivio 底层 `api.rs::send_with_retry` / `send_with_failover` **已经覆盖了 PI 应用级重试的大部分场景**。见 `api.rs:213-236` 注释：

- **429 限流** → 内层退避重试；同 key 连续达阈值且有备用 key 才换 key。
- **5xx / timeout / connect 网络错误** → 内层退避重试，不换 key。
- **401/402/403（坏 key）** → 不重试，立即换 key（failover）。
- **400 / 404 / 422 等确定性客户端错误** → **不重试，快速失败**。

`is_failover_error`（api.rs:223）只对 401/402/403/429 触发换 key。

## 推论 —— 用户「报错一次就停」的真实根因

不是「缺通用退避重试」（5xx/网络/429 底层已重试）。真正没被处理的是：

1. **overflow（上下文超长）** —— 各 provider 多以 **400** 返回（"maximum context length"、"prompt is too long"、"string too long"），被底层判为「确定性 400，快速失败」→ 不重试 → 冒泡 → `recover_synthesis` 兜底 → 停。**这才是该补的：overflow 应走「压缩后重试」而非快速失败。**
2. **内容审核** —— 也常以 400 返回（"Content Exists Risk"）。`recover_synthesis` 已对它做一次 `Remediate`（去敏重试），但没有「压缩 + 重试」。

## 对 PRD 的调整

- **P0 收窄**：不新增通用指数退避重试层（会与 `send_with_retry` 重复退避、放大延迟）。改为：在 synthesis/planning 失败恢复路径里，把 **overflow 类 400** 单独识别出来，走压缩重试（即原 P1 上升为核心）。
- **P1 即核心**：overflow 识别（移植 PI `overflow.ts` 的 `OVERFLOW_PATTERNS` 子集 + `NON_OVERFLOW_PATTERNS` 排除限流误判）→ 压缩一次 → 重发；`overflow_recovery_attempted` 守门只一次。
- 现有 `recovery.rs::classify` 已有 `ContextOverflow` 分支（关键词 maximum context / context length / string too long），但 `decide` 对它走的是 `Remediate`（去敏重试），**不是压缩重试**。核心改动 = 让 ContextOverflow 走「压缩后重发」。

## 落点

- 非流式统一入口 `planning.rs::call_chat_completion_message_with_usage` → `generate_with_chat_provider`。
- 恢复中枢 `synthesis.rs::recover_synthesis` + `recovery.rs::{classify,decide}`。
- 压缩复用 `compaction.rs::maybe_compact_send_view`（已存在）。
