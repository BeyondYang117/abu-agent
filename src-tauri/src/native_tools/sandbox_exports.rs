use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};

use super::user_home_dir;
use crate::mcp::types::ChatToolArtifact;

const RUNS_ROOT: &str = "Kivio/runs";
const LEGACY_OUTPUTS_DIR: &str = "Kivio/outputs";
const DEFAULT_RETENTION_DAYS: u64 = 7;
const MAX_EXPORT_FILE_BYTES: u64 = 12 * 1024 * 1024;
const MAX_EXPORT_FILES_PER_RUN: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxExportContext {
    pub conversation_id: String,
    pub message_id: String,
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SandboxExportMetaFile {
    name: String,
    path: String,
    mime_type: String,
    size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SandboxExportMeta {
    conversation_id: String,
    message_id: String,
    tool_call_id: Option<String>,
    exported_at: i64,
    files: Vec<SandboxExportMetaFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxExportedArtifact {
    pub artifact_index: usize,
    pub path: PathBuf,
}

fn sanitize_path_segment(raw: &str) -> String {
    let sanitized = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn sandbox_run_export_dir(ctx: &SandboxExportContext) -> Result<PathBuf, String> {
    let home = user_home_dir()?;
    Ok(home
        .join(RUNS_ROOT)
        .join(sanitize_path_segment(&ctx.conversation_id))
        .join(sanitize_path_segment(&ctx.message_id)))
}

fn sandbox_exports_root() -> Result<PathBuf, String> {
    Ok(user_home_dir()?.join(RUNS_ROOT))
}

pub fn resolve_sandbox_export_file_path(path: &str) -> Result<PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Generated file path is empty".to_string());
    }
    let full = Path::new(trimmed);
    if !full.is_absolute() {
        return Err("Generated file path must be absolute".to_string());
    }
    if !full.is_file() {
        return Err("Generated file does not exist".to_string());
    }
    let canonical_path = fs::canonicalize(full)
        .map_err(|err| format!("Resolve generated file path failed: {err}"))?;
    let canonical_root = fs::canonicalize(sandbox_exports_root()?)
        .map_err(|err| format!("Resolve generated file root failed: {err}"))?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err("Generated file is outside the Kivio runs directory".to_string());
    }
    Ok(canonical_path)
}

fn decode_data_url(data_url: &str) -> Result<Vec<u8>, String> {
    let payload = data_url
        .split_once(',')
        .map(|(_, data)| data)
        .ok_or_else(|| "Invalid artifact data URL".to_string())?;
    general_purpose::STANDARD
        .decode(payload)
        .map_err(|err| format!("Decode artifact failed: {err}"))
}

fn sanitize_export_filename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("output");
    let sanitized = base
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let trimmed = sanitized.trim_matches(['.', '_']);
    if trimmed.is_empty() {
        "output.bin".to_string()
    } else {
        trimmed.to_string()
    }
}

fn unique_export_path(dir: &Path, filename: &str) -> PathBuf {
    let mut candidate = dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("output");
    let ext = Path::new(filename)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    for index in 2..=99 {
        candidate = dir.join(format!("{stem}-{index}{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}-{}", uuid::Uuid::new_v4()))
}

fn write_export_meta(dir: &Path, meta: &SandboxExportMeta) -> Result<(), String> {
    let json = serde_json::to_string_pretty(meta)
        .map_err(|err| format!("Serialize sandbox export meta failed: {err}"))?;
    fs::write(dir.join("meta.json"), json)
        .map_err(|err| format!("Write sandbox export meta failed: {err}"))
}

fn read_export_meta(dir: &Path) -> Option<SandboxExportMeta> {
    let content = fs::read_to_string(dir.join("meta.json")).ok()?;
    serde_json::from_str(&content).ok()
}

fn merged_export_meta(
    dir: &Path,
    ctx: &SandboxExportContext,
    next_files: Vec<SandboxExportMetaFile>,
) -> SandboxExportMeta {
    let mut files = read_export_meta(dir)
        .map(|meta| meta.files)
        .unwrap_or_default()
        .into_iter()
        .filter(|file| Path::new(&file.path).exists())
        .collect::<Vec<_>>();

    for next in next_files {
        files.retain(|file| file.path != next.path);
        files.push(next);
    }

    SandboxExportMeta {
        conversation_id: ctx.conversation_id.clone(),
        message_id: ctx.message_id.clone(),
        tool_call_id: ctx.tool_call_id.clone(),
        exported_at: chrono::Local::now().timestamp(),
        files,
    }
}

/// Export Pyodide artifacts for one tool run into `~/Kivio/runs/{conversation}/{message}/`.
pub fn export_sandbox_artifacts(
    ctx: &SandboxExportContext,
    artifacts: &[ChatToolArtifact],
) -> Result<Vec<SandboxExportedArtifact>, String> {
    if artifacts.is_empty() {
        return Ok(Vec::new());
    }

    let export_dir = sandbox_run_export_dir(ctx)?;
    fs::create_dir_all(&export_dir)
        .map_err(|err| format!("Create sandbox export dir failed: {err}"))?;

    let mut exported = Vec::new();
    let mut meta_files = Vec::new();

    for (artifact_index, artifact) in artifacts.iter().enumerate().take(MAX_EXPORT_FILES_PER_RUN) {
        if artifact.data_url.trim().is_empty() {
            continue;
        }
        let bytes = decode_data_url(&artifact.data_url)?;
        if bytes.is_empty() {
            continue;
        }
        if bytes.len() as u64 > MAX_EXPORT_FILE_BYTES {
            continue;
        }
        let filename = sanitize_export_filename(&artifact.name);
        let path = unique_export_path(&export_dir, &filename);
        fs::write(&path, &bytes).map_err(|err| format!("Write sandbox export failed: {err}"))?;
        meta_files.push(SandboxExportMetaFile {
            name: filename,
            path: path.display().to_string(),
            mime_type: artifact.mime_type.clone(),
            size_bytes: bytes.len() as u64,
        });
        exported.push(SandboxExportedArtifact {
            artifact_index,
            path,
        });
    }

    if !meta_files.is_empty() {
        let meta = merged_export_meta(&export_dir, ctx, meta_files);
        write_export_meta(&export_dir, &meta)?;
    }

    Ok(exported)
}

/// Guess a MIME type from a file extension. Used by `deliver_file` when the
/// caller omits `mime`. Falls back to `application/octet-stream`.
pub fn guess_mime_from_name(name: &str) -> String {
    let ext = Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    let mime = match ext.as_str() {
        "txt" | "log" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "csv" => "text/csv",
        "json" => "application/json",
        "html" | "htm" => "text/html",
        "xml" => "application/xml",
        "yaml" | "yml" => "application/x-yaml",
        "js" | "mjs" | "cjs" => "text/javascript",
        "ts" | "tsx" => "text/plain",
        "css" => "text/css",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "xls" => "application/vnd.ms-excel",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "doc" => "application/msword",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        _ => "application/octet-stream",
    };
    mime.to_string()
}

/// Deliver a finished file to the user as a downloadable artifact, writing it
/// into the same sandbox-exports tree that `run_python` uses (so it gets the
/// identical sanitize / path-guard / size-cap / retention/cleanup behavior and
/// renders via the same generic file card). `encoding` is `"text"` (default) or
/// `"base64"`. Returns a card-ready [`ChatToolArtifact`] with `path`,
/// `data_url`, `mime_type`, and `size_bytes` populated.
pub fn deliver_file_artifact(
    ctx: &SandboxExportContext,
    name: &str,
    content: &str,
    encoding: &str,
    mime: Option<&str>,
) -> Result<ChatToolArtifact, String> {
    let bytes = match encoding {
        "" | "text" => content.as_bytes().to_vec(),
        "base64" => {
            let trimmed: String = content.split_whitespace().collect();
            general_purpose::STANDARD
                .decode(trimmed.as_bytes())
                .map_err(|err| format!("deliver_file: invalid base64 content: {err}"))?
        }
        other => {
            return Err(format!(
                "deliver_file: unsupported encoding '{other}' (use 'text' or 'base64')"
            ));
        }
    };
    if bytes.is_empty() {
        return Err("deliver_file: content is empty".to_string());
    }
    if bytes.len() as u64 > MAX_EXPORT_FILE_BYTES {
        return Err(format!(
            "deliver_file: content exceeds the {} MB limit",
            MAX_EXPORT_FILE_BYTES / (1024 * 1024)
        ));
    }
    let mime_type = mime
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| guess_mime_from_name(name));

    // Reuse the exact export path/guard/cleanup machinery: build an in-memory
    // artifact (data_url), hand it to export_sandbox_artifacts (which sanitizes
    // the filename, enforces the size cap, writes under the runs tree, and
    // confines the write to the export dir), then read back the on-disk path.
    let data_url = format!(
        "data:{mime_type};base64,{}",
        general_purpose::STANDARD.encode(&bytes)
    );
    let mut artifact = ChatToolArtifact {
        name: name.to_string(),
        mime_type,
        data_url,
        size_bytes: Some(bytes.len() as u64),
        path: None,
    };
    let exported = export_sandbox_artifacts(ctx, std::slice::from_ref(&artifact))?;
    let written = exported
        .into_iter()
        .next()
        .ok_or_else(|| "deliver_file: nothing was written".to_string())?;
    artifact.path = Some(written.path.display().to_string());
    Ok(artifact)
}

pub fn format_exported_paths(exports: &[SandboxExportedArtifact]) -> String {
    if exports.is_empty() {
        return String::new();
    }
    let mut lines = vec![
        "exported files (~/Kivio/runs/<conversation>/<message>/; retained ~7 days):".to_string(),
    ];
    for export in exports {
        lines.push(format!("- {}", export.path.display()));
    }
    lines.join("\n")
}

pub fn format_export_error(err: &str) -> String {
    format!("export warning: failed to save sandbox files to ~/Kivio/runs/: {err}")
}

/// Remove all sandbox exports for one conversation (e.g. when the chat is deleted).
pub fn remove_sandbox_exports_for_conversation(conversation_id: &str) {
    let home = match user_home_dir() {
        Ok(home) => home,
        Err(_) => return,
    };
    let path = home
        .join(RUNS_ROOT)
        .join(sanitize_path_segment(conversation_id));
    if path.is_dir() {
        let _ = fs::remove_dir_all(path);
    }
}

fn dir_modified_at(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

fn remove_dir_if_stale(path: &Path, cutoff: SystemTime, removed: &mut u32, bytes_freed: &mut u64) {
    let modified = match dir_modified_at(path) {
        Some(value) => value,
        None => return,
    };
    if modified >= cutoff {
        return;
    }
    let size = dir_size(path);
    if fs::remove_dir_all(path).is_ok() {
        *removed += 1;
        *bytes_freed += size;
    }
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(_) => return 0,
    };
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            total += dir_size(&entry_path);
        } else if let Ok(meta) = entry.metadata() {
            total += meta.len();
        }
    }
    total
}

/// Remove sandbox run folders older than the default retention window. Also prunes legacy flat `outputs/`.
pub fn cleanup_stale_sandbox_exports() {
    let retention = Duration::from_secs(DEFAULT_RETENTION_DAYS * 24 * 60 * 60);
    let home = match user_home_dir() {
        Ok(home) => home,
        Err(err) => {
            eprintln!("[sandbox-export-cleanup] home dir unavailable: {err}");
            return;
        }
    };

    let mut removed = 0u32;
    let mut bytes_freed = 0u64;

    let runs_root = home.join(RUNS_ROOT);
    if runs_root.is_dir() {
        let cutoff = SystemTime::now()
            .checked_sub(retention)
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if let Ok(conversations) = fs::read_dir(&runs_root) {
            for conv_entry in conversations.flatten() {
                let conv_path = conv_entry.path();
                if !conv_path.is_dir() {
                    continue;
                }
                let Ok(messages) = fs::read_dir(&conv_path) else {
                    continue;
                };
                for msg_entry in messages.flatten() {
                    let msg_path = msg_entry.path();
                    if msg_path.is_dir() {
                        remove_dir_if_stale(&msg_path, cutoff, &mut removed, &mut bytes_freed);
                    }
                }
                if fs::read_dir(&conv_path)
                    .map(|mut dir| dir.next().is_none())
                    .unwrap_or(false)
                {
                    let _ = fs::remove_dir(&conv_path);
                }
            }
        }
    }

    let legacy_outputs = home.join(LEGACY_OUTPUTS_DIR);
    if legacy_outputs.is_dir() {
        let cutoff = SystemTime::now()
            .checked_sub(retention)
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if let Ok(entries) = fs::read_dir(&legacy_outputs) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    remove_dir_if_stale(&path, cutoff, &mut removed, &mut bytes_freed);
                    continue;
                }
                let modified = match fs::metadata(&path).and_then(|meta| meta.modified()) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                if modified >= cutoff {
                    continue;
                }
                let size = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
                if fs::remove_file(&path).is_ok() {
                    removed += 1;
                    bytes_freed += size;
                }
            }
        }
    }

    if removed > 0 {
        eprintln!(
            "[sandbox-export-cleanup] removed {removed} stale export folder(s), freed {} KB",
            bytes_freed / 1024
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_sandbox_artifacts_writes_under_runs_tree() {
        let png = general_purpose::STANDARD.encode([137u8, 80, 78, 71, 13, 10, 26, 10]);
        let artifacts = vec![ChatToolArtifact {
            name: "chart.png".to_string(),
            mime_type: "image/png".to_string(),
            data_url: format!("data:image/png;base64,{png}"),
            size_bytes: Some(8),
            path: None,
        }];
        let ctx = SandboxExportContext {
            conversation_id: "conv_test".to_string(),
            message_id: "msg_test".to_string(),
            tool_call_id: Some("call_test".to_string()),
        };
        let paths = export_sandbox_artifacts(&ctx, &artifacts).expect("export");
        assert_eq!(paths.len(), 1);
        assert!(paths[0].path.exists());
        assert_eq!(paths[0].artifact_index, 0);
        assert!(paths[0]
            .path
            .to_string_lossy()
            .contains("Kivio/runs/conv_test/msg_test/chart.png"));
        let meta_path = paths[0].path.parent().expect("parent").join("meta.json");
        assert!(meta_path.exists());
        let _ = fs::remove_dir_all(
            sandbox_run_export_dir(&ctx)
                .expect("dir")
                .parent()
                .expect("conv"),
        );
    }

    #[test]
    fn export_sandbox_artifacts_supports_csv() {
        let csv = general_purpose::STANDARD.encode(b"a,b\n1,2\n");
        let artifacts = vec![ChatToolArtifact {
            name: "summary.csv".to_string(),
            mime_type: "text/csv".to_string(),
            data_url: format!("data:text/csv;base64,{csv}"),
            size_bytes: Some(10),
            path: None,
        }];
        let ctx = SandboxExportContext {
            conversation_id: "conv_csv".to_string(),
            message_id: "msg_csv".to_string(),
            tool_call_id: None,
        };
        let paths = export_sandbox_artifacts(&ctx, &artifacts).expect("export csv");
        assert_eq!(paths.len(), 1);
        assert!(paths[0].path.to_string_lossy().ends_with("summary.csv"));
        let _ = fs::remove_dir_all(sandbox_run_export_dir(&ctx).expect("dir"));
    }

    #[test]
    fn resolve_sandbox_export_file_path_rejects_outside_paths() {
        let outside = std::env::temp_dir().join("kivio-outside-artifact.txt");
        fs::write(&outside, "outside").expect("write outside file");

        let err = resolve_sandbox_export_file_path(&outside.to_string_lossy())
            .expect_err("outside files must be rejected");
        assert!(err.contains("outside the Kivio runs directory"));

        let _ = fs::remove_file(outside);
    }

    #[test]
    fn deliver_file_writes_text_artifact() {
        let ctx = SandboxExportContext {
            conversation_id: "conv_deliver_text".to_string(),
            message_id: "msg_deliver_text".to_string(),
            tool_call_id: Some("call_text".to_string()),
        };
        let artifact = deliver_file_artifact(&ctx, "notes.md", "# Hello\n\nWorld\n", "text", None)
            .expect("deliver text");
        assert_eq!(artifact.name, "notes.md");
        assert_eq!(artifact.mime_type, "text/markdown");
        assert_eq!(artifact.size_bytes, Some("# Hello\n\nWorld\n".len() as u64));
        let path = artifact.path.as_ref().expect("path set");
        assert!(path.ends_with("notes.md"));
        assert!(path.contains("Kivio/runs/conv_deliver_text/msg_deliver_text"));
        let on_disk = fs::read_to_string(path).expect("read back");
        assert_eq!(on_disk, "# Hello\n\nWorld\n");
        assert!(artifact.data_url.starts_with("data:text/markdown;base64,"));
        let _ = fs::remove_dir_all(sandbox_run_export_dir(&ctx).expect("dir").parent().unwrap());
    }

    #[test]
    fn deliver_file_writes_base64_binary_artifact() {
        // 1x1 transparent PNG header bytes are enough to assert byte fidelity.
        let png_bytes: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
        let b64 = general_purpose::STANDARD.encode(png_bytes);
        let ctx = SandboxExportContext {
            conversation_id: "conv_deliver_bin".to_string(),
            message_id: "msg_deliver_bin".to_string(),
            tool_call_id: None,
        };
        let artifact = deliver_file_artifact(&ctx, "pixel.png", &b64, "base64", None)
            .expect("deliver base64");
        assert_eq!(artifact.mime_type, "image/png");
        let path = artifact.path.as_ref().expect("path set");
        let on_disk = fs::read(path).expect("read back");
        assert_eq!(on_disk, png_bytes);
        let _ = fs::remove_dir_all(sandbox_run_export_dir(&ctx).expect("dir").parent().unwrap());
    }

    #[test]
    fn deliver_file_sanitizes_filename_and_blocks_traversal() {
        let ctx = SandboxExportContext {
            conversation_id: "conv_deliver_sanitize".to_string(),
            message_id: "msg_deliver_sanitize".to_string(),
            tool_call_id: None,
        };
        // A traversal-style name is reduced to its file_name and sanitized, so
        // the write stays inside the message export dir.
        let artifact = deliver_file_artifact(&ctx, "../../etc/passwd", "x", "text", None)
            .expect("deliver sanitized");
        let path = artifact.path.as_ref().expect("path set");
        let export_dir = sandbox_run_export_dir(&ctx).expect("dir");
        assert!(Path::new(path).starts_with(&export_dir), "must stay under export dir");
        assert!(!path.contains(".."));
        // resolve_sandbox_export_file_path agrees the file is inside the runs root.
        assert!(resolve_sandbox_export_file_path(path).is_ok());
        let _ = fs::remove_dir_all(export_dir.parent().unwrap());
    }

    #[test]
    fn deliver_file_rejects_bad_base64() {
        let ctx = SandboxExportContext {
            conversation_id: "conv_deliver_badb64".to_string(),
            message_id: "msg_deliver_badb64".to_string(),
            tool_call_id: None,
        };
        let err = deliver_file_artifact(&ctx, "x.bin", "not*valid*base64!!!", "base64", None)
            .expect_err("bad base64 must error");
        assert!(err.contains("invalid base64"), "got: {err}");
    }

    #[test]
    fn deliver_file_rejects_oversize_content() {
        let ctx = SandboxExportContext {
            conversation_id: "conv_deliver_oversize".to_string(),
            message_id: "msg_deliver_oversize".to_string(),
            tool_call_id: None,
        };
        let huge = "a".repeat((MAX_EXPORT_FILE_BYTES + 1) as usize);
        let err = deliver_file_artifact(&ctx, "big.txt", &huge, "text", None)
            .expect_err("oversize must error");
        assert!(err.contains("exceeds"), "got: {err}");
    }

    #[test]
    fn deliver_file_rejects_unknown_encoding() {
        let ctx = SandboxExportContext {
            conversation_id: "conv_deliver_enc".to_string(),
            message_id: "msg_deliver_enc".to_string(),
            tool_call_id: None,
        };
        let err = deliver_file_artifact(&ctx, "x.txt", "data", "hex", None)
            .expect_err("unknown encoding must error");
        assert!(err.contains("unsupported encoding"), "got: {err}");
    }

    #[test]
    fn export_sandbox_artifacts_merges_meta_across_calls() {
        let first = general_purpose::STANDARD.encode(b"a,b\n1,2\n");
        let second = general_purpose::STANDARD.encode(b"# Report\n\nDone.\n");
        let ctx = SandboxExportContext {
            conversation_id: "conv_merge".to_string(),
            message_id: "msg_merge".to_string(),
            tool_call_id: Some("call_latest".to_string()),
        };

        let first_paths = export_sandbox_artifacts(
            &ctx,
            &[ChatToolArtifact {
                name: "summary.csv".to_string(),
                mime_type: "text/csv".to_string(),
                data_url: format!("data:text/csv;base64,{first}"),
                size_bytes: Some(8),
                path: None,
            }],
        )
        .expect("first export");
        let second_paths = export_sandbox_artifacts(
            &ctx,
            &[ChatToolArtifact {
                name: "report.md".to_string(),
                mime_type: "text/markdown".to_string(),
                data_url: format!("data:text/markdown;base64,{second}"),
                size_bytes: Some(16),
                path: None,
            }],
        )
        .expect("second export");

        let export_dir = sandbox_run_export_dir(&ctx).expect("dir");
        let meta = read_export_meta(&export_dir).expect("meta should deserialize");
        let meta_paths = meta.files.iter().map(|file| &file.path).collect::<Vec<_>>();

        assert_eq!(first_paths.len(), 1);
        assert_eq!(second_paths.len(), 1);
        assert_eq!(meta.files.len(), 2);
        assert!(meta_paths.contains(&&first_paths[0].path.display().to_string()));
        assert!(meta_paths.contains(&&second_paths[0].path.display().to_string()));

        let _ = fs::remove_dir_all(export_dir.parent().expect("conv"));
    }
}
