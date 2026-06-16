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

use super::slash::{dispatch_slash, SlashOutcome};

/// transcript 里的一条目。每条目自带其渲染所需的 [`Component`]（懒构造、按需重渲染）。
pub enum TranscriptItem {
    /// 用户输入的一条消息。
    UserMessage(String),
    /// 助手输出（5b 接 agent；5a 是 echo 通知文本），用 Markdown 组件渲染。
    AssistantMessage(String),
    /// 系统通知 / 提示（slash 命令输出、错误、提示等）。
    Notice(String),
    /// 工具卡片占位（5b 填充真正的工具调用渲染）。
    ToolCard(ToolCardPlaceholder),
}

/// 工具卡片占位结构 —— 5b 会扩展为真正的工具调用 / diff 渲染。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCardPlaceholder {
    pub tool_name: String,
    pub summary: String,
}

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
    /// 用户提交了一条消息（5b：交给 agent loop 跑）。5a 不产出此项（直接在 submit 内 echo）。
    Submitted(String),
}

/// footer 数据模型（5a 只展示静态 cwd/model/status；git 分支 / token 统计留待后续）。
struct Footer {
    cwd_display: String,
    model: String,
    status: String,
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
            footer: Footer { cwd_display, model, status: "ready".to_string() },
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

    /// 追加一条助手消息（5b 流式增量时也复用此入口的变体）。
    pub fn push_assistant(&mut self, text: impl Into<String>) {
        self.transcript.push(TranscriptItem::AssistantMessage(text.into()));
    }

    /// 追加一条通知。
    pub fn push_notice(&mut self, text: impl Into<String>) {
        self.transcript.push(TranscriptItem::Notice(text.into()));
    }

    /// 追加一个工具卡片占位（5b 扩展）。
    pub fn push_tool_card(&mut self, card: ToolCardPlaceholder) {
        self.transcript.push(TranscriptItem::ToolCard(card));
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

        // Ctrl+D：退出（仅在 editor 为空时；非空则当作 forward-delete 交给 editor，对齐常见 shell 习惯）。
        if matches_key(data, "ctrl+d", self.kitty_active) {
            if self.editor.get_text().is_empty() {
                return AppEffect::Quit;
            }
            // 非空：交给 editor 当 forward-delete。
            self.editor.handle_input(data);
            return AppEffect::None;
        }

        // Ctrl+C：清空编辑器；若已空，给一条提示（再按 Ctrl+D 退出）。
        if matches_key(data, "ctrl+c", self.kitty_active) {
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

    /// 提交当前编辑器内容。slash 命令就地分发；否则 5a 回显为助手通知（5b 改为返回 `Submitted`）。
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

        // 普通消息：记入 transcript。
        self.transcript.push(TranscriptItem::UserMessage(trimmed.clone()));
        self.last_submitted = Some(trimmed.clone());

        // 5a：不真正调用 agent，回显为一条助手通知。5b 将 `return AppEffect::Submitted(trimmed)`。
        self.push_assistant(format!(
            "_(echo)_ I received: **{}**\n\n(Agent execution arrives in Phase 5b.)",
            escape_markdown(&trimmed)
        ));
        AppEffect::None
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
                TranscriptItem::AssistantMessage(text) => {
                    let mut md = Markdown::new(text.clone(), 1, 0, MarkdownTheme::plain(), None);
                    lines.extend(md.render(width));
                    lines.push(String::new());
                }
                TranscriptItem::Notice(text) => {
                    let mut t = Text::new(format!("· {text}"), 1, 0, None);
                    lines.extend(t.render(width));
                    lines.push(String::new());
                }
                TranscriptItem::ToolCard(card) => {
                    let mut t = Text::new(format!("[tool] {} — {}", card.tool_name, card.summary), 1, 0, None);
                    lines.extend(t.render(width));
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
            AppMode::Generating => "generating…".to_string(),
        };
        let text = format!("{}  ·  {}  ·  {}", self.footer.cwd_display, self.footer.model, status);
        let mut footer = Text::new(text, 1, 0, None);
        footer.render(width)
    }
}

/// markdown 转义：在回显里把 `*_`` 等防止破坏 Markdown。5a 仅用于 echo。
fn escape_markdown(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '*' | '_' | '`' | '[' | ']' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
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
    fn submit_appends_user_message_and_echo_assistant() {
        let mut a = app();
        type_str(&mut a, "do a thing");
        let effect = a.handle_key("\r"); // enter
        assert_eq!(effect, AppEffect::None);
        // user message + assistant echo = 2 transcript items
        assert_eq!(a.transcript_len(), 2);
        assert_eq!(a.last_submitted(), Some("do a thing"));
        assert!(a.editor_text().is_empty(), "editor cleared after submit");
        // assistant echo references the input
        let lines = a.render(60);
        let joined = lines.join("\n");
        assert!(joined.contains("echo"), "echo notice present: {joined}");
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
        a.push_tool_card(ToolCardPlaceholder {
            tool_name: "read".to_string(),
            summary: "src/main.rs".to_string(),
        });
        let joined = a.render(60).join("\n");
        assert!(joined.contains("read"));
        assert!(joined.contains("src/main.rs"));
    }
}
