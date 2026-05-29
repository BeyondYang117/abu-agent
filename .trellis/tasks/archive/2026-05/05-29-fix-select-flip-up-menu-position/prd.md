# fix select flip-up menu position

## Goal

修复 Settings 中 Select 下拉菜单 flip-up（向上展开）时定位错误，导致菜单飞到窗口顶部而非紧贴触发按钮上方。

## Root Cause

`src/settings/components.tsx` 中 `useSelectMenuRect`：

```
spaceAbove = rect.top - MENU_GAP - MENU_MARGIN   // = rect.top - 14
maxHeight  = min(260, spaceAbove)                 // = spaceAbove (when < 260)
top        = rect.top - MENU_GAP - maxHeight
           = rect.top - 6 - (rect.top - 6 - 8)
           = 8  ← 恒等于 MENU_MARGIN，与按钮位置无关
```

菜单顶端永远钉在 y=8，而内容高度（3 个选项 ~84px）远小于 maxHeight（~230px），视觉上菜单出现在距触发按钮很远的顶部。

## Fix

flip-up 时改用 CSS `bottom` 定位替代 `top`，让菜单底边贴着按钮向上生长：

```
bottom = viewportH - rect.top + MENU_GAP
```

这样菜单底边始终位于按钮上方 6px，内容高度决定菜单向上延伸多少，不再飞到顶部。

## Requirements

- flip-up 时菜单底边紧贴触发按钮上方 MENU_GAP (6px)
- flip-up 时 maxHeight 仍受 spaceAbove 限制（内容超出时可滚动）
- flip-down 行为不变（用 top 定位）
- 只改 `src/settings/components.tsx`

## Acceptance Criteria

- [ ] 点开 Settings > 截图翻译 > 截图处理方式，下拉菜单紧贴按钮上方出现
- [ ] 下拉菜单向下展开时行为不变
- [ ] 菜单距触发按钮不超过 MENU_GAP + MENU_MARGIN 的合理范围

## Out of Scope

- 其他组件或窗口
- 动画/过渡效果

## Technical Notes

- 文件：`src/settings/components.tsx` 函数 `useSelectMenuRect`
- `menuRect` 状态增加可选 `bottom?: number` 字段
- 菜单元素 style 同时传 `top` 和 `bottom`（CSS fixed 元素两者可共存，只设其中一个即可）
