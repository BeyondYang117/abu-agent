use reqwest::Client;
use serde_json::Value;

const MAX_FETCH_BYTES: usize = 512 * 1024;

pub async fn web_fetch(http: &Client, arguments: &Value) -> Result<String, String> {
    let url = arguments
        .get("url")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "web_fetch requires url".to_string())?;

    if !url.starts_with("https://") {
        return Err("web_fetch only allows https:// URLs".to_string());
    }

    let response = http
        .get(url)
        .header(reqwest::header::USER_AGENT, "Kivio/1.0 (Chat web_fetch)")
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|err| format!("Fetch failed: {err}"))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();

    let bytes = response
        .bytes()
        .await
        .map_err(|err| format!("Read body failed: {err}"))?;
    if bytes.len() > MAX_FETCH_BYTES {
        return Err(format!(
            "Response too large (max {} bytes)",
            MAX_FETCH_BYTES
        ));
    }

    let body = String::from_utf8_lossy(&bytes).into_owned();
    let text = if content_type.contains("text/html") {
        html_to_text(&body)
    } else {
        body
    };

    Ok(format!("URL: {url}\n\n{text}"))
}

fn html_to_text(html: &str) -> String {
    let mut s = html.to_string();
    for tag in ["script", "style", "noscript"] {
        while let Some(start) = s.to_ascii_lowercase().find(&format!("<{tag}")) {
            if let Some(end) = s[start..].to_ascii_lowercase().find(&format!("</{tag}>")) {
                let remove_end = start + end + tag.len() + 3;
                s.replace_range(start..remove_end.min(s.len()), " ");
            } else {
                break;
            }
        }
    }
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    collapse_whitespace(&out)
}

fn collapse_whitespace(text: &str) -> String {
    let mut out = String::new();
    let mut prev_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}
