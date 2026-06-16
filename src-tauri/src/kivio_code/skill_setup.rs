//! Skill integration seam for kivio-code (stub — implemented in a later pass).
//!
//! This module owns ALL skill wiring for the headless CLI so skill support can
//! be built out without touching `kivio_code/mod.rs`. `mod.rs` only calls
//! [`build_skill_registry`] when constructing a `TurnAssembly` and
//! [`skill_tool_definitions`] when assembling the per-turn tool set.
use crate::mcp::ChatToolDefinition;
use crate::settings::Settings;
use crate::skills::SkillRegistry;
use std::path::Path;

/// Discover skills (user dir + built-ins) into a registry, headless. STUB:
/// returns an empty registry until skills are wired up.
pub fn build_skill_registry(_settings: &Settings, _cwd: &Path) -> SkillRegistry {
    SkillRegistry::default()
}

/// Extra tool definitions to expose when skills are available (e.g. a
/// skill-activation tool). STUB: none.
pub fn skill_tool_definitions(_registry: &SkillRegistry) -> Vec<ChatToolDefinition> {
    Vec::new()
}
