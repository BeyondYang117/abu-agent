# Windows 稳定性修复

**日期**: 2026-09-08  
**严重性**: Critical  
**影响**: 所有 Windows 用户

## 问题描述

Windows 系统上出现三个严重的可用性问题：

1. **窗口无法关闭/卡死** - 点击右上角关闭按钮无响应，应用完全卡死
2. **登录授权超时** - OAuth 登录流程显示「授权超时，请重新开始」
3. **整体体验退化** - 相比之前版本明显变差（用户反馈："之前使用挺丝滑的，现在体验太差了"）

## 根本原因分析

### 1. 窗口关闭卡死

**根本原因**: `lib.rs:162-167` 的 `CloseRequested` 处理器在主线程同步读取设置：

```rust
// 问题代码
let keep_alive = window
    .app_handle()
    .state::<AppState>()
    .settings_read()  // ← 阻塞读取，Windows 上可能卡死
    .keep_chat_window_alive;
```

**为什么在 Windows 上更严重**:
- Windows WebView2 对主线程阻塞更敏感
- 设置文件可能被其他进程锁定（杀毒软件/备份软件）
- Onboarding 阶段配置可能未完全初始化

### 2. 登录授权超时

**问题1**: 超时错误信息不明确
- 用户不知道是浏览器没打开、网络问题，还是没完成授权
- 没有明确的恢复路径

**问题2**: Windows 浏览器启动可能静默失败
- `api.openExternal()` 在 Windows 上可能因各种原因失败
- 用户不知道浏览器根本没打开

**问题3**: 缺少降级路径
- 自动打开浏览器失败后，用户不知道授权地址

## 修复措施

### 修复 1: 窗口关闭防御性处理 (lib.rs)

```rust
// 修复后代码
if window.label() == "chat" {
    // Windows 修复：避免在 CloseRequested 同步读取设置导致卡死。
    // 使用 try_lock 防御性读取，失败时允许直接关闭（安全降级）。
    let keep_alive = {
        let state = window.app_handle().state::<AppState>();
        match state.settings.try_read() {
            Ok(guard) => guard.keep_chat_window_alive,
            Err(_) => {
                // 设置锁被持有或损坏，安全降级：允许关闭窗口
                eprintln!("Warning: Failed to read keep_chat_window_alive setting during close, allowing window to close");
                false
            }
        }
    };
    if keep_alive {
        api.prevent_close();
        hide_chat_window(window.app_handle(), window);
    }
    return;
}
```

**修复原理**:
- 使用 `try_read()` 替代阻塞的 `settings_read()`
- 失败时降级为允许关闭（`false`），而不是卡死
- 添加错误日志用于诊断

### 修复 2: 增强窗口隐藏防御性 (shortcuts.rs)

```rust
pub(crate) fn hide_chat_window(app: &AppHandle, window: &tauri::Window) {
    // Windows 修复：增加防御性错误处理，避免 hide 操作卡死
    #[cfg(target_os = "windows")]
    {
        if let Err(e) = window.hide() {
            eprintln!("Warning: Failed to hide chat window on Windows: {}", e);
            // 隐藏失败时尝试最小化作为降级方案
            let _ = window.minimize();
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window.hide();
    }

    #[cfg(target_os = "macos")]
    crate::chat::popout::sync_macos_activation_policy(app);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}
```

**修复原理**:
- Windows 特定的错误处理
- `hide()` 失败时降级为 `minimize()`
- 确保即使操作失败也不会卡死

### 修复 3: 登录超时优化 (LoginStep.tsx)

**改进超时错误信息**:
```typescript
if (Date.now() >= expiresAt) {
  stopPolling()
  // Windows 修复：提供更明确的超时原因和恢复建议
  console.error('Device authorization timeout after 10 minutes')
  setError(
    t.onboardingLoginTimeout ||
      '授权超时（10分钟内未完成）。可能原因：浏览器未打开、未完成授权、或网络问题。请重试。',
  )
  setDeviceFlow(null)
  return
}
```

**改进浏览器启动错误处理**:
```typescript
void api.openExternal(verificationUrl).catch((err) => {
  const detail = err instanceof Error ? err.message : String(err)
  console.error('Windows browser launch failed:', detail)
  setError(
    `无法自动打开浏览器（${detail}）。请手动复制下方验证码到浏览器完成授权。`,
  )
})
```

**添加授权地址显示**:
```tsx
<p className="text-xs text-neutral-400 dark:text-neutral-600 mt-2">
  授权地址：{selectedBaseUrl}
  {deviceFlow.verificationUri}
</p>
```

## 测试验证

### 关键测试点

1. **窗口关闭测试**:
   - [ ] 正常关闭（设置未初始化）
   - [ ] 正常关闭（keep_alive = false）
   - [ ] hide 模式（keep_alive = true）
   - [ ] 设置文件被锁定时的降级行为
   - [ ] Onboarding 阶段点击右上角关闭

2. **登录流程测试**:
   - [ ] 正常登录流程
   - [ ] 浏览器启动失败场景
   - [ ] 超时场景（等待 10 分钟）
   - [ ] 手动复制验证码场景
   - [ ] "重新打开浏览器" 按钮

3. **Windows 特定测试**:
   - [ ] Windows 10
   - [ ] Windows 11
   - [ ] 不同默认浏览器（Chrome/Edge/Firefox）
   - [ ] 杀毒软件运行时
   - [ ] 低权限用户

### 回归测试

- [ ] macOS 窗口关闭行为未受影响
- [ ] Linux 行为未受影响
- [ ] Lens 窗口关闭正常
- [ ] Popout 窗口关闭正常

## 代码审查要点

### 安全降级原则

所有 Windows 修复都遵循"安全降级"原则：
- 操作失败时不会卡死应用
- 降级到功能略有损失但仍可用的状态
- 错误会被记录但不会中断用户流程

### Windows 特定代码

所有 Windows 特定修复都使用 `#[cfg(target_os = "windows")]`：
- 不影响 macOS/Linux 的现有行为
- 可以单独优化和测试

### 错误日志

所有关键路径都添加了诊断日志：
- `eprintln!` 用于运行时错误
- `console.error` 用于前端错误
- 便于用户报告问题时提供详细信息

## 影响范围

### 修改的文件

1. `src-tauri/src/lib.rs` - CloseRequested 处理器
2. `src-tauri/src/shortcuts.rs` - hide_chat_window 函数
3. `src/onboarding/steps/LoginStep.tsx` - 登录流程 UI 和错误处理

### 兼容性

- **向后兼容**: 是
- **配置迁移**: 不需要
- **协议变更**: 无
- **最低系统版本**: 不变

## 部署建议

### 发布优先级

**P0 - 紧急修复版本**
- 这些问题阻止 Windows 用户正常使用应用
- 建议作为 hotfix 立即发布

### 发布注记

```markdown
## v0.1.11 - Windows 稳定性紧急修复

### 修复

- **[Windows] 修复窗口无法关闭和卡死问题** - 使用防御性读取设置，失败时允许直接关闭
- **[Windows] 修复窗口隐藏失败导致卡死** - 隐藏失败时降级为最小化
- **[Windows] 改进登录授权超时提示** - 提供明确的原因和恢复建议
- **[Windows] 改进浏览器启动失败处理** - 提供手动降级路径和授权地址显示

### 已知问题

- 需要 Rust 1.88+ 编译（开发环境依赖更新中）
```

## 后续改进

### 短期 (v0.1.12)

1. **增强日志系统**
   - 添加结构化日志收集
   - Windows 特定事件追踪
   - 用户可导出诊断报告

2. **设置加载优化**
   - 异步加载设置
   - 内存缓存热路径配置
   - 降低磁盘 I/O 频率

### 中期 (v0.2.0)

1. **Windows 性能优化**
   - WebView2 生命周期优化
   - 主线程工作负载分析
   - 关键路径性能监控

2. **错误恢复机制**
   - 自动重试机制
   - 配置文件损坏自愈
   - 崩溃后状态恢复

### 长期

1. **平台一致性**
   - 统一 Windows/macOS/Linux 的窗口管理逻辑
   - 减少平台特定代码路径
   - 自动化跨平台测试

## 参考资料

- 用户报告截图: docs/fixes/windows-login-timeout-screenshot.png
- 相关 issue: #TBD
- 设计文档: CLAUDE.md (窗口生命周期部分)
- 测试计划: docs/testing/windows-stability.md (待创建)

---

**维护者**: @BeyondYang117  
**审核者**: TBD  
**测试者**: TBD
