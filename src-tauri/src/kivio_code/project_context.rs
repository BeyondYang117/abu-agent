//! Auto-load project context files and wrap them for the system prompt.
//!
//! When kivio-code runs inside a project, the model should automatically see the
//! project's own instruction files (the way Claude Code reads `CLAUDE.md` and PI
//! reads `AGENTS.md`). This module discovers those files and renders them into a
//! single `<project_context>` block of `<project_instructions path="…">…` entries,
//! mirroring PI's `system-prompt.ts` wrapping.
//!
//! ## Discovery order
//!
//! Files are emitted **root-first so the closest / most specific file appears
//! last** — later text can override earlier text in the model's reading:
//!
//! 1. **Global**: `<app_data>/agents/AGENTS.md` (the user's global instructions),
//!    if present. Resolved via [`settings_loader::app_data_dir`].
//! 2. **Ancestor files**: walk from filesystem root down to (and including)
//!    `cwd`. For each ancestor directory take the FIRST existing of `AGENTS.md`,
//!    `KIVIO.md`, `CLAUDE.md` (one per directory). Root-first ordering means the
//!    file in `cwd` itself comes after its ancestors.
//! 3. **Project `.agent/`** (in `cwd` only): `.agent/AGENTS.md` first, then the
//!    remaining top-level `.agent/*.md` files sorted by name. NON-recursive — we
//!    do not descend into `.agent/` subdirectories.
//!
//! De-duplication is by canonical path (a file referenced twice is included
//! once, at its first occurrence). The combined output is capped at
//! [`MAX_CONTEXT_BYTES`]; once the cap is hit a truncation marker is appended and
//! no further files are added.

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::settings_loader;

/// Total byte cap for the rendered project-context block (~64 KB). Once reached
/// we stop adding files and append a truncation marker so the model knows
/// content was elided.
pub const MAX_CONTEXT_BYTES: usize = 64 * 1024;

/// Candidate instruction filenames, in priority order, taken per-directory when
/// walking ancestors. The FIRST one that exists in a directory wins for that dir.
const ANCESTOR_CANDIDATES: [&str; 3] = ["AGENTS.md", "KIVIO.md", "CLAUDE.md"];

/// Discover and render the project context for a run rooted at `cwd`.
///
/// Returns an empty string when nothing relevant is found; otherwise a
/// `<project_context>` block ready to splice into the system prompt. See the
/// module docs for the discovery order and wrapping format.
pub fn load_project_context(cwd: &Path) -> String {
    let files = discover_context_files(cwd);
    render_context_block(&files)
}

/// A discovered context file: its display path (what goes in the `path="…"`
/// attribute) and its contents.
struct ContextFile {
    display_path: String,
    content: String,
}

/// Collect the context files in discovery order, de-duplicated by canonical path.
fn discover_context_files(cwd: &Path) -> Vec<ContextFile> {
    let mut paths: Vec<PathBuf> = Vec::new();

    // 1. Global user instructions: <app_data>/agents/AGENTS.md
    if let Some(app_data) = settings_loader::app_data_dir() {
        let global = app_data.join("agents").join("AGENTS.md");
        if global.is_file() {
            paths.push(global);
        }
    }

    // 2. Ancestor files, root-first (closest dir = cwd comes last so it can
    //    override). Collect cwd→root, then reverse to root→cwd.
    let mut ancestor_dirs: Vec<&Path> = Vec::new();
    let mut dir: Option<&Path> = Some(cwd);
    while let Some(d) = dir {
        ancestor_dirs.push(d);
        dir = d.parent();
    }
    ancestor_dirs.reverse();
    for d in ancestor_dirs {
        for candidate in ANCESTOR_CANDIDATES {
            let p = d.join(candidate);
            if p.is_file() {
                paths.push(p);
                break; // first existing candidate per directory
            }
        }
    }

    // 3. Project `.agent/` in cwd: AGENTS.md first, then other top-level *.md
    //    sorted by name. NON-recursive.
    let agent_dir = cwd.join(".agent");
    if agent_dir.is_dir() {
        let agents_md = agent_dir.join("AGENTS.md");
        if agents_md.is_file() {
            paths.push(agents_md.clone());
        }
        // BTreeMap keyed by filename → sorted by name.
        let mut others: BTreeMap<String, PathBuf> = BTreeMap::new();
        if let Ok(entries) = std::fs::read_dir(&agent_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let name = match path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n.to_string(),
                    None => continue,
                };
                if !name.to_lowercase().ends_with(".md") {
                    continue;
                }
                if name == "AGENTS.md" {
                    continue; // already added first
                }
                others.insert(name, path);
            }
        }
        for (_, path) in others {
            paths.push(path);
        }
    }

    // De-dup by canonical path, preserving first-seen order, then read contents.
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut files: Vec<ContextFile> = Vec::new();
    for path in paths {
        let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if !seen.insert(canonical) {
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if content.trim().is_empty() {
            continue;
        }
        files.push(ContextFile {
            display_path: path.to_string_lossy().into_owned(),
            content,
        });
    }

    files
}

/// Render the discovered files into a `<project_context>` block, applying the
/// total byte cap. Returns "" when there are no files.
fn render_context_block(files: &[ContextFile]) -> String {
    if files.is_empty() {
        return String::new();
    }

    let mut out = String::from("<project_context>\n\n");
    let mut truncated = false;
    for file in files {
        let entry = format!(
            "<project_instructions path=\"{}\">\n{}\n</project_instructions>\n\n",
            file.display_path,
            file.content.trim_end()
        );
        // Reserve room for the closing tag; stop before exceeding the cap.
        if out.len() + entry.len() > MAX_CONTEXT_BYTES {
            truncated = true;
            break;
        }
        out.push_str(&entry);
    }

    if truncated {
        out.push_str("[project context truncated: byte cap reached]\n\n");
    }
    out.push_str("</project_context>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kivio-ctx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, content).expect("write file");
    }

    #[test]
    fn empty_when_nothing_found() {
        let dir = temp_dir();
        let out = load_project_context(&dir);
        assert_eq!(out, "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn agent_dir_files_loaded_in_order() {
        let dir = temp_dir();
        write(&dir.join(".agent").join("AGENTS.md"), "agents instructions");
        write(&dir.join(".agent").join("zeta.md"), "zeta extra");
        write(&dir.join(".agent").join("alpha.md"), "alpha extra");

        let out = load_project_context(&dir);
        assert!(out.contains("<project_context>"));
        assert!(out.contains("agents instructions"));
        assert!(out.contains("alpha extra"));
        assert!(out.contains("zeta extra"));

        // AGENTS.md first, then alpha (sorted) before zeta.
        let pos_agents = out.find("agents instructions").unwrap();
        let pos_alpha = out.find("alpha extra").unwrap();
        let pos_zeta = out.find("zeta extra").unwrap();
        assert!(pos_agents < pos_alpha, "AGENTS.md must come first");
        assert!(pos_alpha < pos_zeta, "alpha must sort before zeta");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn agent_dir_subdirs_not_recursed() {
        let dir = temp_dir();
        write(&dir.join(".agent").join("top.md"), "top level");
        write(&dir.join(".agent").join("nested").join("deep.md"), "deep nested");

        let out = load_project_context(&dir);
        assert!(out.contains("top level"));
        assert!(!out.contains("deep nested"), ".agent subdirs must not be recursed");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn root_agents_and_claude_picked_up() {
        // A project root with AGENTS.md, plus a nested cwd with CLAUDE.md.
        let root = temp_dir();
        write(&root.join("AGENTS.md"), "root agents");
        let cwd = root.join("sub");
        write(&cwd.join("CLAUDE.md"), "sub claude");

        let out = load_project_context(&cwd);
        assert!(out.contains("root agents"));
        assert!(out.contains("sub claude"));
        // Root-first: ancestor root file comes before the closer cwd file.
        let pos_root = out.find("root agents").unwrap();
        let pos_sub = out.find("sub claude").unwrap();
        assert!(pos_root < pos_sub, "root file must appear before the closer cwd file");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn first_candidate_per_directory_wins() {
        let dir = temp_dir();
        // Both AGENTS.md and CLAUDE.md in the same dir → only AGENTS.md taken.
        write(&dir.join("AGENTS.md"), "preferred agents");
        write(&dir.join("CLAUDE.md"), "ignored claude");

        let out = load_project_context(&dir);
        assert!(out.contains("preferred agents"));
        assert!(!out.contains("ignored claude"), "second candidate in same dir must be skipped");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dedup_by_canonical_path() {
        // A file reachable both as an ancestor AGENTS.md and as `.agent/AGENTS.md`
        // would be distinct paths, so instead test that the same file content
        // does not appear twice when a single AGENTS.md is the only source.
        let dir = temp_dir();
        write(&dir.join("AGENTS.md"), "unique-marker-xyz");

        let out = load_project_context(&dir);
        let count = out.matches("unique-marker-xyz").count();
        assert_eq!(count, 1, "content must appear exactly once");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn byte_cap_truncates() {
        let dir = temp_dir();
        // Two large files; the second should push past the cap.
        let big = "x".repeat(MAX_CONTEXT_BYTES);
        write(&dir.join(".agent").join("AGENTS.md"), &big);
        write(&dir.join(".agent").join("second.md"), &big);

        let out = load_project_context(&dir);
        assert!(out.contains("truncated"), "truncation marker expected");
        assert!(out.len() <= MAX_CONTEXT_BYTES + 256, "output stays near the cap");
        assert!(out.contains("</project_context>"), "block must still close");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wrapping_format_matches_pi() {
        let dir = temp_dir();
        write(&dir.join("AGENTS.md"), "hello");

        let out = load_project_context(&dir);
        assert!(out.starts_with("<project_context>"));
        assert!(out.contains("<project_instructions path=\""));
        assert!(out.contains("</project_instructions>"));
        assert!(out.trim_end().ends_with("</project_context>"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
