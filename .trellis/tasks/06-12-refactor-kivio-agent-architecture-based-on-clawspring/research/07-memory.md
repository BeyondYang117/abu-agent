# 研究文档 07：Memory 子系统（clawspring → kivio）

> 范围：持久化记忆的存储模型、检索、system prompt 注入、会话末固化（consolidation）、工具注册与安全校验。
> clawspring 侧：`memory/` 共 1260 行 —— `store.py`（300）、`tools.py`（288）、`context.py`（225）、`scan.py`（144）、`consolidator.py`（128）、`types.py`（86）、`__init__.py`（89）；外加顶层 `context.py:153-165` 与 `clawspring.py:1214-1252` 的接线。
> kivio 侧：`src-tauri/src/chat/memory.rs`（560 行）、`mcp/types.rs:609-680`（工具 schema）、`mcp/registry.rs:573-583`（分发）、`chat/agent/prepare.rs:347-358`（注入）、`chat/commands.rs:74-85`（请求期读取）、`settings.rs:510-525`（开关）。

---

## 1. clawspring 设计精读

### 1.1 存储模型：每条记忆一个 .md + 自动重建索引

存储布局（`memory/store.py:1-10` 模块注释）：

```
user scope    : ~/.clawspring/memory/<slug>.md
project scope : <cwd>/.clawspring/memory/<slug>.md
MEMORY.md     : 每个目录一个索引文件，save/delete 后自动重建
```

核心数据结构 `MemoryEntry`（`store.py:47-74`）字段相当丰富：

| 字段 | 含义 |
|---|---|
| `name` / `description` | 名称（slug 化为文件名，`_slugify` `store.py:79-83`）+ 一行描述（用于相关性判断） |
| `type` | `user` \| `feedback` \| `project` \| `reference` 四分类（`types.py:8`） |
| `scope` | `user` / `project` 双作用域 |
| `confidence` | 0.0–1.0 可靠度分数，默认 1.0 = 用户明确陈述 |
| `source` | `user` / `model` / `tool` / `consolidator` —— 记忆来源溯源 |
| `last_used_at` | 最近被检索命中的日期（`touch_last_used` `store.py:272-300`，MemorySearch 命中即更新） |
| `conflict_group` | 关联/冲突记忆的分组标签（如 `writing_style`） |

每个文件是 YAML frontmatter + 正文（`_format_entry_md` `store.py:105-124`）。`save_memory`（`store.py:129-145`）写文件后立刻 `_rewrite_index`（`store.py:224-235`）从全部 .md 重建 `MEMORY.md`，每行 `- [name](file.md) — description`——索引永远与文件一致，无需手动维护（这正是 Claude Code memory 目录的格式）。

**冲突检测**：`check_conflict`（`store.py:247-269`）在保存前检查同名记忆是否已存在且内容不同，返回旧记忆的 `confidence`/`source`/`created`，供调用方决定是否覆盖并在工具结果里告知模型（`tools.py:38-45` 输出 "⚠ Replaced conflicting memory ... Old content: ..."）。

### 1.2 索引注入：每轮 system prompt 都带 MEMORY.md

顶层 `context.py:153-165` `build_system_prompt` 每次组装 system prompt 时调用 `get_memory_context()`（`memory/context.py:71-102`）：

- 合并 user scope 与 project scope 的 `MEMORY.md` 原文（project 段加 `[Project memories]` 标签，`context.py:94`）；
- 经 `truncate_index_content`（`context.py:28-66`）做 **行数 + 字节双限截断**（`MAX_INDEX_LINES=200` / `MAX_INDEX_BYTES=25_000`，`store.py:24-25`），先按行截再按字节截，并在尾部附加说明哪个限制触发的 WARNING——把"索引过大"这一事实反馈给模型，引导它写短索引行。

即：**注入的是索引（一行一条），不是全文**；正文按需经 MemorySearch 工具取回。这是 token 成本和召回率之间的关键折中。

### 1.3 检索：关键词 → confidence×时间衰减排序 → 可选 AI 重排

- `search_memory`（`store.py:209-221`）：大小写不敏感关键词匹配 name+description+content。
- `find_relevant_memories`（`memory/context.py:107-153`）：关键词初筛后默认按 mtime 新旧排序取 top-N；`use_ai=True` 时走 `_ai_select_memories`（`context.py:156-225`），用一个小模型调用（默认 haiku，`context.py:189`）从候选清单中选索引，失败回退关键词结果。
- `_memory_search` 工具（`tools.py:57-102`）再做一次 **`rank = confidence × exp(-age_days/30)`** 重排（`tools.py:74-80`，约 21 天半衰期），并对每条命中调用 `touch_last_used` 维护使用时间（`tools.py:84-86`）。

### 1.4 新鲜度/陈旧度治理

`scan.py` 镜像 Claude Code 的 memoryScan/memoryAge：

- `scan_memory_dir`（`scan.py:45-76`）只读每个文件前 30 行 frontmatter（性能），按 mtime 新→旧排序，上限 `MAX_MEMORY_FILES=200`。
- `memory_freshness_text`（`scan.py:109-123`）对 >1 天的记忆生成陈旧度警告文本（"point-in-time observations, not live state … verify against current code"），MemorySearch 结果中逐条附带（`tools.py:90`）。动机注释明确写了：用户报告过"过期的 file:line 引用被当作事实断言"。

### 1.5 会话末固化（consolidator）—— 生命周期钩子

`consolidator.py:47-128` `consolidate_session(messages, config)`：

1. 会话 < `MIN_MESSAGES_TO_CONSOLIDATE=8` 条消息直接跳过（`:17, :57`）；
2. 取最近 40 条消息、每条截 600 字符拼成压缩转写（`:65-79`）;
3. 用一次轻量 AI 调用（`max_tokens=1024, no_tools`，`:82-91`）按 `_SYSTEM` prompt（`:19-44`）抽取记忆，**硬上限 3 条**、自动抽取的 confidence 起步 0.8（低于用户显式保存的 1.0）;
4. 保存前 `check_conflict`：**不覆盖 confidence 更高的既有记忆**（`:117-120`）;
5. 保存时 `source="consolidator"`（`:114`），全程 try/except 静默失败（`:127-128`）。

`_SYSTEM` prompt（`:23-43`）的取舍清单很关键：只抽用户偏好/纠正、显式项目决策、对 AI 的行为反馈；**不抽**代码模式、架构、git 历史、CLAUDE.md 已有内容、临时任务态——与 `types.py:31-40` `WHAT_NOT_TO_SAVE` 一致（"可从代码库推导的不要存"）。

> 准确性说明：当前调用点是 `/memory consolidate` slash 命令（`clawspring.py:1220-1227`，handler `cmd_memory` 在 `clawspring.py:1214` 起），即**手动触发**；docstring（`consolidator.py:3-4`）声明也可"programmatically after a session"调用，但仓库内没有自动调用点。它是为会话末钩子设计的，只是 clawspring 把触发权交给了用户。

### 1.6 工具面：四个注册工具 + prompt 引导

`tools.py:134-288` 注册 `MemorySave` / `MemoryDelete` / `MemorySearch` / `MemoryList`，schema 的 description 本身就是行为引导（MemorySave 的 description 教模型 feedback/project 类型要写 "rule → **Why:** → **How to apply:**" 结构，`tools.py:138-145`）。`types.py:55-86` `MEMORY_SYSTEM_PROMPT` 提供完整使用指引（含"基于记忆推荐前先验证文件/函数仍存在"的防陈旧条款，`types.py:84-85`）。

### 1.7 设计意图小结

- **索引/正文分离**：每轮只付索引的 token 成本，全文按需检索；
- **元数据驱动治理**：confidence/source/last_used_at/mtime 四个信号支撑排序、冲突解决与陈旧度警告；
- **写入门槛**：分类学 + WHAT_NOT_TO_SAVE + 固化上限 3 条，对抗"记忆膨胀成噪声"；
- **双 scope**：用户偏好跨项目共享，项目事实留在项目目录（可进 git）。

---

## 2. kivio 现状

### 2.1 存储模型：L1/L2 两个整体 markdown 文件

`chat/memory.rs` 是一套与 clawspring 完全不同的 **双层单文件** 设计：

- 目录：`app_data_dir/chat-memory/`（`memory.rs:18, 96-104`），全局唯一，无 per-project scope；
- `L1.md` 在线记忆 + `L2.md` 长期记忆（`memory.rs:19-20`），`MemoryLayer` 枚举（`:26-61`）；
- **L1 有 5000 字节硬预算**（`L1_MAX_BYTES`，`:16`），`validate_memory_content`（`:369-378`）超限拒绝写入并提示"把细节移到 L2"；
- L2 无上限、**永不自动注入**（工具 description 明示，`mcp/types.rs:646`）。

### 2.2 注入与 token 记账（这部分做得对）

- `l1_prompt_block`（`memory.rs:151-167`）：L1 非空才返回注入块；**超过 5000B 时返回 Err、拒绝注入**（防御外部手改文件撑爆上下文）；
- `chat_memory_prompt_for_request`（`chat/commands.rs:74-85`）按 `settings.chat_memory.enabled`（`settings.rs:510-525`）读取，错误转成 warning；warning 会写进对话 `context_state.warning` 回传前端（`commands.rs:1103, 2641`）；
- `prepare.rs:347-358` 把 L1 作为 `memory_l1` 上下文段经 `append_context_segment`（`prepare.rs:558-579`）拼入 system prompt，**同时产生 `ContextUsageSegment{estimated_tokens}`**（`estimate_tokens` `prepare.rs:769`），参与前端上下文占用条显示——clawspring 没有任何 token 记账。

### 2.3 工具面：memory_read / memory_modify

- schema：`mcp/types.rs:609-640`（read）、`:642-680`（modify）；`memory_enabled` 时注册（`mcp/types.rs:758, 790-792`）；分发在 `mcp/registry.rs:573-583`；`prepare.rs:175-176, 214` 把它们列为无需确认的原生工具（system prompt 中也明示，`prepare.rs:861, 876`）。
- `tool_read`（`memory.rs:197-237`）：L2 支持 `query` 参数——`l2_read_slice`（`:460-478`）找到**精确文本匹配**后切出所在 heading 段返回；无匹配返回 "No exact text match"。
- `tool_modify`（`memory.rs:239-277`）四种操作：
  - `append`：L2 支持按 heading 插入（`append_to_heading_or_end` `:426-458`，heading 不存在则创建）；
  - `replace` / `remove`：**Edit 工具式唯一匹配校验** `ensure_unique_match`（`:358-367`，0 次/多次匹配都报错）；
  - `archive`（`:319-340`）：L1 → L2 的降级搬运（move/copy 两模式），是 L1 字节预算的配套泄压阀。

### 2.4 安全与可靠性（kivio 独有）

- `validate_secret_free`（`memory.rs:380-414`）：拒绝写入疑似密钥（`sk-`、`ghp_`、`-----BEGIN` 等）与提示注入文本（"ignore previous instructions"、"system prompt"）——clawspring 完全没有这层；
- `atomic_write`（`:496-534`）：tmp 文件 + rename + 3 次退避重试；clawspring 直接 `write_text`；
- 单元测试覆盖（`:536-560`）。

### 2.5 前端联动

`chat_memory_get/save/open_folder` 三个 Tauri command（`memory.rs:169-195`），前端 `src/api/tauri.ts:1115-1121` 封装，设置页可直接查看/编辑两层内容；`chat/storage.rs:1129` 把 memory 工具归入"记忆"数据连接器分组。

### 2.6 缺失确认

`grep memory chat/agent/loop_.rs` 零命中——**循环/会话生命周期里没有任何 memory 钩子**：没有自动固化、没有手动 consolidate 命令、没有检索类工具（只有精确文本 read）。记忆完全依赖模型在对话中"想起来"主动调 `memory_modify`。

---

## 3. 差距分析

### clawspring 有、kivio 缺失/粗糙

| # | 能力 | clawspring | kivio 现状 |
|---|---|---|---|
| G1 | **会话末 AI 固化** | `consolidator.py:47-128`：≤3 条、confidence 0.8、不覆盖更高置信记忆 | 完全没有；记忆写入全靠模型对话中自觉 |
| G2 | **条目化 + 元数据** | 每条一个文件，frontmatter 含 type/confidence/source/last_used_at/conflict_group | 两个扁平 md 文件，无任何条目元数据，无法排序/溯源/判陈旧 |
| G3 | **关键词检索与排序** | `search_memory` + `rank=confidence×exp(-age/30)`（`tools.py:74-80`）+ 可选 AI 重排 | `l2_read_slice` 仅精确文本匹配（`memory.rs:464`），查询词稍有出入即 miss |
| G4 | **陈旧度警告** | `memory_freshness_text`（`scan.py:109-123`）逐条附加 | 无 mtime/created 概念 |
| G5 | **user/project 双 scope** | `store.py:28-42`；project 记忆随仓库走 | 单一全局目录；kivio 明明有项目会话概念（`commands.rs:89-103` project binding）却没有项目记忆 |
| G6 | **冲突检测** | `check_conflict`（`store.py:247-269`）+ conflict_group | replace 的唯一匹配校验只防误编辑，不防语义冲突 |
| G7 | **写入分类学与引导** | 四类型 + WHAT_NOT_TO_SAVE + Why/How-to-apply 结构（`types.py`） | 工具 description 仅一句话，无"什么不该存"的引导 |

### kivio 有、clawspring 没有

| # | 能力 | kivio | clawspring |
|---|---|---|---|
| K1 | **L1 字节硬预算 + archive 泄压** | 5000B 强制上限（`memory.rs:369-378`）+ L1→L2 archive（`:319-340`），主动容量治理 | 索引超 200 行/25KB 只是被动截断 + warning |
| K2 | **token 记账进 UI** | `memory_l1` 段计入 `ContextUsageSegment`（`prepare.rs:347-358, 573-577`） | 无 |
| K3 | **秘密/提示注入过滤** | `validate_secret_free`（`memory.rs:380-414`） | 无（记忆文件可被写入任意内容并逐轮注入 system prompt） |
| K4 | **原子写 + 重试** | `atomic_write`（`:496-534`） | 直接 write_text |
| K5 | **精确编辑语义** | replace/remove 唯一匹配校验（`:358-367`），可做细粒度修订 | 只能整条覆盖或删除 |
| K6 | **前端可视化编辑** | 设置页直接读写两层 + 打开目录（`memory.rs:169-195`、`tauri.ts:1115-1121`） | 仅 CLI `/memory` 查看 |
| K7 | **错误回传对话** | 超限/读失败 warning 进 `context_state.warning`（`commands.rs:2641`） | 静默 |

结论：kivio 在**写入安全、容量治理、工程可靠性、前端联动**上领先；clawspring 在**记忆生命周期（固化）、可检索性、元数据治理**上领先。两者互补而非单向落后。

---

## 4. 重构建议

### P0-A：会话末固化钩子（呼应文档 01 的 P0-1 loop_.rs 拆分）

- **位置**：放在 agent loop 拆分后的 finalize 阶段（文档 01 P0-1 规划的收尾层），而非循环内——固化是会话级钩子，不应阻塞单轮回复。
- **实现**：新增 `chat/memory_consolidate.rs`：
  - `pub async fn consolidate_conversation(app, conversation_id)`：复刻 `consolidator.py` 逻辑——消息数 <8 跳过、取末 40 条各截 600 字符、一次轻量模型调用（复用 `api.rs` 的非流式发送 + 现有 provider 凭据解析）、解析 JSON、≤3 条；
  - 产出写入 **L1 的固定 heading**（如 `## Session insights`，借 `append_to_heading_or_end`），超 L1 预算时落 L2 并提示；
  - 触发：对话空闲/关闭时由 `tokio::spawn` 异步执行，完成后 `app.emit("chat-memory-consolidated", …)` 通知前端徽标提示；设置项 `chat_memory.auto_consolidate: bool` 默认关（尊重用户对自动写记忆的敏感度），另暴露手动 Tauri command。
  - 写入前过 `validate_secret_free`（kivio 既有优势保留）。
- **工作量**：2–3 天（prompt 移植 + 非流式调用封装 + 设置/前端开关）。

### P0-B：memory 段纳入循环内上下文治理（呼应文档 01 P0-2）

- `prepare.rs` 已产出 `memory_l1` 的 `estimated_tokens`（`:347-358`），但压缩/裁剪决策目前不消费它。要求：P0-2 的 token 预算器把 `memory_l1` 列为**不可压缩的固定开销**参与预算计算（它有 5000B 自身上限，预算器只需读数，不需要裁它）。工作量：随 P0-2 顺带，0.5 天。

### P1-A：L2 条目化 + memory_search 工具

- 不推翻 L1/L2 文件模型（前端编辑、字节预算都依赖它），改为**约定 L2 以 `## heading` 为条目边界**，每条目首行可选元数据注释（`<!-- created: 2026-06-12 source: consolidator confidence: 0.8 -->`）；
- 新增 `memory_search` 原生工具（`mcp/types.rs` 加 schema、`registry.rs` 加分发）：分词关键词匹配（中英文皆需，参考 `utils.rs` 语言检测）+ 按元数据 created 做 `exp(-age/30)` 衰减排序，替代现在的精确匹配 `l2_read_slice`；保留 `memory_read` 的 query 路径作为精确读取。
- 检索结果对 >7 天条目附加陈旧度提示（移植 `scan.py:109-123` 文案）。
- **工作量**：3–4 天。

### P1-B：project scope 记忆

- 项目会话（`commands.rs:89-103` 已有 project binding）增加 `<project_root>/.kivio/memory.md`，注入时与全局 L1 并列为 `memory_project` 段（同样计 token）；`memory_modify` 增加 `scope: "project"` 参数。注意 Tauri fs 权限走现有 native_tools/files.rs 的路径校验逻辑。**工作量**：2 天。

### P2

- 固化时的冲突检测（同 heading 下语义重复检查，可先做精确去重）；
- `use_ai` 检索重排（小模型二次调用）；
- 工具 description 补充 WHAT_NOT_TO_SAVE 式引导（`types.py:31-40` 文案直接移植进 `mcp/types.rs:646` description 与 `prepare.rs` system prompt 段）。

### 与文档 05（subagent）的交叉决策

建议：子 agent **共享只读 L1 注入**（偏好对子任务同样有效、且 L1 有 5000B 上限不会膨胀子 agent 上下文），但**不注册 memory_modify**（避免并发写同一文件 + 子 agent 缺少全局会话视角误写记忆）；子 agent 产生的可固化洞察由父会话 finalize 阶段统一经 P0-A 固化。这同时回避了 `atomic_write` 在多 agent 并发下的最后写入者获胜问题。
