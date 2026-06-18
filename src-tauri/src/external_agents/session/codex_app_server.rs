use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Child;
use tokio::time::timeout;

use crate::external_agents::stream::usage_from_numbers;
use crate::external_agents::types::UnifiedAgentEvent;

/// Codex `app-server` speaks newline-delimited JSON-RPC over stdio (one JSON object per line,
/// no `Content-Length` framing). Responses omit the `jsonrpc` field, so we never require it.
async fn write_rpc(
    stdin: &mut tokio::process::ChildStdin,
    id: u64,
    method: &str,
    params: Value,
) -> Result<(), String> {
    let payload = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    let mut line = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| e.to_string())
}

async fn write_rpc_result(
    stdin: &mut tokio::process::ChildStdin,
    id: &Value,
    result: Value,
) -> Result<(), String> {
    let payload = json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    });
    let mut line = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| e.to_string())
}

/// Server → client approval requests are auto-approved. Each request method maps to a different
/// response shape (see the `*RequestApprovalResponse` schemas); return the matching approve value.
fn approval_response(method: &str) -> Option<Value> {
    match method {
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
            Some(json!({ "decision": "acceptForSession" }))
        }
        // Legacy exec/apply-patch approval requests use ReviewDecision.
        "execCommandApproval" | "applyPatchApproval" => {
            Some(json!({ "decision": "approved_for_session" }))
        }
        "item/permissions/requestApproval" => {
            Some(json!({ "permissions": {}, "scope": "session" }))
        }
        _ => None,
    }
}

/// Map a single codex app-server notification to zero or more `UnifiedAgentEvent`s. Returns `true`
/// when the notification signals the turn has ended (completed / failed).
fn map_codex_notification(
    method: &str,
    params: &Value,
    emitted_tools: &mut HashSet<String>,
    sink: &mut dyn FnMut(UnifiedAgentEvent),
) -> bool {
    match method {
        "item/agentMessage/delta" => {
            if let Some(delta) = params.get("delta").and_then(|v| v.as_str()) {
                if !delta.is_empty() {
                    sink(UnifiedAgentEvent::TextDelta {
                        delta: delta.to_string(),
                    });
                }
            }
        }
        "item/reasoning/summaryTextDelta" | "item/reasoning/textDelta" => {
            if let Some(delta) = params.get("delta").and_then(|v| v.as_str()) {
                if !delta.is_empty() {
                    sink(UnifiedAgentEvent::ThinkingDelta {
                        delta: delta.to_string(),
                    });
                }
            }
        }
        "item/commandExecution/outputDelta" => {
            // Output streamed before the item completes; the completed item carries the
            // aggregated output we surface as the tool result, so deltas are not re-emitted.
        }
        "item/started" => {
            if let Some(item) = params.get("item").and_then(|v| v.as_object()) {
                emit_command_execution(item, emitted_tools, sink, false);
            }
        }
        "item/completed" => {
            if let Some(item) = params.get("item").and_then(|v| v.as_object()) {
                emit_command_execution(item, emitted_tools, sink, true);
            }
        }
        "thread/tokenUsage/updated" => {
            if let Some(usage) = params
                .get("tokenUsage")
                .and_then(|v| v.get("total"))
                .and_then(|v| v.as_object())
            {
                let input = usage.get("inputTokens").and_then(|v| v.as_u64()).unwrap_or(0);
                let output = usage
                    .get("outputTokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                if input > 0 || output > 0 {
                    sink(UnifiedAgentEvent::Usage {
                        usage: usage_from_numbers(input, output),
                    });
                }
            }
        }
        "turn/completed" => {
            if let Some(turn) = params.get("turn").and_then(|v| v.as_object()) {
                if turn.get("status").and_then(|v| v.as_str()) == Some("failed") {
                    let message = turn
                        .get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("Codex turn failed");
                    sink(UnifiedAgentEvent::Error {
                        message: message.to_string(),
                        code: None,
                    });
                }
            }
            return true;
        }
        // There is no `turn/failed` notification in the app-server protocol; failures arrive
        // either as a failed `turn/completed` (handled above) or as a top-level `error` /
        // `thread/realtime/error` notification. Surface those and end the loop.
        "error" | "thread/realtime/error" => {
            let message = params
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|v| v.as_str())
                .or_else(|| params.get("message").and_then(|v| v.as_str()))
                .unwrap_or("Codex error");
            sink(UnifiedAgentEvent::Error {
                message: message.to_string(),
                code: None,
            });
            return true;
        }
        _ => {}
    }
    false
}

/// A `commandExecution` ThreadItem (camelCase wire shape) maps to a Bash tool use / result.
fn emit_command_execution(
    item: &serde_json::Map<String, Value>,
    emitted_tools: &mut HashSet<String>,
    sink: &mut dyn FnMut(UnifiedAgentEvent),
    include_result: bool,
) {
    if item.get("type").and_then(|v| v.as_str()) != Some("commandExecution") {
        return;
    }
    let id = match item.get("id").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        Some(id) => id.to_string(),
        None => return,
    };
    let command = item
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if emitted_tools.insert(id.clone()) {
        sink(UnifiedAgentEvent::ToolUse {
            id: id.clone(),
            name: "Bash".to_string(),
            input: json!({ "command": command }),
        });
    }
    if !include_result {
        return;
    }
    let content = item
        .get("aggregatedOutput")
        .map(|value| match value {
            Value::String(s) => s.clone(),
            _ => value.to_string(),
        })
        .unwrap_or_default();
    let exit_code = item.get("exitCode").and_then(|v| v.as_i64());
    let status_failed = matches!(
        item.get("status").and_then(|v| v.as_str()),
        Some("failed") | Some("declined")
    );
    let is_error = exit_code.map(|code| code != 0).unwrap_or(status_failed);
    sink(UnifiedAgentEvent::ToolResult {
        tool_use_id: id,
        content,
        is_error,
    });
}

/// Drive a single Codex turn over the app-server JSON-RPC protocol:
/// `initialize` → `thread/start` (capture threadId) → `turn/start` → consume notifications until
/// the turn completes/fails. Approval requests are auto-approved; cancellation sends
/// `turn/interrupt` and returns `Err`.
pub async fn run_codex_app_server_session(
    child: &mut Child,
    prompt: &str,
    model: Option<&str>,
    reasoning: Option<&str>,
    cwd: &Path,
    mut sink: impl FnMut(UnifiedAgentEvent),
    cancel_check: impl Fn() -> bool,
) -> Result<(), String> {
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "stdin unavailable".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "stdout unavailable".to_string())?;

    let cwd_str = cwd.to_string_lossy().to_string();
    let chosen_model = model.filter(|m| !m.is_empty() && *m != "default");
    let chosen_effort = reasoning.filter(|r| !r.is_empty() && *r != "default");

    sink(UnifiedAgentEvent::Status {
        label: "initializing".to_string(),
        model: chosen_model.map(str::to_string),
    });

    write_rpc(
        &mut stdin,
        1,
        "initialize",
        json!({
            "clientInfo": { "name": "kivio", "title": "kivio", "version": "0" },
        }),
    )
    .await?;

    let mut reader = BufReader::new(stdout).lines();

    // Request IDs: 1=initialize, 2=thread/start, 3=turn/start.
    let thread_start_id: u64 = 2;
    let turn_start_id: u64 = 3;
    let interrupt_id: u64 = 4;
    let mut thread_id: Option<String> = None;
    let mut turn_started = false;
    let mut emitted_tools: HashSet<String> = HashSet::new();
    let mut finished = false;

    while !finished {
        if cancel_check() {
            if let Some(ref tid) = thread_id {
                let _ = write_rpc(
                    &mut stdin,
                    interrupt_id,
                    "turn/interrupt",
                    json!({ "threadId": tid }),
                )
                .await;
            }
            let _ = stdin.shutdown().await;
            let _ = child.start_kill();
            return Err("cancelled".to_string());
        }

        let line = match timeout(Duration::from_millis(200), reader.next_line()).await {
            Ok(Ok(Some(line))) => line,
            Ok(Ok(None)) => {
                if turn_started {
                    break;
                }
                return Err("codex app-server exited before completion".to_string());
            }
            Ok(Err(e)) => return Err(e.to_string()),
            Err(_) => continue,
        };
        if line.trim().is_empty() {
            continue;
        }

        let value: Value = match serde_json::from_str(line.trim()) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Server → client request (approval). Identified by having both `method` and `id`.
        if let (Some(method), Some(id)) = (
            value.get("method").and_then(|v| v.as_str()),
            value.get("id"),
        ) {
            if let Some(result) = approval_response(method) {
                write_rpc_result(&mut stdin, id, result).await?;
                continue;
            }
            // Unknown server request: nothing actionable.
            continue;
        }

        // Server → client notification (no id, has method).
        if let Some(method) = value.get("method").and_then(|v| v.as_str()) {
            let params = value.get("params").cloned().unwrap_or(Value::Null);
            if map_codex_notification(method, &params, &mut emitted_tools, &mut sink) {
                finished = true;
                let _ = stdin.shutdown().await;
            }
            continue;
        }

        // Response to one of our requests.
        if let Some(err) = value.get("error") {
            let message = err
                .get("message")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| err.to_string());
            return Err(message);
        }

        let id = value.get("id").and_then(|v| v.as_u64());
        let result = value.get("result");
        if id == Some(thread_start_id) {
            thread_id = result
                .and_then(|r| r.get("thread"))
                .and_then(|t| t.get("id"))
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let tid = match thread_id.clone() {
                Some(tid) => tid,
                None => return Err("invalid thread/start response".to_string()),
            };
            let mut turn_params = json!({
                "threadId": tid,
                "input": [{ "type": "text", "text": prompt }],
                "cwd": cwd_str,
                "approvalPolicy": "never",
            });
            if let Some(effort) = chosen_effort {
                turn_params["effort"] = json!(effort);
            }
            if let Some(model) = chosen_model {
                turn_params["model"] = json!(model);
            }
            write_rpc(&mut stdin, turn_start_id, "turn/start", turn_params).await?;
            sink(UnifiedAgentEvent::Status {
                label: "running".to_string(),
                model: None,
            });
            continue;
        }
        if id == Some(turn_start_id) {
            turn_started = true;
            continue;
        }
        // id == initialize: send thread/start.
        if id == Some(1) {
            let mut params = json!({
                "cwd": cwd_str,
                "sandbox": "workspace-write",
                "approvalPolicy": "never",
            });
            if let Some(model) = chosen_model {
                params["model"] = json!(model);
            }
            write_rpc(&mut stdin, thread_start_id, "thread/start", params).await?;
            continue;
        }
    }

    let _ = sink(UnifiedAgentEvent::TurnEnd {
        stop_reason: "completed".to_string(),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(method: &str, raw: &str) -> (Vec<UnifiedAgentEvent>, bool) {
        let params: Value = serde_json::from_str(raw).unwrap();
        let mut events = Vec::new();
        let mut tools = HashSet::new();
        let ended = map_codex_notification(method, &params, &mut tools, &mut |e| events.push(e));
        (events, ended)
    }

    #[test]
    fn agent_message_delta_emits_text() {
        let (events, ended) = collect(
            "item/agentMessage/delta",
            r#"{"delta":"hi","itemId":"i","threadId":"t","turnId":"u"}"#,
        );
        assert!(!ended);
        assert!(matches!(
            events.first(),
            Some(UnifiedAgentEvent::TextDelta { delta }) if delta == "hi"
        ));
    }

    #[test]
    fn reasoning_deltas_emit_thinking() {
        let (summary, _) = collect(
            "item/reasoning/summaryTextDelta",
            r#"{"delta":"plan","itemId":"i","summaryIndex":0,"threadId":"t","turnId":"u"}"#,
        );
        assert!(matches!(
            summary.first(),
            Some(UnifiedAgentEvent::ThinkingDelta { delta }) if delta == "plan"
        ));
        let (text, _) = collect(
            "item/reasoning/textDelta",
            r#"{"delta":"think","contentIndex":0,"itemId":"i","threadId":"t","turnId":"u"}"#,
        );
        assert!(matches!(
            text.first(),
            Some(UnifiedAgentEvent::ThinkingDelta { delta }) if delta == "think"
        ));
    }

    #[test]
    fn command_execution_emits_tool_use_and_result() {
        let started = r#"{"item":{"type":"commandExecution","id":"cmd-1","command":"ls","status":"inProgress"},"startedAtMs":0,"threadId":"t","turnId":"u"}"#;
        let completed = r#"{"item":{"type":"commandExecution","id":"cmd-1","command":"ls","aggregatedOutput":"ok\n","exitCode":0,"status":"completed"},"completedAtMs":1,"threadId":"t","turnId":"u"}"#;
        let started_val: Value = serde_json::from_str(started).unwrap();
        let completed_val: Value = serde_json::from_str(completed).unwrap();
        let mut events = Vec::new();
        let mut tools = HashSet::new();
        map_codex_notification("item/started", &started_val, &mut tools, &mut |e| {
            events.push(e)
        });
        map_codex_notification("item/completed", &completed_val, &mut tools, &mut |e| {
            events.push(e)
        });
        assert!(matches!(
            events.first(),
            Some(UnifiedAgentEvent::ToolUse { id, name, .. }) if id == "cmd-1" && name == "Bash"
        ));
        assert!(events.iter().any(|event| matches!(
            event,
            UnifiedAgentEvent::ToolResult { tool_use_id, content, is_error }
                if tool_use_id == "cmd-1" && content.contains("ok") && !*is_error
        )));
    }

    #[test]
    fn token_usage_emits_usage() {
        let (events, _) = collect(
            "thread/tokenUsage/updated",
            r#"{"threadId":"t","turnId":"u","tokenUsage":{"last":{"cachedInputTokens":0,"inputTokens":5,"outputTokens":7,"reasoningOutputTokens":0,"totalTokens":12},"total":{"cachedInputTokens":0,"inputTokens":5,"outputTokens":7,"reasoningOutputTokens":0,"totalTokens":12}}}"#,
        );
        assert!(events
            .iter()
            .any(|event| matches!(event, UnifiedAgentEvent::Usage { .. })));
    }

    #[test]
    fn turn_completed_ends_loop() {
        let (_, ended) = collect(
            "turn/completed",
            r#"{"threadId":"t","turn":{"id":"u","items":[],"status":"completed"}}"#,
        );
        assert!(ended);
    }

    #[test]
    fn turn_failed_emits_error_and_ends() {
        let (events, ended) = collect(
            "turn/completed",
            r#"{"threadId":"t","turn":{"id":"u","items":[],"status":"failed","error":{"message":"boom"}}}"#,
        );
        assert!(ended);
        assert!(events.iter().any(|event| matches!(
            event,
            UnifiedAgentEvent::Error { message, .. } if message == "boom"
        )));
    }

    #[test]
    fn error_notification_emits_error_and_ends() {
        let (events, ended) =
            collect("error", r#"{"error":{"message":"fatal"}}"#);
        assert!(ended);
        assert!(events.iter().any(|event| matches!(
            event,
            UnifiedAgentEvent::Error { message, .. } if message == "fatal"
        )));
    }

    fn event_variant(event: &UnifiedAgentEvent) -> &'static str {
        match event {
            UnifiedAgentEvent::Status { .. } => "Status",
            UnifiedAgentEvent::TextDelta { .. } => "TextDelta",
            UnifiedAgentEvent::ThinkingDelta { .. } => "ThinkingDelta",
            UnifiedAgentEvent::ToolUse { .. } => "ToolUse",
            UnifiedAgentEvent::ToolResult { .. } => "ToolResult",
            UnifiedAgentEvent::Usage { .. } => "Usage",
            UnifiedAgentEvent::TurnEnd { .. } => "TurnEnd",
            UnifiedAgentEvent::Error { .. } => "Error",
            UnifiedAgentEvent::Raw { .. } => "Raw",
            UnifiedAgentEvent::SlashCommands { .. } => "SlashCommands",
        }
    }

    #[tokio::test]
    #[ignore = "requires live codex login + network"]
    async fn codex_app_server_smoke() {
        use tokio::process::Command;
        use tokio::time::{timeout, Duration};

        let cwd = std::env::temp_dir();
        let mut child = Command::new("codex")
            .arg("app-server")
            .current_dir(&cwd)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn codex app-server");

        let events = std::cell::RefCell::new(Vec::<UnifiedAgentEvent>::new());
        let result = timeout(
            Duration::from_secs(90),
            run_codex_app_server_session(
                &mut child,
                "Reply with exactly the token SMOKE_OK and nothing else.",
                None,
                None,
                &cwd,
                |event| events.borrow_mut().push(event),
                || false,
            ),
        )
        .await;

        let _ = child.start_kill();
        let captured = events.into_inner();
        eprintln!("=== codex app-server smoke: {} events ===", captured.len());
        for (i, ev) in captured.iter().enumerate() {
            eprintln!("[{i}] {ev:?}");
        }
        let seq: Vec<&str> = captured.iter().map(event_variant).collect();
        eprintln!("codex sequence: {seq:?}");
        match &result {
            Ok(Ok(())) => eprintln!("codex run_codex_app_server_session: Ok"),
            Ok(Err(e)) => eprintln!("codex run_codex_app_server_session: Err({e})"),
            Err(_) => panic!("codex app-server session HUNG past 90s wall-clock guard"),
        }

        let got_text = captured
            .iter()
            .any(|e| matches!(e, UnifiedAgentEvent::TextDelta { .. }));
        let got_error = captured
            .iter()
            .any(|e| matches!(e, UnifiedAgentEvent::Error { .. }))
            || matches!(&result, Ok(Err(_)));
        assert!(
            got_text || got_error,
            "expected at least one TextDelta or a clean Error, got: {seq:?}"
        );
    }

    #[test]
    fn approval_response_shapes() {
        assert_eq!(
            approval_response("item/commandExecution/requestApproval"),
            Some(json!({ "decision": "acceptForSession" }))
        );
        assert_eq!(
            approval_response("item/fileChange/requestApproval"),
            Some(json!({ "decision": "acceptForSession" }))
        );
        assert_eq!(
            approval_response("item/permissions/requestApproval"),
            Some(json!({ "permissions": {}, "scope": "session" }))
        );
        assert!(approval_response("item/started").is_none());
    }
}
