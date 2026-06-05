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

### Chat Markdown rendering

- Keep Chat message Markdown rendering on the eager Chat load path. `MessageBubble` should statically import `ChatMarkdown` so conversation history and first assistant content render with Markdown immediately.
- Do not lazy-load `ChatMarkdown` at the individual message boundary just to reduce the Chat chunk size. A plain-text fallback flash is worse for the Chat client than the bundle-size win.
- Markdown parser optimizations should target memoization, normalized input stability, and avoiding unnecessary historical-message rerenders.

### Pyodide image/chart execution

- When `run_python` code imports `matplotlib`, force the `Agg` backend before running user code.
- Warm up `matplotlib.pyplot` once before the real execution so first-run backend initialization errors do not leak into the visible tool log.
- If the initial `matplotlib` execution still fails with a Pyodide/wasm/backend-style error, retry once inside the sandbox before surfacing a user-visible failure.
- Save generated images to relative filenames inside the Pyodide filesystem and let Kivio capture them as artifacts; do not require the model to print base64.

---

## Testing Requirements

<!-- What level of testing is expected -->

(To be filled by the team)

---

## Code Review Checklist

<!-- What reviewers should check -->

(To be filled by the team)
