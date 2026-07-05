# 执行计划：系统提示词对齐 opencode

## 顺序清单

### 步骤 1 — prepare.rs：新增"工作方式"纪律段（始终附加，双语）
- [ ] 在 `build_chat_system_prompt_with_segments` 基座/assistant/set 段之后、runtime datetime 之前,`append_context_segment(..., "system_prompt", "System prompt", <zh|en 文案>)`。
- [ ] 按 `language.starts_with("zh")` 选中英文案(design 变更 1 原文)。

### 步骤 2 — prepare.rs：native_tools 段追加"代码工作纪律"（门控）
- [ ] `native_tools_prompt()` 内新增 `has_bash`/`has_edit` 探测(仿 `has_write`)。
- [ ] 当 `has_write || has_edit || has_bash` 时,在返回 prompt 末尾追加 design 变更 2 的 zh/en 文案。
- [ ] 保持紧凑,拼进现有 format! 或 push_str。

### 步骤 3（可选）— settings.rs 基座措辞
- [ ] `default_chat_system_prompt` 四分支"直接、清晰"→"直接、简洁、清晰";英文加 "concisely"。

### 步骤 4 — 测试
- [ ] （可选轻量）在 prepare.rs test mod 加：含 write/bash 工具时 prompt 含代码纪律关键词;不含时不含。
- [ ] `cargo test --lib chat::agent::prepare` 全绿(经 win-cargo-test 机制)。

### 步骤 5 — 校验 + 实测
- [ ] `cargo check` 通过。
- [ ] 重启 dev（Rust 改动需重编），chat-probe 三条(简单事实 / 改代码小任务 / 报告类)对照 design 预期。

## 验证命令
```
cargo test --manifest-path src-tauri/Cargo.toml --lib chat::agent::prepare
# win: 经 scripts/win-cargo-test.ps1 机制放置 CC manifest 后运行
```

## 评审门
- **实现前**:design 里的双语文案需用户评审通过(这是给用户看的核心产物)。
- 步骤 1/2 后 cargo check 确认编译。
- 测试出现非基线新增失败即停。

## 不做
- 不改 Lens/翻译提示词、不改工具集/loop/模型层、不设死行数、不动 custom 覆盖逻辑。
