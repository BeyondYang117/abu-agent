use std::{
    fs,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose, Engine as _};
use tauri::AppHandle;
use uuid::Uuid;

use super::storage::conversation_attachments_dir;
use super::Attachment;

const MAX_ATTACHMENT_PREVIEW_BYTES: u64 = 12 * 1024 * 1024;
const MAX_PASTED_IMAGE_BYTES: usize = 12 * 1024 * 1024;
const MAX_PASTED_ATTACHMENT_BYTES: usize = 25 * 1024 * 1024;

pub(crate) enum PastedImageSave {
    Saved {
        path: PathBuf,
        name: String,
        mime_type: &'static str,
    },
    Failed {
        error: String,
    },
}

pub(crate) enum PastedAttachmentSave {
    Saved { path: PathBuf, name: String },
    Failed { error: String },
}

pub(crate) fn save_pasted_image(
    name: &str,
    mime_type: &str,
    data_base64: &str,
) -> Result<PastedImageSave, String> {
    let mime = normalize_pasted_image_mime(mime_type)?;
    let ext = extension_for_image_mime(mime);
    let mut safe_name = sanitize_attachment_name(name);
    if attachment_type_for_name(&safe_name) != "image" {
        safe_name = format!("{safe_name}.{ext}");
    }

    let payload = data_base64.trim();
    if payload.is_empty() {
        return Ok(PastedImageSave::Failed {
            error: "剪贴板图片为空".to_string(),
        });
    }

    let bytes = match general_purpose::STANDARD.decode(payload) {
        Ok(bytes) => bytes,
        Err(err) => {
            return Ok(PastedImageSave::Failed {
                error: format!("解析剪贴板图片失败: {err}"),
            });
        }
    };
    if bytes.len() > MAX_PASTED_IMAGE_BYTES {
        return Ok(PastedImageSave::Failed {
            error: "剪贴板图片过大，无法添加".to_string(),
        });
    }

    let (path, saved_name) = write_pasted_attachment_bytes(&safe_name, &bytes)
        .map_err(|e| format!("保存剪贴板图片失败: {e}"))?;

    Ok(PastedImageSave::Saved {
        path,
        name: saved_name,
        mime_type: mime,
    })
}

pub(crate) fn save_pasted_attachment(
    name: &str,
    data_base64: &str,
) -> Result<PastedAttachmentSave, String> {
    let safe_name = sanitize_attachment_name(name);
    if !is_attachable_file_name(&safe_name) {
        return Ok(PastedAttachmentSave::Failed {
            error: "无效的文件名".to_string(),
        });
    }

    let payload = data_base64.trim();
    if payload.is_empty() {
        return Ok(PastedAttachmentSave::Failed {
            error: "剪贴板附件为空".to_string(),
        });
    }

    let bytes = match general_purpose::STANDARD.decode(payload) {
        Ok(bytes) => bytes,
        Err(err) => {
            return Ok(PastedAttachmentSave::Failed {
                error: format!("解析剪贴板附件失败: {err}"),
            });
        }
    };
    if bytes.len() > MAX_PASTED_ATTACHMENT_BYTES {
        return Ok(PastedAttachmentSave::Failed {
            error: "剪贴板附件过大，无法添加".to_string(),
        });
    }

    let (path, saved_name) = write_pasted_attachment_bytes(&safe_name, &bytes)?;
    Ok(PastedAttachmentSave::Saved {
        path,
        name: saved_name,
    })
}

fn write_pasted_attachment_bytes(name: &str, bytes: &[u8]) -> Result<(PathBuf, String), String> {
    let dir = std::env::temp_dir().join("kivio-chat-paste");
    fs::create_dir_all(&dir).map_err(|e| format!("创建临时附件目录失败: {e}"))?;
    let file_name = format!("paste-{}-{}", Uuid::new_v4(), name);
    let path = dir.join(&file_name);
    fs::write(&path, bytes).map_err(|e| format!("保存剪贴板附件失败: {e}"))?;
    Ok((path, name.to_string()))
}

pub(crate) fn is_attachable_file_name(name: &str) -> bool {
    !name.trim().is_empty()
}

pub(crate) fn resolve_attachment_file_path(
    app: &AppHandle,
    conversation_id: Option<&str>,
    path: &str,
) -> Result<PathBuf, String> {
    if path.trim().is_empty() {
        return Err("附件路径为空".to_string());
    }

    if let Some(conversation_id) = conversation_id {
        if path.contains('/') || path.contains('\\') {
            return Err("无效的附件路径".to_string());
        }
        let dir = conversation_attachments_dir(app, conversation_id)?;
        let full = dir.join(path);
        if !full.is_file() {
            return Err(format!("附件不存在: {path}"));
        }
        return Ok(full);
    }

    let full = PathBuf::from(path);
    if !full.is_file() {
        return Err(format!("文件不存在: {path}"));
    }
    Ok(full)
}

fn normalize_pasted_image_mime(mime_type: &str) -> Result<&'static str, String> {
    match mime_type.trim().to_ascii_lowercase().as_str() {
        "image/png" => Ok("image/png"),
        "image/jpeg" | "image/jpg" => Ok("image/jpeg"),
        "image/gif" => Ok("image/gif"),
        "image/webp" => Ok("image/webp"),
        "image/bmp" => Ok("image/bmp"),
        "image/tiff" => Ok("image/tiff"),
        "image/heic" => Ok("image/heic"),
        "image/heif" => Ok("image/heif"),
        _ => Err("仅支持粘贴图片".to_string()),
    }
}

fn extension_for_image_mime(mime_type: &str) -> &'static str {
    match mime_type {
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        "image/tiff" => "tiff",
        "image/heic" => "heic",
        "image/heif" => "heif",
        _ => "png",
    }
}

fn mime_type_for_attachment(name: &str) -> &'static str {
    let ext = Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "xlsm" => "application/vnd.ms-excel.sheet.macroenabled.12",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "txt" => "text/plain",
        "md" => "text/markdown",
        _ => "application/octet-stream",
    }
}

pub(crate) fn read_attachment_as_data_url(path: &Path) -> Result<String, String> {
    let metadata = fs::metadata(path).map_err(|e| format!("读取附件信息失败: {e}"))?;
    if metadata.len() > MAX_ATTACHMENT_PREVIEW_BYTES {
        return Err("附件过大，无法在界面内预览".to_string());
    }
    let bytes = fs::read(path).map_err(|e| format!("读取附件失败: {e}"))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("attachment");
    let mime = mime_type_for_attachment(file_name);
    let encoded = general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:{mime};base64,{encoded}"))
}

pub(crate) fn save_message_attachments(
    app: &AppHandle,
    conversation_id: &str,
    attachment_paths: Vec<String>,
) -> Result<Vec<Attachment>, String> {
    let mut attachments = Vec::new();
    if attachment_paths.is_empty() {
        return Ok(attachments);
    }

    let dir = conversation_attachments_dir(app, conversation_id)?;
    for source in attachment_paths {
        let source_path = Path::new(&source);
        if !source_path.is_file() {
            return Err(format!("附件不存在或不是文件: {source}"));
        }

        let id = format!("att_{}", Uuid::new_v4());
        let original_name = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("attachment");
        let safe_name = sanitize_attachment_name(original_name);
        let stored_name = format!("{}-{}", id, safe_name);
        let dest = dir.join(&stored_name);
        fs::copy(source_path, &dest).map_err(|e| format!("保存附件失败: {e}"))?;

        attachments.push(Attachment {
            id,
            attachment_type: attachment_type_for_name(original_name).to_string(),
            name: original_name.to_string(),
            path: stored_name,
        });
    }

    Ok(attachments)
}

fn sanitize_attachment_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches(['.', ' ', '_']).trim();
    if trimmed.is_empty() {
        "attachment".to_string()
    } else {
        trimmed.to_string()
    }
}

fn attachment_type_for_name(name: &str) -> &'static str {
    let ext = Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tiff" | "tif" | "heic" | "heif" => {
            "image"
        }
        _ => "file",
    }
}

fn attachment_type_label(attachment_type: &str) -> &'static str {
    match attachment_type {
        "image" => "图片",
        _ => "文件",
    }
}

fn attachment_extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

fn attachment_skill_for_name(name: &str) -> Option<&'static str> {
    match attachment_extension(name).as_str() {
        "pdf" => Some("pdf"),
        "doc" | "docx" => Some("docx"),
        "xls" | "xlsx" | "xlsm" | "csv" | "tsv" => Some("xlsx"),
        _ => None,
    }
}

fn attachment_format_label(attachment: &Attachment) -> &'static str {
    if attachment.attachment_type == "image" {
        return "图片";
    }

    match attachment_extension(&attachment.name).as_str() {
        "pdf" => "PDF",
        "doc" | "docx" => "Word 文档",
        "xls" | "xlsx" | "xlsm" => "Excel 工作簿",
        "csv" => "CSV 表格",
        "tsv" => "TSV 表格",
        "txt" | "md" => "文本文件",
        _ => attachment_type_label(&attachment.attachment_type),
    }
}

fn stored_attachment_path_for_prompt(
    attachment: &Attachment,
    attachment_dir: Option<&Path>,
) -> String {
    attachment_dir
        .map(|dir| dir.join(&attachment.path).display().to_string())
        .unwrap_or_else(|| attachment.path.clone())
}

fn attachment_processing_hint(attachment: &Attachment) -> String {
    if attachment.attachment_type == "image" {
        return "图片附件会随本轮请求发送给视觉模型。".to_string();
    }

    if let Some(skill) = attachment_skill_for_name(&attachment.name) {
        format!(
            "推荐复用现成 `{skill}` Skill：需要读取或分析该文件时，先调用 skill_activate(name=\"{skill}\")，再按该 Skill 的 SKILL.md / reference / scripts 流程处理安全副本路径。"
        )
    } else {
        "此文件已保存为 Kivio 安全副本；仅在有可用读取工具或对应 Skill 时处理正文。".to_string()
    }
}

pub(crate) fn compose_user_content_for_api(
    content: &str,
    attachments: &[Attachment],
    attachment_dir: Option<&Path>,
) -> String {
    let trimmed = content.trim();
    if attachments.is_empty() {
        return trimmed.to_string();
    }

    let has_images = attachments
        .iter()
        .any(|attachment| attachment.attachment_type == "image");
    let has_files = attachments
        .iter()
        .any(|attachment| attachment.attachment_type != "image");
    let attachment_lines = attachments
        .iter()
        .map(|attachment| {
            let stored_path = stored_attachment_path_for_prompt(attachment, attachment_dir);
            format!(
                "- {} ({})\n  - 附件 ID：{}\n  - Kivio 安全副本路径：{}\n  - 处理建议：{}",
                attachment.name,
                attachment_format_label(attachment),
                attachment.id,
                stored_path,
                attachment_processing_hint(attachment)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let capability_note = match (has_images, has_files) {
        (true, true) => {
            "图片附件会随本轮请求发送给视觉模型；文档/表格附件不会直接随模型请求内联正文，必须复用对应 Agent Skill 或可用工具实际读取安全副本后再分析。"
        }
        (true, false) => "图片附件会随本轮请求发送给视觉模型。",
        (false, true) => {
            "文档/表格附件不会直接随模型请求内联正文，必须复用对应 Agent Skill 或可用工具实际读取安全副本后再分析；不要仅凭文件名臆测内容。"
        }
        (false, false) => "",
    };
    let attachment_note = format!(
        "[已添加附件]\n{}\n\n注意：{}",
        attachment_lines, capability_note
    );

    if trimmed.is_empty() {
        attachment_note
    } else {
        format!("{trimmed}\n\n{attachment_note}")
    }
}

pub(crate) fn title_source_for_user_message(content: &str, attachments: &[Attachment]) -> String {
    let trimmed = content.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }

    let names = attachments
        .iter()
        .map(|attachment| attachment.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if names.is_empty() {
        "新对话".to_string()
    } else {
        format!("附件: {names}")
    }
}

pub(crate) fn stored_image_paths_for_attachments(
    app: &AppHandle,
    conversation_id: &str,
    attachments: &[Attachment],
) -> Result<Vec<PathBuf>, String> {
    let image_attachments = attachments
        .iter()
        .filter(|attachment| attachment.attachment_type == "image")
        .collect::<Vec<_>>();
    if image_attachments.is_empty() {
        return Ok(Vec::new());
    }

    let dir = conversation_attachments_dir(app, conversation_id)?;
    image_attachments
        .into_iter()
        .map(|attachment| {
            let stored = Path::new(&attachment.path);
            if stored.components().count() != 1 {
                return Err(format!("Invalid attachment path: {}", attachment.path));
            }
            let path = dir.join(stored);
            if !path.is_file() {
                return Err(format!("图片附件不存在: {}", attachment.name));
            }
            Ok(path)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn attachment_type_detects_images_case_insensitively() {
        assert_eq!(attachment_type_for_name("screenshot.PNG"), "image");
        assert_eq!(attachment_type_for_name("scan.tif"), "image");
        assert_eq!(attachment_type_for_name("photo.heic"), "image");
        assert_eq!(attachment_type_for_name("notes.pdf"), "file");
    }

    #[test]
    fn attachable_file_names_accept_any_non_empty_name() {
        assert!(is_attachable_file_name("notes.pdf"));
        assert!(is_attachable_file_name("sheet.xlsx"));
        assert!(is_attachable_file_name("archive.zip"));
        assert!(is_attachable_file_name("main.rs"));
        assert!(!is_attachable_file_name("   "));
    }

    #[test]
    fn sanitize_attachment_name_removes_path_like_characters() {
        assert_eq!(sanitize_attachment_name("../secret?.png"), "secret_.png");
        assert_eq!(sanitize_attachment_name("   "), "attachment");
    }

    #[test]
    fn compose_user_content_for_api_mentions_attachment_names() {
        let content = compose_user_content_for_api(
            "看看这个",
            &[Attachment {
                id: "att_1".to_string(),
                attachment_type: "image".to_string(),
                name: "screen.png".to_string(),
                path: "att_1-screen.png".to_string(),
            }],
            None,
        );

        assert!(content.contains("看看这个"));
        assert!(content.contains("screen.png"));
        assert!(content.contains("图片附件会随本轮请求发送给视觉模型"));
    }

    #[test]
    fn compose_user_content_for_api_recommends_document_skill() {
        let content = compose_user_content_for_api(
            "总结一下",
            &[Attachment {
                id: "att_1".to_string(),
                attachment_type: "file".to_string(),
                name: "report.PDF".to_string(),
                path: "att_1-report.PDF".to_string(),
            }],
            Some(Path::new("/Users/test/Library/Application Support/com.zmair.kivio/conversations/conv_1_attachments")),
        );

        assert!(content.contains("report.PDF"));
        assert!(content.contains("PDF"));
        assert!(content.contains("skill_activate(name=\"pdf\")"));
        assert!(content.contains("Kivio 安全副本路径"));
        assert!(content.contains("不要仅凭文件名臆测内容"));
    }

    #[test]
    fn title_source_uses_attachment_name_when_content_empty() {
        let title = title_source_for_user_message(
            "",
            &[Attachment {
                id: "att_1".to_string(),
                attachment_type: "file".to_string(),
                name: "notes.pdf".to_string(),
                path: "att_1-notes.pdf".to_string(),
            }],
        );

        assert_eq!(title, "附件: notes.pdf");
    }
}
