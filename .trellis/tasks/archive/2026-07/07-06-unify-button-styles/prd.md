# 统一按钮风格：React Button 组件 + kv-btn

## Goal
消除全项目动作按钮的样式碎片化，建立单一、类型安全的按钮来源。用户价值：视觉一致（尤其聊天/Lens 域）、未来新按钮统一走一个组件、不再各写各的内联样式。

## Background
- 全项目 **392 个 `<button>`**，**无共享 React Button 组件**（`src/components/` 仅有 `ModelDetailDrawer.tsx`）。其中大量并非设计系统意义的按钮（列表行、菜单项、分段/tab、窗口控件）——见「Out of scope」。
- 存在"两套世界"：
  - **设置/onboarding 域**：成熟的 `kv-btn` CSS 系统（`src/index.css:2817`+）。变体 `primary`/`accent`/`ghost`/`danger`，尺寸 `sm`，配套 `kv-icon-btn`（固定 22×22，`index.css:2783`）。SettingsShell 用了 48 处 kv-btn。
  - **聊天/Lens 域**：几乎全是内联 Tailwind，一处一样。
- 碎片化硬数据（聊天域）：hover 底色 **8 种**透明度（`black/[0.035|0.04|0.05|0.06]`、`white/[0.06|0.07|0.08|0.1]`）；文本字号 **12 种**（10–17px）；图标钮尺寸混用 `size-7/8/9/11`(28/32/36/44px)+`p-0.5`，`rounded-md`(14) 与 `rounded-full`(14) 混用；通用主按钮 `bg-neutral-950 text-white`(聊天) vs `kv-btn.primary`(设置=accent)。
- **关键技术约束（已验证）**：`.kv-btn` 基类**全局可用**（非 scope 于 `.settings-embedded`），聊天窗口直接套 kv-btn 类即可、无需重写 CSS；`.settings-embedded-savebar`/`.onboarding-topbar` 只是上下文覆盖。`.kv-btn` 用 `cursor: default`（桌面应用刻意）。CSS 变量（`--accent` 亮 `#2563eb`/暗 `#3b82f6` 等）全局可用。

## Requirements
- **R1 组件**：新建 `src/components/Button.tsx`，导出 `<Button>` 与 `<IconButton>`。内部**输出现有 kv-btn / kv-icon-btn 类**（一套 React API + 一套 CSS 源）。
  - `<Button>`：`variant: 'default' | 'primary' | 'accent' | 'ghost' | 'danger'`（默认 `default`=中性描边），`size: 'md' | 'sm'`（默认 `md`）。透传 `disabled`/`title`/`onClick`/`type`/`aria-*`/`className`(可追加)/`data-tauri-drag-region` 等原生属性；支持前置 icon。
  - `<IconButton>`：`variant`（复用 `ghost`/`danger` 等），`size: 'sm'(28)|'md'(32)|'lg'(36)`（默认 `sm`），`shape: 'square'|'circle'`（默认 `square`）。
- **R2 CSS 扩展**：为 `kv-icon-btn` 增补 `.sm/.md/.lg` 尺寸修饰与 `.circle` 形状修饰；**保留现有 22×22 基类**给设置域既有用法。不改 `kv-btn` 现有语义。
- **R3 统一 primary**：全项目通用主按钮 = `kv-btn.primary`（accent 蓝）。聊天里黑色 `bg-neutral-950 text-white` 通用主按钮迁移为 `<Button variant="primary">`。
- **R4 批次迁移**（两批都做，见验收）：
  - 批 1：聊天/Lens 的内联**动作按钮** → `<Button>`/`<IconButton>`。
  - 批 2：设置/onboarding 的 kv-btn 用法 → `<Button>`/`<IconButton>`（视觉零变化，统一调用形态）。
- **R5 防回归**：在 `CLAUDE.md` 与组件注释写明"新动作按钮一律用 `<Button>`/`<IconButton>`，勿写内联"。（暂不加 ESLint 规则。）

## Acceptance criteria
- [ ] AC1：`src/components/Button.tsx` 提供 `<Button>`/`<IconButton>`，variant/size/shape 按 R1 落地，渲染出的类名等价于对应 kv-btn/kv-icon-btn。
- [ ] AC2：`kv-icon-btn` 的 `.sm/.md/.lg/.circle` 修饰生效；设置域原有 22×22 图标钮视觉不变。
- [ ] AC3：聊天/Lens 域的动作按钮全部迁移到组件；通用主按钮呈 accent 蓝；**发送键保持珊瑚色圆形+脉冲不变**。
- [ ] AC4：设置/onboarding 的动作按钮迁移到组件，且**视觉与迁移前一致**（截图/HMR 目测无差异）。
- [ ] AC5：务实完成度——动作按钮无遗留内联样式；确需保留的特例登记在 design.md「豁免清单」并注明理由。
- [ ] AC6：`npm run lint`（--max-warnings 0）、`npm run typecheck`、`npm test` 全绿；分批在真机(HMR)目测视觉一致。

## Out of scope
- **非动作按钮**（各自独立模式，本任务不动）：列表行/导航项（如 Sidebar 会话/项目行）、菜单项（下拉/右键）、分段控件/标签页（`kv-seg`/tab）、窗口控件（`WindowControls` 最小化/最大化/关闭）。
- **发送键**：珊瑚色 `#e8a090` 圆形+脉冲（`InputBar.tsx:1892`）为签名控件，列入豁免清单、保持不变。
- 输入框/下拉/开关等非按钮控件（Input/Select/Toggle）。
- 技能行里"启用 pill + Toggle"的冗余（非按钮）。

## Open questions
- （无阻塞项；实施中按逐文件甄别判据处理边界个例，登记到豁免清单。）
