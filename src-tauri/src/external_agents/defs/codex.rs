use super::super::types::{
    PromptInputFormat, RuntimeAgentDef, RuntimeBuildOptions, RuntimeContext, StreamFormat,
    JsonEventParser,
};

const FALLBACK_MODELS: &[(&str, &str)] = &[
    ("default", "Default"),
    ("gpt-5.3-codex", "gpt-5.3-codex"),
    ("gpt-5", "gpt-5"),
    ("o3", "o3"),
];

const REASONING: &[(&str, &str)] = &[
    ("default", "Default"),
    ("none", "None"),
    ("minimal", "Minimal"),
    ("low", "Low"),
    ("medium", "Medium"),
    ("high", "High"),
    ("xhigh", "XHigh"),
];

fn codex_needs_danger_full_access() -> bool {
    if std::env::var("KIVIO_CODEX_SANDBOX")
        .ok()
        .as_deref()
        .map(str::trim)
        == Some("danger-full-access")
    {
        return true;
    }
    if cfg!(target_os = "windows") {
        return true;
    }
    std::env::var("WSL_DISTRO_NAME")
        .ok()
        .is_some_and(|v| !v.trim().is_empty())
}

pub fn build_codex_args(ctx: &RuntimeContext, options: &RuntimeBuildOptions) -> Vec<String> {
    let mut args = if codex_needs_danger_full_access() {
        vec![
            "exec".to_string(),
            "--json".to_string(),
            "--skip-git-repo-check".to_string(),
            "--sandbox".to_string(),
            "danger-full-access".to_string(),
        ]
    } else {
        vec![
            "exec".to_string(),
            "--json".to_string(),
            "--skip-git-repo-check".to_string(),
            "--sandbox".to_string(),
            "workspace-write".to_string(),
            "-c".to_string(),
            "sandbox_workspace_write.network_access=true".to_string(),
        ]
    };
    args.push("-c".to_string());
    args.push("default_permissions=\":workspace\"".to_string());
    if let Some(cwd) = ctx.cwd.as_ref().filter(|c| !c.is_empty()) {
        args.push("-C".to_string());
        args.push(cwd.clone());
    }
    for dir in &ctx.extra_allowed_dirs {
        if !dir.is_empty() {
            args.push("--add-dir".to_string());
            args.push(dir.clone());
        }
    }
    if let Some(model) = options.model.as_ref().filter(|m| *m != "default" && !m.is_empty()) {
        args.push("--model".to_string());
        args.push(model.clone());
    }
    if let Some(reasoning) = options
        .reasoning
        .as_ref()
        .filter(|r| *r != "default" && !r.is_empty())
    {
        args.push("-c".to_string());
        args.push(format!("model_reasoning_effort=\"{reasoning}\""));
    }
    args
}

pub const CODEX_AGENT_DEF: RuntimeAgentDef = RuntimeAgentDef {
    id: "codex",
    name: "Codex CLI",
    bin: "codex",
    fallback_bins: &[],
    version_args: &["--version"],
    auth_probe_args: Some(&["login", "status"]),
    fallback_models: FALLBACK_MODELS,
    reasoning_options: REASONING,
    list_models_args: Some(&["debug", "models"]),
    prompt_via_stdin: true,
    prompt_input_format: PromptInputFormat::Text,
    stream_format: StreamFormat::JsonEventStream,
    json_event_parser: Some(JsonEventParser::Codex),
    external_mcp_injection: None,
    resumes_session_via_cli: false,
    build_args: build_codex_args,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_build_args_workspace_write_on_unix() {
        if cfg!(target_os = "windows") {
            return;
        }
        let args = build_codex_args(
            &RuntimeContext {
                cwd: Some("/tmp/p".to_string()),
                extra_allowed_dirs: vec![],
                resume_session_id: None,
                new_session_id: None,
                include_partial_messages: false,
            },
            &RuntimeBuildOptions {
                model: Some("gpt-5".to_string()),
                reasoning: Some("high".to_string()),
            },
        );
        assert!(args.contains(&"workspace-write".to_string()));
        assert!(args.contains(&"-C".to_string()));
    }
}
