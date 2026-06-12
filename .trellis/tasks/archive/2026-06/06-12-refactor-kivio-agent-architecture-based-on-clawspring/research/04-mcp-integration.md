# 04 — MCP 集成（client / 生命周期 / 工具发现与命名 / 配置管理 / 健康与重连）

> 对照对象：
> - clawspring: `/Users/zmair/ZM database/keylingo/clawspring/mcp/`（client.py 546 行、config.py 133 行、tools.py 131 行、types.py 124 行）
> - kivio: `/Users/zmair/ZM database/keylingo/keylingo/src-tauri/src/mcp/`（client.rs 840 行、registry.rs 871 行、types.rs 1054 行，registry.rs 工作区有未提交的纯格式化修改，无语义变化）

---

## 1. clawspring 设计精读

### 1.1 数据结构（mcp/types.py）

- `MCPTransport`（types.py:11-15）：`stdio / sse / http / ws` 四种枚举（ws 声明了但 client 未实现）。
- `MCPServerConfig`（types.py:18-61）：name、transport、stdio 字段（command/args/env）、HTTP 字段（url/headers）、**每服务器独立 `timeout`（默认 30s）**、`disabled` 开关。`from_dict`（types.py:44-61）容错解析，未知 transport 回落 stdio。
- `MCPServerState`（types.py:66-70）：`disconnected / connecting / connected / error` 四态状态机——这是 clawspring 生命周期管理的核心可观测状态。
- `MCPTool`（types.py:75-91)：保存 `server_name / tool_name（原名）/ qualified_name（mcp__server__tool）/ description / input_schema / read_only`（来自 `annotations.readOnlyHint`）。`to_tool_schema()`（types.py:85-91）转 Claude API 格式，description 前缀 `[MCP:{server}]` 标注来源。
- JSON-RPC helper（types.py:96-124）：`make_request / make_notification / INIT_PARAMS`（协议版本 `2024-11-05`，声明 `tools` + `roots` capabilities）。

### 1.2 传输层（mcp/client.py）

**StdioTransport（client.py:19-129）— 持久子进程 + 后台 reader 线程**：

- `start()`（client.py:37-51）：`subprocess.Popen` 启动子进程（env 合并 `os.environ`），起两个 daemon 线程：
  - `_read_loop`（client.py:53-70）：持续读 stdout，按行解析 JSON，**按 `id` 匹配回 pending 表**（`_pending: {id → {event, result}}`），通过 `threading.Event` 唤醒等待方。能天然容忍服务器主动推的 notification（无 id 的消息被忽略而不是报错）。
  - `_stderr_loop`（client.py:72-80）：**持续收集 stderr，保留尾部 20 行**（client.py:127-129 `stderr_output`），连接失败时可直接给用户看服务器自己打印的错误——这是非常实用的可诊断性设计。
- `request()`（client.py:88-107）：自增 id → 注册 pending → 写入 stdin → `event.wait(timeout)`；超时抛 `TimeoutError`，JSON-RPC error 抛 `RuntimeError`。请求级 timeout 可覆盖服务器级 timeout。
- `alive` 属性（client.py:123-125）：`process.poll() is None`——**直接探测子进程是否存活**，是崩溃检测的依据。

**HttpTransport（client.py:134-274）— 旧式 SSE 会话 + 纯 HTTP**：

- SSE 模式（client.py:172-221）：GET `/sse` 拿 `endpoint` 事件得到 session URL（处理相对/绝对 URL，client.py:196-197），之后请求 POST 到 session URL、响应从常驻 SSE 流里按 id 配对（client.py:201-208）。
- HTTP 模式：直接 POST 拿响应（client.py:242-245）。
- 注意：clawspring 实现的是**旧版 HTTP+SSE 双端点协议**，不是 2025 streamable HTTP（无 `mcp-session-id` 头）。

### 1.3 连接生命周期（MCPClient，client.py:279-438）

- `connect()`（client.py:300-313）：状态机推进 `CONNECTING → CONNECTED/ERROR`，异常时保存 `_error` 并保留 ERROR 态供 status 展示。
- `_handshake()`（client.py:323-327）：`initialize`（独立 15s 超时）→ 记录 `serverInfo` / `capabilities` → 发 `notifications/initialized`。
- `list_tools()`（client.py:349-361）：**先检查 `capabilities` 是否声明 `tools`**（client.py:354），没有则直接返回空，避免对不支持的服务器打无效请求。
- `_parse_tool()`（client.py:363-384）：组装 `mcp__{server}__{tool}` 后**逐字符 sanitize（非字母数字→`_`）**保证 API 兼容；读 `annotations.readOnlyHint`；schema 缺失/非法时兜底 `{"type":"object","properties":{}}`。
- `call_tool()`（client.py:388-417）：按**原始工具名**调用；结果聚合 content blocks——text 取文本、image 转 `[image: mime]` 占位、resource 转 `[resource: uri]` 占位；`isError` 时前缀 `[MCP tool error]` 返回（不抛异常，让模型看到错误文本自行处理）。
- `status_line()`（client.py:421-438）：单行状态（图标 + server 名/版本 + 工具数 + 错误），供 `/mcp` 命令展示。

### 1.4 Manager 与按需重连（MCPManager，client.py:443-534）

- 单例（client.py:539-546），`_clients: {name → MCPClient}`。
- `add_server()`（client.py:449-458）：同名替换前先 disconnect 旧实例（防进程泄漏）。
- `connect_all()`（client.py:460-473）：逐个 connect + list_tools，**返回 `{name: error_or_None}` 而不是抛异常**——单服务器失败不影响其它服务器。`disabled` 服务器记 `"disabled"`。
- `call_tool()`（client.py:493-518）——**最巧妙的部分**：
  1. 解析 `mcp__server__tool` 三段名（client.py:496-500）；
  2. **`if not client.alive: client.reconnect(); client.list_tools()`（client.py:507-509）—— 调用前探活，进程死了自动重启并刷新工具表**，对模型完全透明；
  3. 通过 `qualified_name` 反查**原始工具名**（client.py:512-516），解决 sanitize 造成的名字不可逆问题。
- `reload_server()`（client.py:530-534）：单服务器重连刷新。

### 1.5 配置管理（mcp/config.py）

- 双层配置（config.py:34-35）：`~/.clawspring/mcp.json`（用户级）+ `<cwd>/.mcp.json`（项目级，**向上最多回溯 10 层目录查找**，config.py:54-65），项目级按 server 名覆盖用户级（config.py:60）。格式完全兼容 Claude Code 的 `.mcp.json`。
- CRUD API（config.py:73-115）：`save_user_mcp_config / add_server_to_user_config / remove_server_from_user_config`，写文件时保留其它顶层 key。

### 1.6 工具注册与合入工具列表（mcp/tools.py + clawspring.py）

- `initialize_mcp()`（tools.py:58-90）：加载配置 → 全部连接 → 把每个连接成功服务器的工具用 `_register_tool`（tools.py:45-53）包成 `ToolDef` 注册进全局 `tool_registry`（tool_registry.py:37-39，同名覆盖语义）。`read_only` 透传给注册表（影响并行调度），`concurrent_safe=False`。
- 工具闭包 `_make_mcp_func`（tools.py:34-42）：捕获 qualified_name，调用 `mgr.call_tool`，异常转错误字符串（不让 agent 循环崩溃）。
- **后台线程自动初始化**（tools.py:120-131）：`import mcp.tools` 时起 daemon 线程连接所有服务器，**启动不阻塞主流程**；tools.py 由 tools.py:1067-1069（`import mcp.tools as _mcp_tools`）在内置工具模块末尾触发。
- 合入主循环：MCP 工具与内置工具同处一个 `tool_registry`，agent 取 `get_tool_schemas()` 即自动含 MCP 工具；dispatch 走统一 `execute_tool`（tool_registry.py:57-93，含首半+尾四分之一的输出截断）。
- 运维入口 `/mcp` 命令（clawspring.py:1325-1418）：list（含 status_line 与工具计数）/ reload（全部或单个）/ add / remove。

### 1.7 设计意图小结

1. **连接是长寿命资源**：一次握手多次调用，stdio 子进程常驻；冷启动开销（npx/uvx 下载启动常要数秒）只付一次。
2. **崩溃恢复是调用路径的内建步骤**而非外部巡检：`call_tool` 前探活 + 透明 reconnect。
3. **失败被局部化**：连接失败→ERROR 态+错误信息；调用失败→错误字符串给模型；都不影响其它服务器和主循环。
4. **可诊断性**：stderr 尾部缓存、四态状态机、status_line、`/mcp` 命令构成完整观察面。

---

## 2. kivio 现状

### 2.1 已经做对/领先的部分

- **协议更新**：`MCP_PROTOCOL_VERSION = "2025-06-18"`（client.rs:18），实现了 **streamable HTTP** 传输：`mcp-session-id` 头维护（client.rs:236-241、307-311）、`Accept: application/json, text/event-stream`（client.rs:299-302）、POST 响应可以是 JSON 也可以是 SSE 流并按 JSON-RPC id 精确配对（`read_sse_json_rpc_response` client.rs:331-360、`parse_sse_json_rpc` client.rs:488-498、`json_rpc_id_matches` client.rs:528-541 同时兼容字符串/数字 id）。比 clawspring 的旧 SSE 双端点协议先进一代。
- **工具元数据利用更深**（types.rs）：
  - `ChatToolDefinition`（types.rs:5-20）统一承载 native/skill/mixer/mcp 四类工具，保留 `annotations` 与 `output_schema`；
  - `mcp_tool_requires_confirmation`（types.rs:170-185）：按 `destructiveHint / openWorldHint / readOnlyHint` 注解决定是否敏感，注解缺失再回落 `looks_sensitive_tool` 动词启发（types.rs:814-823）；`annotation_bool` 还兼容 snake_case 注解键（types.rs:146-153）；
  - `is_read_only_tool`（types.rs:55-73）：`readOnlyHint=true` 且非 destructive 且非 openWorld 才算只读——这直接驱动 agent 循环里 MCP 只读工具的**并行执行**（loop_.rs:1512-1529 `tool_call_parallel_eligible`，loop_.rs:998-1090 并行批次调度），clawspring 注册时统一 `concurrent_safe=False`，没有这层。
- **审批策略联动**：`tool_requires_approval`（execute.rs:302-325）对 MCP 工具按注解三级判定（destructive/openWorld/readOnly=false → 必须审批；readOnly=true → 免审批；否则看 sensitive），配合前端审批 UI。clawspring 完全没有审批概念。
- **入参 schema 校验**：`validate_tool_arguments`（execute.rs:329-331 起，递归实现到 execute.rs:400+，支持 anyOf/oneOf/enum/items/数值范围），调用前校验并把违规信息回喂模型重试（execute.rs:133-143）。clawspring 不校验入参。
- **结果结构化**：`McpToolCallResult` 保留 `raw / structured_content / artifacts`（types.rs:76-86），`parse_tool_result` 透传 `structuredContent`（client.rs:543-577）。
- **工具级超时 + 取消**：`execute_tool_call` 用 `tokio::select!` 同时等待超时与会话级取消信号（execute.rs:166-180），取消语义贯穿到 ToolCallRecord 状态机并实时 emit 给前端（execute.rs:131、163、220）。
- **enabled_tools 子集过滤**：每服务器可勾选启用哪些工具（settings.rs:612-624 `ChatMcpServer.enabled_tools`；registry.rs:380-389 过滤）。
- **工具列表缓存**：5 分钟 TTL，cache key 由 chat_tools/memory/image_generation 等设置序列化而成（registry.rs:29、194-245、407-417；state.rs:233-256），设置一变 key 即变，天然失效。
- **配置导入**：`chat_mcp_import_json`（registry.rs:263-309）一次性导入 Cursor/Claude Code 格式 mcp.json，`normalize_imported_transport`（registry.rs:392-405）把 http/sse/带 url 的统一归一为 `streamable_http`。
- **连接测试**：`chat_mcp_test_server`（registry.rs:247-261）独立 Tauri command，前端设置页直接试连并列出工具（src/api/tauri.ts:1085-1093；main.rs:320-322 注册）。
- **测试覆盖**：client.rs 内置手写 TCP 假 MCP 服务器测 JSON 与 SSE 双路径（client.rs:664-840）；types.rs 覆盖命名/敏感性/注解优先级（types.rs:826-1054）。

### 2.2 核心短板：无连接生命周期

**每次调用都全新建连，是 kivio MCP 子系统最大的结构性问题**：

- `StdioMcpClient::call_tool`（client.rs:63-79）和 `list_tools`（client.rs:53-61）每次都走 `self.connect()`（client.rs:81-125）：**spawn 新子进程 → initialize 握手 → notifications/initialized → 发一个请求 → `StdioSession` Drop 时 `start_kill()` 杀进程**（client.rs:39-43）。一个 10 轮 agent 任务若每轮调一次同服务器工具，要冷启动同一个服务器进程 10 次；对 `npx -y xxx` 型服务器每次都是秒级开销。
- `StreamableHttpMcpClient` 同样每次 `call_tool` 前重新 `initialize()`（client.rs:151-168、170-192），session id 用完即弃，不在请求间复用。
- 因此**没有也不需要**崩溃恢复/探活/重连——但这是用「最差性能」换来的「无状态简单性」。clawspring 的 alive 探测 + 透明 reconnect 在 kivio 中没有任何对应物。
- stderr 被直接丢弃：`command.stderr(Stdio::null())`（client.rs:102）。MCP 服务器起不来时用户只能看到 "MCP stdio read timed out"，看不到服务器自己打的错误（对比 clawspring client.py:72-80、127-129）。
- 无服务器状态机/状态面板：没有 connected/error 状态可查，前端只有一次性的 test_server。
- 不读 `initialize` 返回的 `capabilities`，无条件打 `tools/list`（对比 clawspring client.py:354 的 capabilities 检查）；`serverInfo` 也未保存。

### 2.3 工具命名与冲突处理现状

- 命名：`tool_definition_from_mcp`（types.rs:121-144）生成 `id = mcp__{server.id}__{tool.name}`；`openai_tool_name`（types.rs:23-30）对 MCP 工具 sanitize 整个 id（native/skill/mixer 用裸名以对齐系统提示词），`sanitize_openai_tool_name`（types.rs:797-812）非 `[A-Za-z0-9_-]` 转 `_`、截 64 字符、去首尾 `_`。测试确认 `server.one` + `search.web` → `mcp__server_one__search_web`（types.rs:918-945）。
- 分发：**不需要反查原名**——`call_tool`（registry.rs:311-359）直接用 `tool.name`（原始名）调服务器，`server_id` 字段定位服务器，比 clawspring 的字符串三段解析 + 线性反查（client.py:496-516）更干净。
- 匹配：`match_tool_call`（execute.rs:42-49）按 `openai_tool_name() == function_name || tool.name == function_name` 找第一个命中。**隐患**：两个服务器暴露同名工具时，模型若回了裸名（部分模型会丢前缀），`tool.name` 分支命中先注册的那个，可能错路由。
- 去重：Anthropic 适配器按 openai_tool_name 去重（anthropic.rs:424-438 `seen` HashSet，先到先得静默丢弃）；**OpenAI 适配器不去重**（openai.rs:270-278 直接 map），同名（如两个 server.id sanitize 后相同 + 同工具名，或 64 字符截断撞名）会把重复 `function.name` 发给 API，部分供应商会 4xx。两侧行为不一致且都无告警。

### 2.4 调用路径（chat/agent/ → mcp/registry）

- 工具列表：`list_tools_for_chat`（commands.rs:2922-2939）→ `list_enabled_tool_defs`（registry.rs:194-245）：native 内置 + mixer 生图 + 逐个 enabled MCP 服务器 `list_server_tools`（**串行 await**，registry.rs:223-239；单服务器失败仅 eprintln 吞掉，registry.rs:231-237）+ skill 工具，结果进缓存。
- 执行：agent 循环（loop_.rs:998-1090 串行/并行批次）→ `execute_tool_call`（execute.rs:102-222：校验→审批→超时/取消竞速→状态记录）→ `RegistryToolExecutor::call`（commands.rs:3417-3459）→ `mcp::registry::call_tool`（registry.rs:311-359：按 source 分流 native/skill/mixer/mcp，MCP 按 transport 现场 new 一个 client）。
- 死代码：registry.rs:365-367 `list_server_tools` 开头有一个空 if 块（仅注释），应清理。
- 结果转换缺陷：`parse_tool_result`（client.rs:550-568）`filter_map` 只取 `text` 与 `resource`，**MCP 返回的 image content block 被静默丢弃**（`artifacts: Vec::new()` 写死，client.rs:574）——clawspring 至少留 `[image: mime]` 占位（client.py:408-409），而 kivio 的 `ChatToolArtifact` 基建（types.rs:88-97，run_python 已在用）本可以直接承接 MCP 图片。

---

## 3. 差距分析

### 3.1 clawspring 有、kivio 缺失/粗糙

| # | 能力 | clawspring | kivio 现状 | 影响 |
|---|------|-----------|-----------|------|
| G1 | **持久连接（一次握手多次调用）** | StdioTransport 常驻子进程 + pending 表（client.py:19-129） | 每次调用 spawn→握手→杀进程（client.rs:63-125、39-43） | 每次工具调用多秒级冷启动；npx/uvx 服务器尤甚；HTTP 侧每次重复 initialize 往返 |
| G2 | **崩溃检测与透明重连** | `alive` 探活 + call 前 `reconnect()`（client.py:123-125、507-509） | 无（无状态设计使其无意义，但代价是 G1） | 长会话内服务器状态（如登录态、内存缓存）每次调用全部丢失 |
| G3 | **stderr 诊断缓存** | 尾部 20 行（client.py:72-80、127-129） | `Stdio::null()`（client.rs:102） | 服务器启动失败只能看到超时，排障困难 |
| G4 | **服务器状态机 + 运行时状态面板** | 四态 + status_line + `/mcp` 命令（types.py:66-70；client.py:421-438；clawspring.py:1325-1418） | 仅一次性 test_server；运行期 list 失败只 eprintln（registry.rs:231-237） | 用户/前端无法知道哪个服务器挂了、为什么挂 |
| G5 | **capabilities 协商** | initialize 后记录 capabilities，无 tools 能力则跳过 tools/list（client.py:323-327、354） | initialize 结果丢弃（client.rs:363-377 只看成功与否） | 对不支持 tools 的服务器打无效请求；serverInfo 无法展示 |
| G6 | **项目级 `.mcp.json` 文件配置** | 用户级+项目级双层、目录向上回溯、与 Claude Code 格式互通（config.py:34-70） | 配置只存 settings.json（settings.rs:612-624），import 是一次性拷贝（registry.rs:263-309） | 项目随仓库共享 MCP 配置的工作流缺失（对 kivio 的"项目对话"模式正好契合） |
| G7 | **启动期后台预连接** | import 时 daemon 线程 connect_all（tools.py:120-131） | 首次对话才串行连接所有服务器（registry.rs:223-239） | 首条消息延迟被全部服务器握手时间之和拖长（且是串行） |
| G8 | **失败错误回传给模型时的来源标注** | `[MCP tool error]` 前缀 + `[MCP:{server}]` description 前缀（client.py:416；types.py:89） | description 不标注服务器来源（types.rs:127-131 仅空描述兜底） | 多服务器同名工具时模型难以区分语义（小问题） |

### 3.2 kivio 有、clawspring 没有

- **Streamable HTTP（2025-06-18 协议）+ SSE 响应解析 + session-id 管理**（client.rs:18、220-323、331-360）：clawspring 还是 2024-11-05 旧 SSE 协议。
- **注解驱动的安全模型**：readOnly/destructive/openWorld → 敏感性、审批、并行三套策略（types.rs:55-73、170-185；execute.rs:302-325；loop_.rs:1512-1529）。
- **入参 JSON Schema 校验 + 违规反馈重试**（execute.rs:133-143、329+）。
- **工具调用全生命周期 UI 事件流**：ToolCallRecord 状态机（Pending/Running/Success/Error/Skipped/Cancelled）+ trace/span id 实时 emit（execute.rs:112-131、162-221）。
- **超时与会话级取消的竞速**（execute.rs:166-180），clawspring 只有阻塞 event.wait。
- **MCP 只读工具并行批量执行**（loop_.rs:998-1090）。
- **enabled_tools 服务器内工具子集**（registry.rs:380-389）、**工具列表 TTL 缓存且 key 绑定设置**（registry.rs:29、194-245）、**structuredContent 透传**（client.rs:548、types.rs:84-86）、**Cursor 格式导入**（registry.rs:263-309）。

结论：kivio 在「调用一次工具的质量」（安全、校验、可观测、并行、取消）上全面领先；clawspring 在「连接作为资源的生命周期管理」上全面领先。重构应只移植后者，不要动前者。

---

## 4. 重构建议

### P0 — 持久连接管理器（移植 clawspring MCPManager 模式，工作量：大，约 3-5 天）

新建 `src-tauri/src/mcp/manager.rs`，挂到 `AppState`：

```
pub struct McpConnectionManager {
    sessions: tokio::sync::Mutex<HashMap<String /*server_id*/, McpSession>>,
}
pub struct McpSession {
    config_fingerprint: String,      // server 配置序列化 hash，配置变更即重建
    state: McpServerState,           // Connecting/Connected/Error{msg}/Disconnected
    transport: McpTransport,         // Stdio{child, pending, ...} | Http{session_id}
    server_info: Value, capabilities: Value,
    tools: Vec<McpTool>,             // 握手后缓存
    stderr_tail: VecDeque<String>,   // 尾部 ~20 行
    last_used: Instant,              // 空闲回收
}
```

- **Stdio 改造**（client.rs:31-43、362-437 重写）：`StdioSession` 从「borrow &mut self 顺序读写」改为 clawspring 的 pending 表模式——spawn 一个 reader task 持续 `lines.next_line()`，按 id 完成 `HashMap<u64, oneshot::Sender<Value>>` 中的 pending（对应 client.py:53-70）；请求方 `oneshot::Receiver` + `tokio::time::timeout`。同时 `stderr(Stdio::piped())` + stderr reader task 写 `stderr_tail`（对应 client.py:72-80）。这也顺带让单个 stdio 服务器支持并发请求（当前 `&mut session` 只能串行）。
- **按需连接 + 探活重连**（对应 client.py:507-509）：`manager.call_tool(server, name, args)` 入口检查 `child.try_wait()`/管道关闭 → 透明 reconnect 一次再调用；失败则置 `Error` 态并返回含 stderr_tail 的错误。
- **HTTP 侧**：复用 `mcp-session-id`，仅在 404/`session not found` 时重新 initialize（当前 client.rs:151-168 的每调用 initialize 删除）。
- **接线**：`registry.rs:311-359 call_tool` 与 `registry.rs:361-390 list_server_tools` 改为走 manager；`enabled_tools_cache_key` 缓存可保留作为 ChatToolDefinition 层缓存，但工具原始列表由 session 持有。
- **Tauri 约束**：manager 放 `AppState`（`Mutex<HashMap>` 用 tokio Mutex，因为持锁期间要 await 握手；或 per-session `Arc<Mutex<McpSession>>` 减小锁粒度）；app 退出钩子里 `disconnect_all` 杀子进程（clawspring 靠 daemon 线程自动死，Tauri 需显式 `start_kill`，并给 Child 配 `kill_on_drop(true)` 兜底）。

### P0 — MCP image content 不再丢弃（工作量：小，0.5 天）

`parse_tool_result`（client.rs:543-577）：content block `type=="image"` 时取 `data`/`mimeType` 填入 `artifacts`（`ChatToolArtifact` 已存在且前端已支持渲染 run_python 的 artifacts），文本侧留 `[image: mime]` 占位（对齐 clawspring client.py:408-409 并超越它）。

### P1 — 服务器状态事件与设置页状态面板（工作量：中，1-2 天）

- manager 状态变更（connected/error/reconnecting）通过 `app.emit("mcp-server-state", {...})` 推前端（对应 clawspring status_line / `/mcp`，clawspring.py:1386-1418）；
- 新增 `chat_mcp_server_status` command 返回各服务器 `{state, serverInfo, toolCount, lastError, stderrTail}`；设置页 MCP 列表显示实时状态点 + 错误详情 + 「重连」按钮（调 manager.reload_server，对应 client.py:530-534）。
- `list_enabled_tool_defs`（registry.rs:223-239）单服务器失败从 eprintln 升级为状态事件，让失败可见。

### P1 — 启动期并行预热（工作量：小，0.5 天）

对应 tools.py:120-131：app setup 或首个 chat 窗口创建时 `tokio::spawn` 对 enabled 服务器执行 `futures::join_all(connect + list_tools)`（**并行**，优于 clawspring 的串行 connect_all 与 kivio 现在的串行 list）。失败只记状态不阻塞。同时把 registry.rs:223-239 的运行期串行 list 也改并行。

### P1 — 命名冲突治理（工作量：小，0.5-1 天）

- OpenAI 适配器补去重（openai.rs:270-278，对齐 anthropic.rs:424-438），且冲突时不要静默丢弃：第二个起加 `_2` 后缀或记 warning 事件；
- `match_tool_call`（execute.rs:42-49）的裸名回退分支限定为非 MCP 工具（native/skill 才允许裸名匹配），消除跨服务器同名工具误路由；
- `sanitize_openai_tool_name` 64 字符截断（types.rs:810）改为「截断后若冲突则尾部替换为 hash 短码」。

### P2 — capabilities/serverInfo 落地（工作量：小，0.5 天）

握手返回值存入 McpSession（对应 client.py:323-327）；无 `tools` capability 时跳过 tools/list（对应 client.py:354）；serverInfo 展示在状态面板。顺带清理 registry.rs:365-367 死代码 if 块。

### P2 — 项目级 `.mcp.json` 支持（工作量：中，1-2 天）

对应 config.py:49-70：kivio 已有「项目对话」概念（registry.rs:684-708 `resolve_native_workspace` 解析 project root），可在 project root 下发现 `.mcp.json` 并以只读叠加方式合入该项目对话的 MCP 服务器列表（默认 disabled，需用户在设置中确认启用以避免仓库内配置静默执行任意命令——这一点要比 clawspring 更谨慎，clawspring 是直接信任）。

### 不建议移植

- clawspring 的 `mcp__server__tool` 字符串三段解析 + 反查原名（client.py:496-516）：kivio 的 `server_id` + 原名直调（registry.rs:331-358）更好；
- clawspring 的旧 SSE 双端点传输：kivio 的 streamable HTTP 已覆盖且更新；
- clawspring 注册即全局可见、无 enabled_tools/审批/校验的扁平注册表：kivio 现有安全分层应保持。
