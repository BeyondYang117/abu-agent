# Design — 流式工具调用空 id 覆盖修复

## 范围

单文件:`src-tauri/src/chat/model/openai.rs`,两处小改 + 测试。不涉及其他适配器 / 存储 / 前端。

## 改动

### R1 捕获时不被空 id 覆盖(核心)

`handle_openai_stream_tool_calls`,当前(~811):
```rust
if let Some(id) = call.get("id").and_then(|value| value.as_str()) {
    partial.id = Some(id.to_string());
}
```
改为(与同函数里 name 的 `!is_empty()` 过滤一致):
```rust
if let Some(id) = call
    .get("id")
    .and_then(|value| value.as_str())
    .filter(|value| !value.is_empty())
{
    partial.id = Some(id.to_string());
}
```
效果:首片真 id 设入后,续片 `"id":""` 不再覆盖;续片省略 id 也本就不覆盖。partial.id 保住首片真 id。

### R2 兜底覆盖空串(纵深防御)

`finish_tool_call_partials`,当前(~876):
```rust
let id = partial.id.unwrap_or_else(|| format!("tool_{index}"));
```
改为:
```rust
let id = partial
    .id
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| format!("call_{index}"));
```
即使 partial.id 仍为 `Some("")`(理论上 R1 后不会),也生成合法 `call_{index}`,绝不产出空串。用 `call_` 前缀(比 `tool_` 更贴近 OpenAI 惯例;同函数下游 ToolCallStart/Delta/Done/PendingToolCall 全用这同一 id,一致)。

## 一致性说明

- 同一 tool_call 的 id 在整条流里由 R1 保住(真 id)或 R2 生成(合成),`finish_tool_call_partials` 内 ToolCallStart / ToolCallDelta / ToolCallDone / PendingToolCall.id 都用这同一个 `id` 变量 → live 展示、工具执行、结果 tool_call_id、回放全程一致非空。
- 带真 id 的正常 provider:R1 后行为不变(首片真 id 设入,后续同 id 覆盖为同值或非空,无影响)。

## 兼容性 / 回滚

- 纯解析加固,无 schema / 契约变化。回滚即还原两处。

## 风险

- 极低。唯一行为变化:忽略空 id 覆盖 + 空 id 生成合成 id。对合规 provider 无影响,对商汤类修复。
