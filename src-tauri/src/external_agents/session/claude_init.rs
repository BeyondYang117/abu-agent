use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use tokio::io::AsyncBufReadExt;
use uuid::Uuid;

use crate::external_agents::registry::get_agent_def;
use crate::external_agents::slash::is_claude_init;
use crate::external_agents::spawn::{parse_json_line, spawn_agent, write_probe_stdin};
use crate::external_agents::types::{RuntimeBuildOptions, RuntimeContext, RuntimeModelOption};

/// `--model` aliases accepted by Claude Code. We probe each alias via init and keep
/// only aliases the CLI resolves successfully (no local display fallback).
const CLAUDE_MODEL_ALIASES: &[&str] = &[
    "opus",
    "sonnet",
    "sonnet[1m]",
    "opus[1m]",
    "haiku",
    "fable",
    "fable[1m]",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeInitInfo {
    pub resolved_model: String,
    pub context_window_tokens: Option<u32>,
}

pub fn context_window_from_claude_resolved_model(resolved: &str) -> Option<u32> {
    let trimmed = resolved.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.to_ascii_lowercase().ends_with("[1m]") {
        return Some(1_000_000);
    }
    if trimmed.to_ascii_lowercase().contains("claude-") {
        return Some(200_000);
    }
    None
}

pub fn context_window_from_claude_model_alias(alias: &str) -> Option<u32> {
    let alias = alias.trim();
    if alias.is_empty() || alias == "default" {
        return None;
    }
    if alias.to_ascii_lowercase().contains("[1m]") {
        return Some(1_000_000);
    }
    if CLAUDE_MODEL_ALIASES.contains(&alias) {
        return Some(200_000);
    }
    None
}

pub fn label_for_claude_model(alias: &str, resolved: &str) -> String {
    let human = humanize_claude_resolved_model(resolved);
    if alias == "default" {
        format!("Default ({human})")
    } else if alias == "sonnet[1m]" {
        format!("Sonnet (1M context)")
    } else {
        human
    }
}

fn humanize_claude_resolved_model(resolved: &str) -> String {
    let mut base = resolved.trim().to_string();
    let has_1m = base.to_ascii_lowercase().ends_with("[1m]");
    if has_1m {
        base.truncate(base.len().saturating_sub(4));
    }
    if let Some(rest) = base.strip_prefix("claude-") {
        base = rest.to_string();
    }
    let parts: Vec<&str> = base.split('-').filter(|part| !part.is_empty()).collect();
    let label = if parts.is_empty() {
        base
    } else {
        let family = title_case_token(parts[0]);
        if parts.len() >= 3
            && parts[1].chars().all(|ch| ch.is_ascii_digit())
            && parts[2].chars().all(|ch| ch.is_ascii_digit())
        {
            format!("{family} {}.{}", parts[1], parts[2])
        } else if parts.len() >= 2 && parts[1].chars().all(|ch| ch.is_ascii_digit()) {
            format!("{family} {}", parts[1])
        } else {
            parts
                .iter()
                .map(|part| title_case_token(part))
                .collect::<Vec<_>>()
                .join(" ")
        }
    };
    if has_1m {
        format!("{label} (1M context)")
    } else {
        label
    }
}

fn title_case_token(token: &str) -> String {
    let lower = token.to_ascii_lowercase();
    if lower.is_empty() {
        return lower;
    }
    let mut chars = lower.chars();
    let first = chars.next().unwrap().to_ascii_uppercase().to_string();
    first + chars.as_str()
}

pub async fn probe_claude_init(
    resolved_bin: &Path,
    cwd: &Path,
    model_alias: Option<&str>,
) -> Option<ClaudeInitInfo> {
    let def = get_agent_def("claude")?;
    let runtime_ctx = RuntimeContext {
        cwd: Some(cwd.to_string_lossy().into_owned()),
        extra_allowed_dirs: Vec::new(),
        resume_session_id: None,
        new_session_id: Some(Uuid::new_v4().to_string()),
        include_partial_messages: false,
    };
    let build_options = RuntimeBuildOptions {
        model: model_alias
            .filter(|value| !value.is_empty() && *value != "default")
            .map(str::to_string),
        reasoning: None,
    };
    let args = (def.build_args)(&runtime_ctx, &build_options, None);
    let extra_env = std::collections::HashMap::new();
    let mut spawned = spawn_agent(def, resolved_bin, &args, cwd, &extra_env)
        .await
        .ok()?;
    write_probe_stdin(&mut spawned.child).await.ok()?;

    let init = read_claude_init_value(&mut spawned.child, Duration::from_secs(20)).await?;
    let _ = spawned.child.start_kill();
    let _ = spawned.child.wait().await;

    parse_claude_init_info(&init)
}

pub async fn detect_claude_models(resolved_bin: &Path, cwd: &Path) -> Option<Vec<RuntimeModelOption>> {
    let mut out = Vec::new();

    if let Some(info) = probe_claude_init(resolved_bin, cwd, None).await {
        out.push(model_option_from_probe("default", &info));
    }

    let mut handles = Vec::new();
    for alias in CLAUDE_MODEL_ALIASES {
        let bin = resolved_bin.to_path_buf();
        let cwd = cwd.to_path_buf();
        let alias = (*alias).to_string();
        handles.push(tokio::spawn(async move {
            let info = probe_claude_init(&bin, &cwd, Some(&alias)).await?;
            Some((alias, info))
        }));
    }

    for handle in handles {
        if let Ok(Some((alias, info))) = handle.await {
            out.push(model_option_from_probe(&alias, &info));
        }
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn model_option_from_probe(alias: &str, info: &ClaudeInitInfo) -> RuntimeModelOption {
    RuntimeModelOption {
        id: alias.to_string(),
        label: label_for_claude_model(alias, &info.resolved_model),
        context_window_tokens: info.context_window_tokens,
    }
}

pub fn parse_claude_init_info(value: &Value) -> Option<ClaudeInitInfo> {
    if !is_claude_init(value) {
        return None;
    }
    let resolved_model = value.get("model").and_then(|v| v.as_str())?.trim();
    if resolved_model.is_empty() {
        return None;
    }
    Some(ClaudeInitInfo {
        resolved_model: resolved_model.to_string(),
        context_window_tokens: context_window_from_claude_resolved_model(resolved_model),
    })
}

async fn read_claude_init_value(
    child: &mut tokio::process::Child,
    timeout: Duration,
) -> Option<Value> {
    let stdout = child.stdout.as_mut()?;
    let mut reader = tokio::io::BufReader::new(stdout).lines();
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), reader.next_line()).await {
            Ok(Ok(Some(line))) => {
                if line.trim().is_empty() {
                    continue;
                }
                if let Some(value) = parse_json_line(&line) {
                    if is_claude_init(&value) {
                        return Some(value);
                    }
                }
            }
            Ok(Ok(None)) => break,
            Ok(Err(_)) => break,
            Err(_) => continue,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn context_window_from_resolved_model() {
        assert_eq!(
            context_window_from_claude_resolved_model("claude-opus-4-8[1m]"),
            Some(1_000_000)
        );
        assert_eq!(
            context_window_from_claude_resolved_model("claude-sonnet-4-6"),
            Some(200_000)
        );
    }

    #[test]
    fn context_window_from_alias() {
        assert_eq!(context_window_from_claude_model_alias("sonnet[1m]"), Some(1_000_000));
        assert_eq!(context_window_from_claude_model_alias("sonnet"), Some(200_000));
        assert_eq!(context_window_from_claude_model_alias("default"), None);
    }

    #[test]
    fn labels_match_cli_picker() {
        assert_eq!(
            label_for_claude_model("default", "claude-opus-4-8[1m]"),
            "Default (Opus 4.8 (1M context))"
        );
        assert_eq!(
            label_for_claude_model("sonnet[1m]", "claude-sonnet-4-6[1m]"),
            "Sonnet (1M context)"
        );
    }

    #[test]
    fn parse_init_info() {
        let init = json!({
            "type": "system",
            "subtype": "init",
            "model": "claude-opus-4-8[1m]"
        });
        let info = parse_claude_init_info(&init).unwrap();
        assert_eq!(info.resolved_model, "claude-opus-4-8[1m]");
        assert_eq!(info.context_window_tokens, Some(1_000_000));
    }

    #[test]
    fn parse_context_window_label_still_works() {
        use crate::external_agents::context::parse_context_window_label;
        assert_eq!(parse_context_window_label("1m"), Some(1_000_000));
        assert_eq!(parse_context_window_label("200K"), Some(200_000));
    }

    #[tokio::test]
    #[ignore = "requires local claude CLI on PATH"]
    async fn live_detect_claude_models_from_cli() {
        use crate::external_agents::detection::detect_single_agent;
        use crate::external_agents::registry::get_agent_def;

        let def = get_agent_def("claude").expect("claude agent def");
        let detected = detect_single_agent(def).await;
        assert!(detected.available, "claude CLI should be available on PATH");
        for model in &detected.models {
            println!(
                "  {} -> {} ({} tokens)",
                model.id,
                model.label,
                model
                    .context_window_tokens
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "?".to_string())
            );
        }
        assert!(
            detected.models.len() >= 4,
            "expected multiple probed models, got {:?}",
            detected.models
        );

        let default = detected
            .models
            .iter()
            .find(|model| model.id == "default")
            .expect("default model option");
        assert_eq!(
            default.context_window_tokens,
            Some(1_000_000),
            "default should resolve to 1M context"
        );

        let sonnet = detected
            .models
            .iter()
            .find(|model| model.id == "sonnet")
            .expect("sonnet model option");
        assert_eq!(
            sonnet.context_window_tokens,
            Some(200_000),
            "standard sonnet should be 200K"
        );

        let sonnet_1m = detected
            .models
            .iter()
            .find(|model| model.id == "sonnet[1m]")
            .expect("sonnet[1m] model option");
        assert_eq!(
            sonnet_1m.context_window_tokens,
            Some(1_000_000),
            "sonnet[1m] should be 1M"
        );
    }
}
