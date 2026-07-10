# Research: 各终端代理的 shell 执行策略(R4 依据)

调研时间:2026-07-10。来源:pi-mono 源码(本地 clone 分析)+ opencode/codex GitHub issues。

## pi(badlogic/pi-mono)— 源码级分析

`packages/coding-agent/src/utils/shell.ts` + `src/core/tools/bash.ts`(470 行):

**解析顺序(`getShellConfig`)**:
1. 用户显式 `shellPath`(settings.json)— 不存在则报错;
2. Windows:`%ProgramFiles%\Git\bin\bash.exe` → `%ProgramFiles(x86)%\Git\bin\bash.exe`(existsSync 探测);
3. 回落:`where bash.exe` 找 PATH 上的 bash(Cygwin/MSYS2/WSL),**必须 existsSync 复核**(注释明说 `where` 会返回幽灵路径);
4. 都没有 → **硬报错**,错误信息引导安装 Git for Windows / 加 PATH / 设 shellPath。**pi 没有 PowerShell 回落——bash-only**。
5. Unix:`/bin/bash` → `which bash` → 回落 `sh -c`。

**WSL legacy bash 特殊处理**(`isLegacyWslBashPath`):`C:\Windows\System32|sysnative\bash.exe` 不排除,而是改用 **stdin 传命令**(`bash -s`,命令写 stdin),因为 WSL bash 的 argv 处理有问题。普通 bash 用 `-c <command>` 单 argv。

**其他工程细节**:
- 检测无缓存(每次 exec 调 getShellConfig,existsSync 很便宜);
- `getShellEnv()`:把自家 bin 目录 prepend 到 PATH;
- 输出截断:尾部 N 行 / KB 上限,截断时全量落临时文件并告知模型路径;
- `killProcessTree`:Windows `taskkill /F /T /PID`,Unix `kill(-pid, SIGKILL)`(与 Kivio kill_process_group 一致);
- 工具名就叫 `bash`,description 直说 "Execute a bash command"。

## opencode — issue 挖掘(踩坑清单,Kivio 必须避开)

- **#16479**:shell 自动检测对模型不可见 → 模型不知道该写哪种语法。修复方向:**工具 description 按实际选中的 shell 动态生成**。
- **#5557**:有时 WSL2 有时 PowerShell,用户/模型都困惑;工具叫 "bash" 但可能跑 PowerShell,名不副实。
- **#11330**:bash 的 `X=y` 赋值语法漏进 PowerShell → CommandNotFoundException。
- **#15810**(最重要):两种 shell 都会**静默腐蚀命令**——cmd 的多行/引号/`%` 展开问题;Git Bash 下模型写 Windows 反斜杠路径 `C:\Users\...`,bash 把 `\U` 当转义吃掉 → 路径变乱码。缓解:让模型在 bash 下用**正斜杠路径**(Git Bash 接受 `C:/Users/...`)。
- **#11954**:Git bash.exe 与 PTY 实现的 ACCESS_VIOLATION(Kivio 无 PTY,不适用,但注意别引入 conpty)。

## codex

**Windows 硬编码 PowerShell**,模型不熟悉 PS 语法,用户抱怨且无法切换(issue #16717 请求可配置 shell)——反面教材:与"模型的母语是 bash"事实相悖。

## Claude Code

官方要求 Windows 安装 Git Bash(装机前提),bash-only。

## 对 Kivio R4 的设计结论

1. **解析顺序采用 pi 方案**:settings 显式 shell_path(可选)→ Git Bash 已知位置(ProgramFiles/x86/LocalAppData)→ `where bash.exe` + existsSync 复核;**与 pi 不同:查无 bash 回落 PowerShell**(保护无 Git 用户,零回归)。
2. **System32/sysnative 的 WSL bash 直接排除**(不学 pi 的 stdin 方案):WSL bash 看到的是 /mnt/c 文件系统视图,与 Kivio 传的 Windows 路径语义完全不符,用了必坏。
3. **工具 description 必须动态**(opencode 最大教训):选中 bash → 描述写明"bash 语法,Windows 路径用正斜杠 C:/…";回落 PowerShell → 保留现有 PS 描述。模型永远知道自己面对哪个 shell。
4. **路径腐蚀防护**:description 明示正斜杠;不做命令内容改写(静默改写正是 opencode 踩的坑)。
5. 检测结果缓存(OnceLock)——pi 不缓存是因为 node existsSync 便宜,Rust 侧进程级缓存更干净;设置变更时失效(如提供 shell_path 设置则监听变更,MVP 可仅进程级缓存+文档注明重启生效)。
6. 命令经 `-c` 单 argv 传(与现 PS `.arg()` 同法);超时/后台任务/taskkill 机制沿用现有。
