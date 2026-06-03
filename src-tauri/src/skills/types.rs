use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillFileKind {
    SkillMd,
    Reference,
    Script,
    Asset,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFileEntry {
    pub relative_path: String,
    pub kind: SkillFileKind,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMeta {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub path: Option<String>,
    pub recommended_tools: Vec<String>,
    #[serde(default)]
    pub disable_model_invocation: bool,
    #[serde(default)]
    pub files: Vec<SkillFileEntry>,
}

#[derive(Debug, Clone)]
pub struct SkillRecord {
    pub meta: SkillMeta,
    pub location: PathBuf,
    pub base_dir: PathBuf,
    pub body: String,
    pub allowed_tools: Vec<String>,
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

#[derive(Debug, Clone, Default)]
pub struct SkillRegistry {
    pub records: Vec<SkillRecord>,
    pub warnings: Vec<String>,
}

impl SkillRegistry {
    pub fn find(&self, id_or_name: &str) -> Option<&SkillRecord> {
        let needle = id_or_name.trim();
        if needle.is_empty() {
            return None;
        }
        let slug = slugify(needle);
        self.records.iter().find(|record| {
            record.meta.id == needle
                || record.meta.id == slug
                || record.meta.name == needle
                || slugify(&record.meta.name) == slug
        })
    }

    pub fn metas(&self) -> Vec<SkillMeta> {
        self.records
            .iter()
            .map(|record| record.meta.clone())
            .collect()
    }
}

pub fn slugify(value: &str) -> String {
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

pub fn parse_bool(value: Option<&str>) -> bool {
    matches!(
        value
            .map(str::trim)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "true" | "1" | "yes"
    )
}
