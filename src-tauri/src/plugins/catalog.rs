//! 内置插件目录：广场条目 + **给 AI 的安装规范** + 启用后注入的 Skill/MCP/提示。
//!
//! 安装由 Kivio Agent 按 `install_doc` 执行（run_command），**不是**后端静默下载。

/// 插件附带的 stdio MCP 规格：启用时挂到 chatTools.servers，关闭时禁用/断连。
#[derive(Debug, Clone)]
pub struct PluginMcpSpec {
    /// 传给二进制的参数，如 `["mcp"]` → `officecli mcp`（stdio JSON-RPC）
    pub args: &'static [&'static str],
}

#[derive(Debug, Clone)]
pub struct CatalogPlugin {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub binary: &'static str,
    pub tags: &'static [&'static str],
    pub homepage: &'static str,
    pub repo: &'static str,
    /// 官方安装器常用落点（支持 `%LOCALAPPDATA%` 等环境变量）；PATH 未刷新时仍可检测到
    pub known_binary_paths: &'static [&'static str],
    /// 官方 README 原文 URL（raw），安装时 **必须先 web_fetch 阅读**；权威安装/用法以 README 为准
    pub readme_urls: &'static [&'static str],
    /// 短 system 提示（仅 enabled 时注入）
    pub system_hint: &'static str,
    /// 本插件拥有的 skill id 列表（启用才可见）
    pub skill_ids: &'static [&'static str],
    /// 附属 Skill 正文（启用时写入磁盘并进入技能扫描）
    pub skill_md: &'static str,
    /// 可选 MCP：启用时自动注册 stdio server
    pub mcp: Option<PluginMcpSpec>,
    /// **Kivio 安装契约**（薄层）：流程/约束/验收；具体命令以 README 为准，勿与 README 冲突
    pub install_doc: &'static str,
}

pub const PLUGIN_CATALOG: &[CatalogPlugin] = &[CatalogPlugin {
    id: "officecli",
    name: "OfficeCLI",
    description: "面向 AI Agent 的 Word / Excel / PowerPoint CLI。单二进制、无需安装 Office。附带 Skill 与 MCP。",
    binary: "officecli",
    tags: &["Word", "Excel", "PowerPoint", "CLI", "Skill", "MCP"],
    homepage: "https://officecli.ai",
    repo: "https://github.com/iOfficeAI/OfficeCLI",
    // 官方 Windows 安装器默认目录；macOS/Linux 常见用户 bin
    known_binary_paths: &[
        r"%LOCALAPPDATA%\OfficeCLI\officecli.exe",
        r"%USERPROFILE%\.local\bin\officecli",
        r"%USERPROFILE%\bin\officecli",
        "/usr/local/bin/officecli",
        "/opt/homebrew/bin/officecli",
    ],
    readme_urls: &[
        // 中文优先；失败再读英文。安装与用法以仓库 README 为权威来源。
        "https://raw.githubusercontent.com/iOfficeAI/OfficeCLI/main/README_zh.md",
        "https://raw.githubusercontent.com/iOfficeAI/OfficeCLI/main/README.md",
    ],
    // Kivio 适配策略（英文）：启用后拼进 system。强调 MCP≠shell、batch、效率与官方 skill。
    system_hint: "\
### OfficeCLI (plugin: officecli)\n\
**Role.** Create/read/edit .docx / .xlsx / .pptx with OfficeCLI. Prefer this plugin over python-docx / openpyxl / python-pptx.\n\
\n\
**Two entry points (do not confuse them):**\n\
1. **MCP tool `officecli`** — preferred for normal CLI verbs. Argument is one `command` string (or argv-style list) passed to the officecli binary. This is **NOT a shell**: no bash `<<EOF` heredoc, no unquoted pipes, no shell-only redirects.\n\
2. **bash / run_command** — use when you need a real shell (write a temp JSON file, PowerShell here-strings, pipes). Then call `officecli …` inside that shell.\n\
\n\
**Skills (official set is installed).** Before substantial work, activate the matching skill (`skill` tool, or MCP `load_skill <cli-name>`):\n\
- Base strategy → `officecli`\n\
- Slides / deck → `officecli-pptx` (`load_skill pptx`); fundraising pitch → `officecli-pitch-deck`; morph motion → `morph-ppt` / `morph-ppt-3d`\n\
- Word → `officecli-docx`; academic paper → `officecli-academic-paper`; fillable form → `officecli-word-form`\n\
- Excel → `officecli-xlsx`; dashboard → `officecli-data-dashboard`; financial model → `officecli-financial-model`\n\
Do **not** start layout-heavy work without the domain skill.\n\
\n\
**batch (critical in Kivio):**\n\
- Supported forms: `batch <file> --commands '[{...}]'` OR `batch <file> --input <absolute-path.json>` OR (via **bash only**) stdin JSON.\n\
- Each JSON item is an object: `\"command\"` is the bare verb (`add`/`set`/`remove`/…); args are **sibling fields** (`parent`, `path`, `type`, `props`, …), not a nested CLI string.\n\
- **Never** put `<<EOF` / `<<'EOF'` in the MCP `command` field — it will fail. For large batches: write JSON to an **absolute** temp path with bash/`write`, then `batch … --input \"C:\\\\…\\\\cmds.json\"`.\n\
- Prefer **one batch per slide** (or per logical group of ≥3 ops) over dozens of single `add`/`set` calls. If batch fails, fix the payload; do not permanently abandon batch for the rest of the deck.\n\
\n\
**Efficiency (keep tool counts reasonable):**\n\
- Structure first (slides + titles + body), then styling. Avoid one tool call per tiny decoration when batch can carry many shapes.\n\
- For a small deck (≤5 slides): at most **one** full screenshot pass (`view … screenshot` per page or `--grid`) after the main build; a second pass only if issues remain. Do not screenshot after every minor tweak.\n\
- On syntax uncertainty: `officecli help <format> <element>` once — do not guess props.\n\
\n\
**Do NOT:**\n\
- `officecli mcp claude|cursor|vscode|…` (Kivio already registers official stdio `officecli mcp` as plugin-officecli)\n\
- `officecli watch` / `unwatch` (MCP edits do not drive watch; preview is Kivio-side when applicable)\n\
- Invent a thin custom SKILL.md instead of official skills\n\
\n\
**Done.** Tell the user the final saved file path(s).",
    // 官方 skill id = load_skill frontmatter `name`（启用时从 CLI 全量同步）
    skill_ids: OFFICECLI_OFFICIAL_SKILL_IDS,
    skill_md: "", // 空 = 使用官方 CLI 同步（见 install::sync_officecli_official_skills）
    mcp: Some(PluginMcpSpec { args: &["mcp"] }),
    install_doc: OFFICECLI_INSTALL_DOC,
}];

/// `officecli load_skill` / skills install 的完整集合（CLI 子名 → frontmatter skill id）。
/// base `officecli` 单独从 skills install 目录复制，不在此表。
pub const OFFICECLI_DOMAIN_SKILLS: &[(&str, &str)] = &[
    ("pptx", "officecli-pptx"),
    ("word", "officecli-docx"),
    ("excel", "officecli-xlsx"),
    ("morph-ppt", "morph-ppt"),
    ("morph-ppt-3d", "morph-ppt-3d"),
    ("pitch-deck", "officecli-pitch-deck"),
    ("academic-paper", "officecli-academic-paper"),
    ("data-dashboard", "officecli-data-dashboard"),
    ("financial-model", "officecli-financial-model"),
    ("word-form", "officecli-word-form"),
];

/// 插件门闸 / UI 展示用的 skill id 列表（含 base）。
pub const OFFICECLI_OFFICIAL_SKILL_IDS: &[&str] = &[
    "officecli",
    "officecli-pptx",
    "officecli-docx",
    "officecli-xlsx",
    "morph-ppt",
    "morph-ppt-3d",
    "officecli-pitch-deck",
    "officecli-academic-paper",
    "officecli-data-dashboard",
    "officecli-financial-model",
    "officecli-word-form",
];

pub fn catalog_plugin(id: &str) -> Option<&'static CatalogPlugin> {
    PLUGIN_CATALOG.iter().find(|p| p.id == id)
}

/// 插件专属补充。通用「读 GitHub / 兼容 Kivio」写在 get_install_brief 模板里。
const OFFICECLI_INSTALL_DOC: &str = r#"## 本插件补充（OfficeCLI）

| 字段 | 值 |
|------|-----|
| plugin_id | officecli |
| 命令名 | officecli |
| 官网 | https://officecli.ai |
| 常见 Windows 安装目录 | `%LOCALAPPDATA%\OfficeCLI\officecli.exe` |

### 安装阶段（本对话 · 由 Kivio AI 执行，非后台脚本）

1. 按 README 安装 **官方** `officecli` 二进制；验收 `officecli --version`。
2. **官方 Skills — 必须全量安装**（不要只装 pptx/word/excel，不要用自写 stub）：
   - `officecli skills install`（base）
   - 对 **每一个** 领域 skill 执行 `officecli skills install <name>`：
     `pptx` `word` `excel` `morph-ppt` `morph-ppt-3d` `pitch-deck` `academic-paper` `data-dashboard` `financial-model` `word-form`
   - 可用循环一次性装完（PowerShell 示例）：
     ```
     foreach ($s in 'pptx','word','excel','morph-ppt','morph-ppt-3d','pitch-deck','academic-paper','data-dashboard','financial-model','word-form') { officecli skills install $s }
     ```
   - 验收：`officecli skills list` 中上述 skill **全部**为 installed（或等价成功）；抽查 `load_skill pptx`、`load_skill pitch-deck` 能打出正文。
3. **不要**执行 `officecli mcp claude|cursor|vscode|…`（那是给其它 IDE 的）。

### 启用阶段（用户拨开关 · Kivio 运行时自动）

1. **MCP**：注册官方 stdio `plugin-officecli` = `{绝对路径} mcp`（官方内置 MCP）。
2. **Skill**：把**全部**官方 skill 接入 Kivio 对话，供 `skill` 激活。
3. **系统提示**：Kivio 适配策略（优先 MCP、禁止 watch 等）。

装完二进制和 Skills 后务必提醒用户去插件页 **启用**，否则 MCP/Skill 不会进对话。
"#;
