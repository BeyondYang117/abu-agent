# 技术设计：系统提示词对齐 opencode

## 落点

- `src-tauri/src/chat/agent/prepare.rs` — 主战场。新增"工作方式"纪律段(始终附加);在 `native_tools_prompt` 内、当具备 write/edit/bash 时追加"代码工作纪律"。
- `settings.rs::default_chat_system_prompt` — 基座人设句尾"直接、清晰"→"直接、简洁"的轻微措辞(可选,低风险)。

设计原则:纪律放**始终附加段**(prepare.rs),这样即便用户设了自定义人设(custom_system_prompt 覆盖基座),工作纪律仍生效。基座只做人设,不承载纪律。

## 变更 1：新增"工作方式"段(始终附加，双语)

位置:`build_chat_system_prompt_with_segments` 中,基座/assistant/set 段之后、runtime datetime 之前(靠前,作为总纲)。用 `append_context_segment(..., "system_prompt", "System prompt", ...)` 归入系统提示段。

**zh：**
> 工作方式:回答只针对当前问题,不写无谓的开场白、结语或"我接下来要做…"这类旁白;做完文件改动后不必复述改了什么(用户看得到)。篇幅随任务而定:简单问题一两句说清,复杂或报告类任务才展开结构化输出——不要为显得完整而注水。用户只是问"怎么做/可不可以"时,先直接回答,不要擅自动手改东西,也不要做用户没要求的额外工作。

**en：**
> How you work: address only the current request — no filler preamble, no wrap-up postamble, no "here's what I'll do next" narration; after editing files you don't need to restate what changed (the user can see it). Match length to the task: answer simple questions in a sentence or two, and expand into structured output only for complex or report-style tasks — don't pad to look thorough. When the user only asks how to do something or whether it's possible, answer first; don't jump to making changes, and don't do work they didn't ask for.

## 变更 2：native_tools 段追加"代码工作纪律"（仅当 write/edit/bash 存在）

在 `native_tools_prompt()` 内计算 `has_write`(已有)与新增 `has_bash`/`has_edit`;当 `has_write || has_edit || has_bash` 为真时,在返回的 prompt 末尾追加一条(zh/en)。保持紧凑一行。

**zh：**
> 改代码前先看邻近文件与现有约定,模仿既有风格、命名和已用的库/框架;别假设某个库可用,先确认项目已在用它。除非用户要求,不要加代码注释。改完能验证就验证(跑已有测试、lint/typecheck);非用户明确要求,不要 git commit/push。引用代码位置用 `文件路径:行号`。多个互相独立的查询或命令,在同一条消息里并行调用多个工具,别一个个串行。

**en：**
> Before changing code, read neighboring files and existing conventions — mimic the current style, naming, and the libraries/frameworks already in use; never assume a library is available without confirming the project already uses it. Do not add code comments unless asked. After code changes, verify when you can (run existing tests, lint/typecheck); never git commit/push unless the user explicitly asks. Reference code locations as `file_path:line_number`. When several independent lookups or commands are needed, call multiple tools in parallel in one message instead of serially.

## 变更 3（可选）：基座人设措辞

`settings.rs::default_chat_system_prompt` 四个分支的"直接、清晰地回答"→"直接、简洁、清晰地回答";英文"Answer clearly and directly"→"Answer clearly, directly, and concisely"。低风险润色,强化默认简洁。可与变更 1 二选一或并存(变更 1 是主力)。

## 映射到 opencode 段落（覆盖核对）

| opencode | Kivio 落点 |
|---|---|
| Tone and style（简洁、无 preamble/postamble、少 token） | 变更 1 + 变更 3 |
| Proactiveness（先答后做、别多做、改完不赘述） | 变更 1 |
| Following conventions（看邻近文件、确认库、模仿风格） | 变更 2 |
| Code style（不加注释除非要求） | 变更 2 |
| Doing tasks（验证/测试、非请勿 commit） | 变更 2 |
| Tool usage policy（并行批量调工具） | 变更 2 |
| Code References（file_path:line） | 变更 2 |
| （opencode 无、Kivio 特有：Markdown/报告/交付目录/记忆/Skills/KB/PowerShell） | 全部保留不动 |

**刻意不搬**:opencode 的"≤4 行/一个词最好/one word answers"硬约束(CLI 场景);"DO NOT ADD ANY COMMENTS"用更温和的"除非要求不加注释";URL 猜测禁令(Kivio 有 web_fetch/search,场景不同,暂不加)。

## 兼容/风险

- 纯文本增量,不改控制流;现有测试断言的是"存在/缺失某工具文本",新增文本不冲突。风险点:`chat_prompt_prevents_write_file_for_inline_code_requests` 等——新纪律不与之矛盾(仍是"用户要才动文件")。跑测试确认。
- 提示词变长:新增约 2 段共 ~6 行,可接受;注意 estimate_tokens/上下文预算无硬限,不影响。
- 自定义人设:纪律在始终附加段,自定义人设下仍生效(符合需求)。

## 测试计划

- `cargo test --lib chat::agent::prepare`(经 win 脚本机制)——18 项须全绿。
- 新增断言(可选,轻量):`build_chat_system_prompt_with_segments` 在含 write/bash 工具时输出包含代码纪律关键词(如 "file_path:line"/"并行"),不含时不包含。
- 实测 chat-probe 三条:
  1. 简单事实问题(如"今天星期几")→ 简短、无注水、不调工具。
  2. 改代码小任务 → 观察是否先读约定、并行工具、不自作主张多做。
  3. 报告类任务(如上个任务的查 C 盘)→ 仍出结构化 Markdown(证明没被简洁约束砍坏)。

## 回滚点

- 三处均为纯文案增量,`git restore` 单文件即可回滚;变更 3 独立可回滚。
