use crate::external_agents::defs::{claude, codex, cursor};
use crate::external_agents::types::RuntimeAgentDef;

pub const AGENT_DEFS: &[RuntimeAgentDef] =
    &[claude::CLAUDE_AGENT_DEF, codex::CODEX_AGENT_DEF, cursor::CURSOR_AGENT_DEF];

pub fn get_agent_def(id: &str) -> Option<&'static RuntimeAgentDef> {
    AGENT_DEFS.iter().find(|def| def.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_three_agents() {
        assert_eq!(AGENT_DEFS.len(), 3);
        assert!(get_agent_def("claude").is_some());
        assert!(get_agent_def("unknown").is_none());
    }
}
