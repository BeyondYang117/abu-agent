# Chat UI 稳定性：白屏 / 闪烁 / ghost 会话

## Goal
根治长跑/高频流式下的**白屏**、频繁**闪烁**，以及中断后产生的**删不掉的 ghost 会话**（点开报"对话不存在"）。两轮 Explore 排查已定位根因（见下）。纯前端为主。

## 根因分析（已排查）
### A. 白屏（渲染风暴）
`Chat.tsx:626 showStreamSnapshotIfCurrent` 在**每个** chat-stream/chat-tool/chat-subagent/chat-user-prompt 事件都**同步触发整片消息区重渲**，无合帧。长 Orchestrate run 里进行中的 assistant 气泡累积大量 tool 卡 + 子 agent 进度，事件风暴把 WKWebView 渲染线程打爆 → 白屏。

### B. ghost 会话（in-flight 状态未清 + 失败不剔除）
- 中断（白屏）那次 run 后，崩溃/错误路径**没调 `clearConversationInFlight()`** → `inFlightConversationsRef`/`generatingConversationIds` 残留该会话。
- 侧栏乐观合并（`Sidebar.tsx:585` visibleConversations）对"仍 generating"的会话**无条件保留显示**：`generatingConversationIds.has(id) || !realIds.has(id)`。
- 结果：后端 index 已空，它仍挂侧栏；删除即使后端成功+刷新后端列表，乐观层仍留它 → **删不掉**；删除 handler 报错只 log、不本地剔除（`Sidebar.tsx:473`）。
- 点开 `getConversation` 失败（"对话不存在" 来自 `storage.rs:269 load_conversation`）→ catch 里**不剔除、不清 currentConversation**（`Chat.tsx:1004-1011`）。
- 后端没坏（删除对不存在文件宽容返回 Ok、index 一致）——纯前端状态清理缺口。

### C. 闪烁（空帧）
切换会话/流式结束时 `applyConversation(null)` + `restoreStreamingPreview(null)` 等"先清后填"序列产生空帧；高频重渲也放大闪烁。对齐 spec「Assistant Timeline Segments」契约："Finish must not blank the assistant preview before persisted content is available"。

## Requirements（修复）
1. **A 合帧**：`showStreamSnapshotIfCurrent` 改为 **requestAnimationFrame 合帧**——累积快照变更、每帧 flush 一次重渲；done 时立即 flush。事件再多也封顶 ~60fps。
2. **B1**：流式**所有终止路径**（done/cancel/error/中断/invoke 异常）都确保 `clearConversationInFlight()`（finally 兜底），不残留 generating。
3. **B2**：`handleSelectConversation`（及 reload 路径）`getConversation` 失败时——尤其 not-found——把该会话从乐观列表 + inFlightConversationsRef/generating 剔除、清 currentConversation、刷新侧栏（ghost 自动消失），并给清晰错误。
4. **B3**：`Sidebar.handleDeleteConversation` 用 `finally` 始终本地剔除 + `loadSidebarData`（即使 deleteConversation 抛错也剔除）；删"generating"会话先强制清其 in-flight，使乐观合并不再保留它。
5. **C 空帧**：审计 finish/切换/applyConversation 路径，消除"先清空再填充"的空白帧（切换时若有持久化内容直接替换、不中间清空；finish 不在 reload/返回会话应用前清掉预览）。rAF 合帧也顺带减少抖动。
6. 不改后端逻辑（后端删除/index 已正确）；若需后端配合（如 getConversation not-found 的结构化标识）可小改，但优先前端。

## Acceptance Criteria
- [ ] 长 Orchestrate run（多子 agent + 大量 web_search）不再白屏；消息区持续可见、不卡死。
- [ ] 切换会话/流式结束无可见空白闪帧。
- [ ] 中断/白屏后产生的会话不会变成删不掉的 ghost：删除立即从侧栏消失；点开不存在的会话会自动从侧栏剔除而非卡住。
- [ ] typecheck + lint + 现有前端测试（MessageBubble.test、segments.test）全绿。

## Out of Scope
- 不引入虚拟化列表（大改，本期靠合帧 + memo 即可；如仍不够再单独评估）。
- 不改 Orchestrate/子 agent 后端行为。

## Technical Notes（锚点）
- 合帧：`Chat.tsx:626 showStreamSnapshotIfCurrent`；调用点 1202/1342/1386/1428 等（stream/tool/subagent/userprompt 监听）。
- in-flight：`Chat.tsx` markConversationInFlight/clearConversationInFlight(~489-525)、inFlightConversationsRef、syncGeneratingConversationIds；send 错误处理 ~1944-1963；finish 路径 finishStreamingRun(1066)/finishStreamingRunWithConversation(1115)。
- 选择/加载失败：`Chat.tsx handleSelectConversation`(~996-1011)、reloadConversation(~1690-1705)。
- 侧栏：`Sidebar.tsx` visibleConversations(585-597)、handleDeleteConversation(473-487)、loadSidebarData(419-438)。
- 空帧：applyConversation(546)、clearStreamSnapshot(646)、restoreStreamingPreview(596)。
- spec：agent-runtime.md「Assistant Timeline Segments」finish-no-blank 契约。
