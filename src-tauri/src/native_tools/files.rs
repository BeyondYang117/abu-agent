use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Condvar, Mutex, OnceLock},
    time::UNIX_EPOCH,
};

use serde::Serialize;
use serde_json::{json, Value};

use super::{
    assert_writable_path, resolve_tool_read_path, resolve_tool_write_entry_path,
    resolve_tool_write_path, user_home_dir, workspace_display_path, NativeToolWorkspace,
    MAX_READ_FILE_BYTES,
};

const MAX_LIST_ENTRIES: usize = 500;
const MAX_GLOB_RESULTS: usize = 500;
const MAX_SEARCH_FILES: usize = 2_000;
const MAX_SEARCH_MATCHES: usize = 200;
const MAX_SEARCH_FILE_BYTES: u64 = 1024 * 1024;
const DEFAULT_IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".turbo",
    ".vite",
];

static FILE_MUTATION_LOCKS: OnceLock<FileMutationLocks> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
pub struct FileMutationResult {
    pub operation: String,
    pub resolved_path: Option<String>,
    pub files: Vec<FileMutationFile>,
    pub bytes_written: u64,
    pub additions: usize,
    pub removals: usize,
    pub diff: String,
    pub warnings: Vec<String>,
    pub diagnostics: Vec<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileMutationFile {
    pub path: String,
    pub operation: String,
    pub bytes_written: u64,
    pub additions: usize,
    pub removals: usize,
    pub diff: String,
}

impl FileMutationResult {
    pub fn summary(&self) -> String {
        let stats = format!("+{} -{}", self.additions, self.removals);
        if let Some(file) = self.files.first().filter(|_| self.files.len() == 1) {
            return format!(
                "{} {} ({stats})",
                mutation_operation_label(&file.operation),
                file.path
            );
        }
        format!(
            "{} {} file(s) ({stats})",
            mutation_operation_label(&self.operation),
            self.files.len()
        )
    }
}

pub fn read_file(workspace: &NativeToolWorkspace, arguments: &Value) -> Result<String, String> {
    let path = arguments
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "read_file requires path".to_string())?;
    let full = resolve_tool_read_path(workspace, path)?;
    if !full.is_file() {
        return Err(format!("不是可读取的文件: {path}"));
    }
    let metadata = fs::metadata(&full).map_err(|err| format!("Read metadata failed: {err}"))?;
    if metadata.len() > MAX_READ_FILE_BYTES {
        return Err(format!(
            "File too large (max {} bytes)",
            MAX_READ_FILE_BYTES
        ));
    }
    let content = fs::read_to_string(&full).map_err(|err| format!("Read file failed: {err}"))?;

    let offset = arguments
        .get("offset")
        .and_then(|v| v.as_u64())
        .unwrap_or(1)
        .max(1) as usize;
    let limit = arguments
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    if offset == 1 && limit.is_none() {
        return Ok(content);
    }

    let lines: Vec<&str> = content.lines().collect();
    let start = offset.saturating_sub(1).min(lines.len());
    let end = limit
        .map(|lim| (start + lim).min(lines.len()))
        .unwrap_or(lines.len());
    Ok(lines[start..end].join("\n"))
}

pub fn write_file(
    workspace: &NativeToolWorkspace,
    arguments: &Value,
) -> Result<FileMutationResult, String> {
    let path = arguments
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "write_file requires path".to_string())?;
    let content = arguments
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "write_file requires content".to_string())?;
    let full = resolve_tool_write_path(workspace, path)?;
    if !workspace.has_project() {
        assert_writable_path(&full)?;
    }
    let _guard = acquire_file_mutation_locks([full.clone()])?;
    let existed = full.is_file();
    // The existing content is only needed for the diff; degrade gracefully on non-UTF-8.
    let before = if existed {
        fs::read_to_string(&full).ok()
    } else {
        None
    };
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("Create parent dirs failed: {err}"))?;
    }
    fs::write(&full, content).map_err(|err| format!("Write file failed: {err}"))?;
    let operation = if existed { "overwrite" } else { "create" };
    let diff_omitted = existed && before.is_none();
    let file = if diff_omitted {
        FileMutationFile {
            path: workspace_display_path(workspace, &full),
            operation: operation.to_string(),
            bytes_written: content.len() as u64,
            additions: 0,
            removals: 0,
            diff: String::new(),
        }
    } else {
        planned_file_result(workspace, full, operation, before.as_deref(), Some(content))?
    };
    let mut result = file_mutation_result(operation, vec![file]);
    if diff_omitted {
        result
            .warnings
            .push("Existing file content is not valid UTF-8; diff omitted.".to_string());
    }
    Ok(result)
}

pub fn write_file_chunk(
    workspace: &NativeToolWorkspace,
    arguments: &Value,
) -> Result<FileMutationResult, String> {
    let path = required_string(arguments, "path")?;
    let mode = required_string(arguments, "mode")?;
    let full = resolve_tool_write_path(workspace, path)?;
    if !workspace.has_project() {
        assert_writable_path(&full)?;
    }
    let _guard = acquire_file_mutation_locks([full.clone()])?;

    match mode {
        "start" => {
            let content = required_raw_string(arguments, "content")?;
            let existed = full.is_file();
            // The existing content is only needed for the diff; degrade gracefully on non-UTF-8.
            let before = if existed {
                fs::read_to_string(&full).ok()
            } else {
                None
            };
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent)
                    .map_err(|err| format!("Create parent dirs failed: {err}"))?;
            }
            fs::write(&full, content).map_err(|err| format!("Write file chunk failed: {err}"))?;
            let file_operation = if existed { "overwrite" } else { "create" };
            let diff_omitted = existed && before.is_none();
            let file = if diff_omitted {
                FileMutationFile {
                    path: workspace_display_path(workspace, &full),
                    operation: file_operation.to_string(),
                    bytes_written: content.len() as u64,
                    additions: 0,
                    removals: 0,
                    diff: String::new(),
                }
            } else {
                planned_file_result(
                    workspace,
                    full,
                    file_operation,
                    before.as_deref(),
                    Some(content),
                )?
            };
            let mut result = file_mutation_result("write_chunk_start", vec![file]);
            if diff_omitted {
                result
                    .warnings
                    .push("Existing file content is not valid UTF-8; diff omitted.".to_string());
            }
            Ok(result)
        }
        "append" => {
            let content = required_raw_string(arguments, "content")?;
            if !full.is_file() {
                return Err("write_file_chunk append requires an existing file; call mode=start first".to_string());
            }
            let before =
                fs::read_to_string(&full).map_err(|err| format!("Read file failed: {err}"))?;
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(&full)
                .map_err(|err| format!("Open file for append failed: {err}"))?;
            file.write_all(content.as_bytes())
                .map_err(|err| format!("Append file chunk failed: {err}"))?;
            let mut after = before.clone();
            after.push_str(content);
            Ok(file_mutation_result(
                "write_chunk_append",
                vec![planned_file_result(
                    workspace,
                    full,
                    "append",
                    Some(&before),
                    Some(&after),
                )?],
            ))
        }
        "finish" => {
            if !full.is_file() {
                return Err("write_file_chunk finish requires an existing file".to_string());
            }
            let content =
                fs::read_to_string(&full).map_err(|err| format!("Read file failed: {err}"))?;
            let display_path = workspace_display_path(workspace, &full);
            Ok(FileMutationResult {
                operation: "write_chunk_finish".to_string(),
                resolved_path: Some(display_path.clone()),
                files: vec![FileMutationFile {
                    path: display_path,
                    operation: "finish".to_string(),
                    bytes_written: content.len() as u64,
                    additions: 0,
                    removals: 0,
                    diff: String::new(),
                }],
                bytes_written: content.len() as u64,
                additions: 0,
                removals: 0,
                diff: String::new(),
                warnings: Vec::new(),
                diagnostics: Vec::new(),
            })
        }
        other => Err(format!(
            "write_file_chunk mode must be start, append, or finish; got {other}"
        )),
    }
}

pub fn edit_file(
    workspace: &NativeToolWorkspace,
    arguments: &Value,
) -> Result<FileMutationResult, String> {
    let path = arguments
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "edit_file requires path".to_string())?;
    let old_string = arguments
        .get("old_string")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "edit_file requires old_string".to_string())?;
    let new_string = arguments
        .get("new_string")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "edit_file requires new_string".to_string())?;
    let replace_all = arguments
        .get("replace_all")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let full = resolve_tool_write_path(workspace, path)?;
    if !workspace.has_project() {
        assert_writable_path(&full)?;
    }
    let _guard = acquire_file_mutation_locks([full.clone()])?;
    if !full.is_file() {
        return Err(format!("不是可编辑的文件: {path}"));
    }

    let content = fs::read_to_string(&full).map_err(|err| format!("Read file failed: {err}"))?;
    if old_string == new_string {
        let display_path = workspace_display_path(workspace, &full);
        return Ok(FileMutationResult {
            operation: "edit".to_string(),
            resolved_path: Some(display_path.clone()),
            files: vec![FileMutationFile {
                path: display_path,
                operation: "noop".to_string(),
                bytes_written: content.len() as u64,
                additions: 0,
                removals: 0,
                diff: String::new(),
            }],
            bytes_written: content.len() as u64,
            additions: 0,
            removals: 0,
            diff: String::new(),
            warnings: vec!["old_string and new_string are identical; no changes made.".to_string()],
            diagnostics: Vec::new(),
        });
    }
    if !content.contains(old_string) {
        return Err("old_string not found in file".to_string());
    }
    let count = content.matches(old_string).count();
    if !replace_all && count > 1 {
        return Err(format!(
            "old_string appears {count} times; set replace_all=true or use a unique old_string"
        ));
    }

    let updated = if replace_all {
        content.replace(old_string, new_string)
    } else {
        content.replacen(old_string, new_string, 1)
    };
    fs::write(&full, &updated).map_err(|err| format!("Write file failed: {err}"))?;
    Ok(file_mutation_result(
        "edit",
        vec![planned_file_result(
            workspace,
            full,
            "edit",
            Some(&content),
            Some(&updated),
        )?],
    ))
}

pub fn patch(
    workspace: &NativeToolWorkspace,
    arguments: &Value,
) -> Result<FileMutationResult, String> {
    let patch = arguments
        .get("patch")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "patch requires patch".to_string())?;
    let ops = parse_patch(patch)?;
    if ops.is_empty() {
        return Err("patch contains no file operations".to_string());
    }

    let mut seen = HashSet::new();
    let mut resolved_ops = Vec::new();
    let mut seen_resolved = HashSet::new();
    for op in ops {
        validate_patch_path(&op.path)?;
        if !seen.insert(op.path.clone()) {
            return Err(format!(
                "patch modifies the same file more than once: {}",
                op.path
            ));
        }
        let full = match &op.kind {
            PatchOpKind::Add { .. } | PatchOpKind::Update { .. } => {
                resolve_tool_write_path(workspace, &op.path)?
            }
            PatchOpKind::Delete => resolve_tool_write_entry_path(workspace, &op.path)?,
        };
        if !workspace.has_project() {
            assert_writable_path(&full)?;
        }
        if !seen_resolved.insert(full.clone()) {
            return Err(format!(
                "patch modifies the same resolved file more than once: {}",
                workspace_display_path(workspace, &full)
            ));
        }
        resolved_ops.push(ResolvedPatchOperation {
            path: op.path,
            full,
            kind: op.kind,
        });
    }

    let _guard = acquire_file_mutation_locks(resolved_ops.iter().map(|op| op.full.clone()))?;
    let mut plans = Vec::new();
    for op in resolved_ops {
        match op.kind {
            PatchOpKind::Add { lines } => {
                if op.full.exists() {
                    return Err(format!("Add File target already exists: {}", op.path));
                }
                let content = patch_added_content(&lines);
                plans.push(PlannedMutation {
                    path: op.full,
                    operation: "create".to_string(),
                    before: None,
                    after: Some(content),
                });
            }
            PatchOpKind::Update { hunks } => {
                if !op.full.is_file() {
                    return Err(format!("Update File target is not a file: {}", op.path));
                }
                let before = fs::read_to_string(&op.full)
                    .map_err(|err| format!("Read file failed: {err}"))?;
                let after = apply_patch_hunks(&before, &hunks, &op.path)?;
                plans.push(PlannedMutation {
                    path: op.full,
                    operation: "edit".to_string(),
                    before: Some(before),
                    after: Some(after),
                });
            }
            PatchOpKind::Delete => {
                if !op.full.is_file() {
                    return Err(format!("Delete File target is not a file: {}", op.path));
                }
                let before = fs::read_to_string(&op.full)
                    .map_err(|err| format!("Read file failed: {err}"))?;
                plans.push(PlannedMutation {
                    path: op.full,
                    operation: "delete".to_string(),
                    before: Some(before),
                    after: None,
                });
            }
        }
    }

    let mut file_results = Vec::new();
    for plan in &plans {
        file_results.push(planned_file_result(
            workspace,
            plan.path.clone(),
            &plan.operation,
            plan.before.as_deref(),
            plan.after.as_deref(),
        )?);
    }

    let display_paths: Vec<String> = plans
        .iter()
        .map(|plan| workspace_display_path(workspace, &plan.path))
        .collect();
    for (idx, plan) in plans.iter().enumerate() {
        let outcome: Result<(), String> = match &plan.after {
            Some(content) => {
                let mut step = Ok(());
                if let Some(parent) = plan.path.parent() {
                    step = fs::create_dir_all(parent)
                        .map_err(|err| format!("Create parent dirs failed: {err}"));
                }
                step.and_then(|_| {
                    fs::write(&plan.path, content)
                        .map_err(|err| format!("Write file failed: {err}"))
                })
            }
            None => fs::remove_file(&plan.path)
                .map_err(|err| format!("Delete file failed: {err}")),
        };
        if let Err(err) = outcome {
            let applied = display_paths[..idx].join(", ");
            let not_applied = display_paths[idx..].join(", ");
            return Err(format!(
                "{err}. Already applied: [{applied}]. Not applied: [{not_applied}]. The project is partially patched."
            ));
        }
    }

    let mut result = file_mutation_result("patch", file_results);
    if !workspace.has_project() {
        // Relative paths in global mode always resolve under the user's home
        // directory (`candidate_path` joins home); workspace roots only filter
        // which resolved paths are allowed, they are never the join base.
        let base = user_home_dir()
            .map(|home| home.display().to_string())
            .unwrap_or_else(|_| "~".to_string());
        result.warnings.push(format!(
            "No project folder is bound to this conversation; patch paths were resolved under the global write base: {base}"
        ));
    }
    Ok(result)
}

#[derive(Debug)]
struct PlannedMutation {
    path: PathBuf,
    operation: String,
    before: Option<String>,
    after: Option<String>,
}

#[derive(Debug)]
struct ResolvedPatchOperation {
    path: String,
    full: PathBuf,
    kind: PatchOpKind,
}

#[derive(Debug)]
struct PatchOperation {
    path: String,
    kind: PatchOpKind,
}

#[derive(Debug)]
enum PatchOpKind {
    Add { lines: Vec<String> },
    Update { hunks: Vec<PatchHunk> },
    Delete,
}

#[derive(Debug)]
struct PatchHunk {
    lines: Vec<PatchLine>,
}

#[derive(Debug)]
enum PatchLine {
    Context(String),
    Remove(String),
    Add(String),
}

struct FileMutationLocks {
    active: Mutex<HashSet<PathBuf>>,
    ready: Condvar,
}

struct FileMutationLockGuard {
    paths: Vec<PathBuf>,
}

impl Drop for FileMutationLockGuard {
    fn drop(&mut self) {
        let Some(locks) = FILE_MUTATION_LOCKS.get() else {
            return;
        };
        let Ok(mut active) = locks.active.lock() else {
            return;
        };
        for path in &self.paths {
            active.remove(path);
        }
        locks.ready.notify_all();
    }
}

fn acquire_file_mutation_locks<I>(paths: I) -> Result<FileMutationLockGuard, String>
where
    I: IntoIterator<Item = PathBuf>,
{
    let locks = FILE_MUTATION_LOCKS.get_or_init(|| FileMutationLocks {
        active: Mutex::new(HashSet::new()),
        ready: Condvar::new(),
    });
    let mut paths = paths.into_iter().collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    let mut active = locks
        .active
        .lock()
        .map_err(|_| "File mutation lock is unavailable".to_string())?;
    while paths.iter().any(|path| active.contains(path)) {
        active = locks
            .ready
            .wait(active)
            .map_err(|_| "File mutation lock is unavailable".to_string())?;
    }
    for path in &paths {
        active.insert(path.clone());
    }
    Ok(FileMutationLockGuard { paths })
}

fn parse_patch(patch: &str) -> Result<Vec<PatchOperation>, String> {
    let lines = patch
        .lines()
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect::<Vec<_>>();
    let first = lines
        .first()
        .map(|line| line.trim())
        .ok_or_else(|| "patch is empty".to_string())?;
    if first != "*** Begin Patch" {
        return Err("patch must start with *** Begin Patch".to_string());
    }

    let mut idx = 1usize;
    let mut ops = Vec::new();
    while idx < lines.len() {
        let line = lines[idx].trim();
        if line == "*** End Patch" {
            return Ok(ops);
        }
        if let Some(path) = line.strip_prefix("*** Add File: ") {
            idx += 1;
            let mut added = Vec::new();
            while idx < lines.len() && !lines[idx].starts_with("*** ") {
                let raw = &lines[idx];
                let Some(content) = raw.strip_prefix('+') else {
                    return Err(format!("Add File lines must start with '+': {path}"));
                };
                added.push(content.to_string());
                idx += 1;
            }
            ops.push(PatchOperation {
                path: path.trim().to_string(),
                kind: PatchOpKind::Add { lines: added },
            });
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Update File: ") {
            idx += 1;
            let mut hunks = Vec::new();
            let mut current = Vec::new();
            while idx < lines.len() && !lines[idx].starts_with("*** ") {
                let raw = &lines[idx];
                if raw.starts_with("@@") {
                    if !current.is_empty() {
                        hunks.push(PatchHunk { lines: current });
                        current = Vec::new();
                    }
                    idx += 1;
                    continue;
                }
                if raw == r"\ No newline at end of file" {
                    idx += 1;
                    continue;
                }
                let Some(marker) = raw.chars().next() else {
                    // Models often trim the leading space from empty context lines;
                    // git apply tolerates this, so treat it as an empty context line.
                    current.push(PatchLine::Context(String::new()));
                    idx += 1;
                    continue;
                };
                let content = raw.get(marker.len_utf8()..).unwrap_or("").to_string();
                match marker {
                    ' ' => current.push(PatchLine::Context(content)),
                    '-' => current.push(PatchLine::Remove(content)),
                    '+' => current.push(PatchLine::Add(content)),
                    _ => {
                        return Err(format!(
                            "Update File hunk lines must start with ' ', '-' or '+': {path}"
                        ))
                    }
                }
                idx += 1;
            }
            if !current.is_empty() {
                hunks.push(PatchHunk { lines: current });
            }
            if hunks.is_empty() {
                return Err(format!("Update File has no hunks: {path}"));
            }
            ops.push(PatchOperation {
                path: path.trim().to_string(),
                kind: PatchOpKind::Update { hunks },
            });
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Delete File: ") {
            idx += 1;
            while idx < lines.len() && !lines[idx].starts_with("*** ") {
                if !lines[idx].trim().is_empty() {
                    return Err(format!("Delete File cannot include hunk content: {path}"));
                }
                idx += 1;
            }
            ops.push(PatchOperation {
                path: path.trim().to_string(),
                kind: PatchOpKind::Delete,
            });
            continue;
        }
        return Err(format!("Unsupported patch line: {}", lines[idx]));
    }

    Err("patch must end with *** End Patch".to_string())
}

fn validate_patch_path(path: &str) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("patch file path is empty".to_string());
    }
    if trimmed.starts_with('~') {
        return Err("patch file paths must be project-relative; '~' is not allowed".to_string());
    }
    if trimmed.contains('\\') {
        return Err("patch file paths must use forward slashes".to_string());
    }
    let parsed = Path::new(trimmed);
    if parsed.is_absolute() {
        return Err("patch file paths must be relative, not absolute".to_string());
    }
    if parsed.components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return Err("patch file paths cannot contain '..' or roots".to_string());
    }
    Ok(())
}

fn patch_added_content(lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let mut content = lines.join("\n");
    content.push('\n');
    content
}

fn apply_patch_hunks(before: &str, hunks: &[PatchHunk], path: &str) -> Result<String, String> {
    let line_ending = if before.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut lines = split_logical_lines(before);
    for hunk in hunks {
        let (old_lines, new_lines) = hunk_old_new_lines(hunk);
        if old_lines.is_empty() {
            return Err(format!("Patch hunk for {path} has no context or removals"));
        }
        let start = find_unique_subsequence(&lines, &old_lines)
            .ok_or_else(|| format!("Patch hunk did not match file content exactly: {path}"))?;
        lines.splice(start..start + old_lines.len(), new_lines);
    }
    Ok(lines.join(line_ending))
}

fn hunk_old_new_lines(hunk: &PatchHunk) -> (Vec<String>, Vec<String>) {
    let mut old_lines = Vec::new();
    let mut new_lines = Vec::new();
    for line in &hunk.lines {
        match line {
            PatchLine::Context(content) => {
                old_lines.push(content.clone());
                new_lines.push(content.clone());
            }
            PatchLine::Remove(content) => old_lines.push(content.clone()),
            PatchLine::Add(content) => new_lines.push(content.clone()),
        }
    }
    (old_lines, new_lines)
}

fn find_unique_subsequence(haystack: &[String], needle: &[String]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    let mut found = None;
    for idx in 0..=haystack.len() - needle.len() {
        if haystack[idx..idx + needle.len()] == *needle {
            if found.is_some() {
                return None;
            }
            found = Some(idx);
        }
    }
    found
}

fn split_logical_lines(content: &str) -> Vec<String> {
    if content.is_empty() {
        return Vec::new();
    }
    content
        .replace("\r\n", "\n")
        .split('\n')
        .map(str::to_string)
        .collect()
}

fn file_mutation_result(operation: &str, files: Vec<FileMutationFile>) -> FileMutationResult {
    let resolved_path = files
        .first()
        .filter(|_| files.len() == 1)
        .map(|file| file.path.clone());
    let bytes_written = files.iter().map(|file| file.bytes_written).sum();
    let additions = files.iter().map(|file| file.additions).sum();
    let removals = files.iter().map(|file| file.removals).sum();
    let diff = files
        .iter()
        .map(|file| file.diff.as_str())
        .filter(|diff| !diff.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    FileMutationResult {
        operation: operation.to_string(),
        resolved_path,
        files,
        bytes_written,
        additions,
        removals,
        diff,
        warnings: Vec::new(),
        diagnostics: Vec::new(),
    }
}

fn planned_file_result(
    workspace: &NativeToolWorkspace,
    path: PathBuf,
    operation: &str,
    before: Option<&str>,
    after: Option<&str>,
) -> Result<FileMutationFile, String> {
    let display_path = workspace_display_path(workspace, &path);
    let (diff, additions, removals) = unified_diff(&display_path, before, after);
    Ok(FileMutationFile {
        path: display_path,
        operation: operation.to_string(),
        bytes_written: after.map(|content| content.len() as u64).unwrap_or(0),
        additions,
        removals,
        diff,
    })
}

/// LCS guard: above this many DP cells for the changed middle region, fall back
/// to a coarse single-hunk diff (whole middle as remove+add).
const DIFF_LCS_MAX_CELLS: usize = 250_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffOpKind {
    Equal,
    Remove,
    Add,
}

#[derive(Debug)]
struct DiffOp<'a> {
    kind: DiffOpKind,
    text: &'a str,
}

fn unified_diff(path: &str, before: Option<&str>, after: Option<&str>) -> (String, usize, usize) {
    let old_lines = diff_lines(before.unwrap_or(""));
    let new_lines = diff_lines(after.unwrap_or(""));
    if old_lines == new_lines {
        return (String::new(), 0, 0);
    }

    let mut prefix = 0usize;
    while prefix < old_lines.len()
        && prefix < new_lines.len()
        && old_lines[prefix] == new_lines[prefix]
    {
        prefix += 1;
    }

    let mut old_end = old_lines.len();
    let mut new_end = new_lines.len();
    while old_end > prefix && new_end > prefix && old_lines[old_end - 1] == new_lines[new_end - 1] {
        old_end -= 1;
        new_end -= 1;
    }

    let ops = build_diff_ops(&old_lines, &new_lines, prefix, old_end, new_end);

    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, op)| op.kind != DiffOpKind::Equal)
        .map(|(idx, _)| idx)
        .collect();
    if changed.is_empty() {
        return (String::new(), 0, 0);
    }

    // Prefix sums of old/new lines consumed before each op index.
    let mut old_consumed = vec![0usize; ops.len() + 1];
    let mut new_consumed = vec![0usize; ops.len() + 1];
    for (idx, op) in ops.iter().enumerate() {
        old_consumed[idx + 1] = old_consumed[idx]
            + usize::from(matches!(op.kind, DiffOpKind::Equal | DiffOpKind::Remove));
        new_consumed[idx + 1] =
            new_consumed[idx] + usize::from(matches!(op.kind, DiffOpKind::Equal | DiffOpKind::Add));
    }

    // Group changed ops into hunks: runs separated by 7+ unchanged lines split.
    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut group_start = changed[0];
    let mut group_last = changed[0];
    for &idx in &changed[1..] {
        if idx - group_last - 1 >= 7 {
            groups.push((group_start, group_last));
            group_start = idx;
        }
        group_last = idx;
    }
    groups.push((group_start, group_last));

    let old_header = if before.is_none() {
        "/dev/null".to_string()
    } else {
        format!("a/{path}")
    };
    let new_header = if after.is_none() {
        "/dev/null".to_string()
    } else {
        format!("b/{path}")
    };
    let mut out = String::new();
    out.push_str(&format!("--- {old_header}\n+++ {new_header}\n"));
    let mut additions = 0usize;
    let mut removals = 0usize;
    for (first, last) in groups {
        let hunk_start = first.saturating_sub(3);
        let hunk_end = (last + 4).min(ops.len());
        let old_count = old_consumed[hunk_end] - old_consumed[hunk_start];
        let new_count = new_consumed[hunk_end] - new_consumed[hunk_start];
        let old_start = if old_count == 0 {
            old_consumed[hunk_start]
        } else {
            old_consumed[hunk_start] + 1
        };
        let new_start = if new_count == 0 {
            new_consumed[hunk_start]
        } else {
            new_consumed[hunk_start] + 1
        };
        out.push_str(&format!(
            "@@ -{old_start},{old_count} +{new_start},{new_count} @@\n"
        ));
        for op in &ops[hunk_start..hunk_end] {
            match op.kind {
                DiffOpKind::Equal => out.push_str(&format!(" {}\n", op.text)),
                DiffOpKind::Remove => {
                    removals += 1;
                    out.push_str(&format!("-{}\n", op.text));
                }
                DiffOpKind::Add => {
                    additions += 1;
                    out.push_str(&format!("+{}\n", op.text));
                }
            }
        }
    }
    (out, additions, removals)
}

fn build_diff_ops<'a>(
    old_lines: &'a [String],
    new_lines: &'a [String],
    prefix: usize,
    old_end: usize,
    new_end: usize,
) -> Vec<DiffOp<'a>> {
    let mut ops = Vec::with_capacity(old_lines.len() + new_lines.len());
    for line in &old_lines[..prefix] {
        ops.push(DiffOp {
            kind: DiffOpKind::Equal,
            text: line,
        });
    }
    let middle_old = &old_lines[prefix..old_end];
    let middle_new = &new_lines[prefix..new_end];
    if middle_old.len().saturating_mul(middle_new.len()) > DIFF_LCS_MAX_CELLS {
        // Coarse fallback: whole middle as remove+add in a single block.
        for line in middle_old {
            ops.push(DiffOp {
                kind: DiffOpKind::Remove,
                text: line,
            });
        }
        for line in middle_new {
            ops.push(DiffOp {
                kind: DiffOpKind::Add,
                text: line,
            });
        }
    } else {
        let m = middle_old.len();
        let n = middle_new.len();
        let width = n + 1;
        let mut dp = vec![0u32; (m + 1) * width];
        for i in (0..m).rev() {
            for j in (0..n).rev() {
                dp[i * width + j] = if middle_old[i] == middle_new[j] {
                    dp[(i + 1) * width + j + 1] + 1
                } else {
                    dp[(i + 1) * width + j].max(dp[i * width + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0usize, 0usize);
        while i < m && j < n {
            if middle_old[i] == middle_new[j] {
                ops.push(DiffOp {
                    kind: DiffOpKind::Equal,
                    text: &middle_old[i],
                });
                i += 1;
                j += 1;
            } else if dp[(i + 1) * width + j] >= dp[i * width + j + 1] {
                ops.push(DiffOp {
                    kind: DiffOpKind::Remove,
                    text: &middle_old[i],
                });
                i += 1;
            } else {
                ops.push(DiffOp {
                    kind: DiffOpKind::Add,
                    text: &middle_new[j],
                });
                j += 1;
            }
        }
        while i < m {
            ops.push(DiffOp {
                kind: DiffOpKind::Remove,
                text: &middle_old[i],
            });
            i += 1;
        }
        while j < n {
            ops.push(DiffOp {
                kind: DiffOpKind::Add,
                text: &middle_new[j],
            });
            j += 1;
        }
    }
    for line in &old_lines[old_end..] {
        ops.push(DiffOp {
            kind: DiffOpKind::Equal,
            text: line,
        });
    }
    ops
}

fn diff_lines(content: &str) -> Vec<String> {
    if content.is_empty() {
        return Vec::new();
    }
    let normalized = content.replace("\r\n", "\n");
    let mut lines = normalized
        .split('\n')
        .map(str::to_string)
        .collect::<Vec<_>>();
    if normalized.ends_with('\n') {
        lines.pop();
    }
    lines
}

fn mutation_operation_label(operation: &str) -> &'static str {
    match operation {
        "create" => "Created",
        "overwrite" => "Overwrote",
        "edit" => "Updated",
        "delete" => "Deleted",
        "noop" => "No changes",
        "patch" => "Patched",
        "append" => "Appended",
        "finish" => "Finished",
        _ => "Changed",
    }
}

pub fn list_dir(workspace: &NativeToolWorkspace, arguments: &Value) -> Result<String, String> {
    let path = arguments
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(".");
    let include_hidden = arguments
        .get("include_hidden")
        .or_else(|| arguments.get("includeHidden"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let max_entries = arguments
        .get("max_entries")
        .or_else(|| arguments.get("maxEntries"))
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(200)
        .clamp(1, MAX_LIST_ENTRIES);

    let dir = resolve_tool_read_path(workspace, path)?;
    if !dir.is_dir() {
        return Err(format!("不是可列出的文件夹: {path}"));
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|err| format!("Read directory failed: {err}"))? {
        let entry = entry.map_err(|err| format!("Read directory entry failed: {err}"))?;
        let path = entry.path();
        if !include_hidden && is_hidden_path(&path) {
            continue;
        }
        let metadata = entry
            .metadata()
            .map_err(|err| format!("Read entry metadata failed: {err}"))?;
        entries.push(path_info(workspace, &path, &metadata)?);
    }

    entries.sort_by(|a, b| {
        a.get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .cmp(b.get("type").and_then(|v| v.as_str()).unwrap_or(""))
            .then_with(|| {
                a.get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .cmp(b.get("path").and_then(|v| v.as_str()).unwrap_or(""))
            })
    });
    let truncated = entries.len() > max_entries;
    entries.truncate(max_entries);

    format_json(json!({
        "path": workspace_display_path(workspace, &dir),
        "entries": entries,
        "truncated": truncated
    }))
}

pub fn stat_path(workspace: &NativeToolWorkspace, arguments: &Value) -> Result<String, String> {
    let path = required_string(arguments, "path")?;
    let full = resolve_tool_read_path(workspace, path)?;
    let metadata = fs::metadata(&full).map_err(|err| format!("Read metadata failed: {err}"))?;
    format_json(path_info(workspace, &full, &metadata)?)
}

pub fn create_dir(workspace: &NativeToolWorkspace, arguments: &Value) -> Result<String, String> {
    let path = required_string(arguments, "path")?;
    let full = resolve_tool_write_path(workspace, path)?;
    if !workspace.has_project() {
        assert_writable_path(&full)?;
    }
    fs::create_dir_all(&full).map_err(|err| format!("Create directory failed: {err}"))?;
    Ok(format!(
        "Created directory {}",
        workspace_display_path(workspace, &full)
    ))
}

pub fn delete_path(workspace: &NativeToolWorkspace, arguments: &Value) -> Result<String, String> {
    let path = required_string(arguments, "path")?;
    let recursive = arguments
        .get("recursive")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let full = resolve_tool_write_entry_path(workspace, path)?;
    if !workspace.has_project() {
        assert_writable_path(&full)?;
    }
    let _guard = acquire_file_mutation_locks([full.clone()])?;
    reject_workspace_root_delete(workspace, &full)?;

    let metadata = fs::symlink_metadata(&full).map_err(|_| format!("路径不存在: {path}"))?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() || metadata.is_file() {
        fs::remove_file(&full).map_err(|err| format!("Delete file failed: {err}"))?;
    } else if metadata.is_dir() {
        if recursive {
            fs::remove_dir_all(&full).map_err(|err| format!("Delete directory failed: {err}"))?;
        } else {
            fs::remove_dir(&full).map_err(|err| format!("Delete directory failed: {err}"))?;
        }
    } else {
        return Err(format!("不是可删除的文件或文件夹: {path}"));
    }

    Ok(format!(
        "Deleted {}",
        workspace_display_path(workspace, &full)
    ))
}

pub fn move_path(workspace: &NativeToolWorkspace, arguments: &Value) -> Result<String, String> {
    let from = required_string(arguments, "from")?;
    let to = required_string(arguments, "to")?;
    let source = resolve_tool_write_entry_path(workspace, from)?;
    let destination = resolve_tool_write_path(workspace, to)?;
    if !workspace.has_project() {
        assert_writable_path(&source)?;
        assert_writable_path(&destination)?;
    }
    let _guard = acquire_file_mutation_locks([source.clone(), destination.clone()])?;
    reject_workspace_root_delete(workspace, &source)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("Create parent dirs failed: {err}"))?;
    }
    fs::rename(&source, &destination).map_err(|err| format!("Move path failed: {err}"))?;
    Ok(format!(
        "Moved {} to {}",
        workspace_display_path(workspace, &source),
        workspace_display_path(workspace, &destination)
    ))
}

pub fn copy_path(workspace: &NativeToolWorkspace, arguments: &Value) -> Result<String, String> {
    let from = required_string(arguments, "from")?;
    let to = required_string(arguments, "to")?;
    let source = resolve_tool_read_path(workspace, from)?;
    let destination = resolve_tool_write_path(workspace, to)?;
    if !workspace.has_project() {
        assert_writable_path(&destination)?;
    }
    let _guard = acquire_file_mutation_locks([destination.clone()])?;
    if source.is_dir() {
        reject_recursive_directory_copy(&source, &destination)?;
        copy_dir_recursive(&source, &destination)?;
    } else if source.is_file() {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("Create parent dirs failed: {err}"))?;
        }
        fs::copy(&source, &destination).map_err(|err| format!("Copy file failed: {err}"))?;
    } else {
        return Err(format!("不是可复制的文件或文件夹: {from}"));
    }
    Ok(format!(
        "Copied {} to {}",
        workspace_display_path(workspace, &source),
        workspace_display_path(workspace, &destination)
    ))
}

pub fn glob_files(workspace: &NativeToolWorkspace, arguments: &Value) -> Result<String, String> {
    let pattern = required_string(arguments, "pattern")?;
    validate_glob_pattern(pattern)?;
    let include_hidden = arguments
        .get("include_hidden")
        .or_else(|| arguments.get("includeHidden"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let max_results = arguments
        .get("max_results")
        .or_else(|| arguments.get("maxResults"))
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(200)
        .clamp(1, MAX_GLOB_RESULTS);
    let root = resolve_tool_read_path(
        workspace,
        arguments
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("."),
    )?;
    if !root.is_dir() {
        return Err("glob_files path must be a directory".to_string());
    }

    let mut matches = Vec::new();
    for path in walk_paths(&root, true, include_hidden, MAX_SEARCH_FILES)? {
        let rel = relative_slash_path(&root, &path);
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if glob_match(pattern, &rel) || (!pattern.contains('/') && glob_match(pattern, file_name)) {
            let metadata =
                fs::metadata(&path).map_err(|err| format!("Read metadata failed: {err}"))?;
            matches.push(path_info(workspace, &path, &metadata)?);
            if matches.len() >= max_results {
                break;
            }
        }
    }

    format_json(json!({
        "pattern": pattern,
        "matches": matches,
        "truncated": matches.len() >= max_results
    }))
}

pub fn search_files(workspace: &NativeToolWorkspace, arguments: &Value) -> Result<String, String> {
    let query = required_string(arguments, "query")?;
    let root = resolve_tool_read_path(
        workspace,
        arguments
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("."),
    )?;
    if !root.is_dir() {
        return Err("search_files path must be a directory".to_string());
    }
    let case_sensitive = arguments
        .get("case_sensitive")
        .or_else(|| arguments.get("caseSensitive"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let include_hidden = arguments
        .get("include_hidden")
        .or_else(|| arguments.get("includeHidden"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let max_results = arguments
        .get("max_results")
        .or_else(|| arguments.get("maxResults"))
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(100)
        .clamp(1, MAX_SEARCH_MATCHES);
    let needle = if case_sensitive {
        query.to_string()
    } else {
        query.to_lowercase()
    };

    let mut matches = Vec::new();
    for path in walk_paths(&root, true, include_hidden, MAX_SEARCH_FILES)? {
        if matches.len() >= max_results {
            break;
        }
        if !path.is_file() {
            continue;
        }
        let metadata = fs::metadata(&path).map_err(|err| format!("Read metadata failed: {err}"))?;
        if metadata.len() > MAX_SEARCH_FILE_BYTES {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        for (idx, line) in content.lines().enumerate() {
            let haystack = if case_sensitive {
                line.to_string()
            } else {
                line.to_lowercase()
            };
            if haystack.contains(&needle) {
                matches.push(json!({
                    "path": workspace_display_path(workspace, &path),
                    "line": idx + 1,
                    "text": line
                }));
                if matches.len() >= max_results {
                    break;
                }
            }
        }
    }

    format_json(json!({
        "query": query,
        "matches": matches,
        "truncated": matches.len() >= max_results
    }))
}

fn required_string<'a>(arguments: &'a Value, key: &str) -> Result<&'a str, String> {
    arguments
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{key} is required"))
}

/// Like `required_string`, but preserves the value verbatim (no trim). Chunked
/// writes depend on exact leading/trailing whitespace — trimming would merge
/// lines across chunk boundaries.
fn required_raw_string<'a>(arguments: &'a Value, key: &str) -> Result<&'a str, String> {
    arguments
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("{key} is required"))
}

fn format_json(value: Value) -> Result<String, String> {
    serde_json::to_string_pretty(&value)
        .map_err(|err| format!("Serialize tool result failed: {err}"))
}

fn validate_glob_pattern(pattern: &str) -> Result<(), String> {
    let pattern_path = Path::new(pattern);
    if pattern_path.is_absolute() {
        return Err(
            "glob_files pattern must be relative to the search path; put the directory in path instead."
                .to_string(),
        );
    }
    if pattern_path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("glob_files pattern cannot contain '..'.".to_string());
    }
    Ok(())
}

fn path_info(
    workspace: &NativeToolWorkspace,
    path: &Path,
    metadata: &fs::Metadata,
) -> Result<Value, String> {
    let kind = if metadata.is_dir() {
        "directory"
    } else if metadata.is_file() {
        "file"
    } else {
        "other"
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());
    Ok(json!({
        "path": workspace_display_path(workspace, path),
        "type": kind,
        "sizeBytes": metadata.len(),
        "modifiedAt": modified
    }))
}

fn is_hidden_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
}

fn walk_paths(
    root: &Path,
    recursive: bool,
    include_hidden: bool,
    max_paths: usize,
) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).map_err(|err| format!("Read directory failed: {err}"))? {
            let entry = entry.map_err(|err| format!("Read directory entry failed: {err}"))?;
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if !include_hidden && name.starts_with('.') {
                continue;
            }
            let metadata = entry
                .metadata()
                .map_err(|err| format!("Read entry metadata failed: {err}"))?;
            if metadata.is_dir() {
                if recursive && !DEFAULT_IGNORED_DIRS.contains(&name) {
                    stack.push(path.clone());
                }
            }
            out.push(path);
            if out.len() >= max_paths {
                return Ok(out);
            }
        }
    }
    Ok(out)
}

fn relative_slash_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn glob_match(pattern: &str, value: &str) -> bool {
    let pattern_parts: Vec<&str> = pattern.split('/').filter(|part| !part.is_empty()).collect();
    let value_parts: Vec<&str> = value.split('/').filter(|part| !part.is_empty()).collect();
    glob_match_parts(&pattern_parts, &value_parts)
}

fn glob_match_parts(pattern: &[&str], value: &[&str]) -> bool {
    if pattern.is_empty() {
        return value.is_empty();
    }
    if pattern[0] == "**" {
        return glob_match_parts(&pattern[1..], value)
            || (!value.is_empty() && glob_match_parts(pattern, &value[1..]));
    }
    if value.is_empty() {
        return false;
    }
    segment_match(pattern[0], value[0]) && glob_match_parts(&pattern[1..], &value[1..])
}

fn segment_match(pattern: &str, value: &str) -> bool {
    let p = pattern.as_bytes();
    let v = value.as_bytes();
    let (mut pi, mut vi) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut star_match = 0usize;
    while vi < v.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == v[vi]) {
            pi += 1;
            vi += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            star_match = vi;
            pi += 1;
        } else if let Some(star_idx) = star {
            pi = star_idx + 1;
            star_match += 1;
            vi = star_match;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

fn reject_workspace_root_delete(
    workspace: &NativeToolWorkspace,
    path: &Path,
) -> Result<(), String> {
    if let Some(project) = &workspace.project {
        if let Some(root) = project.root_path.as_ref() {
            if let Ok(root) = fs::canonicalize(root) {
                if path == root {
                    return Err("不能删除、移动或覆盖项目根目录。".to_string());
                }
            }
        }
    }
    Ok(())
}

fn reject_recursive_directory_copy(source: &Path, destination: &Path) -> Result<(), String> {
    if destination == source || destination.starts_with(source) {
        return Err("不能将文件夹复制到自身或自身的子目录。".to_string());
    }
    Ok(())
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|err| format!("Create destination dir failed: {err}"))?;
    for entry in fs::read_dir(source).map_err(|err| format!("Read source dir failed: {err}"))? {
        let entry = entry.map_err(|err| format!("Read source entry failed: {err}"))?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let metadata = entry
            .metadata()
            .map_err(|err| format!("Read source metadata failed: {err}"))?;
        if metadata.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if metadata.is_file() {
            fs::copy(&from, &to).map_err(|err| format!("Copy file failed: {err}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    #[test]
    fn read_file_allows_temp_paths() {
        let file = std::env::temp_dir().join(format!("kivio_read_{}.txt", uuid::Uuid::new_v4()));
        fs::write(&file, "alpha\nbeta\n").expect("write");

        let workspace = NativeToolWorkspace::global(&[]);
        let content =
            read_file(&workspace, &json!({ "path": file.to_string_lossy() })).expect("read");
        assert_eq!(content, "alpha\nbeta\n");

        let _ = fs::remove_file(file);
    }

    #[test]
    fn edit_file_requires_unique_match_by_default() {
        let home = super::super::user_home_dir().expect("home");
        let dir = home.join(format!(".kivio_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("mkdir");
        let file = dir.join("sample.txt");
        fs::write(&file, "alpha\nbeta\nalpha\n").expect("write");

        let rel = file.to_string_lossy().to_string();
        let workspace = NativeToolWorkspace::global(&[]);
        let err = edit_file(
            &workspace,
            &json!({
                "path": rel,
                "old_string": "alpha",
                "new_string": "gamma"
            }),
        )
        .unwrap_err();
        assert!(err.contains("appears"));

        edit_file(
            &workspace,
            &json!({
                "path": rel,
                "old_string": "alpha",
                "new_string": "gamma",
                "replace_all": true
            }),
        )
        .expect("replace all");

        let content = fs::read_to_string(&file).expect("read");
        assert_eq!(content, "gamma\nbeta\ngamma\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn edit_file_reports_noop_when_old_equals_new() {
        let home = super::super::user_home_dir().expect("home");
        let dir = home.join(format!(".kivio_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("mkdir");
        let file = dir.join("sample.txt");
        fs::write(&file, "hello world").expect("write");

        let rel = file.to_string_lossy().to_string();
        let workspace = NativeToolWorkspace::global(&[]);
        let result = edit_file(
            &workspace,
            &json!({
                "path": rel,
                "old_string": "hello world",
                "new_string": "hello world"
            }),
        )
        .expect("noop edit");
        assert_eq!(result.files[0].operation, "noop");
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("no changes made")));
        assert_eq!(fs::read_to_string(&file).expect("read"), "hello world");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_file_returns_structured_diff_metadata() {
        let root = std::env::temp_dir().join(format!("kivio_write_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("mkdir");
        let workspace = NativeToolWorkspace::project(
            "proj_test".to_string(),
            "Test".to_string(),
            Some(root.to_string_lossy().into_owned()),
        );

        let result = write_file(
            &workspace,
            &json!({
                "path": "hello.txt",
                "content": "alpha\nbeta\n"
            }),
        )
        .expect("write");

        assert_eq!(result.operation, "create");
        assert_eq!(result.resolved_path.as_deref(), Some("hello.txt"));
        assert_eq!(result.files[0].path, "hello.txt");
        assert_eq!(result.additions, 2);
        assert_eq!(result.removals, 0);
        assert!(result.diff.contains("+++ b/hello.txt"));
        assert!(result.diff.contains("+alpha"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn patch_add_update_and_delete_files() {
        let root = std::env::temp_dir().join(format!("kivio_patch_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(root.join("edit.txt"), "alpha\nbeta\ngamma\n").expect("write edit");
        fs::write(root.join("delete.txt"), "gone\n").expect("write delete");
        let workspace = NativeToolWorkspace::project(
            "proj_test".to_string(),
            "Test".to_string(),
            Some(root.to_string_lossy().into_owned()),
        );

        let result = patch(
            &workspace,
            &json!({
                "patch": "*** Begin Patch\n*** Add File: new.txt\n+first\n+second\n*** Update File: edit.txt\n@@\n alpha\n-beta\n+delta\n gamma\n*** Delete File: delete.txt\n*** End Patch"
            }),
        )
        .expect("patch");

        assert_eq!(result.operation, "patch");
        assert_eq!(result.files.len(), 3);
        assert_eq!(
            fs::read_to_string(root.join("new.txt")).expect("read new"),
            "first\nsecond\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("edit.txt")).expect("read edit"),
            "alpha\ndelta\ngamma\n"
        );
        assert!(!root.join("delete.txt").exists());
        assert_eq!(result.additions, 3);
        assert_eq!(result.removals, 2);
        assert!(result.diff.contains("--- a/edit.txt"));
        assert!(result.diff.contains("+delta"));
        assert!(result.diff.contains("--- a/delete.txt"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn patch_failure_does_not_partially_modify_files() {
        let root = std::env::temp_dir().join(format!("kivio_patch_fail_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(root.join("a.txt"), "alpha\n").expect("write a");
        fs::write(root.join("b.txt"), "beta\n").expect("write b");
        let workspace = NativeToolWorkspace::project(
            "proj_test".to_string(),
            "Test".to_string(),
            Some(root.to_string_lossy().into_owned()),
        );

        let err = patch(
            &workspace,
            &json!({
                "patch": "*** Begin Patch\n*** Update File: a.txt\n@@\n-alpha\n+changed\n*** Update File: b.txt\n@@\n-missing\n+changed\n*** End Patch"
            }),
        )
        .unwrap_err();

        assert!(err.contains("did not match"));
        assert_eq!(
            fs::read_to_string(root.join("a.txt")).expect("read a"),
            "alpha\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("b.txt")).expect("read b"),
            "beta\n"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn patch_rejects_traversal_paths() {
        let root =
            std::env::temp_dir().join(format!("kivio_patch_escape_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("mkdir");
        let workspace = NativeToolWorkspace::project(
            "proj_test".to_string(),
            "Test".to_string(),
            Some(root.to_string_lossy().into_owned()),
        );

        let err = patch(
            &workspace,
            &json!({
                "patch": "*** Begin Patch\n*** Add File: ../escape.txt\n+nope\n*** End Patch"
            }),
        )
        .unwrap_err();

        assert!(err.contains(".."));
        assert!(!root.join("../escape.txt").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn patch_rejects_non_relative_header_paths() {
        let root =
            std::env::temp_dir().join(format!("kivio_patch_invalid_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("mkdir");
        let workspace = NativeToolWorkspace::project(
            "proj_test".to_string(),
            "Test".to_string(),
            Some(root.to_string_lossy().into_owned()),
        );

        for (path, expected) in [
            ("/tmp/escape.txt", "relative"),
            ("~/escape.txt", "~"),
            ("dir\\escape.txt", "forward slashes"),
        ] {
            let err = patch(
                &workspace,
                &json!({
                    "patch": format!("*** Begin Patch\n*** Add File: {path}\n+nope\n*** End Patch")
                }),
            )
            .unwrap_err();
            assert!(
                err.contains(expected),
                "expected {expected:?} in {err:?} for {path:?}"
            );
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn patch_rejects_duplicate_resolved_targets() {
        let root =
            std::env::temp_dir().join(format!("kivio_patch_duplicate_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("mkdir");
        let workspace = NativeToolWorkspace::project(
            "proj_test".to_string(),
            "Test".to_string(),
            Some(root.to_string_lossy().into_owned()),
        );

        let err = patch(
            &workspace,
            &json!({
                "patch": "*** Begin Patch\n*** Add File: same.txt\n+one\n*** Add File: ./same.txt\n+two\n*** End Patch"
            }),
        )
        .unwrap_err();

        assert!(err.contains("same resolved file"));
        assert!(!root.join("same.txt").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn project_workspace_rejects_escape_paths() {
        let root = std::env::temp_dir().join(format!("kivio_project_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("mkdir");
        let workspace = NativeToolWorkspace::project(
            "proj_test".to_string(),
            "Test".to_string(),
            Some(root.to_string_lossy().into_owned()),
        );

        let err = read_file(&workspace, &json!({ "path": "../secret.txt" })).unwrap_err();
        assert!(err.contains(".."));

        let outside = std::env::temp_dir().join(format!("kivio_outside_{}", uuid::Uuid::new_v4()));
        fs::write(&outside, "secret").expect("write outside");
        let err = read_file(&workspace, &json!({ "path": outside.to_string_lossy() })).unwrap_err();
        assert!(err.contains("项目根目录"));

        let _ = fs::remove_file(outside);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn write_file_chunk_start_append_finish_lifecycle_in_project_workspace() {
        let root = std::env::temp_dir().join(format!("kivio_chunk_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("mkdir");
        let workspace = NativeToolWorkspace::project(
            "proj_test".to_string(),
            "Test".to_string(),
            Some(root.to_string_lossy().into_owned()),
        );

        let start = write_file_chunk(
            &workspace,
            &json!({ "path": "out/long.txt", "mode": "start", "content": "line1\nline2\n" }),
        )
        .expect("start");
        assert_eq!(start.operation, "write_chunk_start");
        assert_eq!(start.files.len(), 1);
        assert_eq!(start.files[0].operation, "create");
        assert_eq!(
            fs::read_to_string(root.join("out/long.txt")).expect("read after start"),
            "line1\nline2\n"
        );

        let append = write_file_chunk(
            &workspace,
            &json!({ "path": "out/long.txt", "mode": "append", "content": "line3\n" }),
        )
        .expect("append");
        assert_eq!(append.operation, "write_chunk_append");
        assert_eq!(append.files[0].operation, "append");
        assert_eq!(
            fs::read_to_string(root.join("out/long.txt")).expect("read after append"),
            "line1\nline2\nline3\n"
        );

        let finish = write_file_chunk(
            &workspace,
            &json!({ "path": "out/long.txt", "mode": "finish" }),
        )
        .expect("finish");
        assert_eq!(finish.operation, "write_chunk_finish");
        assert_eq!(finish.files[0].operation, "finish");
        assert_eq!(finish.bytes_written, "line1\nline2\nline3\n".len() as u64);

        let err = write_file_chunk(
            &workspace,
            &json!({ "path": "out/missing.txt", "mode": "append", "content": "x" }),
        )
        .unwrap_err();
        assert!(err.contains("mode=start"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn copy_path_rejects_directory_copy_into_self_or_child() {
        let root = std::env::temp_dir().join(format!("kivio_copy_{}", uuid::Uuid::new_v4()));
        let source = root.join("src");
        fs::create_dir_all(&source).expect("mkdir source");
        fs::write(source.join("file.txt"), "hello").expect("write source file");
        let workspace = NativeToolWorkspace::project(
            "proj_test".to_string(),
            "Test".to_string(),
            Some(root.to_string_lossy().into_owned()),
        );

        let same_err = copy_path(
            &workspace,
            &json!({
                "from": "src",
                "to": "src"
            }),
        )
        .unwrap_err();
        assert!(same_err.contains("自身"));

        let child_err = copy_path(
            &workspace,
            &json!({
                "from": "src",
                "to": "src/backup"
            }),
        )
        .unwrap_err();
        assert!(child_err.contains("自身"));
        assert!(!source.join("backup").exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn glob_files_rejects_path_like_patterns() {
        let root = std::env::temp_dir().join(format!("kivio_glob_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(root.join("package.json"), "{}").expect("write package");
        let workspace = NativeToolWorkspace::project(
            "proj_test".to_string(),
            "Test".to_string(),
            Some(root.to_string_lossy().into_owned()),
        );

        let absolute_err = glob_files(
            &workspace,
            &json!({
                "pattern": format!("{}/*.json", root.display())
            }),
        )
        .unwrap_err();
        assert!(absolute_err.contains("relative"));

        let parent_err = glob_files(
            &workspace,
            &json!({
                "pattern": "../*.json"
            }),
        )
        .unwrap_err();
        assert!(parent_err.contains(".."));

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn delete_path_removes_project_symlink_without_following_target() {
        let root = std::env::temp_dir().join(format!("kivio_link_root_{}", uuid::Uuid::new_v4()));
        let outside =
            std::env::temp_dir().join(format!("kivio_link_target_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("mkdir root");
        fs::write(&outside, "outside").expect("write outside");
        let link = root.join("outside-link.txt");
        std::os::unix::fs::symlink(&outside, &link).expect("symlink");
        let workspace = NativeToolWorkspace::project(
            "proj_test".to_string(),
            "Test".to_string(),
            Some(root.to_string_lossy().into_owned()),
        );

        let result = delete_path(&workspace, &json!({ "path": "outside-link.txt" }))
            .expect("delete symlink");

        assert!(result.contains("outside-link.txt"));
        assert!(!link.exists());
        assert!(outside.exists());

        let _ = fs::remove_file(outside);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn patch_applies_empty_context_line_without_marker() {
        let root =
            std::env::temp_dir().join(format!("kivio_patch_empty_ctx_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(root.join("spaced.txt"), "alpha\n\nbeta\n").expect("write spaced");
        let workspace = NativeToolWorkspace::project(
            "proj_test".to_string(),
            "Test".to_string(),
            Some(root.to_string_lossy().into_owned()),
        );

        // The empty context line between "alpha" and "-beta" has no leading space.
        let result = patch(
            &workspace,
            &json!({
                "patch": "*** Begin Patch\n*** Update File: spaced.txt\n@@\n alpha\n\n-beta\n+gamma\n*** End Patch"
            }),
        )
        .expect("patch with empty context line");

        assert_eq!(result.operation, "patch");
        assert_eq!(
            fs::read_to_string(root.join("spaced.txt")).expect("read spaced"),
            "alpha\n\ngamma\n"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn patch_added_content_emits_trailing_newline() {
        assert_eq!(
            patch_added_content(&["first".to_string(), "second".to_string()]),
            "first\nsecond\n"
        );
        assert_eq!(patch_added_content(&[]), "");
    }

    #[test]
    fn write_file_overwrites_non_utf8_file_with_warning() {
        let root = std::env::temp_dir().join(format!("kivio_binary_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(root.join("bin.dat"), [0xffu8, 0xfe, 0x01]).expect("write binary");
        let workspace = NativeToolWorkspace::project(
            "proj_test".to_string(),
            "Test".to_string(),
            Some(root.to_string_lossy().into_owned()),
        );

        let result = write_file(
            &workspace,
            &json!({
                "path": "bin.dat",
                "content": "clean\n"
            }),
        )
        .expect("overwrite non-utf8 file");

        assert_eq!(result.operation, "overwrite");
        assert!(result.diff.is_empty());
        assert_eq!(result.additions, 0);
        assert_eq!(result.removals, 0);
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("not valid UTF-8")));
        assert_eq!(
            fs::read_to_string(root.join("bin.dat")).expect("read"),
            "clean\n"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn edit_file_replace_all_reports_exact_stats_with_multiple_hunks() {
        let root = std::env::temp_dir().join(format!("kivio_hunks_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("mkdir");
        let mut lines = vec!["needle old".to_string()];
        for i in 0..20 {
            lines.push(format!("unchanged line {i}"));
        }
        lines.push("needle old".to_string());
        let content = format!("{}\n", lines.join("\n"));
        fs::write(root.join("scatter.txt"), &content).expect("write scatter");
        let workspace = NativeToolWorkspace::project(
            "proj_test".to_string(),
            "Test".to_string(),
            Some(root.to_string_lossy().into_owned()),
        );

        let result = edit_file(
            &workspace,
            &json!({
                "path": "scatter.txt",
                "old_string": "old",
                "new_string": "new",
                "replace_all": true
            }),
        )
        .expect("replace all");

        assert_eq!(result.additions, 2);
        assert_eq!(result.removals, 2);
        assert_eq!(result.diff.matches("@@ -").count(), 2);
        let removed_lines = result
            .diff
            .lines()
            .filter(|line| line.starts_with('-') && !line.starts_with("---"))
            .count();
        let added_lines = result
            .diff
            .lines()
            .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
            .count();
        assert_eq!(removed_lines, 2);
        assert_eq!(added_lines, 2);

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn move_path_moves_project_symlink_without_following_target() {
        let root =
            std::env::temp_dir().join(format!("kivio_move_link_root_{}", uuid::Uuid::new_v4()));
        let outside =
            std::env::temp_dir().join(format!("kivio_move_link_target_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("mkdir root");
        fs::write(&outside, "outside").expect("write outside");
        let link = root.join("outside-link.txt");
        std::os::unix::fs::symlink(&outside, &link).expect("symlink");
        let workspace = NativeToolWorkspace::project(
            "proj_test".to_string(),
            "Test".to_string(),
            Some(root.to_string_lossy().into_owned()),
        );

        let result = move_path(
            &workspace,
            &json!({
                "from": "outside-link.txt",
                "to": "moved-link.txt"
            }),
        )
        .expect("move symlink");

        assert!(result.contains("moved-link.txt"));
        assert!(!link.exists() && fs::symlink_metadata(&link).is_err());
        let moved = root.join("moved-link.txt");
        let metadata = fs::symlink_metadata(&moved).expect("moved metadata");
        assert!(metadata.file_type().is_symlink());
        assert_eq!(fs::read_to_string(&outside).expect("read outside"), "outside");

        let _ = fs::remove_file(outside);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn patch_in_global_workspace_warns_about_resolved_base() {
        let home = super::super::user_home_dir().expect("home");
        let name = format!("kivio_test_patch_global_{}.txt", uuid::Uuid::new_v4());
        let workspace = NativeToolWorkspace::global(&[]);

        let result = patch(
            &workspace,
            &json!({
                "patch": format!("*** Begin Patch\n*** Add File: {name}\n+hello\n*** End Patch")
            }),
        )
        .expect("global patch");

        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("No project folder")));
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains(&home.display().to_string())));
        let target = home.join(&name);
        assert_eq!(
            fs::read_to_string(&target).expect("read global file"),
            "hello\n"
        );

        let _ = fs::remove_file(target);
    }
}
