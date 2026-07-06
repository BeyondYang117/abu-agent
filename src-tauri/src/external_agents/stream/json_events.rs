use serde_json::Value;

use crate::external_agents::types::{JsonEventParser, UnifiedAgentEvent};

pub struct JsonEventStreamState {
    parser: JsonEventParser,
}

impl JsonEventStreamState {
    pub fn new(parser: JsonEventParser) -> Self {
        Self { parser }
    }

    pub fn handle_value(&mut self, value: &Value, sink: &mut dyn FnMut(UnifiedAgentEvent)) {
        match self.parser {
            JsonEventParser::Kimi => self.handle_kimi(value, sink),
        }
    }

    fn handle_kimi(&self, value: &Value, sink: &mut dyn FnMut(UnifiedAgentEvent)) {
        let obj = match value.as_object() {
            Some(o) => o,
            None => return,
        };
        let role = obj.get("role").and_then(|v| v.as_str()).unwrap_or("");

        if role == "assistant" {
            if let Some(calls) = obj.get("tool_calls").and_then(|v| v.as_array()) {
                for raw_call in calls {
                    let call = raw_call.as_object();
                    let func = call
                        .and_then(|c| c.get("function"))
                        .and_then(|v| v.as_object());
                    let id = call
                        .and_then(|c| c.get("id"))
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.trim().is_empty());
                    let name = func
                        .and_then(|f| f.get("name"))
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.trim().is_empty());
                    if let (Some(id), Some(name)) = (id, name) {
                        let input = func
                            .and_then(|f| f.get("arguments"))
                            .cloned()
                            .unwrap_or(Value::Null);
                        sink(UnifiedAgentEvent::ToolUse {
                            id: id.to_string(),
                            name: name.to_string(),
                            input,
                        });
                    }
                }
                return;
            }
            if let Some(text) = obj.get("content").and_then(|v| v.as_str()) {
                if !text.is_empty() {
                    sink(UnifiedAgentEvent::TextDelta {
                        delta: text.to_string(),
                    });
                }
            }
            return;
        }

        if role == "tool" {
            if let Some(tool_use_id) = obj.get("tool_call_id").and_then(|v| v.as_str()) {
                if !tool_use_id.trim().is_empty() {
                    let content = obj
                        .get("content")
                        .map(stringify_json_value)
                        .unwrap_or_default();
                    sink(UnifiedAgentEvent::ToolResult {
                        tool_use_id: tool_use_id.trim().to_string(),
                        content,
                        is_error: false,
                    });
                }
            }
        }
    }
}

fn stringify_json_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kimi_assistant_content_emits_delta() {
        let raw = r#"{"role":"assistant","content":"world"}"#;
        let value: Value = serde_json::from_str(raw).unwrap();
        let mut events = Vec::new();
        JsonEventStreamState::new(JsonEventParser::Kimi)
            .handle_value(&value, &mut |e| events.push(e));
        assert!(matches!(
            events.first(),
            Some(UnifiedAgentEvent::TextDelta { delta }) if delta == "world"
        ));
    }
}
