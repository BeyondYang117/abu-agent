# ABU API 集成开发清单

## Phase 1: 认证基础 ✅

### Tauri Commands (Rust)
- [x] `src-tauri/src/abu_api.rs` - ABU API 相关命令
  - [x] `get_device_fingerprint()` - 生成设备指纹
  - [x] `get_hostname()` - 获取主机名
  - [x] `get_platform()` - 获取平台标识
  - [x] `get_client_version()` - 获取客户端版本
  - [x] `get_default_device_name()` - 生成默认设备名
  - [x] `open_external(url)` - 打开外部 URL
  - [x] `save_abu_api_config()` - 保存配置
  - [x] `load_abu_api_config()` - 加载配置
  - [x] `clear_abu_api_session()` - 清除会话

### Settings 扩展
- [x] `src-tauri/src/settings_abu_api.rs` - Settings 结构体扩展
  - [x] `abu_api_base_url: Option<String>`
  - [x] `abu_api_session_token: Option<String>`
  - [x] `abu_api_device_id: Option<String>`
  - [x] `runtime_mode: RuntimeMode` (Cloud | Local)

### Frontend API Client
- [x] `src/api/abuApi.ts` - ABU API 客户端
  - [x] Device Code Flow (createDeviceAuthorization / exchangeDeviceAuthorization)
  - [x] 密码登录 (loginWithPassword / loginWith2FA)
  - [x] 设备管理 (registerDevice / listDevices / revokeDevice)
  - [x] 模型列表 (listModels)
  - [x] Session 管理 (createSession / createRelaySession)
  - [x] Task 管理 (createTask / listTasks / getTaskUsage / etc.)

- [x] `src/api/abuApiAuth.ts` - 认证状态管理
  - [x] Zustand store (useAbuApiAuth)
  - [x] completeLogin()
  - [x] registerOrUpdateDevice()
  - [x] validateAuth()

### UI Components
- [x] `src/onboarding/steps/LoginStep.tsx` - 登录步骤组件
  - [x] Device Code Flow UI
  - [x] 密码登录 UI
  - [x] 模式切换
  - [x] 错误处理

- [x] `src/styles/abuApi.css` - ABU API 相关样式

## Phase 2: 引导流程改造 ⏳

### Onboarding 流程
- [ ] 更新 `src/onboarding/types.ts`
  - [ ] 添加 'login' 步骤到 ONBOARDING_STEPS
  - [ ] 更新 OnboardingStepId 类型

- [ ] 更新 `src/onboarding/OnboardingShell.tsx`
  - [ ] 导入 LoginStep 组件
  - [ ] 在 Provider 步骤前插入 Login 步骤
  - [ ] 添加登录状态检测（已登录自动跳过）
  - [ ] 传递 abuApiBaseUrl prop

- [ ] 简化 Provider 步骤
  - [ ] 方案 A：完全移除（推荐）
  - [ ] 方案 B：改为"选择默认模型"（从 `/api/agent/models` 获取）

- [ ] 更新 i18n 字符串
  - [ ] `src/settings/i18n.ts` 添加所有登录相关翻译
  - [ ] onboardingLoginTitle
  - [ ] onboardingLoginDesc
  - [ ] onboardingLoginWithBrowser
  - [ ] onboardingLoginWaitingForApproval
  - [ ] ... (所有 LoginStep 中用到的 key)

### 启动逻辑
- [ ] 更新 `src/main.tsx` 或 `src/App.tsx`
  - [ ] 调用 `useAbuApiAuth.getState().initialize()`
  - [ ] 在 Chat 挂载前验证认证状态

## Phase 3: 模型与中转 ⏳

### Rust Provider Adapter
- [x] `src-tauri/src/chat/model/abu_api_provider.rs` - ABU API Provider
  - [x] 实现 LanguageModelProvider trait
  - [x] 配额检查 (check_quota)
  - [x] 心跳发送 (send_heartbeat)
  - [x] 流式请求 (stream)
  - [ ] 完整实现 SSE 流解析（重用现有 adapter 逻辑）

- [ ] 更新 `src-tauri/src/chat/model/mod.rs`
  - [ ] 导出 abu_api_provider 模块
  - [ ] 在 select_provider() 中添加 ABU API 分支

### Task 生命周期管理
- [ ] `src-tauri/src/chat/abu_api_task.rs` - Task 管理逻辑
  - [ ] prepare_conversation() - 创建 Session + Task
  - [ ] get_or_create_task() - 获取或创建 Task
  - [ ] finalize_task() - 更新 Task 状态
  - [ ] start_heartbeat_loop() - 后台心跳任务

- [ ] 集成到 `src-tauri/src/chat/commands/send.rs`
  - [ ] 在 run_agent_loop 前检查 runtime_mode
  - [ ] Cloud 模式：调用 prepare_conversation()
  - [ ] Local 模式：使用现有 Provider 选择逻辑

### Frontend 模型选择
- [ ] 更新 `src/chat/ModelSelector.tsx`
  - [ ] 检测 runtime_mode
  - [ ] Cloud 模式：从 abuApi.listModels() 获取列表
  - [ ] Local 模式：使用现有逻辑

## Phase 4: Settings 集成 ⏳

### 账户信息
- [ ] 更新 `src/settings/tabs/GeneralTab.tsx`
  - [ ] 顶部插入账户信息卡片
  - [ ] 显示用户名、邮箱、配额余额
  - [ ] "退出登录"按钮

### 设备管理页面
- [ ] 创建 `src/settings/tabs/DevicesTab.tsx`
  - [ ] 列出所有设备
  - [ ] 标记当前设备
  - [ ] 吊销设备操作
  - [ ] 实时更新 last_seen_at

- [ ] 更新 `src/settings/SettingsShell.tsx`
  - [ ] 添加 'devices' tab
  - [ ] 导入 DevicesTab 组件
  - [ ] 添加到 tabComponents 映射

### 使用统计
- [ ] 更新 `src/settings/tabs/UsageTab.tsx`
  - [ ] 检测 runtime_mode
  - [ ] Cloud 模式：从 abuApi.listTasks() 获取数据
  - [ ] 展示 prompt/output tokens 和消费金额
  - [ ] 按设备/时间筛选

### 运行模式切换
- [ ] 在 `GeneralTab.tsx` 添加模式切换器
  - [ ] Radio: Cloud / Local
  - [ ] 切换提示（数据不互通）
  - [ ] Local → Cloud: 提示登录
  - [ ] Cloud → Local: 提示配置 Provider

## Phase 5: 聊天体验优化 ⏳

### 配额显示
- [ ] 创建 `src/chat/TaskQuotaIndicator.tsx`
  - [ ] 显示当前 Task 的 token 使用量
  - [ ] 显示预估费用
  - [ ] 软上限警告（黄色）
  - [ ] 硬上限阻断（红色）

- [ ] 集成到 `src/chat/InputBar.tsx`
  - [ ] 底部显示 TaskQuotaIndicator
  - [ ] 硬上限时禁用输入

### 实时用量更新
- [ ] 创建 `src/chat/useTaskUsage.ts` hook
  - [ ] 轮询 `/api/agent/tasks/:id/usage`（每 5 秒）
  - [ ] 返回 soft_cap_exceeded / hard_cap_exceeded 状态
  - [ ] 缓存优化

- [ ] 在 `Chat.tsx` 中使用
  - [ ] 传递 taskId prop
  - [ ] 监听配额状态变化

### 错误处理
- [ ] 网络失败：显示重试 / 切换到本地模式
- [ ] Token 过期：自动跳转登录
- [ ] 配额耗尽：显示充值入口
- [ ] Task 取消：清理状态

## Phase 6: 文档与测试 ⏳

### 集成指南
- [x] `docs/ABU_API_INTEGRATION.md` - 总体方案
- [ ] `docs/ABU_API_QUICKSTART.md` - 快速开始
  - [ ] 如何部署 abu-api
  - [ ] 如何配置客户端
  - [ ] 常见问题排查

### 用户文档
- [ ] 更新 `README.md`
  - [ ] 添加 ABU API 集成说明
  - [ ] 云端模式 vs 本地模式对比表

- [ ] 创建 `docs/USER_GUIDE_ABU_API.md`
  - [ ] 登录流程图文教程
  - [ ] 设备管理教程
  - [ ] 配额管理教程

### 测试
- [ ] 单元测试
  - [ ] `abu_api.rs` 命令测试
  - [ ] `abuApi.ts` 客户端测试
  - [ ] `abuApiAuth.ts` 状态管理测试

- [ ] 集成测试
  - [ ] Device Code Flow 端到端
  - [ ] 登录 → 注册设备 → 创建 Task → 调用模型 → 登出

- [ ] E2E 测试（可选）
  - [ ] Onboarding 完整流程
  - [ ] 聊天配额耗尽场景
  - [ ] 网络失败恢复

## Phase 7: 部署与发布 ⏳

### 配置检查
- [ ] 默认 abu-api base URL
- [ ] 是否需要自定义域名支持
- [ ] CORS 配置检查

### 构建优化
- [ ] 确认 Rust 依赖（reqwest, serde_json, tokio-stream）
- [ ] 确认 Node 依赖（zustand, zustand/middleware）
- [ ] 打包体积检查

### 发布准备
- [ ] 版本号更新（遵循语义化版本）
- [ ] CHANGELOG 更新
- [ ] 迁移指南（现有用户如何升级）
- [ ] 回滚计划（如何切回本地模式）

## 关键决策点

### 决策 1：Provider 步骤处理
- **推荐方案**：完全移除 Provider 步骤
- **理由**：登录后自动获取模型列表，用户无需手动配置
- **备选**：改为"选择默认模型"页面

### 决策 2：本地模式保留
- **推荐方案**：保留本地模式作为 fallback
- **理由**：
  - 敏感场景（企业内网）可能无法连接外部 API
  - 用户已有 API Key 可继续使用
  - 降低迁移阻力

### 决策 3：配额上限设置
- **软上限默认值**：¥5 (500 分)
- **硬上限默认值**：¥20 (2000 分)
- **可配置**：Settings 中可调整

### 决策 4：心跳频率
- **建议频率**：5 分钟
- **abu-api 超时**：10 分钟无心跳标记为 failed
- **实现**：后台 tokio task，对话期间持续发送

## 依赖清单

### Rust Crates
```toml
[dependencies]
# 已有
reqwest = { version = "0.13", features = ["json", "stream"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"
anyhow = "1.0"
futures = "0.3"

# 新增（如果没有）
hostname = "0.4"
```

### Node Packages
```json
{
  "dependencies": {
    "zustand": "^4.5.0"
  }
}
```

## 里程碑

- **M1 (Phase 1-2)**: 认证与引导 - 用户可以登录并完成引导
- **M2 (Phase 3)**: 模型调用 - 云端模式可以正常聊天
- **M3 (Phase 4)**: Settings 完善 - 设备管理、使用统计就位
- **M4 (Phase 5)**: 体验优化 - 配额提示、错误处理完善
- **M5 (Phase 6-7)**: 发布准备 - 文档、测试、打包

## 风险与缓解

### 风险 1：abu-api 兼容性
- **缓解**：先在 fork 版本测试，确保接口稳定再集成

### 风险 2：网络延迟
- **缓解**：添加超时配置，支持自定义 base URL（可部署内网 abu-api）

### 风险 3：用户迁移阻力
- **缓解**：保留本地模式，分步引导，不强制切换

### 风险 4：配额计费争议
- **缓解**：
  - 显示详细用量（每条消息的 token 数）
  - 提供历史记录导出
  - 支持预充值 + 预警

## 后续扩展

### 云端同步（可选）
- 对话历史同步
- 设置同步
- 快捷键同步

### 团队共享（可选）
- 家庭套餐配额共享
- 企业级权限管理
- 使用报表

### 高级功能
- 模型性能分析（延迟、吞吐）
- 自动模型降级（配额不足时切换便宜模型）
- 预算告警推送

---

**当前进度**：Phase 1 完成 ✅，Phase 2-7 待实施

**建议下一步**：实施 Phase 2（引导流程改造），优先完成 Device Code Flow 的端到端体验。
