use super::super::types::{
    ExternalMcpInjection, JsonEventParser, PromptInputFormat, RuntimeAgentDef, RuntimeBuildOptions,
    RuntimeContext, StreamFormat,
};

const FALLBACK_MODELS: &[(&str, &str)] = &[
    ("default", "Default"),
    (
        "anthropic/claude-sonnet-4-5",
        "anthropic/claude-sonnet-4-5",
    ),
    ("openai/gpt-5", "openai/gpt-5"),
    ("google/gemini-2.5-pro", "google/gemini-2.5-pro"),
];

pub fn build_opencode_args(
    _ctx: &RuntimeContext,
    options: &RuntimeBuildOptions,
    _prompt: Option<&str>,
) -> Vec<String> {
    let mut args = vec![
        "run".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    if let Some(model) = options.model.as_ref().filter(|m| *m != "default" && !m.is_empty()) {
        args.push("-m".to_string());
        args.push(model.clone());
    }
    args
}

pub const OPENCODE_AGENT_DEF: RuntimeAgentDef = RuntimeAgentDef {
    id: "opencode",
    name: "OpenCode",
    bin: "opencode-cli",
    fallback_bins: &["opencode"],
    version_args: &["--version"],
    auth_probe_args: None,
    fallback_models: FALLBACK_MODELS,
    reasoning_options: &[],
    list_models_args: Some(&["models"]),
    list_models_timeout_secs: Some(15),
    models_from_stderr: false,
    model_probe: None,
    model_probe_args: None,
    env: &[],
    max_prompt_arg_bytes: None,
    prompt_via_stdin: true,
    prompt_input_format: PromptInputFormat::Text,
    stream_format: StreamFormat::JsonEventStream,
    json_event_parser: Some(JsonEventParser::OpenCode),
    external_mcp_injection: Some(ExternalMcpInjection::OpenCodeEnvContent),
    resumes_session_via_cli: false,
    build_args: build_opencode_args,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opencode_build_args_includes_model() {
        let args = build_opencode_args(
            &RuntimeContext {
                cwd: None,
                extra_allowed_dirs: vec![],
                resume_session_id: None,
                new_session_id: None,
                include_partial_messages: false,
            },
            &RuntimeBuildOptions {
                model: Some("anthropic/claude-sonnet-4-5".to_string()),
                reasoning: None,
            },
            None,
        );
        assert!(args.contains(&"run".to_string()));
        assert!(args.contains(&"-m".to_string()));
    }
}
