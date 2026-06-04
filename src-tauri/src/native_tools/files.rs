use std::fs;

use serde_json::Value;

use super::{assert_writable_path, resolve_workspace_path, MAX_READ_FILE_BYTES};

pub fn read_file(workspace_roots: &[String], arguments: &Value) -> Result<String, String> {
    let path = arguments
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "read_file requires path".to_string())?;
    let full = resolve_workspace_path(path, workspace_roots)?;
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

pub fn write_file(workspace_roots: &[String], arguments: &Value) -> Result<String, String> {
    let path = arguments
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "write_file requires path".to_string())?;
    let content = arguments
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "write_file requires content".to_string())?;
    let full = resolve_workspace_path(path, workspace_roots)?;
    assert_writable_path(&full)?;
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("Create parent dirs failed: {err}"))?;
    }
    fs::write(&full, content).map_err(|err| format!("Write file failed: {err}"))?;
    Ok(format!(
        "Wrote {} bytes to {}",
        content.len(),
        full.display()
    ))
}

pub fn edit_file(workspace_roots: &[String], arguments: &Value) -> Result<String, String> {
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

    let full = resolve_workspace_path(path, workspace_roots)?;
    assert_writable_path(&full)?;
    if !full.is_file() {
        return Err(format!("不是可编辑的文件: {path}"));
    }

    let content = fs::read_to_string(&full).map_err(|err| format!("Read file failed: {err}"))?;
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
        full.display(),
        if replace_all { count } else { 1 }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    #[test]
    fn edit_file_requires_unique_match_by_default() {
        let home = super::super::user_home_dir().expect("home");
        let dir = home.join(format!(".kivio_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("mkdir");
        let file = dir.join("sample.txt");
        fs::write(&file, "alpha\nbeta\nalpha\n").expect("write");

        let rel = file.to_string_lossy().to_string();
        let err = edit_file(
            &[],
            &json!({
                "path": rel,
                "old_string": "alpha",
                "new_string": "gamma"
            }),
        )
        .unwrap_err();
        assert!(err.contains("appears"));

        edit_file(
            &[],
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
}
