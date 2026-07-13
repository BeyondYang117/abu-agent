# Design — Chat 窗口首开体验

## 一、reveal 门控（R1）

### 现状
- 后端首次创建 chat 窗口 `visible(false)`（`windows.rs:287`），不调 show，注释期待前端恢复几何后 show（`shortcuts.rs:968-975`）。
- 前端 `App.tsx:358-362` 的 `useLayoutEffect` 在 App commit 后立即 `revealChatWindow()`——早于 lazy Chat chunk 加载，导致"弹出即转圈"。
- 复用路径由后端 `reveal_chat_window`（`shortcuts.rs:926-939`）直接 show，与前端无关，不动。

### 方案：内容就绪信号 + 兜底定时器（纯前端改动）

**信号源（自下而上）**
1. `SettingsShell`：已有 `onReady` 机制（`SettingsShell.tsx:763-769`，`readyEmittedRef` 防重）但只在 `variant==='standalone'` 触发且无消费者（死接口）。改为**只要传了 `onReady` 就触发**（去掉 variant 条件），在 `!loading && (settings || loadError)` 后 emit 一次。语义：设置页数据就绪或错误态就绪。
2. `Chat.tsx`：新增可选 prop `onContentReady?: () => void`（与现有 `onSettingsChange` 同路传递）。触发规则（一次性，ref 防重）：
   - 初始 `chatView !== 'settings'`：挂载后的 `useLayoutEffect` 直接 emit（会话/assistants/skill/onboarding 视图首帧骨架已可渲染，不等会话数据——与 PRD R1 一致）。
   - 初始 `chatView === 'settings'`（`#chat/settings` 冷开）：把 emit 委托给 SettingsShell 的 `onReady`（Chat 把回调接到 `<SettingsShell onReady={...}>`）。SettingsShell 的 lazy Suspense 挂起期间不会 emit——由兜底覆盖 chunk 加载失败。

**App 侧门控（`App.tsx`）**
- `revealedRef`（boolean ref）+ `revealChatWindowOnce()`：第一次被调用才真正执行现有 `revealChatWindow()`，其后调用直通（保持现有"每次 mode→chat 都校正几何/持久化"的行为不劣化：直通路径仍调用 revealChatWindow，它内部本就有 isVisible 幂等检查）。
- 现有 `useLayoutEffect`（mode==='chat'）改为：
  - 启动**兜底定时器 3000ms**：到点未 reveal 则强制 reveal（A3：chunk 挂起/组件抛错/信号丢失时窗口绝不永久 hidden）。
  - 不再立即 reveal。
- `<Chat onContentReady={revealNow}>`：信号到达 → 清定时器 → reveal。
- ErrorBoundary 情形：Chat 抛错被 `ChatErrorBoundary` 接住 → 信号不来 → 3s 兜底 show 错误 UI。可接受（错误路径本就罕见）。
- 非 chat 模式（translator/lens）不经过此路径，零影响。

**为什么不用后端等前端事件**：后端已把 show 的责任完全交给前端（首建路径不 show），前端内部把"何时调 showWindow"从"App commit"推迟到"内容就绪"即可，无 IPC 协议改动、无 Rust 改动。

## 二、settings 前端缓存（R2）

### 现状
- `api.getSettings()`（`tauri.ts:1275`）每次都 invoke + `normalizeSettings`；冷启动被 App 主题、`loadDefaultModel`、onboarding 检查、Sidebar、`refreshToolIndicator`、`ModelSelector`、`SettingsShell` 独立调用 5-6 次。
- `SettingsShell` 打开时 `loading=true` gate 全页（`SettingsShell.tsx:721-742`）。
- 后端无 settings-changed 广播事件（已 grep 确认），写路径全部经前端 `saveSettings`（`save_settings` 返回落盘后的 `Settings`）。

### 方案：模块级单例缓存 `src/api/settingsCache.ts`

```ts
let cached: Settings | null = null
let inflight: Promise<Settings> | null = null

export function peekSettings(): Settings | null            // 同步读缓存（SWR 首帧用）
export function getSettingsCached(): Promise<Settings>     // 有缓存立即 resolve；否则 dedupe 单一 invoke
export function refreshSettings(): Promise<Settings>       // 强制 refetch 并更新缓存
export function updateSettingsCache(s: Settings): void     // 写通入口
export async function saveSettingsCached(s: Settings): Promise<Settings>
  // = api.saveSettings(s) 成功后 updateSettingsCache(返回值)；失败不动缓存
```

要点：
- **并发首读去重**：`inflight` promise 共享；失败时清 `inflight` 且不写缓存（下次重试），保持"失败不合成默认值"语义。
- **写通**：所有 `api.saveSettings` 调用方（`Chat.tsx:1087,1109`、`OnboardingShell.tsx:90,117`、`SettingsShell` 保存流程、收藏模型轻量持久化——若其后端返回新 Settings 同样写通，否则保存后 `refreshSettings()`）改走 `saveSettingsCached`。
- **跨窗口一致性豁免**（PRD 约束）：模块缓存 per-webview；translator/lens 短命窗口销毁即弃。chat 窗口存活期内其他窗口不写 settings（设置编辑器只在 chat 窗内）。在模块头注释说明。

### SettingsShell SWR
- 初始化 effect（`SettingsShell.tsx:721-742`）改为：
  - `peekSettings()` 有值 → 直接 `setSettings(cached)` + snapshot，**不设 loading**（首帧即渲染，A4）；随后后台 `refreshSettings()`，返回后**仅当草稿仍 pristine**（`stableStringify(current) === initialSettingsSnapshot`）才应用新值+新 snapshot——避免静默 refetch 覆盖用户正在编辑的草稿。
  - 无缓存 → 行为同现状（loading gate + 失败显示重试 UI，不合成默认值）。
- 其余读调用方（App/Chat/Sidebar/ModelSelector/toolIndicator）机械替换为 `getSettingsCached()`；`onSettingsChange`→`applyTheme` 链路因写通天然拿到新值。

## 三、权衡与兼容

- **reveal 延迟上界**：正常路径内容就绪在生产 <100ms、dev 首开 = chunk 编译时间（本就要等，只是原来"窗口先弹再转圈"，现在"稍晚弹但即完整"）；异常路径 3s 兜底。感知上从"快但破碎"换成"稍慢但完整"，与成品行为对齐。
- **回滚**：两块改动互相独立，各自一个 commit；回滚 = revert 对应 commit，无数据迁移。
- **不动**：Rust 侧零改动；窗口销毁策略、事件契约、`get_settings`/`save_settings` 签名均不变。

## 四、测试策略

- 新增 `src/api/settingsCache.test.ts`：并发首读去重 / 失败不缓存可重试 / 写通 / peek 语义（mock invoke）。
- `SettingsShell` 已有测试若依赖 loading 首帧，按 SWR 语义修正。
- 手动冒烟（dev 运行中 app）：A1/A2/A4/A7 场景 + 兜底路径（A3 用 DevTools 断点模拟 chunk 挂起）。
