//! Process `PATH` enrichment for macOS GUI launches.
//!
//! A `.app` launched from Finder/Dock/Launchpad on macOS does **not** inherit
//! the user's login-shell `PATH`. It gets only a minimal default
//! (`/usr/bin:/bin:/usr/sbin:/sbin`). User-installed CLIs (claude / codex / pi,
//! Homebrew, npm globals, …) live in `/opt/homebrew/bin`, `/usr/local/bin`,
//! `~/.local/bin`, `~/.cargo/bin`, etc. — none of which are on that minimal
//! `PATH`. As a result CLI probing (`external_agents::spawn::which_binary`,
//! which shells out to `which`) finds nothing in the packaged build, even
//! though `npm run dev` (launched from a terminal with the full shell `PATH`)
//! works fine.
//!
//! [`enrich_path_macos`] runs once at the very start of app startup, before any
//! window creation or CLI probing. It merges the current `PATH`, the user's
//! login-shell `PATH`, and a fixed set of common install directories, then
//! writes the deduplicated result back via `std::env::set_var`. Because every
//! downstream subprocess (detection, `spawn_agent`, MCP stdio servers, skill
//! scripts) inherits the process `PATH`, a single fix here covers all of them.
//!
//! On non-macOS platforms this module compiles to nothing: Windows GUI
//! programs already read `PATH` from the registry, and the call site is
//! `#[cfg(target_os = "macos")]`.

#![cfg(target_os = "macos")]

use std::collections::HashSet;
use std::time::Duration;

/// Hard timeout for invoking the login shell to read its `PATH`. Some users'
/// shell rc files are slow (network calls, version managers); we must never
/// block app startup on them, so we cap the wait and fall back to the
/// common-directory defaults if it doesn't return in time.
const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(3);

/// Merge the current `PATH` with the login-shell `PATH` and common install
/// directories, deduplicate (preserving order), and write the result back to
/// the process `PATH`. Safe to call multiple times and harmless in `dev`
/// (where the process already has the full shell `PATH`).
pub fn enrich_path_macos() {
    let current = std::env::var("PATH").unwrap_or_default();
    let login = login_shell_path();
    let defaults = common_dirs(std::env::var_os("HOME").map(std::path::PathBuf::from));

    let merged = merge_paths(&current, login.as_deref(), &defaults);
    if !merged.is_empty() {
        std::env::set_var("PATH", merged);
    }
}

/// Merge the current `PATH`, the (optional) login-shell `PATH`, and the
/// fallback `defaults` into a single `:`-joined string, deduplicated and
/// order-preserving. Pure — no env access — so it is unit-testable without
/// mutating shared process state.
///
/// Order: existing `PATH` first (preserves current resolution order), then any
/// login-shell additions, then defaults for entries neither source provided.
fn merge_paths(current: &str, login: Option<&str>, defaults: &[String]) -> String {
    let mut seen: HashSet<String> = HashSet::new();
    let mut merged: Vec<String> = Vec::new();
    for source in [current, login.unwrap_or("")] {
        for seg in source.split(':') {
            push_unique(seg, &mut seen, &mut merged);
        }
    }
    for dir in defaults {
        push_unique(dir, &mut seen, &mut merged);
    }
    merged.join(":")
}

/// Push `seg` onto `out` if non-empty and not already present.
fn push_unique(seg: &str, seen: &mut HashSet<String>, out: &mut Vec<String>) {
    let seg = seg.trim();
    if seg.is_empty() {
        return;
    }
    if seen.insert(seg.to_string()) {
        out.push(seg.to_string());
    }
}

/// Common directories where CLIs get installed but which are absent from the
/// minimal Finder/Dock `PATH`. `$HOME`-relative entries are expanded against
/// `home`; if `home` is `None`/empty those entries are simply skipped. Takes
/// `home` as a parameter (rather than reading `$HOME`) so it is testable
/// without env mutation.
fn common_dirs(home: Option<std::path::PathBuf>) -> Vec<String> {
    let mut dirs = vec![
        "/opt/homebrew/bin".to_string(),
        "/opt/homebrew/sbin".to_string(),
        "/usr/local/bin".to_string(),
        "/usr/local/sbin".to_string(),
        "/usr/bin".to_string(),
        "/bin".to_string(),
        "/usr/sbin".to_string(),
        "/sbin".to_string(),
    ];
    if let Some(home) = home {
        if !home.as_os_str().is_empty() {
            for rel in [".local/bin", ".cargo/bin", ".bun/bin"] {
                dirs.push(home.join(rel).to_string_lossy().to_string());
            }
        }
    }
    dirs
}

/// Read the user's login-shell `PATH` by running
/// `$SHELL -l -i -c 'echo $PATH'` with a hard timeout. Returns `None` on any
/// failure (spawn error, non-zero exit, timeout, empty output) so the caller
/// falls back to the common-directory defaults. Never panics, never blocks
/// past [`LOGIN_SHELL_TIMEOUT`].
fn login_shell_path() -> Option<String> {
    use crate::proc::NoConsoleWindow;
    use std::process::{Command, Stdio};

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

    // Spawn the login+interactive shell so it sources the rc files that set
    // PATH (e.g. ~/.zshrc), then echo the resulting PATH on a single line.
    let child = Command::new(&shell)
        .args(["-l", "-i", "-c", "echo \"$PATH\""])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .no_console_window()
        .spawn()
        .ok()?;

    // Enforce the timeout on a helper thread: if the shell hangs (slow rc),
    // give up rather than blocking startup.
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        let out = child.wait_with_output();
        let _ = tx.send(out);
    });

    match rx.recv_timeout(LOGIN_SHELL_TIMEOUT) {
        Ok(Ok(output)) if output.status.success() => {
            let _ = handle.join();
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() {
                None
            } else {
                Some(path)
            }
        }
        // Non-zero exit, I/O error, or timeout: detach the helper thread and
        // bail. We can't easily kill the child after moving it into the thread,
        // but it will exit on its own (echo is instant; the worst case is a
        // hung rc that the OS reaps when the orphaned thread's child closes its
        // pipe). Startup proceeds with the defaults regardless.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn push_unique_skips_empty_and_dupes() {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        push_unique("/a", &mut seen, &mut out);
        push_unique("", &mut seen, &mut out);
        push_unique("  ", &mut seen, &mut out);
        push_unique("/a", &mut seen, &mut out);
        push_unique("/b", &mut seen, &mut out);
        assert_eq!(out, vec!["/a".to_string(), "/b".to_string()]);
    }

    #[test]
    fn common_dirs_includes_homebrew() {
        let dirs = common_dirs(None);
        assert!(dirs.iter().any(|d| d == "/opt/homebrew/bin"));
        assert!(dirs.iter().any(|d| d == "/usr/local/bin"));
    }

    #[test]
    fn common_dirs_expands_home() {
        let dirs = common_dirs(Some(PathBuf::from("/Users/tester")));
        assert!(dirs.iter().any(|d| d == "/Users/tester/.local/bin"));
        assert!(dirs.iter().any(|d| d == "/Users/tester/.cargo/bin"));
        assert!(dirs.iter().any(|d| d == "/Users/tester/.bun/bin"));
    }

    #[test]
    fn common_dirs_skips_empty_home() {
        let dirs = common_dirs(Some(PathBuf::from("")));
        assert!(!dirs.iter().any(|d| d.contains(".local/bin")));
    }

    /// Simulate the minimal Finder/Dock PATH and confirm merging folds in the
    /// common install dirs without dropping the originals, deduped + in order.
    /// Pure (no env mutation) so it can't pollute sibling tests.
    #[test]
    fn merge_from_minimal_path_adds_common_dirs() {
        let current = "/usr/bin:/bin:/usr/sbin:/sbin";
        let defaults = common_dirs(Some(PathBuf::from("/Users/tester")));

        // No login shell available -> defaults-only fallback path.
        let result = merge_paths(current, None, &defaults);
        let segs: Vec<&str> = result.split(':').collect();

        // Originals preserved and first (order kept).
        assert_eq!(&segs[0..4], &["/usr/bin", "/bin", "/usr/sbin", "/sbin"]);
        // Common dirs folded in.
        assert!(segs.contains(&"/opt/homebrew/bin"));
        assert!(segs.contains(&"/usr/local/bin"));
        assert!(segs.contains(&"/Users/tester/.local/bin"));
        assert!(segs.contains(&"/Users/tester/.cargo/bin"));
        // No duplicates.
        let mut unique = segs.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), segs.len(), "PATH has duplicate segments");
    }

    /// Login-shell PATH entries are merged after the current PATH but before
    /// defaults, and overlap is deduped.
    #[test]
    fn merge_includes_login_shell_path() {
        let current = "/usr/bin:/bin";
        let login = "/opt/homebrew/bin:/usr/bin"; // /usr/bin overlaps
        let defaults = common_dirs(None);

        let result = merge_paths(current, Some(login), &defaults);
        let segs: Vec<&str> = result.split(':').collect();

        assert_eq!(segs[0], "/usr/bin");
        assert_eq!(segs[1], "/bin");
        // login-only entry comes right after current PATH, before /usr/local/bin default.
        assert_eq!(segs[2], "/opt/homebrew/bin");
        // /usr/bin not duplicated.
        assert_eq!(segs.iter().filter(|s| **s == "/usr/bin").count(), 1);
    }
}
