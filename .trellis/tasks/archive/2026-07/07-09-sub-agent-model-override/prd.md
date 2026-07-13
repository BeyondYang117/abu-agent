# 子代理模型配置

## Goal

允许用户在设置中为子代理（`agent` 工具 spawn 的 sub-agent）单独指定 provider+model：主代理用高智力模型编排，子代理跑简单任务时用便宜/快速模型。默认不配置时行为与现状完全一致（跟随父会话）。

## Background（代码现状）

- 子代理模型解析在 `src-tauri/src/chat/sub_agent.rs:899-914`：agent 定义的 `model` 字段覆盖 → 否则继承父会话 `parent_conversation.model`；provider 恒为父会话 provider。
- 设置结构 `ChatToolsConfig`（`settings.rs` ~866，前端 `src/api/tauri.ts:501`）已有 `sub_agent_concurrency` 等同族字段。
- 设置 UI 中「Subagent 并发」在 `SettingsShell.tsx` ~3363，同区块可放新选择器。
- 现成组件 `ModelPairSelect`（provider:model 冒号对 + `inheritLabel` 空值继承选项）正好匹配本需求。
- kivio-code CLI 复用同一 Settings 与 agent 工具，配置自动对其生效，无需单独实现。

## Requirements

1. **新设置字段**：`chat_tools.sub_agent_provider_id` + `chat_tools.sub_agent_model`（Rust snake_case / 前端 camelCase），均默认空字符串 = 跟随父会话。遵循现有 translator/lens 的「providerId + model 两字段」惯例。
2. **解析优先级**（在 `sub_agent.rs` spawn 处实现）：
   - agent 定义 `model` 字段（最高，保留现状语义：在**父会话 provider** 下解析，忽略全局设置）
   - → 全局子代理设置（provider+model 都换，允许跨 provider）
   - → 父会话 provider+model（兜底，现状）
3. **校验与降级**：`sanitize_settings` 校验 `sub_agent_provider_id` 指向的 provider 存在；无效时重置为空（回落跟随）。运行时二次防御：provider 解析失败或无 key 则回落父会话，不报错中断 spawn。
4. **设置 UI**：在「Subagent 并发」同区块加「Subagent 模型」选择器，用 `ModelPairSelect`，`inheritLabel` 为「跟随主对话模型 / Follow chat model」；中英文案齐全。
5. **max_output_tokens 等模型相关派生值**（`chat_max_output_tokens_for_model`）基于最终选定的 provider+model 计算（现有调用点已按变量取值，确认传递正确即可）。

## Acceptance Criteria

- [ ] 默认（字段空）：子代理 provider/model 与现状完全一致，现有 Rust 测试（`sub_agent.rs` tests、`loop_tests.rs`）全绿。
- [ ] 设置全局子代理模型后：spawn 的子代理使用配置的 provider+model（含跨 provider），父会话模型不受影响；usage 记录归属正确的 provider/model。
- [ ] agent 定义带 `model` 字段时仍优先于全局设置（现状语义不回归）。
- [ ] 配置的 provider 被删除后：`sanitize_settings` 将字段重置为空，子代理回落跟随，无 panic/报错。
- [ ] 设置 UI 可选择/清除（选回「跟随」），保存后立即对下一次 spawn 生效（设置读取是每次 spawn 时点读，无需重启）。
- [ ] 新增 Rust 单测覆盖解析优先级三分支；`npm run lint` + `npm run typecheck` 通过。

## Non-Goals

- 不做 per-agent-definition 的 provider 覆盖（定义文件仍只有 model 字段）。
- 不做 per-conversation 的子代理模型选择（全局设置一处即可）。
- 不改 external_agents / 外部 CLI 代理（它们不走 `run_sub_agent`）。
