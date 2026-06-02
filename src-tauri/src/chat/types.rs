use serde::{Deserialize, Serialize};

/// 对话消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub role: String, // "user" | "assistant"
    pub content: String,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub reasoning: Option<String>,
    pub timestamp: i64,
}

/// 附件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    #[serde(rename = "type")]
    pub attachment_type: String, // "image" | "file"
    pub name: String,
    pub path: String, // 相对于对话附件目录的路径
}

/// 完整对话数据（存储在 conversations/{id}.json）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub provider_id: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub folder: Option<String>,
}

/// 对话列表项（index.json 中的元数据）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationListItem {
    pub id: String,
    pub title: String,
    pub preview: String, // 最后一条消息的前 100 字符
    pub provider_id: String,
    pub model: String,
    pub message_count: usize,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub folder: Option<String>,
}

/// 对话索引文件结构
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConversationIndex {
    pub conversations: Vec<ConversationListItem>,
}

impl From<&Conversation> for ConversationListItem {
    fn from(conv: &Conversation) -> Self {
        let preview = conv
            .messages
            .iter()
            .rev()
            .find(|m| m.role == "user" || m.role == "assistant")
            .map(|m| {
                let text = m.content.trim();
                if text.len() > 100 {
                    format!("{}...", &text[..100])
                } else {
                    text.to_string()
                }
            })
            .unwrap_or_default();

        ConversationListItem {
            id: conv.id.clone(),
            title: conv.title.clone(),
            preview,
            provider_id: conv.provider_id.clone(),
            model: conv.model.clone(),
            message_count: conv.messages.len(),
            created_at: conv.created_at,
            updated_at: conv.updated_at,
            pinned: conv.pinned,
            folder: conv.folder.clone(),
        }
    }
}
