# brainstorm: 助手中心功能

## Goal

把 Chat 侧边栏中当前不可用的“中心”改名为“助手中心”，用于创建、管理、配置并启动不同类型的助手。该功能应复用 Kivio 已有的 Chat、Skill、MCP 工具、默认模型、上下文管理能力，让用户能把“提示词 + 模型 + 工具/技能 + 使用场景”保存为可重复使用的助手，而不是每次在单个对话里临时选择。

## What I already know

* 用户希望把截图中侧边栏的“中心”改成“助手中心”。
* 用户希望助手中心可以“添加和设定各种助手”。
* 用户要求先结合项目现状，并参考网络上其他软件的类似功能，再判断这个功能应该怎么做、应该包含什么。
* 项目已有 Chat 主窗口，侧边栏包含“新建聊天 / 新建项目 / 搜索 / 中心 / 设置”，其中“中心”目前 disabled。
* 项目已有 `SkillSelector`，可以在单个对话上选择 Skill；Skill 支持名称、描述、来源、推荐工具、禁用模型调用等元数据。
* 项目已有 Chat 工具体系：MCP 工具、内置工具、Skill 目录、工具确认、工具调用记录、默认模型配置和上下文状态。
* `Conversation` 已持久化 `active_skill_id`，但还没有独立的 Assistant/Profile 概念。

## Assumptions (temporary)

* “助手”应当是一个可复用配置，而不是一个聊天记录分类。
* 助手中心第一版应优先服务桌面小体量应用，避免做成复杂平台/市场。
* 助手可以创建新聊天，也可以应用到当前聊天；但具体 MVP 入口需要收敛。
* 现有 Skill 可以作为助手的能力来源之一，但助手不应完全等同于 Skill。

## Open Questions

* MVP 中助手是否需要绑定知识库/文件，还是先只支持提示词、模型、Skill、工具和启动问题？

## Research References

* [`research/assistant-center-patterns.md`](research/assistant-center-patterns.md) — comparable products treat custom assistants as reusable profiles combining identity, instructions, starters, model/runtime choices, tools/capabilities, optional knowledge/context, and lifecycle actions.

## Research Notes

* OpenAI GPTs expose name, description, conversation starters, instructions, knowledge, recommended model, capabilities/actions, preview/testing, version history, duplicate/delete/share.
* Claude Projects separate project chat history, knowledge base, and project instructions; Claude Skills are focused repeatable workflows with metadata, instructions, resources, scripts, and tests.
* Gemini Gems / Managed Agents validate the "save prompt + tools + files/skills as an invokable assistant ID" model.
* Dify Agents emphasize persona, output format, constraints, workflow steps, tool-use guidance, knowledge base, and preview/debug.
* Poe prompt bots include bot identity, base model, prompt, optional knowledge base, greeting, markdown/temperature, and create/start flow.

## Recommended MVP Shape

* The Assistant Center should be a local Chat subview, not a standalone settings tab and not a marketplace.
* Assistants should be stored as reusable `ChatAssistant` profiles: name, description, visual marker, system prompt, provider/model override, optional Skill, tool permission preset, conversation starters, greeting, enabled/archived flag, timestamps.
* Creating a chat from an assistant should store `assistant_id` plus an `assistant_snapshot` on the conversation. The snapshot prevents old conversations from changing behavior when the assistant is edited later.
* Prompt composition should layer global Chat defaults with assistant-specific instructions, then existing runtime/tool/Skill context.
* Skill remains a capability inside an assistant. Assistant is broader than Skill because it also chooses prompt, model, starters, tool policy, and chat launch behavior.
* First version should provide a few built-in preset assistants users can duplicate/edit, for example: 通用助手, 翻译润色助手, 截图分析助手, 编程/数据助手, 写作助手.

## Requirements (evolving)

* 侧边栏“中心”改名为“助手中心”，并变成可点击入口。
* 助手中心展示可用助手列表，支持搜索、创建、编辑、复制、删除/归档助手。
* 每个助手至少包含名称、描述、系统提示词、默认模型、可选 Skill、可选工具策略、开场白/示例问题。
* 从助手创建聊天时，对话应记住使用的助手 ID 和配置快照，避免之后编辑助手导致历史对话语义漂移。
* 对话标题栏或消息输入区应显示当前助手，并允许返回助手中心或切换/清除助手。
* UI 应贴合现有 Chat 窗口气质：轻量、密集、可扫描，不做营销式卡片页。

## Acceptance Criteria (evolving)

* [ ] 侧边栏显示“助手中心”并能打开对应视图。
* [ ] 用户能新增助手并保存基础配置。
* [ ] 用户能从助手中心启动一个带助手配置的新聊天。
* [ ] 用户能编辑已有助手配置。
* [ ] 对话发送消息时能按助手配置注入系统提示词/Skill/工具策略。
* [ ] 使用助手创建的对话保存助手快照；编辑助手后，旧对话行为不被静默改写。
* [ ] 空状态和无 provider/model 的状态有清晰可用的提示。

## Definition of Done (team quality bar)

* Tests added/updated where appropriate.
* `npm run lint` passes.
* `npm run typecheck` passes.
* `cargo test --manifest-path src-tauri/Cargo.toml` passes when backend persistence or command behavior changes.
* Docs/notes updated if behavior changes.
* Rollout/rollback considered if risky.

## Out of Scope (explicit)

* 第一版不做公开市场、云端分享、团队协作或权限系统。
* 第一版不做复杂工作流编排/多 Agent 图形流程。
* 第一版不做独立知识库索引/RAG，除非后续确认这是必须项。
* 第一版不做助手版本历史和回滚；只做当前配置和对话快照。

## Technical Notes

* `src/chat/Sidebar.tsx`：侧边栏已有 disabled 的“中心”行。
* `src/chat/SkillSelector.tsx`：已有 Skill 选择、预览、来源标签、推荐工具展示。
* `src/chat/types.ts`：已有 Skill、Conversation、ChatProject 类型；Conversation 目前只有 `active_skill_id`。
* `src/chat/Chat.tsx`：Chat 主视图目前在 `conversation` / `settings` 间切换，后续可扩展 Assistant Center 视图。
* `.trellis/spec/frontend/type-safety.md`：记录 Chat 命令、Skill、工具和上下文状态契约，后续实现前需要重新阅读。
