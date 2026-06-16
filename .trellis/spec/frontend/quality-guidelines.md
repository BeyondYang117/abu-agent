# Quality Guidelines

> Code quality standards for frontend development.

---

## Overview

<!--
Document your project's quality standards here.

Questions to answer:
- What patterns are forbidden?
- What linting rules do you enforce?
- What are your testing requirements?
- What code review standards apply?
-->

(To be filled by the team)

---

## Forbidden Patterns

<!-- Patterns that should never be used and why -->

(To be filled by the team)

---

## Required Patterns

<!-- Patterns that must always be used -->

### Chat motion utilities

- Keep routine Chat interaction motion centralized in `src/index.css` as reusable `chat-motion-*` utilities.
- Prefer applying those classes from Chat components over adding component-local keyframes or animation libraries.
- Always include a `prefers-reduced-motion` fallback when adding a new Chat animation utility.
- Use small entrance/reveal motions for state continuity; do not change Chat routing, persistence, or streaming data flow to support cosmetic motion.
- Streaming Chat scroll-follow must account for real content size changes, not only React data changes. When the user is still pinned to the bottom, observe the message-list inner content size and scroll again after late layout changes from Thinking scroll boxes, collapse animations, images, or tool blocks.
- User intent wins over scroll-follow: upward wheel/scroll must disable auto-follow until the user returns near the bottom.
- Motion tokens are the single source of truth: use the `:root` `--kv-ease-*` (standard / firm / spring / out) and `--kv-dur-*` (instant / fast / normal / slow) custom properties for every new chat animation/transition (CSS or Tailwind arbitrary values like `duration-[var(--kv-dur-fast)]`). Do not hardcode new cubic-beziers or durations.
- The reduced-motion guard is a blanket rule inside `@media (prefers-reduced-motion: reduce)` that sets `animation-duration`/`transition-duration` to `0.01ms` (NOT `0`). The `0.01ms` is deliberate: it keeps `transitionend`/`animationend` events firing so JS that waits on them to unmount DOM (e.g. attachment exit animations via `chat-motion-exit`) does not hang. Never add an animation that JS depends on completing to the `animation: none` list.
- Do not fade-in in-memory images. Chat artifact/attachment images are data URLs meant to display instantly ("秒显"); gating them behind `onLoad`/opacity (especially with `loading="lazy"`) can leave them invisible. Only fade images that have a genuine async load state the component manages.
- Reusable motion utilities live in `src/index.css`: `chat-motion-fade` (opacity), `chat-motion-fade-up`, `chat-motion-popover`, `chat-motion-reveal` (grid-rows height collapse), `chat-motion-modal-in`, `chat-motion-exit`, `chat-motion-pop` (success ✓), and the `kv-skeleton` loading shimmer. Reach for these before writing new keyframes.
- Content kept mounted while visually collapsed (e.g. `chat-motion-reveal` panels, the slide-out sidebar) must be removed from the tab order / a11y tree with the `inert` DOM property (set via a ref in `useLayoutEffect`, since React 18.2 has no `inert` JSX prop). `overflow: hidden` / `opacity: 0` alone leave focusable controls keyboard-reachable (WCAG 2.1.1).
- Theme (light↔dark) transitions must be gated behind a `theme-transitions-ready` class added to `<html>` one frame AFTER the first successful theme application (not before, and not on the settings-load error path) — otherwise the initial paint animates from light to the resolved theme and flashes.
- `transition` is a SHORTHAND that replaces the whole transition list. A higher-specificity rule that only declares some properties will silently drop an element's other transitions. Concretely: the gated `.theme-transitions-ready .chat-sidebar-shell` theme rule declaring only `background-color`/`border-color` once clobbered the sidebar's collapse `margin-left`/`opacity` transition (two-class selector beats the one-class base), turning the slide-out back into a hard jump. When adding theme/state transitions to an element that ALREADY animates (sidebar collapse, composer focus glow), declare ALL its transitioned properties together in the combined rule, or exclude that element from the shared rule.
- Success/celebration micro-animations (e.g. the tool-complete ✓ pop) must fire only on a live state transition, not on historical remount. Conversation switch remounts the message list, so gate such animations on a previous-status ref, or many historical items animate at once.

### Chat Markdown rendering

- Keep Chat message Markdown rendering on the eager Chat load path. `MessageBubble` should statically import `ChatMarkdown` so conversation history and first assistant content render with Markdown immediately.
- Do not lazy-load `ChatMarkdown` at the individual message boundary just to reduce the Chat chunk size. A plain-text fallback flash is worse for the Chat client than the bundle-size win.
- Markdown parser optimizations should target memoization, normalized input stability, and avoiding unnecessary historical-message rerenders.
- Render assistant answer text through `ChatMarkdown`, but keep `ReasoningBlock` / Thinking content out of Markdown rendering. Thinking text must use a fixed-height plain-text scroll area that preserves raw newlines and characters; fenced code and inline backticks stay literal text instead of becoming code cards. This keeps streaming layout height stable and prevents malformed model reasoning from reshaping the message.
- Tailwind Typography adds visual backticks around inline `code` via pseudo-elements by default. Chat Markdown containers must disable `code::before` and `code::after` content and use explicit inline-code styling instead.
- Render fenced code blocks with a Chat-specific `not-prose` component instead of Tailwind Typography's default dark `pre`. Code blocks should use a light card surface, compact inline language label, copy action, horizontal overflow, and lightweight syntax coloring without adding a large highlighting dependency unless the product explicitly accepts the bundle cost.

### Chat agent todo UI

- Render the persistent agent todo state as a compact Chat titlebar indicator with a popover for the full list.
- Do not render agent todo state as a sticky panel inside the message list. The message stream should remain the chronological conversation, not the assistant's current workspace dashboard.
- Keep `todo_write` and `todo_update` tool calls in the assistant timeline as historical trace entries, but summarize their arguments/results compactly instead of showing the full todo state payload by default.

### Tauri window lifecycle

- Keep Tauri window labels scoped to one user-facing surface: `main` is the input translator, `chat` is the AI client, `settings` is the standalone settings page, and `lens` is the capture/vision overlay.
- Do not reuse `main` as a generic route container for Chat or Settings. Heavy or infrequently used views should get their own `WebviewWindow` label so closing the view can destroy its WebView process.
- Except for `lens`, close buttons and Esc handlers should close the current window instead of hiding it. Hiding keeps the WebView resident and can keep WebView2/WKWebView renderer memory alive in the background.
- `lens` is the explicit exception because capture selection needs fast reuse and has special temporary-image cleanup behavior.
- When adding a new top-level view, wire the route, Tauri command/event target, and window label together. Avoid broadcasting route-change events to unrelated windows.
- Windows frameless Chat must not use a transparent shell gutter or native window shadow. On Windows, WebView transparent regions can reveal the native window rectangle as a second outer frame; keep the Chat shell flush with the window bounds and use only the inner CSS border/radius for the visible edge. Window minimum sizes and default sizes should reflect the visible Chat content area directly: 400/640 x 400 minimum and 1280 x 800 default contracts.
- Do not enable native shadows globally for Lens. On Windows, Lens can keep a full-screen transparent window and crop visible regions with platform APIs, so native undecorated shadow can create unwanted full-screen edge artifacts. Use CSS borders/shadows on Lens floating cards instead, and keep the select overlay visually borderless.

### Pyodide image/chart execution

- When `run_python` code imports `matplotlib`, force the `Agg` backend before running user code.
- Warm up `matplotlib.pyplot` once before the real execution so first-run backend initialization errors do not leak into the visible tool log.
- If the initial `matplotlib` execution still fails with a Pyodide/wasm/backend-style error, retry once inside the sandbox before surfacing a user-visible failure.
- Save generated images to relative filenames inside the Pyodide filesystem and let Kivio capture them as artifacts; do not require the model to print base64.
- Suppress non-fatal dependency warnings (`DeprecationWarning`, `PendingDeprecationWarning`, `FutureWarning`, `ResourceWarning`) in the user-code wrapper before executing sandbox code. These warnings, such as pandas/PyArrow deprecation notices, must not turn a successful artifact generation into a red failed tool call.

### Pyodide sandbox package boundary

- Treat `run_python` as a Pyodide/browser sandbox, not as host Python.
- The security boundary is host filesystem access: sandboxed Python must not read or write `/Users`, app resources, or other host paths.
- Compatible packages may be downloaded inside the Pyodide sandbox with `micropip`; do not describe `run_python` as completely networkless.
- Prefer bundled/local Pyodide packages first, then sandbox-local `micropip` fallback for missing imports.
- Do not use `run_command`, host `pip`, or `python -m pip` to work around sandbox package failures unless the user explicitly asks to modify the host Python environment.
- When `run_python` needs to analyze Kivio attachment safe copies, pass safe-copy paths through the tool's `files` argument. Rust validates and reads only approved chat-attachment/temp inputs, the frontend mounts them in Pyodide, and Python code must read the virtual paths from `KIVIO_INPUT_FILES`.
- Keep Rust tool descriptions, system prompts, and frontend runner behavior consistent when changing this boundary.

### Release packaging for document Skills

- Bundling `pdf`, `docx`, and `xlsx` Skills means bundling their execution runtime too; `SKILL.md` files alone are not a complete release.
- Production installers must include Pyodide core files, `python_stdlib.zip`, and local wheels for common packages used by document/data workflows: `numpy`, `pandas`, `matplotlib`, `scipy`, `sympy`, `scikit-learn`, `statsmodels`, `pillow`, `seaborn`, and `micropip`.
- `run_python` package loading must prefer packaged local resources. CDN loading may exist as a fallback only, not as the normal required path for document analysis.
- Release verification must inspect the final DMG / MSI / NSIS artifacts and confirm that both `skills/pdf|docx|xlsx` and the Python/Pyodide runtime package files are present in the installed app resources.
- The canonical release checklist lives in `docs/RELEASE_PACKAGING.md`; update it whenever the packaging flow changes.

---

## Testing Requirements

- Pure utility modules under `src/**` must ship Vitest unit tests alongside the source file (`*.test.ts`).
- React components with non-trivial UI state (Chat tool blocks, reasoning collapse, settings controls) should ship colocated `*.test.tsx` files using `@testing-library/react` with `jsdom`.
- Shared test setup lives in [`src/test/setup.ts`](../../src/test/setup.ts); mock heavy dependencies (Tauri bridge, `ChatMarkdown`) at the test boundary instead of booting the full desktop shell.
- Bug fixes in shared frontend utilities or UI regressions should include a targeted regression test when behavior is deterministic.
- Every PR must pass CI: `npm run lint`, `npm run typecheck`, `npm run test`, and `cargo test --manifest-path src-tauri/Cargo.toml`.
- Full Tauri window/hotkey/capture E2E remains manual smoke testing until a dedicated WebDriver job is added.
- Backend-only changes should add or update targeted Rust unit tests in the touched module before merge when practical.

---

## Code Review Checklist

<!-- What reviewers should check -->

(To be filled by the team)
