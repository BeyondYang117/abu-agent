# ABU API 集成开发清单

> **状态核对**：2026-09-01，对照 `5014c79c` 的实际代码逐项验证过，不再依赖此前的文档描述。
>
> 图例：
> - `[x]` 已实现**且已接线**（有调用方，跑得到）
> - `[~]` 代码已写，但**没有任何调用方**，属于孤岛代码
> - `[ ]` 未开始

## ⚠️ 两处文档与实现不符（先看这里）

**1. Phase 3 的 Rust 代码是孤岛。**
`chat/abu_api_task.rs`（344 行）和 `chat/model/abu_api_provider.rs`（226 行）都已写完，
`chat/model/mod.rs:13-16` 也导出了符号，但全仓库 grep 不到任何调用方——
`prepare_conversation` / `finalize_task` / `create_abu_api_virtual_provider` /
`check_task_quota` / `send_task_heartbeat` 全部零引用。云端模式目前跑不通，
下一步的主要工作是接线，不是重写。

**2. 实现选了「虚拟 Provider」，不是文档写的「trait 实现」。**
`abu_api_provider.rs` 暴露的是 `create_abu_api_virtual_provider()`，构造一个
`ModelProvider` 塞进现有分发链路，而不是文档里设想的 `impl LanguageModelProvider`。
现有分发入口是 `generate_with_chat_provider()` / `stream_with_chat_provider()`
（`chat/model/mod.rs:55` 起，按 `provider.api_format_kind()` 分叉），
**没有** `select_provider()` 这个函数。文档里所有提到 `select_provider` 的方案需要重写。

**3. `settings_abu_api.rs` 是死文件。**
它自己定义了一份 `Settings` 和 `RuntimeMode` 枚举，但 `lib.rs` 只声明了 `pub mod abu_api`，
没有 `mod settings_abu_api`。真正生效的字段落在 `settings.rs:1679`，
且 `runtime_mode` 是 `String` 而非枚举。要么接线，要么删掉，别留着误导。

## Phase 1: 认证基础 ✅

### Tauri Commands (Rust)
- [x] `src-tauri/src/abu_api.rs` - ABU API 相关命令（已在 `lib.rs:783-790` 注册）
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
- [x] Settings 字段已落地，但**不在** `settings_abu_api.rs` 里（见上方说明 3）
  - [x] `abu_api_base_url: Option<String>`
  - [x] `abu_api_session_token: Option<String>`
  - [x] `abu_api_device_id: Option<String>`
  - [x] `runtime_mode` — 实际在 `settings.rs:1679`，类型是 `String`（`"cloud"` | `"local"`），默认 `"local"`
- [~] `src-tauri/src/settings_abu_api.rs` - 未在 `lib.rs` 声明，死文件；内含 `RuntimeMode` 枚举 + 单测
  - [ ] 决定：接线（改用枚举）或删除

### Frontend API Client
- [x] `src/api/abuApi.ts` - ABU API 客户端
  - [x] Device Code Flow (createDeviceAuthorization / exchangeDeviceAuthorization)
  - [x] 密码登录 (loginWithPassword / loginWith2FA)
  - [x] 设备管理 (registerDevice / listDevices / revokeDevice)
  - [x] 模型列表 (listModels)
  - [x] Session 管理 (createSession / createRelaySession)
  - [x] Task 管理 (createTask / listTasks / getTaskUsage / etc.)
  - 注：`listModels` / `createSession` / Task 系列**目前没有前端调用方**，等 Phase 3-5 接入

- [x] `src/api/abuApiAuth.ts` - 认证状态管理（127 行）
  - [x] Zustand store (useAbuApiAuth) + `abuApiAuthStore`
  - [x] completeLogin()
  - [x] registerOrUpdateDevice()
  - [x] validateAuth()

- [x] `src/api/tauri.ts` - 9 个 ABU API 命令的 TS 绑定已就位（`tauri.ts:1957-1970`）

### UI Components
- [x] `src/onboarding/steps/LoginStep.tsx` - 登录步骤组件（318 行）
  - [x] Device Code Flow UI
  - [x] 密码登录 UI
  - [x] 模式切换
  - [x] 错误处理

- [~] `src/styles/abuApi.css` - 文件存在，但 `src/index.css` **没有 import**，样式当前不生效
  - [ ] 在 `src/index.css` 加 `@import './styles/abuApi.css';`

## Phase 2: 引导流程改造 ✅（仅剩 Provider 步决策）

### Onboarding 流程
- [x] 更新 `src/onboarding/types.ts`
  - [x] 添加 'login' 步骤到 ONBOARDING_STEPS（在 welcome 之后、provider 之前）
  - [x] 更新 OnboardingStepId 类型

- [x] 更新 `src/onboarding/OnboardingShell.tsx`
  - [x] 导入 LoginStep 组件（`:12`）
  - [x] 在 Provider 步骤前插入 Login 步骤（`:282`）
  - [x] 添加登录状态检测（`:85-86`，`loginCompleted || isAuthenticated` 可跳过）
  - [x] 传递 abuApiBaseUrl prop（`:285`，回落到 `DEFAULT_ABU_API_BASE_URL`）
  - [x] 步骤标签 `onboardingStepLogin`（`:200`）

- [ ] 简化 Provider 步骤 — **未决策**，`ProviderStep.tsx` 仍是原样（45 行）
  - [ ] 方案 A：完全移除（文档推荐）
  - [ ] 方案 B：改为"选择默认模型"（从 `/api/agent/models` 获取）
  - 注：登录后仍会走一遍手动配置 Provider，与"无需再配置 API Key"的目标冲突

- [x] 更新 i18n 字符串
  - [x] `src/settings/i18n.ts:904-925` 共 22 个 `onboardingLogin*` key + `onboardingStepLogin`
  - [x] LoginStep 用到的 key 全部有定义（Title/Desc/WithBrowser/DeviceDesc/WaitingForApproval/
        CodeLabel/CodeHint/ReopenBrowser/Username/Password/占位符/Submit/LoggingIn/Starting/
        SwitchToPassword/SwitchToDevice/Denied/Expired/Timeout/EmptyCredentials/SkipStep）

### 启动逻辑
- [x] 更新 `src/main.tsx`
  - [x] 启动时 `api.loadAbuApiConfig()` → `abuApiAuthStore.updateFromSettings()`
  - [x] 失败降级为 `console.warn`，不阻塞挂载

## Phase 3: 模型与中转 ⏳（代码在，线没接）

### Rust Provider Adapter
- [~] `src-tauri/src/chat/model/abu_api_provider.rs`（226 行）— **零调用方**
  - [x] `create_abu_api_virtual_provider()` - 构造虚拟 ModelProvider（非 trait 实现，见说明 2）
  - [x] `check_task_quota()` - 配额检查
  - [x] `send_task_heartbeat()` - 心跳发送
  - [x] `TaskQuotaStatus` / `ABU_API_PROVIDER_ID`
  - [ ] SSE 流解析 — 走虚拟 Provider 后由现有 adapter 承担，需验证中转端点的响应格式能对上

- [~] 更新 `src-tauri/src/chat/model/mod.rs`
  - [x] 导出 abu_api_provider 模块（`:6`）与 5 个符号（`:13-16`）
  - [ ] **在分发链路里加分支** — 目标函数是 `generate_with_chat_provider()` 与
        `stream_with_chat_provider()`（`:55` 起），不是文档写的 `select_provider()`

### Task 生命周期管理
- [~] `src-tauri/src/chat/abu_api_task.rs`（344 行，已在 `chat/mod.rs:2` 声明）— **零调用方**
  - [x] `AbuApiTaskContext`（`:33`）
  - [x] `prepare_conversation()`（`:164`）- 创建 Session + Task
  - [x] `finalize_task()`（`:259`）- 更新 Task 状态
  - [ ] `get_or_create_task()` - 未实现
  - [ ] `start_heartbeat_loop()` - 未实现（`send_task_heartbeat` 有了，但没有后台循环驱动它）

- [ ] 集成到 `src-tauri/src/chat/commands/send.rs`
  - [ ] 在 run_agent_loop 前检查 runtime_mode
  - [ ] Cloud 模式：调用 prepare_conversation()
  - [ ] Local 模式：使用现有 Provider 选择逻辑
  - **这一项是打通云端模式的关键路径**

### Frontend 模型选择
- [ ] 更新 `src/chat/ModelSelector.tsx` — 无任何 abuApi 引用
  - [ ] 检测 runtime_mode
  - [ ] Cloud 模式：从 abuApi.listModels() 获取列表
  - [ ] Local 模式：使用现有逻辑

## Phase 4: Settings 集成 ⏳（未开始）

### 账户信息
- [ ] 更新 `src/settings/tabs/GeneralTab.tsx` — 零 abuApi / logout / runtime_mode 引用
  - [ ] 顶部插入账户信息卡片
  - [ ] 显示用户名、邮箱、配额余额
  - [ ] "退出登录"按钮

### 设备管理页面
- [ ] 创建 `src/settings/tabs/DevicesTab.tsx` — 文件不存在
  - [ ] 列出所有设备
  - [ ] 标记当前设备
  - [ ] 吊销设备操作
  - [ ] 实时更新 last_seen_at

- [ ] 更新 `src/settings/SettingsShell.tsx`
  - [ ] 添加 'devices' tab
  - [ ] 导入 DevicesTab 组件
  - [ ] 添加到 tabComponents 映射

### 使用统计
- [ ] `src/settings/tabs/UsageTab.tsx` — **文件不存在**（清单原写"更新"，实际需新建；
      现有用量展示在别处，接入前先确认落点）
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

## Phase 5: 聊天体验优化 ⏳（未开始）

### 配额显示
- [ ] 创建 `src/chat/TaskQuotaIndicator.tsx` — 文件不存在
  - [ ] 显示当前 Task 的 token 使用量
  - [ ] 显示预估费用
  - [ ] 软上限警告（黄色）
  - [ ] 硬上限阻断（红色）

- [ ] 集成到 `src/chat/InputBar.tsx`
  - [ ] 底部显示 TaskQuotaIndicator
  - [ ] 硬上限时禁用输入

### 实时用量更新
- [ ] 创建 `src/chat/useTaskUsage.ts` hook — 文件不存在
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

## Phase 6: 文档与测试 ⏳（仅设计文档就位）

### 集成指南
- [x] `docs/ABU_API_INTEGRATION.md` - 总体方案
- [x] `docs/ABU_API_INTEGRATION_SUMMARY.md` - 集成总结
      ⚠️ 内含已过时的 `select_provider` / `settings_abu_api` 接线方案，需按说明 2、3 修订
- [x] `docs/settings-schema-abu-api.json` - 配置 Schema
- [ ] `docs/ABU_API_QUICKSTART.md` - 不存在
  - [ ] 如何部署 abu-api
  - [ ] 如何配置客户端
  - [ ] 常见问题排查

### 用户文档
- [ ] 更新 `README.md` — 当前 0 处 abu-api / ABU API 提及
  - [ ] 添加 ABU API 集成说明
  - [ ] 云端模式 vs 本地模式对比表
- [ ] `README.en.md` 同步

- [ ] 创建 `docs/USER_GUIDE_ABU_API.md` - 不存在
  - [ ] 登录流程图文教程
  - [ ] 设备管理教程
  - [ ] 配额管理教程

### 测试
- [ ] 单元测试
  - [ ] `abu_api.rs` 命令测试
  - [x] `settings_abu_api.rs` 有 RuntimeMode 序列化测试 — 但文件未编译进 crate，测试跑不到
  - [ ] `abuApi.ts` 客户端测试 — 无 `abuApi.test.ts`
  - [ ] `abuApiAuth.ts` 状态管理测试
  - 注：`src/api/` 下同类模块都带 `.test.ts`，两个 abuApi 文件是唯一例外

- [ ] 集成测试
  - [ ] Device Code Flow 端到端
  - [ ] 登录 → 注册设备 → 创建 Task → 调用模型 → 登出

- [ ] E2E 测试（可选）
  - [ ] Onboarding 完整流程
  - [ ] 聊天配额耗尽场景
  - [ ] 网络失败恢复

## Phase 7: 部署与发布 ⏳（未开始）

### 配置检查
- [x] 默认 abu-api base URL（`DEFAULT_ABU_API_BASE_URL`，`5014c79c` 已更新）
- [x] 自定义域名支持（`settings.abu_api_base_url` 可覆盖默认值）
- [ ] CORS 配置检查

### 构建优化
- [ ] 确认 Rust 依赖（reqwest, serde_json, tokio-stream）
- [ ] 确认 Node 依赖（zustand, zustand/middleware）
- [ ] 打包体积检查
- [ ] `cargo build` / `npm run typecheck` 未在本轮跑过，孤岛代码可能有编译告警

### 发布准备
- [ ] 版本号更新（遵循语义化版本）
- [ ] CHANGELOG 更新
- [ ] 迁移指南（现有用户如何升级）
- [ ] 回滚计划（如何切回本地模式）

## 下一步建议

按依赖顺序，优先级从高到低：

1. **接线 Phase 3**（`send.rs` + `mod.rs` 分发分支）— 唯一能让云端模式从"代码存在"
   变成"能用"的一步，已写的 570 行 Rust 才有意义
2. **补 `start_heartbeat_loop()` + `get_or_create_task()`** — Task 生命周期缺这两块不闭环
3. **两处一行修复**：`index.css` 导入 abuApi.css；处理 `settings_abu_api.rs`（接线或删）
4. **决策 Provider 步**（Phase 2 尾巴）— 直接影响"登录后无需配置 API Key"的体验承诺
5. Phase 4 Settings，再 Phase 5 配额 UI
6. 修订 SUMMARY 文档里过时的接线方案，避免下一轮再被带偏

## 关键决策点

### 决策 1：Provider 步骤处理 —— **仍未决策**
- **推荐方案**：完全移除 Provider 步骤
- **理由**：登录后自动获取模型列表，用户无需手动配置
- **备选**：改为"选择默认模型"页面
- **现状**：`ProviderStep.tsx` 未改动，登录后仍走手动配置

### 决策 2：本地模式保留 —— **已落地**
- **推荐方案**：保留本地模式作为 fallback
- **现状**：`runtime_mode` 默认 `"local"`，现有用户升级无感
- **理由**：
  - 敏感场景（企业内网）可能无法连接外部 API
  - 用户已有 API Key 可继续使用
  - 降低迁移阻力

### 决策 3：配额上限设置 —— 待接线时确认默认值是否已写入代码
- **软上限默认值**：¥5 (500 分)
- **硬上限默认值**：¥20 (2000 分)
- **可配置**：Settings 中可调整（Settings UI 未实现）

### 决策 4：心跳频率 —— 待 `start_heartbeat_loop()` 落地
- **建议频率**：5 分钟
- **abu-api 超时**：10 分钟无心跳标记为 failed
- **实现**：后台 tokio task，对话期间持续发送

### 决策 5：Provider 接入形态 —— **已落地（与原设计不同）**
- **实际选择**：虚拟 `ModelProvider`，复用现有 `api_format_kind()` 分发链路
- **放弃**：`impl LanguageModelProvider` + `select_provider()`（该函数在本仓库不存在）
- **影响**：中转端点的响应格式必须与现有 adapter 兼容，接线时首先验证这点

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

- **M1 (Phase 1-2)**: 认证与引导 - 用户可以登录并完成引导 —— ✅ 基本达成（差 Provider 步决策）
- **M2 (Phase 3)**: 模型调用 - 云端模式可以正常聊天 —— ⏳ 代码就位，未接线，**当前跑不通**
- **M3 (Phase 4)**: Settings 完善 - 设备管理、使用统计就位 —— ⏳ 未开始
- **M4 (Phase 5)**: 体验优化 - 配额提示、错误处理完善 —— ⏳ 未开始
- **M5 (Phase 6-7)**: 发布准备 - 文档、测试、打包 —— ⏳ 仅设计文档

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

### 风险 5：孤岛代码腐化（新增）
- **现象**：570 行 Rust 无调用方，编译器覆盖不到实际调用路径，改动其他模块时容易与之漂移
- **缓解**：尽快接线，或在接线前至少加单测把 `prepare_conversation` / `check_task_quota` 钉住

### 风险 6：文档与实现漂移（新增）
- **现象**：SUMMARY 文档描述的 `select_provider` / `settings_abu_api` 接线方式与真实代码不符
- **缓解**：接线完成后同步修订文档；本清单以代码为准，冲突时信代码

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

**当前进度**：Phase 1 ✅ / Phase 2 ✅（差 1 项决策）/ Phase 3 代码就位但未接线 / Phase 4-7 未开始

**建议下一步**：接线 Phase 3——在 `send.rs` 与 `chat/model/mod.rs` 的分发链路里加 Cloud 分支，
让已写好的 `abu_api_task.rs` + `abu_api_provider.rs` 真正跑起来。
