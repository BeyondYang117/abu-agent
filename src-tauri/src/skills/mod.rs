use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_shell::ShellExt;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMeta {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub path: Option<String>,
    pub recommended_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDetail {
    #[serde(flatten)]
    pub meta: SkillMeta,
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillListResult {
    pub success: bool,
    pub skills: Vec<SkillMeta>,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillReadResult {
    pub success: bool,
    pub skill: Option<SkillDetail>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillImportResult {
    pub success: bool,
    pub skill: Option<SkillMeta>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillOpenFolderResult {
    pub success: bool,
    pub path: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub fn chat_skills_list(
    app: AppHandle,
    state: State<'_, AppState>,
    skill_scan_paths: Option<Vec<String>>,
) -> SkillListResult {
    let paths = skill_scan_paths
        .unwrap_or_else(|| state.settings_read().chat_tools.skill_scan_paths.clone());
    match collect_skills(&app, &paths) {
        Ok((skills, warnings)) => SkillListResult {
            success: true,
            skills,
            error: if warnings.is_empty() {
                None
            } else {
                Some(warnings.join("\n"))
            },
            warnings,
        },
        Err(err) => SkillListResult {
            success: false,
            skills: Vec::new(),
            warnings: Vec::new(),
            error: Some(err),
        },
    }
}

#[tauri::command]
pub fn chat_skills_read(
    app: AppHandle,
    state: State<'_, AppState>,
    skill_id: String,
) -> SkillReadResult {
    match read_skill_detail(
        &app,
        &state.settings_read().chat_tools.skill_scan_paths,
        &skill_id,
    ) {
        Ok(skill) => SkillReadResult {
            success: true,
            skill: Some(skill),
            error: None,
        },
        Err(err) => SkillReadResult {
            success: false,
            skill: None,
            error: Some(err),
        },
    }
}

/// 在系统文件管理器中打开用户 Skill 目录（不存在则创建）。
#[tauri::command]
#[allow(deprecated)]
pub fn chat_skills_open_folder(app: AppHandle) -> SkillOpenFolderResult {
    match user_skills_dir(&app) {
        Ok(dir) => {
            let path = dir.display().to_string();
            if let Err(err) = app.shell().open(&path, None) {
                SkillOpenFolderResult {
                    success: false,
                    path: Some(path),
                    error: Some(err.to_string()),
                }
            } else {
                SkillOpenFolderResult {
                    success: true,
                    path: Some(path),
                    error: None,
                }
            }
        }
        Err(err) => SkillOpenFolderResult {
            success: false,
            path: None,
            error: Some(err),
        },
    }
}

#[tauri::command]
pub fn chat_skills_import(app: AppHandle, path: String) -> SkillImportResult {
    let source = PathBuf::from(path);
    let skills_dir = match user_skills_dir(&app) {
        Ok(path) => path,
        Err(err) => {
            return SkillImportResult {
                success: false,
                skill: None,
                error: Some(err),
            }
        }
    };
    let result = if source.is_dir() {
        import_skill_dir(&source, &skills_dir)
    } else if source
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
    {
        import_skill_zip(&source, &skills_dir)
    } else {
        Err("Skill import expects a folder or zip containing SKILL.md".to_string())
    };

    match result {
        Ok(meta) => SkillImportResult {
            success: true,
            skill: Some(meta),
            error: None,
        },
        Err(err) => SkillImportResult {
            success: false,
            skill: None,
            error: Some(err),
        },
    }
}

pub fn read_skill_detail(
    app: &AppHandle,
    extra_paths: &[String],
    skill_id: &str,
) -> Result<SkillDetail, String> {
    for path in scan_roots(app, extra_paths)? {
        for skill_path in skill_files_under(&path)? {
            let raw = fs::read_to_string(&skill_path)
                .map_err(|err| format!("Read skill {} failed: {err}", skill_path.display()))?;
            let id = skill_id_for_path(&skill_path);
            let parsed =
                parse_skill_markdown(&id, &raw, "user", Some(skill_path.display().to_string()))?;
            if parsed.meta.id == skill_id {
                return Ok(parsed);
            }
        }
    }

    Err(format!("Skill not found: {skill_id}"))
}

fn collect_skills(
    app: &AppHandle,
    extra_paths: &[String],
) -> Result<(Vec<SkillMeta>, Vec<String>), String> {
    let mut skills = Vec::new();
    let mut warnings = Vec::new();

    for path in scan_roots(app, extra_paths)? {
        for skill_path in skill_files_under(&path)? {
            let raw = match fs::read_to_string(&skill_path) {
                Ok(raw) => raw,
                Err(err) => {
                    warnings.push(format!("Read skill {} failed: {err}", skill_path.display()));
                    continue;
                }
            };
            let id = skill_id_for_path(&skill_path);
            match parse_skill_markdown(&id, &raw, "user", Some(skill_path.display().to_string())) {
                Ok(parsed) => skills.push(parsed.meta),
                Err(err) => warnings.push(format!(
                    "Parse skill {} failed: {err}",
                    skill_path.display()
                )),
            }
        }
    }

    skills.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    skills.dedup_by(|a, b| a.id == b.id);
    Ok((skills, warnings))
}

fn scan_roots(app: &AppHandle, extra_paths: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut roots = vec![user_skills_dir(app)?];
    roots.extend(
        extra_paths
            .iter()
            .map(PathBuf::from)
            .filter(|path| path.is_dir()),
    );
    Ok(roots)
}

fn user_skills_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("app_data_dir unavailable: {err}"))?
        .join("skills");
    fs::create_dir_all(&dir).map_err(|err| format!("create skills dir failed: {err}"))?;
    Ok(dir)
}

fn skill_files_under(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    if !root.is_dir() {
        return Ok(out);
    }
    for entry in fs::read_dir(root).map_err(|err| format!("read skills dir failed: {err}"))? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            let candidate = path.join("SKILL.md");
            if candidate.is_file() {
                out.push(candidate);
            }
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name == "SKILL.md")
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
    Ok(out)
}

fn parse_skill_markdown(
    fallback_id: &str,
    raw: &str,
    source: &str,
    path: Option<String>,
) -> Result<SkillDetail, String> {
    let (frontmatter, body) = split_frontmatter(raw);
    let name = frontmatter
        .get("name")
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "Skill name is required".to_string())?;
    let description = frontmatter
        .get("description")
        .cloned()
        .ok_or_else(|| "Skill description is required".to_string())?;
    let mut recommended_tools = parse_list_value(frontmatter.get("recommended-tools"));
    recommended_tools.extend(parse_list_value(frontmatter.get("mcp-tools")));
    recommended_tools.sort();
    recommended_tools.dedup();
    let id = slugify(
        frontmatter
            .get("id")
            .map(String::as_str)
            .unwrap_or(fallback_id),
    );
    Ok(SkillDetail {
        meta: SkillMeta {
            id,
            name,
            description,
            source: source.to_string(),
            path,
            recommended_tools,
        },
        body: body.trim().to_string(),
    })
}

fn split_frontmatter(raw: &str) -> (std::collections::HashMap<String, String>, &str) {
    let mut map = std::collections::HashMap::new();
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return (map, raw);
    }
    let rest = &trimmed[3..];
    let Some(end) = rest.find("\n---") else {
        return (map, raw);
    };
    let fm = &rest[..end];
    let body = &rest[end + 4..];
    let mut current_key: Option<String> = None;
    let mut current_items: Vec<String> = Vec::new();
    for line in fm.lines() {
        let line = line.trim_end();
        if let Some(item) = line.trim_start().strip_prefix("- ") {
            if current_key.is_some() {
                current_items.push(item.trim().to_string());
            }
            continue;
        }
        if let Some(key) = current_key.take() {
            if !current_items.is_empty() {
                map.insert(key, current_items.join(","));
                current_items.clear();
            }
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_string();
        let value = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        if value.is_empty() {
            current_key = Some(key);
        } else {
            map.insert(key, value);
        }
    }
    if let Some(key) = current_key {
        if !current_items.is_empty() {
            map.insert(key, current_items.join(","));
        }
    }
    (map, body)
}

fn parse_list_value(value: Option<&String>) -> Vec<String> {
    value
        .map(|value| {
            value
                .trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .split(',')
                .map(|item| item.trim().trim_matches('"').trim_matches('\'').to_string())
                .filter(|item| !item.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn import_skill_dir(source: &Path, skills_dir: &Path) -> Result<SkillMeta, String> {
    let skill_file = source.join("SKILL.md");
    if !skill_file.is_file() {
        return Err("Selected folder does not contain SKILL.md".to_string());
    }
    let raw =
        fs::read_to_string(&skill_file).map_err(|err| format!("Read SKILL.md failed: {err}"))?;
    let parsed = parse_skill_markdown(
        source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("skill"),
        &raw,
        "user",
        None,
    )?;
    let dest = skills_dir.join(&parsed.meta.id);
    copy_dir_recursive(source, &dest)?;
    Ok(SkillMeta {
        path: Some(dest.join("SKILL.md").display().to_string()),
        ..parsed.meta
    })
}

fn import_skill_zip(source: &Path, skills_dir: &Path) -> Result<SkillMeta, String> {
    let bytes = fs::read(source).map_err(|err| format!("Read zip failed: {err}"))?;
    let reader = Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|err| format!("Open zip failed: {err}"))?;
    let mut skill_raw = String::new();
    let mut skill_path = String::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|err| err.to_string())?;
        if file.name().ends_with("SKILL.md") {
            file.read_to_string(&mut skill_raw)
                .map_err(|err| format!("Read SKILL.md in zip failed: {err}"))?;
            skill_path = file.name().to_string();
            break;
        }
    }
    if skill_raw.trim().is_empty() {
        return Err("Zip does not contain SKILL.md".to_string());
    }
    let parsed = parse_skill_markdown("skill", &skill_raw, "user", None)?;
    let dest = skills_dir.join(&parsed.meta.id);
    fs::create_dir_all(&dest).map_err(|err| format!("create skill dir failed: {err}"))?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|err| err.to_string())?;
        if file.is_dir() {
            continue;
        }
        let name = file.name();
        let relative = if skill_path.contains('/') {
            let prefix = skill_path
                .rsplit_once('/')
                .map(|(prefix, _)| format!("{prefix}/"))
                .unwrap_or_default();
            name.strip_prefix(&prefix).unwrap_or(name)
        } else {
            name
        };
        if relative.contains("..") {
            continue;
        }
        let out = dest.join(relative);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let mut output = fs::File::create(&out).map_err(|err| err.to_string())?;
        std::io::copy(&mut file, &mut output).map_err(|err| err.to_string())?;
    }
    Ok(SkillMeta {
        path: Some(dest.join("SKILL.md").display().to_string()),
        ..parsed.meta
    })
}

fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|err| err.to_string())?;
    for entry in fs::read_dir(from).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_dir_recursive(&src, &dst)?;
        } else if src.is_file() {
            fs::copy(&src, &dst).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn skill_id_for_path(path: &Path) -> String {
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .map(slugify)
        .unwrap_or_else(|| "skill".to_string())
}

fn slugify(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if c == '-' || c == '_' || c.is_whitespace() {
            if !out.ends_with('-') {
                out.push('-');
            }
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        format!("skill-{}", uuid::Uuid::new_v4())
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skill_supports_recommended_tools_and_mcp_tools_alias() {
        let raw = r#"---
name: Test Skill
description: Uses selected tools.
recommended-tools:
  - web_search
mcp-tools:
  - fetch
  - web_search
---
# Body
"#;

        let parsed = parse_skill_markdown("test-skill", raw, "user", None).unwrap();

        assert_eq!(
            parsed.meta.recommended_tools,
            vec!["fetch".to_string(), "web_search".to_string()]
        );
    }

    #[test]
    fn parse_skill_requires_name_and_description() {
        let err = parse_skill_markdown(
            "missing-name",
            r#"---
description: Missing name.
---
# Body
"#,
            "user",
            None,
        )
        .expect_err("missing name should be rejected");

        assert!(err.contains("name"));
    }
}
