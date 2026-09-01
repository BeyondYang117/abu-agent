# ABU Agent Desktop × ABU API 集成方案

## 概述

将 ABU Agent Desktop 改造为 abu-api 的原生客户端，通过 abu-api 的 Agent 认证体系实现统一账户管理、模型配额、使用统计和云端同步。

## 核心改造点

### 1. 认证与会话管理

#### 1.1 Device Code Flow（推荐用于桌面端）

abu-api 已实现完整的 Device Code 授权流程（`/api/agent/auth/device`），类似 GitHub CLI / AWS CLI：

```
客户端                          abu-api                         浏览器
   |                              |                              |
   |--POST /agent/auth/device---->|                              |
   |<----device_code, user_code---|                              |
   |                              |                              |
   |--打开浏览器 /agent/authorize-->|                             |
   |                              |<----用户登录并输入 user_code--|
   |                              |                              |
   |--轮询 /device/exchange------->|                              |
   |<----pending--------------------|                              |
   |--轮询 /device/exchange------->|                              |
   |<----session_token--------------|                              |
```

**优势**：
- 无需在客户端输入密码
- 支持所有 Web 端登录方式（密码、GitHub OAuth、Discord、OIDC、LinuxDO 等）
- 支持双因素认证
- 浏览器完成验证后自动返回客户端

**对应端点**：
- `POST /api/agent/auth/device` — 创建授权请求，返回 `device_code` / `user_code` / `verification_uri`
- `POST /api/agent/auth/device/exchange` — 轮询兑换 token（interval: 2s）
- `POST /api/agent/auth/device/approve` — Web 端批准授权（需 `UserAuth` 中间件）

#### 1.2 直接密码登录（备选）

对于需要纯命令行体验的用户：

- `POST /api/agent/auth/login` — 密码登录，返回 `session_token`
- `POST /api/agent/auth/login/2fa` — 双因素验证登录

#### 1.3 会话管理

- Token 格式：abu-api 的 session token（Redis 存储，支持多设备）
- 客户端存储：Tauri 的 `tauri-plugin-store`（已用于 settings.json）
- 传递方式：`X-Abu-Session-Token` header（已在 `AgentSessionAuth` 中间件支持）
- 登出：`POST /api/agent/auth/logout` — 撤销当前 token

### 2. 设备注册与管理

abu-api 的 Agent 设备管理体系（`agent_devices` 表）：

```rust
// 设备指纹：客户端生成稳定唯一标识
// 平台：darwin / win32 / linux
// 客户端版本：当前 ABU Agent Desktop 版本号
// 设备名：用户自定义或自动生成（如 "MacBook Pro - Abu"）
// Capabilities：可选，JSON 格式能力声明
```

**端点**：
- `POST /api/agent/devices` — 注册/更新设备（幂等，相同 fingerprint 自动更新）
- `GET /api/agent/devices` — 列出当前用户的所有设备
- `DELETE /api/agent/devices/:id` — 吊销设备

**集成到 Onboarding**：
在引导流程的 **Provider 步骤之前** 插入 **登录步骤**：

```
欢迎 → 登录（新增）→ 供应商 → Web 搜索 → 快捷键 → 完成
```

### 3. 模型列表与配额

#### 3.1 可用模型

- `GET /api/agent/models` — 返回用户所在分组可用的模型列表 + 推荐模型
- 逻辑：根据用户的 `group` 查询 `groups` 表的 `models` 字段（分组级模型权限）
- 过滤：自动排除图像/视频生成模型、纯 Responses 模型

**替代现有 Provider 配置**：
- 用户无需手动配置 API Key（服务端统一管理）
- 前端 ModelSelector 直接从 `/api/agent/models` 获取列表
- 推荐模型优先级：`gpt-4o-mini` → `gpt-4.1-mini` → `claude-3-5-sonnet` → `gpt-4o`

#### 3.2 中转请求

abu-api 提供完整的 `/api/agent/relay/*` 转发层（已实现）：

```
ABU Agent Desktop → /api/agent/relay/v1/chat/completions → abu-api → 上游模型
                                    ↓
                            自动计费、限流、配额扣除
```

**支持的端点**：
- `GET /v1/models` — OpenAI 兼容模型列表
- `GET /v1beta/models` — Gemini 模型列表
- `POST /v1/chat/completions` — OpenAI Chat Completions
- `POST /v1/responses` — OpenAI Responses
- `POST /v1/messages` — Anthropic Messages
- `POST /v1beta/models/*` — Gemini native

**认证**：通过 `AgentRelayAuth` 中间件（读取 Task 的 relay session token）

### 4. Task 与使用统计

#### 4.1 Task 模型

每次聊天对话 = 一个 Agent Task（`agent_tasks` 表）：

```rust
struct AgentTask {
    id: String,               // UUID
    user_id: i32,
    session_id: String,       // Agent Session ID
    device_id: String,        // 设备 ID
    task_type: String,        // "chat" / "code" / ...
    status: String,           // running / succeeded / failed / cancelled
    soft_cap: i32,            // 软上限（预警，¥分）
    hard_cap: i32,            // 硬上限（强制停止，¥分）
    consumed_quota: i32,      // 已消费配额（¥分）
    fail_reason: Option<String>,
    created_at: i64,
    updated_at: i64,
    finished_at: Option<i64>,
}
```

**生命周期**：
1. 对话开始前：`POST /api/agent/tasks` 创建 Task
2. 每轮对话前：检查 `hard_cap`，超过则拒绝
3. 每轮对话后：调用 `POST /api/agent/tasks/:id/events` 记录 token 使用量
4. 心跳：每 5 分钟调用 `POST /api/agent/tasks/:id/heartbeat`（防止僵尸任务）
5. 对话结束：`POST /api/agent/tasks/:id/status` 更新为终态

#### 4.2 使用统计

- `GET /api/agent/tasks/:id/usage` — 单任务用量（prompt/output tokens + 消费金额）
- `GET /api/agent/tasks?status=running` — 运行中的任务列表
- `GET /api/agent/tasks?device_id=xxx` — 按设备筛选

**前端展示**：
- 聊天窗口底栏：显示当前对话的 token 使用量和预估费用
- Settings → 使用统计：对接 `/api/agent/tasks` 获取历史记录

### 5. Relay Session（中转认证）

Task 创建后，客户端需要获取临时 relay token：

```
POST /api/agent/relay-session?device_id=xxx&task_id=xxx
→ { relay_key: "...", expires_at: 1234567890 }
```

然后在调用 `/api/agent/relay/*` 时携带：

```
Authorization: Bearer {relay_key}
```

**有效期**：Task 运行期间有效，Task 终止后自动失效

### 6. 引导流程改造

#### 原流程
```
欢迎 → 供应商 → Web 搜索 → 快捷键 → 完成
```

#### 新流程
```
欢迎 → 登录 → 供应商选择（可选/简化）→ Web 搜索 → 快捷键 → 完成
```

#### 6.1 登录步骤（新增）

**UI 组件**：`src/onboarding/steps/LoginStep.tsx`

```tsx
export function LoginStep({ t, onLoginSuccess }) {
  const [mode, setMode] = useState<'device' | 'password'>('device')
  
  // Device Code Flow
  const [deviceCode, setDeviceCode] = useState<string | null>(null)
  const [userCode, setUserCode] = useState<string | null>(null)
  const [verificationUri, setVerificationUri] = useState<string>('')
  const [polling, setPolling] = useState(false)
  
  // 1. 请求 device code
  const startDeviceFlow = async () => {
    const res = await api.agentAuthDevice({ device_name: getDeviceName() })
    setDeviceCode(res.device_code)
    setUserCode(res.user_code)
    setVerificationUri(res.verification_uri)
    
    // 2. 打开浏览器
    await api.openExternal(`https://your-abu-api.com${res.verification_uri}?code=${res.user_code}`)
    
    // 3. 开始轮询
    pollExchange(res.device_code)
  }
  
  const pollExchange = async (deviceCode: string) => {
    setPolling(true)
    const interval = setInterval(async () => {
      try {
        const res = await api.agentAuthExchange({ device_code: deviceCode })
        if (res.status === 'consumed') {
          clearInterval(interval)
          await saveSessionToken(res.session_token)
          onLoginSuccess(res.session_token)
        }
      } catch (err) {
        if (err.message.includes('expired')) {
          clearInterval(interval)
          setPolling(false)
        }
      }
    }, 2000)
  }
  
  return (
    <div className="onboarding-step">
      <h2>{t.onboardingLoginTitle}</h2>
      <p>{t.onboardingLoginDesc}</p>
      
      {mode === 'device' ? (
        <div className="onboarding-login-device">
          {!deviceCode ? (
            <Button onClick={startDeviceFlow}>
              {t.onboardingLoginWithBrowser}
            </Button>
          ) : (
            <div className="onboarding-login-waiting">
              <p>{t.onboardingLoginWaitingForApproval}</p>
              <div className="onboarding-login-code">{userCode}</div>
              <p className="text-sm">{t.onboardingLoginCodeHint}</p>
            </div>
          )}
        </div>
      ) : (
        <PasswordLoginForm onSuccess={onLoginSuccess} />
      )}
      
      <button onClick={() => setMode(mode === 'device' ? 'password' : 'device')}>
        {mode === 'device' ? t.onboardingLoginSwitchToPassword : t.onboardingLoginSwitchToDevice}
      </button>
    </div>
  )
}
```

#### 6.2 供应商步骤简化

登录后，用户已关联到服务端分组，无需手动配置 API Key：

- **选项 A（推荐）**：完全移除 Provider 步骤，直接使用服务端模型列表
- **选项 B**：保留为"选择默认模型"步骤，从 `/api/agent/models` 获取候选列表

### 7. Settings 集成

#### 7.1 账户信息

在 Settings → General 顶部添加：

```tsx
<SettingsGroup title="账户">
  <div className="settings-account-card">
    <div className="settings-account-avatar">
      {user.username.charAt(0).toUpperCase()}
    </div>
    <div className="settings-account-info">
      <div className="settings-account-name">{user.username}</div>
      <div className="settings-account-email">{user.email}</div>
      <div className="settings-account-quota">
        剩余额度: ¥{(user.quota / 100).toFixed(2)}
      </div>
    </div>
  </div>
  <Button onClick={handleLogout}>退出登录</Button>
</SettingsGroup>
```

#### 7.2 设备管理

Settings → 新增"设备"标签页（`DevicesTab.tsx`）：

```tsx
export function DevicesTab({ t, settings, onChange }) {
  const [devices, setDevices] = useState<AgentDevice[]>([])
  
  useEffect(() => {
    api.agentListDevices().then(setDevices)
  }, [])
  
  const handleRevoke = async (id: string) => {
    await api.agentRevokeDevice(id)
    setDevices(prev => prev.filter(d => d.id !== id))
  }
  
  return (
    <div className="settings-devices">
      {devices.map(device => (
        <div key={device.id} className="settings-device-card">
          <div className="settings-device-info">
            <div className="settings-device-name">{device.device_name}</div>
            <div className="settings-device-meta">
              {device.platform} · v{device.client_version}
            </div>
            <div className="settings-device-time">
              最后活跃: {formatRelativeTime(device.last_seen_at)}
            </div>
          </div>
          {device.id === currentDeviceId ? (
            <span className="settings-device-current">当前设备</span>
          ) : (
            <Button variant="ghost" onClick={() => handleRevoke(device.id)}>
              吊销
            </Button>
          )}
        </div>
      ))}
    </div>
  )
}
```

#### 7.3 使用统计

Settings → Usage 标签页改为对接 abu-api：

```tsx
const stats = await api.agentListTasks({ 
  limit: 100, 
  status: 'succeeded',
  device_id: currentDeviceId // 可选，筛选当前设备
})

// 计算总 token 和费用
const totalPromptTokens = stats.reduce((sum, task) => sum + task.prompt_tokens, 0)
const totalOutputTokens = stats.reduce((sum, task) => sum + task.output_tokens, 0)
const totalCost = stats.reduce((sum, task) => sum + task.consumed_quota, 0) / 100 // 分 → 元
```

### 8. 聊天改造

#### 8.1 对话创建时

```rust
// src-tauri/src/chat/commands/send.rs

async fn prepare_conversation(conversation_id: &str, app: &AppHandle) -> Result<()> {
    // 1. 检查是否已有 Task
    if let Some(task_id) = get_task_for_conversation(conversation_id).await? {
        return Ok(());
    }
    
    // 2. 创建 Session
    let session = create_agent_session(&app).await?;
    
    // 3. 创建 Task
    let task = create_agent_task(&app, CreateTaskRequest {
        session_id: session.id,
        device_id: get_current_device_id(&app)?,
        task_type: "chat".to_string(),
        soft_cap: 500,  // ¥5 软上限
        hard_cap: 2000, // ¥20 硬上限
    }).await?;
    
    // 4. 获取 Relay Session
    let relay_session = create_relay_session(&app, &task.id).await?;
    
    // 5. 存储关联
    save_conversation_task_mapping(conversation_id, task.id, relay_session.relay_key).await?;
    
    Ok(())
}
```

#### 8.2 模型调用时

```rust
// src-tauri/src/chat/model/abu_api_provider.rs

pub struct AbuApiProvider {
    base_url: String,       // https://your-abu-api.com/api/agent/relay
    relay_key: String,      // 从 Task 获取的临时 token
    task_id: String,
}

impl LanguageModelProvider for AbuApiProvider {
    async fn stream(&self, request: GenerateRequest) -> Result<impl Stream<Item = StreamPart>> {
        // 1. 检查配额（可选，服务端也会检查）
        let usage = self.get_task_usage(&self.task_id).await?;
        if usage.hard_cap_exceeded {
            return Err(Error::QuotaExhausted);
        }
        
        // 2. 调用中转端点
        let url = format!("{}/v1/chat/completions", self.base_url);
        let req = self.http_client.post(&url)
            .header("Authorization", format!("Bearer {}", self.relay_key))
            .json(&request);
        
        // 3. 流式返回（abu-api 会自动记录 usage）
        let stream = req.send().await?.bytes_stream();
        Ok(stream.map(|chunk| self.parse_sse(chunk)))
    }
}
```

#### 8.3 对话结束时

```rust
async fn finalize_conversation(conversation_id: &str, status: TaskStatus) -> Result<()> {
    if let Some(task_id) = get_task_for_conversation(conversation_id).await? {
        update_task_status(&task_id, UpdateTaskStatusRequest {
            from: "running",
            to: match status {
                TaskStatus::Success => "succeeded",
                TaskStatus::Failed => "failed",
                TaskStatus::Cancelled => "cancelled",
            },
            reason: None,
        }).await?;
    }
    Ok(())
}
```

### 9. 配额与限流体验

#### 9.1 软上限提醒

当 Task 接近/超过 `soft_cap` 时，前端显示警告：

```tsx
{task.soft_cap_exceeded && (
  <div className="chat-quota-warning">
    ⚠️ 本次对话已超过预设预算（¥{task.soft_cap / 100}），继续使用将产生额外费用
  </div>
)}
```

#### 9.2 硬上限阻断

当 Task 达到 `hard_cap` 时，abu-api 会拒绝新的 relay 请求：

```json
{
  "success": false,
  "message": "本次对话已达配额上限",
  "data": { "hard_cap_exceeded": true }
}
```

前端禁用输入框并显示提示：

```tsx
{task.hard_cap_exceeded && (
  <div className="chat-quota-exceeded">
    本次对话已达配额上限（¥{task.hard_cap / 100}），无法继续
    <Button onClick={handleRecharge}>充值</Button>
  </div>
)}
```

### 10. 离线模式（可选）

为了保留现有的"本地配置 API Key"能力（不依赖 abu-api），可以实现双模式：

#### 模式切换

Settings → General → 运行模式：

- **云端模式**（推荐）：使用 abu-api 账户，统一配额管理
- **本地模式**：使用本地配置的 Provider，完全离线

```rust
enum RuntimeMode {
    Cloud { session_token: String, device_id: String },
    Local { providers: Vec<ModelProvider> },
}
```

#### 条件路由

```rust
match get_runtime_mode(&app).await? {
    RuntimeMode::Cloud { session_token, .. } => {
        // 使用 AbuApiProvider
        let provider = AbuApiProvider::new(session_token, task_id);
        run_agent_loop_with_provider(provider).await
    }
    RuntimeMode::Local { providers } => {
        // 使用现有 OpenAI/Anthropic/Gemini 适配器
        let provider = select_provider_from_local_config(providers, model_name);
        run_agent_loop_with_provider(provider).await
    }
}
```

## 实施路线图

### Phase 1: 认证基础（1-2 天）

- [ ] 实现 Device Code Flow UI（`LoginStep.tsx`）
- [ ] 实现 Tauri 命令：`agent_auth_device` / `agent_auth_exchange` / `agent_logout`
- [ ] Session Token 存储（tauri-plugin-store）
- [ ] 设备注册（`agent_register_device`）

### Phase 2: 引导流程（1 天）

- [ ] 在 Onboarding 中插入 Login 步骤
- [ ] 简化/移除 Provider 步骤
- [ ] 登录状态检测与自动跳过

### Phase 3: 模型与中转（2-3 天）

- [ ] 实现 `AbuApiProvider`（Rust adapter）
- [ ] 对接 `/api/agent/models` 获取模型列表
- [ ] 对接 `/api/agent/relay/*` 中转请求
- [ ] Task 生命周期管理

### Phase 4: Settings 集成（1-2 天）

- [ ] 账户信息卡片
- [ ] 设备管理标签页
- [ ] 使用统计对接 abu-api

### Phase 5: 配额与体验优化（1 天）

- [ ] 软上限警告 UI
- [ ] 硬上限阻断 UI
- [ ] 实时 token 计数显示

### Phase 6: 测试与文档（1 天）

- [ ] E2E 测试（登录 → 聊天 → 退出）
- [ ] 错误处理（网络失败、Token 过期、配额耗尽）
- [ ] 用户文档更新

## API 端点清单

### 认证相关
- `POST /api/agent/auth/device` — 创建设备授权请求
- `POST /api/agent/auth/device/exchange` — 兑换 session token
- `POST /api/agent/auth/device/approve` — 批准授权（Web 端）
- `POST /api/agent/auth/login` — 密码登录
- `POST /api/agent/auth/logout` — 登出

### 设备管理
- `POST /api/agent/devices` — 注册设备
- `GET /api/agent/devices` — 列出设备
- `DELETE /api/agent/devices/:id` — 吊销设备

### 模型与中转
- `GET /api/agent/models` — 获取可用模型列表
- `POST /api/agent/sessions` — 创建 Agent Session
- `POST /api/agent/relay-session` — 创建中转 Session
- `GET /api/agent/relay/v1/models` — 模型列表（中转）
- `POST /api/agent/relay/v1/chat/completions` — Chat Completions（中转）
- `POST /api/agent/relay/v1/messages` — Anthropic Messages（中转）
- `POST /api/agent/relay/v1/responses` — OpenAI Responses（中转）

### Task 管理
- `POST /api/agent/tasks` — 创建 Task
- `GET /api/agent/tasks` — 列出 Tasks
- `GET /api/agent/tasks/:id` — 获取 Task 详情
- `GET /api/agent/tasks/:id/usage` — 获取 Task 用量
- `POST /api/agent/tasks/:id/events` — 追加事件（已废弃，中转自动记录）
- `POST /api/agent/tasks/:id/status` — 更新 Task 状态
- `POST /api/agent/tasks/:id/heartbeat` — 心跳
- `POST /api/agent/tasks/:id/cancel` — 取消 Task
- `DELETE /api/agent/tasks/:id` — 删除 Task

## 安全与隐私

1. **Session Token 存储**：使用 Tauri 的 secure storage（macOS Keychain / Windows Credential Manager）
2. **通信加密**：强制 HTTPS
3. **Token 过期**：abu-api 的 session 支持自动刷新
4. **设备吊销**：用户可在 Web 端或其他设备上远程吊销
5. **本地模式保留**：敏感场景可切换到完全离线的本地模式

## 用户体验优化

1. **无感知登录**：首次运行时自动唤起浏览器，Web 端登录后桌面端自动完成
2. **多设备同步**：对话历史、设置、快捷键可通过 abu-api 云端同步（可选功能）
3. **配额可视化**：实时显示当前对话的 token 使用和费用估算
4. **家庭/团队共享**：abu-api 支持分组管理，可实现家庭套餐共享配额
5. **离线回退**：网络故障时自动切换到本地模式（如果配置了本地 Provider）

## FAQ

**Q: 用户必须注册 abu-api 账户吗？**  
A: 推荐但非强制。可以保留"本地模式"作为 fallback，用户仍可手动配置 API Key 使用。

**Q: 如何处理现有用户的迁移？**  
A: 首次更新后检测到无 session token 时，弹出引导："使用 ABU API 账户以获得统一配额管理和使用统计"，提供"登录"和"继续使用本地模式"两个选项。

**Q: abu-api 的模型价格如何对接？**  
A: abu-api 已有完整的计费系统（`consumed_quota` 字段），客户端只需展示金额，无需关心单价计算。

**Q: 如何处理网络故障？**  
A: 客户端缓存最近一次的模型列表，网络故障时使用缓存；中转请求失败时显示明确错误并提示检查网络或切换到本地模式。

**Q: 支持自建 abu-api 吗？**  
A: 支持。Settings 中可配置自定义 abu-api 地址（默认官方服务器），企业用户可部署私有实例。

---

**下一步**：选择一个 Phase 开始实施，我可以帮你编写具体代码。
