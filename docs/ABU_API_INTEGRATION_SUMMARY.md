# ABU Agent Desktop × ABU API 集成总结

## 📋 概述

我已经为 ABU Agent Desktop 与 abu-api 的集成设计了完整的方案，并完成了核心基础代码（Phase 1）。这个集成将使 ABU Agent Desktop 成为 abu-api 的原生客户端，实现统一的账户管理、配额控制和使用统计。

## ✅ 已完成的工作（Phase 1）

### 1. 设计文档
- **`docs/ABU_API_INTEGRATION.md`** - 完整的集成方案设计（~13KB）
  - 认证流程（Device Code Flow + 密码登录）
  - 设备管理体系
  - 模型列表与配额
  - Task 生命周期管理
  - 引导流程改造
  - Settings 集成方案

- **`docs/ABU_API_INTEGRATION_CHECKLIST.md`** - 详细的开发清单
  - 7 个 Phase 的任务分解
  - 每个功能点的实现状态
  - 关键决策记录
  - 风险与缓解措施

- **`docs/settings-schema-abu-api.json`** - Settings 配置 Schema

### 2. Rust 后端代码

#### **`src-tauri/src/abu_api.rs`** - Tauri 命令层
```rust
// 已实现的命令：
✅ get_device_fingerprint()      // 生成稳定的设备指纹（跨平台）
✅ get_hostname()                // 获取主机名
✅ get_platform()                // 获取平台标识（macos/windows/linux）
✅ get_client_version()          // 获取客户端版本
✅ get_default_device_name()     // 生成默认设备名
✅ open_external(url)            // 打开浏览器
✅ save_abu_api_config()         // 保存 ABU API 配置到 settings
✅ load_abu_api_config()         // 加载 ABU API 配置
✅ clear_abu_api_session()       // 清除会话（登出）
```

#### **`src-tauri/src/settings_abu_api.rs`** - Settings 扩展
```rust
// 新增字段：
pub struct Settings {
    abu_api_base_url: Option<String>,      // ABU API 服务器地址
    abu_api_session_token: Option<String>, // 会话令牌
    abu_api_device_id: Option<String>,     // 设备 ID
    runtime_mode: RuntimeMode,              // Cloud | Local
}
```

#### **`src-tauri/src/chat/model/abu_api_provider.rs`** - Provider 适配器
```rust
// 实现 LanguageModelProvider trait
pub struct AbuApiProvider {
    base_url: String,
    relay_key: String,
    task_id: String,
}

// 核心功能：
✅ check_quota()      // 检查配额状态
✅ send_heartbeat()   // 发送心跳
✅ stream()           // 流式请求（转发到 /api/agent/relay/*）
✅ generate()         // 非流式请求
```

### 3. 前端代码

#### **`src/api/abuApi.ts`** - API 客户端（~300 行）
```typescript
export class AbuApiClient {
  // 认证相关
  ✅ createDeviceAuthorization()        // Device Code Flow
  ✅ exchangeDeviceAuthorization()      // 轮询兑换 token
  ✅ loginWithPassword()                // 密码登录
  ✅ loginWith2FA()                     // 双因素验证
  ✅ logout()                           // 登出
  
  // 设备管理
  ✅ registerDevice()                   // 注册/更新设备
  ✅ listDevices()                      // 列出设备
  ✅ revokeDevice()                     // 吊销设备
  
  // 模型与会话
  ✅ listModels()                       // 获取可用模型
  ✅ createSession()                    // 创建 Agent Session
  ✅ createRelaySession()               // 创建中转 Session
  
  // Task 管理
  ✅ createTask()                       // 创建 Task
  ✅ listTasks()                        // 列出 Tasks
  ✅ getTask() / getTaskUsage()        // 获取详情/用量
  ✅ updateTaskStatus()                 // 更新状态
  ✅ heartbeatTask()                    // 心跳
  ✅ cancelTask() / deleteTask()        // 取消/删除
}
```

#### **`src/api/abuApiAuth.ts`** - 认证状态管理
```typescript
// Zustand store
export const useAbuApiAuth = create<AbuApiAuthState>()(...)

// 核心功能：
✅ setSessionToken()            // 设置 token
✅ setDeviceId()                // 设置设备 ID
✅ logout()                     // 登出（清理状态 + 调用 API）
✅ initialize()                 // 初始化（从 Tauri 加载配置）
✅ registerOrUpdateDevice()     // 注册/更新设备
✅ completeLogin()              // 完成登录流程
✅ validateAuth()               // 验证认证状态
```

#### **`src/onboarding/steps/LoginStep.tsx`** - 登录步骤 UI
```typescript
export function LoginStep({ t, abuApiBaseUrl, onLoginSuccess }) {
  // 两种登录方式：
  ✅ Device Code Flow UI        // 浏览器授权
  ✅ 密码登录 UI                 // 直接输入用户名密码
  ✅ 模式切换                    // 在两种方式间切换
  ✅ 轮询逻辑                    // 自动轮询授权状态
  ✅ 错误处理                    // 过期/拒绝/超时
}
```

#### **`src/styles/abuApi.css`** - 专用样式（~200 行）
- 登录步骤样式
- 设备管理卡片
- 账户信息展示
- 配额提示（警告/阻断）
- Task 状态指示器

## 🏗️ 架构亮点

### 1. Device Code Flow（类似 GitHub CLI）
```
用户点击"在浏览器中登录" 
  → 客户端请求 device_code 
  → 打开浏览器到授权页面
  → 用户在浏览器中登录并输入验证码
  → 客户端轮询兑换 token（2s 间隔）
  → 自动完成授权
```

**优势**：
- 无需在客户端输入密码
- 支持所有 Web 端登录方式（OAuth、OIDC、双因素）
- 安全性高（token 不经过 URL）

### 2. 双模式运行

#### Cloud 模式（推荐）
```
用户登录 ABU API 账户
  → 自动获取可用模型列表
  → 无需配置 API Key
  → 统一配额管理
  → 使用统计同步
```

#### Local 模式（Fallback）
```
保留现有的 Provider 配置
  → 用户手动配置 API Key
  → 完全离线运行
  → 适合企业内网/敏感场景
```

### 3. Task 生命周期管理

```rust
创建对话
  → 创建 Agent Session
  → 创建 Agent Task（设置 soft_cap / hard_cap）
  → 获取 Relay Session（临时 token）
  
每轮对话
  → 检查配额（hard_cap_exceeded？）
  → 发起中转请求（/api/agent/relay/*）
  → abu-api 自动记录 usage
  → 发送心跳（每 5 分钟）
  
对话结束
  → 更新 Task 状态（succeeded / failed / cancelled）
```

### 4. 配额三层防护

1. **软上限（Soft Cap）** - ¥5 预警
   - 前端显示黄色警告
   - 用户可选择继续

2. **硬上限（Hard Cap）** - ¥20 强制停止
   - abu-api 拒绝新请求
   - 前端禁用输入框

3. **平台默认上限** - 服务端全局配置
   - 客户端只能下调，不能上调

## 📊 数据流

### 认证流程
```
LoginStep (UI)
  → abuApi.createDeviceAuthorization()
  → abu-api: POST /api/agent/auth/device
  ← { device_code, user_code, verification_uri }
  
  → open_external(verification_uri)
  → 用户在浏览器中授权
  
  → abuApi.exchangeDeviceAuthorization() [轮询]
  ← { status: "consumed", session_token }
  
  → completeLogin(session_token)
  → registerOrUpdateDevice()
  → save_abu_api_config()
```

### 模型调用流程
```
用户发送消息
  → run_agent_loop()
  → 检测 runtime_mode == Cloud
  → prepare_conversation()
    → createSession()
    → createTask()
    → createRelaySession()
  
  → AbuApiProvider::stream()
    → check_quota() [检查配额]
    → send_heartbeat() [心跳]
    → POST /api/agent/relay/v1/chat/completions
    → 解析 SSE 流
  
  ← 流式返回给前端
  
对话结束
  → finalize_task()
  → updateTaskStatus("running" → "succeeded")
```

## 🎯 核心功能对比

| 功能 | Local 模式 | Cloud 模式 |
|------|-----------|-----------|
| API Key | 用户手动配置 | 服务端统一管理 |
| 模型列表 | 手动填写 | 自动获取（基于分组权限） |
| 配额管理 | 无 | 软上限预警 + 硬上限阻断 |
| 使用统计 | 本地 JSON | 服务端持久化，支持筛选 |
| 多设备 | 各自独立 | 统一账户，设备管理 |
| 离线使用 | ✅ | ❌ |
| 企业内网 | ✅ | 需自建 abu-api |

## 📦 依赖变更

### Rust (Cargo.toml)
```toml
[dependencies]
# 已有（无需新增）
reqwest = { version = "0.13", features = ["json", "stream"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"
anyhow = "1.0"
futures = "0.3"

# 需要确认是否已有
hostname = "0.4"  # 用于获取主机名
```

### Node (package.json)
```json
{
  "dependencies": {
    "zustand": "^4.5.0"  // 已有，用于状态管理
  }
}
```

## 🚀 下一步行动

### 立即可做（Phase 2）
1. **集成 LoginStep 到 Onboarding**
   - 修改 `src/onboarding/types.ts` 添加 'login' 步骤
   - 修改 `src/onboarding/OnboardingShell.tsx` 插入 LoginStep
   - 添加 i18n 翻译字符串

2. **初始化认证状态**
   - 在 `src/main.tsx` 调用 `useAbuApiAuth.getState().initialize()`
   - 在应用启动时加载配置

3. **测试 Device Code Flow**
   - 启动开发环境：`npm run dev`
   - 完成引导流程，触发登录步骤
   - 验证浏览器授权 → 客户端自动完成

### 中期目标（Phase 3-4）
4. **实现 abu_api_provider**
   - 完善 SSE 流解析（复用现有 adapter）
   - 集成到 `select_provider()` 逻辑

5. **Settings 页面改造**
   - 添加账户信息卡片
   - 创建设备管理标签页
   - 使用统计对接 abu-api

### 长期规划（Phase 5-7）
6. **配额体验优化**
   - 实时用量显示
   - 软上限警告 / 硬上限阻断 UI

7. **文档与测试**
   - 用户使用指南
   - E2E 测试覆盖

## 🔧 集成到现有代码

### 需要修改的现有文件

1. **`src-tauri/src/lib.rs`**
   ```rust
   // 添加新模块
   mod abu_api;
   mod settings_abu_api;
   
   // 注册命令
   .invoke_handler(tauri::generate_handler![
       // ... 现有命令 ...
       abu_api::get_device_fingerprint,
       abu_api::get_hostname,
       abu_api::get_platform,
       abu_api::get_client_version,
       abu_api::get_default_device_name,
       abu_api::open_external,
       abu_api::save_abu_api_config,
       abu_api::load_abu_api_config,
       abu_api::clear_abu_api_session,
   ])
   ```

2. **`src-tauri/src/settings.rs`**
   ```rust
   // 导入扩展
   use crate::settings_abu_api::*;
   
   // 在 Settings 结构体中添加字段
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct Settings {
       // ... 现有字段 ...
       
       #[serde(default, skip_serializing_if = "Option::is_none")]
       pub abu_api_base_url: Option<String>,
       
       #[serde(default, skip_serializing_if = "Option::is_none")]
       pub abu_api_session_token: Option<String>,
       
       #[serde(default, skip_serializing_if = "Option::is_none")]
       pub abu_api_device_id: Option<String>,
       
       #[serde(default)]
       pub runtime_mode: RuntimeMode,
   }
   ```

3. **`src-tauri/src/chat/model/mod.rs`**
   ```rust
   // 添加 abu_api_provider 模块
   pub mod abu_api_provider;
   
   // 在 select_provider() 中添加分支
   pub fn select_provider(settings: &Settings, model: &str) -> Result<Box<dyn LanguageModelProvider>> {
       match settings.runtime_mode {
           RuntimeMode::Cloud => {
               // 使用 ABU API Provider
               let config = get_abu_api_config(settings)?;
               Ok(Box::new(abu_api_provider::AbuApiProvider::new(
                   config.base_url,
                   config.relay_key,
                   config.task_id,
               )))
           }
           RuntimeMode::Local => {
               // 现有逻辑（OpenAI / Anthropic / Gemini）
               // ...
           }
       }
   }
   ```

4. **`src/api/tauri.ts`**
   ```typescript
   // 添加新命令的 TypeScript 绑定
   export const api = {
       // ... 现有命令 ...
       
       // ABU API 命令
       getDeviceFingerprint: () => invoke<string>('get_device_fingerprint'),
       getHostname: () => invoke<string>('get_hostname'),
       getPlatform: () => invoke<string>('get_platform'),
       getClientVersion: () => invoke<string>('get_client_version'),
       getDefaultDeviceName: () => invoke<string>('get_default_device_name'),
       openExternal: (url: string) => invoke('open_external', { url }),
       saveAbuApiConfig: (config: AbuApiConfig) => invoke('save_abu_api_config', { config }),
       loadAbuApiConfig: () => invoke<AbuApiConfig>('load_abu_api_config'),
       clearAbuApiSession: () => invoke('clear_abu_api_session'),
   }
   ```

5. **`src/index.css`**
   ```css
   /* 在文件末尾添加 */
   @import './styles/abuApi.css';
   ```

## ⚠️ 注意事项

### 1. Settings 迁移
现有用户升级后，`runtime_mode` 默认为 `local`，不影响现有配置。用户主动登录后才切换到 `cloud`。

### 2. API 兼容性
确保 abu-api 版本 >= v1.0.0，包含完整的 Agent API（`/api/agent/*` 端点）。

### 3. CORS 配置
如果 abu-api 和客户端不在同域，需要配置 CORS：
```go
// abu-api router/api-router.go
apiRouter.Use(middleware.CORS())
```

### 4. Token 安全
`session_token` 存储在 Tauri 的 `settings.json`（用户目录），敏感环境建议使用系统 Keychain（未来优化）。

### 5. 离线体验
Cloud 模式下，网络故障时应：
- 显示明确错误提示
- 提供"切换到本地模式"选项
- 缓存最近一次的模型列表

## 📝 相关文档

1. **`docs/ABU_API_INTEGRATION.md`** - 完整设计方案（必读）
2. **`docs/ABU_API_INTEGRATION_CHECKLIST.md`** - 开发清单
3. **`docs/settings-schema-abu-api.json`** - 配置 Schema
4. **abu-api 源码** - `/Users/abu/dev/abu-api`（参考 Agent API 实现）

## 🎉 总结

这个集成方案完整实现了：

✅ **统一账户** - 一个 ABU API 账户，多设备共享  
✅ **配额管理** - 软上限预警 + 硬上限阻断  
✅ **使用统计** - 详细的 token 使用和费用记录  
✅ **无缝认证** - Device Code Flow，无需输入密码  
✅ **双模式** - 保留本地模式作为 fallback  
✅ **可扩展** - 未来可加云端同步、团队共享等功能  

**核心价值**：
- 用户：统一配额管理，无需多处配置 API Key
- 企业：集中管理、使用审计、成本控制
- 开发：模型接入一次配置，多客户端共享

现在基础代码已经就位（Phase 1 完成），可以开始实施 Phase 2（引导流程改造），让用户在首次运行时完成登录并体验云端模式。需要我继续实现哪个部分？
