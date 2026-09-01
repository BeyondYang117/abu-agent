// ABU API Provider Adapter
//
// 实现 LanguageModelProvider trait，将请求转发到 abu-api 的 /agent/relay/* 端点

use crate::chat::model::{
    GenerateOutput, GenerateRequest, LanguageModelProvider, StreamPart, WebSearchResult,
};
use anyhow::{anyhow, bail, Context, Result};
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tokio_stream::StreamExt;

pub struct AbuApiProvider {
    base_url: String,
    relay_key: String,
    task_id: String,
    http_client: Client,
}

impl AbuApiProvider {
    pub fn new(base_url: String, relay_key: String, task_id: String) -> Self {
        Self {
            base_url,
            relay_key,
            task_id,
            http_client: Client::new(),
        }
    }

    /// 检查 Task 配额状态
    async fn check_quota(&self) -> Result<TaskUsage> {
        let url = format!(
            "/api/agent/tasks/{}/usage",
            self.base_url, self.task_id
        );

        let response = self
            .http_client
            .get(&url)
            .header("X-Abu-Session-Token", &self.relay_key)
            .send()
            .await
            .context("Failed to check task usage")?;

        if !response.status().is_success() {
            bail!("Failed to check quota: HTTP {}", response.status());
        }

        let body: ApiResponse<TaskUsage> = response
            .json()
            .await
            .context("Failed to parse usage response")?;

        if !body.success {
            bail!("Failed to check quota: {}", body.message.unwrap_or_default());
        }

        body.data.ok_or_else(|| anyhow!("No usage data returned"))
    }

    /// 发送心跳
    async fn send_heartbeat(&self) -> Result<()> {
        let url = format!(
            "{}/api/agent/tasks/{}/heartbeat",
            self.base_url, self.task_id
        );

        let response = self
            .http_client
            .post(&url)
            .header("X-Abu-Session-Token", &self.relay_key)
            .send()
            .await
            .context("Failed to send heartbeat")?;

        if !response.status().is_success() {
            bail!("Heartbeat failed: HTTP {}", response.status());
        }

        Ok(())
    }

    /// 构建中转请求 URL
    fn build_relay_url(&self, path: &str) -> String {
        format!("{}/api/agent/relay{}", self.base_url, path)
    }
}

impl LanguageModelProvider for AbuApiProvider {
    fn name(&self) -> &str {
        "abu-api"
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn stream(
        &self,
        request: GenerateRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamPart>> + Send>>> {
        // 1. 检查配额
        let usage = self.check_quota().await?;
        if usage.hard_cap_exceeded {
            bail!("Task quota exhausted (hard cap: ¥{})", usage.hard_cap as f64 / 100.0);
        }

        // 2. 发送心跳（每次请求发一次）
        let _ = self.send_heartbeat().await; // 忽略心跳失败

        // 3. 根据 request 类型选择端点
        let url = match request.api_format.as_deref() {
            Some("anthropic") => self.build_relay_url("/v1/messages"),
            Some("gemini") => {
                // Gemini 需要特殊路径处理
                let model = request.model.as_deref().unwrap_or("gemini-pro");
                self.build_relay_url(&format!("/v1beta/models/{}:generateContent", model))
            }
            Some("openai_responses") => self.build_relay_url("/v1/responses"),
            _ => self.build_relay_url("/v1/chat/completions"), // OpenAI 默认
        };

        // 4. 构建请求体（直接传递，abu-api 会处理）
        let body = serde_json::to_value(&request)?;

        // 5. 发起流式请求
        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.relay_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send relay request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            bail!("Relay request failed: HTTP {} - {}", status, error_text);
        }

        // 6. 解析 SSE 流（重用现有的 openai/anthropic/gemini 解析器）
        let stream = response.bytes_stream();
        let adapter = request.api_format.as_deref().unwrap_or("openai");

        // 根据 adapter 选择对应的解析器
        let parsed_stream = match adapter {
            "anthropic" => {
                Box::pin(parse_anthropic_stream(stream)) as Pin<Box<dyn Stream<Item = Result<StreamPart>> + Send>>
            }
            "gemini" => {
                Box::pin(parse_gemini_stream(stream))
            }
            "openai_responses" => {
                Box::pin(parse_responses_stream(stream))
            }
            _ => {
                Box::pin(parse_openai_stream(stream))
            }
        };

        Ok(parsed_stream)
    }

    async fn generate(&self, request: GenerateRequest) -> Result<GenerateOutput> {
        // 使用 stream 并收集结果
        let mut stream = self.stream(request).await?;
        let mut output = GenerateOutput::default();

        while let Some(part) = stream.next().await {
            let part = part?;
            match part {
                StreamPart::TextDelta(delta) => {
                    output.text.push_str(&delta.text);
                }
                StreamPart::ReasoningDelta(delta) => {
                    output.reasoning_content.push_str(&delta.text);
                }
                StreamPart::ToolCall(call) => {
                    output.tool_calls.push(call);
                }
                StreamPart::Usage(usage) => {
                    output.usage = Some(usage);
                }
                StreamPart::WebSearch(search) => {
                    output.web_search = Some(search);
                }
                _ => {}
            }
        }

        Ok(output)
    }
}

// ==================== 辅助结构体 ====================

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    success: bool,
    message: Option<String>,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct TaskUsage {
    task_id: String,
    prompt_tokens: i64,
    output_tokens: i64,
    consumed_quota: i64,
    soft_cap: i64,
    hard_cap: i64,
    soft_cap_exceeded: bool,
    hard_cap_exceeded: bool,
}

// ==================== 流解析器（占位，需要实现具体逻辑）====================

use bytes::Bytes;
use futures::stream::BoxStream;

fn parse_openai_stream(
    stream: impl Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
) -> BoxStream<'static, Result<StreamPart>> {
    // 重用 chat/model/openai.rs 的解析逻辑
    // 这里简化处理，实际应该调用现有的 parse_sse_stream
    Box::pin(stream.map(|chunk| {
        chunk
            .map_err(|e| anyhow!("Stream error: {}", e))
            .and_then(|_bytes| {
                // TODO: 实际解析 SSE
                Ok(StreamPart::TextDelta(crate::chat::model::TextDelta {
                    text: String::new(),
                }))
            })
    }))
}

fn parse_anthropic_stream(
    stream: impl Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
) -> BoxStream<'static, Result<StreamPart>> {
    // 重用 chat/model/anthropic.rs 的解析逻辑
    Box::pin(stream.map(|chunk| {
        chunk
            .map_err(|e| anyhow!("Stream error: {}", e))
            .and_then(|_bytes| Ok(StreamPart::TextDelta(crate::chat::model::TextDelta { text: String::new() })))
    }))
}

fn parse_gemini_stream(
    stream: impl Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
) -> BoxStream<'static, Result<StreamPart>> {
    // 重用 chat/model/gemini.rs 的解析逻辑
    Box::pin(stream.map(|chunk| {
        chunk
            .map_err(|e| anyhow!("Stream error: {}", e))
            .and_then(|_bytes| Ok(StreamPart::TextDelta(crate::chat::model::TextDelta { text: String::new() })))
    }))
}

fn parse_responses_stream(
    stream: impl Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
) -> BoxStream<'static, Result<StreamPart>> {
    // 重用 chat/model/responses.rs 的解析逻辑
    Box::pin(stream.map(|chunk| {
        chunk
            .map_err(|e| anyhow!("Stream error: {}", e))
            .and_then(|_bytes| Ok(StreamPart::TextDelta(crate::chat::model::TextDelta { text: String::new() })))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_relay_url() {
        let provider = AbuApiProvider::new(
            "https://api.abuai.com".to_string(),
            "test_key".to_string(),
            "test_task".to_string(),
        );

        assert_eq!(
            provider.build_relay_url("/v1/chat/completions"),
            "https://api.abuai.com/api/agent/relay/v1/chat/completions"
        );

        assert_eq!(
            provider.build_relay_url("/v1/messages"),
            "https://api.abuai.com/api/agent/relay/v1/messages"
        );
    }
}
