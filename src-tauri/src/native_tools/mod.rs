mod fetch;
mod files;
mod shell;

pub use fetch::web_fetch;
pub use files::{edit_file, read_file, write_file};
pub use shell::run_command;

use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub const MAX_READ_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// Block writes/edits under these path segments (relative to home).
const WRITE_BLOCKLIST_SEGMENTS: &[&str] = &[
    ".ssh",
    ".gnupg",
    "Library/Keychains",
    "Library/Application Support/Keychain",
];

pub fn user_home_dir() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .map_err(|_| "USERPROFILE is not set".to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .map_err(|_| "HOME is not set".to_string())
    }
}

pub fn resolve_workspace_path(
    raw_path: &str,
    workspace_roots: &[String],
) -> Result<PathBuf, String> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err("路径为空，请提供要访问的文件或目录路径。".to_string());
    }

    let home = user_home_dir()?;
    let home_canon = fs_canonicalize_existing_or_self(&home)?;

    let candidate = {
        let path = Path::new(trimmed);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            home.join(path)
        }
    };

    for component in candidate.components() {
        if matches!(component, Component::ParentDir) {
            return Err("路径不能包含 '..'，请使用明确的文件路径。".to_string());
        }
    }

    let canonical = fs_canonicalize_existing_or_self(&candidate)?;
    if !canonical.starts_with(&home_canon) {
        return Err("路径不在允许范围内：只能访问用户主目录下的文件。".to_string());
    }

    if !workspace_roots.is_empty() {
        let allowed = workspace_roots.iter().any(|root| {
            let root_path = Path::new(root.trim());
            if root_path.as_os_str().is_empty() {
                return false;
            }
            let root_canon = fs_canonicalize_existing_or_self(root_path).ok();
            root_canon
                .map(|root_canon| {
                    root_canon.starts_with(&home_canon) && canonical.starts_with(&root_canon)
                })
                .unwrap_or(false)
        });
        if !allowed {
            return Err(
                "路径不在允许的工作区根目录内，请到设置 > MCP > Kivio 内置工具检查 workspaceRoots。"
                    .to_string(),
            );
        }
    }

    Ok(canonical)
}

pub fn assert_writable_path(path: &Path) -> Result<(), String> {
    let home = user_home_dir()?;
    let home_canon = fs_canonicalize_existing_or_self(&home)?;
    let canonical = if path.exists() {
        fs::canonicalize(path).map_err(|err| format!("Resolve path failed: {err}"))?
    } else if let Some(parent) = path.parent() {
        let parent_canon = fs_canonicalize_existing_or_self(parent)?;
        parent_canon.join(path.file_name().ok_or_else(|| "Invalid path".to_string())?)
    } else {
        return Err("Invalid path".to_string());
    };

    if !canonical.starts_with(&home_canon) {
        return Err("路径不在允许范围内：只能写入用户主目录下的文件。".to_string());
    }

    let relative = canonical
        .strip_prefix(&home_canon)
        .map_err(|_| "Invalid path".to_string())?;
    let rel = relative.to_string_lossy();
    for blocked in WRITE_BLOCKLIST_SEGMENTS {
        if rel.as_ref() == *blocked || rel.starts_with(&format!("{blocked}/")) {
            return Err(format!("出于安全策略，禁止写入 {blocked} 目录。"));
        }
    }
    Ok(())
}

fn fs_canonicalize_existing_or_self(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        fs::canonicalize(path).map_err(|err| format!("Resolve path failed: {err}"))
    } else {
        Ok(path.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent_dir_traversal() {
        let err = resolve_workspace_path("../etc/passwd", &[]).unwrap_err();
        assert!(err.contains(".."));
    }
}
