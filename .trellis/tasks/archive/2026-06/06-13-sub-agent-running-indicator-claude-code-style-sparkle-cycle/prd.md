# 子 Agent 运行指示器 — Claude Code 风格摩点星芒

## Goal
把 SubAgentCard 头部那颗静态四芒星换成 Claude-Code 风格的**摩点星芒字符循环**动效：运行中逐帧循环 `· ✱ ✷ ✶ ✳ ✢`（原地摩变、紫色），完成/失败/取消静止。用户实测后要求更"动"，参考 Cursor/Claude Code，已选定 Claude Code 这套。

## Requirements
- 运行中（status==running）：紫色星芒字符**逐帧循环**（纯 CSS `content` @keyframes + steps，约 1.2–1.5s/轮，不引依赖、不加 JS 定时器）。
- 完成/失败/取消：静止单字符星芒（暗淡紫），终态 ✓/✗ 仍由尾部 StatusIcon 表达。
- 复用现有紫色调；暗色模式可见；补 prefers-reduced-motion 降级（静止）。
- 删除被替换掉的旧四芒星 SVG + 旧 twinkle/glow CSS（若不再被用），避免死代码。状态行流光 `chat-motion-subagent-shimmer` 保留。

## Acceptance Criteria
- [ ] 运行中子 agent 头部是循环摩变的星芒字符；完成后静止。
- [ ] reduced-motion 下静止。
- [ ] typecheck + lint 全绿；无死 CSS/未用 import。

## Out of Scope
- 不改后端、卡片结构、token 行、流光状态行、StatusIcon、其它工具。

## Technical Notes
- 组件：src/chat/ToolCallBlock.tsx 的 SubAgentCard（现用 inline SVG 四芒星 + `subagent-twinkle` 类）。
- 动效 CSS：src/index.css（现有 `subagent-twinkle`/`chat-motion-subagent-shimmer` + reduced-motion 块）。
- 调研：Claude Code 用 `· ✻ ✽ ✶ ✳ ✢` 摩点星芒循环；本任务用等价的星芒序列即可，挑渲染宽度/基线一致的字符。
