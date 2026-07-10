# Design: 插件即插即用 — Kivio 运行时健壮性

## 总体思路

五项改动分四层,互不耦合,可独立验证、独立回滚:

| 需求 | 层 | 文件 |
|---|---|---|
| R1 图直喂 | MCP 结果处理 | `mcp/registry.rs`(+复用 `chat/commands.rs` 现有函数) |
| R2 审查向 | 辅助视觉 | `chat/commands.rs`(提示词常量) |
| R3 通用规范 | 系统提示 | `chat/agent/prepare.rs` |
| R4 Git Bash | native 工具 | `native_tools/shell.rs` |
| R5 hint 瘦身 | 插件目录 | `plugins/catalog.rs`(纯删) |

## R1 — MCP 工具结果图片直达模型

### 现状
- `mcp/client.rs::parse_tool_result`:image 块 → `ChatToolArtifact{data_url,…}` 进 `artifacts`,文本置 `[image: <mime>]` 占位;`follow_up_user_messages` 恒为空(client.rs:594)。
- 管子已存在:`McpToolCallResult.follow_up_user_messages` → `rounds.rs::push_tool_execution_result` 追加为 user 消息;`read` 工具的 `read_image_as_tool_result`(commands.rs:4938)是唯一用户,已验证四协议适配器兼容(Anthropic 侧合并进同一 user turn)。

### 方案
在 **`mcp/registry.rs` 的 MCP 工具执行点**(`state.mcp_call_tool` 返回后、`note_after_officecli_tool` 附近)加一步通用后处理 `attach_image_artifacts_for_model`:

1. 收集 `result.artifacts` 中 image 类 artifact(mime 以 `image/` 开头、`data_url` 非空)。无图 → 原样返回。
2. 判断主模型 vision:复用 `model_supports_vision(provider, model)`。执行点需要 conversation 的 provider/model —— 从现有上下文取(`native_ctx` 携带 conversation_id;与 `read_image_as_tool_result` 同源的取法)。
3. **vision == true**:把每个 image artifact 的 `data_url` 转成模型 image content part(新增小工具函数 `data_url_image_part`,与 `image_content_part` 并列——后者吃 path,这里吃 data_url,不落盘),塞进 `follow_up_user_messages`(一条 user 消息带多 part)。文本占位符保留(模型知道"图在下一条")。
4. **vision != true**:走 `auxiliary_vision_model_for_images` 路径。该函数吃 `&[PathBuf]`,而 MCP artifact 是 data_url —— 写临时 PNG 到 `temp_dir`(复用 `screenshot.rs` 的临时文件 GC 约定 `kivio-mcpimg-*.png`)再调用;失败降级为保留占位符。
5. **护栏**:
   - 单图 ≤ 8MB(base64 解码后),超限跳过该图,占位符改为"图过大未内联,已存 <path>"。
   - 单结果 ≤ 4 图,超出部分同上。
   - 仅当轮生效,不回填历史(rounds механизм天然如此)。

### 为什么放 registry 而不是 client
client.rs 是纯解析层,拿不到 settings/conversation(判 vision 需要);registry 执行点两者都有,且 officecli 预览钩子先例在此。

## R2 — 辅助视觉"审查向"

`analyze_chat_images_with_auxiliary_model` 所用 prompt(commands.rs 内常量/内联字符串)追加审查指令段:

> 除内容描述外,必须显式检查并报告:文字被截断/溢出容器、元素重叠、字面转义符($n、\n 等以文字出现)、明显错位、低对比度不可读。逐条列出;确无问题才写"未见视觉缺陷"。

保持双语环境兼容(该函数已按 `resolve_chat_language` 出中/英)。

## R3 — 通用行为规范(prepare.rs)

在 `build_chat_system_prompt_with_segments` 现有通用段(与工具使用纪律同级)追加三条,措辞平台无关:

1. 中间工作文件(批处理描述、审查截图、草稿)写系统临时目录;交付目录只放最终交付物。
2. 结束前删除本次任务的中间文件。
3. 给 stdio MCP 工具传文件路径必须绝对路径(server 工作目录不可预测)。

仅当 `chat_tools.enabled` 时注入(无工具的纯聊天不需要)。

## R4 — Windows run_command 优先 Git Bash

依据 `research/shell-execution-survey.md`(pi 源码 + opencode/codex issues)。

### 现状
`native_tools/shell.rs`:Windows 走 pwsh→powershell,`-Command` 经 `.arg()` 传(见记忆 windows-run-command-powershell)。

### 方案(pi 解析顺序 + PowerShell 回落 + 动态描述)
1. **检测 `find_git_bash()`**(`OnceLock<Option<PathBuf>>`,进程级缓存):
   - 已知安装位依次 existsSync:`%ProgramFiles%\Git\bin\bash.exe` → `%ProgramFiles(x86)%\Git\bin\bash.exe` → `%LocalAppData%\Programs\Git\bin\bash.exe`;
   - 回落 `where.exe bash.exe`:取第一行,**必须复核文件存在**(pi 注释:where 会返回幽灵路径),且**排除** `System32|sysnative\bash.exe`(WSL bash,/mnt/c 视图与 Windows 路径语义不符——pi 用 stdin 方案兼容它,Kivio 直接排除更安全);
   - 都无 → None。
2. **执行**:Some(bash) → `bash.exe -c <command>`(`.arg()` 单参,与 PS 同法);None → 现有 pwsh→powershell 原样。背景任务同分支;`kill_process_group`(taskkill /T /F)按 pid 杀树,不受 shell 更换影响。
3. **动态工具描述**(opencode #16479 核心教训——shell 选择必须对模型可见):`mcp/types.rs` 的 run_command description 不能再是静态字符串,按 `find_git_bash()` 结果生成:
   - bash:"Runs in Git Bash (bash syntax: pipes, heredoc, $VAR). Write Windows paths with forward slashes (C:/Users/…) — backslashes are escape chars in bash."(防 opencode #15810 `\U` 路径腐蚀)
   - PowerShell 回落:保留现有 PS 描述。
   - native_registry 构建工具列表处本就每轮执行,取缓存值零开销。
4. **环境**:继承进程 env(path_env.rs 已富化);`CREATE_NO_WINDOW`(proc.rs)照常。
5. 不做命令内容静默改写(opencode 静默腐蚀教训);`shell_path` 显式设置字段 MVP 后置。

### 风险
- 存量 PowerShell 习惯命令(`Remove-Item` 等)在 bash 下失败 → 动态 description 是主要缓解,模型见错自改;严重时后置 `shell_path`/开关。
- 检测缓存进程级,装/卸 Git 需重启 Kivio 生效(可接受,注释注明)。

## R5 — hint 瘦身(catalog.rs)

保留(≈10 行):角色一句 + skill 激活映射表 + 三条 Do NOT(bash 里跑 officecli / `officecli watch` / `officecli mcp <ide>`)+ Done 一句。
删除:入口教学、batch 教学、截图落盘+read 指引、绝对路径、效率细则(全部由 R1–R4 的能力或 R3 通用段接管)。

## 兼容与回滚

- 各项独立 commit,单项回滚不影响其他。
- R1 若某协议适配器对 follow-up 图有兼容问题(实测验证),可按 provider 关闭:vision 判断已天然按 provider 走。
- R4 保留 PowerShell 完整回落路径,删除 bash 检测即回滚。

## 验证

- Rust:`scripts/win-cargo-test.ps1`(对照基线失败集);R1/R4 加单测(parse→attach 的 artifact 过滤、bash 检测排除 System32)。
- 前端:`npm run lint && npm run typecheck && npm test`。
- E2E:chat_probe / GUI 新对话跑 5 页 PPT(含植入缺陷页),扒 conv JSON 验 AC1–AC5(方法已成熟:tool_calls 统计 + model_messages 图块检查)。
