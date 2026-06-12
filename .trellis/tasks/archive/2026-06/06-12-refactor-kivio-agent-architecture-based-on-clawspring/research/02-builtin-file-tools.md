# 02 — 内置基础工具（文件操作 / Shell / 工具注册表）对比研究

> 对比对象：clawspring `tools.py`(1083 行) + `tool_registry.py`(98 行) + `agent.py`/`clawspring.py` 调度链
> vs kivio `src-tauri/src/native_tools/`(files.rs 1873 行、shell.rs 486 行、fetch.rs 576 行、mod.rs 545 行) + `mcp/types.rs`/`mcp/registry.rs` 注册分发 + `chat/agent/execute.rs` 执行管线。
> 全部引用为实际读取后的真实行号。

---

## 1. clawspring 设计精读

### 1.1 工具 Schema 与注册表

**数据结构**：`ToolDef`（tool_registry.py:12-27）是整个插件体系的核心：

```python
@dataclass
class ToolDef:
    name: str
    schema: Dict[str, Any]        # JSON schema，直接发给 API
    func: Callable[[params, config], str]
    read_only: bool = False       # 永不修改状态
    concurrent_safe: bool = False # 可与其他工具并行
```

注册表是模块级 dict（tool_registry.py:32 `_registry: Dict[str, ToolDef]`），暴露 4 个 API：
- `register_tool`（:37-39）— 同名覆盖注册，这是插件机制的基础；
- `get_tool` / `get_all_tools` / `get_tool_schemas`（:42-54）— schema 列表在每轮请求时由 `agent.py:97` 现取，所以运行中注册的工具（MCP 后台连接、插件）下一轮自动可见；
- `execute_tool`（:57-93）— 统一分发 + **头尾保留式截断**：超过 `max_output=32000` 字符时保留前 1/2 + 后 1/4，中间替换为 `[... N chars truncated ...]`（:83-91）。尾部保留是个巧妙点：编译错误、测试失败摘要通常在输出尾部。

**Schema 定义**集中在 `TOOL_SCHEMAS`（tools.py:22-310），15 个内置工具。注册时通过 name→schema map 解耦顺序（tools.py:933-934 `_schemas = {s["name"]: s for s in TOOL_SCHEMAS}`，注释明确说 "ordering changes never break this"）。

**插件式扩展**（tools.py:1047-1083）：memory / multi_agent / skill / mcp / plugin / task 各子系统的工具都通过 "import 即注册" 模式挂进同一个注册表，模块底部 `_register_builtins()`（:1044）+ 一串 `import xxx.tools` 完成全部装配。核心循环（agent.py）对工具种类零感知。

### 1.2 文件工具实现

**Read**（tools.py:351-367）：
- 不存在 / 是目录返回 `Error: ...` 字符串（:354-356），错误即工具结果，模型可自我纠正；
- `encoding="utf-8", errors="replace", newline=""`（:359）— 不抛编码异常、保留原始行尾；
- 输出 `cat -n` 风格：`f"{start+i+1:6}\t{l}"`（:365），6 字符行号 + tab，注释明确说 "matching Claude's expected format"——行号让模型能精确构造后续 Edit；
- 空文件返回 `"(empty file)"`（:363）而不是空串，避免模型误判失败。

**Write**（tools.py:370-389）：
- 自动 `mkdir(parents=True)`（:376）；
- **写后返回 unified diff**：新文件返回 `Created path (N lines)`（:381），覆盖已有文件时用 `difflib.unified_diff` 生成 diff（:383，`generate_unified_diff` 在 :333-338）并经 `maybe_truncate_diff` 截到 80 行（:340-346）。设计意图：模型在结果里"看到"自己实际改了什么，能立即发现写歪。
- `newline=""` 写入防止 Windows 双重 CRLF（:378）。

**Edit**（tools.py:392-435）— 最精细的一个：
- **精确匹配语义**：`old_string` 必须逐字符匹配，0 次命中报 `"Error: old_string not found... ensure EXACT match, including all exact leading spaces/indentation and trailing newlines"`（:412）——错误信息直接教模型怎么修复；多次命中且未 `replace_all` 时报次数并给两条出路（:413-415）；
- **CRLF 归一化匹配**（:400-427）：匹配前把 content/old/new 都归一到 `\n`；只有当文件是"纯 CRLF"（`crlf_count == lf_count` 且 >0，:403）才在写回时还原 CRLF，混合行尾文件统一落 LF。解决了"模型给的 old_string 是 LF 而文件是 CRLF 导致永远匹配不上"的经典问题；
- 写后同样返回完整 unified diff（:432-433）。

注意：**clawspring 并没有 "必须先 Read 才能 Edit" 的硬约束**（Claude Code 有，clawspring 没实现）；它靠精确匹配语义 + 失败错误信息间接达到类似效果。

**Bash**（tools.py:438-451）：`subprocess.run(shell=True)`，stdout+stderr 合并（stderr 加 `[stderr]` 前缀，:446），timeout 默认 30s，空输出返回 `"(no output)"`。无状态（schema 注明 "no cd persistence"，:70）。

**Glob**（:454-462）：`Path.glob` 排序后截 500 条。**Grep**（:473-497）：探测 ripgrep 可用性（`_has_rg`，:465-470），不可用回落 grep；三种 `output_mode`（content/files_with_matches/count）映射到 `-n/-l/-c` 旗标，`--glob` vs `--include` 按引擎切换（:489），输出截 20000 字符。复用系统级搜索引擎而非自己实现，是"借力"设计。

### 1.3 权限护栏

两层闸门：

1. **安全前缀白名单** `_SAFE_PREFIXES`（tools.py:314-324）：30+ 个只读命令前缀（`ls`、`git status`、`rg ` 等），`_is_safe_bash`（:326-328）做 `startswith` 判断。注意这只是 UX 优化（少打扰），不是安全边界——`python ` 在白名单里其实能做任何事。
2. **agent 循环内的许可门**（agent.py:127-140）：`_check_permission`（agent.py:155-170）按 `permission_mode` 三态——`accept-all` 全放行、`manual` 全要问、`auto` 下只读工具（Read/Glob/Grep/WebFetch/WebSearch 硬编码列表，:165）+ 安全 bash 放行，Write/Edit 必问。被拒后工具结果是 `"Denied: user rejected this operation"`（:134），模型据此调整。

许可交互的解耦很优雅：`run()` 是生成器，遇到需许可的调用 yield 一个 `PermissionRequest(description)` 对象（agent.py:129-131），REPL 在事件循环里收到后调 `ask_permission_interactive`（clawspring.py:327-339，含 `a` 键升级为 accept-all）并把布尔写回 `event.granted`（clawspring.py:2780），生成器恢复执行。**核心循环不依赖任何 UI**。

`tools.execute_tool` 包装层（tools.py:887-926）还保留了一份独立许可检查（Write/Edit/Bash/NotebookEdit，:911-924），agent.py 调它时传 `permission_mode="accept-all"`（agent.py:138，注释 "already gate-checked above"）跳过——双层是历史兼容产物。

**路径安全：clawspring 基本没有**。Read/Write/Edit 接受任意绝对路径，无 home 边界、无黑名单目录、无 `..` 拒绝。这是它作为本地 CLI 玩具（用户=操作者）的取舍。

### 1.4 调度与结果回传

主循环 agent.py:83-150：流式取得 `AssistantTurn` → 无 tool_calls 则 break → **严格串行**逐个执行（:123-150），每个调用 yield `ToolStart` → 许可 → `execute_tool` → yield `ToolEnd` → 以中立格式 `{"role":"tool","tool_call_id","name","content"}` 追加进 messages（:145-150）。`ToolDef.concurrent_safe` 字段定义了但**主循环根本没用**——并行执行只是预留。

工具结果一律是 **纯字符串**；providers.py:266-267 在协议适配层把中立格式转回各 API 的 `tool_result` 块。

### 1.5 设计意图总结

1. **注册表 = 唯一扩展点**：新工具（含 MCP、插件、子 agent）只需 `register_tool(ToolDef(...))`，调度/截断/schema 导出全部自动获得。
2. **错误信息是给模型看的修复指南**，不是给人看的日志。
3. **写操作回显 diff**，把"自检"塞进工具结果。
4. **行尾归一化**贯穿 Read/Write/Edit，消除跨平台匹配失败。
5. 核心循环（生成器 + 事件对象）与 UI/许可完全解耦。

---

## 2. kivio 现状

### 2.1 工具定义与"注册"

kivio 没有注册表对象；等价物是三段式硬编码：

1. **定义**：`mcp/types.rs` 每个工具一个构造函数（`native_read_file_tool` :308-329、`native_write_file_tool` :423-444、`native_edit_file_tool` :445-468、`native_run_command_tool` :556-580 等），返回 `ChatToolDefinition { id, name, description, source, input_schema, sensitive, annotations, output_schema }`（types.rs:7-21）。`sensitive: bool` 字段（write/edit/delete/move/copy/run_command 为 true）等价于 clawspring 的 `read_only` 取反。
2. **汇集**：`list_native_builtin_tool_defs`（types.rs:755-796）按 `ChatNativeToolsConfig` 开关（settings.rs:646-663，read_file/write_file/edit_file/run_command/run_python 等布尔）逐个 push；`mcp/registry.rs::list_enabled_tool_defs`（registry.rs:194-245）再叠加 mixer 图像工具、MCP 服务器工具、skill 工具，并带 TTL 缓存（`get_cached_chat_tools`）。
3. **分发**：`registry.rs::call_tool`（:310-330）按 `tool.source` 四路分支（native/skill/mixer/mcp）；native 内部 `call_native_tool`（registry.rs:511-625）是一个 **17 臂字符串 match**（"web_search"/"read_file"/"write_file"/.../"run_command"），未知名返回 `Unknown native tool: {other}`（:620）。

新增一个内置工具需要同步改 **4 处**：types.rs 构造函数、`list_native_builtin_tool_defs`、`call_native_tool` 的 match、`prepare.rs::disabled_builtin_tool_feedback` 的 `BUILTIN_NAMES` 名单（prepare.rs:159-181），还可能涉及 settings 开关与 `tool_call_parallel_eligible`（见下）。

### 2.2 执行管线（execute.rs / loop_.rs）—— 做得比 clawspring 好的部分

`execute_tool_call`（execute.rs:102-222）是一条完整的生产级管线：

- **JSON Schema 预校验**（execute.rs:133-143 → `validate_tool_arguments` :329-465，自研递归校验器支持 type/enum/required/anyOf/oneOf/additionalProperties/数值范围/maxItems），失败时回传给模型的内容明确说 "Retry this tool call with arguments that match the declared JSON schema"（:141）——clawspring 完全没有参数校验，坏参数直接 `**p` 解包抛 TypeError。
- **审批门**：`tool_requires_approval`（execute.rs:302-323）按 `approval_policy`（auto/always_confirm/默认）+ MCP annotations（destructive_hint/open_world_hint/read_only_hint）+ `sensitive` 分级；`builtin_tool_bypasses_approval`（prepare.rs:204-215）放行 todo/ask_user/memory 等内部工具。审批经 `AgentHost::request_tool_approval` trait（host.rs:38）→ `commands.rs:3596-3649`：oneshot channel + `app.emit("chat-tool-confirm", ...)` 通知 React 前端 + 60s 超时 + `tokio::select!` 监听取消。与 clawspring 的生成器 yield 同构，但是异步事件驱动版。
- **超时与取消**：每个调用 `tokio::time::timeout` + `select!` 监听 generation 失活（execute.rs:166-180），状态机 Pending→Running→Success/Error/Cancelled/Skipped，每次变更 `emit_tool_record` 推给前端实时渲染。
- **输出截断**：`limit_tool_text_for_model`（execute.rs:518-535）按 `max_tool_output_chars` 截断——但只保头部 + 截断说明，**没有 clawspring 的尾部保留**。
- **受限并行**：loop_.rs 把只读 native 工具（read_file/list_dir/search_files/glob_files/stat_path/web_search/web_fetch 白名单，`tool_call_parallel_eligible` loop_.rs:1512-1529）+ 只读 MCP 工具攒进 `parallel_batch`，每 4 个一组 `tokio::join!` 并发（`execute_parallel_chunk` loop_.rs:1218-1266，手写 1/2/3/4 臂展开）；写操作与 skill 走串行。clawspring 的 `concurrent_safe` 只是个没人读的字段，kivio 真的实现了并行。

### 2.3 文件工具（files.rs）—— 已做对的部分

- **read_file**（files.rs:105-176）：1-based offset/limit、超 2MB 拒绝整读并提示窗口化（:129-137），`read_file_window_streaming`（:183-242）按行流式只留窗口于内存；返回结构化 `ReadFileResult`（:53-67）含 `total_lines/start_line/end_line/truncated/next_offset/read_state`——`next_offset` 让模型可以续读，比 clawspring 的纯文本结果信息量大。
- **write_file**（:252-305）：`atomic_write_text`（:470-485 → `atomic_write_bytes` :564-616）临时文件 + fsync + rename 原子写、保留权限位、保留 CRLF 与 BOM；**懒惰占位符拒绝**（:273-274 + `looks_like_placeholder_content` :487-502）——已有代码文件被写入 "rest of file unchanged"/"省略" 等短语时整次拒绝，这是 clawspring 没有的反"偷懒覆盖"护栏。
- **edit_file**（:307-388）：与 clawspring 相同的精确匹配 + 唯一性语义（0 命中 :361-363、多命中报次数并提示 replace_all :364-369）；额外有 old==new 的 noop 分支返回 warning 而非报错（:338-360）。
- **自研 unified diff**（:654-849）：前后缀裁剪 + LCS DP，>250k cells 退化为粗 diff（:639、:776），多 hunk、统计 additions/removals，写入 `FileMutationResult.diff` 随 structured_content 返回。
- **并发互斥**：进程内路径锁 `FILE_MUTATION_LOCKS`（Mutex<HashSet<PathBuf>> + Condvar，:390-439），write/edit/delete/move/copy 都先 `acquire_file_mutation_locks`；registry.rs:633-648 把这些阻塞操作丢 `spawn_blocking`，避免 Condvar 等待卡死 tokio worker。
- **路径安全远超 clawspring**（mod.rs）：home 边界（`resolve_workspace_path` :84-122）、写黑名单 `.ssh/.gnupg/Keychains`（:25-30 + `assert_writable_path` :402-428）、拒绝 `..`（:466-470）、项目模式下相对路径锁死项目根 + 绝对路径显式逃逸需重新过全局规则（`resolve_project_escape_write_path` :174-200）、删除/移动用 entry 语义不跟随最终 symlink（`canonicalize_entry_or_missing` :391-400，防止"删项目内链接结果删了链接目标"）、禁删项目根（files.rs:1320-1334）、禁止目录拷进自身（:1336-1341）。测试覆盖充分（files.rs:1362-1873 共 16 个测试）。
- **shell.rs**：命令黑名单（:12-21 sudo/rm -rf / 等）、pip 安装拦截导向沙箱 run_python（:23-95）、`cd xxx &&` 前缀规范化 + 含空格路径强制 cwd 参数（:151-218）、dev server 自动后台化 + setsid 脱离进程组（:31-45、:240-287）、超时后 kill 整个进程组（:356-363）、非零退出码作为工具错误回传 stdout/stderr（:133-140）。

### 2.4 现状中的弱项

- **没有 grep/正则搜索**：`search_files`（files.rs:1088-1162）只支持**字面量子串**匹配，自己遍历目录逐文件 `read_to_string` 整文件读入（:1135），>1MB 文件直接跳过（:1132），`MAX_SEARCH_FILES=2000`（:21）上限对真实代码库太小，也没有 ripgrep 回落。clawspring 直接借 rg/grep（tools.py:473-497），能力差距巨大。
- **walk_paths 的 max_paths 截断是静默的**（files.rs:1227-1261），glob/search 在大仓库里可能漏报且模型无从得知（glob 的 `truncated` 只在"结果数达上限"时为 true，遍历被 2000 文件截断不会反映）。
- **write/edit 的模型回传太薄**：`file_mutation_tool_result`（registry.rs:653-668）的 `content` 只是 `result.summary()` 一行（如 "Updated foo.rs (+2 -1)"），完整 diff 在 `structured_content` 里。`tool_content_with_structured_output`（execute.rs:467-482）会把 structured JSON 拼进给模型的文本——所以 diff 实际能到模型，但是以整段未美化 JSON 形式，含全部字段，token 效率差于 clawspring 直接给的 80 行内裁剪 diff。
- **read_file 结果给模型的是 JSON 序列化**（registry.rs:670-682 把整个 `ReadFileResult` serde 成一行 JSON 作 content），内容里没有行号前缀。模型要构造 edit_file 的 old_string 时只能数 `start_line` 自己推算——clawspring 的 `N\tline` 格式在这点上对模型更友好。
- **`ReadFileState.already_read` 字段恒为 false**（files.rs:172、:238）——"read 先行/重复读检测"的设计意图存在（字段、scope:"full|partial"、mtime 都备好了），但没有任何调用方维护会话级读取账本；edit/write 前也不检查文件自上次读取后是否被改过（mtime 已返回但无人比对）。
- **截断只保头部**（execute.rs:525-535），长编译输出的关键错误常在尾部。
- **DSML 解析**（chat/dsml_tools.rs:45-76）是 kivio 为兼容 DeepSeek 把 tool_calls 写进 content 的修补，clawspring 无此问题（providers.py 各协议适配干净），属于 kivio 必须背负的多模型现实。

---

## 3. 差距分析

### 3.1 clawspring 有、kivio 缺失或粗糙

| # | 能力 | clawspring | kivio 现状 |
|---|------|-----------|-----------|
| G1 | **统一工具注册表** | `ToolDef` + `register_tool`，单点注册自动获得 schema 导出/分发/截断（tool_registry.py:37-93） | 4 处硬编码同步点：types.rs 构造器、list_native_builtin_tool_defs（types.rs:755）、call_native_tool 17 臂 match（registry.rs:511）、BUILTIN_NAMES（prepare.rs:159）；`tool_call_parallel_eligible` 又一份白名单（loop_.rs:1516-1526）。加一个工具改 4-5 个文件 |
| G2 | **正则/ripgrep 级代码搜索** | Grep 工具委托 rg/grep，支持 regex、glob 过滤、context、三种输出模式（tools.py:473-497） | search_files 仅字面量、整文件读入、2000 文件上限、无 context（files.rs:1088-1162） |
| G3 | **行号化 Read 输出** | `{n:6}\t{line}` 格式（tools.py:365），模型可直接引用行 | content 为裸文本拼进 JSON（registry.rs:670-682），无行号 |
| G4 | **写后 diff 直接回显给模型（裁剪过）** | unified diff ≤80 行进结果文本（tools.py:383-387、:432-433） | diff 在 structured_content JSON 里整包传输，未裁剪、未格式化（registry.rs:653-668） |
| G5 | **头+尾保留截断** | 前 1/2 + 后 1/4（tool_registry.py:83-91） | 只保头部（execute.rs:525-535） |
| G6 | **Edit 行尾归一匹配** | CRLF/LF 归一后匹配，纯 CRLF 文件写回还原（tools.py:400-427） | edit_file 直接 `content.contains(old_string)`（files.rs:361），CRLF 文件 + LF old_string 必定 0 命中（atomic_write_text 只在**写出**时归一 :476-481，匹配阶段不归一） |
| G7 | 安全 bash 前缀白名单（免审批只读命令） | `_SAFE_PREFIXES` 30+ 前缀（tools.py:314-328） | run_command 一律 sensitive=true 走审批（types.rs:575），auto 策略下虽然 `"auto" => false` 全放行（execute.rs:307），但默认策略下 `ls`/`git status` 也要确认，无分级 |
| G8 | GetDiagnostics（pyright/tsc/shellcheck 多级回落）（tools.py:686-761） | 无对应工具 |
| G9 | NotebookEdit（tools.py:553-649） | 无 |

注：G6 是真实 bug 级差距——kivio `normalize_line_endings`（files.rs:555-562）只服务写出端，edit 匹配端没有归一化，Windows 项目上 edit_file 会高频报 "old_string not found"。

### 3.2 kivio 有、clawspring 没有

- **真实的路径安全模型**：home 边界、`.ssh` 等写黑名单、项目根禁逃逸+显式逃逸、symlink entry 语义、禁删项目根（mod.rs 全文 + files.rs:1320-1341）。clawspring 零防护。
- **原子写**（临时文件+fsync+rename，files.rs:564-616）与 BOM/CRLF/权限位保留；clawspring `p.write_text` 直接覆盖，崩溃即半截文件。
- **占位符懒写拒绝**（files.rs:273、:487-553），独有的反模型偷懒护栏。
- **JSON Schema 参数预校验**（execute.rs:329-465），失败信息含字段路径，先于审批与执行。
- **工具调用全生命周期遥测**：ToolCallRecord 状态机 + trace_id/span_id + 实时 emit 给前端（execute.rs:111-131、:325-327）。
- **超时（按工具种类调整，execute.rs:484-508）与取消（generation 机制）**；clawspring 仅 Bash 有 timeout，工具执行不可中断。
- **只读工具受控并行**（loop_.rs:1512-1529 + :1187-1266）+ 文件路径互斥锁（files.rs:390-439）+ spawn_blocking 隔离（registry.rs:633-648）。
- **结构化结果**（FileMutationResult/ReadFileResult serde 给前端渲染 diff 卡片），clawspring 只有字符串。
- **shell 护栏**更细：denylist、pip 拦截、cd 前缀规范化、dev server 后台化、进程组清杀（shell.rs 全文）。
- web_fetch 的 Jina reader 回落 + 可读性判定（fetch.rs:36-100），优于 clawspring 的正则扒 HTML（tools.py:500-520）。

**结论**：kivio 的文件工具**实现质量**（安全、原子性、并发、遥测）整体高于 clawspring；落后的是**对模型的人体工学**（行号、diff 回显、错误可修复性、搜索能力）和**注册机制的可扩展性**。clawspring 的 registry 模式明显更可扩展；kivio 的 source 四路分支 + 字符串 match 已在 4+ 处产生重复名单。

---

## 4. 重构建议

### P0（bug 级 / 高收益低风险）

**P0-1 edit_file 行尾归一匹配**（工作量 ~0.5 天）
`files.rs::edit_file`（:307-388）：匹配前将 content/old_string/new_string 归一为 LF（复用 `normalize_line_endings` :555），命中计数与替换在归一文本上做；写回交给现有 `atomic_write_text` 的 CRLF 还原逻辑（:476-481，需把"原文件含 CRLF"判定从 existing_text 传入，已有）。补 Windows CRLF 测试。纯 Rust 同步代码，无 Tauri/前端影响。

**P0-2 错误信息升级为"修复指南"**（~0.5 天）
- edit_file 0 命中时（files.rs:362）从 `"old_string not found in file"` 扩成 clawspring 式提示（精确空白/缩进/换行，建议先 read_file 带 offset 确认），可附最相近行片段（简单子串模糊查找即可）；
- `call_native_tool` 的 `Unknown native tool`（registry.rs:620）附可用工具名列表。
仅改错误字符串，零风险。

**P0-3 read_file 给模型的 content 加行号**（~1 天）
`read_file_tool_result`（registry.rs:670-682）：content 改为 `start_line` 起始的 `N\tline` 文本 + 精简元数据行，structured_content 保留现有 JSON 给前端。注意前端 `ToolCallBlock` 读 structured_content 渲染，不受影响；需确认 system prompt 中对 read_file 的描述同步更新（types.rs:312）。

### P1（人体工学 / 能力补齐）

**P1-1 引入 grep 工具（regex 搜索）**（~2-3 天）
新增 `native_tools/grep.rs`：优先探测系统 ripgrep（`which rg`，缓存结果），不可用回落到内置 `regex` crate 流式逐行扫描（保持现 walk_paths 的忽略目录与 hidden 逻辑）；支持 `output_mode: content|files_with_matches|count`、`context`、`glob` 过滤。子进程用 `tokio::process::Command`（同 shell.rs 模式），路径经 `resolve_tool_read_path` 校验。同时把 `MAX_SEARCH_FILES` 提到 1 万级或改为按时间预算截断，并在结果 JSON 里显式报告 `files_scanned/truncated_by_walk`。

**P1-2 write/edit 回传裁剪后的 diff 文本**（~1 天）
`file_mutation_tool_result`（registry.rs:653-668）：content = summary + 截断到 ~80 行的 diff（diff 已在 `FileMutationResult.diff` 里，加一个 `maybe_truncate_diff` 等价函数即可）；同时把 structured_content 里的全量 diff 保留给前端。注意 `tool_content_with_structured_output`（execute.rs:467-482）会因 content 已含 diff 片段而仍拼 JSON——应为 file mutation 结果设 `structured_content` 进 record 但**不**拼进模型文本（小改 execute.rs:189-197 的拼接条件，或让 native 文件工具返回时将 structured_content 只挂 record）。

**P1-3 头+尾保留截断**（~0.5 天）
`truncate_tool_content_for_model`（execute.rs:525-535）改为前 1/2 + 后 1/4 + 中间省略标记，按 char 边界切。对 run_command 的编译/测试输出收益最大。

**P1-4 利用已有 read_state 实现"读后改"一致性检查**（~2 天）
字段已备好（files.rs:70-75 mtime/already_read）。在 `AgentHost` 或 run 级上下文挂一个 `HashMap<PathBuf, u64>`（path→上次 read 的 mtime），read_file 成功后记账；edit_file/write_file（覆盖分支）执行前比对当前 mtime：账本缺失 → warning "你尚未读取此文件"；mtime 变化 → 拒绝并提示重读。注意并行只读批次不会动账本（只读工具并行安全），账本读写放 ToolExecutor 实现处（registry.rs::call_native_tool 需要拿到 conversation 级状态——可挂 `NativeToolContext`，该结构已有 conversation_id）。需要决定是 hard fail 还是 warning，建议先 warning 观察。

### P2（架构 / 长期可扩展性）

**P2-1 内置工具注册表化**（~3-5 天，纯后端重构）
仿 clawspring `ToolDef`，在 `native_tools/mod.rs` 定义：

```rust
pub struct NativeToolDef {
    pub def: fn() -> ChatToolDefinition,          // schema 构造
    pub enabled: fn(&Settings) -> bool,            // 替代 list_native_builtin_tool_defs 的 if 链
    pub parallel_safe: bool,                       // 替代 loop_.rs:1516 白名单
    pub bypasses_approval: bool,                   // 替代 prepare.rs:204 名单
    pub call: NativeToolCallFn,                    // async 分发，替代 17 臂 match
}
```

用 `&'static [NativeToolDef]` 静态表（Rust 下比 HashMap 注册更直接；不需要运行时插件就不要 OnceLock<RwLock>）。`call` 用 `for<'a> fn(...) -> Pin<Box<dyn Future + Send + 'a>>` 或简单 enum 包两类签名（同步文件类走 spawn_blocking、异步网络类直 await）。收益：BUILTIN_NAMES、parallel_eligible、bypasses_approval、list_native_builtin_tool_defs、call_native_tool 五份名单收敛为一张表；新工具单文件单注册项。风险低（行为不变的机械重构），建议在 P1 全部落地后做，避免重构与功能改动交织。

**P2-2 diagnostics 工具（可选）**（~2 天）
对标 GetDiagnostics：探测 tsc/cargo check/eslint，按扩展名路由，输出截断。kivio 已有 run_command，模型其实能自己跑——价值在于结构化输出与免审批（只读），优先级最低。

**不建议照搬**：clawspring 的"无路径安全"、双层许可检查、`concurrent_safe` 空字段、DuckDuckGo 正则爬虫式 WebSearch（kivio 的 Tavily/Exa 集成更好）。kivio 的审批/取消/遥测/原子写体系应原样保留，是其相对 clawspring 的核心优势。
