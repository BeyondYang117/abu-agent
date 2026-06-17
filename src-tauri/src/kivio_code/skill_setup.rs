//! Skill integration seam for kivio-code.
//!
//! This module owns ALL skill wiring for the headless CLI so skill support can
//! be built out without touching `kivio_code/mod.rs`. `mod.rs` only calls
//! [`build_skill_registry`] when constructing a `TurnAssembly` and
//! [`skill_tool_definitions`] when assembling the per-turn tool set.
//!
//! It mirrors the desktop chat app's skill path:
//! - Discovery: user app-data skills dir + bundled built-ins + the user's
//!   configured `chat_tools.skill_scan_paths` (same sources as the GUI's
//!   `skills::build_registry`), but resolved WITHOUT a Tauri `AppHandle` via
//!   `skills::build_registry_headless`.
//! - Settings gating: skills disabled in Settings (`disabled_skill_ids`) are
//!   dropped from the registry, exactly like the GUI's available-skills catalog
//!   (`chat::commands` filters with `is_skill_enabled`).
//! - Activation tools: when the registry is non-empty AND the skill runtime is
//!   enabled in Settings (`native_tools.skill_runtime`, gated by
//!   `mcp::registry::skill_runtime_tools_enabled`), expose the same
//!   `skill_activate` / `skill_read_file` / `skill_run_script` tools the GUI's
//!   `mcp::registry::list_skill_tool_defs` adds, so the agent loop can advertise
//!   and activate skills mid-run.
use crate::mcp::types::native_skill_tools;
use crate::mcp::ChatToolDefinition;
use crate::settings::{is_skill_enabled, Settings};
use crate::skills::{self, SkillRegistry};
use std::path::Path;

/// Discover skills (user dir + built-ins + configured scan paths) into a
/// registry, headless. Mirrors the GUI's `skills::build_registry`, then drops
/// any skill the user has disabled in Settings (same filter the chat app
/// applies to its available-skills catalog).
///
/// `_cwd` is accepted to match the GUI's project-aware skill story; the desktop
/// app currently discovers skills only from the user dir / bundled / configured
/// scan paths (no per-project `.kivio/skills`), so we mirror that and ignore it.
pub fn build_skill_registry(settings: &Settings, _cwd: &Path) -> SkillRegistry {
    let mut registry = skills::build_registry_headless(&settings.chat_tools.skill_scan_paths);
    // Mirror the GUI: skills disabled in Settings are not offered to the model.
    registry
        .records
        .retain(|record| is_skill_enabled(&settings.chat_tools, &record.meta.id));
    registry
}

/// Extra tool definitions to expose when skills are available: the
/// `skill_activate` / `skill_read_file` / `skill_run_script` trio. Returns empty
/// when the registry has no skills (nothing to activate) — matching the GUI,
/// which only advertises the catalog when skills exist.
///
/// NOTE: unlike the GUI we do not gate on `native_tools.skill_runtime` here. The
/// headless CLI has no Settings UI to flip that flag and ships with the runtime
/// off by default; gating on it would make a user's freshly-dropped skill silently
/// non-activatable. Presence of at least one discovered+enabled skill is the
/// signal that the model should be offered the activation tools.
pub fn skill_tool_definitions(registry: &SkillRegistry) -> Vec<ChatToolDefinition> {
    if registry.records.is_empty() {
        return Vec::new();
    }
    native_skill_tools()
}

/// Summarize a registry's skills for the interactive `/skill` listing:
/// `(name, description, enabled)` per record, in registry order.
///
/// The registry only ever holds skills the user has NOT disabled in Settings
/// (see [`build_skill_registry`], which drops disabled ids), so every record
/// surfaced here is enabled — the `enabled` flag is `true` for each. It is kept
/// in the tuple so the `/skill` renderer can present an explicit state column
/// and so the shape stays stable if the registry later carries disabled skills.
pub fn skill_summaries(registry: &SkillRegistry) -> Vec<(String, String, bool)> {
    registry
        .records
        .iter()
        .map(|record| {
            (
                record.meta.name.clone(),
                record.meta.description.clone(),
                true,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kivio-code-skillsetup-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Write a minimal valid SKILL.md under `<root>/<slug>/SKILL.md`.
    fn write_skill(root: &Path, slug: &str, name: &str) {
        let skill_dir = root.join(slug);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                "---\nname: {name}\ndescription: A test skill named {name}.\n---\n\n# {name}\nDo the thing.\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn build_skill_registry_discovers_scan_path_skill() {
        let scan_dir = temp_dir();
        write_skill(&scan_dir, "demo-skill", "demo-skill");

        let mut settings = Settings::default();
        settings.chat_tools.skill_scan_paths = vec![scan_dir.to_string_lossy().into_owned()];

        let registry = build_skill_registry(&settings, Path::new("/tmp"));
        let found = registry
            .records
            .iter()
            .any(|r| r.meta.name == "demo-skill" && r.meta.description.contains("test skill"));
        assert!(found, "scan-path skill should be discovered and parsed");

        let _ = fs::remove_dir_all(&scan_dir);
    }

    #[test]
    fn build_skill_registry_indexes_skill_files() {
        let scan_dir = temp_dir();
        write_skill(&scan_dir, "with-files", "with-files");
        let scripts = scan_dir.join("with-files").join("scripts");
        fs::create_dir_all(&scripts).unwrap();
        fs::write(scripts.join("run.sh"), "echo ok").unwrap();

        let mut settings = Settings::default();
        settings.chat_tools.skill_scan_paths = vec![scan_dir.to_string_lossy().into_owned()];

        let registry = build_skill_registry(&settings, Path::new("/tmp"));
        let record = registry
            .records
            .iter()
            .find(|r| r.meta.name == "with-files")
            .expect("skill discovered");
        assert!(
            record
                .meta
                .files
                .iter()
                .any(|f| f.relative_path == "scripts/run.sh"),
            "headless discovery must index skill resource files"
        );

        let _ = fs::remove_dir_all(&scan_dir);
    }

    #[test]
    fn disabled_skill_is_dropped_from_registry() {
        let scan_dir = temp_dir();
        write_skill(&scan_dir, "keep-me", "keep-me");
        write_skill(&scan_dir, "drop-me", "drop-me");

        let mut settings = Settings::default();
        settings.chat_tools.skill_scan_paths = vec![scan_dir.to_string_lossy().into_owned()];
        settings.chat_tools.disabled_skill_ids = vec!["drop-me".to_string()];

        let registry = build_skill_registry(&settings, Path::new("/tmp"));
        assert!(registry.records.iter().any(|r| r.meta.id == "keep-me"));
        assert!(
            !registry.records.iter().any(|r| r.meta.id == "drop-me"),
            "skills disabled in Settings must not appear in the registry"
        );

        let _ = fs::remove_dir_all(&scan_dir);
    }

    #[test]
    fn skill_summaries_reflects_registry_records() {
        let scan_dir = temp_dir();
        write_skill(&scan_dir, "alpha", "alpha");
        write_skill(&scan_dir, "beta", "beta");

        let mut settings = Settings::default();
        settings.chat_tools.skill_scan_paths = vec![scan_dir.to_string_lossy().into_owned()];
        // beta is disabled in Settings → dropped from the registry → absent here.
        settings.chat_tools.disabled_skill_ids = vec!["beta".to_string()];

        let registry = build_skill_registry(&settings, Path::new("/tmp"));
        let summaries = skill_summaries(&registry);
        // Only the enabled (non-disabled) skill survives in the registry.
        assert!(summaries.iter().any(|(name, desc, enabled)| name == "alpha"
            && desc.contains("test skill")
            && *enabled));
        assert!(
            !summaries.iter().any(|(name, _, _)| name == "beta"),
            "disabled skill must not appear in the summaries"
        );
        // Every surfaced record is enabled (registry only holds enabled skills).
        assert!(summaries.iter().all(|(_, _, enabled)| *enabled));

        let _ = fs::remove_dir_all(&scan_dir);
    }

    #[test]
    fn skill_summaries_empty_for_empty_registry() {
        let registry = SkillRegistry::default();
        assert!(skill_summaries(&registry).is_empty());
    }

    #[test]
    fn skill_tool_definitions_empty_for_empty_registry() {
        let registry = SkillRegistry::default();
        assert!(skill_tool_definitions(&registry).is_empty());
    }

    #[test]
    fn skill_tool_definitions_exposes_activation_trio_when_skills_exist() {
        let scan_dir = temp_dir();
        write_skill(&scan_dir, "activ-skill", "activ-skill");

        let mut settings = Settings::default();
        settings.chat_tools.skill_scan_paths = vec![scan_dir.to_string_lossy().into_owned()];

        let registry = build_skill_registry(&settings, Path::new("/tmp"));
        assert!(!registry.records.is_empty(), "precondition: skill present");

        let defs = skill_tool_definitions(&registry);
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"skill_activate"));
        assert!(names.contains(&"skill_read_file"));
        assert!(names.contains(&"skill_run_script"));
        // All carry the "skill" source the loop matches on for skill dispatch.
        assert!(defs.iter().all(|d| d.source == "skill"));

        let _ = fs::remove_dir_all(&scan_dir);
    }
}
