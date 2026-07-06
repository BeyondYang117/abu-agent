# Process PATH Enrichment (startup, cross-platform)

Contracts for `src-tauri/src/path_env.rs` — the one-shot startup fixup that makes
user-installed CLIs (node/npm, claude/codex/pi, Homebrew, version-manager shims)
findable by every subprocess Kivio spawns (`run_command`, MCP stdio servers,
external-agent detection, skill scripts). Read before touching `path_env.rs` or its
call site in `lib.rs::run()`.

**Why it exists:** a GUI launch does not inherit the *current* user PATH.
- macOS: a `.app` from Finder/Dock gets only the minimal `/usr/bin:/bin:/usr/sbin:/sbin`.
- Windows: the process inherits `explorer.exe`'s environment, a **stale login-time snapshot**;
  worse, version managers like **fnm** don't put node on any static PATH at all — they inject a
  per-shell dir (`%LOCALAPPDATA%\fnm_multishells\<pid>_<ts>`) into `$env:PATH` from the
  **PowerShell profile** at shell startup. Nothing in the registry ever names that dir.

## Signatures

```rust
// Entry points — called once, synchronously, at the very top of lib.rs::run(),
// before any window creation, thread spawn, or CLI probing.
#[cfg(target_os = "macos")]   pub fn enrich_path_macos();
#[cfg(target_os = "windows")] pub fn enrich_path_windows();

// Shared timeout helper (mac + windows): spawn a caller-configured Command, wait on a
// helper thread, give up after `timeout`. Never blocks past the timeout, never panics.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn capture_stdout_with_timeout(cmd: std::process::Command, timeout: Duration) -> Option<String>;

// Windows profile probe (the fnm/nvm fix).
#[cfg(target_os = "windows")] fn profile_shell_path() -> Option<String>;
#[cfg(target_os = "windows")] fn profile_shell_exe() -> &'static str; // "pwsh" if on PATH else "powershell"
#[cfg(any(target_os = "windows", test))] fn parse_profile_path_output(&str) -> Option<String>;

// Pure merge (order-preserving, dedup). `profile` is the new source.
#[cfg(any(target_os = "windows", test))]
fn merge_paths_windows(current: &str, system: Option<&str>, user: Option<&str>,
                       profile: Option<&str>, defaults: &[String]) -> String;
```

## Contract: enrich_path_windows is TWO-PHASE — order is load-bearing

```
① registry(system+user, %VAR%-expanded) + common_dirs → merge → set_var   [always]
② profile_shell_path()  (runs PowerShell WITH profile, 3s timeout)         [always attempted]
③ Some(profile) → merge(current = ①result, profile) → set_var             [only if ② succeeded]
   None/timeout → skip ③, leave ①'s PATH untouched (byte-identical to pre-fix behavior)
```

- **Phase ① must run before ②.** `profile_shell_exe()` scans the *process* PATH for `pwsh.exe`;
  pwsh is often installed in a dir only present after the registry merge. Probing before ① can
  pick the wrong shell (whose profile differs) and miss the user's real config.
- Merge source order is fixed: `current → system → user → profile → defaults`. Existing
  resolution order wins; dedup folds ASCII case (first spelling kept). A system-installed node
  therefore beats a profile-injected one — same tradeoff as the macOS branch, intentional.

## Hard invariants

- **The probe command must NOT pass `-NoProfile`.** Loading the profile is the entire point
  (that's where `fnm env | Invoke-Expression` lives). Command is exactly:
  `<pwsh|powershell> -NoLogo -NonInteractive -Command "try{[Console]::OutputEncoding=[System.Text.Encoding]::UTF8}catch{}; $env:PATH"`,
  with stdin=null / stdout=piped / stderr=null / `.no_console_window()`.
- **This is orthogonal to `run_command`, which KEEPS `-NoProfile`** (`native_tools/shell.rs`).
  The tool shell stays fast and deterministic; node visibility comes from the *inherited process
  PATH* that this module fixed at startup — never from re-loading the profile per command.
  Do not "fix" a missing-node report by dropping `-NoProfile` in `shell.rs`.
- **Read-only.** Never write the registry, never write a profile. `read_registry_path` opens
  `KEY_READ` only.
- **Never block startup.** Both platforms use the same helper-thread + `recv_timeout` pattern;
  timeout = 3s (`PROFILE_SHELL_TIMEOUT` == mac `LOGIN_SHELL_TIMEOUT`). On any failure, silently
  degrade to the registry+defaults result.
- `parse_profile_path_output`: take the **last non-empty trimmed line**; accept only if it
  contains `;` or is drive-rooted (`^[A-Za-z]:\`). Rejects profile banner text / empty output
  being mistaken for a PATH.

## Validation & failure matrix

| Condition | Behavior |
|-----------|----------|
| profile probe returns a valid PATH | phase ③ merges its dirs into process PATH |
| no profile / profile throws / empty output | ② returns `None` → ③ skipped → identical to pre-fix |
| PowerShell missing / spawn error | ② returns `None` → degrade to registry+defaults |
| probe exceeds 3s | helper thread detached, ② returns `None` → degrade |
| output last line is plain text (no `;`, not drive-rooted) | `parse_*` returns `None` → degrade |

## Fallback stable dirs (second line of defense)

`common_dirs_windows()` also pushes, when the base var exists: `%NVM_SYMLINK%` (nvm-windows),
and fnm's default alias (`%FNM_DIR%` else `%LOCALAPPDATA%\fnm` / `%USERPROFILE%\.fnm`, each
pushing both `aliases\default` and `aliases\default\installation`). These cover the case where
the profile probe fails; the probe is the primary mechanism.

## Wrong vs Correct

**Wrong** — probe with `-NoProfile`, or before the registry merge:
```rust
// ❌ profile never loads → fnm's dir never appears → node still missing
Command::new("powershell").args(["-NoProfile", "-Command", "$env:PATH"]);
// ❌ scanning for pwsh before ① → picks "powershell" when the user actually uses pwsh 7
```

**Correct** — registry merge first, then a profile-loading probe, then re-merge:
```rust
// ① set_var(registry+defaults merge)   → now pwsh.exe is discoverable
// ② profile_shell_path() loads profile → captures fnm/nvm-injected dirs
// ③ if Some, set_var(merge with profile as a new source)
```

## Tests / verification

- Unit (all platforms, `#[cfg(any(..., test))]`): `merge_paths_windows` with a profile source
  (lands after `user`, before `defaults`; case-folded dedup; `profile=None` ≡ old behavior);
  `parse_profile_path_output` (normal / banner-then-path / single drive-root accept / plain-text
  reject / empty reject). Run via `scripts/win-cargo-test.ps1` (bare `cargo test` binaries fail
  `0xC0000139` on this Windows env; baseline has ~2 mac-path assertions that fail on Windows —
  not regressions).
- E2E (verified 2026-07-06, GUI chat-probe channel): simulated fnm by creating a Windows
  PowerShell profile that prepends a temp dir (absent from registry AND launching-shell PATH) to
  `$env:PATH`, put a uniquely-named marker `.cmd` there, launched the freshly-built GUI, and had
  the agent `run_command` the marker. It resolved and printed its token → the profile-injected
  dir reached the kivio process PATH via the startup probe, and `run_command`'s `-NoProfile`
  child inherited it. The marker's only possible source was the profile ⇒ the fix works
  end-to-end. Real-fnm acceptance on an actual fnm/nvm box remains the ideal final check.
