# Abu Agent Desktop 集成设计

## 目标

将现有 ABU Agent Desktop 改造为 Abu Agent Desktop，并以 `https://api.abuai.chat` 作为默认 abu-api 服务。桌面端通过 abu-api 完成设备授权登录、设备登记、可用模型读取、额度控制和云端模型 Relay；现有本地 Agent 配置、会话历史与第三方 Provider 保持兼容。

## 非目标

- 第一阶段不将 Agent 定义、系统提示词、工具权限、项目绑定或会话正文同步到 abu-api。
- 不移除第三方或本地 Provider。
- 不在客户端保存 abu-api 平台 API Key。
- 不实现账号密码登录入口；设备授权是唯一的桌面登录流程。

## 系统边界

### 桌面端

新增 Abu 集成模块，负责服务地址、桌面会话、设备指纹、设备 ID、授权轮询、模型缓存与 Relay 生命周期。React 只消费脱敏后的登录状态与错误信息；管理会话令牌由 Rust/Tauri 层持久化和注入。

保留现有 `Settings` Provider 模型，增加固定 id 为 `abu-api` 的受管 Provider。该 Provider 的 `baseUrl` 指向服务地址，模型列表来自 `/api/agent/models`，不得将 `session_token` 或 `relay_key` 写入通用 Provider `apiKeys`。

### abu-api

复用现有 Agent API：

1. `POST /api/agent/auth/device`，body 为可选的 `{device_name}`，返回 `{device_code, user_code, verification_uri, interval, expires_at}`。`verification_uri` 当前是相对路径 `/agent/authorize`，客户端必须拼接服务地址并追加 `?user_code=<user_code>`。
2. 用户在浏览器登录并批准验证码。客户端按 `interval` 秒轮询 `POST /api/agent/auth/device/exchange`，body 为 `{device_code}`：`202` 表示仍等待，`200` 返回 `{status, session_token}`，`409` 表示不可兑换或已兑换，`410` 表示过期。
3. 使用 `X-Abu-Session-Token` 调用 `POST /api/agent/devices` 登记或刷新设备，body 为 `{fingerprint, platform, client_version, device_name, capabilities}`。设备 ID 使用响应中的 `data.id`，不能由客户端自行生成替代。
4. 使用 `GET /api/agent/models` 获取 `{models, recommended}`。这是管理面接口，不是 Relay 的 `/v1/models`，模型缓存只保存模型名和展示元数据。
5. 每次云端运行先调用 `POST /api/agent/sessions?device_id=<id>`，再调用 `POST /api/agent/tasks`，body 为 `{session_id, device_id, type, soft_cap?, hard_cap?}`。任务响应中的 `data.id` 是后续所有心跳、事件、状态和 Relay 请求的任务 ID。
6. 对运行中的任务调用 `POST /api/agent/relay-session?device_id=<id>&task_id=<id>`，响应为 `{relay_key, expires_at, device_id, task_id}`。该 `aar_...` 凭证只绑定这一台设备和一个任务。
7. Relay URL 必须按 Provider API format 精确映射：`/api/agent/relay/v1/chat/completions`、`/api/agent/relay/v1/responses`、`/api/agent/relay/v1/messages`、`/api/agent/relay/v1beta/models/*path`。请求使用 `Authorization: Bearer <relay_key>` 和 `X-Abu-Agent-Task-ID: <task_id>`；不得让通用 Provider 的 `/models` 或路径拼接逻辑绕过 `/api/agent/relay` 前缀。
8. 任务控制面使用 `POST /api/agent/tasks/:id/heartbeat`、`POST /api/agent/tasks/:id/events`、`POST /api/agent/tasks/:id/status`、`POST /api/agent/tasks/:id/cancel` 和 `GET /api/agent/tasks/:id/usage`。事件必须带递增 `seq`、唯一 `client_event_id`、受限的 `type/payload`；状态只允许从 `running` 转为 `succeeded`、`failed` 或 `cancelled`。登出调用 `POST /api/agent/auth/logout` 并带 `X-Abu-Session-Token`。

当前服务端创建 session/task 的 POST 没有幂等键，且没有列出 sessions 或携带客户端 operation ID 的字段。桌面端仍为每次运行生成本地 operation ID 供本地日志关联，但安全重试只用于 GET；POST 超时视为“结果未知”，不得自动重复创建或猜测关联。客户端可用 `GET /api/agent/tasks?device_id=...` 展示服务端任务供用户确认，无法确认时停止创建并要求用户恢复。后续若服务端增加幂等键或 operation ID 字段，客户端再启用带键重试。

Relay 不是独立的聊天 API，而是 abu-api 现有 Agent Relay 入口的认证方式：服务端继续执行订阅、额度、限流、审计和上游 Provider 路由。

## 数据与安全

- `server_url` 默认 `https://api.abuai.chat`，高级设置可覆盖。URL 必须规范化并要求 HTTPS；仅显式开发配置可允许 localhost。
- `session_token` 只存 Rust 侧安全存储；不输出到日志、前端状态或设置导出文件。
- `relay_key` 只存运行时内存，不能进入 `ProviderConfig`、持久化设置、崩溃报告或前端 store。服务端 TTL 当前为 15 分钟，任务结束或失败后立即清除。
- 受管 Provider 的请求必须通过 Rust 侧的临时 credential context 注入 Relay key；现有模型适配器只接收请求级鉴权覆盖，不读取或改写 `apiKeys`。管理请求使用 session token，模型请求使用 relay key，两者生命周期和错误处理完全分离。
- 管理接口的 401 或设备撤销才清理本地 session 并回到未登录；Relay 请求的 401 先只刷新当前任务的 relay key，刷新失败且管理接口仍有效时显示任务错误，不登出用户。
- 402/额度上限终止当前任务并刷新服务端 usage；403 视接口区分账户不可用与设备撤销；409 按具体接口分类处理：授权兑换冲突停止轮询、任务已结束停止发送、凭证与任务不匹配视为客户端状态错误并重新获取凭证，不能统一盲目重试；网络超时仅对幂等 GET 自动重试。
- 登出不删除本地 Agent 配置或第三方 Provider，只清理 Abu 会话、设备运行态与受管 Provider 的云端模型缓存。

## UI 与品牌

- 用户可见产品名、窗口标题、安装包元数据、HTML title、设置与引导文案统一为 `Abu Agent Desktop`，品牌简称 `Abu`。
- 替换应用图标和 favicon。保留旧数据目录、settings 文件结构和旧字段兼容，升级不迁移或删除用户数据。
- 首次引导新增 Abu 登录步骤。用户可跳过并继续使用本地功能；选择 Abu Provider 运行云端 Agent 时必须登录。
- 设置新增 Abu 账户页：连接状态、账户摘要、设备信息、模型刷新、额度/用量入口、重新授权、退出登录和服务地址高级设置。

## 运行时流程

### 登录

应用启动读取本地会话并使用轻量请求验证。无会话时显示登录入口；点击后创建设备授权、打开浏览器授权页并按服务器 `interval` 轮询。成功后登记设备、刷新模型并 upsert `abu-api` Provider；推荐模型仅在当前默认模型为空或已是受管 Provider 时写入默认选择。

### 云端运行

用户选择 `abu-api` Provider 后，Rust 运行时确保有效设备和 Agent session，创建任务并取得 Relay key。请求层依现有 Provider API format 映射到 `/api/agent/relay` 路径，持续发送任务心跳和事件。任务完成、取消、额度中断或网络失败时更新平台任务状态并释放内存凭证。

### 恢复

Relay 过期时只为当前任务重新申请 Relay key，不重新登录；正在进行的流式请求不自动重放未知结果，交由任务恢复策略决定。session 失效时暂停新任务并提示重新授权。应用重启不恢复运行中的云端任务，启动时只读取并展示服务端任务状态，避免复用过期凭证；本地会话历史照常保留。

## 代码组织

- `src/abu/`：登录状态、设备授权 UI、账户设置与脱敏类型。
- `src-tauri/src/abu/`：HTTP 客户端、凭证存储、设备指纹、授权轮询和 Relay 适配；通过 `commands.rs` 暴露窄接口。
- `src/settings/`：受管 Provider 展示与选择逻辑。既有聊天/Agent 请求层只依赖 Provider 请求接口，不直接读取 token。
- `package.json`、`index.html`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`、图标资源和用户可见文案负责品牌元数据。

## 测试与验收

- Rust 单测：授权状态机、轮询间隔、服务地址校验、凭证不落日志、Relay header/path 构造、错误映射和登出清理。
- React 单测：登录/等待/成功/失败/过期状态、受管 Provider upsert、登出后保留本地配置。
- Mock HTTP 集成测试：完整设备授权到模型请求的 happy path，以及 401、403、402、409、超时和重复轮询。
- 最终执行现有前端单测、Rust 相关测试、TypeScript 检查和 UI 构建；不要求真实生产账号测试。

## 分阶段交付

1. 品牌元数据与 Abu 集成数据模型、凭证存储、设备授权登录。
2. 模型同步与受管 Provider、账户设置页和会话恢复。
3. Relay 请求适配、任务生命周期、心跳/事件/额度错误处理。
4. 全量测试、跨平台构建检查和升级兼容验证。
