# 内建 provider 请求抓包调试面板

## Goal

把这两天用外挂 Python 抓包代理做的事内建进 Kivio：开启后，每次 provider 调用的完整请求（headers + body）+ 响应 + usage + 耗时被记录到内存，设置里的开发者面板提供请求列表 + 完整 JSON 查看器 + 导出。免改 baseUrl、免重启、覆盖所有 provider 和入口。调试请求格式/工具调用/串台等问题时不再需要外挂代理。

## 决策（已与用户确认，2026-07-03）

- **存储**：内存环形缓冲（最近 N 条，默认 50），不落盘 → 零磁盘泄漏面；进程退出即清。
- **记录内容**：请求（headers + body）+ 响应（最终文本/工具调用/finish_reason/usage）都记。
- **覆盖面**：全部 provider 调用（chat/子agent + 翻译/截图OCR/Lens），走各 adapter 统一接入点。
- **UI**：设置里"用量统计"页新增"请求调试"二级视图（与现有 UsageStatsPanel 同构、同页切换）。

## Requirements

### R1：后端记录（内存环形缓冲）
- `AppState` 增加一个 `Mutex<VecDeque<RequestDebugRecord>>`（容量上限，超出丢最旧）。
- 记录字段：`id`、`created_at`、`duration_ms`、`provider_id`/`provider_name`/`model`、`api_format`、`operation`（复用 usage 的 operation 派生）、`conversation_id`/`message_id`、`request`（`{url, headers(脱敏), body}`）、`response`（`{status_code, text, tool_calls, finish_reason, usage, error}`）、`status`。
- **接入点**：model adapter 层（`openai.rs`/`anthropic.rs`/`responses.rs`）。现有 `record_usage_success`/`record_usage_failure` 已持有 `request`+timing+usage，是天然挂载点；请求 body 复用 `request_body()`，headers 复用 adapter 已构造的头。
- **开关**：`settings.chat_tools`（或新 `settings.developer`）加 `request_debug_enabled: bool`，默认 `false`。关闭时零开销（不构造记录）。
- **脱敏**：Authorization 头截断为 `sk-xxxx…`（复用抓包脚本口径）；不记 api_keys 明文。

### R2：Tauri 命令
- `get_request_debug_records() -> Vec<RequestDebugRecord>`（返回当前缓冲快照，最新在前）。
- `clear_request_debug_records()`（清空）。
- 开关经现有 settings 保存链路（`persist_settings`）。

### R3：前端开发者面板
- `src/settings/` 新增 `RequestDebugPanel.tsx`：顶部开关 + 清空/刷新/导出按钮；左列请求列表（时间/operation/model/token/耗时/状态），右侧完整 JSON 查看器（可折叠、复制）。
- **并入"用量统计"页**（不再作为独立 Settings 导航项）：用量统计页顶部加二级切换（用量统计 / 请求调试），复用 `kv-seg` 分段控件；`UsageStatsPanel` 与 `RequestDebugPanel` 两个组件保持独立，仅挂载位置合并。
- 导出：把当前记录导成 JSON 文件（复用现有文件保存/交付能力或浏览器下载）。
- `src/api/tauri.ts` 加对应 binding。

## Acceptance Criteria

- [ ] AC1：开关默认关；关闭时不产生任何记录（缓冲为空）、无可测开销。
- [ ] AC2：开启后，chat 一轮带工具的对话在面板出现对应记录，请求 JSON 含 headers（Authorization 已脱敏）+ 完整 body（messages/tools/session 字段齐全），响应含 finish_reason + tool_calls + usage。
- [ ] AC3：翻译/截图OCR/Lens 的 provider 调用也被记录（覆盖面验证，至少 chat + 一个非 chat 入口）。
- [ ] AC4：缓冲超过上限时丢弃最旧记录，不无限增长；进程退出后不残留磁盘文件。
- [ ] AC5：面板可查看完整 JSON、复制、清空、导出为 JSON 文件。
- [ ] AC6：脱敏彻底——记录里任何位置都不含完整 api key。
- [ ] AC7：`npm run typecheck`/`lint`/`test` 全绿；`cargo check --lib --tests` 干净；后端新逻辑有单测（环形缓冲淘汰、脱敏；cargo test 环境问题时 harness 验证）。

## Notes

- 范围外：响应流的逐帧记录（只记最终聚合响应，不记每个 SSE chunk）；跨重启持久化（明确不落盘）；记录的搜索/过滤 UI（首版列表 + 查看即可）。
- 安全：这是开发者调试功能，默认关闭；即便开启也只在内存、脱敏 key。
- 复用：operation 派生、provider 元数据用 usage.rs 现成的；面板结构参考 UsageStatsPanel.tsx。
