use super::super::types::{
    PromptInputFormat, RuntimeAgentDef, RuntimeBuildOptions, RuntimeContext, StreamFormat,
    JsonEventParser,
};

const FALLBACK_MODELS: &[(&str, &str)] = &[
    ("default", "Default"),
    ("auto", "auto"),
    ("sonnet-4", "sonnet-4"),
    ("gpt-5", "gpt-5"),
];

pub fn build_cursor_args(ctx: &RuntimeContext, options: &RuntimeBuildOptions) -> Vec<String> {
    let mut args = vec![
        "--print".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--stream-partial-output".to_string(),
        "--force".to_string(),
        "--trust".to_string(),
    ];
    if let Some(cwd) = ctx.cwd.as_ref().filter(|c| !c.is_empty()) {
        args.push("--workspace".to_string());
        args.push(cwd.clone());
    }
    if let Some(model) = options.model.as_ref().filter(|m| *m != "default" && !m.is_empty()) {
        args.push("--model".to_string());
        args.push(model.clone());
    }
    args
}

pub const CURSOR_AGENT_DEF: RuntimeAgentDef = RuntimeAgentDef {
    id: "cursor-agent",
    name: "Cursor Agent",
    bin: "cursor-agent",
    fallback_bins: &[],
    version_args: &["--version"],
    auth_probe_args: Some(&["status"]),
    fallback_models: FALLBACK_MODELS,
    reasoning_options: &[],
    list_models_args: Some(&["models"]),
    prompt_via_stdin: true,
    prompt_input_format: PromptInputFormat::Text,
    stream_format: StreamFormat::JsonEventStream,
    json_event_parser: Some(JsonEventParser::CursorAgent),
    external_mcp_injection: None,
    resumes_session_via_cli: false,
    build_args: build_cursor_args,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_build_args_includes_workspace() {
        let args = build_cursor_args(
            &RuntimeContext {
                cwd: Some("/proj".to_string()),
                extra_allowed_dirs: vec![],
                resume_session_id: None,
                new_session_id: None,
                include_partial_messages: false,
            },
            &RuntimeBuildOptions {
                model: Some("auto".to_string()),
                reasoning: None,
            },
        );
        assert!(args.contains(&"--workspace".to_string()));
        assert!(args.contains(&"/proj".to_string()));
        assert!(!args.iter().any(|a| a == "-"));
    }
}
