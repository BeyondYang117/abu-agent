# P2-A：MCP 持久连接管理器

> 来源：`06-12-refactor-kivio-agent-architecture-based-on-clawspring` P2 三线之一。
> 调研蓝图由 workflow `p2-research-blueprint`（2026-06-13）产出，已逐符号锚定真实代码。

## 背景与现状（已核实）

当前 stdio MCP 是**一次性连接**：`StdioMcpClient::connect`（client.rs:81）每次 `list_tools`/`call_tool` 都 `spawn()` 子进程 + `initialize()` 握手，`StdioSession::Drop`（client.rs:39-43）在作用域结束即杀进程。10 次调用 = 10 次 spawn + 10 次握手。无空闲跟踪、无探活、无状态面板；`parse_tool_result`（client.rs:543-577）硬编码 `artifacts: Vec::new()`，image content 被静默丢弃。

## 目标 / 验收标准（prd.md:81 口径）

- [ ] 同一 MCP 服务器连续 10 次调用仅 1 次握手（`McpSession.handshake_count == 1`）。
- [ ] 空闲超时（默认 10 分钟）后子进程被回收，下次调用透明重连。
- [ ] kill 服务器进程后下次调用透明重连（liveness 探活 + 单次重连重试）。
- [ ] app 退出无孤儿进程（`kill_on_drop(true)` + ExitRequested 同步 `disconnect_all`）。
- [ ] MCP image content → artifacts（不再丢弃）。
- [ ] 状态事件 `mcp-server-state` + 设置页状态面板（状态点 / lastError / stderr tail / 重连按钮）。
- [ ] 启动期并行预热（非阻塞，失败仅置 Error 态）。

## 用户决策（2026-06-13）

- **空闲超时做成设置项**：`ChatToolsConfig.mcp_idle_timeout_ms`，`#[serde(default)]` 默认 `600_000`，`sanitize_settings` 钳制（下限如 60s），设置页可配。**本任务内做，不 defer**（覆盖蓝图中 "deferred optional" 的判断）。
- 蓝图回来直接实现，范围以上述验收标准为准。

## 实现蓝图（文件级）

### 数据结构（新文件 `src-tauri/src/mcp/manager.rs`）

- `McpServerState`（`Connecting/Connected/Error{message}/Disconnected`，`#[serde(tag="kind")]` 序列化给前端）。
- `McpSession`：`config_fingerprint`（ChatMcpServer 序列化哈希，变更即重建）、`state`、`server_info`、`capabilities`、`tools: Vec<McpTool>`、`stderr_tail: Arc<Mutex<VecDeque<String>>>`（尾 ~20 行）、`last_used: Instant`、`handshake_count: u64`、`transport: McpTransport`。
- `McpTransport`：`Stdio(StdioConn)` | `Http { session_id: Option<String> }`。
- `StdioConn`：`child`（`kill_on_drop(true)`）、`stdin`、`next_id`、`pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value,String>>>>>`、`reader_task`、`stderr_task`、`timeout`。`Drop` abort 两个 task + `start_kill()`。
- 连接池挂 `AppState`（镜像 `key_cooldowns` 模式）：`mcp_sessions: tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<McpSession>>>>`。每会话独立 `Arc<Mutex>`，A 服务器握手不阻塞 B。
- `McpConnectionManager` 以 `impl AppState` API 形式存在（复用 `self.http`/`self.settings_read()`/池），不另建结构。常量 `IDLE_TIMEOUT`（来自设置）、`STDERR_TAIL_LINES = 20`。

### 生命周期

- `mcp_get_or_connect(server) -> Arc<Mutex<McpSession>>`：锁池 → 算 fingerprint → 命中且 Connected 则克隆 Arc 返回（**立即释放外层锁，不跨 await 持锁**）；否则插 Connecting 占位、发事件、释锁、spawn/initialize 一次、存元数据、置 Connected、`handshake_count += 1`。失败置 `Error{msg + stderr_tail}`。
- `mcp_call_tool`（重写 registry.rs:342-357 的两个 client 分支为单一 `state.mcp_call_tool(...)`）：get-or-connect → 锁会话 → **liveness 探活**（stdio `child.try_wait()?.is_some()` 或 pending 发送失败 ⇒ 死）→ 死则丢弃 transport、重连一次、刷新 tools、重试单请求（最多一次）。stdio 请求：`next_id += 1`、注册 oneshot、写行、`timeout(self.timeout, rx)`；reader task 循环 `next_line` 解析 JSON 按 id 匹配 `pending.remove(id).send(...)`，无 id 通知忽略（保留 client.rs:428-430 逻辑）→ 支持并发在途请求。HTTP：复用 `session_id`，仅 404 / session-not-found 清除并 re-initialize 重试一次，其他错误透传。成功后 `last_used = now`。
- **空闲回收 reaper**（main.rs setup 内 `tokio::spawn`）：`interval(60s)` → 锁池收集 `last_used.elapsed() > IDLE_TIMEOUT` 的 id 并移除（Drop 杀进程）、发 `Disconnected`。锁内只收集+移除，无 await。
- **退出杀全部**（main.rs:319 `RunEvent::ExitRequested`）：`code.is_none()` 时 `prevent_exit()`；否则 `block_on(state.mcp_disconnect_all())` 排干池、每会话 Drop abort task + `start_kill`，`kill_on_drop(true)` 兜底。

### 文件改动清单

- **新建** `src-tauri/src/mcp/manager.rs`（上述全部）。
- `src-tauri/src/mcp/mod.rs`：`pub mod manager;`。
- `src-tauri/src/mcp/client.rs`：`stderr(Stdio::piped())` + `.kill_on_drop(true)`；把握手 JSON 抽成可复用 helper 供 manager 调用；**`parse_tool_result` 加 image 映射**（`type=="image"` → `ChatToolArtifact{name,mime_type,data_url,size_bytes}`，文本插 `[image: <mime>]` 占位，返回 artifacts 而非空 vec）；保留 HTTP/SSE 机制与 TCP-fake-server 测试模块。
- `src-tauri/src/state.rs`：加 `mcp_sessions` 字段；`test_state()` 构造器初始化（编译必需）。
- `src-tauri/src/main.rs`：`AppState{}` 字面量加字段；setup 后 spawn reaper + 非阻塞 warmup；注册 `chat_mcp_server_status` / `chat_mcp_reload_server`；ExitRequested 加 disconnect_all。
- `src-tauri/src/mcp/registry.rs`：`call_tool` MCP 分支改走 `mcp_call_tool`；`list_server_tools` 拆 test 路径（复用 `state.http` 修 line 369 的 `reqwest::Client::new()` 不一致）vs session 路径；`list_enabled_tool_defs`（221-238）serial+eprintln 改 `futures::join_all` + 失败置 Error 态发事件；新增两个 Tauri 命令（camelCase）。definitions 层 TTL 缓存不动。
- `src-tauri/src/settings.rs`：**加 `mcp_idle_timeout_ms`**（用户决策，见上）到 `ChatToolsConfig`，`#[serde(default)]` + sanitize 钳制。
- `src/api/tauri.ts`：加 `chatMcpServerStatus()` / `chatMcpReloadServer(serverId)` + `listen('mcp-server-state')` + `McpServerStatus` 类型。
- `src/settings/SettingsShell.tsx`：状态点 + lastError + 折叠 stderr tail + 重连按钮（复用 mcpTestFeedback 区域）。
- `src-tauri/capabilities/default.json`：确认 `mcp-server-state` emit 的 window label 允许（`["main","lens","chat"]`）。
- **不动** chat 事件 payload 契约（`McpToolCallResult`/`ChatToolArtifact`/`ChatToolDefinition` 形状不变）、execute.rs 的 select! 超时/取消缝、`(record, content)` 返回契约。

### 测试计划（`cargo test --manifest-path src-tauri/Cargo.toml`）

复用 client.rs:716 的 TCP-fake-HTTP harness，新增 fake-stdio server（小脚本/测试二进制 echo JSON-RPC，`cfg(unix)` 门控）。
- `ten_calls_one_handshake`（验收①）
- `liveness_reconnect_on_dead_child`（kill-proc 透明重连，`handshake_count == 2`）
- `http_reconnect_only_on_404`（404 重连 / 500 不重连）
- `idle_reap_evicts_and_reconnects`（注入小 IDLE_TIMEOUT）
- `parse_tool_result_maps_image_to_artifact`（纯函数）
- `config_fingerprint_rebuilds_session`
- `disconnect_all_kills_children`（无孤儿）
- 并发 stdio：2 在途请求按 id 正确关联
- AppState 编译门：`test_state()` 仍可构造（既有 `pick_active_key` 测试保持绿）

**红线**：测试禁止清除/覆盖真实 settings.json 的 providers/API key，一律用内存 `Settings::default()` + fake server。

### Commit 切分（每步编译 + cargo test 绿）

1. `feat(mcp): map image content blocks to artifacts`（client.rs + 单测，零行为风险，最先做）。
2. `refactor(mcp): add McpSession + connection pool scaffold on AppState`（manager.rs + 池字段 + test_state/main.rs 构造 + mod 导出 + reaper + disconnect_all + ExitRequested hook；未接调用路径）。
3. `feat(mcp): route call_tool/list through persistent sessions`（registry 重写 + reader-task/pending + liveness + reconnect + warmup；最高风险，靠 ten_calls/liveness 测试护航）。
4. `feat(mcp): server status command + state events + settings panel`（命令 + emit + tauri.ts + SettingsShell）。
5. `feat(settings): mcp_idle_timeout_ms configurable`（用户决策；可并入 commit 2 或独立）。

### 关键风险

- **跨 await 持池锁死锁**：绝不跨握手/RPC await 持 `mcp_sessions` 外层锁，克隆 per-session Arc 后立即释锁。
- **孤儿进程**：`kill_on_drop(true)` 强制；stdio server 自身派生的孙进程可能仍泄漏（与 clawspring 一致，列为后续 process-group kill）。
- **契约保持**：serde 形状不变 ⇒ 无 chat payload / settings.json 迁移；新字段必 `#[serde(default)]`。
