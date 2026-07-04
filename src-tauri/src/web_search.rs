use serde::{Deserialize, Serialize};

use crate::{
    api::{send_with_retry, with_standard_request_timeout},
    mcp::client::StreamableHttpMcpClient,
    settings::{ChatMcpServer, LensWebSearchConfig, WebSearchProvider},
    state::AppState,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
    pub content: String,
    pub published_date: Option<String>,
    pub score: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct TavilySearchResponse {
    #[serde(default)]
    answer: Option<String>,
    #[serde(default)]
    results: Vec<TavilySearchResult>,
}

#[derive(Debug, Deserialize)]
struct TavilySearchResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    score: Option<f64>,
    #[serde(default)]
    published_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExaSearchResponse {
    #[serde(default)]
    results: Vec<ExaSearchResult>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExaSearchResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    highlights: Vec<String>,
    #[serde(default)]
    score: Option<f64>,
    #[serde(default)]
    published_date: Option<String>,
}

pub async fn search_web(
    state: &AppState,
    config: &LensWebSearchConfig,
    query: &str,
    retry_attempts: usize,
) -> Result<Vec<WebSearchResult>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    match config.provider {
        WebSearchProvider::Tavily => search_tavily(state, config, query, retry_attempts).await,
        WebSearchProvider::Exa => search_exa(state, config, query, retry_attempts).await,
        WebSearchProvider::ExaMcp => search_exa_mcp(state, config, query).await,
        WebSearchProvider::Unknown => {
            Err("Selected web search provider is not supported yet".to_string())
        }
    }
}

/// Exa MCP 搜索：调用 Exa 官方 MCP 服务器（默认 https://mcp.exa.ai/mcp）的
/// `web_search_exa` 工具，复用通用的 Streamable HTTP MCP 客户端。API Key 走
/// `?exaApiKey=` 查询参数（Exa MCP 的约定），无 key 也可低配额试用。
async fn search_exa_mcp(
    state: &AppState,
    config: &LensWebSearchConfig,
    query: &str,
) -> Result<Vec<WebSearchResult>, String> {
    let base = config.exa_mcp_url.trim();
    if base.is_empty() {
        return Err("Exa MCP endpoint is not configured".to_string());
    }
    let api_key = config.exa_api_key.trim();
    let url = if api_key.is_empty() {
        base.to_string()
    } else if base.contains('?') {
        format!("{base}&exaApiKey={api_key}")
    } else {
        format!("{base}?exaApiKey={api_key}")
    };

    let server = ChatMcpServer {
        id: "exa-mcp".to_string(),
        name: "Exa MCP".to_string(),
        enabled: true,
        transport: "streamable_http".to_string(),
        url,
        ..ChatMcpServer::default()
    };
    let client = StreamableHttpMcpClient::new(server, 30_000, state.http.clone());

    let max_results = config.max_results.clamp(1, 10);
    let result = client
        .call_tool(
            "web_search_exa",
            serde_json::json!({ "query": query, "numResults": max_results }),
        )
        .await?;
    if result.is_error {
        return Err(format!("Exa MCP search failed: {}", result.content));
    }

    Ok(parse_exa_mcp_results(&result.content, max_results as usize))
}

/// Exa MCP 的工具返回体是一段文本，通常内嵌 JSON（`{ "results": [...] }`）。
/// 尽量结构化解析；解析失败时把整段文本作为单条结果返回，保证至少有可用内容。
fn parse_exa_mcp_results(content: &str, max_results: usize) -> Vec<WebSearchResult> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if let Ok(parsed) = serde_json::from_str::<ExaSearchResponse>(trimmed) {
        let results: Vec<WebSearchResult> = parsed
            .results
            .into_iter()
            .filter(|result| !result.url.trim().is_empty())
            .map(|result| {
                let content = if !result.highlights.is_empty() {
                    result.highlights.join("\n")
                } else if !result.summary.trim().is_empty() {
                    result.summary
                } else {
                    result.text
                };
                WebSearchResult {
                    title: result.title.trim().to_string(),
                    url: result.url.trim().to_string(),
                    content: content.trim().to_string(),
                    published_date: result.published_date,
                    score: result.score,
                }
            })
            .take(max_results)
            .collect();
        if !results.is_empty() {
            return results;
        }
    }
    vec![WebSearchResult {
        title: "Exa MCP result".to_string(),
        url: "https://mcp.exa.ai/mcp".to_string(),
        content: trimmed.chars().take(4000).collect(),
        published_date: None,
        score: None,
    }]
}

async fn search_tavily(
    state: &AppState,
    config: &LensWebSearchConfig,
    query: &str,
    retry_attempts: usize,
) -> Result<Vec<WebSearchResult>, String> {
    let api_key = config.tavily_api_key.trim();
    if api_key.is_empty() {
        return Err("Tavily API key is not configured".to_string());
    }

    let max_results = config.max_results.clamp(1, 10);
    let search_depth = match config.search_depth.as_str() {
        "ultra-fast" | "fast" | "basic" | "advanced" => config.search_depth.as_str(),
        _ => "basic",
    };
    let body = serde_json::json!({
        "query": query,
        "search_depth": search_depth,
        "max_results": max_results,
        "include_answer": true,
        "include_raw_content": false,
        "include_images": false,
        "include_favicon": false,
    });

    let response = send_with_retry("Tavily search", retry_attempts, || {
        with_standard_request_timeout(
            state
                .http
                .post("https://api.tavily.com/search")
                .bearer_auth(api_key)
                .json(&body),
        )
        .send()
    })
    .await?;

    let raw = response
        .text()
        .await
        .map_err(|err| format!("Tavily search read body: {err}"))?;
    let parsed: TavilySearchResponse = serde_json::from_str(&raw).map_err(|err| {
        format!(
            "Tavily search parse JSON: {} (body: {})",
            err,
            raw.chars().take(500).collect::<String>()
        )
    })?;

    let mut results: Vec<WebSearchResult> = parsed
        .results
        .into_iter()
        .filter(|result| !result.url.trim().is_empty())
        .map(|result| WebSearchResult {
            title: result.title.trim().to_string(),
            url: result.url.trim().to_string(),
            content: result.content.trim().to_string(),
            published_date: result.published_date,
            score: result.score,
        })
        .collect();

    if let Some(answer) = parsed
        .answer
        .as_deref()
        .filter(|answer| !answer.trim().is_empty())
    {
        results.insert(
            0,
            WebSearchResult {
                title: "Tavily answer".to_string(),
                url: "https://api.tavily.com/search".to_string(),
                content: answer.trim().to_string(),
                published_date: None,
                score: None,
            },
        );
    }

    Ok(results)
}

async fn search_exa(
    state: &AppState,
    config: &LensWebSearchConfig,
    query: &str,
    retry_attempts: usize,
) -> Result<Vec<WebSearchResult>, String> {
    let api_key = config.exa_api_key.trim();
    if api_key.is_empty() {
        return Err("Exa API key is not configured".to_string());
    }

    let max_results = config.max_results.clamp(1, 10);
    let body = serde_json::json!({
        "query": query,
        "numResults": max_results,
        "contents": {
            "highlights": true
        }
    });

    let response = send_with_retry("Exa search", retry_attempts, || {
        with_standard_request_timeout(
            state
                .http
                .post("https://api.exa.ai/search")
                .header("x-api-key", api_key)
                .json(&body),
        )
        .send()
    })
    .await?;

    let raw = response
        .text()
        .await
        .map_err(|err| format!("Exa search read body: {err}"))?;
    let parsed: ExaSearchResponse = serde_json::from_str(&raw).map_err(|err| {
        format!(
            "Exa search parse JSON: {} (body: {})",
            err,
            raw.chars().take(500).collect::<String>()
        )
    })?;

    Ok(parsed
        .results
        .into_iter()
        .filter(|result| !result.url.trim().is_empty())
        .map(|result| {
            let content = if !result.highlights.is_empty() {
                result.highlights.join("\n")
            } else if !result.summary.trim().is_empty() {
                result.summary
            } else {
                result.text
            };
            WebSearchResult {
                title: result.title.trim().to_string(),
                url: result.url.trim().to_string(),
                content: content.trim().to_string(),
                published_date: result.published_date,
                score: result.score,
            }
        })
        .collect())
}

/// Render web search results into the textual context block injected into the
/// model conversation: a two-line header, then per result a `[N] Title` line, a
/// `URL: …` line, and optional `Published:` / `Score:` / `Snippet:` lines.
///
/// NOTE: the kivio-code tool card parser (`kivio_code::interactive::tool_card::
/// parse_web_results`) depends on this exact line shape — it recognizes `[N] Title`
/// title lines and the following `URL:` line to render a compact result list.
/// If you change the per-result format here (reorder lines, drop the `[N]` title
/// prefix, rename `URL:`), update that parser too, or the card silently falls back
/// to a flat text preview / mis-associates URLs.
pub fn format_web_context(results: &[WebSearchResult]) -> String {
    if results.is_empty() {
        return String::new();
    }

    let mut lines = Vec::with_capacity(results.len() * 5 + 4);
    lines.push("Web search context:".to_string());
    lines.push(
        "Use only these sources for current web facts. Cite sources with [1], [2], etc. If the sources are insufficient, say so."
            .to_string(),
    );

    for (idx, result) in results.iter().enumerate() {
        let title = if result.title.is_empty() {
            "Untitled"
        } else {
            result.title.as_str()
        };
        lines.push(format!("[{}] {}", idx + 1, title));
        lines.push(format!("URL: {}", result.url));
        if let Some(date) = result
            .published_date
            .as_deref()
            .filter(|d| !d.trim().is_empty())
        {
            lines.push(format!("Published: {}", date.trim()));
        }
        if let Some(score) = result.score {
            lines.push(format!("Score: {:.3}", score));
        }
        if !result.content.is_empty() {
            let snippet: String = result.content.chars().take(1200).collect();
            lines.push(format!("Snippet: {}", snippet));
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{
        format_web_context, parse_exa_mcp_results, ExaSearchResponse, TavilySearchResponse,
        WebSearchResult,
    };

    #[test]
    fn tavily_response_deserializes_results_and_answer() {
        let raw = r#"{
            "answer": "Sample answer",
            "results": [
                {
                    "title": "Example",
                    "url": "https://example.com",
                    "content": "Snippet",
                    "score": 0.91,
                    "published_date": "2026-01-01"
                }
            ]
        }"#;
        let parsed: TavilySearchResponse = serde_json::from_str(raw).expect("tavily json");
        assert_eq!(parsed.answer.as_deref(), Some("Sample answer"));
        assert_eq!(parsed.results.len(), 1);
        assert_eq!(parsed.results[0].title, "Example");
        assert_eq!(
            parsed.results[0].published_date.as_deref(),
            Some("2026-01-01")
        );
    }

    #[test]
    fn exa_response_deserializes_camel_case_fields() {
        let raw = r#"{
            "results": [
                {
                    "title": "Exa Result",
                    "url": "https://exa.ai/article",
                    "text": "Body text",
                    "summary": "Summary text",
                    "highlights": ["highlight one"],
                    "score": 0.75,
                    "publishedDate": "2026-02-02"
                }
            ]
        }"#;
        let parsed: ExaSearchResponse = serde_json::from_str(raw).expect("exa json");
        assert_eq!(parsed.results.len(), 1);
        let result = &parsed.results[0];
        assert_eq!(result.title, "Exa Result");
        assert_eq!(result.highlights, vec!["highlight one".to_string()]);
        assert_eq!(result.published_date.as_deref(), Some("2026-02-02"));
    }

    #[test]
    fn format_web_context_includes_numbered_sources_and_snippets() {
        let context = format_web_context(&[WebSearchResult {
            title: "Docs".to_string(),
            url: "https://docs.example.com".to_string(),
            content: "Helpful snippet".to_string(),
            published_date: Some("2026-03-03".to_string()),
            score: Some(0.5),
        }]);
        assert!(context.contains("Web search context:"));
        assert!(context.contains("[1] Docs"));
        assert!(context.contains("URL: https://docs.example.com"));
        assert!(context.contains("Published: 2026-03-03"));
        assert!(context.contains("Snippet: Helpful snippet"));
    }

    #[test]
    fn exa_mcp_parses_embedded_json_results() {
        let raw = r#"{
            "results": [
                { "title": "MCP Doc", "url": "https://exa.ai/mcp", "text": "body", "highlights": ["hl"], "publishedDate": "2026-04-04" }
            ]
        }"#;
        let results = parse_exa_mcp_results(raw, 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].url, "https://exa.ai/mcp");
        assert_eq!(results[0].content, "hl");
    }

    #[test]
    fn exa_mcp_falls_back_to_raw_text_when_not_json() {
        let results = parse_exa_mcp_results("plain text answer", 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "plain text answer");
    }

    #[test]
    fn exa_mcp_empty_content_yields_no_results() {
        assert!(parse_exa_mcp_results("   ", 5).is_empty());
    }
}
