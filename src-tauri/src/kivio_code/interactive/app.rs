//! 交互模式的 App 状态机 —— PI `modes/interactive/interactive-mode.ts` 的循环形态在 Rust 端的
//! **纯状态机**抽象。
//!
//! 设计原则：把所有「输入 → 状态变更 → 期望副作用」的逻辑收进一个不依赖真实 TTY 的对象，便于用
//! `BufferTerminal` + 合成 [`Key`](super::Key) 单测（事件循环 `mod.rs` 只负责把真实输入喂进来 +
//! 把 `render()` 的行交给差分渲染器）。
//!
//! [`App`] 持有：
//! - 一个 transcript（[`TranscriptItem`] 列表：用户消息 / 助手消息（Markdown 渲染）/ 通知 / 工具卡片占位）；
//! - 输入用的 [`Editor`]（复用 Phase 4 组件，含历史 / kill-ring / autocomplete）；
//! - footer 模型（cwd / model / 状态）；
//! - 一个模式（[`AppMode::Idle`] / [`AppMode::Generating`]）。
//!
//! 对外暴露纯方法：[`App::handle_key`]（返回 [`AppEffect`]）、[`App::submit`]、[`App::render`]
//! （把 transcript + editor + footer 组合成行）。5a 阶段 submit 不真正调用 agent，而是把输入回显为
//! 一条助手通知（真正接 agent loop 留待 5b）。

use crate::kivio_code::tui::components::{Editor, EditorTheme, Markdown, MarkdownTheme, SelectListTheme, Spacer, Text};
use crate::kivio_code::tui::render::Component;

use super::agent_host::AgentUiEvent;
use super::slash::{dispatch_slash, SlashOutcome};
use crate::chat::types::{ToolCallRecord, ToolCallStatus};

/// transcript 里的一条目。每条目自带其渲染所需的 [`Component`]（懒构造、按需重渲染）。
pub enum TranscriptItem {
    /// 用户输入的一条消息。
    UserMessage(String),
    /// 助手输出（流式累积的 Markdown 文本）。
    AssistantMessage(AssistantMessage),
    /// 系统通知 / 提示（slash 命令输出、错误、提示等）。
    Notice(String),
    /// 工具卡片（按 tool-call id upsert 状态 / 结果 / diff）。
    ToolCard(ToolCard),
}

/// 一条助手消息：流式期间增量累积 `content`，完成后 `streaming=false`。`message_id`
/// 让流式 delta 能定位到正在写的这条（多条助手消息按 id 区分）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssistantMessage {
    pub message_id: String,
    pub content: String,
    pub reasoning: String,
    pub streaming: bool,
}

/// 工具卡片：从 [`ToolCallRecord`] 投影出渲染所需的字段，按 `id` upsert。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCard {
    /// provider 分配的 tool-call id（upsert key）。
    pub id: String,
    /// 工具名（read / write / edit / bash …）。
    pub tool_name: String,
    /// 当前状态。
    pub status: ToolCallStatus,
    /// 简短的参数摘要（如 `path=src/main.rs`），用于卡片标题行。
    pub summary: String,
    /// 结果预览（成功）或错误文本（失败），已裁剪。
    pub detail: Option<String>,
    /// 文件改动的 unified diff（仅 write/edit；从 structured_content 提取）。
    pub diff: Option<String>,
}

/// 旧的占位结构名保留为别名，方便既有 `push_tool_card` 调用点与测试编译。
pub type ToolCardPlaceholder = ToolCard;

/// App 当前模式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppMode {
    /// 空闲，等待用户输入。
    Idle,
    /// 正在生成助手回复（5b：agent loop 运行中；5a 不进入此态）。
    Generating,
}

/// `handle_key` / `submit` 的副作用，由事件循环消费。保持纯：状态变更在 App 内完成，仅把「需要外部
/// 做的事」作为枚举返回。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppEffect {
    /// 无副作用（已就地处理，事件循环只需重绘）。
    None,
    /// 退出交互模式。
    Quit,
    /// 用户提交了一条消息，事件循环应交给 agent loop 跑。
    Submitted(String),
    /// 用户请求中断当前生成（Esc / generating 中的 Ctrl+C）。事件循环翻 cancel flag。
    Cancel,
}

/// footer 数据模型（cwd / model / status；token 统计在一轮结束后由事件循环填入）。
struct Footer {
    cwd_display: String,
    model: String,
    status: String,
    /// 上一轮的 token usage 摘要（如 `1.2k in · 340 out`），无则不显示。
    usage: Option<String>,
}

/// 交互模式 App 状态机。
pub struct App {
    transcript: Vec<TranscriptItem>,
    editor: Editor,
    footer: Footer,
    mode: AppMode,
    kitty_active: bool,
    /// 最近一次 submit 留下的待处理回显（5a：让事件循环也能观察到“刚提交了什么”用于断言）。
    last_submitted: Option<String>,
}

impl App {
    /// 构造一个新的交互 App。`cwd_display` 已做 home→`~` 折叠；`model` 形如 `provider:model`。
    pub fn new(cwd_display: String, model: String) -> Self {
        let mut editor = Editor::new(default_editor_theme());
        editor.focused = true;
        editor.set_padding_x(1);
        Self {
            transcript: Vec::new(),
            editor,
            footer: Footer { cwd_display, model, status: "ready".to_string(), usage: None },
            mode: AppMode::Idle,
            kitty_active: false,
            last_submitted: None,
        }
    }

    pub fn set_kitty_active(&mut self, active: bool) {
        self.kitty_active = active;
        self.editor.set_kitty_active(active);
    }

    /// 终端尺寸变化时调用（editor 据此决定可见行数 / 翻页）。
    pub fn set_terminal_rows(&mut self, rows: u16) {
        self.editor.set_terminal_rows(rows as usize);
    }

    pub fn mode(&self) -> AppMode {
        self.mode
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.footer.status = status.into();
    }

    /// transcript 条目数（测试用）。
    pub fn transcript_len(&self) -> usize {
        self.transcript.len()
    }

    /// 最近一次提交的原文（测试用）。
    pub fn last_submitted(&self) -> Option<&str> {
        self.last_submitted.as_deref()
    }

    /// 当前编辑器内容（测试用）。
    pub fn editor_text(&self) -> String {
        self.editor.get_text()
    }

    /// 追加一条已完成的助手消息（通知 / 测试用；流式由 [`apply_agent_event`] 驱动）。
    pub fn push_assistant(&mut self, text: impl Into<String>) {
        self.transcript.push(TranscriptItem::AssistantMessage(AssistantMessage {
            message_id: String::new(),
            content: text.into(),
            reasoning: String::new(),
            streaming: false,
        }));
    }

    /// 追加一条通知。
    pub fn push_notice(&mut self, text: impl Into<String>) {
        self.transcript.push(TranscriptItem::Notice(text.into()));
    }

    /// 追加一个工具卡片（测试 / 直接构造用；运行时由 [`apply_agent_event`] upsert）。
    pub fn push_tool_card(&mut self, card: ToolCard) {
        self.transcript.push(TranscriptItem::ToolCard(card));
    }

    /// 进入 / 退出 generating 态（事件循环在 spawn agent / 收尾时调用）。
    pub fn set_mode(&mut self, mode: AppMode) {
        self.mode = mode;
    }

    /// 设置上一轮 token usage 摘要（footer 展示）。
    pub fn set_usage(&mut self, usage: Option<String>) {
        self.footer.usage = usage;
    }

    /// 当前是否有一条正在流式写入的助手消息（测试 / 收尾判断用）。
    pub fn assistant_streaming(&self) -> bool {
        self.transcript.iter().any(|item| {
            matches!(item, TranscriptItem::AssistantMessage(m) if m.streaming)
        })
    }

    /// 取最后一条助手消息的累积文本（测试用）。
    pub fn last_assistant_text(&self) -> Option<String> {
        self.transcript.iter().rev().find_map(|item| match item {
            TranscriptItem::AssistantMessage(m) => Some(m.content.clone()),
            _ => None,
        })
    }

    /// 取某 id 的工具卡片（测试用）。
    pub fn tool_card(&self, id: &str) -> Option<&ToolCard> {
        self.transcript.iter().rev().find_map(|item| match item {
            TranscriptItem::ToolCard(card) if card.id == id => Some(card),
            _ => None,
        })
    }

    /// 工具卡片数量（测试用）。
    pub fn tool_card_count(&self) -> usize {
        self.transcript
            .iter()
            .filter(|item| matches!(item, TranscriptItem::ToolCard(_)))
            .count()
    }

    /// 把一条 agent 事件折叠进 transcript。事件循环在 drain mpsc 时逐条调用；本方法纯（不触
    /// TTY / 不调模型），便于喂合成事件做单测。
    ///
    /// - [`AgentUiEvent::StreamDelta`]：定位到 `message_id` 的助手消息（不存在则新建一条流式中的），
    ///   追加 delta / reasoning，并将其后出现的工具卡片视为「在这条消息之后」（保持时间序）。
    /// - [`AgentUiEvent::ToolRecord`]：按 `record.id` upsert 一张工具卡片（Pending→Running→Success/Error）。
    /// - [`AgentUiEvent::Done`]：把对应助手消息 finalize（停止流式）。
    pub fn apply_agent_event(&mut self, event: AgentUiEvent) {
        match event {
            AgentUiEvent::StreamDelta { message_id, delta, reasoning } => {
                self.stream_assistant_delta(&message_id, &delta, &reasoning);
            }
            AgentUiEvent::ToolRecord(record) => {
                self.upsert_tool_card(&record);
            }
            AgentUiEvent::Done { message_id, reason } => {
                self.finalize_assistant(&message_id, &reason);
            }
        }
    }

    /// 把 delta 追加到 `message_id` 对应的助手消息；不存在则在 transcript 末尾新建一条流式中的助手消息。
    fn stream_assistant_delta(&mut self, message_id: &str, delta: &str, reasoning: &str) {
        if let Some(msg) = self.assistant_mut(message_id) {
            msg.content.push_str(delta);
            msg.reasoning.push_str(reasoning);
            return;
        }
        let mut msg = AssistantMessage {
            message_id: message_id.to_string(),
            content: String::new(),
            reasoning: String::new(),
            streaming: true,
        };
        msg.content.push_str(delta);
        msg.reasoning.push_str(reasoning);
        self.transcript.push(TranscriptItem::AssistantMessage(msg));
    }

    /// 标记 `message_id` 的助手消息流式结束。`cancelled` / `error` 时追加一条状态说明（如果该消息存在）。
    fn finalize_assistant(&mut self, message_id: &str, reason: &str) {
        let note = match reason {
            "cancelled" => Some("(cancelled)"),
            "error" => Some("(error)"),
            _ => None,
        };
        if let Some(msg) = self.assistant_mut(message_id) {
            msg.streaming = false;
            if let Some(note) = note {
                if !msg.content.is_empty() {
                    msg.content.push_str("\n\n");
                }
                msg.content.push_str(note);
            }
        } else if let Some(note) = note {
            // No streamed content arrived (e.g. cancelled before any token):
            // still surface the outcome as a notice so the user sees it.
            self.push_notice(note);
        }
    }

    /// upsert 一张工具卡片：已存在（同 id）则就地更新状态 / 结果 / diff；否则新建并 push。
    fn upsert_tool_card(&mut self, record: &ToolCallRecord) {
        let card = ToolCard::from_record(record);
        for item in self.transcript.iter_mut() {
            if let TranscriptItem::ToolCard(existing) = item {
                if existing.id == card.id {
                    *existing = card;
                    return;
                }
            }
        }
        self.transcript.push(TranscriptItem::ToolCard(card));
    }

    /// 取某 message_id 的助手消息可变引用（流式累积用）。空 id 不匹配（已完成的通知类助手消息用空 id）。
    fn assistant_mut(&mut self, message_id: &str) -> Option<&mut AssistantMessage> {
        if message_id.is_empty() {
            return None;
        }
        self.transcript.iter_mut().rev().find_map(|item| match item {
            TranscriptItem::AssistantMessage(m) if m.message_id == message_id => Some(m),
            _ => None,
        })
    }

    /// 清空 transcript（`/new`）。
    pub fn clear_transcript(&mut self) {
        self.transcript.clear();
    }

    /// 处理一段已解码的输入序列（一个按键 / 转义序列的原始字节串，由事件循环从 StdinBuffer 喂入）。
    ///
    /// 返回 [`AppEffect`]。app 级按键（提交 / Ctrl+C / Ctrl+D）优先于 editor；其余转发给 editor。
    pub fn handle_key(&mut self, data: &str) -> AppEffect {
        use crate::kivio_code::tui::keys::matches_key;

        // Esc：generating 中请求中断当前 agent 轮次（空闲时透传给 editor，由其处理补全关闭等）。
        if matches_key(data, "escape", self.kitty_active) && self.mode == AppMode::Generating {
            return AppEffect::Cancel;
        }

        // Ctrl+D：退出（仅在 editor 为空时；非空则当作 forward-delete 交给 editor，对齐常见 shell 习惯）。
        if matches_key(data, "ctrl+d", self.kitty_active) {
            if self.editor.get_text().is_empty() {
                return AppEffect::Quit;
            }
            // 非空：交给 editor 当 forward-delete。
            self.editor.handle_input(data);
            return AppEffect::None;
        }

        // Ctrl+C：generating 中 → 中断当前轮次；否则清空编辑器（已空则给退出提示）。
        if matches_key(data, "ctrl+c", self.kitty_active) {
            if self.mode == AppMode::Generating {
                return AppEffect::Cancel;
            }
            if self.editor.get_text().is_empty() {
                self.push_notice("(To exit, press Ctrl+D or type /quit)");
            } else {
                self.editor.set_text("");
            }
            return AppEffect::None;
        }

        // 提交：Enter（editor 在内部也会响应 submit，但我们要拦截以分流 slash / echo）。
        if matches_key(data, "enter", self.kitty_active) {
            return self.submit();
        }

        // 其余一律交给 editor（含历史 / 编辑 / autocomplete / 换行 alt+enter 等）。
        self.editor.handle_input(data);
        AppEffect::None
    }

    /// 提交当前编辑器内容。slash 命令就地分发；否则记入 transcript 并返回 [`AppEffect::Submitted`]，
    /// 由事件循环交给 agent loop 跑。
    pub fn submit(&mut self) -> AppEffect {
        let raw = self.editor.get_expanded_text();
        let trimmed = raw.trim().to_string();
        if trimmed.is_empty() {
            return AppEffect::None;
        }

        // 记入历史并清空编辑器。
        self.editor.add_to_history(&trimmed);
        self.editor.set_text("");

        // slash 命令分发。
        if trimmed.starts_with('/') {
            return self.dispatch_slash_command(&trimmed);
        }

        // generating 中拒绝新提交（一次只跑一轮；事件循环也会 gate，这里双保险）。
        if self.mode == AppMode::Generating {
            self.push_notice("(busy — wait for the current turn to finish or press Esc)");
            return AppEffect::None;
        }

        // 普通消息：记入 transcript，交给 agent loop。
        self.transcript.push(TranscriptItem::UserMessage(trimmed.clone()));
        self.last_submitted = Some(trimmed.clone());
        AppEffect::Submitted(trimmed)
    }

    fn dispatch_slash_command(&mut self, input: &str) -> AppEffect {
        match dispatch_slash(input) {
            SlashOutcome::Quit => AppEffect::Quit,
            SlashOutcome::ClearTranscript => {
                self.clear_transcript();
                AppEffect::None
            }
            SlashOutcome::Notice(text) => {
                self.push_notice(text);
                AppEffect::None
            }
            SlashOutcome::Unknown(name) => {
                self.push_notice(format!("Unknown command: /{name}. Type /help for the list."));
                AppEffect::None
            }
        }
    }

    /// 渲染整棵 UI（transcript → 间隔 → editor → footer）成行数组（每行 ≤ width 可见列）。
    ///
    /// 每次调用重建组件树：transcript 体量在 5a 可控，重建简单可靠；5b 大 transcript 可改增量缓存。
    pub fn render(&mut self, width: u16) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();

        // transcript。
        for item in &self.transcript {
            match item {
                TranscriptItem::UserMessage(text) => {
                    let mut t = Text::new(format!("> {text}"), 1, 0, None);
                    lines.extend(t.render(width));
                    lines.push(String::new());
                }
                TranscriptItem::AssistantMessage(msg) => {
                    // 流式中追加一个光标提示，让用户看到「还在写」。
                    let body = if msg.streaming {
                        format!("{}▌", msg.content)
                    } else {
                        msg.content.clone()
                    };
                    let mut md = Markdown::new(body, 1, 0, MarkdownTheme::plain(), None);
                    lines.extend(md.render(width));
                    lines.push(String::new());
                }
                TranscriptItem::Notice(text) => {
                    let mut t = Text::new(format!("· {text}"), 1, 0, None);
                    lines.extend(t.render(width));
                    lines.push(String::new());
                }
                TranscriptItem::ToolCard(card) => {
                    lines.extend(render_tool_card(card, width));
                    lines.push(String::new());
                }
            }
        }

        // editor。
        lines.extend(self.editor.render(width));

        // footer：一行空隔 + 状态行。
        let mut spacer = Spacer::new(0);
        lines.extend(spacer.render(width));
        lines.extend(self.render_footer(width));

        lines
    }

    fn render_footer(&mut self, width: u16) -> Vec<String> {
        let status = match self.mode {
            AppMode::Idle => self.footer.status.clone(),
            AppMode::Generating => "generating… (Esc to cancel)".to_string(),
        };
        let mut text = format!("{}  ·  {}  ·  {}", self.footer.cwd_display, self.footer.model, status);
        if let Some(usage) = &self.footer.usage {
            text.push_str(&format!("  ·  {usage}"));
        }
        let mut footer = Text::new(text, 1, 0, None);
        footer.render(width)
    }
}

impl ToolCard {
    /// 从一条 [`ToolCallRecord`] 投影出卡片。参数摘要从 `arguments` JSON 取常见键
    /// （path/command/pattern/…）；diff 从 `structured_content`（file mutation）里取。
    pub fn from_record(record: &ToolCallRecord) -> Self {
        let summary = summarize_arguments(&record.name, &record.arguments);
        let detail = record
            .error
            .clone()
            .or_else(|| record.result_preview.clone())
            .map(|t| clip(&t, 200));
        let diff = extract_diff(record);
        Self {
            id: record.id.clone(),
            tool_name: record.name.clone(),
            status: record.status.clone(),
            summary,
            detail,
            diff,
        }
    }
}

/// 工具卡片的状态符号（与 print-mode host 对齐）。
fn status_symbol(status: &ToolCallStatus) -> &'static str {
    match status {
        ToolCallStatus::Pending => "·",
        ToolCallStatus::Running => "▶",
        ToolCallStatus::Success => "✓",
        ToolCallStatus::Error => "✗",
        ToolCallStatus::Skipped => "⊘",
        ToolCallStatus::Cancelled => "⊗",
    }
}

/// 渲染一张工具卡片：标题行（符号 + 工具名 + 参数摘要），随后是结果预览 / 错误，
/// 以及（write/edit）一段裁剪后的 unified diff（带 +/- 前缀的逐行展示）。
fn render_tool_card(card: &ToolCard, width: u16) -> Vec<String> {
    let mut lines = Vec::new();
    let header = if card.summary.is_empty() {
        format!("{} {}", status_symbol(&card.status), card.tool_name)
    } else {
        format!("{} {} — {}", status_symbol(&card.status), card.tool_name, card.summary)
    };
    let mut header_text = Text::new(header, 1, 0, None);
    lines.extend(header_text.render(width));

    if let Some(diff) = &card.diff {
        // diff 优先：逐行展示（已在 from_record/extract_diff 里裁剪过行数）。
        for raw in diff.lines() {
            let mut t = Text::new(format!("  {raw}"), 1, 0, None);
            lines.extend(t.render(width));
        }
    } else if let Some(detail) = &card.detail {
        // 非文件改动：展示一行结果预览（换行折叠成空格，已裁剪长度）。
        let collapsed = detail.replace('\n', " ");
        let mut t = Text::new(format!("  {collapsed}"), 1, 0, None);
        lines.extend(t.render(width));
    }
    lines
}

/// 从工具参数 JSON 里取一个简短的人读摘要。失败 / 无常见键时返回空串。
fn summarize_arguments(tool_name: &str, arguments: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(arguments) else {
        return String::new();
    };
    let obj = match value.as_object() {
        Some(obj) => obj,
        None => return String::new(),
    };
    // 按工具/优先级取一个最具代表性的键。
    for key in ["path", "command", "pattern", "query", "url", "old_string"] {
        if let Some(v) = obj.get(key).and_then(|v| v.as_str()) {
            let label = if key == "old_string" { "match" } else { key };
            // bash 的 command 通常多行/很长，裁剪到首行 + 80 列。
            let v = v.lines().next().unwrap_or(v);
            return format!("{label}={}", clip(v, 80));
        }
    }
    let _ = tool_name;
    String::new()
}

/// 从 file-mutation 工具的 `structured_content` 里取 unified diff，并裁剪到 ~40 行。
/// 非文件改动工具返回 `None`。
fn extract_diff(record: &ToolCallRecord) -> Option<String> {
    if !matches!(record.name.as_str(), "write" | "edit" | "write_file" | "edit_file") {
        return None;
    }
    let structured = record.structured_content.as_ref()?;
    let diff = structured.get("diff").and_then(|d| d.as_str())?;
    if diff.trim().is_empty() {
        return None;
    }
    const MAX_DIFF_LINES: usize = 40;
    let lines: Vec<&str> = diff.lines().collect();
    if lines.len() > MAX_DIFF_LINES {
        let head = lines[..MAX_DIFF_LINES].join("\n");
        Some(format!(
            "{head}\n… diff clipped ({} of {} lines)",
            MAX_DIFF_LINES,
            lines.len()
        ))
    } else {
        Some(diff.to_string())
    }
}

/// 字符安全地裁剪到 `max` 列，超出加 `…`。
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let head: String = text.chars().take(max).collect();
        format!("{head}…")
    }
}

/// 一个 ANSI dim 风格的默认 editor 主题（边框灰、补全下拉素色），不依赖完整主题系统（Phase 4f 留待后续）。
fn default_editor_theme() -> EditorTheme {
    use std::sync::Arc;
    let dim: crate::kivio_code::tui::components::ColorFn =
        Arc::new(|s: &str| format!("\x1b[2m{s}\x1b[22m"));
    let cyan: crate::kivio_code::tui::components::ColorFn =
        Arc::new(|s: &str| format!("\x1b[36m{s}\x1b[39m"));
    EditorTheme {
        border_color: dim.clone(),
        select_list: SelectListTheme {
            selected_prefix: cyan.clone(),
            selected_text: cyan,
            description: dim.clone(),
            scroll_info: dim.clone(),
            no_match: dim,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut a = App::new("~/proj".to_string(), "openai:gpt-4o".to_string());
        a.set_terminal_rows(24);
        a
    }

    /// 模拟敲入一串普通字符（每个字符一个 handle_key）。
    fn type_str(a: &mut App, s: &str) {
        for ch in s.chars() {
            a.handle_key(&ch.to_string());
        }
    }

    #[test]
    fn editor_receives_keystrokes() {
        let mut a = app();
        type_str(&mut a, "hello");
        assert_eq!(a.editor_text(), "hello");
    }

    #[test]
    fn submit_appends_user_message_and_returns_submitted() {
        let mut a = app();
        type_str(&mut a, "do a thing");
        let effect = a.handle_key("\r"); // enter
        assert_eq!(effect, AppEffect::Submitted("do a thing".to_string()));
        // only the user message is recorded; the assistant message arrives via streaming.
        assert_eq!(a.transcript_len(), 1);
        assert_eq!(a.last_submitted(), Some("do a thing"));
        assert!(a.editor_text().is_empty(), "editor cleared after submit");
        let lines = a.render(60);
        let joined = lines.join("\n");
        assert!(joined.contains("do a thing"));
    }

    #[test]
    fn empty_submit_is_noop() {
        let mut a = app();
        let effect = a.handle_key("\r");
        assert_eq!(effect, AppEffect::None);
        assert_eq!(a.transcript_len(), 0);
    }

    #[test]
    fn slash_quit_yields_quit() {
        let mut a = app();
        type_str(&mut a, "/quit");
        let effect = a.handle_key("\r");
        assert_eq!(effect, AppEffect::Quit);
    }

    #[test]
    fn slash_new_clears_transcript() {
        let mut a = app();
        type_str(&mut a, "hi");
        a.handle_key("\r");
        assert!(a.transcript_len() > 0);
        type_str(&mut a, "/new");
        let effect = a.handle_key("\r");
        assert_eq!(effect, AppEffect::None);
        assert_eq!(a.transcript_len(), 0);
    }

    #[test]
    fn slash_clear_clears_transcript() {
        let mut a = app();
        type_str(&mut a, "hi");
        a.handle_key("\r");
        type_str(&mut a, "/clear");
        a.handle_key("\r");
        assert_eq!(a.transcript_len(), 0);
    }

    #[test]
    fn slash_help_shows_commands_notice() {
        let mut a = app();
        type_str(&mut a, "/help");
        let effect = a.handle_key("\r");
        assert_eq!(effect, AppEffect::None);
        assert_eq!(a.transcript_len(), 1);
        let joined = a.render(80).join("\n");
        assert!(joined.contains("/help"));
        assert!(joined.contains("/quit"));
        assert!(joined.contains("/new"));
    }

    #[test]
    fn unknown_slash_yields_notice() {
        let mut a = app();
        type_str(&mut a, "/bogus");
        let effect = a.handle_key("\r");
        assert_eq!(effect, AppEffect::None);
        let joined = a.render(80).join("\n");
        assert!(joined.contains("Unknown command"));
        assert!(joined.contains("bogus"));
    }

    #[test]
    fn ctrl_d_quits_when_empty() {
        let mut a = app();
        let effect = a.handle_key("\x04"); // ctrl+d
        assert_eq!(effect, AppEffect::Quit);
    }

    #[test]
    fn ctrl_d_does_not_quit_when_editor_nonempty() {
        let mut a = app();
        type_str(&mut a, "abc");
        // cursor at end; ctrl+d (forward-delete) deletes nothing but must NOT quit
        let effect = a.handle_key("\x04");
        assert_eq!(effect, AppEffect::None);
        assert_eq!(a.editor_text(), "abc");
    }

    #[test]
    fn ctrl_c_clears_editor() {
        let mut a = app();
        type_str(&mut a, "draft text");
        let effect = a.handle_key("\x03"); // ctrl+c
        assert_eq!(effect, AppEffect::None);
        assert!(a.editor_text().is_empty());
    }

    #[test]
    fn ctrl_c_on_empty_shows_hint() {
        let mut a = app();
        let effect = a.handle_key("\x03");
        assert_eq!(effect, AppEffect::None);
        assert_eq!(a.transcript_len(), 1);
        let joined = a.render(80).join("\n");
        assert!(joined.contains("Ctrl+D") || joined.contains("/quit"));
    }

    #[test]
    fn render_composes_transcript_editor_footer() {
        let mut a = app();
        type_str(&mut a, "question one");
        a.handle_key("\r");
        let lines = a.render(70);
        let joined = lines.join("\n");
        // transcript user line
        assert!(joined.contains("question one"), "transcript present");
        // footer shows cwd + model + status
        assert!(joined.contains("~/proj"), "footer cwd present");
        assert!(joined.contains("openai:gpt-4o"), "footer model present");
        assert!(joined.contains("ready"), "footer status present");
        // every line within width
        for l in &lines {
            assert!(
                crate::kivio_code::tui::text_width::visible_width(l) <= 70,
                "line exceeds width: {:?}",
                l
            );
        }
    }

    #[test]
    fn push_tool_card_renders() {
        let mut a = app();
        a.push_tool_card(ToolCard {
            id: "c1".to_string(),
            tool_name: "read".to_string(),
            status: ToolCallStatus::Success,
            summary: "path=src/main.rs".to_string(),
            detail: None,
            diff: None,
        });
        let joined = a.render(60).join("\n");
        assert!(joined.contains("read"));
        assert!(joined.contains("src/main.rs"));
    }

    fn tool_record(id: &str, name: &str, status: ToolCallStatus) -> ToolCallRecord {
        ToolCallRecord {
            id: id.to_string(),
            name: name.to_string(),
            source: "native".to_string(),
            server_id: None,
            arguments: serde_json::json!({ "path": "src/lib.rs" }).to_string(),
            status,
            result_preview: Some("ok".to_string()),
            error: None,
            duration_ms: None,
            started_at: None,
            completed_at: None,
            round: 1,
            sensitive: false,
            artifacts: Vec::new(),
            trace_id: None,
            span_id: None,
            structured_content: None,
        }
    }

    #[test]
    fn apply_stream_delta_accumulates_into_assistant_message() {
        let mut a = app();
        a.apply_agent_event(AgentUiEvent::StreamDelta {
            message_id: "m1".to_string(),
            delta: "Hel".to_string(),
            reasoning: String::new(),
        });
        a.apply_agent_event(AgentUiEvent::StreamDelta {
            message_id: "m1".to_string(),
            delta: "lo".to_string(),
            reasoning: String::new(),
        });
        assert_eq!(a.last_assistant_text().as_deref(), Some("Hello"));
        assert!(a.assistant_streaming(), "still streaming before Done");
    }

    #[test]
    fn apply_done_finalizes_streaming() {
        let mut a = app();
        a.apply_agent_event(AgentUiEvent::StreamDelta {
            message_id: "m1".to_string(),
            delta: "Answer".to_string(),
            reasoning: String::new(),
        });
        a.apply_agent_event(AgentUiEvent::Done {
            message_id: "m1".to_string(),
            reason: "completed".to_string(),
        });
        assert!(!a.assistant_streaming(), "finalized after Done");
        assert_eq!(a.last_assistant_text().as_deref(), Some("Answer"));
    }

    #[test]
    fn apply_done_cancelled_appends_note() {
        let mut a = app();
        a.apply_agent_event(AgentUiEvent::StreamDelta {
            message_id: "m1".to_string(),
            delta: "partial".to_string(),
            reasoning: String::new(),
        });
        a.apply_agent_event(AgentUiEvent::Done {
            message_id: "m1".to_string(),
            reason: "cancelled".to_string(),
        });
        let text = a.last_assistant_text().unwrap();
        assert!(text.contains("partial"));
        assert!(text.contains("cancelled"));
        assert!(!a.assistant_streaming());
    }

    #[test]
    fn tool_card_upsert_transitions_status_by_id() {
        let mut a = app();
        a.apply_agent_event(AgentUiEvent::ToolRecord(Box::new(tool_record(
            "call_1",
            "read",
            ToolCallStatus::Pending,
        ))));
        assert_eq!(a.tool_card_count(), 1);
        assert_eq!(a.tool_card("call_1").unwrap().status, ToolCallStatus::Pending);

        a.apply_agent_event(AgentUiEvent::ToolRecord(Box::new(tool_record(
            "call_1",
            "read",
            ToolCallStatus::Running,
        ))));
        a.apply_agent_event(AgentUiEvent::ToolRecord(Box::new(tool_record(
            "call_1",
            "read",
            ToolCallStatus::Success,
        ))));
        // Same id → still one card, now Success.
        assert_eq!(a.tool_card_count(), 1);
        assert_eq!(a.tool_card("call_1").unwrap().status, ToolCallStatus::Success);
        assert_eq!(a.tool_card("call_1").unwrap().summary, "path=src/lib.rs");
    }

    #[test]
    fn distinct_tool_ids_make_distinct_cards() {
        let mut a = app();
        a.apply_agent_event(AgentUiEvent::ToolRecord(Box::new(tool_record(
            "call_1",
            "read",
            ToolCallStatus::Success,
        ))));
        a.apply_agent_event(AgentUiEvent::ToolRecord(Box::new(tool_record(
            "call_2",
            "grep",
            ToolCallStatus::Success,
        ))));
        assert_eq!(a.tool_card_count(), 2);
    }

    #[test]
    fn edit_tool_card_extracts_diff() {
        let mut record = tool_record("call_1", "edit", ToolCallStatus::Success);
        record.structured_content = Some(serde_json::json!({
            "diff": "@@ -1 +1 @@\n-old line\n+new line"
        }));
        let card = ToolCard::from_record(&record);
        assert!(card.diff.as_deref().unwrap().contains("+new line"));

        let mut a = app();
        a.push_tool_card(card);
        let joined = a.render(80).join("\n");
        assert!(joined.contains("new line"), "diff rendered in card: {joined}");
    }

    #[test]
    fn submit_while_generating_is_rejected() {
        let mut a = app();
        a.set_mode(AppMode::Generating);
        type_str(&mut a, "another");
        let effect = a.handle_key("\r");
        assert_eq!(effect, AppEffect::None);
        // user message not recorded; a busy notice is.
        assert_eq!(a.last_submitted(), None);
    }

    #[test]
    fn esc_while_generating_requests_cancel() {
        let mut a = app();
        a.set_mode(AppMode::Generating);
        let effect = a.handle_key("\x1b"); // ESC
        assert_eq!(effect, AppEffect::Cancel);
    }

    #[test]
    fn ctrl_c_while_generating_requests_cancel() {
        let mut a = app();
        a.set_mode(AppMode::Generating);
        let effect = a.handle_key("\x03"); // ctrl+c
        assert_eq!(effect, AppEffect::Cancel);
    }

    #[test]
    fn footer_shows_usage_when_set() {
        let mut a = app();
        a.set_usage(Some("1.2k in · 340 out".to_string()));
        let joined = a.render(80).join("\n");
        assert!(joined.contains("1.2k in"));
    }
}
