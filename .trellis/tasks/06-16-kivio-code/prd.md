# PRD — Kivio Code CLI

> Rust 终端编码 agent，复用 Kivio 现有 Rust agent loop，覆盖面与交互体验对标 **PI agent**（基准，TS monorepo，`/Users/zmair/ZM database/Kivio agent/pi`）。参考 opencode。

## 1. 背景与目标

Kivio 已有一套**与 Tauri 解耦良好**的 Rust agent 运行时（`chat/agent/` `run_agent_loop`、`chat/model/` provider 层、`native_tools/` 工具）。目标是在同一 cargo 工程内新增一个 headless 二进制 `kivio-code`，复用该运行时，做成一个可与 Claude Code / PI agent / opencode 同台的终端编码 agent。

**硬约束（owner）：**
- ❌ 不做 `run_python` / Pyodide。
- ✅ 基础工具先做扎实：`read / write / edit / bash / grep / find / ls`（+ `web_fetch`）。
- ✅ 覆盖面对标 PI agent，不偷工。
- ✅ 完整 TUI（差分渲染、editor、markdown、diff、selector、footer、主题、keybindings）。
- ✅ 交付前**全面交互测试**：无 bug、TUI 完整。是打磨级长线工程。

## 2. 研究依据

详见同任务 `research/`：
- `pi-tools.md` — PI 7 个工具的完整契约 + Kivio native_tools 差距清单。
- `pi-runtime-session.md` — PI agent loop / 消息模型 / JSONL 会话树 / compaction，及 Kivio 复用边界。
- `pi-tui.md` — PI 行级差分渲染模型 + Rust 实现策略（crossterm 而非 ratatui）+ 组件清单。
- `pi-cli-ux.md` — PI CLI 参数 / slash 命令 / keybindings / settings / trust + MVP-vs-later 分层。

## 3. 架构决策

1. **复用循环，不重写**：`run_agent_loop(config, host, executor)` 签名无 Tauri 类型 → 直接复用。
2. **CLI host/executor**：
   - `CliAgentHost: AgentHost` —— 流式/工具记录/审批渲染到终端（仿 `SubAgentHost` 形态）。
   - `CliToolExecutor: ToolExecutor` —— **直接调 `native_tools::` 纯函数**，绕开 `AppHandle`（核心工具无 app 依赖）。
3. **crate 重构**：现为 bin-only。抽 `lib.rs`（Tauri 惯用 lib+thin-main 拆分），main.rs 仅 `kivio_lib::run()`；新增 `[[bin]] kivio-code`。
4. **headless AppState**：直读 `settings.json`（serde），构造 `AppState`（无 AppHandle），组 `AgentRunConfig`。
5. **TUI**：crossterm（raw I/O + key 解码）+ 手写 PI 式**差分行渲染器**（`Component.render(width)->Vec<String>`，帧间 diff 最小化 ANSI 输出）。不使用 ratatui 的 cell-grid 模型。
6. **会话存储**：新建 JSONL-per-session（按 cwd 分目录、追加式、resume/list/continue）。

## 4. 分阶段交付

### Phase 0 — 地基：crate 重构 + CLI 脚手架
- 抽 `src-tauri/src/lib.rs`，暴露 `chat / model / native_tools / mcp / skills / agents / state / settings / api / prompts / utils` 等；`main.rs` 变薄。`Cargo.toml` 加 `[lib]` + `[[bin]] kivio-code`。
- `kivio-code` bin 打印版本即可（占位），但能编译、能 `cargo build`，**Tauri app 仍正常构建**。
- **验收**：`cargo build`、`cargo test`、Tauri `npm run build` 均通过；`cargo run --bin kivio-code -- --version` 有输出。

### Phase 1 — headless 运行时 + print 模式
- 直读 settings.json → `AppState`。
- `CliAgentHost` + `CliToolExecutor`（先挂 read/write/edit/bash/glob/grep/ls）。
- `kivio-code -p "<prompt>"`：跑通一整轮 agent loop（规划→工具→合成），结果打到 stdout；流式可见；`stdin` 管道可作为 prompt。
- clap 参数骨架：`-p/--print`、`--model`、`--provider`、`-C/--cwd`、`--no-approve` 等。
- **验收**：真实 provider 下 `-p` 完成一次含工具调用的任务（如"读取并总结某文件"），退出码正确。Rust 单测覆盖 host/executor。

### Phase 2 — 核心工具 PI 对齐
- `edit`：实现**模糊匹配**（NFKC + 智能引号/破折号/空白归一化）、对原始内容匹配、重叠检测。
- `bash`：流式 + **尾部截断**（保留结尾，~2000 行/50KB）、超时、abort。
- `grep`：`context` 行 + 长行截断；`read`：行号/offset/limit、超长行处理。
- 工具同时惠及 Tauri app（共享 native_tools）。
- **验收**：每个工具的契约级 Rust 单测，覆盖 PI 文档化的边界。

### Phase 3 — JSONL 会话存储
- 追加式 JSONL，按 cwd 分目录；记录头 + 树状条目（message / tool / compaction…）。
- `--continue` / `--resume` / `--session` / 列表。
- **验收**：建→存→列→续跑闭环；崩溃可恢复；单测覆盖序列化与 leaf→root 重建。

### Phase 4 — Rust TUI 库
- 按 `pi-tui.md` 清单顺序：Terminal(crossterm)+raw/Kitty 协商 → 宽度/ANSI 工具 → StdinBuffer → key 解码+keybindings → **差分渲染器** → Text/Box/Spacer/TruncatedText → Input(+kill-ring/undo/word-nav) → SelectList → fuzzy+autocomplete → **Editor** → Loader → Markdown → overlays → SettingsList → theme。
- **验收**：每组件可独立 demo；resize/wrap/IME 光标正确；按键矩阵单测。

### Phase 5 — 交互模式 + UX
- TUI 接 agent loop：流式渲染、工具卡片、diff 渲染、footer 状态、thinking 指示、消息队列。
- slash 命令（MVP ~11 个：/model /session /compact /new /help /quit /fork…）、`!`/`!!` 内联 shell、`@file` 引用。
- 默认 keymap（Esc 中断、Ctrl+C 清空、Ctrl+L 模型选择…）、model/session/theme selector、首次设置。
- 审批流（敏感工具终端 y/n）、project trust。
- **验收**：完整一轮交互对话端到端。

### Phase 6 — 打磨 + 全面交互测试
- 交互测试矩阵：启动/首设、模型切换、多轮、工具卡片与 diff、超长输出、resize、中断/取消、会话续跑、错误态、各 slash 命令、keybindings。
- 修复至**无 bug、TUI 完整**。
- **验收**：测试清单全过；手动 smoke 全绿；`cargo test` + `npm test` 绿。

## 5. 风险

- **Phase 0 crate 重构**触碰在用的 Tauri app —— 必须保持 app 构建与运行不破。
- **TUI 差分渲染器 + Editor** 是最大未知量，需充分单测与多终端验证。
- provider 真实调用依赖用户 settings.json 的 key —— 测试需注意成本与可重复性（优先用 fake host/录制做单测）。

## 6. 非目标（本轮）

run_python/Pyodide、文档型 Skills（pdf/docx/xlsx）、生图、RPC/JSON 模式、OAuth `/login`、`/export`/`/share`、自定义主题全套 51 token（先内置 dark/light）。
