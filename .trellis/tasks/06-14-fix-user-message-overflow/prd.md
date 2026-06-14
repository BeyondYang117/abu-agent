# 修复用户长消息在聊天气泡中横向溢出

## Goal

聊天里粘贴超长无空格内容（如长 URL / JSON 串）作为用户消息时，消息气泡被撑破、文字横向冲出窗口两侧（见用户截图：cloudflare 524 的 JSON 报错串）。需让用户消息气泡正确换行、宽度约束在 `max-w-[85%]` 内。

## Root cause（已定位）

`src/chat/MessageBubble.tsx` 用户消息块（`isUser` 分支，约 298-343）：
- 气泡列 `<div className="flex max-w-[85%] flex-col items-end gap-1">`（约 302）是 flex item，默认 `min-width:auto`(=min-content)。超长无空格 token 的 min-content 宽度 > 85%，**CSS 中 min-width 覆盖 max-width** → 气泡被撑破溢出。
- 内容 `<div className="whitespace-pre-wrap break-words ...">`（约 312）的 `break-words`(=`overflow-wrap:break-word`) **不影响 min-content 计算**，拦不住撑破。

## Requirements

- 给 `max-w-[85%] flex-col` 那层加 `min-w-0`（允许 flex item 收缩到 85% 内）。
- 内容换行从 `break-words` 改为 `overflow-wrap:anywhere`（Tailwind 任意值 `[overflow-wrap:anywhere]`），让超长 token 任意处可断、min-content 随之变小。保留 `whitespace-pre-wrap`（维持 JSON 换行/空格）。
- 若父链（`flex justify-end` 那层或上游 MessageList 容器）仍导致溢出，按需补 `min-w-0`，但优先在气泡层修。

## Acceptance Criteria

- [ ] 粘贴超长 URL/JSON 作为用户消息后，气泡不再横向溢出、内容在 85% 宽度内换行。
- [ ] 普通短消息、中英文混排、换行（pre-wrap）显示不回归。
- [ ] 不影响 assistant 消息 / 附件 / 复制删除按钮。
- [ ] `npm run typecheck` / `npm run lint` 通过；`MessageBubble.test.tsx` 仍绿。

## Out of Scope

- assistant 消息渲染（ChatMarkdown，当前正常）。
- InputBar 模式选择器（另一个任务）。

## Technical Notes

- 纯 `src/chat/MessageBubble.tsx` 用户分支的 className 调整。Tailwind v4，可用 `[overflow-wrap:anywhere]` 任意值。
