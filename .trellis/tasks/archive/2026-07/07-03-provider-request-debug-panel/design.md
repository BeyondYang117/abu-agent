# Design：内建 provider 请求抓包调试面板

## 1. 架构总览

```
adapter (openai/anthropic/responses)
  ├─ 已有: record_usage_success / record_usage_failure  → usage.rs::record_model_call (落盘 usage)
  └─ 新增: 同处调用 request_debug::record(...)          → AppState.request_debug_buffer (内存环形)
                                                              ↑
Tauri 命令 get/clear_request_debug_records ────────────────┘
  ↑
前端 RequestDebugPanel.tsx (设置页)
```

复用现有 `record_usage_*` 作为挂载点：它已持有 `request`（含 metadata）、`started_at`、`duration`、`usage`、`status`。新增记录与 usage 记录同处触发，口径一致。

## 2. 数据结构（新模块 `src-tauri/src/chat/request_debug.rs`）

```rust
pub struct RequestDebugRecord {
    pub id: String,                    // dbg_<uuid>
    pub created_at: i64,
    pub duration_ms: u64,
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    pub api_format: String,
    pub operation: String,             // 复用 usage 的 operation
    pub conversation_id: Option<String>,
    pub message_id: Option<String>,
    pub status: String,                // "success" | "error"
    pub request: RequestDebugRequest,  // { url, headers: Map<String,String>(脱敏), body: Value }
    pub response: RequestDebugResponse,// { status_code, text, tool_calls: Value, finish_reason, usage, error }
}
```

- `AppState` 增 `pub request_debug: Mutex<VecDeque<RequestDebugRecord>>`（`new`/`new_headless` 初始化空）。
- 容量常量 `REQUEST_DEBUG_CAPACITY = 50`；push 时 `while len >= cap { pop_front() }`。
- `record(state, record)`：开关关 → 直接 return（调用方在构造前先查开关，见 §4 零开销）。

## 3. 脱敏

`sanitize_headers(headers) -> Map`：Authorization / api-key / x-api-key 等值截断为 `<前4字符>…`（复用抓包脚本口径）。body 里不含 key（key 只在 header）。`RequestDebugRequest` 存脱敏后的 headers。

## 4. adapter 接入（三处 adapter × success/failure）

在 `record_usage_success`/`record_usage_failure` 内，`record_model_call(...)` 之后追加：

```rust
if self.state.request_debug_enabled() {   // 读 settings 开关；关 → 短路，不构造 body
    let record = build_debug_record(request, &self.provider, status, &body_or_rebuild, response_meta, ...);
    crate::chat::request_debug::record(self.state, record);
}
```

- **零开销保证**：开关关时不调 `request_body()`、不 clone body。开关判定用一个轻量读（`state.settings_read()` 已有）。
- **request body**：success/failure 路径能拿到构造 body 的素材（`request` + `self.provider`）；stream 与非 stream 各自的 `body` 变量在方法作用域外，故 `build_debug_record` 内用 `self.request_body(request, is_stream)` 重建（幂等纯函数，仅在开关开时执行）。
- **headers**：adapter 已知会加的头（Authorization、Accept-Encoding、会话亲和头）在 `build_debug_record` 里按同规则重建后脱敏——不实际读 reqwest builder（那不可反查）。
- **response**：success 传 `GenerateOutput` 摘要（text 截断、tool_calls、finish_reason、usage）；failure 传 error 字符串 + status_code。

> 三个 adapter 结构相同，`build_debug_record` 作为 `request_debug.rs` 的自由函数，adapter 只传 `request/provider/is_stream/response_meta`。openai 与 responses 的会话头/body 规则不同 → 各 adapter 传自己那份已构造的 headers map（把"构造头"抽成可复用小函数，避免重建漂移）。

## 5. 开关 settings

`settings.chat_tools` 加 `request_debug_enabled: bool`（`#[serde(default)]`，默认 false）。前端经现有 settings 保存链路切换。放 `chat_tools` 而非新 struct，避免新增顶层设置节点的迁移面。

## 6. Tauri 命令（`chat/commands.rs` 或新 `commands` 区）

- `get_request_debug_records(state) -> Vec<RequestDebugRecord>`：`buffer.lock()` 克隆，最新在前（push 尾、返回时 reverse 或前端倒序）。
- `clear_request_debug_records(state)`：清空 buffer。
- 在 `lib.rs` 注册两个命令。

## 7. 前端（`src/settings/RequestDebugPanel.tsx`）

- 结构参考 `UsageStatsPanel.tsx`：顶部开关（切 `request_debug_enabled`）+ 刷新/清空/导出。
- 左列表：`created_at`（相对时间）、`operation`、`model`、`status`、`duration_ms`、token；点击选中。
- 右详情：请求（url/headers/body）+ 响应，用等宽 JSON 展示（复用 chat 已有的 JSON/代码渲染或 `<pre>`）+ 复制按钮。
- 导出：`get_request_debug_records` 结果 `JSON.stringify` → 触发浏览器下载（`Blob` + `a[download]`），文件名带时间戳。
- `src/api/tauri.ts` 加 `getRequestDebugRecords` / `clearRequestDebugRecords` binding + `RequestDebugRecord` 类型（`src/chat/types.ts` 或 `settings` 类型区）。

## 8. 兼容性 / 风险

- 默认关，开启才有内存占用（50 条 × 请求体，量级 MB 内可控）。
- 不落盘 → 无跨重启泄漏；即便开启也脱敏 key。
- `new_headless`（CLI/OCR）也初始化 buffer，避免 None 分支；CLI 无面板但记录不报错。
- 重建 body/headers 若与真实发送漂移 → 记录失真。缓解：把"构造 headers"抽成 adapter 内单一函数，发送与记录共用同一函数（§4）。

## 9. 测试

- `request_debug.rs` 单测：环形淘汰（push 51 条剩 50、丢最旧）、`sanitize_headers`（Authorization 截断、非敏感头原样）。
- adapter：开关关 → buffer 空；开关开 → 一次 success 产生一条含脱敏 header + body 的记录（用 headless state + mock provider，或纯 `build_debug_record` 单测）。
- 本机 cargo test 有 0xC0000139 → 纯函数（淘汰/脱敏/build_debug_record）用独立 harness 验证。
- 前端：面板开关切换、列表渲染、导出（Vitest + 组件测试，或最小化靠类型 + 手测）。
