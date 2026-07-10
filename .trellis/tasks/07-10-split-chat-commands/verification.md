# Verification

## 执行结果

- `rustfmt --edition 2021 --check --config skip_children=true src-tauri/src/chat/mcp_image_feedback.rs src-tauri/src/chat/model_call.rs src-tauri/src/chat/vision.rs`：通过。
- `./scripts/win-cargo-test.ps1 --lib chat::vision::tests`：3/3 通过。
- `./scripts/win-cargo-test.ps1 --lib chat::mcp_image_feedback::tests`：7/7 通过。
- `./scripts/win-cargo-test.ps1 --lib chat::commands::tests::auxiliary_vision_result_becomes_text_for_main_chat_model`：1/1 通过。
- `cargo check --manifest-path src-tauri/Cargo.toml`：通过。
- `git diff --check`：通过。

## 变更边界审查

- `mcp/native_registry.rs` 未修改，仍调用 `crate::chat::commands::read_image_as_tool_result`。
- `mcp/registry.rs` 未修改，仍调用 `crate::chat::commands::attach_image_artifacts_for_model`。
- `commands.rs` 保留上述两个 `pub(crate)` 薄代理，实际实现迁入 `chat/vision.rs`。
- `vision.rs` 与 `model_call.rs` 均未引用 `commands`，不存在反向依赖。
- 最终依赖边界为：`commands -> model_call`、`commands -> vision`、`vision -> model_call/storage/mcp_image_feedback/model_metadata`。
- `commands.rs` 从 9,320 行降至 8,458 行；新增 `mcp_image_feedback.rs` 232 行、`model_call.rs` 94 行、`vision.rs` 672 行。
- 未修改视觉策略、图片护栏、提示词、OCR 降级、临时文件生命周期、provider request shape、MCP JSON、follow-up message 顺序或 Tauri 注册。

## 已知基线项

`cargo check` 仍报告既有警告：

- `src-tauri/src/shortcuts.rs` 未使用参数
- `src-tauri/src/capture_geometry.rs` dead code
- `src-tauri/src/plugins/install.rs` dead code

本任务未混入修复。

仓库当前整体并非由本机 rustfmt 版本完全格式化；因此仅对三个新模块执行定向 rustfmt，避免全仓无关格式 churn。
