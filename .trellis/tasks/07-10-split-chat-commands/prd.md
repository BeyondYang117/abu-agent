# 分阶段拆分 `chat/commands.rs`：图片反馈与视觉服务边界

## Goal

在不改变聊天、MCP 工具调用、图片直喂或辅助视觉行为的前提下，分两刀降低 `src-tauri/src/chat/commands.rs` 的体积与职责密度：

1. 提取 MCP 图片反馈的纯护栏 helper 与单元测试。
2. 提取共享 provider 调用边界和完整视觉服务，同时保留既有 `chat::commands::*` 兼容入口。

## Background

- `src-tauri/src/chat/commands.rs` 原有 9,320 行，同时承担命令入口、回复编排、上下文估算/压缩、工具执行宿主、视觉处理、消息维护以及大量单元测试等职责。
- MCP 图片反馈能力由任务 `07-10-plugin-runtime-robustness` 引入，契约记录在 `.trellis/spec/chat/mcp-image-feedback.md`。
- `mcp/registry.rs` 调用 `chat::commands::attach_image_artifacts_for_model`，`mcp/native_registry.rs` 调用 `chat::commands::read_image_as_tool_result`；拆分时必须保持这两个路径和函数签名不变。
- 视觉服务需要复用聊天 provider dispatch。若直接从 `vision` 调用 `commands`，会形成反向依赖，因此先建立 `chat/model_call.rs` 共享边界。

## Requirements

### R1 — 提取纯辅助模块

新增 `src-tauri/src/chat/mcp_image_feedback.rs`，承载以下纯辅助函数及其专用测试：

- `select_image_artifacts_for_attach`
- `append_tool_result_note`
- `image_extension_for_mime`
- `data_url_image_part`

使用 `pub(super)` 或更窄可见性，不创建新的 crate 级公共 API。

### R2 — 提取共享模型调用边界

新增 `src-tauri/src/chat/model_call.rs`，承载聊天与视觉服务共同使用的 provider dispatch 和模型错误 helper：

- `call_chat_completion_message`
- `generate_with_chat_provider`
- `session_model_for_conversation`
- `format_chat_missing_api_key_error`
- `chat_missing_model_error`

provider request shape、retry 行为、错误文本和会话模型选择语义必须保持不变。

### R3 — 提取完整视觉服务

新增 `src-tauri/src/chat/vision.rs`，迁移辅助视觉模型选择、视觉工具记录、图片分析、图片 content part、`read_image_as_tool_result` 与 `attach_image_artifacts_for_model` 的实现及相关测试。

不得改变：

- 视觉模型选择策略
- 8MB/4 张 MCP 图片护栏与 12MB read 图片护栏
- 中英文审查提示词
- OCR 降级
- 临时文件创建、清理与启动 GC 前缀
- MCP tool result JSON 和 follow-up user message 顺序
- 已喂模型图片清空 artifacts 的条件

### R4 — 保持兼容入口

`chat/commands.rs` 保留薄代理：

- `pub(crate) async fn read_image_as_tool_result(...)`
- `pub(crate) async fn attach_image_artifacts_for_model(...)`

因此 `mcp/native_registry.rs`、`mcp/registry.rs` 与 Tauri command 注册无需修改。

### R5 — 测试随职责移动

- 7 个 MCP 图片纯 helper 测试迁到 `mcp_image_feedback.rs`。
- 3 个辅助视觉模型选择测试迁到 `vision.rs`。
- 仍依赖 `commands.rs` message builder 的 `auxiliary_vision_result_becomes_text_for_main_chat_model` 保留原位。
- 不删除或弱化现有断言。

### R6 — 单一职责重构

本任务只做模块提取、import/可见性调整、兼容代理和任务/spec 文档同步；不顺手修复既有 Rust 警告、业务问题或全仓格式。

## Technical Constraints

- 遵守 `.trellis/spec/chat/mcp-image-feedback.md`。
- 依赖方向必须保持为：

  ```text
  commands -> model_call
  commands -> vision -> model_call
                     -> storage
                     -> mcp_image_feedback
                     -> model_metadata
  ```

- `vision.rs` 与 `model_call.rs` 不得 import `commands`。
- 新模块默认私有，跨同级模块仅使用 `pub(super)`。
- 不改变任何序列化结构、事件、持久化数据、provider wire shape 或 Tauri command 注册。
- 只对新模块执行定向 rustfmt；禁止运行会产生全仓 churn 的 `cargo fmt`。

## Acceptance Criteria

- [x] AC1: `chat/mod.rs` 声明私有 `mcp_image_feedback`、`model_call`、`vision` 模块。
- [x] AC2: 四个纯 helper 及 7 个测试迁到 `mcp_image_feedback.rs`，行为保持不变。
- [x] AC3: 共享 provider dispatch 和错误 helper 迁到 `model_call.rs`。
- [x] AC4: 完整视觉实现和 3 个模型选择测试迁到 `vision.rs`。
- [x] AC5: `commands.rs` 仅保留两个旧路径薄代理；两个 MCP registry 文件无需修改。
- [x] AC6: `vision.rs`、`model_call.rs` 不反向依赖 `commands`。
- [x] AC7: 7 个纯 helper 测试、3 个视觉选择测试、1 个 commands 集成测试全部通过。
- [x] AC8: 三个新模块定向 rustfmt、`cargo check`、`git diff --check` 通过。
- [x] AC9: diff 不包含视觉策略、提示词、护栏、临时文件生命周期、provider wire shape 或 Tauri 注册变更。
- [x] AC10: `.trellis/spec/chat/mcp-image-feedback.md` 与最终实现位置同步。

## Out of Scope

- 拆分 conversation/assistant/project/set CRUD commands。
- 拆分上下文估算与压缩入口。
- 拆分回复 fan-out、direct image generation 或其余巨型测试模块。
- 修复 `shortcuts.rs`、`capture_geometry.rs`、`plugins/install.rs` 的既有警告。
- 全仓 rustfmt 或无关格式整理。
