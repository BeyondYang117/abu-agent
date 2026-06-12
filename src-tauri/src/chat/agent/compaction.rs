use serde_json::{json, Value};

use crate::chat::model_metadata::context_window_for_model;

use super::loop_::{LoopEnv, RunState};
use super::planning::call_chat_completion_message;
use super::prepare::estimate_tokens;

/// 最近 N 条消息保持原样（一般覆盖当前工具轮上下文），更早的才参与 snip/摘要。
pub(crate) const KEEP_RECENT_RAW_MESSAGES: usize = 8;
/// 旧 tool 消息超过该字符数才 snip。
pub(crate) const SNIP_THRESHOLD_CHARS: usize = 4_000;
/// 估算占用超过窗口的该比例才触发压缩。
pub(crate) const COMPACT_TRIGGER_RATIO: f32 = 0.85;
/// Layer2 摘要请求中，每条旧消息最多带入的字符数。
const SUMMARY_SOURCE_CHARS_PER_MESSAGE: usize = 500;

/// Layer1：零成本 snip。除最后 `keep_recent` 条外，`role=="tool"` 且 content 超过
/// `snip_threshold` 字符的消息，content 替换为 头1/2 + 截断标记 + 尾1/4。
/// 返回新 Vec，不修改入参——调用方决定它是只作发送视图还是写回工作副本。
pub(crate) fn snip_old_tool_results(
    messages: &[Value],
    keep_recent: usize,
    snip_threshold: usize,
) -> Vec<Value> {
    let protected_from = messages.len().saturating_sub(keep_recent);
    messages
        .iter()
        .enumerate()
        .map(|(idx, message)| {
            if idx >= protected_from {
                return message.clone();
            }
            if message.get("role").and_then(Value::as_str) != Some("tool") {
                return message.clone();
            }
            let Some(content) = message.get("content").and_then(Value::as_str) else {
                return message.clone();
            };
            let total_chars = content.chars().count();
            if total_chars <= snip_threshold {
                return message.clone();
            }
            let head_chars = total_chars / 2;
            let tail_chars = total_chars / 4;
            let head: String = content.chars().take(head_chars).collect();
            let tail: String = content
                .chars()
                .skip(total_chars.saturating_sub(tail_chars))
                .collect();
            let snipped_chars = total_chars - head_chars - tail_chars;
            let mut next = message.clone();
            next["content"] = json!(format!(
                "{head}\n[... {snipped_chars} chars snipped ...]\n{tail}"
            ));
            next
        })
        .collect()
}

/// 估算消息序列的 token 数：逐条把 content 字符串（以及非字符串 content / tool_calls
/// 等结构化字段的 JSON 序列化）喂给 chars 启发式累加。
pub(crate) fn estimate_messages_tokens(messages: &[Value]) -> usize {
    messages
        .iter()
        .map(|message| match message.get("content").and_then(Value::as_str) {
            Some(text) => {
                let extra = message
                    .get("tool_calls")
                    .map(|calls| estimate_tokens(&calls.to_string()))
                    .unwrap_or(0);
                estimate_tokens(text) + extra + 4
            }
            None => estimate_tokens(&message.to_string()),
        })
        .sum()
}

/// Layer2 摘要的输入：旧段每条截断后的角色标注文本。
fn summary_source_text(messages: &[Value]) -> String {
    let mut out = String::new();
    for message in messages {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let content = match message.get("content").and_then(Value::as_str) {
            Some(text) => text.to_string(),
            None => message.to_string(),
        };
        let clipped: String = content.chars().take(SUMMARY_SOURCE_CHARS_PER_MESSAGE).collect();
        out.push_str(&format!("[{role}] {clipped}\n"));
    }
    out
}

/// 把消息序列按 (系统前缀, 可压缩旧段, 受保护尾段) 三段切开。
/// 系统前缀 = 开头连续的 role=="system" 消息；尾段 = 最后 keep_recent 条（不含进旧段）。
fn split_for_summary(
    messages: &[Value],
    keep_recent: usize,
) -> (Vec<Value>, Vec<Value>, Vec<Value>) {
    let system_end = messages
        .iter()
        .position(|m| m.get("role").and_then(Value::as_str) != Some("system"))
        .unwrap_or(messages.len());
    let protected_from = messages.len().saturating_sub(keep_recent).max(system_end);
    (
        messages[..system_end].to_vec(),
        messages[system_end..protected_from].to_vec(),
        messages[protected_from..].to_vec(),
    )
}

/// 用摘要替换旧段，返回新的消息序列：系统前缀 + summary(user)/ack(assistant) 对 + 尾段。
/// user/assistant 成对插入保证 role 交替对严格 provider 合法。
fn replace_with_summary(
    system_prefix: Vec<Value>,
    summary: &str,
    recent: Vec<Value>,
) -> Vec<Value> {
    let mut out = system_prefix;
    out.push(json!({
        "role": "user",
        "content": format!(
            "[context summary] 以下是本次任务早前工具轮的压缩摘要（原始消息已省略以节省上下文）：\n{summary}"
        ),
    }));
    out.push(json!({
        "role": "assistant",
        "content": "已了解早前工具轮的摘要，继续当前任务。",
    }));
    out.extend(recent);
    out
}

/// 循环内上下文治理入口。返回本步应发送的消息视图：
/// - 未超限：原样 clone（零行为变化）。
/// - 超限：Layer1 snip（只影响发送视图，不落回 state）；
/// - 仍超限：Layer2 模型摘要，成功后**写回 state.runtime_messages**（工作副本），
///   失败或取消则降级返回 snip 视图——压缩是优化，绝不让它失败掉整轮。
/// `generated_api_messages`（持久化镜像）在任何分支都不被触碰。
pub(crate) async fn maybe_compact_send_view(env: &LoopEnv<'_>, state: &mut RunState) -> Vec<Value> {
    let config = env.config;
    let (window, _estimated) = context_window_for_model(Some(&config.provider), &config.model);
    if window == 0 {
        return state.runtime_messages.clone();
    }
    let budget = (window as f32 * COMPACT_TRIGGER_RATIO) as usize;
    let estimated = estimate_messages_tokens(&state.runtime_messages);
    if estimated <= budget {
        return state.runtime_messages.clone();
    }

    let snipped = snip_old_tool_results(
        &state.runtime_messages,
        KEEP_RECENT_RAW_MESSAGES,
        SNIP_THRESHOLD_CHARS,
    );
    let after_snip = estimate_messages_tokens(&snipped);
    eprintln!(
        "Chat context compaction L1: est {estimated} -> {after_snip} tokens (window {window})"
    );
    if after_snip <= budget {
        return snipped;
    }

    let (system_prefix, old_segment, recent) =
        split_for_summary(&snipped, KEEP_RECENT_RAW_MESSAGES);
    if old_segment.is_empty() {
        return snipped;
    }
    let summary_request = vec![
        json!({
            "role": "system",
            "content": "You compress conversation history. Summarize the following earlier tool-loop messages into a compact brief that preserves: the user's goal, key facts/values discovered by tools, files touched and their states, and decisions made. Reply with the summary only.",
        }),
        json!({
            "role": "user",
            "content": summary_source_text(&old_segment),
        }),
    ];
    let summary = tokio::select! {
        result = call_chat_completion_message(
            config.state,
            &config.provider,
            &config.model,
            summary_request,
            None,
            config.retry_attempts,
            false,
            config.max_output_tokens,
            &config.conversation_id,
            &config.message_id,
            "Chat context compaction",
        ) => result,
        _ = env.host.wait_for_generation_inactive(&config.conversation_id, config.generation) => {
            // 取消进行中：放弃压缩，让后续 planning 自己检测取消并正常收尾。
            return snipped;
        }
    };
    match summary {
        Ok(message) => {
            let text = super::stop::assistant_content_from_api_message(&message);
            if text.trim().is_empty() {
                eprintln!("Chat context compaction L2 returned empty summary; falling back to L1 view");
                return snipped;
            }
            let compacted = replace_with_summary(system_prefix, text.trim(), recent);
            let after = estimate_messages_tokens(&compacted);
            eprintln!(
                "Chat context compaction L2: {} old messages -> summary, est {after_snip} -> {after} tokens"
            , old_segment.len());
            // 摘要写回工作副本：后续轮次基于压缩后的历史继续，避免每轮重复摘要。
            state.runtime_messages = compacted.clone();
            compacted
        }
        Err(err) => {
            eprintln!("Chat context compaction L2 failed: {err}; falling back to L1 view");
            snipped
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_msg(content: &str) -> Value {
        json!({ "role": "tool", "tool_call_id": "tc1", "content": content })
    }

    #[test]
    fn snip_truncates_old_oversized_tool_messages_with_marker() {
        let big = "x".repeat(10_000);
        let mut messages = vec![tool_msg(&big)];
        for i in 0..8 {
            messages.push(json!({ "role": "user", "content": format!("recent {i}") }));
        }
        let out = snip_old_tool_results(&messages, 8, 4_000);
        let snipped = out[0]["content"].as_str().unwrap();
        assert!(snipped.contains("chars snipped"));
        assert!(snipped.chars().count() < 10_000);
        // 头 1/2 + 尾 1/4 保留。
        assert!(snipped.starts_with(&"x".repeat(100)));
        assert!(snipped.ends_with(&"x".repeat(100)));
        // 最近 8 条原样。
        for (i, message) in out.iter().skip(1).enumerate() {
            assert_eq!(message["content"], format!("recent {i}"));
        }
    }

    #[test]
    fn snip_leaves_recent_small_and_non_tool_messages_untouched() {
        let big = "y".repeat(10_000);
        let messages = vec![
            json!({ "role": "assistant", "content": big.clone() }), // 非 tool：不动
            tool_msg("small"),                                        // 阈值内：不动
            tool_msg(&big),                                           // 最近 8 条内：不动
        ];
        let out = snip_old_tool_results(&messages, 8, 4_000);
        assert_eq!(out, messages);
    }

    #[test]
    fn snip_handles_multibyte_content_without_panic() {
        let big = "中文字符串".repeat(2_000); // 10k chars, 多字节
        let mut messages = vec![tool_msg(&big)];
        for _ in 0..8 {
            messages.push(json!({ "role": "user", "content": "近" }));
        }
        let out = snip_old_tool_results(&messages, 8, 4_000);
        assert!(out[0]["content"].as_str().unwrap().contains("chars snipped"));
    }

    #[test]
    fn estimate_counts_content_and_structured_fields() {
        let messages = vec![
            json!({ "role": "user", "content": "abcd".repeat(10) }),
            json!({ "role": "assistant", "content": "", "tool_calls": [{"id": "x", "function": {"name": "f", "arguments": "{}"}}] }),
        ];
        assert!(estimate_messages_tokens(&messages) > 10);
    }

    #[test]
    fn split_protects_system_prefix_and_recent_tail() {
        let mut messages = vec![
            json!({ "role": "system", "content": "sys" }),
            json!({ "role": "system", "content": "sys2" }),
        ];
        for i in 0..12 {
            messages.push(json!({ "role": "user", "content": format!("m{i}") }));
        }
        let (sys, old, recent) = split_for_summary(&messages, 8);
        assert_eq!(sys.len(), 2);
        assert_eq!(old.len(), 4); // 12 - 8
        assert_eq!(recent.len(), 8);
        assert_eq!(old[0]["content"], "m0");
        assert_eq!(recent[0]["content"], "m4");
    }

    #[test]
    fn replace_with_summary_keeps_role_alternation_legal() {
        let sys = vec![json!({ "role": "system", "content": "sys" })];
        let recent = vec![
            json!({ "role": "user", "content": "latest question" }),
            json!({ "role": "assistant", "content": "latest answer" }),
        ];
        let out = replace_with_summary(sys, "the summary", recent);
        let roles: Vec<&str> = out
            .iter()
            .map(|message| message["role"].as_str().unwrap())
            .collect();
        assert_eq!(roles, vec!["system", "user", "assistant", "user", "assistant"]);
        assert!(out[1]["content"].as_str().unwrap().contains("the summary"));
    }
}
