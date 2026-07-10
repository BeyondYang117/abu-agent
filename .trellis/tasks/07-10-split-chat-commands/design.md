# Design — 分阶段拆分 `chat/commands.rs`

## 1. 设计目标

本任务不一次性重写 9,320 行的 `chat/commands.rs`，而是选择可独立验证的边界分两刀提取：

1. 先迁移无 IO、无网络、无状态读写的 MCP 图片护栏 helper。
2. 再迁移完整视觉服务，并建立聊天与视觉共同依赖的 provider 调用模块。

每刀都要求行为中立、测试随职责移动、旧调用路径保持可用。

## 2. 第一刀：纯 MCP 图片反馈 helper

新增 `chat/mcp_image_feedback.rs`，仅承载：

- `select_image_artifacts_for_attach`
- `append_tool_result_note`
- `image_extension_for_mime`
- `data_url_image_part`
- 上述 helper 的 7 个单元测试和专用 fixture

该模块不读取设置、不加载会话、不发网络请求、不落临时文件，也不依赖 `commands`。

## 3. 第二刀：视觉服务与共享模型调用

### 3.1 `chat/vision.rs`

完整视觉职责迁入 `chat/vision.rs`：

- `AuxiliaryVisionModel` / `AuxiliaryVisionResult`
- 辅助视觉模型自动/显式选择
- mixer 工具记录创建与完成
- 辅助视觉 provider 调用与审查提示词
- 图片文件转模型 content part 与 MIME 映射
- `read_image_as_tool_result` 实现
- `attach_image_artifacts_for_model` 实现
- 3 个辅助视觉模型选择测试

`commands.rs` 中仍有回复编排需要调用部分视觉 helper，因此这些符号使用 `pub(super)`，只在 `chat` 模块树内部可见。

### 3.2 `chat/model_call.rs`

视觉分析必须复用聊天已有 provider dispatch。若 `vision.rs` 直接调用 `commands.rs::call_chat_completion_message`，会形成 `vision -> commands` 反向依赖，重新把视觉服务绑回巨型命令模块。

因此新增 `chat/model_call.rs`，承载：

- OpenAI Chat / Anthropic Messages / OpenAI Responses / Gemini dispatch
- OpenAI-compatible message 请求组装
- 缺少 API Key / 模型的稳定错误文本
- 从会话构造 `SessionModel`

`commands.rs` 和 `vision.rs` 都只向下依赖该共享模块。

## 4. 依赖方向

```text
mcp/native_registry.rs ─┐
                       ├─> commands (compatibility wrappers) ─> vision
mcp/registry.rs ───────┘                                  │
commands ─────────────────────────────────────────────────┼─> model_call
vision ───────────────────────────────────────────────────┤
                                                         ├─> storage
                                                         ├─> model_metadata
                                                         └─> mcp_image_feedback
```

禁止：

- `vision -> commands`
- `model_call -> commands`
- `mcp_image_feedback -> vision/commands`

## 5. 兼容入口与可见性

`chat/mod.rs` 以私有模块声明：

```rust
mod mcp_image_feedback;
mod model_call;
mod vision;
```

`commands.rs` 保留两个 crate 内可见入口：

```rust
pub(crate) async fn read_image_as_tool_result(...) {
    super::vision::read_image_as_tool_result(...).await
}

pub(crate) async fn attach_image_artifacts_for_model(...) {
    super::vision::attach_image_artifacts_for_model(...).await
}
```

这样 `mcp/native_registry.rs` 和 `mcp/registry.rs` 不需要修改，Tauri command 注册也不受影响。其余新符号使用 `pub(super)` 或私有可见性。

## 6. 行为保持清单

迁移前后必须逐项一致：

- 主模型/辅助视觉模型选择策略
- MCP 图片 8MB/4 张护栏
- read 图片 12MB 护栏
- 中英文辅助视觉审查提示词
- OCR 最终降级
- `kivio-mcpimg-<uuid>.<ext>` 临时文件命名与每条退出路径清理
- 主模型可看图时的 follow-up user message JSON 与顺序
- 成功喂图后清空 artifacts 的条件
- provider request shape、retry 次数和错误字符串
- MCP registry 调用路径与 Tauri 注册

## 7. 测试归属

- `chat/mcp_image_feedback.rs`：7 个纯 helper 测试。
- `chat/vision.rs`：3 个辅助视觉模型选择测试。
- `chat/commands.rs`：保留 `auxiliary_vision_result_becomes_text_for_main_chat_model`，因为它覆盖 commands 内主模型消息构造。

Windows 下使用 `scripts/win-cargo-test.ps1`，避免直接运行测试二进制时的环境问题。

## 8. 风险与控制

### 风险

- 移动函数时遗漏 import 或扩大可见性。
- 为复用 provider dispatch 引入 `vision -> commands` 循环耦合。
- 兼容入口被删除，迫使 MCP registry 同步改动。
- 全文件 rustfmt 制造大量与重构无关的格式 churn。
- 删除大段代码时误伤相邻编排或测试。

### 控制

- 用函数边界提取，不重写函数体。
- 搜索 `commands::` / `crate::chat::commands`，确认新模块无反向依赖。
- 搜索两个 MCP registry 调用点，确认旧路径仍在。
- 运行 3+7+1 个定向测试和 `cargo check`。
- 只对三个新模块执行定向 rustfmt，并运行 `git diff --check`。
- 最终审查完整 diff，确认只有移动、最小 import、薄代理和文档。

### 回滚

本任务无 schema、持久化或 wire-format 迁移。若门禁失败，可恢复 `commands.rs` / `mod.rs` / spec，并删除三个新模块；MCP registry 无需回滚。

## 9. 格式化策略

```powershell
rustfmt --edition 2021 --check --config skip_children=true `
  src-tauri/src/chat/mcp_image_feedback.rs `
  src-tauri/src/chat/model_call.rs `
  src-tauri/src/chat/vision.rs
```

禁止运行 `cargo fmt`，避免本机 rustfmt 对旧文件产生全仓无关变更。

## 10. 后续拆分路线

1. conversation/assistant/project/set CRUD commands。
2. 上下文估算与压缩入口。
3. 回复 fan-out / direct image generation。
4. 按所属模块继续下沉 `commands.rs` 巨型测试。
