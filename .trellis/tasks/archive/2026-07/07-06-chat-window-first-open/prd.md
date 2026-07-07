# Chat 窗口首开体验：show 等内容就绪 + settings 前端缓存

## 背景

用户反馈：首次启动后点设置要"转一会圈"，第二次才秒开。全链路调研确认根因有两层：

1. **窗口 show 时机过早**：chat 窗口首次创建时 hidden，`App.tsx` 的 `useLayoutEffect` 在 App 组件 commit 后立即 `show()`（`src/App.tsx:358-362`），但此时 lazy 的 Chat chunk 尚未加载——用户看到的是"已弹出但在转圈"的窗口。后端注释宣称"前端恢复几何后再 show"只防了几何闪烁，未防内容空窗。
2. **settings 无前端缓存**：一次 chat 冷启动中 `get_settings` 被独立 invoke 5-6 次（App 主题、loadDefaultModel、onboarding 检查、Sidebar 用户资料、refreshToolIndicator、ModelSelector、SettingsShell）；SettingsShell 打开时 `loading=true` 转圈等 `getSettings` 返回，尽管后端是纯内存读。

## Requirements

### R1 首次 reveal 等内容就绪
- chat 窗口**首次创建**的 reveal（show+focus）延迟到首屏内容可渲染之后：
  - 会话视图：Chat 组件完成首次挂载（首帧已有 UI 骨架，不要求会话数据返回）。
  - 设置视图（`#chat/settings` 首开）：SettingsShell 完成首次可渲染（settings 数据就绪或错误态就绪）。
- **必须有兜底**：内容就绪信号因任何原因未触发（chunk 加载失败、组件抛错被 ErrorBoundary 接住等）时，最迟 2-4 秒内窗口仍然 show，绝不允许窗口永久 hidden。
- 窗口**已存在**的复用路径（后端 eval hash + `reveal_chat_window`）行为不变。

### R2 settings 前端内存缓存
- 前端提供 settings 缓存读取入口：首次调用走 invoke，之后立即返回缓存；并发首读只发一次 invoke。
- 写通：`saveSettings`（及轻量收藏模型持久化）成功后用后端返回值更新缓存。
- SettingsShell 打开时改为 stale-while-revalidate：有缓存则**首帧直接渲染缓存数据**（不显示 loading spinner），后台静默 refetch 校准；无缓存行为同现状。
- 保留现有安全语义：加载失败时不合成默认值（避免错误状态下 Save 覆盖磁盘真实数据）。
- 现有各处 `api.getSettings()` 调用方切换到缓存入口。

## Constraints

- 不改变"关闭即销毁"的窗口生命周期策略。
- 不改后端 `get_settings`/`save_settings` 命令签名。
- 跨窗口（translator/lens 窗）缓存一致性不在本任务范围：这些窗口短命（用完销毁），各自模块缓存随 webview 销毁而消失；接受窗口存活期内的理论 staleness，在代码注释中说明。
- `chat-stream`/`chat-tool`/`chat-context` 等 UI 事件契约不动。

## Acceptance Criteria

- [ ] A1：冷启动(无 chat 窗口)从托盘/翻译器点"设置"，窗口弹出瞬间即显示设置页内容(或其错误态)，不出现"空窗转圈"帧；dev 与生产构建均如此。
- [ ] A2：冷启动打开 chat 主界面，窗口弹出瞬间即显示 chat UI 骨架，无空窗转圈帧。
- [ ] A3：人为让 Chat chunk 加载挂起（断点/网络模拟），窗口在兜底超时内仍然 show。
- [ ] A4：SettingsShell 二次打开（同窗口内 settings→返回→再进 settings）首帧无 loading spinner。
- [ ] A5：修改任一设置并保存后，依赖 settings 的 UI（主题、默认模型等）行为与改动前一致（缓存写通生效）。
- [ ] A6：`npm run lint`、`npm run typecheck`、`npm test` 全绿。
- [ ] A7：窗口复用路径（chat 窗已开时再点设置/托盘）行为与改动前一致。
