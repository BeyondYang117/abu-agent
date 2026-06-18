use serde_json::Value;

use crate::external_agents::stream::usage_from_numbers;
use crate::external_agents::types::UnifiedAgentEvent;

#[derive(Default)]
pub struct ClaudeStreamState {
    text_streamed: bool,
}

impl ClaudeStreamState {
    pub fn handle_value(&mut self, value: &Value, sink: &mut dyn FnMut(UnifiedAgentEvent)) {
        let obj = match value.as_object() {
            Some(o) => o,
            None => return,
        };
        let kind = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match kind {
            "system" => {
                if obj.get("subtype").and_then(|v| v.as_str()) == Some("init") {
                    sink(UnifiedAgentEvent::Status {
                        label: "initializing".to_string(),
                        model: obj
                            .get("model")
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                    });
                }
            }
            "stream_event" => {
                if let Some(event) = obj.get("event").and_then(|v| v.as_object()) {
                    let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    match event_type {
                        "content_block_delta" => {
                            if let Some(delta) = event.get("delta").and_then(|v| v.as_object()) {
                                match delta.get("type").and_then(|v| v.as_str()) {
                                    Some("text_delta") => {
                                        if let Some(text) = delta.get("text").and_then(|v| v.as_str())
                                        {
                                            self.text_streamed = true;
                                            sink(UnifiedAgentEvent::TextDelta {
                                                delta: text.to_string(),
                                            });
                                        }
                                    }
                                    Some("thinking_delta") => {
                                        if let Some(text) =
                                            delta.get("thinking").and_then(|v| v.as_str())
                                        {
                                            sink(UnifiedAgentEvent::ThinkingDelta {
                                                delta: text.to_string(),
                                            });
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            "assistant" => {
                if let Some(message) = obj.get("message").and_then(|v| v.as_object()) {
                    if let Some(content) = message.get("content").and_then(|v| v.as_array()) {
                        for block in content {
                            let block = match block.as_object() {
                                Some(b) => b,
                                None => continue,
                            };
                            match block.get("type").and_then(|v| v.as_str()) {
                                Some("text") => {
                                    if !self.text_streamed {
                                        if let Some(text) =
                                            block.get("text").and_then(|v| v.as_str())
                                        {
                                            sink(UnifiedAgentEvent::TextDelta {
                                                delta: text.to_string(),
                                            });
                                        }
                                    }
                                }
                                Some("tool_use") => {
                                    sink(UnifiedAgentEvent::ToolUse {
                                        id: block
                                            .get("id")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("tool")
                                            .to_string(),
                                        name: block
                                            .get("name")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("tool")
                                            .to_string(),
                                        input: block
                                            .get("input")
                                            .cloned()
                                            .unwrap_or(Value::Null),
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            "user" => {
                if let Some(message) = obj.get("message").and_then(|v| v.as_object()) {
                    if let Some(content) = message.get("content").and_then(|v| v.as_array()) {
                        for block in content {
                            let block = match block.as_object() {
                                Some(b) => b,
                                None => continue,
                            };
                            if block.get("type").and_then(|v| v.as_str()) == Some("tool_result") {
                                sink(UnifiedAgentEvent::ToolResult {
                                    tool_use_id: block
                                        .get("tool_use_id")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                    content: block
                                        .get("content")
                                        .map(|v| {
                                            if let Some(s) = v.as_str() {
                                                s.to_string()
                                            } else {
                                                v.to_string()
                                            }
                                        })
                                        .unwrap_or_default(),
                                    is_error: block
                                        .get("is_error")
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(false),
                                });
                            }
                        }
                    }
                }
            }
            "result" => {
                let usage = obj.get("usage").and_then(|u| u.as_object());
                let input = usage
                    .and_then(|u| u.get("input_tokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let output = usage
                    .and_then(|u| u.get("output_tokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                sink(UnifiedAgentEvent::Usage {
                    usage: usage_from_numbers(input, output),
                });
                sink(UnifiedAgentEvent::TurnEnd {
                    stop_reason: obj
                        .get("subtype")
                        .and_then(|v| v.as_str())
                        .unwrap_or("completed")
                        .to_string(),
                });
            }
            "error" => {
                sink(UnifiedAgentEvent::Error {
                    message: obj
                        .get("error")
                        .and_then(|v| v.as_str())
                        .or_else(|| obj.get("message").and_then(|v| v.as_str()))
                        .unwrap_or("unknown error")
                        .to_string(),
                    code: obj
                        .get("code")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_delta_from_stream_event() {
        let raw = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"hi"}}}"#;
        let value: Value = serde_json::from_str(raw).unwrap();
        let mut events = Vec::new();
        ClaudeStreamState::default().handle_value(&value, &mut |e| events.push(e));
        assert!(matches!(
            events.first(),
            Some(UnifiedAgentEvent::TextDelta { delta }) if delta == "hi"
        ));
    }
}
