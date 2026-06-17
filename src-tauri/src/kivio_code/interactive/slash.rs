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
    SlashCommandSpec { name: "copy", aliases: &["cp"], description: "Copy the last assistant message to the clipboard" },
    SlashCommandSpec { name: "init", aliases: &[], description: "Analyze the project and write KIVIO.md" },
    SlashCommandSpec { name: "mcp", aliases: &[], description: "List configured MCP servers and their status" },
    SlashCommandSpec { name: "skill", aliases: &["skills"], description: "List discovered skills" },
    SlashCommandSpec { name: "plan", aliases: &[], description: "Switch to plan mode (read-only research & planning)" },
    SlashCommandSpec { name: "build", aliases: &[], description: "Switch to build mode (full tools)" },
    SlashCommandSpec { name: "settings", aliases: &["setting", "config"], description: "Toggle kivio-code settings" },
    SlashCommandSpec { name: "quit", aliases: &["exit", "q"], description: "Exit kivio-code" },
];

/// `/init` 触发的固定提示词：让模型用现有 read/ls/grep/glob 工具扫描项目，再用 `write_file` 落盘到
/// 项目根目录的 `KIVIO.md`（对标 Claude Code 根目录的 `CLAUDE.md`）。走普通 agent turn
/// （[`crate::kivio_code::interactive::app::AppEffect::Submitted`]），故无需新工具。结构对齐
/// `research/context-init-commands.md` §2 的模板。
pub const INIT_PROMPT: &str = "Analyze the current project at the working directory and write a concise context file for future coding-agent sessions. Use the ls, glob, grep, and read tools to inspect the repo: read manifest files (package.json, Cargo.toml, pyproject.toml, go.mod, etc.), the README, lint/test/build config, and the top-level source layout. Then use write_file to create `KIVIO.md` at the project root (the same relative path `KIVIO.md`, like Claude Code's root `CLAUDE.md`) with these sections, in order: Overview, Tech Stack, Project Structure, Build / Run / Test commands, Conventions, Notes. Be factual and derived from what the tools find — no placeholders, no fluff, keep it concise. If a `KIVIO.md` already exists, improve it rather than discarding accurate content.";

/// slash 分发结果。App 据此变更状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlashOutcome {
    /// 退出。
    Quit,
    /// 开新会话（`/new` / `/clear`）：清屏 + 重置上下文（runtime_messages / session / ctx）。
    NewConversation,
    /// 把最近一条助手消息复制到系统剪贴板。
    CopyLastAssistant,
    /// 打开模型选择器（数据由事件循环 / App 从 settings 注入）。
    OpenModelSelector,
    /// 打开会话选择器（数据由事件循环从磁盘注入）。
    OpenSessionSelector,
    /// 在 transcript 里追加一条通知（已构造好的文本）。
    Notice(String),
    /// `/init`：跑一轮 agent 生成项目上下文文件（事件循环映射为提交 [`INIT_PROMPT`]）。
    RunInit,
    /// `/mcp`：列出已配置 MCP 服务器及其状态（事件循环 block_on 探测后推进 transcript）。
    ShowMcp,
    /// `/skill`：列出已发现的技能（事件循环从活动 runtime 的 skill_registry 渲染）。
    ShowSkills,
    /// `/settings`（别名 `/setting`、`/config`）：打开设置覆盖层（事件循环填充可切换项）。
    OpenSettings,
    /// `/plan`：切到只读 plan 工作模式（App 据此 gate 工具 + 注入 plan 系统提示）。
    EnterPlan,
    /// `/build`：切回 build 工作模式（全工具集）。
    EnterBuild,
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
        Some("new") | Some("clear") => SlashOutcome::NewConversation,
        Some("copy") => SlashOutcome::CopyLastAssistant,
        Some("init") => SlashOutcome::RunInit,
        Some("mcp") => SlashOutcome::ShowMcp,
        Some("skill") => SlashOutcome::ShowSkills,
        Some("plan") => SlashOutcome::EnterPlan,
        Some("build") => SlashOutcome::EnterBuild,
        Some("settings") => SlashOutcome::OpenSettings,
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
    fn new_and_clear_start_new_conversation() {
        assert_eq!(dispatch_slash("/new"), SlashOutcome::NewConversation);
        assert_eq!(dispatch_slash("/clear"), SlashOutcome::NewConversation);
    }

    #[test]
    fn copy_dispatches_to_copy_last_assistant() {
        assert_eq!(dispatch_slash("/copy"), SlashOutcome::CopyLastAssistant);
        assert_eq!(dispatch_slash("/cp"), SlashOutcome::CopyLastAssistant);
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
        assert!(text.contains("/copy"));
        assert!(text.contains("/init"));
        assert!(text.contains("/mcp"));
        assert!(text.contains("/skill"));
        assert!(text.contains("/settings"));
    }

    #[test]
    fn init_dispatches_to_run_init() {
        assert_eq!(dispatch_slash("/init"), SlashOutcome::RunInit);
    }

    #[test]
    fn mcp_dispatches_to_show_mcp() {
        assert_eq!(dispatch_slash("/mcp"), SlashOutcome::ShowMcp);
    }

    #[test]
    fn skill_and_skills_alias_dispatch_to_show_skills() {
        assert_eq!(dispatch_slash("/skill"), SlashOutcome::ShowSkills);
        assert_eq!(dispatch_slash("/skills"), SlashOutcome::ShowSkills);
    }

    #[test]
    fn settings_and_aliases_dispatch_to_open_settings() {
        assert_eq!(dispatch_slash("/settings"), SlashOutcome::OpenSettings);
        assert_eq!(dispatch_slash("/setting"), SlashOutcome::OpenSettings);
        assert_eq!(dispatch_slash("/config"), SlashOutcome::OpenSettings);
    }

    #[test]
    fn plan_and_build_set_mode() {
        assert_eq!(dispatch_slash("/plan"), SlashOutcome::EnterPlan);
        assert_eq!(dispatch_slash("/build"), SlashOutcome::EnterBuild);
    }

    #[test]
    fn help_lists_plan_and_build() {
        let SlashOutcome::Notice(text) = dispatch_slash("/help") else {
            panic!("expected notice");
        };
        assert!(text.contains("/plan"));
        assert!(text.contains("/build"));
    }

    #[test]
    fn init_prompt_targets_root_kivio_file() {
        assert!(INIT_PROMPT.contains("KIVIO.md"));
        assert!(!INIT_PROMPT.contains(".kivio/AGENTS.md"));
        assert!(INIT_PROMPT.contains("write_file"));
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
        assert_eq!(dispatch_slash("/new   anything here"), SlashOutcome::NewConversation);
    }

    #[test]
    fn bare_slash_is_unknown() {
        assert_eq!(dispatch_slash("/"), SlashOutcome::Unknown(String::new()));
    }
}
