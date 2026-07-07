# Implement — 统一按钮风格：React Button 组件 + kv-btn

> 上下文加载顺序：本文件 → prd.md → design.md。实施为**内联**模式（本会话直接改），非 sub-agent 派发，故不走 jsonl 清单门。

## 校验命令（每阶段末执行）
- `npm run lint`（`--max-warnings 0`）
- `npm run typecheck`
- `npm test`（Vitest 一次跑；改动组件相关则 `npx vitest run src/components`）
- 视觉：dev 已在跑（HMR，端口 5713）。每批迁移后在真机目测对应界面。

---

## Phase 0 — 组件与 CSS 基座（独立提交，回滚点 A）
- [ ] 0.1 新建 `src/components/Button.tsx`：按 design.md 契约实现 `Button` / `IconButton`（`export function` + 内联类型 + `...props` 透传；类名合成按 design.md）。
- [ ] 0.2 `src/index.css`：在 `.kv-icon-btn` 定义处（`~2783`）后追加 `.sm/.md/.lg/.circle` 修饰 + 带尺寸修饰时的 svg 尺寸规则（见 design.md CSS 块）。
- [ ] 0.3 新增 `src/components/Button.test.tsx`（Vitest + RTL）：断言 variant/size/shape → 期望类名；`IconButton` 无 `label` 时的 aria 行为。
- [ ] 0.4 校验：typecheck + `npx vitest run src/components` 绿。
- **Gate**：组件与 CSS 落地、单测绿，再进入迁移。

## Phase 1 — 批 1：聊天/Lens 动作按钮迁移（按文件，每文件一提交，回滚点 B_n）
> 判据：只迁**动作按钮**；跳过列表行/菜单项/tab/窗口控件/发送键（见 prd Out of scope + design 豁免清单）。逐文件甄别。
- [ ] 1.1 `src/chat/InputBar.tsx`：迁工具栏/操作类图标钮与文本按钮；**发送键/停止键保持不变**（豁免）。
- [ ] 1.2 `src/chat/Sidebar.tsx`：仅迁 hover-reveal 图标动作钮（如重命名/删除）；**会话行/项目行/导航项不动**。
- [ ] 1.3 `src/chat/AssistantCenter.tsx`：动作按钮 → 组件。
- [ ] 1.4 `src/chat/MessageBubble.tsx` + `AssistantMessageMeta.tsx`：复制/重试/展开等动作钮。
- [ ] 1.5 `src/chat/SkillCenter.tsx`：底部/行内动作钮（注意与设置域 SkillListSection 不同文件）。
- [ ] 1.6 `src/Lens.tsx`：动作钮（含黑色主按钮 → `variant="primary"`）。
- [ ] 1.7 `src/chat/AskUserBlock.tsx`：确认类主按钮 → `variant="primary"`（黑→accent）。
- [ ] 1.8 其余聊天文件（`ChatMarkdown`/`GeneratedFileArtifacts`/`RuntimePicker`/`ConversationContextMenu` 的动作钮/`ChatAttachments`/`ChatImageViewer` 等）逐一甄别迁移。
- [ ] 1.9 校验：lint + typecheck + test 绿；HMR 目测——聊天/Lens 视觉一致，通用主按钮呈 accent 蓝，发送键不变。
- **Gate（审查）**：与用户核对批 1 视觉（尤其 primary 变蓝、图标钮尺寸/形状归拢是否符合预期）。

## Phase 2 — 批 2：设置/onboarding 迁移（等价替换，回滚点 C）
> 目标视觉零变化：把 `<button className="kv-btn ...">` / `kv-icon-btn` 用法换成 `<Button>` / `<IconButton>`。
- [ ] 2.1 `src/settings/SettingsShell.tsx`（48 处 kv-btn，量最大，可分块提交）。
- [ ] 2.2 其余设置面板：`ConnectorsPanel`(14)/`RequestDebugPanel`/`UsageStatsPanel`/`ExternalAgentsSettings`/`EmailConnectorModal`/`ProviderModelsPicker`/`EmailConnectorForm`/`ScreenshotTranslationSettings`/`ConnectorDetailModal`/`WebSearchPanel`/`DocumentProcessingPanel` 等。
- [ ] 2.3 onboarding：`OnboardingShell`(8)/`ProviderSetupPanel`/`steps/ProviderStep`。
- [ ] 2.4 `src/components/ModelDetailDrawer.tsx`(2) 的 kv-btn。
- [ ] 2.5 校验：lint + typecheck + test 绿；HMR 目测设置/onboarding 与迁移前**逐屏一致**（重点 savebar 保存/取消键、模型页）。
- **Gate**：设置域视觉零差异确认。

## Phase 3 — 防回归与收尾（回滚点 D）
- [ ] 3.1 `CLAUDE.md`「Code Style」补一条：新动作按钮一律用 `src/components/Button` 的 `<Button>`/`<IconButton>`，勿写内联按钮样式；列表行/菜单项/tab/窗口控件/发送键为既有独立模式，不适用。
- [ ] 3.2 组件文件顶部注释写明用法与豁免边界。
- [ ] 3.3 全量校验：`npm run lint` + `npm run typecheck` + `npm test` 全绿。
- [ ] 3.4 复核 AC1–AC6 逐条；豁免清单与实际保留项一致。
- [ ] 3.5 Phase 3.3/3.4 属 Trellis Finish 阶段的 spec 更新 + commit（分批提交或按回滚点聚合）。

## 风险与回滚点
- **回滚点 A**：Phase 0 组件+CSS 一提交——出问题整体 revert 不影响现有界面。
- **回滚点 B_n**：批 1 每文件独立提交——单文件视觉异常可精准 revert。
- **回滚点 C**：批 2 因是等价替换，若某屏出现差异优先排查类名合成，再决定 revert。
- **易错点**：① `<Button>` 默认 `type="button"`，迁移表单内按钮时确认未破坏原 submit 语义；② 迁移时勿把带选中态的列表行/tab 误判为按钮；③ IconButton `label` 必填，迁移时从原 `title`/`aria-label` 取值，勿丢可访问性；④ `data-tauri-drag-region="false"` 等原生属性需随迁移透传（组件已 `...props` 支持）。
