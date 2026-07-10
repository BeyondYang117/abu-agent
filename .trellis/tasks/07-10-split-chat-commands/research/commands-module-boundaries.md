# `chat/commands.rs` 模块边界调研

## 文件规模

- `src-tauri/src/chat/commands.rs`: 9,320 行，约 359 KB。
- 同目录已有较大模块：`sub_agent.rs`、`storage.rs`、`types.rs`、`attachments.rs` 等，说明项目接受按 chat 子域拆分。

## 主要职责区间（近似）

| 行区间 | 主要职责 | 后续候选模块 |
|---|---|---|
| 1–1018 | conversation/assistant/project/set 查询与 CRUD | `commands/conversations.rs`, `commands/assistants.rs`, `commands/projects.rs` |
| 1019–1745 | context 命令、取消/确认、外部发送、附件命令 | 按 context/session/attachments 分组 |
| 1746–3210 | assistant reply、fan-out、直接图片生成、skill 触发 | `reply_pipeline.rs` |
| 3211–4517 | token/context 估算、辅助视觉模型选择、压缩 | `context.rs` / `vision.rs` |
| 4518–5316 | 工具列表、API message 构建、图片/辅助视觉/MCP feedback | `tooling.rs` / `vision.rs` / `mcp_image_feedback.rs` |
| 5317–6244 | agent host、probe、provider 调用、sanitize、事件 | `agent_host.rs` / `provider_call.rs` |
| 6245–6893 | 图片 content part、消息修改/重生成/删除/分叉、标题 | `message_commands.rs` / `conversation_fork.rs` |
| 6895–9320 | 集中单元测试 | 随实现模块逐步下沉 |

## 首次提取候选分析

### 候选 A：完整 `attach_image_artifacts_for_model`

优点：一次减少约 200 行编排代码。

阻碍：它依赖仍在 `commands.rs` 的 conversation 加载、vision capability、辅助视觉模型选择、provider 调用和 session model。直接迁移会要求扩大多个私有类型/函数的可见性，或形成新模块反向依赖 `commands`。不适合作为第一刀。

### 候选 B：MCP 图片反馈纯辅助函数（选定）

优点：

- 无 IO、无网络、无状态。
- 依赖仅为 `base64`、`serde_json` 与 MCP 类型。
- 已有 7 个集中单测。
- `commands -> helper module` 单向依赖。
- 不修改 `mcp/registry.rs` 与对外路径。

代价：首次减少的行数较小，但验证成本最低，并为后续完整迁移建立命名和测试落点。

## 调用与可见性证据

- `mcp/registry.rs:471` 是 `attach_image_artifacts_for_model` 的唯一外部调用点。
- 四个纯 helper 的生产调用均位于 `attach_image_artifacts_for_model`；额外调用只存在于其单元测试。
- `data_url_image_part` 只服务于 MCP artifact，虽当前与 `image_content_part` 相邻，但没有其他调用者。
- `chat/mod.rs` 尚无同名模块或现成 helper 文件可复用。

## 相关契约

- `.trellis/spec/chat/mcp-image-feedback.md`
- `.trellis/spec/guides/code-reuse-thinking-guide.md`
- `.trellis/spec/guides/cross-layer-thinking-guide.md`

## 不应混入的事项

- 全量 Rust 测试的 10 个既有失败。
- `shortcuts.rs` / `capture_geometry.rs` / `plugins/install.rs` 的既有警告。
- MCP 图片反馈的功能修复、文案修改、上限调整或 provider 适配。