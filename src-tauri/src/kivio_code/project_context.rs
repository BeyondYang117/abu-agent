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
//!    if present. Resolved via [`settings_loader::app_data_dir`]. When
//!    `read_claude` is on, the user-global `~/.claude/CLAUDE.md` is also loaded
//!    (Claude-Code compatibility, like opencode).
//! 2. **Ancestor files**: walk from filesystem root down to (and including)
//!    `cwd`. For each ancestor directory take the FIRST existing of `AGENTS.md`,
//!    `KIVIO.md`, and (only when `read_claude` is on) `CLAUDE.md` — one per
//!    directory. Root-first ordering means the file in `cwd` itself comes after
//!    its ancestors.
//! 3. **Project `.claude/CLAUDE.md`** (in `cwd` only, when `read_claude` is on):
//!    the project's Claude-Code context file.
//! 4. **Project `.kivio/`** (in `cwd` only): `.kivio/AGENTS.md` first, then the
//!    remaining top-level `.kivio/*.md` files sorted by name. NON-recursive — we
//!    do not descend into `.kivio/` subdirectories, and the `.kivio/agents/`
//!    subdir (sub-agent personas, not context) is explicitly skipped.
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
/// `CLAUDE.md` is only a candidate when `read_claude` is on (see
/// [`load_project_context`]).
const ANCESTOR_CANDIDATES_BASE: [&str; 2] = ["AGENTS.md", "KIVIO.md"];
/// `CLAUDE.md` is appended to the ancestor candidates only when `read_claude` is on.
const CLAUDE_CANDIDATE: &str = "CLAUDE.md";

/// Discover and render the project context for a run rooted at `cwd`.
///
/// `read_claude` toggles Claude-Code compatibility: when true, `CLAUDE.md` is a
/// per-directory ancestor candidate AND the user-global `~/.claude/CLAUDE.md`
/// plus the project `<cwd>/.claude/CLAUDE.md` are loaded; when false those are
/// dropped entirely (only `AGENTS.md`/`KIVIO.md` ancestors + the global
/// `<app_data>/agents/AGENTS.md` + the project `.kivio/` folder remain).
///
/// Returns an empty string when nothing relevant is found; otherwise a
/// `<project_context>` block ready to splice into the system prompt. See the
/// module docs for the discovery order and wrapping format.
pub fn load_project_context(cwd: &Path, read_claude: bool) -> String {
    let files = discover_context_files(cwd, read_claude);
    render_context_block(&files)
}

/// A discovered context file: its display path (what goes in the `path="…"`
/// attribute) and its contents.
struct ContextFile {
    display_path: String,
    content: String,
}

/// Collect the context files in discovery order, de-duplicated by canonical path.
fn discover_context_files(cwd: &Path, read_claude: bool) -> Vec<ContextFile> {
    let mut paths: Vec<PathBuf> = Vec::new();

    // 1. Global user instructions: <app_data>/agents/AGENTS.md, plus (when
    //    read_claude is on) the user-global ~/.claude/CLAUDE.md.
    if let Some(app_data) = settings_loader::app_data_dir() {
        let global = app_data.join("agents").join("AGENTS.md");
        if global.is_file() {
            paths.push(global);
        }
    }
    if read_claude {
        if let Some(home) = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf()) {
            let global_claude = home.join(".claude").join("CLAUDE.md");
            if global_claude.is_file() {
                paths.push(global_claude);
            }
        }
    }

    // 2. Ancestor files, root-first (closest dir = cwd comes last so it can
    //    override). Collect cwd→root, then reverse to root→cwd. `CLAUDE.md` is a
    //    per-directory candidate only when read_claude is on.
    let mut candidates: Vec<&str> = ANCESTOR_CANDIDATES_BASE.to_vec();
    if read_claude {
        candidates.push(CLAUDE_CANDIDATE);
    }
    let mut ancestor_dirs: Vec<&Path> = Vec::new();
    let mut dir: Option<&Path> = Some(cwd);
    while let Some(d) = dir {
        ancestor_dirs.push(d);
        dir = d.parent();
    }
    ancestor_dirs.reverse();
    for d in ancestor_dirs {
        for candidate in &candidates {
            let p = d.join(candidate);
            if p.is_file() {
                paths.push(p);
                break; // first existing candidate per directory
            }
        }
    }

    // 3. Project `.claude/CLAUDE.md` in cwd (Claude-Code compatibility), only
    //    when read_claude is on.
    if read_claude {
        let project_claude = cwd.join(".claude").join("CLAUDE.md");
        if project_claude.is_file() {
            paths.push(project_claude);
        }
    }

    // 4. Project `.kivio/` in cwd: AGENTS.md first, then other top-level *.md
    //    sorted by name. NON-recursive; the `.kivio/agents/` subdir (sub-agent
    //    personas, not context) is skipped along with every other subdir.
    let kivio_dir = cwd.join(".kivio");
    if kivio_dir.is_dir() {
        let agents_md = kivio_dir.join("AGENTS.md");
        if agents_md.is_file() {
            paths.push(agents_md.clone());
        }
        // BTreeMap keyed by filename → sorted by name.
        let mut others: BTreeMap<String, PathBuf> = BTreeMap::new();
        if let Ok(entries) = std::fs::read_dir(&kivio_dir) {
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
        // read_claude=false so the developer's real ~/.claude/CLAUDE.md (if any) is
        // not pulled in — this asserts that an empty project dir yields nothing.
        let out = load_project_context(&dir, false);
        assert_eq!(out, "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn kivio_dir_files_loaded_in_order() {
        let dir = temp_dir();
        write(&dir.join(".kivio").join("AGENTS.md"), "agents instructions");
        write(&dir.join(".kivio").join("zeta.md"), "zeta extra");
        write(&dir.join(".kivio").join("alpha.md"), "alpha extra");

        let out = load_project_context(&dir, true);
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
    fn kivio_dir_subdirs_not_recursed() {
        let dir = temp_dir();
        write(&dir.join(".kivio").join("top.md"), "top level");
        write(&dir.join(".kivio").join("nested").join("deep.md"), "deep nested");
        // The `.kivio/agents/` subdir is sub-agent personas, not context — skipped.
        write(&dir.join(".kivio").join("agents").join("persona.md"), "persona body");

        let out = load_project_context(&dir, true);
        assert!(out.contains("top level"));
        assert!(!out.contains("deep nested"), ".kivio subdirs must not be recursed");
        assert!(!out.contains("persona body"), ".kivio/agents/ must be skipped");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn root_agents_and_claude_picked_up() {
        // A project root with AGENTS.md, plus a nested cwd with CLAUDE.md.
        let root = temp_dir();
        write(&root.join("AGENTS.md"), "root agents");
        let cwd = root.join("sub");
        write(&cwd.join("CLAUDE.md"), "sub claude");

        let out = load_project_context(&cwd, true);
        assert!(out.contains("root agents"));
        assert!(out.contains("sub claude"));
        // Root-first: ancestor root file comes before the closer cwd file.
        let pos_root = out.find("root agents").unwrap();
        let pos_sub = out.find("sub claude").unwrap();
        assert!(pos_root < pos_sub, "root file must appear before the closer cwd file");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn read_claude_false_excludes_ancestor_claude() {
        // With read_claude OFF, a directory whose ONLY context file is CLAUDE.md
        // contributes nothing.
        let dir = temp_dir();
        write(&dir.join("CLAUDE.md"), "claude only content");

        let out = load_project_context(&dir, false);
        assert!(out.is_empty(), "CLAUDE.md must be dropped when read_claude=false");

        // With read_claude ON it is picked up.
        let out_on = load_project_context(&dir, true);
        assert!(out_on.contains("claude only content"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_claude_false_excludes_project_claude_dir() {
        // `.claude/CLAUDE.md` is loaded only when read_claude is on; AGENTS.md
        // remains regardless.
        let dir = temp_dir();
        write(&dir.join("AGENTS.md"), "agents stays");
        write(&dir.join(".claude").join("CLAUDE.md"), "dot claude content");

        let out_off = load_project_context(&dir, false);
        assert!(out_off.contains("agents stays"), "AGENTS.md is unconditional");
        assert!(
            !out_off.contains("dot claude content"),
            ".claude/CLAUDE.md must be excluded when read_claude=false"
        );

        let out_on = load_project_context(&dir, true);
        assert!(out_on.contains("agents stays"));
        assert!(out_on.contains("dot claude content"), ".claude/CLAUDE.md loaded when on");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn first_candidate_per_directory_wins() {
        let dir = temp_dir();
        // Both AGENTS.md and CLAUDE.md in the same dir → only AGENTS.md taken.
        write(&dir.join("AGENTS.md"), "preferred agents");
        write(&dir.join("CLAUDE.md"), "ignored claude");

        let out = load_project_context(&dir, true);
        assert!(out.contains("preferred agents"));
        assert!(!out.contains("ignored claude"), "second candidate in same dir must be skipped");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dedup_by_canonical_path() {
        // A file reachable both as an ancestor AGENTS.md and as `.kivio/AGENTS.md`
        // would be distinct paths, so instead test that the same file content
        // does not appear twice when a single AGENTS.md is the only source.
        let dir = temp_dir();
        write(&dir.join("AGENTS.md"), "unique-marker-xyz");

        let out = load_project_context(&dir, true);
        let count = out.matches("unique-marker-xyz").count();
        assert_eq!(count, 1, "content must appear exactly once");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn byte_cap_truncates() {
        let dir = temp_dir();
        // Two large files; the second should push past the cap.
        let big = "x".repeat(MAX_CONTEXT_BYTES);
        write(&dir.join(".kivio").join("AGENTS.md"), &big);
        write(&dir.join(".kivio").join("second.md"), &big);

        let out = load_project_context(&dir, true);
        assert!(out.contains("truncated"), "truncation marker expected");
        assert!(out.len() <= MAX_CONTEXT_BYTES + 256, "output stays near the cap");
        assert!(out.contains("</project_context>"), "block must still close");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wrapping_format_matches_pi() {
        let dir = temp_dir();
        write(&dir.join("AGENTS.md"), "hello");

        let out = load_project_context(&dir, true);
        assert!(out.starts_with("<project_context>"));
        assert!(out.contains("<project_instructions path=\""));
        assert!(out.contains("</project_instructions>"));
        assert!(out.trim_end().ends_with("</project_context>"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
