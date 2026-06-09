use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use serde_json::{json, Value};

use super::{
    assert_writable_path, resolve_tool_read_path, resolve_tool_write_entry_path,
    resolve_tool_write_path, workspace_display_path, NativeToolWorkspace, MAX_READ_FILE_BYTES,
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

pub fn write_file(workspace: &NativeToolWorkspace, arguments: &Value) -> Result<String, String> {
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
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("Create parent dirs failed: {err}"))?;
    }
    fs::write(&full, content).map_err(|err| format!("Write file failed: {err}"))?;
    Ok(format!(
        "Wrote {} bytes to {}",
        content.len(),
        workspace_display_path(workspace, &full)
    ))
}

pub fn edit_file(workspace: &NativeToolWorkspace, arguments: &Value) -> Result<String, String> {
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
    if !full.is_file() {
        return Err(format!("不是可编辑的文件: {path}"));
    }

    let content = fs::read_to_string(&full).map_err(|err| format!("Read file failed: {err}"))?;
    if old_string == new_string {
        return Ok(format!("No changes made: {path}"));
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
    Ok(format!(
        "Updated {} ({} replacement(s))",
        workspace_display_path(workspace, &full),
        if replace_all { count } else { 1 }
    ))
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
    let source = resolve_tool_write_path(workspace, from)?;
    let destination = resolve_tool_write_path(workspace, to)?;
    if !workspace.has_project() {
        assert_writable_path(&source)?;
        assert_writable_path(&destination)?;
    }
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
        assert!(result.contains("No changes made"));
        assert_eq!(fs::read_to_string(&file).expect("read"), "hello world");

        let _ = fs::remove_dir_all(&dir);
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
}
