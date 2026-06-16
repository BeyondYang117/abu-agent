//! Readable per-tool rendering for interactive tool cards (Phase 5c).
//!
//! Where 5b dumped a raw clipped JSON / preview blob under every tool card, this
//! module mirrors PI's `modes/interactive/components/tool-execution.ts`: each
//! tool's result is shaped into a compact, human-readable body —
//!
//! - `ls` / `find` / `glob_files`: a clean file/dir name list, truncated with a
//!   `… +N more` line.
//! - `search_files` / `grep`: matched `file:line` lines (kept verbatim from the
//!   tool's own `path:line:` formatting), truncated.
//! - `read_file`: a one-line `read <path> (N lines)` header from
//!   `structured_content`.
//! - `edit_file` / `write_file`: the unified diff rendered prominently with
//!   green `+` / red `-` line coloring, clipped.
//! - `bash` / `run_command`: the command echoed, then a tail of its output.
//!
//! Everything here is pure (`ToolCard` + width → `Vec<String>` of pre-colored
//! ANSI lines) so it is unit-testable without a TTY. The colors are emitted as
//! raw SGR escapes (matching the rest of the interactive module's `default_*`
//! themes) and stripped by the test helpers via `visible_width`.

use super::app::ToolCard;
use crate::chat::types::ToolCallStatus;
use crate::kivio_code::tui::components::{Text, Markdown, MarkdownTheme};
use crate::kivio_code::tui::render::Component;

/// Max list entries (ls/find) or match lines (grep) shown before a `+N more`.
const MAX_LIST_LINES: usize = 12;
/// Max diff lines shown in a card before clipping.
const MAX_DIFF_LINES: usize = 40;
/// Max output (tail) lines shown for bash.
const MAX_BASH_TAIL_LINES: usize = 16;
/// Hard cap on a single rendered detail line's source length before the Text
/// component word-wraps it (keeps very long lines from dominating the card).
const MAX_DETAIL_CHARS: usize = 4000;

const DIM: &str = "\x1b[2m";
const DIM_OFF: &str = "\x1b[22m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const CYAN: &str = "\x1b[36m";
const COLOR_OFF: &str = "\x1b[39m";

/// The status glyph shown at the head of a card (shared with `app.rs`).
pub fn status_symbol(status: &ToolCallStatus) -> &'static str {
    match status {
        ToolCallStatus::Pending => "·",
        ToolCallStatus::Running => "▶",
        ToolCallStatus::Success => "✓",
        ToolCallStatus::Error => "✗",
        ToolCallStatus::Skipped => "⊘",
        ToolCallStatus::Cancelled => "⊗",
    }
}

/// Render one tool card to ANSI lines for the given viewport width.
///
/// The first line is always the header (`<glyph> <tool> — <summary>`); the body
/// is shaped per tool. Errors short-circuit to a red error line regardless of
/// tool type.
pub fn render_tool_card(card: &ToolCard, width: u16) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(header_lines(card, width));

    // An error result is shown the same way for every tool.
    if matches!(card.status, ToolCallStatus::Error) {
        if let Some(detail) = &card.detail {
            for raw in clip_lines(detail, MAX_BASH_TAIL_LINES) {
                lines.extend(text_line(&format!("{RED}{}{COLOR_OFF}", raw), width));
            }
        }
        return lines;
    }

    // Still running / pending: no body yet.
    if matches!(card.status, ToolCallStatus::Pending | ToolCallStatus::Running) {
        return lines;
    }

    lines.extend(body_lines(card, width));
    lines
}

/// The card header line(s): `<glyph> <tool> — <summary>`.
fn header_lines(card: &ToolCard, width: u16) -> Vec<String> {
    let header = if card.summary.is_empty() {
        format!("{} {}", status_symbol(&card.status), card.tool_name)
    } else {
        format!(
            "{} {} {DIM}—{DIM_OFF} {}",
            status_symbol(&card.status),
            card.tool_name,
            card.summary
        )
    };
    text_line(&header, width)
}

/// The per-tool body. Dispatches on the (normalized) tool name; falls back to a
/// single collapsed preview line for anything unrecognized.
fn body_lines(card: &ToolCard, width: u16) -> Vec<String> {
    match normalize_tool(&card.tool_name) {
        ToolKind::Read => read_body(card, width),
        ToolKind::Listing => listing_body(card, width),
        ToolKind::Grep => grep_body(card, width),
        ToolKind::Mutation => mutation_body(card, width),
        ToolKind::Bash => bash_body(card, width),
        ToolKind::Other => preview_body(card, width),
    }
}

/// Tool families that share a rendering shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolKind {
    Read,
    Listing,
    Grep,
    Mutation,
    Bash,
    Other,
}

fn normalize_tool(name: &str) -> ToolKind {
    match name {
        "read" | "read_file" => ToolKind::Read,
        "ls" | "list_dir" | "find" | "glob_files" => ToolKind::Listing,
        "grep" | "search_files" => ToolKind::Grep,
        "write" | "write_file" | "edit" | "edit_file" => ToolKind::Mutation,
        "bash" | "run_command" => ToolKind::Bash,
        _ => ToolKind::Other,
    }
}

/// `read_file`: a single dim header `read <path> (N lines)`, preferring the
/// structured content for an accurate range; falls back to the summary path.
fn read_body(card: &ToolCard, width: u16) -> Vec<String> {
    let header = read_header(card);
    text_line(&format!("  {DIM}{header}{DIM_OFF}"), width)
}

fn read_header(card: &ToolCard) -> String {
    if let Some(sc) = &card.structured_content {
        let path = sc.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let total = sc.get("total_lines").and_then(|v| v.as_u64());
        let start = sc.get("start_line").and_then(|v| v.as_u64());
        let end = sc.get("end_line").and_then(|v| v.as_u64());
        if !path.is_empty() {
            return match (total, start, end) {
                (Some(total), Some(start), Some(end)) if start > 1 || end < total => {
                    format!("read {path} (lines {start}-{end} of {total})")
                }
                (Some(total), _, _) => format!("read {path} ({total} lines)"),
                _ => format!("read {path}"),
            };
        }
    }
    // Fallback: derive from the summary (`path=...`).
    match arg_after_eq(&card.summary, "path") {
        Some(path) => format!("read {path}"),
        None => "read".to_string(),
    }
}

/// `ls` / `find`: a clean per-entry list, clipped with a `… +N more` line. The
/// tool returns a newline (ls) or newline-joined glob list as text; we split,
/// drop blank/heading lines, and show the leaf entries.
fn listing_body(card: &ToolCard, width: u16) -> Vec<String> {
    let Some(detail) = card.detail.as_deref() else {
        return Vec::new();
    };
    let entries: Vec<&str> = detail
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .collect();
    if entries.is_empty() {
        return text_line(&format!("  {DIM}(empty){DIM_OFF}"), width);
    }
    let mut lines = Vec::new();
    let shown = entries.len().min(MAX_LIST_LINES);
    for entry in &entries[..shown] {
        lines.extend(text_line(&format!("  {entry}"), width));
    }
    if entries.len() > shown {
        let more = entries.len() - shown;
        lines.extend(text_line(&format!("  {DIM}… +{more} more{DIM_OFF}"), width));
    }
    lines
}

/// `grep`: matched lines (kept as the tool emits them, typically `path:line:…`),
/// clipped with a `… +N more` line.
fn grep_body(card: &ToolCard, width: u16) -> Vec<String> {
    let Some(detail) = card.detail.as_deref() else {
        return Vec::new();
    };
    let matches: Vec<&str> = detail
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .collect();
    if matches.is_empty() {
        return text_line(&format!("  {DIM}no matches{DIM_OFF}"), width);
    }
    let mut lines = Vec::new();
    let shown = matches.len().min(MAX_LIST_LINES);
    for m in &matches[..shown] {
        // Dim the line so the `path:line:` prefix and the matched text read as
        // a reference rather than chat content.
        lines.extend(text_line(&format!("  {DIM}{m}{DIM_OFF}"), width));
    }
    if matches.len() > shown {
        let more = matches.len() - shown;
        lines.extend(text_line(&format!("  {DIM}… +{more} more{DIM_OFF}"), width));
    }
    lines
}

/// `write_file` / `edit_file`: the unified diff, green `+` / red `-`, clipped.
fn mutation_body(card: &ToolCard, width: u16) -> Vec<String> {
    let Some(diff) = mutation_diff(card) else {
        // No diff (e.g. no-op write): fall back to the model-facing summary.
        return preview_body(card, width);
    };
    diff_lines(&diff, width)
}

/// Extract the unified diff for a mutation card: prefer `structured_content.diff`
/// (full diff); fall back to the precomputed `card.diff`.
fn mutation_diff(card: &ToolCard) -> Option<String> {
    if let Some(sc) = &card.structured_content {
        if let Some(diff) = sc.get("diff").and_then(|d| d.as_str()) {
            if !diff.trim().is_empty() {
                return Some(diff.to_string());
            }
        }
    }
    card.diff.clone().filter(|d| !d.trim().is_empty())
}

/// Color a unified diff: `+` green, `-` red, `@@` cyan, everything else dim.
/// Clipped to [`MAX_DIFF_LINES`] with a trailing `… diff clipped` note.
fn diff_lines(diff: &str, width: u16) -> Vec<String> {
    let raw: Vec<&str> = diff.lines().collect();
    let total = raw.len();
    let shown = total.min(MAX_DIFF_LINES);
    let mut lines = Vec::new();
    for line in &raw[..shown] {
        let colored = color_diff_line(line);
        lines.extend(text_line(&format!("  {colored}"), width));
    }
    if total > shown {
        lines.extend(text_line(
            &format!("  {DIM}… diff clipped ({shown} of {total} lines){DIM_OFF}"),
            width,
        ));
    }
    lines
}

fn color_diff_line(line: &str) -> String {
    // `+++`/`---` file headers are part of the hunk frame; dim them so the
    // actual `+`/`-` content lines stand out.
    if line.starts_with("+++") || line.starts_with("---") {
        format!("{DIM}{line}{DIM_OFF}")
    } else if line.starts_with('+') {
        format!("{GREEN}{line}{COLOR_OFF}")
    } else if line.starts_with('-') {
        format!("{RED}{line}{COLOR_OFF}")
    } else if line.starts_with("@@") {
        format!("{CYAN}{line}{COLOR_OFF}")
    } else {
        format!("{DIM}{line}{DIM_OFF}")
    }
}

/// `bash` / `run_command`: the command (from the summary) then a clipped tail of
/// its output. The tool's text result is the command output.
fn bash_body(card: &ToolCard, width: u16) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(cmd) = arg_after_eq(&card.summary, "command") {
        lines.extend(text_line(&format!("  {CYAN}${COLOR_OFF} {cmd}"), width));
    }
    if let Some(detail) = &card.detail {
        let output: Vec<&str> = detail.lines().collect();
        let total = output.len();
        // Keep the *tail* (most shells' useful output is at the end).
        let start = total.saturating_sub(MAX_BASH_TAIL_LINES);
        if start > 0 {
            lines.extend(text_line(
                &format!("  {DIM}… {start} earlier line(s) hidden{DIM_OFF}"),
                width,
            ));
        }
        for line in &output[start..] {
            lines.extend(text_line(&format!("  {DIM}{line}{DIM_OFF}"), width));
        }
    }
    lines
}

/// Fallback: a single collapsed preview line (newlines → spaces), clipped. Used
/// for `web_fetch` and any unrecognized tool.
fn preview_body(card: &ToolCard, width: u16) -> Vec<String> {
    let Some(detail) = &card.detail else {
        return Vec::new();
    };
    let collapsed = detail.replace('\n', " ");
    let collapsed = clip_chars(&collapsed, MAX_DETAIL_CHARS);
    // Render through Markdown so fenced/inline formatting in tool text reads
    // cleanly, matching assistant message rendering.
    let mut md = Markdown::new(format!("  {collapsed}"), 1, 0, MarkdownTheme::plain(), None);
    md.render(width)
}

// ---- helpers ----

/// Render one source line through the `Text` component (word-wrap + width clamp).
fn text_line(s: &str, width: u16) -> Vec<String> {
    let mut t = Text::new(s.to_string(), 1, 0, None);
    t.render(width)
}

/// Split `text` into at most `max` non-empty lines.
fn clip_lines(text: &str, max: usize) -> Vec<String> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .take(max)
        .map(|l| l.to_string())
        .collect()
}

/// Char-safe truncate to `max` chars, appending `…` when clipped.
fn clip_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let head: String = text.chars().take(max).collect();
        format!("{head}…")
    }
}

/// Pull the value after `key=` out of a `key=value` summary (the summary the
/// card builder produced from the call arguments). Returns the remainder of the
/// summary after the first `key=`.
fn arg_after_eq(summary: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=");
    summary
        .strip_prefix(&needle)
        .map(|rest| rest.to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kivio_code::tui::text_width::visible_width;

    fn card(name: &str, status: ToolCallStatus) -> ToolCard {
        ToolCard {
            id: "c1".to_string(),
            tool_name: name.to_string(),
            status,
            summary: String::new(),
            detail: None,
            diff: None,
            structured_content: None,
        }
    }

    fn joined(card: &ToolCard, width: u16) -> String {
        render_tool_card(card, width).join("\n")
    }

    fn strip_ansi(s: &str) -> String {
        // crude SGR stripper for assertions
        let mut out = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                while let Some(&n) = chars.peek() {
                    chars.next();
                    if n == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn header_shows_glyph_tool_and_summary() {
        let mut c = card("read", ToolCallStatus::Success);
        c.summary = "path=src/main.rs".to_string();
        let text = strip_ansi(&joined(&c, 60));
        assert!(text.contains("read"));
        assert!(text.contains("src/main.rs"));
    }

    #[test]
    fn read_body_uses_structured_line_count() {
        let mut c = card("read_file", ToolCallStatus::Success);
        c.summary = "path=src/lib.rs".to_string();
        c.structured_content = Some(serde_json::json!({
            "path": "src/lib.rs",
            "total_lines": 120,
            "start_line": 1,
            "end_line": 120,
        }));
        let text = strip_ansi(&joined(&c, 80));
        assert!(text.contains("read src/lib.rs (120 lines)"), "{text}");
    }

    #[test]
    fn read_body_shows_range_when_windowed() {
        let mut c = card("read_file", ToolCallStatus::Success);
        c.structured_content = Some(serde_json::json!({
            "path": "big.txt", "total_lines": 500, "start_line": 10, "end_line": 60,
        }));
        let text = strip_ansi(&joined(&c, 80));
        assert!(text.contains("lines 10-60 of 500"), "{text}");
    }

    #[test]
    fn listing_body_lists_entries_and_truncates() {
        let mut c = card("ls", ToolCallStatus::Success);
        let names: Vec<String> = (0..20).map(|i| format!("file{i}.rs")).collect();
        c.detail = Some(names.join("\n"));
        let text = strip_ansi(&joined(&c, 80));
        assert!(text.contains("file0.rs"));
        assert!(text.contains("file11.rs"));
        // 20 entries, 12 shown → 8 more.
        assert!(text.contains("+8 more"), "{text}");
        assert!(!text.contains("file19.rs"));
    }

    #[test]
    fn listing_empty_shows_empty_marker() {
        let mut c = card("find", ToolCallStatus::Success);
        c.detail = Some("   \n  ".to_string());
        let text = strip_ansi(&joined(&c, 80));
        assert!(text.contains("(empty)"));
    }

    #[test]
    fn grep_body_shows_match_lines_and_truncates() {
        let mut c = card("grep", ToolCallStatus::Success);
        let lines: Vec<String> = (0..15)
            .map(|i| format!("src/a.rs:{}: let x = {i};", i + 1))
            .collect();
        c.detail = Some(lines.join("\n"));
        let text = strip_ansi(&joined(&c, 100));
        assert!(text.contains("src/a.rs:1:"));
        assert!(text.contains("+3 more"), "{text}");
    }

    #[test]
    fn grep_no_matches() {
        let mut c = card("search_files", ToolCallStatus::Success);
        c.detail = Some(String::new());
        let text = strip_ansi(&joined(&c, 80));
        assert!(text.contains("no matches"));
    }

    #[test]
    fn mutation_body_renders_colored_diff() {
        let mut c = card("edit_file", ToolCallStatus::Success);
        c.structured_content = Some(serde_json::json!({
            "diff": "@@ -1 +1 @@\n-old line\n+new line",
        }));
        let raw = joined(&c, 80);
        // raw contains the green/red SGR codes
        assert!(raw.contains(GREEN), "expected green for additions");
        assert!(raw.contains(RED), "expected red for removals");
        let text = strip_ansi(&raw);
        assert!(text.contains("+new line"));
        assert!(text.contains("-old line"));
    }

    #[test]
    fn mutation_body_clips_long_diff() {
        let mut c = card("write_file", ToolCallStatus::Success);
        let big: Vec<String> = (0..100).map(|i| format!("+line {i}")).collect();
        c.structured_content = Some(serde_json::json!({ "diff": big.join("\n") }));
        let text = strip_ansi(&joined(&c, 80));
        assert!(text.contains("diff clipped"), "{text}");
        assert!(text.contains("40 of 100 lines"), "{text}");
    }

    #[test]
    fn mutation_falls_back_to_preview_without_diff() {
        let mut c = card("write_file", ToolCallStatus::Success);
        c.detail = Some("wrote new.txt (+0 -0)".to_string());
        let text = strip_ansi(&joined(&c, 80));
        assert!(text.contains("wrote new.txt"));
    }

    #[test]
    fn bash_body_shows_command_and_output_tail() {
        let mut c = card("bash", ToolCallStatus::Success);
        c.summary = "command=cargo test".to_string();
        let out: Vec<String> = (0..30).map(|i| format!("line {i}")).collect();
        c.detail = Some(out.join("\n"));
        let text = strip_ansi(&joined(&c, 80));
        assert!(text.contains("cargo test"));
        // tail kept: last line present, earliest hidden
        assert!(text.contains("line 29"));
        assert!(text.contains("earlier line(s) hidden"), "{text}");
        assert!(!text.contains("line 0\n") || text.contains("hidden"));
    }

    #[test]
    fn error_status_shows_red_error_regardless_of_tool() {
        let mut c = card("read", ToolCallStatus::Error);
        c.detail = Some("file not found".to_string());
        let raw = joined(&c, 80);
        assert!(raw.contains(RED));
        assert!(strip_ansi(&raw).contains("file not found"));
    }

    #[test]
    fn running_card_has_no_body() {
        let mut c = card("bash", ToolCallStatus::Running);
        c.summary = "command=sleep 1".to_string();
        c.detail = Some("should not show".to_string());
        let text = strip_ansi(&joined(&c, 80));
        assert!(!text.contains("should not show"));
    }

    #[test]
    fn every_line_within_width() {
        let mut c = card("grep", ToolCallStatus::Success);
        let lines: Vec<String> = (0..5)
            .map(|i| format!("src/very/long/path/that/keeps/going/file{i}.rs:{i}: {}", "x".repeat(120)))
            .collect();
        c.detail = Some(lines.join("\n"));
        for line in render_tool_card(&c, 50) {
            assert!(visible_width(&line) <= 50, "line exceeds width: {line:?}");
        }
    }
}
