# Implementation Plan — 首个安全提取

## Step 1 — 建立模块

- [x] 在 `src-tauri/src/chat/mod.rs` 声明私有 `mcp_image_feedback` 模块。
- [x] 新建 `src-tauri/src/chat/mcp_image_feedback.rs`。

## Step 2 — 移动纯实现

- [x] 原样移动 `select_image_artifacts_for_attach`。
- [x] 原样移动 `append_tool_result_note`。
- [x] 原样移动 `image_extension_for_mime`。
- [x] 原样移动 `data_url_image_part`。
- [x] 使用 `pub(super)` 控制可见性。
- [x] 在 `commands.rs` 添加最小 import，并删除原定义。

## Step 3 — 下沉测试

- [x] 移动 MCP 图片辅助测试和 `image_artifact` fixture helper。
- [x] 清理 `commands.rs` 测试模块中只为这些测试存在的引用。
- [x] 保持测试数据和断言语义不变。

## Step 4 — 验证

- [x] `rustfmt --edition 2021 --check --config skip_children=true src-tauri/src/chat/mcp_image_feedback.rs`
- [x] 运行 7 个 MCP 图片辅助单元测试（名称过滤或逐项过滤）。
- [x] `cargo check --manifest-path src-tauri/Cargo.toml`
- [x] `git diff --check`
- [x] 审查 diff，确认 `mcp/registry.rs`、公开入口和业务编排无变化。

## Step 5 — 记录下一刀

- [x] 根据本次结果记录完整 MCP 图片编排迁移需要先建立的视觉服务边界，不在本提交中实施。
## Step 6 — 建立共享模型调用边界

- [x] 新建 `chat/model_call.rs`。
- [x] 迁移 `call_chat_completion_message` 与 provider dispatch。
- [x] 迁移会话模型和稳定错误 helper。
- [x] 调整 `commands.rs` import，不改变调用参数与请求形态。

## Step 7 — 提取完整视觉服务

- [x] 新建 `chat/vision.rs`。
- [x] 迁移辅助视觉选择、工具记录、分析提示词、图片 content part 与 MIME helper。
- [x] 迁移 `read_image_as_tool_result` 和 MCP artifact 编排实现。
- [x] 在 `commands.rs` 保留原签名薄代理。
- [x] 确认 `vision.rs`、`model_call.rs` 不 import `commands`。

## Step 8 — 下沉测试并完成最终验证

- [x] 将 3 个辅助视觉模型选择测试迁到 `vision.rs`。
- [x] 保留 commands message builder 集成测试在原模块。
- [x] 对三个新模块执行定向 rustfmt 检查。
- [x] 运行 `chat::vision::tests`（3/3）。
- [x] 运行 `chat::mcp_image_feedback::tests`（7/7）。
- [x] 运行 `chat::commands::tests::auxiliary_vision_result_becomes_text_for_main_chat_model`（1/1）。
- [x] 运行 `cargo check --manifest-path src-tauri/Cargo.toml`。
- [x] 运行 `git diff --check`。
- [x] 检查 MCP registry 兼容入口和新模块依赖方向。

## Step 9 — 文档与最终审查

- [x] 更新 `verification.md`。
- [x] 更新 `.trellis/spec/chat/mcp-image-feedback.md` 中的实现位置和模块边界。
- [x] 审查完整 diff，确认仅包含模块移动、最小 import/代理和任务文档。
- [ ] 等用户确认后再决定是否 stage / commit。
