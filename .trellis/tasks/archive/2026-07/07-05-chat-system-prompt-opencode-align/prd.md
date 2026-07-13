# 系统提示词对齐 opencode 精简与工作纪律

## 背景

用户对比:同一任务 opencode 秒完、Kivio 啰嗦且爱多做。shell 侧根因(cmd→wmic 卡死)已在 07-05-run-command-powershell-windows 修复。剩下的是**提示词侧**:Kivio 现有 chat 系统提示词偏"能力清单 + 工具规则",缺 opencode 那种"工作方式纪律"(简洁、先答后做、不过度、遵循既有约定、验证、并行工具)。

参考 opencode 系统提示词(用户提供全文)的可借鉴段落:Tone and style / Proactiveness / Following conventions / Code style / Doing tasks / Tool usage policy / Code References。

## 方向(已与用户确认)

**取神不取形**:Kivio 是富文本 GUI(要出 Markdown 表格/报告/文件卡片),不是等宽终端。因此:
- 采纳 opencode 的**精神**:默认简洁、不写无谓前后缀、不过度工程、只做所求、先答后做、遵循既有约定、能验证就验证、非请勿提交、并行调工具、代码位置用 `file_path:line`。
- **不照搬**其 CLI 硬约束("≤4 行 / 一个词最好")——保留 Kivio 该出的结构化 Markdown/报告能力,不设死行数上限,篇幅随任务复杂度自适应。

## 需求

1. **风格/主动性纪律(始终生效,双语)**:新增一段"工作方式"提示,注入到系统提示词(prepare.rs 段落,始终附加,不随自定义人设丢失):
   - 回答只针对当前问题,去掉开场白/结语/"我接下来要做…"旁白;改完文件不必复述改了啥。
   - 篇幅随任务:简单问题一两句,复杂/报告类才展开;不为显得完整而注水。
   - 用户只问"怎么做/可不可以"时先直接回答,不擅自动手;不做用户没要求的额外工作。
2. **代码工作纪律(仅当具备 write/edit/bash 工具时,双语)**:并入 native_tools 段:
   - 改代码前看邻近文件/现有约定,模仿风格与已用库;不假设库可用,先确认项目在用。
   - 除非要求,不加代码注释。
   - 改完能验证就验证(跑已有测试、lint/typecheck);非用户明确要求不 git commit/push。
   - 代码位置用 `file_path:line`。
   - 多个独立查询/命令在同一条消息并行调多个工具,别串行。
3. **保留现有能力**:Markdown/表格、LaTeX、run_python/交付目录、记忆、Skills、知识库、Obsidian、sub-agent 等段落与规则不删不弱化;PowerShell 那段(上个任务)保持。
4. **双语一致**:zh/en 两版语义对齐。
5. **精简本身**:新增文本要克制(别自己变成啰嗦提示词);能合并进现有段落就不新开段。

## 非目标

- 不改 Lens/翻译/截图的提示词(`default_lens_system_prompt` 等)——仅 chat。
- 不改工具集、agent loop、模型层。
- 不设死回答行数上限;不禁用 Markdown/表格/报告。
- 不动用户自定义提示词(custom_system_prompt)覆盖逻辑;新纪律走"始终附加段",与自定义人设并存。

## 验收标准

- [ ] chat 系统提示词包含新的"风格/主动性"纪律(zh + en),语义对齐 opencode Tone/Proactiveness 的"神",且无 CLI 行数硬限制。
- [ ] 具备 write/edit/bash 时包含"代码工作纪律"(约定/无注释/验证/非请勿提交/并行/代码引用);不具备这些工具时不注入该段(纯聊天不受污染)。
- [ ] 现有 prepare.rs 提示词测试(18 项)全绿;新增/调整不破坏 run_python、web 工具门控、obsidian、wire alias 等断言。
- [ ] 实测(chat-probe):简单事实问题回答简短无注水;一个需要改代码的小任务里,模型体现"先看约定/并行工具/不自作主张多做"的行为;报告类任务仍能出结构化 Markdown。
- [ ] cargo check/相关 --lib 测试通过(对照 [[windows-cargo-lib-preexisting-failures]] 基线);双语文案无格式破坏。

## 备注

- 关联:07-05-run-command-powershell-windows(shell 侧)、[[windows-run-command-powershell]]。
- 落点文件预计:`settings.rs`(基座人设可轻微加"简洁"措辞)+ `src-tauri/src/chat/agent/prepare.rs`(新增/并入纪律段)。以 prepare.rs 的"始终附加段"为主,保证自定义人设下纪律仍在。
