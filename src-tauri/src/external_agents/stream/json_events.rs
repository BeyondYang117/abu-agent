use serde_json::Value;

use crate::external_agents::stream::usage_from_numbers;
use crate::external_agents::types::{JsonEventParser, UnifiedAgentEvent};

pub struct JsonEventStreamState {
    parser: JsonEventParser,
    cursor_text: String,
}

impl JsonEventStreamState {
    pub fn new(parser: JsonEventParser) -> Self {
        Self {
            parser,
            cursor_text: String::new(),
        }
    }

    pub fn handle_value(&mut self, value: &Value, sink: &mut dyn FnMut(UnifiedAgentEvent)) {
        match self.parser {
            JsonEventParser::Codex => self.handle_codex(value, sink),
            JsonEventParser::CursorAgent => self.handle_cursor(value, sink),
        }
    }

    fn handle_codex(&self, value: &Value, sink: &mut dyn FnMut(UnifiedAgentEvent)) {
        let obj = match value.as_object() {
            Some(o) => o,
            None => return,
        };
        let kind = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match kind {
            "thread.started" => sink(UnifiedAgentEvent::Status {
                label: "initializing".to_string(),
                model: None,
            }),
            "turn.started" => sink(UnifiedAgentEvent::Status {
                label: "running".to_string(),
                model: None,
            }),
            "item.completed" => {
                if let Some(item) = obj.get("item").and_then(|v| v.as_object()) {
                    if item.get("type").and_then(|v| v.as_str()) == Some("agent_message") {
                        if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                            sink(UnifiedAgentEvent::TextDelta {
                                delta: text.to_string(),
                            });
                        }
                    }
                }
            }
            "turn.completed" => {
                if let Some(usage) = obj.get("usage").and_then(|v| v.as_object()) {
                    let input = usage
                        .get("input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let output = usage
                        .get("output_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    sink(UnifiedAgentEvent::Usage {
                        usage: usage_from_numbers(input, output),
                    });
                }
            }
            "error" | "turn.failed" => sink(UnifiedAgentEvent::Error {
                message: value.to_string(),
                code: None,
            }),
            _ => {}
        }
    }

    fn handle_cursor(&mut self, value: &Value, sink: &mut dyn FnMut(UnifiedAgentEvent)) {
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
                        model: None,
                    });
                }
            }
            "assistant" => {
                if obj.get("timestamp_ms").is_some() {
                    if let Some(message) = obj.get("message").and_then(|v| v.as_object()) {
                        if let Some(content) = message.get("content").and_then(|v| v.as_array()) {
                            for block in content {
                                if let Some(text) = block
                                    .get("text")
                                    .and_then(|v| v.as_str())
                                    .or_else(|| block.as_str())
                                {
                                    self.cursor_text.push_str(text);
                                    sink(UnifiedAgentEvent::TextDelta {
                                        delta: text.to_string(),
                                    });
                                }
                            }
                        }
                    } else if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                        sink(UnifiedAgentEvent::TextDelta {
                            delta: text.to_string(),
                        });
                    }
                }
            }
            "result" => {
                if let Some(usage) = obj.get("usage").and_then(|v| v.as_object()) {
                    let input = usage
                        .get("input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let output = usage
                        .get("output_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    sink(UnifiedAgentEvent::Usage {
                        usage: usage_from_numbers(input, output),
                    });
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_agent_message_emits_text_delta() {
        let raw = r#"{"type":"item.completed","item":{"type":"agent_message","text":"hello"}}"#;
        let value: Value = serde_json::from_str(raw).unwrap();
        let mut events = Vec::new();
        JsonEventStreamState::new(JsonEventParser::Codex)
            .handle_value(&value, &mut |e| events.push(e));
        assert!(matches!(
            events.first(),
            Some(UnifiedAgentEvent::TextDelta { delta }) if delta == "hello"
        ));
    }
}
