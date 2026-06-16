# AI 客户端交互与动画体验优化

## Goal

为 Kivio 的 chat（AI 客户端）UI 系统性优化交互与动画，让体验更顺滑、更有"活的"反馈感。**地基优先、分阶段全做**：先统一 motion token 地基并消除最扎眼的导航层硬切换，再逐阶段打磨流式、加载、状态机动效。沿用现有克制的 macOS 原生质感，纯 CSS + 扩展现有 `chat-motion-*` 基础设施，**不引入任何动画库**（无 framer-motion / auto-animate）。

## What I already know

来自一轮 6-agent 并行 survey（流式消息 / 工具·推理块 / 导航侧栏 / 输入框 / 选择器·弹窗 / CSS 基础设施）：

**已有的好基础（要保留、不破坏）：**
* `index.css` 已有 11 个 `@keyframes`：`chat-motion-fade-up/popover/row/soft-pulse/search-reveal/subagent-shimmer`、`reasoning-shimmer`、`reasoning-tail-pulse`、打字机光标、子 agent 星芒、图片生成脉冲。
* `index.css:981` 一处较完整的 `@media (prefers-reduced-motion: reduce)` 守卫。
* 流式时 `MessageBubble` / `ChatMarkdown` 用 `React.memo` 防重渲染；滚动"贴底"逻辑（≤32px 判定 + ResizeObserver + RAF）写得聪明。
* 主 easing 已基本统一为 `cubic-bezier(0.22, 1, 0.36, 1)`。

**确认的问题（按主题，已去重）：**
1. **缺统一 motion token** — 两套不兼容 easing（`(0.22,1,0.36,1)` vs `(0.2,0,0,1)`）+ 十几种散落 duration（0.12s→2.4s），无命名/集中管理。
2. **导航层硬切换** — 侧栏折叠是直接 DOM 卸载（`Chat.tsx:2627` / `Sidebar.tsx:680`，240px→0 瞬跳）；切换会话是硬切，消息列表瞬间替换（`Chat.tsx:1762`）。
3. **该平滑却瞬变** — 选择器 chevron 无 transition（`ModelSelector.tsx:62`/`SkillSelector.tsx:94`/`SkillCenter.tsx:481`）；textarea 增高命令式无缓动（`InputBar.tsx:580`）；附件 chip 删除瞬消（`ChatAttachments.tsx:170`）；发送→停止整体替换无 crossfade（`InputBar.tsx:1711`）；弹窗/对话框无入场（`ProjectDialog.tsx:68`、技能预览）；折叠面板、图片缩放瞬变。
4. **流式/消息打磨** — 多 segment 同时弹入无错峰（`MessageBubble.tsx:232`）；工具块无入场动画；滚动到底瞬间 snap（`MessageList.tsx:109`）；复制反馈仅图标切换无微动效。
5. **加载态缺失** — 无骨架屏 primitive，全靠 spinner/纯文字（侧栏"加载中…"纯文字）。
6. **状态切换生硬** — 工具 `pending→running→done`、todo 项、AskUser 选项/翻题都是瞬间 class 替换。
7. **reduced-motion 漏网** — 内联 Tailwind `transition-colors/all` 没被那条 media query 覆盖。

## Requirements (evolving)

### Phase 1 — 地基 + 最扎眼的硬切换（高性价比，首批交付）
* **R1.1** 在 `index.css` `:root` 建立 motion token：`--kv-ease-*`（standard / emphasized / spring）+ `--kv-dur-*`（fast/normal/slow 等），并把现有关键帧与 transition 渐进式改用 token（不破坏现有观感）。
* **R1.2** reduced-motion 兜底：在现有 media query 内加全局守卫（`* { transition-duration / animation-duration → 极短或 0 }`），覆盖内联 Tailwind 过渡；保留必要的"瞬时可见"语义。
* **R1.3** 侧栏折叠/展开改为带动画的 max-width + 内容淡入（保留 DOM，不卸载）。
* **R1.4** 切换会话时消息区做轻量 cross-fade（按 conversation id 触发）。
* **R1.5** 选择器 chevron 旋转加 `transition-transform`（ModelSelector / SkillSelector / SkillCenter 高级设置等）。
* **R1.6** 附件 chip 删除加退出动画（scale+fade，动画结束后再移除节点）。
* **R1.7** 发送 ↔ 停止按钮改 crossfade（两按钮共存 + opacity/scale 过渡），点击目标稳定。
* **R1.8** 弹窗/对话框入场动画（ProjectDialog、技能预览复用 `chat-motion-popover`）。

### Phase 2 — 流式/消息体验打磨
* **R2.1** 流式 segment 错峰入场（`--chat-motion-delay` 按 index 递增，封顶）。
* **R2.2** 工具块入场动画（新工具调用 fade-up）。
* **R2.3** 滚动到底平滑化（**保守**：不与用户滚动冲突，详见决策）。
* **R2.4** 复制反馈微动效（Check 图标 pop-in）。
* **R2.5** 图片懒加载骨架/淡入。
* **R2.6** reasoning 流式滚动平滑化。

### Phase 3 — 加载态 + 状态机 + 深度交互
* **R3.1** 骨架屏 primitive（`@keyframes` shimmer + `.kv-skeleton`），用于侧栏加载、消息列表加载、附件缩略图。
* **R3.2** 工具状态 `pending→running→done` 过渡动画（图标 fade+scale 入场、运行态高亮）。
* **R3.3** todo 项状态变更高亮 + 列表 reflow。
* **R3.4** AskUserBlock 选项选中动画 + 翻题过渡。
* **R3.5** 折叠面板（高级设置等）高度动画。
* **R3.6** 图片查看器缩放缓动。
* **R3.7** 窗口控制按钮 press 反馈（轻弹）。
* **R3.8** 暗色模式切换过渡（**谨慎**：避免首屏/初始化闪烁）。
* **R3.9** focus-visible ring 打磨 + 选择器列表项 hover 反馈。

## Acceptance Criteria (evolving)

* [ ] Phase 1 全部落地，`npm run lint`、`npm run typecheck`、`npm test` 全绿。
* [ ] 侧栏折叠/会话切换/chevron/附件删除/发送停止/弹窗均有顺滑过渡，无硬跳。
* [ ] motion token 建立后，新增动效统一引用 token；现有观感不退化。
* [ ] `prefers-reduced-motion: reduce` 下所有新动效都被正确禁用/降级（含内联过渡）。
* [ ] `npm run dev` 手动冒烟：流式、折叠、切换、弹窗、附件无卡顿/闪烁。
* [ ] 每个 Phase 独立成一批（可单独 review / 合并）。

## Definition of Done

* 每个 Phase：lint / typecheck / 相关 Vitest 绿；改动了渲染流的组件其 `.test.tsx` 保持绿。
* 不引入运行时依赖（纯 CSS + 现有基础设施）。
* 行为变化处补充必要注释（沿用仓库中英注释风格）。
* 关键路径手动冒烟（capture/hotkey 不受影响——本任务只动 chat UI）。
* reduced-motion 与暗色模式两条无障碍/主题路径都验证过。

## Out of Scope (explicit)

* 引入任何动画库（framer-motion、auto-animate、gsap、lottie 等）——明确不做。
* 重写 Rust 后端 / 流式事件协议（`chat-stream`/`chat-tool`/`chat-context` 等 UI 契约保持稳定）。
* Lens / 翻译 / 截图 OCR 等非 chat 界面的动效（除非与共享 token 顺带受益）。
* 重构组件架构 / 状态管理；只做交互与动效层的增强。
* 新增功能性特性（仅体验优化）。

## Technical Approach

* **token 化优先**：先建 `:root` 的 `--kv-ease-*` / `--kv-dur-*`，作为后续所有动效的单一来源；现有硬编码渐进替换，避免一次性大改导致回归。
* **复用 > 新增**：优先复用 `chat-motion-*` 既有关键帧；只有缺口（骨架屏、退出动画、crossfade）才新增。
* **退出动画统一手法**：加 `removing`/`is-closing` 类 → 等 `onAnimationEnd`/timeout → 再卸载 DOM。
* **导航过渡**：侧栏保留 DOM + `max-width`/`opacity` 过渡；会话切换用 key 驱动的 opacity cross-fade。
* **无障碍**：所有新动效都纳入 reduced-motion 守卫；Phase 1 顺带补全内联过渡漏网。
* **分阶段 = 分批交付**：Phase 1/2/3 各自可独立 review、独立合并；地基（token + reduced-motion）必须在 Phase 1 先落地。

## Open Questions

* **Q-A（Phase 2 决策，可延后到 Phase 2 再定）** 滚动到底平滑化策略：保守只在"用户已贴底"时平滑、还是 CSS `scroll-behavior: smooth` 全局兜底？倾向保守方案，避免与用户滚动/快速流式冲突。
* **Q-B（Phase 3 决策）** 暗色模式切换过渡：是否值得做（需防首屏闪烁）？倾向做但加初始化守卫。

## Technical Notes

* 当前分支 / base：`refactor/pi-style-tools`。
* 核心文件：`src/index.css`（~3224 行，动效集中地）、`src/chat/*.tsx`。
* 无动画库；Tailwind v4 + 自定义 CSS。
* survey 原始结论（6 agent）已读入会话上下文；本 PRD 为其去重收敛版。

---

## 进度记录

### Phase 1 — 已完成实现（2026-06-16）
全部 8 项落地：R1.1 motion token 地基（:root --kv-ease-*/--kv-dur-*，现有 chat-motion-* 改用 token，零行为变化）；R1.2 reduced-motion 全局兜底（媒体查询内 * 兜底，0.01ms 保留 animationend）；R1.3 侧栏折叠（负 margin 滑出 + opacity + inert，flex 让出 240px）；R1.4 会话切换（MessageList key + 滚动根 chat-motion-fade）；R1.5 chevron 缓动 token 化（注：survey 误判为"无过渡"，实为已有 Tailwind 150ms）；R1.6 附件退出动画（removingIds + chat-motion-exit + animationend 清理）；R1.7 发送↔停止 crossfade（共存 + opacity/scale + aria/tabIndex）；R1.8 弹窗入场（chat-motion-fade backdrop + chat-motion-modal-in）。
验证：npm run typecheck / lint / test（72）全绿；npx vite build 通过；产物 CSS 确认 token/新类/任意值/reduced-motion 均编译正确。
新增可复用 CSS：chat-motion-exit / chat-motion-fade / chat-motion-modal-in；扩展 .chat-sidebar-shell.is-collapsed。

### Phase 1 — 对抗式评审与修复（2026-06-16）
评审：4 维度 × 对抗验证（15 agent）。修复 5 项：
- [CRITICAL] index.css 注释 `transition-*/animate-*` 中的 `*/` 提前闭合 CSS 注释 → 损坏 reduced-motion 兜底规则。已重写注释；重建后产物 `@media(prefers-reduced-motion){*,:before,:after{...animation-duration:.01ms...}}` 确认完好、无 CSS 警告。
- [a11y] SkillCenter 预览弹窗补 role="dialog" / aria-modal / aria-labelledby（对齐 ProjectDialog）。
- [polish] 发送按钮隐藏时不再挂 chat-motion-soft-pulse（条件化）。
- [a11y] Sidebar inert 改 useLayoutEffect（与 aria-hidden 绘制前原子生效）。
- [doc] reduced-motion 块注明新类（fade/modal-in/exit）刻意靠兜底压时长以保留 animationend。
未修（经判断）：发送按钮 tabIndex=-1 为既有刻意设计（Enter 发送；流式时输入禁用，停止按钮才需可 tab），非本次回归。
复验：typecheck / lint / test(72) 全绿；vite build 干净。

### Phase 2 — 已完成实现 + 评审修复（2026-06-16）
实现：R2.1/R2.2 每个 timeline segment 包 chat-motion-fade 纯透明度入场（无 stagger，避免流式滞后；键控防重放）；R2.4 复制 ✓ 弹入 chat-motion-pop（MessageBubble/AssistantMessageMeta/ChatMarkdown 代码块/GeneratedFileArtifacts）；R2.5 ChatAttachments ImagePreview 加载态用 kv-skeleton（移除 Loader2）+ 加载完成淡入。R2.3/R2.6 保守决策：流式 pin-to-bottom 保持瞬时（平滑会滞后+违反 scroll-follow spec），不动；回到底部按钮属功能、不在本阶段。
评审（3 维度 × 对抗验证，26 agent）修复：
- [HIGH] ArtifactImage 原误加淡入——artifact 图本是内存 data URL（秒显），opacity-0+onLoad+lazy 反而可能不显示/违背秒显意图。已回退为直接显示（ImagePreview 的真实异步加载态仍保留 skeleton+淡入）。
- [MEDIUM] kv-skeleton 在 reduced-motion 下渐变冻结屏外 → 加显式静态规则（animation:none + background-image:none，留静态底色占位）。
- [doc] reduced-motion 注释补充 chat-motion-pop / kv-skeleton 说明。
确认正确无需改：TimelineSegments 键控/记忆化、流式标志、Loader2 移除、性能。
复验：typecheck / lint / test(72) 全绿；vite build 干净。

### Phase 3 — 已完成实现 + 评审修复（2026-06-16）
实现（高+中价值，跳过 R3.7）：R3.1 侧栏加载态用 6 行 kv-skeleton（替"加载中…"文字）；R3.2 工具完成 ✓ chat-motion-pop；R3.3 todo 状态点 transition-colors + 项 chat-motion-fade；R3.4 AskUser 问题容器 key+chat-motion-fade（翻题淡入）+ 选中图标 pop；R3.5 SkillCenter 高级设置改 chat-motion-reveal 高度动画；R3.6 图片查看器缩放 width 过渡（%↔% 步进平滑）；R3.8 暗色切换过渡（applyTheme 成功后下一帧加 theme-transitions-ready 守卫防首屏闪烁 + 主要表面 background/border 过渡）；R3.9 focus-visible ring box-shadow 缓入。
评审（3 维度 × 对抗验证）修复 2 项：
- [HIGH] SkillCenter 高级面板改 always-mount 后折叠态表单仍可 Tab 聚焦（WCAG 2.1.1）→ 加 inert 守卫（与侧栏一致），恢复折叠不可聚焦。
- [MEDIUM] 工具 ✓ pop 在切换会话时所有历史工具一起弹动（MessageList 重挂）→ StatusIcon 用 prev-status ref 门控，只在实时 running→completed 时 pop。
未改（经判断）：[NIT] focus ring 未被 theme-transitions-ready 门控（键盘焦点恰落初始化 ~16ms 几乎不可能）；[MEDIUM 但建议有误] applyTheme 错误路径——reviewer 的 try/finally 会在出错后加 ready 导致随后成功应用时闪烁，现有"成功后才加 ready"才正确。
复验：typecheck / lint / test(72) 全绿；vite build 干净。

### 追加修复（用户测试反馈）
1. 侧栏折叠回归：Phase 3 主题过渡的 `.theme-transitions-ready .chat-sidebar-shell{transition:background-color,border-color}`（高优先级）用简写覆盖掉了 Phase 1 的折叠 `transition:margin-left,opacity` → 折叠变硬跳。修：侧栏单列合并声明（四属性一起），composer 移出主题过渡（恢复其聚焦辉光）。
2. Windows 伸缩闪白：chat 窗口原生背景硬编码白色，暗色下伸缩露白。修：apply_chat_window_theme_background + chat_window_is_dark（settings.theme，system 跟随 window.theme()）+ apply_chat_window_chrome 改用之 + set_chat_window_background 命令（前端 applyTheme 实时同步）。macOS/Linux 透明窗 no-op。经 Explore agent 审查 Tauri v2 API/cfg/逻辑均正确；cargo check（macOS）通过；Windows 路径需上机验。

### 追加修复：窗口伸缩卡顿（IPC 洪流）
两个 onResized 监听器在每次 resize 事件都发 IPC：① ChatWindowHost 的 getCurrentWindow().isMaximized()；② App.tsx 的 persistChatWindowGeometry()（snapshot 多次 IPC 读尺寸 + 写 store），且 onMoved 同样。拖动伸缩时每秒数十次事件 → 每帧 5+ 次 IPC → 与 resize 渲染抢资源 → 卡顿（Windows/WebView2 尤甚）。修：两处都 debounce（isMaximized 150ms、几何持久化 250ms），拖动过程零 IPC、停止后做一次；timer 在 cleanup 清理。typecheck/lint/test(72) 全绿。
未做（风险）：消息列表 content-visibility/containment 可进一步降重排成本，但会干扰流式滚动跟随（与 scroll-follow spec 冲突），暂不动；若 debounce 后仍有纯重排卡顿再评估。
