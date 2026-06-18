//! Persistent cross-turn session registry for external CLI agents (Phase 2).
//!
//! A live session keeps the CLI process alive across user turns so the server holds prior
//! context natively (no full-history replay). Each session is owned by a dedicated actor task
//! reachable only through an `mpsc::Sender<SessionCommand>` — the registry never holds the
//! `Child` or any lock across a turn await, only the cheap clonable control sender.

use tokio::sync::{mpsc, oneshot};

use crate::external_agents::types::{StreamFormat, UnifiedAgentEvent};

/// A command sent to a live session's actor task.
pub enum SessionCommand {
    /// Run one turn: write the prompt, stream `UnifiedAgentEvent`s into `events`, and report the
    /// terminal result through `done`. The actor processes exactly one turn at a time.
    RunTurn {
        prompt: String,
        model: Option<String>,
        reasoning: Option<String>,
        events: mpsc::Sender<UnifiedAgentEvent>,
        done: oneshot::Sender<Result<(), String>>,
    },
    /// Interrupt the in-flight turn without killing the process (protocol-level interrupt).
    Cancel,
    /// Shut the session down (close stdin + kill the child) and end the actor.
    Close,
}

/// Registry entry: the control channel plus metadata used to decide reuse.
pub struct LiveSession {
    pub control: mpsc::Sender<SessionCommand>,
    pub protocol: StreamFormat,
    pub agent_id: String,
    pub cwd: String,
    /// Native session/thread id captured at connect (for resume + diagnostics).
    pub native_id: Option<String>,
}

impl LiveSession {
    /// A session is reusable only if its actor is still listening and it targets the same
    /// agent + working directory as the incoming turn.
    pub fn is_reusable(&self, agent_id: &str, cwd: &str) -> bool {
        !self.control.is_closed() && self.agent_id == agent_id && self.cwd == cwd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(agent: &str, cwd: &str) -> (LiveSession, mpsc::Receiver<SessionCommand>) {
        let (tx, rx) = mpsc::channel(1);
        (
            LiveSession {
                control: tx,
                protocol: StreamFormat::CodexAppServer,
                agent_id: agent.to_string(),
                cwd: cwd.to_string(),
                native_id: Some("thread-1".to_string()),
            },
            rx,
        )
    }

    #[test]
    fn reusable_when_agent_and_cwd_match_and_actor_alive() {
        let (session, _rx) = make("codex", "/proj");
        assert!(session.is_reusable("codex", "/proj"));
        assert!(!session.is_reusable("codex", "/other"));
        assert!(!session.is_reusable("claude", "/proj"));
    }

    #[test]
    fn not_reusable_when_actor_dropped() {
        let (session, rx) = make("codex", "/proj");
        drop(rx); // actor gone → control channel closed
        assert!(!session.is_reusable("codex", "/proj"));
    }
}
