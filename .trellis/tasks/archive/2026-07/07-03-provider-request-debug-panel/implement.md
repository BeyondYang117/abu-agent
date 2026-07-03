# Implement：内建 provider 请求抓包调试面板

前置：读 prd.md + design.md。

## 执行顺序

### Step 1：后端核心模块 + 状态
- [ ] 新建 `src-tauri/src/chat/request_debug.rs`：`RequestDebugRecord`/`RequestDebugRequest`/`RequestDebugResponse` 结构（serde camelCase）、`REQUEST_DEBUG_CAPACITY=50`、`record(state, rec)`（环形淘汰）、`sanitize_headers`、`build_debug_record(...)`。
- [ ] `AppState` 加 `request_debug: Mutex<VecDeque<RequestDebugRecord>>`；`new`/`new_headless`/test 构造初始化空。
- [ ] `mod request_debug;` 挂到 `chat/mod.rs`。
- [ ] 单测：环形淘汰、sanitize_headers。
- 验证：`cargo check --lib --tests`（0xC0000139 时 harness）。

### Step 2：settings 开关
- [ ] `ChatToolsConfig` 加 `request_debug_enabled: bool`（`#[serde(default)]`）。
- [ ] `AppState::request_debug_enabled()` 便捷读（`settings_read().chat_tools.request_debug_enabled`）。
- 验证：cargo check。

### Step 3：adapter 接入（三处）
- [ ] 把各 adapter "构造 headers" 抽成单一函数（openai 已有 `with_session_headers`，把 Authorization/Accept-Encoding/会话头统一；anthropic/responses 同理），发送与记录共用。
- [ ] `openai.rs`/`anthropic.rs`/`responses.rs` 的 `record_usage_success`/`record_usage_failure`（或等价 usage 记录点）后追加：开关开 → `build_debug_record` + `request_debug::record`。
- [ ] response 摘要：success 传 `GenerateOutput`（text 截断/tool_calls/finish_reason/usage），failure 传 error+status_code。
- 验证：cargo check；开关关 buffer 空、开关开产生一条（headless state 单测或 harness）。

### Step 4：Tauri 命令
- [ ] `get_request_debug_records` / `clear_request_debug_records`（commands.rs）。
- [ ] `lib.rs` 注册。
- 验证：cargo check。

### Step 5：前端面板
- [x] `src/api/tauri.ts`：binding + `RequestDebugRecord` 类型。
- [x] `src/settings/RequestDebugPanel.tsx`：开关 + 列表 + JSON 详情 + 复制/清空/导出。
- [x] 挂进 Settings 导航。
- 验证：`npm run typecheck && lint && test`。

### Step 5b：并入用量统计 + claude-tap 风格重做（本轮）
- [x] 面板并入"用量统计"页二级视图（去掉独立导航项），PRD R3 已同步。
- [x] 列表项对齐 claude-tap：来源类别彩色徽标 + 左侧竖条、model 徽标染色、token(千分位)/耗时/时间、endpoint、gap 分隔。
- [x] 详情默认视图：工具（可展开参数，兼容 OpenAI/Anthropic）、系统提示词、消息（角色卡片 + tool_use/tool_result 归一化）、响应、请求 Body/Headers、完整 JSON；usage 彩色明细；请求 JSON/cURL/整条 复制。
- [x] 详情 Trace 视图：输入/输出/元数据分块 + JSON/YAML 切换 + 整块复制（无 SSE，范围外）。
- 验证：typecheck/lint/vitest(185) 全绿；trellis-check PASS。

### Step 6：全量检查 + 手测
- [x] typecheck/lint/vitest 全绿；cargo check --lib --tests 干净（backend 未改，102647d 已验）。
- [x] 手测：开关开 → chat 带工具对话 → 面板出现记录，headers 脱敏、body 完整、工具/消息/响应/usage 齐；翻译/标题总结等非 chat 入口也被记录；类别徽标与来源一致。

## 回滚点
- Step 1–2 是纯新增（模块+字段+开关），无行为变化，可独立回滚。
- Step 3 是唯一触碰发送路径的改动（且被开关短路），出问题优先 revert 它。
- 每 Step 一个 commit。

## Review gate
- Step 3 后自查"零开销"（开关关时不构造 body/headers）+ 脱敏彻底。
- Step 6 结束跑 trellis-check 再进 Phase 3。
