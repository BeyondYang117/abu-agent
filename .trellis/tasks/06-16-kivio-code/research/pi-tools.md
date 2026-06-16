# Research: PI agent TOOL SYSTEM (baseline for Kivio Code)

- **Query**: Study PI agent's `packages/coding-agent/src/core/tools/` exhaustively; document each tool's schema, behavior, guards, output handling; then map to Kivio's `src-tauri/src/native_tools/` and flag gaps.
- **Scope**: mixed (PI internal source + Kivio internal source)
- **Date**: 2026-06-16

PI source root: `/Users/zmair/ZM database/Kivio agent/pi/packages/coding-agent/src/core/tools/`
PI shared utils: `.../src/utils/{paths.ts,shell.ts,tools-manager.ts}`
Kivio source root: `/Users/zmair/ZM database/Kivio agent/kivio/src-tauri/src/native_tools/{files.rs,shell.rs,fetch.rs}` + `src-tauri/src/mcp/{native_registry.rs,types.rs,registry.rs}`

---

## 0. Architecture & shared infrastructure (PI)

### Tool factory pattern
Every tool is `createXToolDefinition(cwd, options?)` returning a `ToolDefinition` (name, label, description, `promptSnippet`, optional `promptGuidelines[]`, `parameters` (TypeBox/JSON Schema), `execute`, TUI `renderCall`/`renderResult`, optional `prepareArguments`, `executionMode`, `renderShell`). `createXTool(cwd, options?)` wraps it into an `AgentTool` via `wrapToolDefinition` (`tool-definition-wrapper.ts:5`). `index.ts` exposes bundles: `createCodingTools` = `[read, bash, edit, write]`; `createReadOnlyTools` = `[read, grep, find, ls]`; `createAllTools` = all 7. Tool names (`index.ts:83`): **`read, bash, edit, write, grep, find, ls`** (exactly 7).

Each tool accepts a pluggable `operations` object (e.g. `ReadOperations`, `BashOperations`) so execution can be delegated to remote/SSH backends. A Rust port can ignore this layer (only the local impl matters) but it explains why path resolution and FS access are factored out.

### Path resolution (`path-utils.ts`, `utils/paths.ts`)
- `resolveToCwd(filePath, cwd)` → `resolvePath` with `{normalizeUnicodeSpaces:true, stripAtPrefix:true}`. `resolvePath` (`paths.ts:81`): `normalizePath` then, if absolute → `node:path.resolve(normalized)`, else → `resolve(baseDir, normalized)`. So **relative paths resolve against `cwd`, absolute paths are taken as-is**.
- `normalizePath` (`paths.ts:57`): trims unicode spaces (`  -   　` → space), strips leading `@`, expands `~`/`~/`/(win)`~\` to homedir, and converts `file://` URLs via `fileURLToPath`.
- `resolveReadPathAsync` (`path-utils.ts:86`): resolve then, **if the file is missing, try macOS screenshot filename variants** in order: narrow-no-break-space before AM/PM, NFD-normalized, curly-quote (`'`→`’`), NFD+curly. Used by `read` only.
- **No path-traversal/sandbox guard anywhere.** `..` and absolute paths outside cwd are allowed. There is **no write blocklist** (no `.ssh`/keychain protection). Safety is delegated to an external approval/permission layer, not the tool code.
- `canonicalizePath` follows symlinks via `realpathSync` (used only by mutation-queue keying, not for confinement). **Symlinks are followed transparently; there is no symlink rejection.**

### Truncation (`truncate.ts`)
Two independent limits, **whichever hits first wins**:
- `DEFAULT_MAX_LINES = 2000`
- `DEFAULT_MAX_BYTES = 50 * 1024` (50 KB)
- `GREP_MAX_LINE_LENGTH = 500` chars/line
- `truncateHead` (file reads, find, ls, grep — keep first N): never returns a partial line; if first line alone > maxBytes returns empty content with `firstLineExceedsLimit=true`.
- `truncateTail` (bash output — keep last N, to preserve errors/final results): may return a partial first line if the last line exceeds maxBytes (`lastLinePartial=true`).
- `truncateLine(line, 500)` → `"<first 500>... [truncated]"`. Byte counting uses `Buffer.byteLength(utf-8)`; UTF-8 boundaries respected.
- `formatSize` → `B`/`KB`/`MB`.
- `TruncationResult` carries `{content, truncated, truncatedBy:"lines"|"bytes"|null, totalLines, totalBytes, outputLines, outputBytes, lastLinePartial, firstLineExceedsLimit, maxLines, maxBytes}`.

### OutputAccumulator (`output-accumulator.ts`) — the streaming bash backbone
Incrementally accumulates streaming process output with bounded memory:
- Streaming UTF-8 `TextDecoder`; tracks `totalLines`, `totalDecodedBytes`, `currentLineBytes`.
- Keeps only a rolling decoded **tail** (`maxRollingBytes = max(maxBytes*2,1)`) for display; trims tail at UTF-8 boundaries.
- When output exceeds `maxBytes`/`maxLines`, lazily opens a **temp file** (`tmpdir()/<prefix>-<hex>.log`, prefix `pi-bash`) and streams the *full raw output* there, so the model can be pointed at the complete log. `snapshot({persistIfTruncated})` returns `{content (tail-truncated), truncation, fullOutputPath}`.

### File mutation queue (`file-mutation-queue.ts`)
`withFileMutationQueue(filePath, fn)` serializes mutations **per real (canonical) path** while letting different files run in parallel. Key = `realpath(resolve(filePath))` (falls back to resolved path if ENOENT/ENOTDIR). A global `registrationQueue` serializes the registration step so the map mutation is race-free; the per-file promise chain releases the next waiter in `finally`. Used by `write` and `edit`.

### External tool bootstrapping (`utils/tools-manager.ts`)
`grep` and `find` shell out to **ripgrep (`rg`)** and **fd**. `ensureTool("rg"|"fd")`: checks Kivio-bin dir, then system PATH (`fd`/`fdfind`), else **downloads the binary from GitHub releases** for the platform/arch (or skips if `PI_OFFLINE`, or instructs `pkg install` on Termux/Android). `getShellEnv` prepends the bin dir to `PATH`.

---

## 1. `read` (`read.ts`)

- **name** `read`, label `read`. `promptGuidelines`: "Use read to examine files instead of cat or sed."
- **Description**: "Read the contents of a file. Supports text files and images (jpg, png, gif, webp). Images are sent as attachments. For text files, output is truncated to 2000 lines or 50KB (whichever is hit first). Use offset/limit for large files. When you need the full file, continue with offset until complete."

**Input schema** (`read.ts:20`):
| param | type | req | default | description |
|---|---|---|---|---|
| `path` | string | ✅ | — | Path to file (relative or absolute) |
| `offset` | number | — | 1 | 1-indexed start line |
| `limit` | number | — | (none) | Max lines to read |

**Behavior:**
- Resolves via `resolveReadPathAsync` (macOS filename fallbacks). `access(R_OK)`; aborts on `signal`.
- **Image detection** via `detectSupportedImageMimeTypeFromFile` (content sniff, not extension). If image: optionally `resizeImage` to ≤2000×2000 (`autoResizeImages` default true); returns a `text` note + `image` content block (base64). If model lacks vision, appends "[Current model does not support images...]". If resize fails, returns text-only note.
- **Text:** `buffer.toString("utf-8")`, split on `\n`. `offset` → 0-indexed `startLine = max(0, offset-1)`; out-of-bounds offset throws `"Offset N is beyond end of file (M lines total)"`. With `limit`, slices `[start, start+limit)`. Then `truncateHead` (2000 lines / 50 KB).
- **Output is NOT line-numbered** (raw text). Continuation notices appended: `[Showing lines A-B of TOTAL. Use offset=N to continue.]`. If a single line exceeds 50 KB: `[Line N is XKB, exceeds 50.0KB limit. Use bash: sed -n 'Np' <path> | head -c 51200]`.
- Edge: user `limit` that stops early but file has more → `[N more lines in file. Use offset=N to continue.]`.
- **No max-file-size pre-check** — it reads the whole file into memory, then truncates. **No binary-text guard** (binary files become lossy UTF-8 unless detected as image).

## 2. `bash` (`bash.ts`)

- **name** `bash`. `promptSnippet`: "Execute bash commands (ls, grep, find, etc.)".
- **Description**: "Execute a bash command in the current working directory. Returns stdout and stderr. Output is truncated to last 2000 lines or 50KB (whichever is hit first). If truncated, full output is saved to a temp file. Optionally provide a timeout in seconds."

**Input schema** (`bash.ts:24`):
| param | type | req | default | description |
|---|---|---|---|---|
| `command` | string | ✅ | — | Bash command to execute |
| `timeout` | number | — | **none (no default timeout)** | Timeout in seconds |

**Execution (`createLocalBashOperations`, `bash.ts:66`):**
- **Shell selection** (`getShellConfig`, `shell.ts:57`): user `shellPath` → (Windows) Git Bash known paths → `bash.exe` on PATH (error if none) → (Unix) `/bin/bash` → `bash` on PATH → `sh`. Always invoked as `shell -c <command>`.
- `spawn(shell, [...args, command], { cwd, detached: !win32, env, stdio: [ignore, pipe, pipe], windowsHide:true })`. cwd existence checked first (throws "Working directory does not exist").
- **env**: `getShellEnv()` = `process.env` with the Kivio bin dir prepended to PATH (so downloaded `rg`/`fd` are reachable). Mutable via `spawnHook` and `commandPrefix` (prepended as `${prefix}\n${command}`).
- **Streaming**: stdout+stderr both piped to one `onData` handler → `OutputAccumulator`. UI updates throttled to 100ms (`BASH_UPDATE_THROTTLE_MS`), preview = last 5 lines (`BASH_PREVIEW_LINES`).
- **Timeout**: only if `timeout>0`; on fire sets `timedOut` and `killProcessTree(pid)`. On exit throws `timeout:<secs>` → surfaced as "Command timed out after N seconds".
- **Abort** (`signal`): `killProcessTree(child.pid)` — on Unix `process.kill(-pid, SIGKILL)` (process **group**, hence `detached`/`setsid`), on Windows `taskkill /F /T /PID`. Detached child PIDs are tracked (`trackDetachedChildPid`) for parent-shutdown cleanup.
- **Output truncation = `truncateTail`** (keep the END — errors/results). On truncation, footer like `[Showing lines A-B of TOTAL. Full output: /tmp/pi-bash-xxx.log]` (or byte/partial-line variants).
- **Non-zero exit** → throws `Error("...output...\n\nCommand exited with code N")` (exit code becomes a tool error).
- **No background mode**, **no denylist/blocklist** of commands — every command runs.

## 3. `write` (`write.ts`)

- **name** `write`. `promptSnippet`: "Create or overwrite files". `promptGuidelines`: "Use write only for new files or complete rewrites."
- **Description**: "Write content to a file. Creates the file if it doesn't exist, overwrites if it does. Automatically creates parent directories."

**Input schema** (`write.ts:14`):
| param | type | req | description |
|---|---|---|---|
| `path` | string | ✅ | Path to write (relative or absolute) |
| `content` | string | ✅ | Content to write |

**Behavior:** `resolveToCwd`; wraps in `withFileMutationQueue`. `mkdir(dirname, {recursive})` then `writeFile(path, content, "utf-8")`. Abort is observed via `throwIfAborted()` *after* each await (deliberately NOT a reject-on-abort listener, to keep the queue locked until the in-flight FS op settles — see comment `write.ts:204`). Returns `Successfully wrote <N> bytes to <path>`. **No diff, no overwrite confirmation, no blocklist, not atomic** (direct `writeFile`).

## 4. `edit` (`edit.ts` + `edit-diff.ts`)

- **name** `edit`, `renderShell:"self"`, `prepareArguments: prepareEditArguments`.
- **Description**: "Edit a single file using exact text replacement. Every edits[].oldText must match a unique, non-overlapping region of the original file. If two changes affect the same block or nearby lines, merge them into one edit instead of emitting overlapping edits. Do not include large unchanged regions just to connect distant changes."
- `promptGuidelines[]`: use edit for precise changes; one call with multiple `edits[]` for multiple sites; each oldText matched against the *original* (not incrementally); keep oldText minimal but unique.

**Input schema** (`edit.ts:44`, `additionalProperties:false`):
| param | type | req | description |
|---|---|---|---|
| `path` | string | ✅ | File to edit (relative or absolute) |
| `edits` | array of `{oldText:string, newText:string}` (both required, `additionalProperties:false`) | ✅ | One or more targeted replacements |

`prepareArguments` (`edit.ts:94`) is a **compat shim**: if `edits` arrived as a JSON *string* (Opus 4.6 / GLM-5.1 quirk) it `JSON.parse`s it; if legacy top-level `{oldText,newText}` present, folds them into `edits[]`.

**Edit algorithm (`applyEditsToNormalizedContent`, `edit-diff.ts:193`):**
1. Read file; `stripBom`; detect line ending (`detectLineEnding` — first of `\r\n`/`\n`); `normalizeToLF` the content; normalize each edit's old/newText to LF.
2. Reject empty `oldText` (`getEmptyOldTextError`).
3. **Match: exact first, then fuzzy** (`fuzzyFindText`). Fuzzy = `normalizeForFuzzyMatch` (NFKC, strip per-line trailing whitespace, smart quotes→ASCII, all unicode dashes→`-`, special spaces→space). If *any* edit needs fuzzy match, the **whole operation runs in fuzzy-normalized space** (`baseContent` becomes the normalized content).
4. **Uniqueness**: `countOccurrences` (in fuzzy space) — `==0` → `getNotFoundError`; `>1` → `getDuplicateError` ("Found N occurrences... must be unique. Provide more context."). Errors are indexed per-edit when multiple edits.
5. **Overlap check**: sort matched edits by index; if `prev.matchIndex+prev.matchLength > cur.matchIndex` → throws "edits[i] and edits[j] overlap... Merge them or target disjoint regions."
6. Apply replacements **in reverse index order** (offsets stay stable). If result == base → `getNoChangeError` ("No changes made... replacement produced identical content.").
7. `restoreLineEndings` to the detected ending, re-prepend BOM, `writeFile`.

**Diff outputs** (in `EditToolDetails`): `diff` = `generateDiffString` (display diff: `+`/`-`/` ` prefixed, right-padded line numbers, 4 context lines, collapses long unchanged runs with `...`, reports `firstChangedLine`); `patch` = `generateUnifiedPatch` via the `diff` npm lib (`createTwoFilesPatch`, 4 context lines, FILE_HEADERS_ONLY). Returns `Successfully replaced <N> block(s) in <path>.` `computeEditsDiff` is a non-mutating preview used by the TUI before execution.

## 5. `grep` (`grep.ts`) — ripgrep wrapper

- **name** `grep`. **Description**: "Search file contents for a pattern. Returns matching lines with file paths and line numbers. **Respects .gitignore.** Output is truncated to 100 matches or 50KB (whichever is hit first). Long lines are truncated to 500 chars."

**Input schema** (`grep.ts:24`):
| param | type | req | default | description |
|---|---|---|---|---|
| `pattern` | string | ✅ | — | Regex or literal string |
| `path` | string | — | `.` | Dir or file to search |
| `glob` | string | — | — | Filter files, e.g. `*.ts`, `**/*.spec.ts` |
| `ignoreCase` | boolean | — | false | Case-insensitive |
| `literal` | boolean | — | false | Treat pattern as literal (`--fixed-strings`) |
| `context` | number | — | 0 | Lines before/after each match |
| `limit` | number | — | 100 | Max matches |

**Behavior:** `ensureTool("rg")` (download if missing → error if unavailable). Spawns `rg --json --line-number --color=never --hidden [--ignore-case] [--fixed-strings] [--glob G] -- <pattern> <searchPath>`. **`--hidden` searches dotfiles; `.gitignore` is respected by rg's default** (no `--no-ignore`). Streams JSON, collects up to `limit` matches then kills rg (`matchLimitReached`). Output rows: `relpath:line: text` (or `relpath-line- text` for context lines). Long lines → `truncateLine` (500). Final byte-truncated via `truncateHead(maxLines=MAX_SAFE_INTEGER)` (match count is the row cap; only byte limit applies). Notices: `100 matches limit reached. Use limit=200...`, `50.0KB limit reached`, `Some lines truncated to 500 chars. Use read tool...`. `code !==0 && !==1` (rg "no match" is 1) → error. No matches → `"No matches found"`.

## 6. `find` (`find.ts`) — fd wrapper

- **name** `find`. **Description**: "Search for files by glob pattern. Returns matching file paths relative to the search directory. **Respects .gitignore.** Output is truncated to 1000 results or 50KB (whichever is hit first)."

**Input schema** (`find.ts:20`):
| param | type | req | default | description |
|---|---|---|---|---|
| `pattern` | string | ✅ | — | Glob: `*.ts`, `**/*.json`, `src/**/*.spec.ts` |
| `path` | string | — | `.` | Directory to search |
| `limit` | number | — | 1000 | Max results |

**Behavior:** `ensureTool("fd")`. Spawns `fd --glob --color=never --hidden --no-require-git --max-results <limit> [--full-path] -- <pattern> <searchPath>`. `--no-require-git` applies hierarchical `.gitignore` even outside a git repo. **fd `--glob` matches basename unless `--full-path`**; so if pattern contains `/`, adds `--full-path` and (unless it starts with `/`/`**/`) prefixes `**/`. Output = paths relativized to searchPath (POSIX-slashed), preserving trailing-slash dir markers. Truncated via `truncateHead(maxLines=MAX_SAFE_INTEGER)`. Notices: `1000 results limit reached. Use limit=2000...`, `50.0KB limit reached`. No results → `"No files found matching pattern"`.

## 7. `ls` (`ls.ts`)

- **name** `ls`. **Description**: "List directory contents. Returns entries sorted alphabetically, with '/' suffix for directories. **Includes dotfiles.** Output is truncated to 500 entries or 50KB (whichever is hit first)."

**Input schema** (`ls.ts:14`):
| param | type | req | default | description |
|---|---|---|---|---|
| `path` | string | — | `.` | Directory to list |
| `limit` | number | — | 500 | Max entries |

**Behavior:** `resolveToCwd`; check exists + isDirectory (else "Path not found" / "Not a directory"). `readdir`, **sort case-insensitive alphabetical**, `stat` each (dirs get `/` suffix; unstat-able entries skipped). **No recursion, no .gitignore filtering, dotfiles always included.** Cap at `limit` entries → `entryLimitReached`. Byte-truncated via `truncateHead(MAX_SAFE_INTEGER)`. Empty → `"(empty directory)"`. Notices: `500 entries limit reached. Use limit=1000`, `50.0KB limit reached`.

---

## 8. Comparison: PI tools → Kivio native_tools

Kivio's tool names already mirror PI's 7 (`mcp/native_registry.rs:469`: `read, ls, grep, find, write, edit, bash`), but the **internal contracts differ substantially**. Kivio dispatches via the `NATIVE_TOOLS` registry; schemas live in `mcp/types.rs`; impls in `native_tools/{files.rs,shell.rs}`.

| PI tool | Kivio tool | impl | Parity? |
|---|---|---|---|
| `read` (`read.ts`) | `read` (`native_read_file_tool`, `files.rs:read_file`) | files.rs:105 | Partial — see below |
| `bash` (`bash.ts`) | `bash` (`native_run_command_tool`, `shell.rs:run_command`) | shell.rs:50 | Partial — different schema/semantics |
| `write` (`write.ts`) | `write` (`native_write_file_tool`, `files.rs:write_file`) | files.rs:252 | Kivio is a superset (atomic+diff) |
| `edit` (`edit.ts`) | `edit` (`native_edit_file_tool`, `files.rs:edit_file`) | files.rs:298 | Different param names + no fuzzy match |
| `grep` (`grep.ts`, rg) | `grep` (`native_search_files_tool`, `files.rs:search_files`) | files.rs:949 | Different engine (`ignore` crate, no rg) + richer modes |
| `find` (`find.ts`, fd) | `find` (`native_glob_files_tool`, `files.rs:glob_files`) | files.rs:899 | Different engine + path-pattern restriction |
| `ls` (`ls.ts`) | `ls` (`native_list_dir_tool`, `files.rs:list_dir`) | files.rs:841 | Roughly equivalent; Kivio defaults differ |

### Per-tool differences

**read** — Kivio (`files.rs:105`, schema `types.rs:303`): params `path/offset/limit` (same shape). Kivio **prepends `cat -n` line numbers** in the model-facing text (`registry.rs:750`, `{:>6}\t{line}`) — PI's read is *unnumbered*. Kivio has a hard **`MAX_READ_FILE_BYTES = 2 MB` pre-check** (`mod.rs:23`) that *rejects* whole-file reads over 2 MB unless offset/limit given (then streams a line window); PI has no size cap (reads everything, then truncates to 50 KB/2000 lines). Kivio returns rich structured `ReadFileResult` (total_lines, next_offset, mtime, read_state). **Kivio `read` does NOT handle images** (text-only); PI returns image content blocks with resize. PI's read has macOS screenshot filename fallbacks; Kivio does not.

**bash** — Kivio (`shell.rs:50`, schema `types.rs:457`) name `bash` but params are `command, cwd, background, timeout_ms, allow_host_python_package_install` vs PI's `command, timeout` (seconds). Major Kivio additions: **command denylist** (`COMMAND_DENYLIST`: `sudo`, `rm -rf /`, fork-bomb, `mkfs.`, `dd if=/dev/zero`, `> /dev/sd`), **host pip-install blocking** (unless `allow_host_python_package_install` + `--user`/venv), **`cd foo &&` auto-strip + reject** (paths with spaces must use `cwd`), **background mode** with auto-detection of dev servers (`LONG_RUNNING_DEV_PATTERNS`), default timeout from settings clamped to `[CHAT_TOOL_MIN/MAX_TIMEOUT_MS]`. Kivio shells out to `sh -c` (Unix) / `cmd /C` (Windows, `raw_arg` to preserve quotes) — **not bash**; no Git-Bash discovery. Kivio output is **not streamed** (buffered `wait_with_output`), formatted as `exit_code:/stdout:/stderr:`, offloaded to a temp log over **16 KB** (`MAX_INLINE_COMMAND_OUTPUT_BYTES`, `shell.rs:147`) — PI streams and offloads over 50 KB / 2000 lines with tail-truncation. Both kill the process group on timeout/abort (Kivio uses `setsid` + `kill(-pid, SIGKILL)` on macOS only).

**write** — Kivio (`files.rs:252`) is a **superset**: same `path/content` schema, but writes **atomically** (`.kivio-tmp-*` + rename, `files.rs:528`), preserves **CRLF + BOM** of the existing file, computes a **unified diff with +/- stats** (`FileMutationResult`), and serializes via an in-process `acquire_file_mutation_locks` (HashSet+Condvar — Kivio's analog of PI's `file-mutation-queue`). PI's write is plain non-atomic `writeFile` with no diff. Neither has a write blocklist (Kivio CLAUDE.md mentions `WRITE_BLOCKLIST_SEGMENTS` but it is **not present** in current `files.rs`/`mod.rs` — see Caveats).

**edit** — Kivio (`files.rs:298`, schema `types.rs:423`): params `path` + `edits[]` of **`{old_string, new_string}`** (PI uses `{oldText, newText}`). Kivio normalizes line endings before matching (CRLF↔LF tolerant) and requires each `old_string` to occur **exactly once** (no `replace_all`), applied in order against the *progressively-edited* working text (`working.matches().count()` each step) — **PI matches every edit against the ORIGINAL content and rejects overlaps**, a subtle ordering-semantics difference. Kivio has **no fuzzy matching** (no smart-quote/dash/whitespace normalization beyond line endings) — PI's `normalizeForFuzzyMatch` is a notable feature absent from Kivio. Kivio skips identical old==new edits with a warning rather than erroring; PI errors on no-op. Both return diff stats.

**grep** — Kivio (`files.rs:949`) is a **richer schema**: `query`(+alias `pattern`), `path`, `regex`, `case_sensitive`, `include_hidden`, `glob` (with **brace expansion** `*.{py,ts}`), **`output_mode` (`content`/`files_with_matches`/`count`)**, `max_results` (default 100, max 1000). PI's grep has `context` (before/after lines) and `literal`+`ignoreCase` but **no output_mode and no brace expansion**. Engine: Kivio uses the Rust **`ignore` crate** (`walk_paths`, `files.rs:1203`) — same gitignore engine as rg but in-process (no `rg` binary download); skips a hardcoded floor (`.git, node_modules, target, dist, build, .next, .turbo, .vite`) plus full `.gitignore`/`.ignore`/global/exclude/parents. Per-file size cap `MAX_SEARCH_FILE_BYTES = 1 MB`; walk cap `MAX_SEARCH_FILES = 5000`. PI relies on `rg --json --hidden` (downloaded binary), with `context` support and 500-char line truncation that Kivio lacks (Kivio returns full lines).

**find** — Kivio (`files.rs:899`): `pattern`(req), `path`, `include_hidden`, `max_results` (default 200, max 500). **Rejects absolute patterns and `..` in patterns** (`validate_glob_pattern`, `files.rs:1148`) and uses an in-process custom `glob_match` (`*`, `?`, `**`) over the gitignore-aware `walk_paths`. PI uses `fd --glob --hidden --no-require-git` (downloaded binary), default limit **1000**, supports `--full-path` for `/`-containing patterns. Result caps differ: Kivio 500 max vs PI 1000.

**ls** — Kivio (`files.rs:841`): `path`(default `.`), `include_hidden` (**default false** — PI always includes dotfiles), `max_entries` (default 200, max 500 — PI default/max 500). Both sort and mark directories (Kivio returns JSON `entries[]` with type/size/mtime; PI returns plain text with `/` suffix). PI's 50 KB byte cap; Kivio caps by entry count + structured JSON.

### Registry / safety model (Kivio-specific, no PI analog)
`native_registry.rs` gives each tool flags: `parallel_safe`, `bypasses_approval`, `read_only`, **`requires_session_consent`** (the 7 file/shell tools sit behind a one-time per-conversation full-disk consent gate, `native_registry.rs:339`), and `sensitive` (write/edit/bash marked `sensitive:true` in `types.rs`). PI has none of this in the tool layer — its gating is entirely in an external approval/extension layer. Kivio path resolution (`mod.rs`) is **no-boundary** (absolute/`~`/`..` all allowed; access gated by session consent) — matching PI's no-sandbox stance, but enforced via consent rather than approval.

---

## Gaps vs Kivio native_tools (checklist for Kivio Code)

Kivio Code is a **fresh Rust terminal agent**, so "gap" = PI behaviors a faithful baseline needs that Kivio's *current* native_tools do NOT provide:

- [ ] **`read` image support** — PI returns resized image content blocks (jpg/png/gif/webp, ≤2000×2000) via content-sniff detection. Kivio `read` is text-only. (Terminal agent may skip, but PI baseline has it.)
- [ ] **`read` line-length / first-line-exceeds handling** — PI emits the `sed -n 'Np' | head -c` fallback hint when one line > 50 KB. Kivio uses a 2 MB whole-file cap instead.
- [ ] **`edit` fuzzy matching** — PI's `normalizeForFuzzyMatch` (NFKC + smart-quote/dash/whitespace normalization) lets edits succeed despite cosmetic unicode differences. Kivio only normalizes line endings → MISSING.
- [ ] **`edit` match-against-original + overlap detection** — PI matches all edits against the original content and explicitly rejects overlapping ranges; Kivio matches sequentially against progressively-edited text (different semantics). Decide which contract Kivio Code adopts.
- [ ] **`edit`/`write` param naming** — PI uses `oldText/newText` and TUI-style diff `patch` (unified). Kivio uses `old_string/new_string`. Pick one and document.
- [ ] **`bash` streaming + `OutputAccumulator` + tail-truncation** — PI streams output live and keeps the *end* (errors/results) with a temp-file spill at 50 KB / 2000 lines. Kivio buffers, keeps the head, spills at 16 KB. A coding agent benefits from PI's tail-keep + streaming.
- [ ] **`bash` timeout default + units** — PI: seconds, no default timeout. Kivio: ms, settings-driven default with clamp. Reconcile.
- [ ] **`grep` `context` lines + 500-char line truncation** — PI supports before/after context and caps line length; Kivio returns full lines, no context.
- [ ] **`grep`/`find` real ripgrep/fd** — PI auto-downloads `rg`/`fd` (fast, battle-tested gitignore). Kivio reimplements with the `ignore` crate + custom `glob_match`. For a terminal coding agent, the `rg`/`fd` path is the de-facto standard; decide whether to bundle/download or keep the in-process walker.
- [ ] **macOS screenshot filename fallbacks** (`read`) — PI tries NFD/curly-quote/AM-PM variants. Niche; likely skip.
- [ ] **PI has NO command denylist / pip guard / background mode / cd-prefix handling** — these are Kivio *additions*, not gaps. If Kivio Code wants the PI baseline only, they are optional; if it inherits Kivio's safety posture, port them from `shell.rs`.
- [ ] **Approval/consent model** — PI tools have no built-in gating; Kivio's `requires_session_consent`/`sensitive` flags are extra. Kivio Code (terminal) will need its own approval UX; PI's is external.

---

## Caveats / Not Found

- **`WRITE_BLOCKLIST_SEGMENTS` not found**: Kivio's `CLAUDE.md` claims writes are blocked under `.ssh`/`.gnupg`/Keychains via `WRITE_BLOCKLIST_SEGMENTS`, but I could not find this constant or any home-segment blocklist in the current `native_tools/files.rs` or `mod.rs`. The actual model is **no-boundary path resolution gated by per-conversation session consent** (`mod.rs:77-118` comments explicitly say "No sandbox" / "`..` is no longer rejected"). The CLAUDE.md description appears stale. Flag for the implementer.
- PI's `MAX_READ_FILE_BYTES` analog: there is **no max-file-size guard in PI's read** — it reads the entire file then truncates display. The 2 MB cap is a Kivio-only safety addition.
- PI tool TUI rendering (`renderCall`/`renderResult`, diff components, syntax highlighting) is irrelevant to a headless Rust port and was only skimmed.
- `fetch.rs` (Kivio `web_fetch`) was not deeply read — PI's tool set under `core/tools/` has **no fetch tool**; web fetch in PI lives elsewhere (not in the 7-tool coding set), so it's out of scope for this slice.
