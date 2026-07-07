# Design — 统一按钮风格：React Button 组件 + kv-btn

## 架构与边界
- 新增单一来源组件文件 `src/components/Button.tsx`，导出 `Button`、`IconButton`。
- **CSS 不新造一套**：组件把 props 映射为现有 `kv-btn` / `kv-icon-btn` 类名（+ 必要的修饰类）。唯一 CSS 改动是给 `kv-icon-btn` 补尺寸/形状修饰（R2）。
- 组件放 `src/components/`（现有共享组件目录）。设置域的 `src/settings/components.tsx` 保留其表单原语（Toggle/Select/Input），**不搬动**；其内部若有动作按钮，批 2 迁移时改为引用 `src/components/Button`。
- 组件编写范式对齐现有：`export function`、内联 prop 类型、`...props` 透传原生属性（参考 `settings/components.tsx` 的 `Input`）。

## 组件契约

### `<Button>`
```
type ButtonProps = {
  variant?: 'default' | 'primary' | 'accent' | 'ghost' | 'danger'  // 默认 'default'（中性描边=kv-btn 基类）
  size?: 'md' | 'sm'                                               // 默认 'md'
  icon?: LucideIcon                                                // 可选前置图标（渲染 <Icon/> 于文本前）
  className?: string                                               // 追加类（合并到 kv-btn 后）
} & React.ButtonHTMLAttributes<HTMLButtonElement>                  // 透传 type/disabled/onClick/title/aria-*/data-* 等
```
- 类名合成：`['kv-btn', variant !== 'default' && variant, size === 'sm' && 'sm', className].filter(Boolean).join(' ')`。
- `type` 默认 `'button'`（避免误触发表单提交）；调用方可覆盖。
- icon 渲染：`{Icon && <Icon />}{children}`；`kv-btn svg` 已有 12px 尺寸规则，无需额外样式。

### `<IconButton>`
```
type IconButtonProps = {
  variant?: 'default' | 'ghost' | 'danger'   // 默认 'default'
  size?: 'sm' | 'md' | 'lg'                  // 28 / 32 / 36，默认 'sm'
  shape?: 'square' | 'circle'                // 默认 'square'
  label: string                              // 必填 → aria-label + title（可访问性）
} & React.ButtonHTMLAttributes<HTMLButtonElement>
```
- 类名合成：`['kv-icon-btn', size, shape === 'circle' && 'circle', variant !== 'default' && variant, className]`。
- children 为图标元素（lucide `<Trash2 size={.. }/>` 等）；尺寸由 CSS 修饰控制外框，svg 由 `kv-icon-btn svg` 规则控制。

## CSS 改动（`src/index.css`）

### 关键前提修复：kv-btn 依赖的语义 token 原来非全局（本任务发现的坑）
- `.kv-btn`/`.kv-icon-btn` 依赖 `--accent`/`--accent-hover`/`--accent-soft`/`--text-onaccent`/`--bg-input`/`--border-input`/`--text`/`--text-faint`/`--bg-hover`/`--danger`/`--danger-soft`/`--shadow-sm`，这些**原本只定义在 `.kv`/`.settings-embedded`/`.onboarding-shell` 作用域**（`index.css` ~1481/~2263/~3864），聊天窗口 `.chat-window-shell` 与 Lens `.lens-floating-surface` 都不在其中 → 在聊天/Lens 里这些变量全未定义 → `kv-btn.primary` 的 `background:var(--accent)` 失效退回浅底（primary 不蓝）、hover/描边缺失。
- **修复**：把这套语义 token 提升为**全局默认**——新增 `:root {...}`（亮色，映射 `--theme-surface` 系列 + accent/text/danger 硬编码）与 `.dark {...}`（暗色显式覆盖，因 `--theme-surface` 无暗色态）两个块，值**镜像 `.kv`**（primary 亮 `#2f6ff0`/暗 `#5c8df7`），使聊天 kv-btn 与设置窗口像素级一致。
- **覆盖安全**：设置/onboarding 的 `.kv`/`.settings-embedded` 位于文件更靠后（亮色同特异性靠源码顺序）且暗色选择器更高特异性（`.dark .kv` > `.dark`），故设置域仍用自身值、观感不变。已验证聊天/Lens 无其他 `var(--accent/--text/...)` 消费者 → 零副作用。

### kv-icon-btn 尺寸/形状修饰（供 <IconButton>）
```css
/* 保留现有 .kv-icon-btn { 22×22 } 基类不动（设置域 hotkey-clear / 路径删除按钮在用） */
.kv-icon-btn.sm { width: 28px; height: 28px; border-radius: 6px; }
.kv-icon-btn.md { width: 32px; height: 32px; border-radius: 7px; }
.kv-icon-btn.lg { width: 36px; height: 36px; border-radius: 8px; }
.kv-icon-btn.circle { border-radius: 9999px; }
.kv-icon-btn.sm svg,
.kv-icon-btn.md svg,
.kv-icon-btn.lg svg { width: 15px; height: 15px; }  /* 大于基类 13px，匹配聊天视觉密度 */
```
- 说明：基类 22px 无修饰时不受影响；带 `sm/md/lg` 才放大。svg 尺寸在带尺寸修饰时统一 15px（可在实施时按目测微调 14–16px）。

## 数据流 / 迁移映射（内联 → 组件）
| 现状内联模式 | 目标 |
|---|---|
| `bg-neutral-950 text-white`（通用主按钮） | `<Button variant="primary">`（accent 蓝，R3） |
| 描边中性 `border bg-white ...` | `<Button>`（default） |
| 文本/幽灵 `hover:bg-black/[0.0x]` 系列 | `<Button variant="ghost">` |
| 删除/危险 `text-red-* hover:bg-red-*` | `<Button variant="danger">` / `<IconButton variant="danger">` |
| `size-7 place-items-center rounded-md` 图标钮 | `<IconButton size="sm">` |
| `size-8/9` 图标钮 | `<IconButton size="md"/"lg">` |
| `rounded-full` 图标钮 | `<IconButton shape="circle">` |
- 8 种 hover 透明度 → 统一收敛到 kv-btn/kv-icon-btn 的 `--bg-hover`；12 种字号 → 收敛到 kv-btn 的 12px（sm 11.5px）。
- **紧凑微钮（决策 A）**：聊天里 `h-7 px-1.5 text-[11.5px]` 类的紧凑文本/幽灵/图标微钮（导航、行内小动作）**也统一进组件**，用 `<Button variant="ghost" size="sm">` / `<IconButton size="sm">`。接受其变为 kv-btn 紧凑标准态（略矮、更一致），消除杂散 hover/字号。极少数会挤坏布局的登记豁免。
- **尺寸映射经验值**：kv-btn `md`≈27px 高（对应聊天 h-7/h-8 常规按钮）；`sm`≈22px（对应紧凑微钮）。文本按钮默认 `md`，紧凑态 `sm`。

## 豁免清单（不迁移，保持现状）
| 控件 | 位置 | 理由 |
|---|---|---|
| 发送键（珊瑚 `#e8a090` 圆形+脉冲） | `InputBar.tsx:1892` | 签名控件，独有品牌色与动效 |
| 停止键（`bg-neutral-900` 圆形，与发送同槽 crossfade） | `InputBar.tsx:1901` | 与发送键成对的特殊态，随发送键一起豁免 |
| 窗口控件（min/max/close） | `chat/WindowControls.tsx` | OS chrome，非动作按钮 |
| 列表行/导航项、菜单项、分段/tab | Sidebar 等 | 各自独立交互模式（见 prd Out of scope） |
| 带"选中/激活"态的 toggle 钮 | Lens 箭头标注(`bg-blue-500` active)、InputBar 模式 pill、Sidebar 底栏设置齿轮(aria-pressed) | kv-btn/kv-icon-btn 变体不建模 active/selected 态，属分段/toggle 模式 |
| 超小移除/清除微控件（<28px） | ChatAttachments 移除钮(h-4/h-5=16/20px)、SkillSelector 内联清除 X(16px in pill) | 比 IconButton 最小档 28px 还小，套用会撑坏紧凑宿主布局（决策 A 豁免） |
| 内容/媒体点击目标 | ChatMarkdown 引用 chip `[n]`、图片缩略图点击区、GeneratedFileArtifacts 卡片行、ChatAttachments 图片/文件 chip 主体 | 非离散动作按钮，是行/媒体点击目标 |
| 复合下拉/选择器触发器 | Lens 历史下拉、RuntimePicker chip/模型选择器、SkillSelector 主触发 | 复合菜单/选择器触发器，非简单动作按钮 |
| HotkeyRecorder 录制态钮（`kv-btn ${recording?'accent':''}`） | `settings/components.tsx:411` | accent 表示「录制中」active 态；`<Button>` variant 不建模 accent/active，属 toggle 模式（批 2 发现） |
- 说明：Lens 发送键为珊瑚 `#D97757`（与 InputBar `#e8a090` 同为签名送出键角色），一并豁免。
- 实施中若发现新的合理特例，追加到本表并注明理由（对应 AC5）。

## 兼容性 / 回滚
- 纯前端、无后端契约变化；无数据迁移。
- 批 2 对设置域是**等价替换**（kv-btn 类不变），视觉零风险。
- 回滚点：组件文件 + CSS 修饰为独立提交；每批迁移独立提交，可按文件粒度 `git revert`。
- 风险面：`kv-icon-btn` 新增修饰类若命名与既有类冲突→已检索确认 `.sm/.md/.lg/.circle` 未被 `kv-icon-btn` 复用，仅作用于带该基类的元素。

## 权衡
- 选"组件包 kv-btn"而非"纯 CSS 类"或"新造 tokens"：兼得 React 类型安全 + 复用成熟 CSS，改动面最小、风险最低（用户已选定）。
- 务实完成度而非 0 残留：优先拿到视觉一致与主要收益，边缘个例登记豁免（用户已选定）。
