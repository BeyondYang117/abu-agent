# Implement — 流式工具调用空 id 覆盖修复

上下文:本文件 + `prd.md` + `design.md`。全在 `src-tauri/src/chat/model/openai.rs`。Windows Rust 测试用 `scripts/win-cargo-test.ps1`。

## 清单
- [ ] A `handle_openai_stream_tool_calls`(~811):id 捕获加 `.filter(|v| !v.is_empty())`,只在非空时覆盖 `partial.id`。
- [ ] B `finish_tool_call_partials`(~876):`partial.id.filter(|s| !s.is_empty()).unwrap_or_else(|| format!("call_{index}"))`。
- [ ] C 单测(openai::tests):
  - `streamed_tool_call_keeps_first_chunk_id_over_empty`:喂两片同 index——首片 `id="call_real"` + name,续片 `id=""` + 参数增量 → `finish_tool_call_partials` 得 id="call_real"。
  - `streamed_tool_call_synthesizes_id_when_all_empty_or_absent`:分片 id 全空/无 → 得非空 `call_0`。
  - 复用既有 `handle_openai_stream_tool_calls` 测试风格(构造 delta Value + PartialToolCall + FakeSink)。
- [ ] D `cargo check` → `powershell -File scripts/win-cargo-test.ps1 --lib "chat::model::openai"`。
- [ ] E dev app 实测(复现→修复):对商汤 `deepseek-v4-flash` 跑会触发工具的 chat-probe,由 400 变正常完成;检查发出的 tool_call_id 非空(必要时临时日志,验证后移除)。

## 验证命令
- `cd src-tauri && cargo check --no-default-features`
- `powershell -File scripts/win-cargo-test.ps1 --lib "chat::model::openai"`

## 风险 / 回滚
- 极低;还原两处即可。
