//! `kivio-code` session storage (Phase 3).
//!
//! This module will hold the JSONL-per-session store for the terminal coding
//! agent: append-only `.jsonl` files grouped by working directory, recording a
//! session header followed by tree-structured entries (message / tool /
//! compaction…), plus the `--continue` / `--resume` / `--session` / list flows
//! and leaf→root reconstruction on resume.
//!
//! Currently a scaffolding stub: no real types are defined yet.

#[cfg(test)]
mod tests {
    #[test]
    fn stub() {}
}
