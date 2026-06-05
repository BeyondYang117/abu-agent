use std::{
    fs,
    path::{Path, PathBuf},
};

use tauri::{AppHandle, Manager};

use super::{
    parse::parse_skill_record,
    types::{slugify, SkillFileEntry, SkillFileKind, SkillRegistry},
};

const MAX_SCAN_DEPTH: usize = 6;

const SKIP_DIR_NAMES: &[&str] = &[".git", "node_modules", ".svn", ".hg"];

struct SkillScanRoot {
    path: PathBuf,
    source: &'static str,
}

pub fn user_skills_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("app_data_dir unavailable: {err}"))?
        .join("skills");
    fs::create_dir_all(&dir).map_err(|err| format!("create skills dir failed: {err}"))?;
    Ok(dir)
}

fn bundled_skills_dir(app: &AppHandle) -> Option<PathBuf> {
    let resource_dir = app.path().resource_dir().ok()?.join("builtin-skills");
    resource_dir.is_dir().then_some(resource_dir)
}

fn dev_builtin_skills_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("builtin-skills");
    dir.is_dir().then_some(dir)
}

pub fn scan_roots(app: &AppHandle, extra_paths: &[String]) -> Result<Vec<PathBuf>, String> {
    Ok(scan_root_entries(app, extra_paths)?
        .into_iter()
        .map(|entry| entry.path)
        .collect())
}

fn scan_root_entries(
    app: &AppHandle,
    extra_paths: &[String],
) -> Result<Vec<SkillScanRoot>, String> {
    let mut roots = vec![SkillScanRoot {
        path: user_skills_dir(app)?,
        source: "user",
    }];
    roots.extend(extra_paths.iter().map(PathBuf::from).filter_map(|path| {
        path.is_dir().then_some(SkillScanRoot {
            path,
            source: "external",
        })
    }));
    if let Some(dir) = bundled_skills_dir(app).or_else(dev_builtin_skills_dir) {
        if !roots.iter().any(|root| root.path == dir) {
            roots.push(SkillScanRoot {
                path: dir,
                source: "builtin",
            });
        }
    }
    Ok(roots)
}

pub fn build_registry(app: &AppHandle, extra_paths: &[String]) -> Result<SkillRegistry, String> {
    let mut registry = SkillRegistry::default();
    let roots = scan_root_entries(app, extra_paths)?;

    for root in roots {
        collect_skill_files(&root.path, 0, &mut registry, root.source)?;
    }

    dedup_records(&mut registry.records, &mut registry.warnings);
    registry
        .records
        .sort_by(|a, b| a.meta.name.to_lowercase().cmp(&b.meta.name.to_lowercase()));
    Ok(registry)
}

fn dedup_records(records: &mut Vec<super::types::SkillRecord>, warnings: &mut Vec<String>) {
    let mut seen = std::collections::HashMap::<String, usize>::new();
    let mut out: Vec<super::types::SkillRecord> = Vec::new();
    for record in records.drain(..) {
        let key = record.meta.id.clone();
        if let Some(index) = seen.get(&key) {
            warnings.push(format!(
                "Skill {} shadowed duplicate id {}",
                out[*index].meta.name, record.meta.name
            ));
            continue;
        }
        seen.insert(key, out.len());
        out.push(record);
    }
    *records = out;
}

fn collect_skill_files(
    root: &Path,
    depth: usize,
    registry: &mut SkillRegistry,
    source: &str,
) -> Result<(), String> {
    if depth > MAX_SCAN_DEPTH || !root.is_dir() {
        return Ok(());
    }

    let skill_md = root.join("SKILL.md");
    if skill_md.is_file() {
        match load_skill_at(&skill_md, source) {
            Ok(record) => registry.records.push(record),
            Err(err) => registry
                .warnings
                .push(format!("Parse skill {} failed: {err}", skill_md.display())),
        }
        return Ok(());
    }

    if depth == MAX_SCAN_DEPTH {
        return Ok(());
    }

    let entries = fs::read_dir(root).map_err(|err| format!("read skills dir failed: {err}"))?;
    for entry in entries {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if SKIP_DIR_NAMES.contains(&name) {
            continue;
        }
        collect_skill_files(&path, depth + 1, registry, source)?;
    }
    Ok(())
}

fn load_skill_at(skill_md_path: &Path, source: &str) -> Result<super::types::SkillRecord, String> {
    let raw = fs::read_to_string(skill_md_path)
        .map_err(|err| format!("Read skill {} failed: {err}", skill_md_path.display()))?;
    let base_dir = skill_md_path
        .parent()
        .ok_or_else(|| "Skill path has no parent directory".to_string())?;
    let files = index_skill_files(base_dir)?;
    let mut warnings = Vec::new();
    parse_skill_record(skill_md_path, &raw, source, files, &mut warnings)
}

pub fn index_skill_files(base_dir: &Path) -> Result<Vec<SkillFileEntry>, String> {
    let mut files = Vec::new();
    walk_files(base_dir, base_dir, 0, &mut files)?;
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(files)
}

fn walk_files(
    base_dir: &Path,
    current: &Path,
    depth: usize,
    out: &mut Vec<SkillFileEntry>,
) -> Result<(), String> {
    if depth > MAX_SCAN_DEPTH {
        return Ok(());
    }
    let entries = fs::read_dir(current).map_err(|err| err.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if path.is_dir() {
            if SKIP_DIR_NAMES.iter().any(|skip| *skip == name.as_ref()) {
                continue;
            }
            walk_files(base_dir, &path, depth + 1, out)?;
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let relative = path
            .strip_prefix(base_dir)
            .map_err(|err| err.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if relative == "SKILL.md" {
            continue;
        }
        let size_bytes = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        out.push(SkillFileEntry {
            relative_path: relative.clone(),
            kind: classify_file(&relative),
            size_bytes,
        });
    }
    Ok(())
}

fn classify_file(relative_path: &str) -> SkillFileKind {
    let normalized = relative_path.replace('\\', "/");
    if normalized.starts_with("scripts/") {
        return SkillFileKind::Script;
    }
    if normalized.starts_with("references/") || normalized.ends_with(".md") {
        return SkillFileKind::Reference;
    }
    if normalized.starts_with("assets/") {
        return SkillFileKind::Asset;
    }
    SkillFileKind::Other
}

pub fn folder_slug_for_path(path: &Path) -> String {
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .map(slugify)
        .unwrap_or_else(|| "skill".to_string())
}
