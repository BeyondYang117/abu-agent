use super::super::types::{
    ExternalMcpInjection, PromptInputFormat, RuntimeAgentDef, RuntimeBuildOptions, RuntimeContext,
    StreamFormat,
};

const FALLBACK_MODELS: &[(&str, &str)] = &[
    ("default", "Default"),
    ("sonnet", "Sonnet (alias)"),
    ("opus", "Opus (alias)"),
    ("haiku", "Haiku (alias)"),
];

pub fn build_claude_args(ctx: &RuntimeContext, options: &RuntimeBuildOptions) -> Vec<String> {
    let mut args = vec![
        "-p".to_string(),
        "--input-format".to_string(),
        "stream-json".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--verbose".to_string(),
    ];
    if ctx.include_partial_messages {
        args.push("--include-partial-messages".to_string());
    }
    if let Some(model) = options.model.as_ref().filter(|m| *m != "default" && !m.is_empty()) {
        args.push("--model".to_string());
        args.push(model.clone());
    }
    for dir in &ctx.extra_allowed_dirs {
        if !dir.is_empty() {
            args.push("--add-dir".to_string());
            args.push(dir.clone());
        }
    }
    if let Some(session_id) = ctx.resume_session_id.as_ref().filter(|s| !s.is_empty()) {
        args.push("--resume".to_string());
        args.push(session_id.clone());
    } else if let Some(session_id) = ctx.new_session_id.as_ref().filter(|s| !s.is_empty()) {
        args.push("--session-id".to_string());
        args.push(session_id.clone());
    }
    args.push("--permission-mode".to_string());
    args.push("bypassPermissions".to_string());
    args
}

pub const CLAUDE_AGENT_DEF: RuntimeAgentDef = RuntimeAgentDef {
    id: "claude",
    name: "Claude Code",
    bin: "claude",
    fallback_bins: &["openclaude"],
    version_args: &["--version"],
    auth_probe_args: Some(&["auth", "status"]),
    fallback_models: FALLBACK_MODELS,
    reasoning_options: &[],
    list_models_args: None,
    prompt_via_stdin: true,
    prompt_input_format: PromptInputFormat::StreamJson,
    stream_format: StreamFormat::ClaudeStreamJson,
    json_event_parser: None,
    external_mcp_injection: Some(ExternalMcpInjection::ClaudeMcpJson),
    resumes_session_via_cli: true,
    build_args: build_claude_args,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_build_args_includes_resume_and_add_dir() {
        let args = build_claude_args(
            &RuntimeContext {
                cwd: Some("/tmp/w".to_string()),
                extra_allowed_dirs: vec!["/skills".to_string()],
                resume_session_id: Some("sess-1".to_string()),
                new_session_id: None,
                include_partial_messages: true,
            },
            &RuntimeBuildOptions {
                model: Some("sonnet".to_string()),
                reasoning: None,
            },
        );
        assert!(args.contains(&"--resume".to_string()));
        assert!(args.contains(&"sess-1".to_string()));
        assert!(args.contains(&"--add-dir".to_string()));
        assert!(args.contains(&"/skills".to_string()));
        assert!(args.contains(&"--model".to_string()));
    }
}
