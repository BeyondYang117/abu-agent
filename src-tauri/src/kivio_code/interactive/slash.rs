//! Slash 命令分发 —— 纯函数，便于单测，与 App 状态解耦。
//!
//! 5a 范围（对齐 `pi-cli-ux.md` §3 的 MVP 子集里能无 agent 落地的几个）：
//! - `/help` —— 列出命令；
//! - `/quit`（别名 `/exit`、`/q`）—— 退出；
//! - `/new` —— 清空 transcript（开新会话的雏形）；
//! - `/clear` —— 清空 transcript（PI 无此内建，这里按任务说明作为 `/new` 的别名）。
//!
//! 真正需要 agent / session / model selector 的命令（`/model` `/session` `/compact` `/fork` …）留待
//! 5b/5c，届时由 App 注入回调。未知命令返回 [`SlashOutcome::Unknown`]。

/// 一条 slash 命令的元数据（用于 `/help` 渲染与分发匹配）。
pub struct SlashCommandSpec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
}

/// 5c 支持的内建 slash 命令表。
pub const SLASH_COMMANDS: &[SlashCommandSpec] = &[
    SlashCommandSpec { name: "help", aliases: &["h", "?"], description: "Show available commands" },
    SlashCommandSpec { name: "model", aliases: &["m"], description: "Switch the active model" },
    SlashCommandSpec { name: "sessions", aliases: &["session", "resume"], description: "Resume a recent session" },
    SlashCommandSpec { name: "new", aliases: &[], description: "Clear the transcript and start fresh" },
    SlashCommandSpec { name: "clear", aliases: &[], description: "Clear the transcript" },
    SlashCommandSpec { name: "quit", aliases: &["exit", "q"], description: "Exit kivio-code" },
];

/// slash 分发结果。App 据此变更状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlashOutcome {
    /// 退出。
    Quit,
    /// 清空 transcript。
    ClearTranscript,
    /// 打开模型选择器（数据由事件循环 / App 从 settings 注入）。
    OpenModelSelector,
    /// 打开会话选择器（数据由事件循环从磁盘注入）。
    OpenSessionSelector,
    /// 在 transcript 里追加一条通知（已构造好的文本）。
    Notice(String),
    /// 未知命令（携带去掉前导 `/` 的命令名）。
    Unknown(String),
}

/// 解析并分发一条 slash 输入（形如 `/help`、`/quit`、`/model gpt-4o`）。
///
/// 仅看第一个 token（命令名，去掉前导 `/`、小写比较）；参数（如有）在 5a 暂被忽略，留待具体命令实现。
pub fn dispatch_slash(input: &str) -> SlashOutcome {
    let trimmed = input.trim();
    let without_slash = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let name = without_slash.split_whitespace().next().unwrap_or("").to_lowercase();

    if name.is_empty() {
        return SlashOutcome::Unknown(String::new());
    }

    let spec = SLASH_COMMANDS
        .iter()
        .find(|s| s.name == name || s.aliases.iter().any(|a| *a == name));

    match spec.map(|s| s.name) {
        Some("help") | Some("?") => SlashOutcome::Notice(help_text()),
        Some("model") => SlashOutcome::OpenModelSelector,
        Some("sessions") => SlashOutcome::OpenSessionSelector,
        Some("new") | Some("clear") => SlashOutcome::ClearTranscript,
        Some("quit") => SlashOutcome::Quit,
        _ => SlashOutcome::Unknown(name),
    }
}

/// 渲染 `/help` 的文本（命令 + 描述）。
pub fn help_text() -> String {
    let mut out = String::from("Available commands:\n");
    for spec in SLASH_COMMANDS {
        out.push_str(&format!("  /{:<8} {}\n", spec.name, spec.description));
    }
    out.push_str("\nKeys: Enter submit · Ctrl+C clear input · Ctrl+D exit · Esc cancel · Ctrl+L model");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_and_aliases() {
        assert_eq!(dispatch_slash("/quit"), SlashOutcome::Quit);
        assert_eq!(dispatch_slash("/exit"), SlashOutcome::Quit);
        assert_eq!(dispatch_slash("/q"), SlashOutcome::Quit);
    }

    #[test]
    fn new_and_clear_clear_transcript() {
        assert_eq!(dispatch_slash("/new"), SlashOutcome::ClearTranscript);
        assert_eq!(dispatch_slash("/clear"), SlashOutcome::ClearTranscript);
    }

    #[test]
    fn help_lists_commands() {
        let SlashOutcome::Notice(text) = dispatch_slash("/help") else {
            panic!("expected notice");
        };
        assert!(text.contains("/help"));
        assert!(text.contains("/quit"));
        assert!(text.contains("/new"));
        assert!(text.contains("/clear"));
    }

    #[test]
    fn unknown_command() {
        assert_eq!(dispatch_slash("/frobnicate"), SlashOutcome::Unknown("frobnicate".to_string()));
    }

    #[test]
    fn model_opens_selector() {
        assert_eq!(dispatch_slash("/model"), SlashOutcome::OpenModelSelector);
        assert_eq!(dispatch_slash("/m"), SlashOutcome::OpenModelSelector);
    }

    #[test]
    fn sessions_opens_selector() {
        assert_eq!(dispatch_slash("/sessions"), SlashOutcome::OpenSessionSelector);
        assert_eq!(dispatch_slash("/session"), SlashOutcome::OpenSessionSelector);
        assert_eq!(dispatch_slash("/resume"), SlashOutcome::OpenSessionSelector);
    }

    #[test]
    fn case_insensitive_and_args_ignored() {
        assert_eq!(dispatch_slash("/QUIT"), SlashOutcome::Quit);
        assert_eq!(dispatch_slash("/new   anything here"), SlashOutcome::ClearTranscript);
    }

    #[test]
    fn bare_slash_is_unknown() {
        assert_eq!(dispatch_slash("/"), SlashOutcome::Unknown(String::new()));
    }
}
