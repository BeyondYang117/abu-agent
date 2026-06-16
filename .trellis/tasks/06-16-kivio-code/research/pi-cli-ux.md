# Research: PI coding-agent CLI & UX surface (for Kivio Code)

- **Query**: Document PI agent's user-facing CLI + interactive UX so Kivio Code (Rust terminal coding agent) can match it.
- **Scope**: external codebase study (PI TS monorepo at `/Users/zmair/ZM database/Kivio agent/pi`)
- **Date**: 2026-06-16
- **PI package root studied**: `packages/coding-agent/`

All paths below are relative to `/Users/zmair/ZM database/Kivio agent/pi/packages/coding-agent/` unless absolute.

---

## 1. Entry & startup flow

- Binary entry: `src/cli.ts` → sets `process.title`, configures HTTP dispatcher, calls `main(process.argv.slice(2))`.
- `src/main.ts:457 main()` is the orchestrator. Order of operations:
  1. `--offline` / `PI_OFFLINE` short-circuit (disables network/version check) — `main.ts:459`.
  2. Windows self-update quarantine cleanup — `main.ts:465`.
  3. **Package subcommands** (`install`/`remove`/`uninstall`/`update`/`list`) via `handlePackageCommand` and `config` via `handleConfigCommand` — `main.ts:469-476`. These run before normal arg parsing and exit.
  4. `parseArgs(args)` (`src/cli/args.ts:63`) → emits diagnostics (warnings/errors); error diagnostics `exit(1)`.
  5. `--version` prints `VERSION` and exits; `--export <file> [out]` exports a session to HTML and exits (`main.ts:490-507`).
  6. **Mode resolution** `resolveAppMode` (`main.ts:98`): `rpc`/`json` if `--mode`; else `print` if `--print` OR stdin not a TTY OR stdout not a TTY; else `interactive`. Piped stdin in an otherwise interactive invocation downgrades to `print` (`main.ts:741`).
  7. `runMigrations(cwd)` — keybindings/auth/settings migrations + deprecation warnings (`migrations.ts`).
  8. Build `SettingsManager` (startup, cwd-scoped), optionally run **first-time setup** (only when `PI_EXPERIMENTAL=1`, official distribution, default agent dir, no settings.json yet — `cli/startup-ui.ts:54 shouldRunFirstTimeSetup`).
  9. Resolve session dir precedence: `--session-dir` > `PI_CODING_AGENT_SESSION_DIR` env > settings `sessionDir` (`main.ts:544-548`).
  10. `createSessionManager` resolves which session to open (new / continue / resume / fork / by id / path / ephemeral — see §6).
  11. Project-trust resolution (`core/project-trust.ts`, `core/trust-manager.ts`), then build the agent-session runtime (`core/agent-session-runtime.ts`).
  12. `--help` prints help (`printHelp`, `args.ts:212`) and exits; `--list-models [search]` lists models (`cli/list-models.ts`) and exits.
  13. Dispatch: `runRpcMode` | `InteractiveMode.run()` | `runPrintMode` (`main.ts:779-823`).
- `process.title = APP_NAME` and `PI_CODING_AGENT=true` set in `cli.ts:12-13`.

### App name / config-dir branding
`src/config.ts:473-482` reads `package.json` `piConfig` to derive `APP_NAME` (default `pi`), `CONFIG_DIR_NAME` (default `.pi`), and env-var names `PI_CODING_AGENT_DIR` / `PI_CODING_AGENT_SESSION_DIR`. The whole tool is rebrandable from package.json. **For Kivio Code: pick `kivio`/`.kivio` analogues.**

### Config/agent paths (`config.ts:500-552`)
| Path helper | Location |
|---|---|
| `getAgentDir()` | `~/.pi/agent/` (override `PI_CODING_AGENT_DIR`) |
| settings.json | `~/.pi/agent/settings.json` |
| auth.json | `~/.pi/agent/auth.json` (chmod 600) |
| models.json | `~/.pi/agent/models.json` |
| keybindings.json | `~/.pi/agent/keybindings.json` |
| themes dir | `~/.pi/agent/themes/` |
| prompts dir | `~/.pi/agent/prompts/` |
| sessions dir | `~/.pi/agent/sessions/` |
| trust.json | `~/.pi/agent/trust.json` |
| debug log | `~/.pi/agent/pi-debug.log` |
| Project-scoped | `.pi/settings.json`, `.pi/themes/`, `.pi/SYSTEM.md`, `.pi/sessions` etc. |

---

## 2. CLI invocation surface (full flag list)

Source of truth: `src/cli/args.ts:63-210` (parser) and `args.ts:212-389` (`printHelp`). Usage: `pi [options] [@files...] [messages...]`.

### Subcommands (handled before arg parsing, `package-manager-cli.ts`)
- `pi install <source> [-l]` — install extension/package source (`-l` = project-local).
- `pi remove <source> [-l]` / `pi uninstall <source> [-l]` — remove.
- `pi update [source|self|pi]` (`--extensions`, `--self`, `--extension <src>`) — update pi + packages.
- `pi list` — list installed packages.
- `pi config` — TUI to enable/disable package resources.
- `pi <subcmd> --help` — per-subcommand help.

### Flags (positional bareword tokens become `messages[]`; `@token` become `fileArgs[]`)
| Flag (aliases) | Arg | Effect | args.ts line |
|---|---|---|---|
| `--help` `-h` | – | help | 74 |
| `--version` `-v` | – | version | 76 |
| `--mode` | `text\|json\|rpc` | output mode | 78 |
| `--print` `-p` | optional next msg | non-interactive one-shot; consumes next non-flag/non-@ token as a message | 140 |
| `--continue` `-c` | – | continue most recent session | 83 |
| `--resume` `-r` | – | interactive session picker at startup | 85 |
| `--provider` | name | provider (default `google`) | 87 |
| `--model` | pattern | `provider/id` and `:<thinking>` shorthand supported | 89 |
| `--api-key` | key | runtime key for the chosen provider | 91 |
| `--system-prompt` | text | replace default system prompt | 93 |
| `--append-system-prompt` | text | repeatable; append text/file to prompt | 95 |
| `--name` `-n` | name | session display name | 98 |
| `--no-session` | – | ephemeral (no save) | 104 |
| `--session` | path\|id | open specific session (partial UUID match) | 106 |
| `--session-id` | id | use exact project session id, create if missing | 108 |
| `--fork` | path\|id | fork session into new file | 110 |
| `--session-dir` | dir | session storage dir | 112 |
| `--models` | csv patterns | scoped models for Ctrl+P cycling (glob + fuzzy) | 114 |
| `--no-tools` `-nt` | – | disable all tools | 116 |
| `--no-builtin-tools` `-nbt` | – | disable builtins, keep ext/custom | 118 |
| `--tools` `-t` | csv | allowlist of tool names | 120 |
| `--exclude-tools` `-xt` | csv | denylist of tool names | 125 |
| `--thinking` | level | `off\|minimal\|low\|medium\|high\|xhigh` | 130 |
| `--extension` `-e` | path | repeatable; load extension | 149 |
| `--no-extensions` `-ne` | – | disable extension discovery | 152 |
| `--skill` | path | repeatable; load skill | 154 |
| `--prompt-template` | path | repeatable | 157 |
| `--theme` | path | repeatable; load theme | 160 |
| `--no-skills` `-ns` | – | disable skill discovery | 163 |
| `--no-prompt-templates` `-np` | – | disable prompt templates | 165 |
| `--no-themes` | – | disable theme discovery | 167 |
| `--no-context-files` `-nc` | – | disable AGENTS.md/CLAUDE.md | 169 |
| `--list-models` | optional search | list models w/ fuzzy filter | 171 |
| `--verbose` | – | force verbose startup | 178 |
| `--approve` `-a` | – | trust project files for this run | 180 |
| `--no-approve` `-na` | – | ignore project files for this run | 182 |
| `--offline` | – | no startup network ops | 184 |
| `--export` | file | export session to HTML (handled in main) | 147 |
| `@file` | – | attach file (text inlined as `<file name=...>`, images attached) | 186 |
| `--unknown=value` / `--unknown value` | – | captured into `unknownFlags` for extensions | 188 |
| `-x` (single dash, unknown) | – | error "Unknown option" | 202 |

Thinking levels constant: `args.ts:57 VALID_THINKING_LEVELS = ["off","minimal","low","medium","high","xhigh"]`.

### Print mode behavior (`modes/print-mode.ts`)
- `--mode text` (default print): sends `initialMessage` then each `messages[]` to `session.prompt`, prints only the **final assistant text content** to stdout (`print-mode.ts:129-146`). Exit code 1 if the last assistant message stop reason is `error`/`aborted` (error text → stderr).
- `--mode json`: prints session header then each agent event as one JSON line (`print-mode.ts:104-117`). See `docs/json.md`.
- `--mode rpc`: JSON-RPC over stdin/stdout; `@file` args rejected (`main.ts:515`). See `docs/rpc.md`.
- **stdin piping**: `readPipedStdin` (`main.ts:56`) reads piped stdin (only when stdin is not a TTY) and merges into the initial prompt via `cli/initial-message.ts` (`buildInitialMessage`). `cat README.md | pi -p "..."` works.
- Signal handling: SIGTERM → exit 143, SIGHUP → exit 129 (`print-mode.ts:47-62`); kills tracked detached children.

### Exit codes
- 0 normal; 1 on arg error / runtime diagnostic error / "no models available" in non-interactive / print-mode assistant error.
- `process.exit(0)` for `--version`, `--help`, `--list-models`, `--export` success.

---

## 3. Slash commands (interactive)

Built-in list: `src/core/slash-commands.ts:18-41` `BUILTIN_SLASH_COMMANDS`. Typed `/` opens command completion in the editor; extensions add custom commands; skills appear as `/skill:name`; prompt templates expand via `/templatename` (`docs/usage.md:34`).

| Command | Description (from slash-commands.ts) |
|---|---|
| `/settings` | Open settings menu (thinking, theme, message delivery, transport — `settings-selector.ts`) |
| `/model` | Model selector UI (`model-selector.ts`) |
| `/scoped-models` | Enable/disable models for Ctrl+P cycling (`scoped-models-selector.ts`) |
| `/export [file]` | Export session (HTML default; `.html`/`.jsonl` path) |
| `/import` | Import + resume a session from a JSONL file |
| `/share` | Share session as a secret GitHub gist (HTML link) |
| `/copy` | Copy last agent message to clipboard |
| `/name <name>` | Set session display name |
| `/session` | Show session info + stats (file, id, message count, tokens, cost) |
| `/changelog` | Show changelog entries |
| `/hotkeys` | Show all keyboard shortcuts |
| `/fork` | New fork from a previous user message (user-message selector) |
| `/clone` | Duplicate current active branch into a new session file |
| `/tree` | Navigate session tree / switch branches (`tree-selector.ts`) |
| `/trust` | Save project trust decision for future sessions |
| `/login` | Configure provider authentication (OAuth/API key) |
| `/logout` | Remove provider authentication |
| `/new` | Start a new session |
| `/compact [prompt]` | Manually compact context (optional custom instructions) |
| `/resume` | Resume a different session (picker) |
| `/reload` | Reload keybindings, extensions, skills, prompts, themes, context files |
| `/quit` | Quit |

Dispatch: a few are handled inline in `interactive-mode.ts` (`/compact` line 2638, `/reload` 2644, `/resume` 2664, `/quit` 2669, plus hidden easter eggs `/debug`, `/arminsayshi`, `/dementedelves`). The rest are routed through the agent session's command bindings (extension-style command context — `print-mode.ts:74-97` shows the `commandContextActions` interface: `waitForIdle`, `newSession`, `fork`, `navigateTree`, `switchSession`, `reload`). There is **no `/clear`** built-in (clearing the editor is `Ctrl+C`; starting fresh is `/new`); docs mention `/privacy` for analytics (first-time-setup.ts:74).

### Inline shell ("bash mode") — `interactive-mode.ts:2507, 2676`
- `!command` runs a shell command and **sends output to the model context**.
- `!!command` runs but **excludes output from context**.
- Editor border turns `bashMode` color when text starts with `!` (`isBashMode`, line 2507).
- Blocks if a bash command is already running (must Esc to cancel).

---

## 4. Keybindings (full default keymap)

Source: `src/core/keybindings.ts:63-202` (app-level) + TUI defaults documented in `docs/keybindings.md`. Customizable via `~/.pi/agent/keybindings.json` (each id → single key or array). Legacy non-namespaced ids auto-migrate (`keybindings.ts:204-264`). `/reload` applies changes live.

### App-level (`app.*`)
| Action | Default | Description |
|---|---|---|
| `app.interrupt` | `escape` | Cancel/abort (also restores queued messages) |
| `app.clear` | `ctrl+c` | Clear editor |
| `app.exit` | `ctrl+d` | Exit when editor empty |
| `app.suspend` | `ctrl+z` (none on Windows) | Suspend to background |
| `app.thinking.cycle` | `shift+tab` | Cycle thinking level |
| `app.thinking.toggle` | `ctrl+t` | Toggle thinking blocks visibility |
| `app.model.cycleForward` | `ctrl+p` | Next scoped model |
| `app.model.cycleBackward` | `shift+ctrl+p` | Previous scoped model |
| `app.model.select` | `ctrl+l` | Open model selector |
| `app.tools.expand` | `ctrl+o` | Toggle tool output expansion |
| `app.editor.external` | `ctrl+g` | Open `$VISUAL`/`$EDITOR` |
| `app.message.followUp` | `alt+enter` | Queue follow-up message |
| `app.message.dequeue` | `alt+up` | Restore queued messages to editor |
| `app.clipboard.pasteImage` | `ctrl+v` (`alt+v` Windows) | Paste image |
| `app.session.new/tree/fork/resume` | *(none)* | Session ops (also slash) |

Session-picker / tree / scoped-models selectors each rebind `ctrl+*` keys contextually (e.g. `ctrl+p` toggles path in picker, toggles provider in scoped-models). Full tables: `keybindings.ts:90-201`, `docs/keybindings.md:92-152`, `docs/sessions.md:88-99`.

### TUI editor (from `pi-tui` defaults; `docs/keybindings.md:25-79`)
- Cursor: arrows, `ctrl+b/f` (left/right), `alt+left`/`ctrl+left`/`alt+b` (word left), `home`/`ctrl+a`, `end`/`ctrl+e`, `pageUp`/`pageDown`.
- Delete: `backspace`, `delete`/`ctrl+d` (fwd), `ctrl+w`/`alt+backspace` (word back), `alt+d` (word fwd), `ctrl+u` (to line start), `ctrl+k` (to line end).
- Kill-ring: `ctrl+y` yank, `alt+y` yank-pop, `ctrl+-` undo.
- Input: `enter` submit, `shift+enter` (Win Terminal `ctrl+enter`) newline, `tab` autocomplete/path completion.
- Clipboard/select: `ctrl+c` copy selection; select lists `up/down`, `pageUp/pageDown`, `enter` confirm, `escape`/`ctrl+c` cancel.
- `@` → fuzzy file reference; `Tab` → path completion.

`doubleEscapeAction` setting (`tree`/`fork`/`none`) controls double-Esc behavior (`interactive-mode.ts:3996`).

---

## 5. Config / settings

### Files & precedence (`docs/settings.md`)
- Global: `~/.pi/agent/settings.json`. Project: `.pi/settings.json` (loaded only after trust). **Project overrides global; nested objects deep-merge.**
- Resource paths in global settings resolve relative to `~/.pi/agent`; in project settings relative to `.pi`.

### Settings schema (`docs/settings.md:24-273`, impl `core/settings-manager.ts`)
- **Model/thinking**: `defaultProvider`, `defaultModel`, `defaultThinkingLevel`, `hideThinkingBlock`, `thinkingBudgets` (per-level token budgets), `enabledModels` (Ctrl+P cycle patterns).
- **UI/display**: `theme` (default `dark`), `quietStartup`, `defaultProjectTrust` (`ask`/`always`/`never`, global only), `collapseChangelog`, `enableInstallTelemetry`, `enableAnalytics`, `trackingId`, `doubleEscapeAction`, `treeFilterMode`, `editorPaddingX`, `autocompleteMaxVisible`, `showHardwareCursor`.
- **Compaction**: `compaction.enabled/reserveTokens/keepRecentTokens`; `branchSummary.reserveTokens/skipPrompt`.
- **Retry**: `retry.enabled/maxRetries/baseDelayMs` + `retry.provider.timeoutMs/maxRetries/maxRetryDelayMs`.
- **Message delivery**: `steeringMode` / `followUpMode` (`all`/`one-at-a-time`), `transport`, `httpIdleTimeoutMs`, `websocketConnectTimeoutMs`.
- **Terminal/images**: `terminal.showImages/imageWidthCells/clearOnShrink`, `images.autoResize/blockImages`.
- **Shell**: `shellPath`, `shellCommandPrefix`, `npmCommand`.
- **Sessions**: `sessionDir`. **Markdown**: `markdown.codeBlockIndent`.
- **Resources**: `packages[]`, `extensions[]`, `skills[]`, `prompts[]`, `themes[]`, `enableSkillCommands`. Arrays support globs + `!`/`+`/`-` include/exclude.
- **Warnings**: `warnings.anthropicExtraUsage`.

### Auth storage (`core/auth-storage.ts`)
- `auth.json` (mode 0600) holds per-provider credentials: `{type:"api_key",key}` or `{type:"oauth",...}` (`auth-storage.ts:24-35`). File-locked via `proper-lockfile` for concurrent instances.
- Credential resolution order surfaces as `AuthStatus.source`: `stored` | `runtime` (`--api-key`) | `environment` (env vars) | `fallback` | `models_json_key`/`models_json_command` (`auth-storage.ts:37-41`).
- Subscription OAuth logins: Claude Pro/Max, ChatGPT Plus/Pro (Codex), GitHub Copilot (`docs/quickstart.md:54`, `login-dialog.ts`, `oauth-selector.ts`). API-key providers via env vars (huge list in `args.ts:335-373`).

### Project trust model (`core/project-trust.ts`, `core/trust-manager.ts`, `docs/usage.md:113-125`)
- On interactive startup, if cwd has trust-requiring project resources (`.pi/settings.json`, `.pi` resources, project `.agents/skills`) and no saved decision → prompt: trust folder? (writes `trust.json`).
- Before trust resolved, only context files + user/global/CLI `-e` extensions load (they can handle the `project_trust` event). Project-local resources load only after trust.
- Non-interactive modes never prompt; they use `defaultProjectTrust` (`ask`/`never` ignore project resources, `always` trusts). `--approve`/`-a` and `--no-approve`/`-na` override per run.
- `/trust` saves a decision (incl. parent folder) but does not reload the live session.

---

## 6. Interactive UX flow (what the user sees)

### Layout (`docs/usage.md:9-16`)
Four areas: **startup header** (shortcuts, loaded context files, prompt templates, skills, extensions), **messages** (user/assistant/tool calls/tool results/notifications/errors/extension UI), **editor** (border color = thinking level; bash-mode color when `!`), **footer**.

### Startup header / first-time setup
- First-time setup (`first-time-setup.ts`): two-step dialog — theme (dark/light, with live preview + detected system appearance) then analytics opt-in. Only under `PI_EXPERIMENTAL=1` on the official distribution.
- `quietStartup` hides the header; `--verbose` forces verbose.

### Footer (`modes/interactive/components/footer.ts`)
- Line 1: cwd (home→`~`), `(git-branch)`, `• session name`.
- Line 2 (left): token stats `↑input ↓output Rcache-read Wcache-write CH<hit%>`, cost `$x.xxx` (`(sub)` if OAuth subscription), context usage `pct%/contextWindow (auto)` colorized (>90% error, >70% warning).
- Line 2 (right): model id, `• <thinking>` if reasoning model, `(provider)` prefix when multiple providers.
- Optional line 3: extension statuses.
- Git branch is watched live via `core/footer-data-provider.ts` (fs.watch on `.git/HEAD` + reftable).

### Chat turn lifecycle (`interactive-mode.ts`)
- Submitting text: if compacting → queue (extension commands run now); if streaming → `session.prompt(text, {streamingBehavior:"steer"})` (steering message); else normal submit.
- **Message queue** (`docs/usage.md:59-69`): `Enter` queues a *steering* message (delivered after current assistant turn's tool calls finish); `Alt+Enter` queues a *follow-up* (after all work done); `Escape` aborts and restores queued to editor; `Alt+Up` retrieves queued messages. Delivery configurable via `steeringMode`/`followUpMode`.
- Streaming: assistant text streams into a message bubble; tool calls render as live tool cards (`tool-execution.ts`) with pending → success/error background tint (`toolPendingBg`/`toolSuccessBg`/`toolErrorBg`); `Ctrl+O` expands/collapses tool output. Thinking blocks render with `thinkingText` color; `Ctrl+T` toggles visibility.
- Tool cards support a `renderShell` ("default" box vs "self" framing), custom `renderCall`/`renderResult`, and inline image output (Kitty/iTerm/sixel via terminal capabilities) (`tool-execution.ts:178-353`).
- **Diff rendering** (`components/diff.ts`): edit/write diffs show `-`/`+` lines with line numbers, intra-line word-level diff highlighting (inverse video on changed tokens), context lines dim. Colors `toolDiffAdded/Removed/Context`.
- Bash execution UI: `components/bash-execution.ts` (live streaming command output, cancellable with Esc).
- Retry: agent-level retry shows a countdown timer + loader; Esc cancels (`interactive-mode.ts:2754-2759`).
- Errors / notifications render as distinct message components (`custom-message.ts`, error boundaries).

### Selectors (overlay the editor) — `modes/interactive/components/`
- `model-selector.ts` (`/model`, Ctrl+L) — fuzzy model picker grouped by provider.
- `scoped-models-selector.ts` (`/scoped-models`) — multi-select with provider toggle, reorder (Alt+↑/↓), save (Ctrl+S), enable/clear all.
- `session-selector.ts` + `session-selector-search.ts` (`/resume`, `pi -r`) — search, toggle path (Ctrl+P), sort (Ctrl+S), named filter (Ctrl+N), rename (Ctrl+R), delete (Ctrl+D, uses `trash` CLI when available).
- `tree-selector.ts` (`/tree`) — full session-tree navigator with fold/unfold, labels (Shift+L), label timestamps (Shift+T), filter modes (default/no-tools/user-only/labeled-only/all) cycled with Ctrl+O.
- `user-message-selector.ts` (`/fork`) — pick an earlier user message.
- `theme-selector.ts` (`/settings` → theme).
- `thinking-selector.ts` — levels with token-budget descriptions (`thinking-selector.ts:11-18`).
- `settings-selector.ts` (`/settings`) — thinking level, theme, message delivery, transport, double-escape, etc.
- `login-dialog.ts` + `oauth-selector.ts` (`/login`) — OAuth device-code flow / API key entry; `openBrowser`.
- `trust-selector.ts` (`/trust`, startup trust prompt).
- `config-selector.ts` / `extension-selector.ts` / `extension-input.ts` / `extension-editor.ts` — package/extension config TUI.
- `first-time-setup.ts`, `keybinding-hints.ts` (renders `keyHint(id,label)` from current bindings), `countdown-timer.ts`, `bordered-loader.ts`, `dynamic-border.ts`, `show-images-selector.ts`, `compaction-summary-message.ts`, `branch-summary-message.ts`, `skill-invocation-message.ts`, `assistant-message.ts`, `user-message.ts`.

### Session save/resume (`docs/sessions.md`, `core/session-manager.ts`)
- Auto-saved JSONL trees under `~/.pi/agent/sessions/`, organized by cwd. Each entry has `id`/`parentId`; current position = active leaf.
- Resolution (`main.ts:243-339 createSessionManager`): `--no-session`→in-memory; `--fork`→`forkFrom`; `--session <id|path>` (local match → open; cross-project → prompt to fork); `--resume`→picker; `--continue`→most recent; `--session-id`→open-or-create; else `create`.
- `/tree` vs `/fork` vs `/clone`: tree edits same file (optional branch summary), fork/clone make new files (`docs/sessions.md:118-127`).

---

## 7. Built-in tools & system prompt

- Default tool set: **`read`, `bash`, `edit`, `write`** (on by default). Additional read-only **`grep`, `find`, `ls`** (off by default, enable via `--tools`) (`args.ts:381-389`, `docs/quickstart.md:77-84`). Tool impls in `src/core/tools/`.
- `--no-tools` (none), `--no-builtin-tools` (keep ext/custom), `--tools` allowlist, `--exclude-tools` denylist.
- System prompt: `core/system-prompt.ts:130` — "expert coding assistant operating inside pi". Lists available tools (from one-line snippets), guidelines ("Be concise", "Show file paths clearly"), appends project context files (`AGENTS.md`/`CLAUDE.md` as `<project_instructions>`), then skills, then current date + cwd. `--system-prompt` replaces it; `--append-system-prompt` and `.pi/SYSTEM.md`/`APPEND_SYSTEM.md` augment.
- **Design philosophy** (`docs/usage.md:299-305`): PI intentionally has NO built-in MCP, sub-agents, permission popups, plan mode, todos, or background bash — those are extensions. (Note: Kivio's existing chat app HAS MCP/sub-agents/skills; Kivio Code can choose.)

---

## 8. Themes

- JSON files; 51 required color tokens + optional `export` section (`docs/themes.md`). Built-in `dark`/`light`; user `~/.pi/agent/themes/*.json`; project `.pi/themes/*.json`; package themes; `--theme`/`--no-themes`.
- Color values: hex `#rrggbb`, 256-color index, `vars` reference, or `""` (terminal default). 24-bit truecolor with 256-color fallback.
- Hot reload of the active custom theme file. First run auto-detects terminal background for dark/light.
- Token groups: core UI (11), backgrounds/content (11), markdown (10), tool diffs (3), syntax (9), thinking-level borders (6), bash mode (1). Editor border color encodes thinking level; bash mode has its own border color.

---

## 9. Kivio Code UX scope (mapping to Rust)

Kivio Code is a **new Rust terminal coding agent**, reusing Kivio's provider/model/key infrastructure but as a standalone TUI. Owner constraints: **NO `run_python`**, focus on solid basic tools, but UX coverage COMPREHENSIVE to PI's standard.

### What Kivio ALREADY provides (reuse, don't rebuild)
From `src-tauri/src/settings.rs` and the existing chat runtime:
- **Multi-provider model layer** with `ModelProvider { id, api_keys: Vec<String>, ... }` (key pool + failover), OpenAI-compatible AND Anthropic Messages adapters. → PI's provider/model/auth concern is largely solved; Kivio Code just needs CLI/TUI selection on top.
- **API key storage** in `settings.json` `providers[].apiKeys` (PI uses separate `auth.json`; Kivio already has a working store). Multi-key cooldown/failover (`state.rs`).
- **Theme** (`theme`, `theme_color`) and **model selection** structs (`DefaultModelSelection`, `DefaultModelsConfig`).
- **Agent tool loop + native tools** (`src-tauri/src/chat/agent/`, `native_tools/`): `read_file`/`write_file`/`edit_file`/`glob_files`/`search_files`/`list_dir`/`run_command` etc., with write blocklist + read size caps. → These map directly to PI's `read`/`write`/`edit`/`find`/`grep`/`ls`/`bash`.
- **Skills, MCP, sub-agents, compaction** (chat subsystem) — PI deliberately omits MCP/sub-agents; Kivio could expose them but MVP should not require them.
- **Streaming events** contract (`chat-stream`/`chat-tool`) — but Kivio Code is a terminal app, so it needs a NEW terminal renderer, not the Tauri webview.

### What is NEW for Kivio Code (must build in Rust)
- A **TUI rendering layer** (PI uses its own `pi-tui`; Rust equivalent: `ratatui`/`crossterm`). All of §6's components (footer, tool cards, diff renderer, selectors, editor with `@`/`!` modes) are new.
- A **CLI arg parser** (clap) matching §2's surface.
- **Print/JSON modes** + stdin piping + exit codes (§2).
- **Session persistence as JSONL trees** with tree navigation (`/tree`/`/fork`/`/clone`) — Kivio's chat `storage.rs` persists conversations but not necessarily the cwd-organized JSONL-tree format PI uses.
- **Keybindings system** (configurable JSON) and **theme JSON** (51 tokens) loaders.
- **Project trust model** + context-file loading (`AGENTS.md`/`CLAUDE.md`).
- **Slash-command engine** + completion.

---

## 10. Kivio Code CLI/UX feature checklist (prioritized)

### MVP (match PI's core daily-driver loop)
**CLI / invocation**
- [ ] `kivio [messages...]` interactive; bareword tokens → initial message.
- [ ] `-p`/`--print` one-shot mode → prints final assistant text, exit 1 on error.
- [ ] stdin piping merged into prompt (`cat x | kivio -p "..."`).
- [ ] `--model <provider/id[:thinking]>`, `--provider`, `--thinking <level>` (6 levels).
- [ ] `--api-key`, env-var key fallback (reuse Kivio providers).
- [ ] `--system-prompt` / `--append-system-prompt`.
- [ ] `-c`/`--continue`, `-r`/`--resume`, `--no-session`, `--name`, `--session <id|path>`.
- [ ] `--tools`/`-t`, `--exclude-tools`/`-xt`, `--no-tools`, `--no-builtin-tools`.
- [ ] `--no-context-files`, `-h`/`--help`, `-v`/`--version`, `--list-models [search]`.
- [ ] Exit codes (0 / 1) per §2.

**Tools (NO run_python)**
- [ ] `read`, `write`, `edit`, `bash` (default on); `grep`, `find`, `ls` (off by default). Map to existing native_tools.
- [ ] Write blocklist + read size caps (already in Kivio).

**Interactive UX**
- [ ] Editor with multiline (Shift+Enter), submit (Enter), history.
- [ ] `@`-file fuzzy reference + Tab path completion.
- [ ] `!cmd` / `!!cmd` bash mode with border color.
- [ ] Streaming assistant text + tool cards (pending/success/error tint) + Ctrl+O expand.
- [ ] Diff rendering for edit/write (line numbers, +/-, intra-line highlight).
- [ ] Footer: cwd + git branch + session name; token/cost/context% + model + thinking.
- [ ] Thinking indicator (editor border color by level).
- [ ] Message queue: Enter=steer, Alt+Enter=follow-up, Esc=abort/restore, Alt+Up=dequeue.
- [ ] Esc cancels current turn/bash.

**Slash commands (MVP subset)**
- [ ] `/model`, `/new`, `/resume`, `/session`, `/name`, `/compact`, `/copy`, `/help` (+`/hotkeys`), `/quit`, `/reload`.

**Config**
- [ ] Global + project settings files with project-override merge.
- [ ] `theme` (dark/light builtins), `defaultProvider`/`defaultModel`/`defaultThinkingLevel`, `enabledModels`.
- [ ] Context files: `~/.kivio/agent/AGENTS.md`, walk-up `AGENTS.md`/`CLAUDE.md`.

**Keybindings**
- [ ] Default keymap from §4 (editor + app-level); `Ctrl+L` model selector, `Shift+Tab` thinking cycle, `Ctrl+P` model cycle, `Ctrl+T` thinking toggle, `Ctrl+G` external editor.

### Later (full PI parity / nice-to-have)
- [ ] `/tree` session-tree navigator + `/fork` + `/clone`; JSONL tree session format with branch summaries.
- [ ] `/scoped-models` selector + `--models` cycling with glob/fuzzy.
- [ ] `/login` / `/logout` OAuth (Claude/Codex/Copilot) + `auth.json`-style storage (Kivio currently uses settings.json keys — decide).
- [ ] `/export` HTML + `/import` + `/share` gist.
- [ ] `/settings` TUI (message delivery, transport, double-escape, tree filter, image settings).
- [ ] `/trust` + full project-trust prompt flow + `--approve`/`--no-approve`.
- [ ] `--mode json` / `--mode rpc` (programmatic integration).
- [ ] User-customizable `keybindings.json` (+ legacy migration) and `/reload`.
- [ ] Custom theme JSON loader (51 tokens) + hot reload + `--theme`/`--no-themes`.
- [ ] Skills (`--skill`, `/skill:name`), prompt templates (`/templatename`), extensions/packages — Kivio already has skills/MCP/sub-agents in chat; optional to surface in Code.
- [ ] Inline terminal image rendering (Kitty/iTerm/sixel).
- [ ] First-time setup wizard (theme + analytics).
- [ ] Compaction settings + auto-compaction footer indicator.
- [ ] `--offline`, telemetry/update-check toggles, `--verbose`/`quietStartup`.
- [ ] `kivio install/update/list/config` package subcommands (likely out of scope unless Kivio wants an extension ecosystem).

---

## Caveats / Not Found

- Slash-command dispatch is split: a handful inline in `interactive-mode.ts` (~6000 lines, not read in full), the rest routed via the agent-session command-context binding (`print-mode.ts:74-97`). The exact handler for each non-inline command (`/export`, `/share`, `/login`, etc.) lives in `core/agent-session.ts` (105KB) and was not read line-by-line — descriptions taken from `slash-commands.ts` + docs, which are authoritative for behavior.
- `/clear` does not exist as a built-in; the task prompt listed it as an example — closest equivalents are `Ctrl+C` (clear editor) and `/new` (new session).
- Provider list and OAuth details: env-var list is exhaustive in `args.ts:335-373`; OAuth provider IDs come from `@earendil-works/pi-ai/oauth` (external package, not inspected).
- Kivio mapping is grounded in `src-tauri/src/settings.rs` symbol survey (struct names/fields) and the project CLAUDE.md; the existing chat runtime is a Tauri webview app, so Kivio Code's terminal renderer is entirely new work.
