use serde::{Deserialize, Serialize};

/// 工具调用状态（保存在 assistant message metadata 中）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Pending,
    Running,
    Success,
    Error,
    Cancelled,
    Skipped,
}

/// Chat 工具调用记录。字段保持 snake_case 存储，前端可同时兼容 camelCase 事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub server_id: Option<String>,
    #[serde(default)]
    pub arguments: String,
    pub status: ToolCallStatus,
    #[serde(default)]
    pub result_preview: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    pub completed_at: Option<i64>,
    #[serde(default)]
    pub round: u8,
    #[serde(default)]
    pub sensitive: bool,
}

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
    #[serde(default)]
    pub tool_calls: Vec<ToolCallRecord>,
    #[serde(default)]
    pub active_skill_id: Option<String>,
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
    #[serde(default)]
    pub active_skill_id: Option<String>,
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
