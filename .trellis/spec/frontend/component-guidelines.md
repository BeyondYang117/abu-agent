# Component Guidelines

> How components are built in this project.

---

## Overview

<!--
Document your project's component conventions here.

Questions to answer:
- What component patterns do you use?
- How are props defined?
- How do you handle composition?
- What accessibility standards apply?
-->

(To be filled by the team)

---

## Component Structure

<!-- Standard structure of a component file -->

(To be filled by the team)

---

## Props Conventions

<!-- How props should be defined and typed -->

(To be filled by the team)

---

## Styling Patterns

<!-- How styles are applied (CSS modules, styled-components, Tailwind, etc.) -->

(To be filled by the team)

---

## Accessibility

<!-- A11y requirements and patterns -->

### Convention: Lens keyboard focus surface

**What**: Lens modes that do not render an input (`translate` and `translateText`) must still keep a programmatically focusable root surface, usually `tabIndex={-1}`, and focus it after the native Tauri window is shown or resized.

**Why**: `Escape` close/cancel handlers are implemented in the webview. If no webview element owns keyboard focus, macOS/Windows may route `Escape` to the previous app or play the system alert sound until the user clicks the Lens window.

**Example**:
```tsx
const rootRef = useRef<HTMLDivElement>(null)

await getCurrentWindow().setFocus()
rootRef.current?.focus({ preventScroll: true })

return <div ref={rootRef} tabIndex={-1} />
```

**Rule**: When adding a new Lens stage or mode, update the focus helper's allowed stage list. Chat mode should keep focusing its text input; non-input modes should focus the root surface.

### Convention: IME-safe Enter submission

**What**: Text inputs or textareas that submit on Enter must ignore Enter while IME composition is active. Check both `event.nativeEvent.isComposing` and `event.keyCode === 229` before calling `preventDefault()` or the submit handler.

**Why**: Chinese, Japanese, and Korean input methods often use Enter to confirm a candidate. Treating that keydown as submit sends incomplete content and makes the Chat composer feel unreliable.

**Example**:
```tsx
const handleKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
  if (event.key !== 'Enter' || event.shiftKey) return
  if (event.nativeEvent.isComposing || event.keyCode === 229) return
  event.preventDefault()
  handleSubmit()
}
```

**Rule**: Apply this guard to every composer-style input that sends, confirms, saves, or commits content on Enter. Keep Shift+Enter newline behavior intact when the control supports multiline input.

---

## Common Mistakes

<!-- Component-related mistakes your team has made -->

- Adding keyboard shortcuts only to `window` listeners without ensuring the Tauri webview is focused. Window listeners do not fire when the OS focus remains on the previously active app.
- Handling Enter submit in a composer without IME guards. This breaks candidate confirmation for CJK input methods and can send half-composed messages.
