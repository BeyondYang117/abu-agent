use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Manager};

use super::{Conversation, ConversationIndex, ConversationListItem};

const WRITE_RETRY_ATTEMPTS: usize = 3;

fn validate_conversation_id(id: &str) -> Result<(), String> {
    let valid = id.starts_with("conv_")
        && id.len() > "conv_".len()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if valid {
        Ok(())
    } else {
        Err(format!("Invalid conversation id: {id}"))
    }
}

fn atomic_write(path: &Path, content: &str, label: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{label} path has no parent"))?;
    fs::create_dir_all(parent).map_err(|e| format!("create {label} dir: {e}"))?;

    for attempt in 0..WRITE_RETRY_ATTEMPTS {
        let tmp_path = parent.join(format!(
            ".{}.tmp.{}",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("conversation"),
            attempt
        ));

        let write_result = fs::write(&tmp_path, content).and_then(|_| {
            fs::rename(&tmp_path, path).or_else(|_| {
                if path.exists() {
                    fs::remove_file(path)?;
                }
                fs::rename(&tmp_path, path)
            })
        });

        match write_result {
            Ok(()) => return Ok(()),
            Err(e) if attempt + 1 < WRITE_RETRY_ATTEMPTS => {
                let _ = fs::remove_file(&tmp_path);
                thread::sleep(Duration::from_millis(20 * (attempt as u64 + 1)));
                if e.kind() == ErrorKind::NotFound {
                    fs::create_dir_all(parent).map_err(|e| format!("create {label} dir: {e}"))?;
                }
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp_path);
                return Err(format!("write {label} file: {e}"));
            }
        }
    }

    Err(format!("write {label} file failed"))
}

fn read_conversation_file(path: &Path, id: &str) -> Result<Conversation, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("读取对话文件失败（{id}）：{e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("对话文件已损坏，无法加载（{id}）：{e}"))
}

fn load_conversation_list_from_files(app: &AppHandle) -> Result<Vec<ConversationListItem>, String> {
    let dir = conversations_dir(app)?;
    let entries = fs::read_dir(&dir).map_err(|e| format!("read conversations dir: {e}"))?;
    let mut conversations = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                eprintln!("skip unreadable conversation dir entry: {e}");
                continue;
            }
        };
        let path = entry.path();
        if path.file_name().and_then(|name| name.to_str()) == Some("index.json")
            || path.extension().and_then(|ext| ext.to_str()) != Some("json")
        {
            continue;
        }

        let id = match path.file_stem().and_then(|stem| stem.to_str()) {
            Some(id) if validate_conversation_id(id).is_ok() => id,
            _ => continue,
        };

        match read_conversation_file(&path, id) {
            Ok(conversation) => conversations.push(ConversationListItem::from(&conversation)),
            Err(e) => eprintln!("skip corrupt conversation file {id}: {e}"),
        }
    }

    Ok(conversations)
}

fn load_index_or_scan(app: &AppHandle) -> Result<ConversationIndex, String> {
    match load_index(app) {
        Ok(index) => Ok(index),
        Err(e) => {
            eprintln!("conversation index unavailable, rebuilding list from files: {e}");
            Ok(ConversationIndex {
                conversations: load_conversation_list_from_files(app)?,
            })
        }
    }
}

/// 获取对话存储根目录：{app_data_dir}/conversations/
pub fn conversations_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir unavailable: {e}"))?;
    let dir = base.join("conversations");
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| format!("create conversations dir: {e}"))?;
    }
    Ok(dir)
}

/// 获取对话索引文件路径
pub fn index_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(conversations_dir(app)?.join("index.json"))
}

/// 获取对话文件路径
pub fn conversation_file_path(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    validate_conversation_id(id)?;
    Ok(conversations_dir(app)?.join(format!("{}.json", id)))
}

/// 获取对话附件目录
#[allow(dead_code)]
pub fn conversation_attachments_dir(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    validate_conversation_id(id)?;
    let dir = conversations_dir(app)?.join(format!("{}_attachments", id));
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| format!("create attachments dir: {e}"))?;
    }
    Ok(dir)
}

/// 加载对话索引
pub fn load_index(app: &AppHandle) -> Result<ConversationIndex, String> {
    let path = index_file_path(app)?;
    if !path.exists() {
        return Ok(ConversationIndex::default());
    }

    let content = fs::read_to_string(&path).map_err(|e| format!("read index file: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("parse index file: {e}"))
}

/// 保存对话索引
pub fn save_index(app: &AppHandle, index: &ConversationIndex) -> Result<(), String> {
    let path = index_file_path(app)?;
    let content =
        serde_json::to_string_pretty(index).map_err(|e| format!("serialize index: {e}"))?;
    atomic_write(&path, &content, "index")
}

/// 加载对话详情
pub fn load_conversation(app: &AppHandle, id: &str) -> Result<Conversation, String> {
    let path = conversation_file_path(app, id)?;
    if !path.exists() {
        return Err(format!("对话不存在：{id}"));
    }

    read_conversation_file(&path, id)
}

/// 保存对话详情
pub fn save_conversation(app: &AppHandle, conversation: &Conversation) -> Result<(), String> {
    let path = conversation_file_path(app, &conversation.id)?;
    let content = serde_json::to_string_pretty(conversation)
        .map_err(|e| format!("serialize conversation: {e}"))?;
    atomic_write(&path, &content, "conversation")?;

    // 更新索引
    let mut index = load_index_or_scan(app)?;
    let list_item = ConversationListItem::from(conversation);

    if let Some(pos) = index
        .conversations
        .iter()
        .position(|c| c.id == conversation.id)
    {
        index.conversations[pos] = list_item;
    } else {
        index.conversations.insert(0, list_item);
    }

    save_index(app, &index)
}

/// 删除对话
pub fn delete_conversation(app: &AppHandle, id: &str) -> Result<(), String> {
    // 删除对话文件
    let path = conversation_file_path(app, id)?;
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("delete conversation file: {e}"))?;
    }

    // 删除附件目录
    let attachments_dir = conversations_dir(app)?.join(format!("{}_attachments", id));
    if attachments_dir.exists() {
        fs::remove_dir_all(&attachments_dir).map_err(|e| format!("delete attachments dir: {e}"))?;
    }

    // 更新索引
    let mut index = load_index_or_scan(app)?;
    index.conversations.retain(|c| c.id != id);
    save_index(app, &index)
}

/// 获取对话列表（分页）
pub fn get_conversations(
    app: &AppHandle,
    offset: usize,
    limit: usize,
    folder: Option<String>,
) -> Result<Vec<ConversationListItem>, String> {
    let mut index = load_index_or_scan(app)?;

    // 按 folder 筛选
    if let Some(folder_name) = folder {
        index
            .conversations
            .retain(|c| c.folder.as_deref() == Some(&folder_name));
    }

    // 按 updated_at 倒序排序（最新的在前）
    index
        .conversations
        .sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    // 分页
    if offset >= index.conversations.len() {
        return Ok(vec![]);
    }
    let end = (offset + limit).min(index.conversations.len());
    Ok(index.conversations[offset..end].to_vec())
}
