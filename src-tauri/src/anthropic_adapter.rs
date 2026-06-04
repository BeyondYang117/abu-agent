use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;

use crate::chat::commands::PendingToolCall;

const ANTHROPIC_VERSION: &str = "2023-06-01";

/// 构造 Anthropic Messages API 请求头。
pub fn build_anthropic_headers(api_key: &str) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-api-key",
        HeaderValue::from_str(api_key).map_err(|e| format!("Invalid API key: {e}"))?,
    );
    headers.insert(
        "anthropic-version",
        HeaderValue::from_static(ANTHROPIC_VERSION),
    );
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    Ok(headers)
}

/// 构造 Anthropic Messages API URL。
pub fn build_anthropic_url(base_url: &str) -> String {
    format!("{}/messages", base_url.trim_end_matches('/'))
}

/// 将 OpenAI 格式的 messages 转换为 Anthropic 格式。
///
/// 返回 (system_prompt, anthropic_messages)。
/// Anthropic 要求 system prompt 作为独立顶层参数，不是 message。
pub fn convert_messages_to_anthropic(messages: &[Value]) -> (String, Vec<Value>) {
    let mut system_parts: Vec<String> = Vec::new();
    let mut anthropic_msgs: Vec<Value> = Vec::new();

    for msg in messages {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        match role {
            "system" => {
                if let Some(content) = extract_text_content(msg) {
                    system_parts.push(content);
                }
            }
            "user" => {
                anthropic_msgs.push(convert_user_message(msg));
            }
            "assistant" => {
                if let Some(converted) = convert_assistant_message(msg) {
                    anthropic_msgs.push(converted);
                }
            }
            "tool" => {
                convert_tool_result_message(msg, &mut anthropic_msgs);
            }
            _ => {}
        }
    }

    // 合并连续同角色消息（Anthropic 要求严格 user/assistant 交替）
    merge_consecutive_roles(&mut anthropic_msgs);

    (system_parts.join("\n\n"), anthropic_msgs)
}

/// 将 OpenAI 格式的 tools 转换为 Anthropic 格式。
///
/// OpenAI: `{type:"function", function:{name, description, parameters}}`
/// Anthropic: `{name, description, input_schema}`
pub fn convert_tools_to_anthropic(tools: &[Value]) -> Vec<Value> {
    let mut result = Vec::new();
    let mut seen_names = std::collections::HashSet::new();

    for tool in tools {
        // OpenAI 格式
        if let Some(function) = tool.get("function") {
            let name = function.get("name").and_then(|n| n.as_str()).unwrap_or("");
            if name.is_empty() {
                continue;
            }
            if !seen_names.insert(name.to_string()) {
                continue; // 去重
            }
            let description = function
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("");
            let parameters = function
                .get("parameters")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}));

            result.push(serde_json::json!({
                "name": name,
                "description": description,
                "input_schema": normalize_schema(parameters),
            }));
        }
        // 已经是 Anthropic 格式（带 input_schema）
        else if let Some(name) = tool.get("name").and_then(|n| n.as_str()) {
            if !seen_names.insert(name.to_string()) {
                continue;
            }
            result.push(tool.clone());
        }
    }

    result
}

/// 解析 Anthropic 响应，提取 content、reasoning、tool_calls 和 stop_reason。
pub fn parse_anthropic_response(response: &Value) -> AnthropicParsedResponse {
    let mut content_parts: Vec<String> = Vec::new();
    let mut reasoning_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<PendingToolCall> = Vec::new();

    if let Some(blocks) = response.get("content").and_then(|c| c.as_array()) {
        for block in blocks {
            let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match block_type {
                "text" => {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        if !text.is_empty() {
                            content_parts.push(text.to_string());
                        }
                    }
                }
                "thinking" => {
                    if let Some(thinking) = block.get("thinking").and_then(|t| t.as_str()) {
                        if !thinking.is_empty() {
                            reasoning_parts.push(thinking.to_string());
                        }
                    }
                }
                "tool_use" => {
                    let id = block
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let input = block.get("input").cloned().unwrap_or(Value::Null);
                    let arguments_raw = if input.is_null() {
                        "{}".to_string()
                    } else {
                        serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string())
                    };
                    tool_calls.push(PendingToolCall {
                        id,
                        function_name: name,
                        arguments: input,
                        arguments_raw,
                        arguments_parse_error: None,
                    });
                }
                _ => {}
            }
        }
    }

    let stop_reason = response
        .get("stop_reason")
        .and_then(|s| s.as_str())
        .unwrap_or("end_turn");

    let finish_reason = match stop_reason {
        "end_turn" => "stop",
        "tool_use" => "tool_calls",
        "max_tokens" => "length",
        _ => "stop",
    };

    AnthropicParsedResponse {
        content: content_parts.join("\n\n"),
        reasoning: if reasoning_parts.is_empty() {
            None
        } else {
            Some(reasoning_parts.join("\n\n"))
        },
        tool_calls,
        finish_reason: finish_reason.to_string(),
    }
}

/// Anthropic SSE 流式事件解析器。
///
/// 解析 Anthropic 的 SSE 事件格式，返回增量内容。
pub fn parse_anthropic_sse_event(line: &str) -> Option<AnthropicSseEvent> {
    let line = line.trim();
    if !line.starts_with("data:") {
        return None;
    }
    let data = line.trim_start_matches("data:").trim();
    if data.is_empty() {
        return None;
    }
    let value: Value = serde_json::from_str(data).ok()?;
    let event_type = value.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match event_type {
        "content_block_start" => {
            let block = value.get("content_block")?;
            let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if block_type == "tool_use" {
                Some(AnthropicSseEvent::ToolUseStart {
                    id: block
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    name: block
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
            } else {
                None
            }
        }
        "content_block_delta" => {
            let delta = value.get("delta")?;
            let delta_type = delta.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match delta_type {
                "text_delta" => {
                    let text = delta.get("text").and_then(|t| t.as_str()).unwrap_or("");
                    if text.is_empty() {
                        None
                    } else {
                        Some(AnthropicSseEvent::TextDelta(text.to_string()))
                    }
                }
                "thinking_delta" => {
                    let thinking = delta.get("thinking").and_then(|t| t.as_str()).unwrap_or("");
                    if thinking.is_empty() {
                        None
                    } else {
                        Some(AnthropicSseEvent::ThinkingDelta(thinking.to_string()))
                    }
                }
                "input_json_delta" => {
                    let json = delta
                        .get("partial_json")
                        .and_then(|t| t.as_str())
                        .unwrap_or("");
                    Some(AnthropicSseEvent::ToolInputDelta(json.to_string()))
                }
                _ => None,
            }
        }
        "content_block_stop" => Some(AnthropicSseEvent::ContentBlockStop),
        "message_stop" => Some(AnthropicSseEvent::MessageStop),
        "message_delta" => {
            // 包含 stop_reason 等顶层信息
            let delta = value.get("delta")?;
            let stop_reason = delta.get("stop_reason").and_then(|s| s.as_str());
            if let Some(reason) = stop_reason {
                Some(AnthropicSseEvent::MessageStopWithReason(reason.to_string()))
            } else {
                None
            }
        }
        "error" => {
            let error = value
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown Anthropic error");
            Some(AnthropicSseEvent::Error(error.to_string()))
        }
        _ => None,
    }
}

/// 将 Anthropic 流式 tool_use 块组装为 PendingToolCall。
pub fn assemble_tool_call_from_stream(
    id: &str,
    name: &str,
    input_json_parts: &[String],
) -> PendingToolCall {
    let raw = input_json_parts.join("");
    let (arguments, arguments_parse_error) = if raw.is_empty() {
        (Value::Object(serde_json::Map::new()), None)
    } else {
        crate::chat::commands::parse_tool_arguments(&raw)
    };
    PendingToolCall {
        id: id.to_string(),
        function_name: name.to_string(),
        arguments,
        arguments_raw: raw,
        arguments_parse_error,
    }
}

// ===== 内部辅助函数 =====

fn extract_text_content(msg: &Value) -> Option<String> {
    match msg.get("content")? {
        Value::String(s) => Some(s.clone()),
        Value::Array(parts) => {
            let texts: Vec<&str> = parts
                .iter()
                .filter_map(|p| {
                    if p.get("type").and_then(|t| t.as_str()) == Some("text") {
                        p.get("text").and_then(|t| t.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            if texts.is_empty() {
                None
            } else {
                Some(texts.join("\n"))
            }
        }
        _ => None,
    }
}

fn convert_user_message(msg: &Value) -> Value {
    let content = msg.get("content").cloned().unwrap_or(Value::Null);
    // 处理 OpenAI 多模态格式 (image_url)
    if let Value::Array(parts) = &content {
        let anthropic_parts: Vec<Value> = parts
            .iter()
            .map(|part| {
                let part_type = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match part_type {
                    "text" => serde_json::json!({
                        "type": "text",
                        "text": part.get("text").and_then(|t| t.as_str()).unwrap_or("")
                    }),
                    "image_url" => {
                        let url = part
                            .get("image_url")
                            .and_then(|u| u.get("url"))
                            .and_then(|u| u.as_str())
                            .unwrap_or("");
                        if url.starts_with("data:") {
                            // data:image/png;base64,xxx
                            parse_data_url(url)
                        } else {
                            serde_json::json!({
                                "type": "image",
                                "source": { "type": "url", "url": url }
                            })
                        }
                    }
                    _ => part.clone(),
                }
            })
            .collect();
        serde_json::json!({ "role": "user", "content": anthropic_parts })
    } else {
        serde_json::json!({ "role": "user", "content": content })
    }
}

fn parse_data_url(url: &str) -> Value {
    // data:image/png;base64,xxxx
    let without_prefix = url.strip_prefix("data:").unwrap_or(url);
    if let Some((media, data)) = without_prefix.split_once(',') {
        let media_type = media.split(';').next().unwrap_or("image/png");
        let base64_data = data.trim();
        serde_json::json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": media_type,
                "data": base64_data
            }
        })
    } else {
        serde_json::json!({
            "type": "image",
            "source": { "type": "url", "url": url }
        })
    }
}

fn convert_assistant_message(msg: &Value) -> Option<Value> {
    let mut blocks: Vec<Value> = Vec::new();

    // 推理内容
    if let Some(reasoning) = msg
        .get("reasoning_content")
        .or_else(|| msg.get("reasoning"))
        .and_then(|r| r.as_str())
        .filter(|r| !r.is_empty())
    {
        blocks.push(serde_json::json!({
            "type": "thinking",
            "thinking": reasoning
        }));
    }

    // 文本内容
    if let Some(content) = msg
        .get("content")
        .and_then(|c| c.as_str())
        .filter(|c| !c.is_empty())
    {
        blocks.push(serde_json::json!({
            "type": "text",
            "text": content
        }));
    }

    // tool_calls → tool_use 块
    if let Some(tool_calls) = msg.get("tool_calls").and_then(|tc| tc.as_array()) {
        for tc in tool_calls {
            let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let function = tc.get("function");
            let name = function
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let arguments_raw = function
                .and_then(|f| f.get("arguments"))
                .and_then(|a| a.as_str())
                .unwrap_or("{}");
            let input: Value = serde_json::from_str(arguments_raw).unwrap_or(Value::Null);

            blocks.push(serde_json::json!({
                "type": "tool_use",
                "id": id,
                "name": name,
                "input": input
            }));
        }
    }

    if blocks.is_empty() {
        // 空 assistant 消息，Anthropic 需要至少一个块
        blocks.push(serde_json::json!({
            "type": "text",
            "text": ""
        }));
    }

    Some(serde_json::json!({
        "role": "assistant",
        "content": blocks
    }))
}

fn convert_tool_result_message(msg: &Value, output: &mut Vec<Value>) {
    let tool_call_id = msg
        .get("tool_call_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");

    // Anthropic 要求 tool_result 放在 user 消息中
    // 尝试追加到上一个 user 消息，否则新建
    let tool_result = serde_json::json!({
        "type": "tool_result",
        "tool_use_id": tool_call_id,
        "content": content
    });

    if let Some(last) = output.last_mut() {
        if last.get("role").and_then(|r| r.as_str()) == Some("user") {
            if let Some(content_arr) = last.get_mut("content").and_then(|c| c.as_array_mut()) {
                content_arr.push(tool_result);
                return;
            }
        }
    }

    output.push(serde_json::json!({
        "role": "user",
        "content": [tool_result]
    }));
}

fn merge_consecutive_roles(messages: &mut Vec<Value>) {
    if messages.len() < 2 {
        return;
    }
    let mut i = 1;
    while i < messages.len() {
        let prev_role = messages[i - 1]
            .get("role")
            .and_then(|r| r.as_str())
            .unwrap_or("");
        let curr_role = messages[i]
            .get("role")
            .and_then(|r| r.as_str())
            .unwrap_or("");
        if prev_role == curr_role {
            // 合并到前一条
            let curr_content = messages[i].get("content").cloned().unwrap_or(Value::Null);
            let prev_content = messages[i - 1].get_mut("content").unwrap();

            let mut prev_parts: Vec<Value> = match prev_content {
                Value::Array(arr) => arr.clone(),
                Value::String(s) => vec![serde_json::json!({"type": "text", "text": s})],
                _ => vec![],
            };
            let curr_parts: Vec<Value> = match curr_content {
                Value::Array(arr) => arr,
                Value::String(s) => vec![serde_json::json!({"type": "text", "text": s})],
                _ => vec![],
            };
            prev_parts.extend(curr_parts);
            messages[i - 1]["content"] = Value::Array(prev_parts);
            messages.remove(i);
        } else {
            i += 1;
        }
    }
}

fn normalize_schema(schema: Value) -> Value {
    // Anthropic 不支持 nullable union，简化 anyOf: [{type: "string"}, {type: "null"}]
    if let Some(any_of) = schema.get("anyOf").and_then(|a| a.as_array()) {
        if any_of.len() == 2 {
            let has_null = any_of
                .iter()
                .any(|item| item.get("type").and_then(|t| t.as_str()) == Some("null"));
            if has_null {
                let non_null = any_of
                    .iter()
                    .find(|item| item.get("type").and_then(|t| t.as_str()) != Some("null"));
                if let Some(inner) = non_null {
                    let mut result = inner.clone();
                    // 保留原始字段
                    if let Some(desc) = schema.get("description") {
                        result["description"] = desc.clone();
                    }
                    return result;
                }
            }
        }
    }

    // 确保 object 类型有 properties
    if schema.get("type").and_then(|t| t.as_str()) == Some("object") {
        if schema.get("properties").is_none() {
            let mut result = schema.clone();
            result["properties"] = serde_json::json!({});
            return result;
        }
    }

    schema
}

// ===== 公开类型 =====

pub struct AnthropicParsedResponse {
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls: Vec<PendingToolCall>,
    pub finish_reason: String,
}

pub enum AnthropicSseEvent {
    TextDelta(String),
    ThinkingDelta(String),
    ToolUseStart { id: String, name: String },
    ToolInputDelta(String),
    ContentBlockStop,
    MessageStop,
    MessageStopWithReason(String),
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_messages_extracts_system() {
        let messages = vec![
            serde_json::json!({"role": "system", "content": "You are helpful."}),
            serde_json::json!({"role": "user", "content": "Hello"}),
        ];
        let (system, msgs) = convert_messages_to_anthropic(&messages);
        assert_eq!(system, "You are helpful.");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
    }

    #[test]
    fn convert_tools_unwraps_function_wrapper() {
        let tools = vec![serde_json::json!({
            "type": "function",
            "function": {
                "name": "skill_activate",
                "description": "Activate a skill",
                "parameters": {"type": "object", "properties": {"name": {"type": "string"}}}
            }
        })];
        let result = convert_tools_to_anthropic(&tools);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["name"], "skill_activate");
        assert_eq!(result[0]["description"], "Activate a skill");
        assert!(result[0]["input_schema"].is_object());
        assert!(result[0]["type"].is_null()); // 没有 type: "function" 包装
    }

    #[test]
    fn convert_tools_deduplicates() {
        let tools = vec![
            serde_json::json!({"type": "function", "function": {"name": "foo", "parameters": {}}}),
            serde_json::json!({"type": "function", "function": {"name": "foo", "parameters": {}}}),
        ];
        let result = convert_tools_to_anthropic(&tools);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn parse_response_extracts_tool_use_blocks() {
        let response = serde_json::json!({
            "content": [
                {"type": "text", "text": "I'll activate the skill."},
                {"type": "tool_use", "id": "toolu_123", "name": "skill_activate", "input": {"name": "tavily"}}
            ],
            "stop_reason": "tool_use"
        });
        let parsed = parse_anthropic_response(&response);
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].function_name, "skill_activate");
        assert_eq!(parsed.finish_reason, "tool_calls");
    }

    #[test]
    fn parse_response_extracts_thinking() {
        let response = serde_json::json!({
            "content": [
                {"type": "thinking", "thinking": "Let me think..."},
                {"type": "text", "text": "The answer is 42."}
            ],
            "stop_reason": "end_turn"
        });
        let parsed = parse_anthropic_response(&response);
        assert_eq!(parsed.reasoning, Some("Let me think...".to_string()));
        assert_eq!(parsed.content, "The answer is 42.");
        assert_eq!(parsed.finish_reason, "stop");
    }

    #[test]
    fn parse_sse_text_delta() {
        let event = parse_anthropic_sse_event("data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}");
        match event {
            Some(AnthropicSseEvent::TextDelta(text)) => assert_eq!(text, "Hello"),
            _ => panic!("Expected TextDelta"),
        }
    }

    #[test]
    fn parse_sse_tool_use_start() {
        let event = parse_anthropic_sse_event("data: {\"type\":\"content_block_start\",\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_123\",\"name\":\"skill_activate\"}}");
        match event {
            Some(AnthropicSseEvent::ToolUseStart { id, name }) => {
                assert_eq!(id, "toolu_123");
                assert_eq!(name, "skill_activate");
            }
            _ => panic!("Expected ToolUseStart"),
        }
    }

    #[test]
    fn merge_consecutive_roles_combines_user_messages() {
        let mut msgs = vec![
            serde_json::json!({"role": "user", "content": [{"type": "text", "text": "hello"}]}),
            serde_json::json!({"role": "user", "content": [{"type": "tool_result", "tool_use_id": "123", "content": "ok"}]}),
            serde_json::json!({"role": "assistant", "content": [{"type": "text", "text": "hi"}]}),
        ];
        merge_consecutive_roles(&mut msgs);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[1]["role"], "assistant");
    }

    #[test]
    fn normalize_schema_simplifies_nullable_union() {
        let schema = serde_json::json!({
            "anyOf": [{"type": "string"}, {"type": "null"}],
            "description": "optional string"
        });
        let result = normalize_schema(schema);
        assert_eq!(result["type"], "string");
        assert_eq!(result["description"], "optional string");
    }

    #[test]
    fn normalize_schema_adds_properties_to_object() {
        let schema = serde_json::json!({"type": "object"});
        let result = normalize_schema(schema);
        assert!(result["properties"].is_object());
    }
}
