use std::time::Duration;

use crate::external_agents::registry::AGENT_DEFS;
use crate::external_agents::types::{
    DetectedAgent, RuntimeAgentDef, RuntimeModelOption, default_model_option,
    fallback_models_from_pairs, reasoning_options_from_pairs,
};

pub async fn detect_all_agents() -> Vec<DetectedAgent> {
    let mut out = Vec::new();
    for def in AGENT_DEFS {
        out.push(detect_single_agent(def).await);
    }
    out
}

pub async fn detect_single_agent(def: &RuntimeAgentDef) -> DetectedAgent {
    let path = super::spawn::resolve_binary(def).await;
    let available = path.is_some();
    let version = if available {
        probe_version(def, path.as_deref()).await
    } else {
        None
    };
    let auth_status = if available {
        probe_auth(def, path.as_deref()).await
    } else {
        Some("unavailable".to_string())
    };
    let models = if available {
        probe_models(def, path.as_deref())
            .await
            .unwrap_or_else(|| fallback_models_from_pairs(def.fallback_models))
    } else {
        fallback_models_from_pairs(def.fallback_models)
    };

    DetectedAgent {
        id: def.id.to_string(),
        name: def.name.to_string(),
        available,
        path: path.map(|p| p.to_string_lossy().into_owned()),
        version,
        models,
        reasoning_options: reasoning_options_from_pairs(def.reasoning_options),
        auth_status,
        external_mcp_injection: def.external_mcp_injection,
    }
}

async fn probe_version(def: &RuntimeAgentDef, path: Option<&std::path::Path>) -> Option<String> {
    let bin = path?;
    let output = tokio::process::Command::new(bin)
        .args(def.version_args)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?.trim();
    if line.is_empty() {
        None
    } else {
        Some(line.to_string())
    }
}

async fn probe_auth(def: &RuntimeAgentDef, path: Option<&std::path::Path>) -> Option<String> {
    let args = def.auth_probe_args?;
    let bin = path?;
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::process::Command::new(bin).args(args).output(),
    )
    .await
    .ok()?
    .ok()?;
    if output.status.success() {
        Some("ok".to_string())
    } else {
        Some("auth_required".to_string())
    }
}

async fn probe_models(
    def: &RuntimeAgentDef,
    path: Option<&std::path::Path>,
) -> Option<Vec<RuntimeModelOption>> {
    let args = def.list_models_args?;
    let bin = path?;
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::process::Command::new(bin).args(args).output(),
    )
    .await
    .ok()?
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_models_list(def.id, text.as_ref())
}

fn parse_models_list(agent_id: &str, stdout: &str) -> Option<Vec<RuntimeModelOption>> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() || trimmed.to_lowercase().contains("no models available") {
        return None;
    }
    let mut out = vec![default_model_option()];
    match agent_id {
        "cursor-agent" => {
            for line in trimmed.lines().map(str::trim).filter(|l| !l.is_empty()) {
                if line.eq_ignore_ascii_case("available models") || line.eq_ignore_ascii_case("models")
                {
                    continue;
                }
                let id = line.split_whitespace().next()?.to_string();
                if id == "default" {
                    continue;
                }
                out.push(RuntimeModelOption {
                    id: id.clone(),
                    label: id,
                });
            }
        }
        "codex" => {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if let Some(models) = value.get("models").and_then(|v| v.as_array()) {
                    for entry in models {
                        let id = entry
                            .get("slug")
                            .or_else(|| entry.get("id"))
                            .and_then(|v| v.as_str())?;
                        out.push(RuntimeModelOption {
                            id: id.to_string(),
                            label: id.to_string(),
                        });
                    }
                }
            }
        }
        _ => {}
    }
    if out.len() > 1 {
        Some(out)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cursor_models_skips_header() {
        let models = parse_models_list(
            "cursor-agent",
            "Available models\nauto\nsonnet-4 - Sonnet 4",
        )
        .unwrap();
        assert!(models.iter().any(|m| m.id == "auto"));
        assert!(models.iter().any(|m| m.id == "sonnet-4"));
    }
}
