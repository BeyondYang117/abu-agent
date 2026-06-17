//! 框架级:模型调用失败的统一分类 + 恢复策略中枢。
//!
//! **职责边界(对标 PI 的 agent-session)**:所有「模型调用失败后怎么办」的语义级决策
//! 只在这里表达——`classify`(失败归类)+ `decide`(策略选择)。synthesis / planning 的
//! 失败路径统一调用恢复入口(`synthesis::recover_synthesis`)执行本模块给出的动作,
//! 不再各自散写 overflow / 去敏 / 兜底 if-else。
//! 传输层重试(429 / 5xx / 网络退避、换 key)归 `api.rs::send_with_retry` / `send_with_failover`,
//! 与本模块的语义级恢复(overflow 压缩重试 / 去敏 / 确定性兜底)互不重叠。
//!
//! 设计目标:一种失败 = 一条分类(`classify`)+ 一条策略(`decide`),所有模型调用阶段
//! 共用;并保证「产生过工具结果的轮次永不空手而归」这一不变式只在此处定义
//! (`DegradeToGathered` → `assemble_results_from_tool_records`)。
//!
//! 不重复造轮子:沿用 `api::extract_status_code` 从错误串里取 HTTP 状态码(failover 逻辑
//! 也是这么做的),内容审核 / 超长靠 body 关键词判定。错误既可能是流式 `ModelError`,
//! 也可能是非流式 `String`,统一按消息文本分类即可,无需改动适配器返回类型。

use crate::chat::types::{ToolCallRecord, ToolCallStatus};

/// 模型调用失败的归类(只列出我们会**区别处置**的类型)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FailureKind {
    /// 供应商内容审核拒绝(典型:400 + "content/risk/policy/safety/审核")。
    ContentModeration,
    /// 上下文超长(400/413 + "context/maximum/token length")。
    ContextOverflow,
    /// 模型调用成功但产出为空。
    Empty,
    /// 限流 / 鉴权 / 5xx / 网络等——底层 api.rs 已重试或换 key,升到这层即已耗尽。
    Exhausted,
    /// 其它(无法归类)。
    Other,
}

/// 对一次失败应采取的恢复动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryAction {
    /// 上下文超长:先压缩一次历史(maybe_compact_send_view),再用压缩后的消息重发一次。
    /// 对标 PI 的 `_overflowRecoveryAttempted` 单次守门——只压缩-重试一次,避免「压完仍超」死循环。
    CompactAndRetry,
    /// 用"去敏 + 精简"的输入重做一次合成(可能产出真正的总结)。
    Remediate,
    /// 直接用已收集到的工具结果确定性兜底(不经模型 → 不被审核)。
    DegradeToGathered,
    /// 无可恢复(且没有工具结果)——交回上层用静态文案。
    Surface,
}

/// 上下文超长信号:覆盖 OpenAI / Anthropic / Gemini / 各代理 / DeepSeek 及国内供应商
/// 常见文案(对标 PI `ai/src/utils/overflow.ts` 的 `OVERFLOW_PATTERNS` 子集)。
/// 命中其一即认为是「上下文撑爆」类失败,应走压缩重试而非快速失败。
const OVERFLOW_PATTERNS: &[&str] = &[
    // OpenAI / 通用
    "maximum context",
    "context length",
    "context_length_exceeded",
    "context window",
    "too many tokens",
    "reduce the length",
    "reduce your prompt",
    "string too long",
    "max_tokens",
    "input is too long",
    "input too long",
    // Anthropic
    "prompt is too long",
    "request too large",
    "request_too_large",
    // Gemini / 其它代理
    "exceeds the maximum",
    "exceeds the limit",
    "exceeds the context",
    "token limit",
    "tokens limit",
    "exceed the context",
    "exceed context",
    // 国内供应商 / 中文文案
    "上下文长度",
    "上下文过长",
    "超出最大",
    "超过最大",
    "输入过长",
    "tokens 超",
];

/// 排除项:这些文案虽含「limit/token」等词,实为限流/配额而非上下文超长,
/// 命中则**不**判为 overflow(对标 PI `NON_OVERFLOW_PATTERNS`,防止把限流误判成 overflow)。
const NON_OVERFLOW_PATTERNS: &[&str] = &[
    "rate limit",
    "rate_limit",
    "too many requests",
    "requests per",
    "quota",
    "insufficient",
    "billing",
    "usage limit",
    "available balance",
];

/// 判断错误文本是否为「上下文超长」(overflow)。先排除限流/配额误判,再匹配 overflow 文案。
fn is_context_overflow(lower: &str) -> bool {
    if NON_OVERFLOW_PATTERNS.iter().any(|n| lower.contains(n)) {
        return false;
    }
    OVERFLOW_PATTERNS.iter().any(|n| lower.contains(n))
}

/// 把错误消息文本归类。`message` 为空视为 `Empty`(调用方在"成功但空响应"时传空串)。
pub(crate) fn classify(message: &str) -> FailureKind {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return FailureKind::Empty;
    }
    let status = crate::api::extract_status_code(trimmed);
    let lower = trimmed.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));

    // 内容审核:供应商措辞不一,关键词覆盖中英常见形态。
    if has(&[
        "content exists risk",
        "content policy",
        "content_policy",
        "content filter",
        "moderation",
        "safety",
        "sensitive",
        "审核",
        "违规",
        "敏感",
    ]) {
        return FailureKind::ContentModeration;
    }
    // 上下文超长。先于 status 判定:overflow 多以 400 返回,若不在此截获会被下面
    // 的 `_ => Other` 当成普通 BadRequest。命中 NON_OVERFLOW(限流/配额)则不算 overflow。
    if is_context_overflow(&lower) {
        return FailureKind::ContextOverflow;
    }
    match status {
        // 审核常以 400 返回但措辞没命中上面的词:仍按 BadRequest→Other 处理,交给
        // Remediate 兜一手(去敏精简后重试),不会更糟。
        Some(429) | Some(401) | Some(402) | Some(403) => FailureKind::Exhausted,
        Some(code) if (500..600).contains(&code) => FailureKind::Exhausted,
        _ => FailureKind::Other,
    }
}

/// 策略:给定失败类型 + 上下文,决定动作。集中表达,取代各阶段散落判断。
///
/// `has_tool_results`:本轮是否已产生工具结果(决定能否兜底)。
/// `already_remediated`:是否已经做过一次 Remediate(避免无限重试)。
/// `overflow_recovery_attempted`:是否已经做过一次 CompactAndRetry(单次守门,避免
/// 「压缩后仍超 → 再压缩」死循环,对标 PI `_overflowRecoveryAttempted`)。
pub(crate) fn decide(
    kind: FailureKind,
    has_tool_results: bool,
    already_remediated: bool,
    overflow_recovery_attempted: bool,
) -> RecoveryAction {
    if !has_tool_results {
        // 没有可兜底的素材:只能交回上层(静态文案 / 向上传播错误)。
        return RecoveryAction::Surface;
    }
    if already_remediated {
        // 去敏重试都失败了,确定性兜底,保证有结果。
        return RecoveryAction::DegradeToGathered;
    }
    match kind {
        // 上下文超长:走「压缩一次 → 重发一次」专用通道(而非去敏重试)。压缩+重试已经
        // 试过一次仍失败 → 降级到确定性兜底,避免死循环(单次守门)。
        FailureKind::ContextOverflow => {
            if overflow_recovery_attempted {
                RecoveryAction::DegradeToGathered
            } else {
                RecoveryAction::CompactAndRetry
            }
        }
        // 请求因内容被拒,或措辞没命中的 400(归到 Other)→ 用去敏精简的输入重做
        // 一次,通常能产出真正的总结;失败了下一轮 already_remediated 会兜底,不会更糟。
        FailureKind::ContentModeration | FailureKind::Other => RecoveryAction::Remediate,
        // 空响应 / 限流耗尽:重做无意义(同样的输入只会再失败),直接用已收集结果兜底。
        FailureKind::Empty | FailureKind::Exhausted => RecoveryAction::DegradeToGathered,
    }
}

/// 不变式实现:从已收集的工具结果确定性拼出可读答复(不经模型,不会被审核)。
/// 没有任何可用 preview 时返回空串,调用方据此退回静态文案。
pub(crate) fn assemble_results_from_tool_records(
    records: &[ToolCallRecord],
    language: &str,
) -> String {
    let zh = language.starts_with("zh");
    let mut blocks: Vec<String> = Vec::new();
    for record in records {
        if record.status != ToolCallStatus::Success {
            continue;
        }
        let preview = record
            .result_preview
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty());
        if let Some(preview) = preview {
            blocks.push(format!("【{}】\n{}", record.name, preview));
        }
    }
    if blocks.is_empty() {
        return String::new();
    }
    let header = if zh {
        "未能生成总结(模型供应商内容审核拦截或调用失败,可能是上下文过长)。上方工具结果已保存,你可以更换更大上下文的模型、让我重新生成,或精简上下文后再试。以下是已检索到的内容:"
    } else {
        "Could not produce a summary (provider content moderation, call failure, or context too long). The tool results below were saved; you can switch to a larger-context model, regenerate, or trim the context and retry. Here is what was gathered:"
    };
    format!("{header}\n\n{}", blocks.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(name: &str, status: ToolCallStatus, preview: Option<&str>) -> ToolCallRecord {
        ToolCallRecord {
            id: "t".into(),
            name: name.into(),
            source: "native".into(),
            server_id: None,
            arguments: String::new(),
            status,
            result_preview: preview.map(|p| p.to_string()),
            error: None,
            duration_ms: None,
            started_at: None,
            completed_at: None,
            round: 0,
            sensitive: false,
            artifacts: Vec::new(),
            trace_id: None,
            span_id: None,
            structured_content: None,
        }
    }

    #[test]
    fn classify_detects_moderation_overflow_empty() {
        assert_eq!(
            classify("Chat stream Error: 400 Bad Request - {\"error\":{\"message\":\"Content Exists Risk\"}}"),
            FailureKind::ContentModeration
        );
        assert_eq!(
            classify("Error: 400 - This model's maximum context length is 8192 tokens"),
            FailureKind::ContextOverflow
        );
        assert_eq!(classify(""), FailureKind::Empty);
        assert_eq!(
            classify("Chat stream Error: 429 Too Many Requests"),
            FailureKind::Exhausted
        );
        assert_eq!(
            classify("Chat API Error: 500 Internal Server Error"),
            FailureKind::Exhausted
        );
    }

    #[test]
    fn classify_detects_broad_overflow_provider_wording() {
        // 各供应商的超长文案都应归到 ContextOverflow(多以 400 返回)。
        for msg in [
            "Error: 400 - prompt is too long: 250000 tokens > 200000 maximum",
            "Chat API Error: 413 - request too large",
            "Error: 400 context_length_exceeded",
            "Error: 400 - input is too long for requested model",
            "Error: 400 - This request exceeds the maximum token limit",
            "Error: 400 reduce the length of the messages",
            "错误:400 - 上下文长度超出最大限制",
            "错误:400 - 输入过长",
        ] {
            assert_eq!(
                classify(msg),
                FailureKind::ContextOverflow,
                "should classify as overflow: {msg}"
            );
        }
    }

    #[test]
    fn classify_excludes_rate_limit_from_overflow() {
        // 限流/配额文案虽含 limit/token 等词,绝不能被当成 overflow。核心断言:不是 ContextOverflow。
        for msg in [
            "Chat stream Error: 429 Too Many Requests - rate limit exceeded, too many tokens per minute",
            "Chat API Error: 429 - quota exceeded for this token limit",
            "Chat API Error: 402 - insufficient available balance",
        ] {
            let kind = classify(msg);
            assert_ne!(kind, FailureKind::ContextOverflow, "must not be overflow: {msg}");
            assert_eq!(kind, FailureKind::Exhausted, "should be exhausted: {msg}");
        }
    }

    #[test]
    fn decide_upholds_invariant() {
        // 无工具结果 → 交回上层
        assert_eq!(
            decide(FailureKind::ContentModeration, false, false, false),
            RecoveryAction::Surface
        );
        // 审核 + 有结果 + 未补救 → 先去敏重试
        assert_eq!(
            decide(FailureKind::ContentModeration, true, false, false),
            RecoveryAction::Remediate
        );
        // 补救后仍失败 → 确定性兜底
        assert_eq!(
            decide(FailureKind::ContentModeration, true, true, false),
            RecoveryAction::DegradeToGathered
        );
        // 已耗尽(限流/5xx)+ 有结果 → 直接兜底
        assert_eq!(
            decide(FailureKind::Exhausted, true, false, false),
            RecoveryAction::DegradeToGathered
        );
        // 措辞没命中的 400(Other)+ 有结果 + 未补救 → 也先去敏重试(与 classify 注释一致)
        assert_eq!(
            decide(FailureKind::Other, true, false, false),
            RecoveryAction::Remediate
        );
        // 空响应重做无意义 → 直接兜底
        assert_eq!(
            decide(FailureKind::Empty, true, false, false),
            RecoveryAction::DegradeToGathered
        );
    }

    #[test]
    fn decide_overflow_compacts_once_then_degrades() {
        // overflow + 有结果 + 未尝试压缩重试 → 走压缩重试通道
        assert_eq!(
            decide(FailureKind::ContextOverflow, true, false, false),
            RecoveryAction::CompactAndRetry
        );
        // overflow + 已尝试过一次压缩重试 → 单次守门:降级兜底,不再压缩(防死循环)
        assert_eq!(
            decide(FailureKind::ContextOverflow, true, false, true),
            RecoveryAction::DegradeToGathered
        );
        // overflow + 无工具结果 → 仍交回上层(没素材可兜底)
        assert_eq!(
            decide(FailureKind::ContextOverflow, false, false, false),
            RecoveryAction::Surface
        );
    }

    #[test]
    fn assemble_uses_successful_previews_only() {
        let records = vec![
            rec("web_search", ToolCallStatus::Success, Some("标题A\n标题B")),
            rec("web_search", ToolCallStatus::Error, Some("不该出现")),
            rec("noop", ToolCallStatus::Success, None),
        ];
        let out = assemble_results_from_tool_records(&records, "zh-CN");
        assert!(out.contains("标题A"));
        assert!(!out.contains("不该出现"));
        assert!(out.contains("web_search"));

        assert!(assemble_results_from_tool_records(&[], "zh-CN").is_empty());
    }
}
