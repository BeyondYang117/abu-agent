# Hook Guidelines

> How hooks are used in this project.

---

## Overview

<!--
Document your project's hook conventions here.

Questions to answer:
- What custom hooks do you have?
- How do you handle data fetching?
- What are the naming conventions?
- How do you share stateful logic?
-->

(To be filled by the team)

---

## Custom Hook Patterns

<!-- How to create and structure custom hooks -->

(To be filled by the team)

---

## Data Fetching

<!-- How data fetching is handled (React Query, SWR, etc.) -->

(To be filled by the team)

---

## Naming Conventions

<!-- Hook naming rules (use*, etc.) -->

(To be filled by the team)

---

## Common Mistakes

### Async Tauri Event Listeners

**Problem**: Tauri event listener helpers return `Promise<UnlistenFn>`. In React StrictMode, an effect can be mounted, cleaned up, and mounted again before the first promise resolves. If cleanup only calls `unlisten?.()`, the unresolved listener survives and future stream events are handled multiple times.

**Rule**: Any `useEffect` that registers an async Tauri listener must use a `cancelled` flag and dispose the listener immediately if the promise resolves after cleanup.

```tsx
useEffect(() => {
  let cancelled = false
  let unlisten: (() => void) | undefined

  api.onChatStream((payload) => {
    if (cancelled) return
    // handle payload
  }).then((dispose) => {
    unlisten = dispose
    if (cancelled) dispose()
  })

  return () => {
    cancelled = true
    unlisten?.()
  }
}, [])
```

**Applies to**: `api.onChatStream`, `api.onLensStream`, `api.onLensTranslateStream`, `api.onOpenSettings`, and any future Tauri event listener wrapper used inside React effects.
