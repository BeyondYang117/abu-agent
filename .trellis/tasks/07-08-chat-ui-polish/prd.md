# 聊天界面质感优化（空态首屏）

## 背景

用户反馈聊天空态首屏"不够高级、缺质感"。对照截图与代码勘察，多数基础（表面层级、输入框多层阴影、图标统一 18px/1.75 stroke、accent #e8a090）已存在，真实差距集中在三处：

1. **点阵背景观感差**（最大减分项）：`ChatDotGridBackground.tsx` 中每个点的基础透明度含大幅随机分量（`base = 0.08 + depth * 0.14`，接近 3 倍差异），叠加流动光带动画后，静态一眼看去深浅不均、成片断裂，像渲染噪点而非设计语言。
2. **Hero 标题字重默认 400**：`chat-empty-hero-title` 无字重类，中英文标题显得松、廉价。
3. **发送按钮空态存在感错误**：disabled 态为实心灰圆（`disabled:bg-neutral-300`），视觉重量与可用态相同，看起来像"坏掉的按钮"而非"待激活"。

## Requirements

- R1 点阵背景均匀、安静：压缩每点随机明度差异，整体透明度下调，光带动画增益降低——静止观感为均匀细点阵，动画退为极轻微流光。暗色模式同步生效。
- R2 Hero 标题增加字重（medium 档）并收紧字距，提升排版精度。
- ~~R3 发送按钮 disabled 态改描边幽灵样式~~（已实施后用户反馈不好看，回退为原实心灰样式；此项撤销）
- R4 不改变现有布局、事件契约、动画架构（canvas 分桶渲染、reduced-motion 分支、暗色监听均保持）。

## Acceptance Criteria

- [x] A1 空态首屏点阵无成片深浅斑块；动画光带可感知但不抢焦点。（代码已改，待人工目检确认）
- [x] A2 标题字重/字距变化在浅色+暗色下均正常。
- ~~A3 发送按钮空态~~（R3 已撤销，恢复原样式）
- [x] A4 `npm run lint`、`npm run typecheck` 通过；`npm test` 无新增失败。（37 files / 201 tests 全过）
- [ ] A5 亮/暗、三套浅色主题（default/warm/cool）下无样式回归（人工目检待做）。

## 范围外

- 侧栏/主区表面层级重构（已有实现，观感可接受）
- 输入框阴影系统（已是多层阴影）
- accent color 体系调整

## 涉及文件

- `src/chat/ChatDotGridBackground.tsx`（R1）
- `src/chat/Chat.tsx` 空态 hero 标题 className（R2）
- `src/chat/InputBar.tsx` 发送按钮 disabled 样式（R3）
