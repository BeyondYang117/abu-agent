use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

use crate::chat::attachments::{
    is_attachable_file_name, read_attachment_as_data_url, resolve_attachment_file_path,
    save_pasted_attachment, save_pasted_image, PastedAttachmentSave, PastedImageSave,
};

/// 读取附件为 data URL，供前端 `<img>` 预览。`conversation_id` 为空时按本机绝对路径读取（发送前预览）。
#[tauri::command]
pub(crate) fn chat_read_attachment(
    app: AppHandle,
    conversation_id: Option<String>,
    path: String,
) -> Result<serde_json::Value, String> {
    let full = resolve_attachment_file_path(&app, conversation_id.as_deref(), &path)?;
    let data_url = read_attachment_as_data_url(&full)?;
    Ok(serde_json::json!({
        "success": true,
        "data": data_url,
    }))
}

/// 用系统默认应用打开附件。
#[tauri::command]
#[allow(deprecated)]
pub(crate) fn chat_open_attachment(
    app: AppHandle,
    conversation_id: Option<String>,
    path: String,
) -> Result<(), String> {
    let full = resolve_attachment_file_path(&app, conversation_id.as_deref(), &path)?;
    let path_str = full.to_string_lossy().into_owned();
    app.shell().open(path_str, None).map_err(|e| e.to_string())
}

/// 用系统默认应用打开生成产物文件。仅允许打开 Kivio sandbox export 目录下的文件。
#[tauri::command]
#[allow(deprecated)]
pub(crate) fn chat_open_generated_artifact(app: AppHandle, path: String) -> Result<(), String> {
    let full = crate::native_tools::resolve_sandbox_export_file_path(&path)?;
    let path_str = full.to_string_lossy().into_owned();
    app.shell().open(path_str, None).map_err(|e| e.to_string())
}

/// 在文件系统中打开生成产物所在目录。仅允许 Kivio sandbox export 目录下的文件。
#[tauri::command]
#[allow(deprecated)]
pub(crate) fn chat_reveal_generated_artifact(app: AppHandle, path: String) -> Result<(), String> {
    let full = crate::native_tools::resolve_sandbox_export_file_path(&path)?;
    let parent = full
        .parent()
        .ok_or_else(|| "Generated file has no parent directory".to_string())?;
    let path_str = parent.to_string_lossy().into_owned();
    app.shell().open(path_str, None).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn chat_save_pasted_image(
    name: String,
    mime_type: String,
    data_base64: String,
) -> Result<serde_json::Value, String> {
    match save_pasted_image(&name, &mime_type, &data_base64)? {
        PastedImageSave::Saved {
            path,
            name,
            mime_type,
        } => Ok(serde_json::json!({
            "success": true,
            "path": path.to_string_lossy(),
            "name": name,
            "mimeType": mime_type,
        })),
        PastedImageSave::Failed { error } => Ok(serde_json::json!({
            "success": false,
            "error": error,
        })),
    }
}

#[tauri::command]
pub(crate) fn chat_save_pasted_attachment(
    name: String,
    data_base64: String,
) -> Result<serde_json::Value, String> {
    match save_pasted_attachment(&name, &data_base64)? {
        PastedAttachmentSave::Saved { path, name } => Ok(serde_json::json!({
            "success": true,
            "path": path.to_string_lossy(),
            "name": name,
        })),
        PastedAttachmentSave::Failed { error } => Ok(serde_json::json!({
            "success": false,
            "error": error,
        })),
    }
}

/// 读取系统剪贴板中的文件路径（Finder / 资源管理器复制文件）。
#[tauri::command]
pub(crate) fn chat_read_clipboard_files() -> Result<serde_json::Value, String> {
    use arboard::Clipboard;

    let mut clipboard = Clipboard::new().map_err(|e| format!("读取剪贴板失败: {e}"))?;
    let paths = match clipboard.get().file_list() {
        Ok(paths) => paths,
        Err(_) => {
            return Ok(serde_json::json!({
                "success": true,
                "files": [],
            }));
        }
    };

    let files: Vec<Value> = paths
        .into_iter()
        .filter(|path| path.is_file())
        .filter_map(|path| {
            let name = path.file_name()?.to_string_lossy().to_string();
            if !is_attachable_file_name(&name) {
                return None;
            }
            Some(serde_json::json!({
                "path": path.to_string_lossy(),
                "name": name,
            }))
        })
        .collect();

    Ok(serde_json::json!({
        "success": true,
        "files": files,
    }))
}
