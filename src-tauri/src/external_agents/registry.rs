use crate::external_agents::defs::{claude, codex, cursor, gemini, hermes, kimi, opencode, pi};
use crate::external_agents::types::RuntimeAgentDef;

pub const AGENT_DEFS: &[RuntimeAgentDef] = &[
    claude::CLAUDE_AGENT_DEF,
    codex::CODEX_AGENT_DEF,
    cursor::CURSOR_AGENT_DEF,
    opencode::OPENCODE_AGENT_DEF,
    gemini::GEMINI_AGENT_DEF,
    kimi::KIMI_AGENT_DEF,
    pi::PI_AGENT_DEF,
    hermes::HERMES_AGENT_DEF,
];

pub fn get_agent_def(id: &str) -> Option<&'static RuntimeAgentDef> {
    AGENT_DEFS.iter().find(|def| def.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_eight_agents() {
        assert_eq!(AGENT_DEFS.len(), 8);
        assert!(get_agent_def("claude").is_some());
        assert!(get_agent_def("opencode").is_some());
        assert!(get_agent_def("pi").is_some());
        assert!(get_agent_def("hermes").is_some());
        assert!(get_agent_def("unknown").is_none());
    }
}
