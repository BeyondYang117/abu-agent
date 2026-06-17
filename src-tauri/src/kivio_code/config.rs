//! Persisted kivio-code configuration (distinct from the shared Kivio
//! `Settings`). Stored as `<app_data>/kivio-code/config.json`, resolved via
//! [`settings_loader::app_data_dir`].
//!
//! Today this holds a single toggle: whether to read Claude-Code context files
//! (`CLAUDE.md` ancestors, `~/.claude/CLAUDE.md`, `<cwd>/.claude/CLAUDE.md`) for
//! cross-tool compatibility. Default is ON (like opencode reading legacy
//! `CLAUDE.md`). Missing or malformed config falls back to the default so the
//! CLI never fails to boot over a corrupt config file.

use serde::{Deserialize, Serialize};

use super::settings_loader;

/// kivio-code's own persisted config (not the shared Kivio `Settings`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KivioCodeConfig {
    /// Read Claude-Code context files (`CLAUDE.md` + `.claude/CLAUDE.md`) for
    /// compatibility. Default ON; the `/settings` command flips it.
    #[serde(default = "default_read_claude_dir")]
    pub read_claude_dir: bool,
}

fn default_read_claude_dir() -> bool {
    true
}

impl Default for KivioCodeConfig {
    fn default() -> Self {
        Self {
            read_claude_dir: default_read_claude_dir(),
        }
    }
}

/// Path to `<app_data>/kivio-code/config.json` (None when no home/app dir).
fn config_path() -> Option<std::path::PathBuf> {
    settings_loader::app_data_dir().map(|dir| dir.join("kivio-code").join("config.json"))
}

/// Load the persisted config. Missing/unreadable/malformed → [`KivioCodeConfig::default`]
/// (never panics, never errors).
pub fn load() -> KivioCodeConfig {
    let Some(path) = config_path() else {
        return KivioCodeConfig::default();
    };
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(_) => return KivioCodeConfig::default(),
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

/// Persist the config to `<app_data>/kivio-code/config.json`, creating the
/// directory if needed.
pub fn save(config: &KivioCodeConfig) -> Result<(), String> {
    let path = config_path().ok_or_else(|| "could not resolve app data directory".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_reads_claude_dir() {
        assert!(KivioCodeConfig::default().read_claude_dir);
    }

    #[test]
    fn missing_file_yields_default() {
        // load() reads the real app-data path; on a machine with no config file
        // it must return the default (read_claude_dir = true) rather than erroring.
        let cfg = load();
        // The value is whatever is persisted (default true if absent); the
        // contract is simply that it never panics and yields a valid struct.
        let _ = cfg.read_claude_dir;
    }

    #[test]
    fn deserialize_missing_field_defaults_true() {
        // An empty object (e.g. a forward/backward-compat config) must default the
        // toggle to true via serde default.
        let cfg: KivioCodeConfig = serde_json::from_str("{}").unwrap();
        assert!(cfg.read_claude_dir);
    }

    #[test]
    fn garbage_deserializes_to_default() {
        let cfg: KivioCodeConfig = serde_json::from_str("not json").unwrap_or_default();
        assert!(cfg.read_claude_dir);
    }

    #[test]
    fn roundtrip_serialize_deserialize() {
        let cfg = KivioCodeConfig {
            read_claude_dir: false,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: KivioCodeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, cfg);
        assert!(!back.read_claude_dir);
    }
}
