# Design — 死代码清理

## 方法

审计已逐项 grep 验证。本任务不做新设计，只按「安全删除」原则执行：删除零调用方项、合并逐字重复项。每类删除都以 lint/typecheck/test/build 作为回归护栏。

批次划分原则：**同语言、同护栏工具、可独立回滚**。分 5 批，每批一个 commit。

---

## 批次 1 — 前端确证死代码（护栏：lint + typecheck + test）

### 1a. 整文件/大块删除
- `src/App.css`（45）— Vite 模板残留，零导入。
- `src/chat/SkillSelector.tsx`（226）— 全仓零导入。
- `scripts/chat-katex-perf-smoke.mjs`（701）+ `scripts/chat_skill_e2e.py`（429）— 孤儿脚本。

### 1b. 死 i18n key（~162 行 × 逐 key 验证）
- `src/settings/i18n.ts`：`shot*` 全家、`contextStatus*`(6)、`onboardingWelcomeScreenshot*`/`onboardingLens*`/`onboardingLanguage*`(20)、`lensHotkey`、`lensTranslating`、`chatStreamHint`、`visionModel`、`visionOpenai`、`engineAI`、`registeredModels`、`noPermissionNeeded`、`connectorsOauthSoon` 等。**删前对每个 key 再跑一次 bare-substring grep 确认零命中。**

### 1c. 零调用方导出/组件/字段
- `src/settings/components.tsx`：`SectionTitle`、`TabButton`（零导入）；`Input` 的 `list` prop（无人传）。
- `src/lens/layout.ts`：`cubicBezier`（仅自测试）+ 其 test describe。
- `src/data/modelMatching.ts`：`hasModelInfo`（仅自测试）+ 其 test。
- `src/chat/utils.ts`：`groupConversationsByTime` + `src/chat/types.ts` `ConversationGroup` 类型（仅自测试）。
- `src/chat/multiAnswerViewMode.ts`：`_resetMultiAnswerViewModeForTest`/`_setMultiAnswerViewModeForTest` — 改测试用 localStorage seed。
- `src/chat/persistence.ts`：`getChatPlatformWindowSize`（恒等函数，4 调用点改用实参）、`getRememberedChatSize`（零调用）。
- `src/chat/ConversationList.tsx`：不可达空态分支 + `emptyLabel` prop。
- `src/chat/TypewriterText.tsx`：`charDelayMs`/`startDelayMs`/`className`/`resetKey` prop（改用原生 `key=`）。
- `src/chat/approvalPolicies.ts`：`approvalPolicyOption`（零调用）+ 更新过期注释。
- `src/chat/toolStatus.ts`：`isToolCallErrorStatus`（仅自测试）。
- `src/chat/ConversationContextMenu.tsx`：死 prop `conversationTitle`。
- `src/chat/ToolCallBlock.tsx`：`labels` prop + `defaultLabels` 合并（3 调用点全不传）→ 直用常量。
- `src/api/tauri.ts`：`maxToolOutputChars`（normalize 无条件抹 null 的死旋钮）；`src/onboarding/types.ts` `OnboardingStepId` 死 `'language'` 成员。

### 1d. 近重复合并（shrink）
- `isTauriRuntime` 5 处定义 → 收敛到一个共享 util（tauri.ts 或 utils），全部 import。含 `src/chat/utils.ts`、`src/chat/knowledgeBase.ts`、App.tsx、Chat.tsx 内联。
- `formatTime`/`formatDuration`（RequestDebugPanel↔UsageStatsPanel 逐字重复）+ `formatTokens`（重造 `src/utils/tokens.ts`）→ 共享。
- provider-usable 逻辑三份（tauri.ts `providerHasUsableConfig`+`settingsHasUsableProviderConfig` ↔ onboarding/validation.ts）→ import 一处。
- chat 内小工具：`compactText`×3、`artifactDataUrl`×4、`toolRecordRawName`×3、`visibleProviders`×2、`isWindows`/`isMac` 再造 → 各收敛为单一 import。
- `detectExternalAgents()` 三组件各自 mount 拉取 → 一个缓存 promise（照 settingsCache 模式）。
- RequestDebugPanel 手搓 YAML 序列化器（~55）→ 删 YAML 开关，保留 JSON（`prettyJson` 已在）。
- `userProfile.ts` 两个一行函数 → 内联进 Sidebar，删文件。

> 注：仅「取消 export」类项（缩小 API 面）本批可选做，优先做真正删行的项，避免噪音 diff。

---

## 批次 2 — Rust core 确证死代码（护栏：cargo build + 测试基线）

- `src-tauri/src/lib.rs` + `lens_commands.rs`：`lens_request`/`lens_request_translate`/`lens_request_translate_text`/`lens_request_replace` 去掉 `#[tauri::command]` + 4 个 invoke_handler 行（**保留 fn 本体**，热键仍调用）。
- `api.rs`：`record_api_usage_success/failure/cancelled` 三个转发 wrapper → 直调 `record_api_usage(.., Outcome::X)`；配合把 map_err 里重复 ~20 次的 failure 记录收敛为每函数一个本地 `fail` 闭包（~150+60 行）。
- `api.rs`：`send_with_retry_for_failover` 单调用纯转发 → 内联。
- `state.rs`：`AppState` 三处字面量（lib.rs manage / `new_headless` / `test_app_state`）→ 一个基构造器 + override 差异字段（~80）。
- `state.rs`：`chat_stream_generations` map → 全局 `AtomicU64`（~25）；`get/set_cached_detected_agents` → 复用 `get_cached/set_cached` 定 key（~20）。
- `rapidocr.rs`：`ocr_image`/`ocr_image_lines` 重复 17 行 pipeline init → 提 `pipeline()`。
- `windows_ocr.rs`：`ocr_image` async 壳 → 合并进同步版（仅 delegate）。

---

## 批次 3 — chat 后端确证死代码（护栏：cargo build + loop_tests）

- `chat/agent/types.rs` + rounds/planning/finalize/prepare：删 `AgentStepResult` / `RunState.steps` / `AgentRunResult.steps` / `PrepareStepInput.previous_steps`+`step_number`，改 ~8 处 loop_tests 断言到 `tool_records`/`stop_reason`（~80）。
- `chat/agent/types.rs` + 4 构造点：删 `AgentRunConfig` 六个死字段 `entry`/`has_image`/`skill_registry`/`active_skill_id`/`active_skill_detail`/`custom_system_prompt`（~40）。
- `chat/model/types.rs` + 4 adapter：删 `LanguageModelProvider::capabilities()` + `ProviderCapabilities`（零调用，~45）。
- `chat/agent/prepare.rs` + planning/synthesis：内联 `prepare_agent_step`/`PreparedStep` 两行 phase match，去全量 message clone（~40）。
- `chat/memory.rs`：`atomic_write` 复制 → 复用 `storage::atomic_write`（~38）。
- `chat/agent/compaction.rs`：`has_tool_calls`/`token_split_chat_messages`/`manual_fallback_split` 移入 `#[cfg(test)]`（~35）。
- `chat/agent/stop.rs`：`evaluate_stop_after_model_step`（仅自测试）+ 2 test（~30）。
- `chat/todo.rs`：`apply_tool` 单名分发 + 死 `_current` 参 + 恒定 `changed` → 直调 `apply_todo_write`（~25）。
- 小件：`truncate_chars` 5 份合并留 execute.rs pub 版（~25）；`model/types.rs` `GenerateOptions.stream`（无人读）、`ModelMessage::text_content`（零调用）、`temperature`（恒 0.7，写死或露出）；`agent/filter.rs` `_builtin_marker`；`storage.rs` 过期 `#[allow(dead_code)]` on `conversation_attachments_dir`（有 9 调用方）；`plan.rs` `is_plan_mode`/`is_orchestrate_mode` 一行 wrapper 内联。

---

## 批次 4 — kivio_code / external_agents 确证死代码（护栏：cargo build + 相关测试）

- `kivio_code/tui/components/input.rs`（560）+ mod re-export — 死 Input 组件。
- `external_agents/stream/json_events.rs`：`handle_codex`/`handle_cursor`/`handle_opencode`/`handle_gemini` + 4 个死 `JsonEventParser` 变体（types.rs）+ mod.rs 不可达 match 臂（**保留 `handle_kimi`**，~380）。
- `kivio_code/tui/autocomplete.rs`（320）+ editor provider 管线 — 生产不可达。
- `external_agents/defs/`：cursor/gemini/opencode/hermes 四 def → 一个 `acp_def(...)` + 数据行（~240）。
- `kivio_code/tui/keys.rs`：`parse_key`+legacy 表、`decode_kitty_printable`、`is_key_release`/`is_key_repeat`/`event_type`/`KeyEventType`（仅测试，~130）。
- `kivio_code/tui/components/loader.rs`：`CancellableLoader` + `set_indicator`/`abort_handle`/`aborted`（~110）。
- `kivio_code/tui/components/text.rs`：`BoxView`/`TruncatedText`（~110）。
- `kivio_code/tui/text_width.rs`：`extract_segments`（仅测试）+ pub `extract_ansi_code` 壳（~90）。
- `kivio_code/tui/keybindings.rs`：`set_user_bindings`/`conflicts`/`get_keys` + 收敛为静态默认表（~70）。
- `external_agents/types.rs` + run.rs：`UnifiedAgentEvent::Status`/`TurnEnd`/`Raw` 变体 + ~30 构造点（零消费，`_ => {}` 丢弃，~60）。
- `external_agents/run.rs`：`append_text`/`append_reasoning` → `append(kind, idx)`（~45）。
- `external_agents/slash.rs`：三 match 臂重复 RuntimeContext 构造上提（~45）。
- `external_agents/detection.rs`：`parse_models_list` 的 cursor/opencode 不可达臂（~30）。
- `external_agents/defs/codex.rs`：`codex_needs_danger_full_access` + `_codex_sandbox_hint`（~22）。
- 零调用小件合并（~90）：`skills/discover.rs` `scan_roots`/`folder_slug_for_path`；`external_agents/session/mod.rs` `sessions_root`/`session_file_exists`/`managed_session_path`；`kivio_code/session/mod.rs` `list_all_sessions`；`external_agents/spawn.rs` `emit_from_value`+`stream/mod.rs` `map_json_value`；`skills/mod.rs` `read_skill_record`；`external_agents/workspace.rs` `detection_cache_ttl`/`fresh`+`ResolvedWorkspace` 单字段壳；`kivio_code/interactive/app.rs` `loader_interval`。
- 只写不读字段：`external_agents/prompt.rs` `skip_transcript`、`types.rs` `RuntimeContext.cwd`、`mcp/registry.rs` `NativeToolContext.run_id`/`generation`/`depth`（~20）。
- `connectors/mod.rs` `slugify` → 复用 `skills::slugify`（~20）。

---

## 批次 5 — 依赖清理（护栏：cargo check，Windows 上验证）

- 删 `tauri-plugin-clipboard-manager`（Cargo.toml + lib.rs:142 init 行）— 全走 arboard。
- 删直接依赖 `ndarray`（仅 ort/oar-ocr 传递需要）。
- 删 `windows-future`（windows 0.61 已传递）——**cargo check 验证后再删**。
- `windows` crate 去 `Globalization`/`Foundation_Collections` feature——**cargo check 验证**。
- `tar` 移到 `[target.'cfg(target_os = "macos")'.dependencies]`。
- `package.json` 删 `@types/katex`（katex 0.16 自带类型）。

---

## Tradeoffs / 风险

- **测试改造**：删测试专用出货导出（`_reset*ForTest`、`isToolCallErrorStatus` 等）需同步改测试驱动方式，否则测试红。
- **平台依赖**：批 5 的 windows-future / windows features 传递关系只能在 Windows 上 `cargo check` 确认；删错会编译失败但可回滚。
- **回滚**：每批独立 commit，git 可逐批 revert。
- **取消 export 类项**：仅缩小 API 面、不减行，作为可选，避免制造大量噪音 diff 淹没真实删除。

## 兼容性

无外部契约变更。Tauri 命令去 `#[command]` 的 4 个 lens fn 前端从未 invoke，去除不影响 IPC 面；其余均为内部符号。
