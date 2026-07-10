//! OfficeCLI 实时预览（MCP 路径）。
//!
//! 上游 `officecli watch` **只**在「CLI 进程」对文档做 add/set/remove 时推 SSE；
//! 经 `officecli mcp` 的修改**不会**通知 watch，浏览器会一直停在创建时的空页。
//!
//! 因此 Kivio 在 MCP 成功改文档后：
//! 1. 要求 MCP 侧 `OFFICECLI_RESIDENT_FLUSH=each`（见 lifecycle）保证磁盘已落盘
//! 2. 用 `officecli view <file> html -o <preview.html>` 导出当前画面
//! 3. 注入 `<meta http-equiv="refresh" content="2">`，浏览器每 2s 自动重载本地 HTML
//! 4. 首次导出时用系统浏览器打开该文件
//!
//! 不阻塞 MCP 工具调用（debounced 后台任务）。

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;
use tokio::process::Command;

use super::state::{is_enabled, is_installed, plugin_dir, resolve_binary};
use crate::mcp::types::{ChatToolDefinition, McpToolCallResult};
use crate::proc::NoConsoleWindow;
use crate::state::AppState;

const OFFICECLI_PLUGIN_ID: &str = "officecli";
const PLUGIN_MCP_SERVER_ID: &str = "plugin-officecli";
const PREVIEW_HTML_NAME: &str = "live-preview.html";
/// 浏览器 meta refresh 间隔（秒）
const PREVIEW_REFRESH_SECS: u32 = 2;
/// 合并连续 MCP 写操作，避免每条工具都跑一次 view
const PREVIEW_DEBOUNCE_MS: u64 = 450;
/// 静默期：最后一次编辑后这么久没有新编辑，就摘掉 meta refresh 让浏览器停刷。
/// （上游 `officecli watch` 是 HTTP server 推送、天然会停；file:// 轮询必须自己收口）
const PREVIEW_QUIESCE_SECS: u64 = 45;

#[derive(Debug, Default)]
struct PreviewRuntime {
    /// 当前预览的文档路径
    active_path: Option<String>,
    /// 是否已打开过浏览器
    browser_opened: bool,
    /// 最近一次调度时间（debounce）
    last_schedule: Option<Instant>,
    /// 生成代数：debounce 任务只处理最新一次
    generation: u64,
}

static PREVIEW: Mutex<PreviewRuntime> = Mutex::new(PreviewRuntime {
    active_path: None,
    browser_opened: false,
    last_schedule: None,
    generation: 0,
});

/// MCP 工具成功返回后调用：若为 OfficeCLI 文档操作则异步刷新本地 HTML 预览。
pub fn note_after_officecli_tool(
    app: &AppHandle,
    _state: &AppState,
    tool: &ChatToolDefinition,
    arguments: &serde_json::Value,
    result: &McpToolCallResult,
) -> Option<String> {
    if result.is_error {
        return None;
    }
    if !is_officecli_mcp_tool(tool) {
        return None;
    }
    if !is_enabled(OFFICECLI_PLUGIN_ID) || !is_installed(OFFICECLI_PLUGIN_ID) {
        return None;
    }
    // 升级兼容：补上 MCP FLUSH=each（否则 view html 可能读到旧磁盘）
    super::lifecycle::ensure_officecli_mcp_flush_env(app, _state);
    let command = command_from_arguments(arguments);
    if command.is_empty() || !should_auto_preview(&command) {
        return None;
    }
    let path = extract_office_doc_path(&command)?;
    // 文件可能刚 create；canonicalize 失败时仍用原路径
    let path = normalize_path_key(&path);

    let gen = {
        let mut guard = PREVIEW.lock().unwrap_or_else(|e| e.into_inner());
        guard.generation = guard.generation.wrapping_add(1);
        guard.active_path = Some(path.clone());
        guard.last_schedule = Some(Instant::now());
        guard.generation
    };

    let app = app.clone();
    let path_for_task = path.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(PREVIEW_DEBOUNCE_MS)).await;
        {
            let guard = PREVIEW.lock().unwrap_or_else(|e| e.into_inner());
            if guard.generation != gen {
                return; // 有更新的调度
            }
        }
        if let Err(err) = refresh_html_preview(&app, &path_for_task).await {
            eprintln!("[plugins/preview] refresh failed: {err}");
        }

        // 静默收口：一段时间没有新编辑就摘掉 meta refresh，浏览器自然停刷。
        // （meta refresh 是浏览器端无限轮询，任务结束没人通知它；这里补上终止信号）
        tokio::time::sleep(Duration::from_secs(PREVIEW_QUIESCE_SECS)).await;
        {
            let mut guard = PREVIEW.lock().unwrap_or_else(|e| e.into_inner());
            if guard.generation != gen {
                return; // 期间又有编辑，交给最新一次调度收口
            }
            // 下次编辑重新打开浏览器视为新会话（旧标签页已停刷）
            guard.browser_opened = false;
        }
        if let Err(err) = finalize_html_preview().await {
            eprintln!("[plugins/preview] finalize failed: {err}");
        }
    });

    let html_path = preview_html_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| PREVIEW_HTML_NAME.to_string());
    Some(format!(
        "[Kivio] Live preview refreshing → open `{html_path}` (auto-reload every {PREVIEW_REFRESH_SECS}s). \
Do NOT call `officecli watch` yourself (MCP edits do not push to watch; Kivio exports HTML instead)."
    ))
}

/// 关闭插件 / 卸载 / 退出时清理标记（HTML 文件可保留）。
pub fn stop_all_previews() {
    let mut guard = PREVIEW.lock().unwrap_or_else(|e| e.into_inner());
    guard.active_path = None;
    guard.browser_opened = false;
    guard.generation = guard.generation.wrapping_add(1);
    // 顺带清掉可能残留的 watch 进程（旧实现或用户手动起的）
    kill_watch_leftovers();
}

fn kill_watch_leftovers() {
    if let Some(binary) = resolve_binary(OFFICECLI_PLUGIN_ID) {
        // best-effort unwatch unknown; kill only if we had tracked pid — none now
        let _ = binary;
    }
}

async fn refresh_html_preview(app: &AppHandle, doc_path: &str) -> Result<(), String> {
    let binary = resolve_binary(OFFICECLI_PLUGIN_ID)
        .ok_or_else(|| "officecli binary not found".to_string())?;
    let out = preview_html_path().ok_or_else(|| "app data unavailable".to_string())?;
    if let Some(parent) = out.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("create preview dir: {e}"))?;
    }

    // 等磁盘落盘（MCP + FLUSH=each 通常已写完；短等兼容慢盘/OneDrive）
    wait_for_file(doc_path, 8, 80).await;

    // 尽量把 MCP resident 刷到磁盘（与 MCP 共享 named-pipe 时 save 会生效）
    let _ = Command::new(&binary)
        .args(["save", doc_path])
        .no_console_window()
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    let output = Command::new(&binary)
        .args([
            "view",
            doc_path,
            "html",
            "-o",
            out.to_str().ok_or_else(|| "preview path not utf-8".to_string())?,
        ])
        .no_console_window()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("officecli view html: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let out_s = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "view html failed ({}): {} {}",
            output.status,
            err.trim(),
            out_s.trim()
        ));
    }

    // 注入自动刷新 + 滚动位置保持，使浏览器在 Agent 继续改文档时无需 F5
    let mut html = tokio::fs::read_to_string(&out)
        .await
        .map_err(|e| format!("read preview html: {e}"))?;
    html = inject_meta_refresh(&html, PREVIEW_REFRESH_SECS);
    html = inject_scroll_keeper(&html);
    // 标注来源，方便排查
    if !html.contains("kivio-live-preview") {
        html = html.replacen(
            "<body>",
            &format!(
                "<body data-kivio-live-preview=\"1\" data-source-doc=\"{}\">",
                html_escape_attr(doc_path)
            ),
            1,
        );
    }
    tokio::fs::write(&out, html)
        .await
        .map_err(|e| format!("write preview html: {e}"))?;

    let open_browser = {
        let mut guard = PREVIEW.lock().unwrap_or_else(|e| e.into_inner());
        let open = !guard.browser_opened;
        if open {
            guard.browser_opened = true;
        }
        open
    };

    if open_browser {
        let url = path_to_file_url(&out);
        #[allow(deprecated)]
        let _ = app.shell().open(url.as_str(), None);
    }

    Ok(())
}

fn preview_html_path() -> Option<PathBuf> {
    plugin_dir(OFFICECLI_PLUGIN_ID).map(|d| d.join(PREVIEW_HTML_NAME))
}

fn path_to_file_url(path: &Path) -> String {
    // Windows: file:///C:/Users/...
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let s = abs.to_string_lossy();
    #[cfg(windows)]
    {
        let normalized = s.trim_start_matches(r"\\?\").replace('\\', "/");
        if normalized.chars().nth(1) == Some(':') {
            return format!("file:///{normalized}");
        }
        format!("file:///{normalized}")
    }
    #[cfg(not(windows))]
    {
        format!("file://{s}")
    }
}

fn inject_meta_refresh(html: &str, secs: u32) -> String {
    let tag = format!(
        r#"<meta http-equiv="refresh" content="{secs}" data-kivio-preview-refresh="1">"#
    );
    if html.contains("data-kivio-preview-refresh") {
        // 已注入过则替换秒数
        if let Some(start) = html.find("data-kivio-preview-refresh") {
            // 简单：整段 head 内已有则不再重复
            let _ = start;
            return html.to_string();
        }
    }
    if let Some(idx) = html.find("<head>") {
        let mut out = String::with_capacity(html.len() + tag.len() + 8);
        out.push_str(&html[..idx + 6]);
        out.push('\n');
        out.push_str(&tag);
        out.push('\n');
        out.push_str(&html[idx + 6..]);
        return out;
    }
    if let Some(idx) = html.find("<head ") {
        if let Some(end) = html[idx..].find('>') {
            let ins = idx + end + 1;
            let mut out = String::with_capacity(html.len() + tag.len() + 8);
            out.push_str(&html[..ins]);
            out.push('\n');
            out.push_str(&tag);
            out.push('\n');
            out.push_str(&html[ins..]);
            return out;
        }
    }
    format!("{tag}\n{html}")
}

/// meta refresh 触发的是「新导航」而非 reload，Chrome 不恢复滚动位置——每 2 秒
/// 被拽回顶部，预览翻不了页。注入一小段 JS：卸载前把各滚动容器的位置存
/// sessionStorage（file:// 下按目录同源，可用），加载后恢复。幂等。
fn inject_scroll_keeper(html: &str) -> String {
    const MARK: &str = "data-kivio-scroll-keeper";
    if html.contains(MARK) {
        return html.to_string();
    }
    let script = format!(
        r#"<script {MARK}="1">(function(){{
var KEY='kivio-preview-scroll';
function containers(){{
  var all=document.querySelectorAll('*'),out=[document.scrollingElement||document.documentElement];
  for(var i=0;i<all.length;i++){{var e=all[i];if(e.scrollHeight>e.clientHeight+4&&e.clientHeight>0)out.push(e);}}
  return out;
}}
function keyOf(e,i){{return e.id?('#'+e.id):(e.className&&typeof e.className==='string'?e.tagName+'.'+e.className.split(' ')[0]:e.tagName)+'@'+i;}}
function save(){{
  try{{var m={{}};var cs=containers();for(var i=0;i<cs.length;i++){{var e=cs[i];if(e.scrollTop||e.scrollLeft)m[keyOf(e,i)]=[e.scrollTop,e.scrollLeft];}}
  sessionStorage.setItem(KEY,JSON.stringify(m));}}catch(_e){{}}
}}
function restore(){{
  try{{var raw=sessionStorage.getItem(KEY);if(!raw)return;var m=JSON.parse(raw);var cs=containers();
  for(var i=0;i<cs.length;i++){{var e=cs[i],v=m[keyOf(e,i)];if(v){{e.scrollTop=v[0];e.scrollLeft=v[1];}}}}}}catch(_e){{}}
}}
window.addEventListener('pagehide',save);
window.addEventListener('beforeunload',save);
if(document.readyState==='complete'||document.readyState==='interactive')setTimeout(restore,0);
else document.addEventListener('DOMContentLoaded',function(){{setTimeout(restore,0);}});
window.addEventListener('load',function(){{setTimeout(restore,50);}});
}})();</script>"#
    );
    if let Some(idx) = html.find("</body>") {
        let mut out = String::with_capacity(html.len() + script.len() + 2);
        out.push_str(&html[..idx]);
        out.push_str(&script);
        out.push('\n');
        out.push_str(&html[idx..]);
        return out;
    }
    format!("{html}\n{script}")
}

/// 从 HTML 里摘掉我们注入的 meta refresh 标签（浏览器下一次自刷拿到无刷新版，
/// 自然停止轮询）。滚动脚本保留无害。
fn strip_meta_refresh(html: &str) -> String {
    let Some(start) = html.find(r#"<meta http-equiv="refresh""#) else {
        return html.to_string();
    };
    // 只摘带我们标记的那个
    let Some(end_rel) = html[start..].find('>') else {
        return html.to_string();
    };
    let tag = &html[start..start + end_rel + 1];
    if !tag.contains("data-kivio-preview-refresh") {
        return html.to_string();
    }
    let mut out = String::with_capacity(html.len());
    out.push_str(&html[..start]);
    out.push_str(&html[start + end_rel + 1..]);
    out
}

/// 静默收口：把当前 live-preview.html 重写为无 meta refresh 版本。
async fn finalize_html_preview() -> Result<(), String> {
    let out = preview_html_path().ok_or_else(|| "app data unavailable".to_string())?;
    let html = tokio::fs::read_to_string(&out)
        .await
        .map_err(|e| format!("read preview html: {e}"))?;
    let stripped = strip_meta_refresh(&html);
    if stripped.len() != html.len() {
        tokio::fs::write(&out, stripped)
            .await
            .map_err(|e| format!("write preview html: {e}"))?;
    }
    Ok(())
}

fn html_escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

async fn wait_for_file(path: &str, attempts: u32, delay_ms: u64) {
    for _ in 0..attempts {
        if Path::new(path).is_file() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
}

fn is_officecli_mcp_tool(tool: &ChatToolDefinition) -> bool {
    tool.source == "mcp"
        && tool
            .server_id
            .as_deref()
            .is_some_and(|id| id == PLUGIN_MCP_SERVER_ID || id.ends_with("officecli"))
}

/// 把 MCP arguments 里的 command 规范成可解析字符串（支持 string 或 JSON 数组 batch）。
fn command_from_arguments(arguments: &serde_json::Value) -> String {
    match arguments.get("command") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join(" "),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

/// 是否值得为这条 officecli 命令刷新预览。
pub fn should_auto_preview(command: &str) -> bool {
    let verb = first_token(command).unwrap_or("").to_ascii_lowercase();
    // 去掉可能的前导 officecli
    let verb = if verb == "officecli" {
        command
            .split_whitespace()
            .nth(1)
            .unwrap_or("")
            .to_ascii_lowercase()
    } else {
        verb
    };
    match verb.as_str() {
        "help" | "watch" | "unwatch" | "close" | "version" | "--version" | "-v" | "marks"
        | "mark" | "unmark" | "goto" | "load_skill" => false,
        _ => true,
    }
}

/// 从 officecli command 里抽出第一个 .docx/.xlsx/.pptx 路径。
pub fn extract_office_doc_path(command: &str) -> Option<String> {
    let bytes = command.as_bytes();
    let mut i = 0;
    let len = bytes.len();
    let mut tokens: Vec<String> = Vec::new();

    while i < len {
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= len {
            break;
        }
        let (token, next) = if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            let start = i;
            while i < len && bytes[i] != quote {
                i += 1;
            }
            let s = String::from_utf8_lossy(&bytes[start..i]).into_owned();
            if i < len {
                i += 1;
            }
            (s, i)
        } else {
            let start = i;
            while i < len && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            (String::from_utf8_lossy(&bytes[start..i]).into_owned(), i)
        };
        i = next;
        if !token.is_empty() {
            tokens.push(token);
        }
    }

    for (idx, tok) in tokens.iter().enumerate() {
        if idx == 0 && (tok.eq_ignore_ascii_case("officecli") || !is_office_doc_path(tok)) {
            // 跳过动词；但若动词位就是路径则下面全扫会捡到
            if is_office_doc_path(tok) {
                return Some(tok.trim_matches('"').trim_matches('\'').to_string());
            }
            continue;
        }
        if is_office_doc_path(tok) {
            return Some(tok.trim_matches('"').trim_matches('\'').to_string());
        }
    }
    tokens.into_iter().find(|t| is_office_doc_path(t))
}

fn is_office_doc_path(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    // 去掉 JSON 数组残留引号
    let lower = lower.trim_matches('"').trim_matches('\'');
    lower.ends_with(".pptx")
        || lower.ends_with(".docx")
        || lower.ends_with(".xlsx")
        || lower.ends_with(".pptm")
        || lower.ends_with(".xlsm")
        || lower.ends_with(".docm")
}

fn first_token(command: &str) -> Option<&str> {
    command.split_whitespace().next()
}

fn normalize_path_key(path: &str) -> String {
    let p = PathBuf::from(path);
    std::fs::canonicalize(&p)
        .unwrap_or(p)
        .to_string_lossy()
        // Windows \\?\ 前缀去掉，方便 officecli 与展示
        .trim_start_matches(r"\\?\")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_quoted_windows_path() {
        let cmd = r#"create "C:\Users\11028\OneDrive\Desktop\2026Q2_业务回顾.pptx""#;
        let p = extract_office_doc_path(cmd).expect("path");
        assert!(p.ends_with(".pptx"), "{p}");
        assert!(p.contains("OneDrive"));
    }

    #[test]
    fn extracts_batch_array_style() {
        let cmd = r#"batch C:\Users\a\Desktop\deck.pptx --commands [...]"#;
        assert_eq!(
            extract_office_doc_path(cmd).as_deref(),
            Some(r"C:\Users\a\Desktop\deck.pptx")
        );
    }

    #[test]
    fn extracts_unquoted_path() {
        let cmd = "add /tmp/deck.pptx / --type slide --prop title=Hi";
        assert_eq!(
            extract_office_doc_path(cmd).as_deref(),
            Some("/tmp/deck.pptx")
        );
    }

    #[test]
    fn extracts_set_path() {
        let cmd = r#"set "D:\work\q2.docx" /body/p[1] --prop text=Hello"#;
        assert_eq!(
            extract_office_doc_path(cmd).as_deref(),
            Some(r"D:\work\q2.docx")
        );
    }

    #[test]
    fn skips_help() {
        assert!(!should_auto_preview("help pptx shape"));
        assert!(!should_auto_preview("watch deck.pptx"));
        assert!(should_auto_preview(
            r#"create "C:\Users\a\Desktop\a.pptx""#
        ));
        assert!(should_auto_preview("view deck.pptx outline"));
        assert!(should_auto_preview("batch deck.pptx --commands []"));
    }

    #[test]
    fn no_path_returns_none() {
        assert!(extract_office_doc_path("help pptx").is_none());
    }

    #[test]
    fn inject_refresh_into_head() {
        let html = "<!DOCTYPE html><html><head><meta charset=utf-8></head><body>hi</body></html>";
        let out = inject_meta_refresh(html, 2);
        assert!(out.contains("data-kivio-preview-refresh"));
        assert!(out.contains("content=\"2\""));
        assert!(out.find("<head>").unwrap() < out.find("data-kivio-preview-refresh").unwrap());
    }

    #[test]
    fn strip_refresh_removes_only_our_tag() {
        let html = "<!DOCTYPE html><html><head><meta charset=utf-8></head><body>hi</body></html>";
        let injected = inject_meta_refresh(html, 2);
        let stripped = strip_meta_refresh(&injected);
        assert!(!stripped.contains("http-equiv=\"refresh\""));
        assert!(stripped.contains("<meta charset=utf-8>"));
        // 非我们注入的 refresh 不动
        let foreign = r#"<head><meta http-equiv="refresh" content="9"></head>"#;
        assert_eq!(strip_meta_refresh(foreign), foreign);
        // 幂等
        assert_eq!(strip_meta_refresh(&stripped), stripped);
    }

    #[test]
    fn scroll_keeper_injected_before_body_close_and_idempotent() {
        let html = "<html><head></head><body>hi</body></html>";
        let out = inject_scroll_keeper(html);
        assert!(out.contains("data-kivio-scroll-keeper"));
        assert!(out.find("data-kivio-scroll-keeper").unwrap() < out.find("</body>").unwrap());
        assert_eq!(inject_scroll_keeper(&out), out);
    }

    #[test]
    fn command_from_array_args() {
        let v = serde_json::json!({
            "command": ["batch", r"C:\Users\a\a.pptx", "--commands", "[]"]
        });
        let c = command_from_arguments(&v);
        assert!(c.contains("batch"));
        assert!(c.contains("a.pptx"));
        assert!(extract_office_doc_path(&c).is_some());
    }
}
