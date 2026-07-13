# 流式工具调用空 id 覆盖修复

## Goal

修复 OpenAI 兼容流式解析里"工具调用真 id 被后续空 id 分片覆盖"的 bug——它导致商汤(SenseNova)DeepSeek 等严格校验端点在多轮工具对话时报 `400 invalid tool_call_id (code 3)`,历史/工具消息发不出去。

## Background / 已核实的根因(抓包对比 opencode)

- OpenAI 流式规范:工具调用的 `id` 只在该 tool_call 的**第一个 delta 分片**里给;后续参数增量分片不再带 id(或带空)。
- 商汤实测(Claude Tap 抓 opencode↔SenseNova 真实流量,`trace_c72cfbee.html`):首片给真 id `call_2e20de04c13f4092b795e80c`,续片给 `"id":""`(空串)。opencode 忽略空续片、保住真 id → 回放 `tool_call_id` 全用真 id → 正常。opencode 与 Kivio 都是 `stream:true`,同一 provider/model。
- Kivio bug(`src-tauri/src/chat/model/openai.rs`):
  - `handle_openai_stream_tool_calls` **第 811 行** `if let Some(id) = call.get("id")... { partial.id = Some(id) }` —— **每个分片都无条件覆盖 `partial.id`**(name 在 794 行有 `!is_empty()` 过滤,id 没有)。首片真 id 被续片 `""` 覆盖成空。
  - `finish_tool_call_partials` **第 876 行** `partial.id.unwrap_or_else(|| "tool_{index}")` 只兜底 `None`,`Some("")` 逃过 → 最终 `tool_call_id = ""`。
  - Kivio 定向调试日志确认:发给商汤的 assistant `tool_calls[].id=""`、tool 消息 `tool_call_id=""` → 商汤 400。
- 责任:商汤给了合法 id(合规);**是 Kivio 流式解析把真 id 覆盖丢了(纯 Kivio bug)**。OpenAI 官方/多数网关忽略空 id 所以没暴露。

## Requirements

- R1 `handle_openai_stream_tool_calls`(811 行):捕获 tool_call `id` 时**只在非空时才覆盖** `partial.id`(加 `.filter(|v| !v.is_empty())`,与 name 一致)。保住首片真 id,忽略续片空 id。
- R2 `finish_tool_call_partials`(876 行,纵深防御):兜底同时覆盖 `None` **和空串**——`partial.id.filter(|s| !s.is_empty()).unwrap_or_else(|| format!("call_{index}"))`。即使极端情况下仍拿到空 id,也生成合法非空 id,而非发空串。
- R3 不改其他 provider 行为(带真 id 的正常路径不受影响);移除此前排查用的临时调试日志(已还原,确认工作树无残留)。

## Acceptance Criteria

- [ ] AC1 商汤 DeepSeek 多轮工具对话不再报 `invalid tool_call_id`;实测(chat-probe,会触发工具的 prompt)由 400 变为正常完成。
- [ ] AC2 单测:模拟"首片带真 id、续片带空 id"的流式分片序列 → 最终 PendingToolCall.id = 真 id(未被覆盖)。
- [ ] AC3 单测:分片从不带 id / 全空 → 最终 id 为生成的非空 `call_{index}`(不为空串)。
- [ ] AC4 既有 openai 流式工具测试不回归;`cargo`(Windows 用 `scripts/win-cargo-test.ps1`)通过。

## Out of Scope

- 非流式 `pending_tool_calls_from_openai_message` 的空 id(商汤走流式;如需可后续同理加固)。
- 不改 anthropic/gemini/responses 适配器(它们各有 id 来源,不涉此 bug)。

## Open Questions

（无;根因经抓包对比 opencode 已确证。）
