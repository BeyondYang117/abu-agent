# Research: PI's TUI library (pi-tui) — for porting to Rust ("Kivio Code")

- **Query**: Study `packages/tui/src/` exhaustively (rendering model, terminal abstraction, input handling, editor, components, autocomplete/fuzzy, themes) and produce a Rust implementation strategy + component build checklist.
- **Scope**: external/cross-repo (PI monorepo at `/Users/zmair/ZM database/Kivio agent/pi`)
- **Date**: 2026-06-16

All file refs below are absolute under `/Users/zmair/ZM database/Kivio agent/pi/packages/tui/src/` unless noted. Docs are under `/Users/zmair/ZM database/Kivio agent/pi/packages/coding-agent/docs/`.

---

## Findings

### Files Found

| File Path | Description |
|---|---|
| `tui.ts` (1642 lines) | The core: `Component`/`Focusable`/`Container` interfaces, `TUI` class with differential renderer, overlay stack, hardware-cursor/IME positioning, OSC11 bg query, Kitty image diffing. **The hardest single file to port.** |
| `terminal.ts` (532 lines) | `Terminal` interface + `ProcessTerminal`: raw mode, bracketed paste, Kitty keyboard protocol negotiation, `modifyOtherKeys` fallback, Windows VT input, OSC 9;4 progress, cursor/clear primitives. |
| `terminal-colors.ts` (63 lines) | OSC 11 background-color response parsing → `RgbColor`. |
| `terminal-image.ts` (~440 lines) | Kitty / iTerm2 image protocols, capability detection, cell-size, OSC 8 hyperlinks, image dimension parsers (png/jpeg/gif/webp). |
| `stdin-buffer.ts` (435 lines) | Splits batched stdin into complete escape sequences (CSI/OSC/DCS/APC/SS3), bracketed-paste extraction, 10ms flush timer. Based on OpenTUI. |
| `keys.ts` (1399 lines) | Key decoding: `matchesKey`/`parseKey`/`Key`/`decodeKittyPrintable`; Kitty protocol + legacy + modifyOtherKeys; event types (press/repeat/release). |
| `keybindings.ts` (245 lines) | `KeybindingsManager`, `TUI_KEYBINDINGS` table, user-override config, conflict detection. |
| `native-modifiers.ts` (60 lines) | macOS native `.node` addon to read live modifier state (Apple Terminal Shift+Enter fallback). |
| `word-navigation.ts` (118 lines) | `findWordBackward`/`findWordForward` using `Intl.Segmenter` word granularity + punctuation boundaries. |
| `autocomplete.ts` (~530 lines) | `AutocompleteProvider` interface + `CombinedAutocompleteProvider` (slash commands + `@file` fuzzy paths via `fd`/`fdPath`). |
| `fuzzy.ts` (138 lines) | `fuzzyMatch`/`fuzzyFilter` — subsequence match scoring (consecutive bonus, word-boundary bonus, gap penalty). |
| `editor-component.ts` (75 lines) | `EditorComponent` interface (the contract custom editors implement). |
| `undo-stack.ts` (29 lines) | Generic `UndoStack<S>` with `structuredClone` snapshots. |
| `kill-ring.ts` (47 lines) | Emacs kill-ring: push/peek/rotate with accumulate. |
| `utils.ts` (~1180 lines) | `visibleWidth`, `truncateToWidth`, `wrapTextWithAnsi`, `sliceByColumn`, `sliceWithWidth`, `extractSegments`, `extractAnsiCode`, grapheme/word segmenters, east-asian width. |
| `components/editor.ts` (2350+ lines) | The big multi-line editor: layout, scrolling, history, paste markers, kill-ring, undo, jump-to-char, autocomplete integration. |
| `components/input.ts` (448 lines) | Single-line input with horizontal scroll, kill-ring, undo, IME cursor marker. |
| `components/markdown.ts` (~900 lines) | Markdown renderer via `marked` tokenizer + theme + syntax highlight hook. |
| `components/select-list.ts` (230 lines) | Scrollable filterable selection list with two-column primary/description layout. |
| `components/settings-list.ts` (~300 lines) | Toggle/cycle settings list with optional fuzzy search + submenus. |
| `components/box.ts` (138) / `text.ts` (107) / `spacer.ts` (29) / `truncated-text.ts` (66) | Layout/content primitives with bg-fn + padding + caching. |
| `components/loader.ts` (93) / `cancellable-loader.ts` (41) | Animated spinner (extends `Text`) + escape-to-cancel variant with `AbortController`. |
| `components/image.ts` (127) | Image component wrapping terminal-image protocols with fallback. |
| `index.ts` (110) | Public API surface (exports). |

### The rendering model — differential rendering

This is the crown jewel and the hardest thing to replicate. PI does **not** use a cell grid/back-buffer like ratatui. Instead it is **line-based**: every component's `render(width)` returns `string[]` (one ANSI-bearing string per terminal row), the whole tree is flattened, and the renderer diffs *line arrays* between frames and emits the minimal cursor moves + line rewrites.

**Component contract** (`tui.ts:58-82`):
```ts
interface Component {
  render(width: number): string[];   // each line MUST be ≤ width visible cells
  handleInput?(data: string): void;  // when focused
  wantsKeyRelease?: boolean;          // opt into Kitty key-release events
  invalidate(): void;                 // drop caches (theme change / resize)
}
```
- `Container` (`tui.ts:250-284`) just concatenates children's lines vertically. There is **no horizontal layout engine** — horizontal composition is done by string building inside a component (e.g. `select-list` builds two columns by hand), and only overlays composite horizontally.
- `Focusable` (`tui.ts:98-114`): a focused component emits a zero-width APC marker `CURSOR_MARKER = "\x1b_pi:c\x07"` (`tui.ts:114`) at the text cursor; TUI finds/strips it and positions the *hardware* cursor there for IME candidate windows.

**The render loop** (`TUI.doRender`, `tui.ts:1208-1574`) — study this carefully:
1. `render(width)` produces `newLines`; overlays are composited in (`compositeOverlays`, `tui.ts:986-1045`).
2. `extractCursorPosition` (`tui.ts:1188-1206`) scans the bottom `height` lines for `CURSOR_MARKER`, records `{row,col}`, strips it.
3. `applyLineResets` (`tui.ts:1049-1058`) appends `\x1b[0m\x1b]8;;\x07` (SGR reset + OSC 8 hyperlink reset) to every non-image line. **Styles never carry across lines.**
4. Decide redraw strategy:
   - First render (`previousLines.length===0`): `fullRender(false)` — write everything, no clear (assumes clean screen).
   - **Width changed** → always `fullRender(true)` (clear scrollback `\x1b[2J\x1b[H\x1b[3J` + redraw) because wrapping changes (`tui.ts:1297`).
   - **Height changed** (non-Termux) → `fullRender(true)` to realign viewport (`tui.ts:1306`); Termux is special-cased because the soft keyboard toggles height.
   - Otherwise → **differential**: find `firstChanged`/`lastChanged` line indices (`tui.ts:1322-1335`), move the cursor there with relative `\x1b[nA`/`\x1b[nB` + `\r`, rewrite only the changed range, each line cleared with `\x1b[2K`, then clear trailing deleted lines.
5. Everything is wrapped in **synchronized output** `\x1b[?2026h … \x1b[?2026l` (`tui.ts:1240,1262,1417,1524`) so the terminal renders atomically (no flicker/tearing).
6. Cursor bookkeeping: `cursorRow` (logical end of content), `hardwareCursorRow` (actual terminal row), `previousViewportTop`, `maxLinesRendered` (high-water mark). Scrolling is handled by emitting `\r\n` to push content up when changes are below the viewport bottom (`tui.ts:1419-1432`).
7. A hard safety check: if any rendered line's `visibleWidth(line) > width`, it dumps a crash log and throws (`tui.ts:1474-1500`) — width overflow is fatal because it corrupts the differential model.

**Render scheduling** (`requestRender`, `tui.ts:681-728`): coalesces requests, throttled to `MIN_RENDER_INTERVAL_MS = 16` (~60fps) via `process.nextTick` + `setTimeout`. `force=true` resets all diff state for a full repaint.

**Resize**: handled by re-render on `process.stdout 'resize'` event (`terminal.ts:150`); width change forces full redraw, height change too (except Termux). On start it self-sends SIGWINCH to refresh stale dims after suspend/resume.

**Kitty image diffing** (`tui.ts:1060-1127`, `1078-1090`): image "lines" are special — the renderer expands the changed range to cover an image's reserved rows, deletes prior Kitty image IDs before redraw, and reserves N blank rows for an R-row image.

### Terminal abstraction (`terminal.ts`)

`Terminal` interface (`terminal.ts:52-94`) is the seam the renderer writes through:
- `start(onInput, onResize)`, `stop()`, `drainInput(maxMs, idleMs)`, `write(data)`.
- `columns`/`rows` getters; `kittyProtocolActive`.
- `moveBy(lines)`, `hideCursor`/`showCursor`, `clearLine`/`clearFromCursor`/`clearScreen`, `setTitle` (OSC 0), `setProgress` (OSC 9;4 with 1s keepalive interval).

`ProcessTerminal` (`terminal.ts:99-531`):
- **Raw mode**: `process.stdin.setRawMode(true)`, `setEncoding('utf8')`, `resume()`; saves/restores prior raw state on stop.
- **Bracketed paste**: enables `\x1b[?2004h` on start, disables `\x1b[?2004l` on stop.
- **Kitty keyboard protocol negotiation** (`terminal.ts:220-307`): writes `\x1b[>7u\x1b[?u\x1b[c` (push flags 1|2|4 = disambiguate + report event types + report alternate keys, then query, then DA sentinel). Parses the response; if Kitty flags come back nonzero → enable Kitty; if DA comes back first → fall back to xterm `modifyOtherKeys` (`\x1b[>4;2m`). The negotiation buffer handles responses split across reads with a 150ms fragment timeout.
- **Windows VT input**: loads a native `.node` addon to set `ENABLE_VIRTUAL_TERMINAL_INPUT` so Shift+Tab etc. arrive as VT sequences (`terminal.ts:338-366`).
- **drainInput** (`terminal.ts:368-404`): disables Kitty/modifyOtherKeys then drains stray bytes so late key-release events don't leak to the parent shell over SSH.
- Cursor/clear ops are thin ANSI writers (`terminal.ts:473-523`): `moveBy` → `\x1b[nB`/`\x1b[nA`; `clearScreen` → `\x1b[2J\x1b[H`.
- Optional `PI_TUI_WRITE_LOG` taps the raw ANSI stream to a file — extremely useful for debugging the renderer.

ANSI/color/style is **not** abstracted — components emit raw SGR codes themselves (e.g. `\x1b[7m…\x1b[27m` for inverse cursor in `input.ts:437`). Color *values* come from the theme layer (see Theme system).

### Input handling

Three stages: **stdin buffering → key decoding → keybinding match**.

1. **`StdinBuffer`** (`stdin-buffer.ts`): accumulates bytes and emits *complete* escape sequences via a `data` event, and pasted content via a `paste` event. `isCompleteSequence` (`stdin-buffer.ts:29-78`) classifies CSI/OSC/DCS/APC/SS3/meta and decides complete vs incomplete; SGR mouse sequences (`\x1b[<…m/M`) and old mouse (`\x1b[M`+3 bytes) are special-cased. Bracketed paste (`\x1b[200~`…`\x1b[201~`) is extracted as one `paste` event. A 10ms timeout flushes incomplete buffers. Handles the WezTerm `\x1b\x1b[…u` double-escape edge case (`stdin-buffer.ts:217-230`) and Kitty unmodified-printable dedup (`stdin-buffer.ts:184-190,389-398`). `ProcessTerminal` re-wraps paste content back into bracketed-paste markers before forwarding to the input handler (`terminal.ts:195-199`).
2. **Key decoding** (`keys.ts`): `matchesKey(data, "ctrl+c")` is the workhorse. It parses the key-id string into `{key, ctrl, shift, alt, super}` (`keys.ts:788-801`), then checks the raw bytes against (a) Kitty CSI-u sequences (`parseKittySequence`, `keys.ts:587-651`, regex `^\x1b\[(\d+)(?::(\d*))?(?::(\d+))?(?:;(\d+))?(?::(\d+))?u$`), (b) xterm modifyOtherKeys `\x1b[27;mod;code~`, and (c) legacy sequences (big static tables `LEGACY_KEY_SEQUENCES`/`LEGACY_SHIFT_SEQUENCES`/`LEGACY_CTRL_SEQUENCES`, `keys.ts:368-481`). Handles: modifiers bitmask (shift=1, alt=2, ctrl=4, super=8; `keys.ts:292-297`), lock-mask stripping, numpad→base normalization, shifted-letter identity, non-Latin base-layout-key matching (Cyrillic Ctrl+С → Ctrl+c), ctrl-char formula `code & 0x1f`. Event types press/repeat/release: `isKeyRelease`/`isKeyRepeat` (`keys.ts:527-577`) detect `:3`/`:2` in the sequence; `_lastEventType` is module state. `decodeKittyPrintable` (`keys.ts:1349`) turns a CSI-u printable back into the character. There is also Apple Terminal Shift+Enter native-modifier fallback (`terminal.ts:44-47`, `native-modifiers.ts`).
3. **Keybindings** (`keybindings.ts`): `KeybindingsManager` maps action ids (`tui.editor.cursorUp`, `tui.input.submit`, …) → arrays of key-ids, merging `TUI_KEYBINDINGS` defaults (`keybindings.ts:54-134`) with user overrides; `matches(data, id)` loops `matchesKey`. Components call `getKeybindings().matches(data, "tui.editor.deleteWordBackward")`. Full default table is documented in `docs/keybindings.md` (editor movement, deletion, kill-ring, input, selection, app-level).

Input routing in TUI (`handleInput`, `tui.ts:730-801`): OSC11 bg replies consumed first → global input listeners → cell-size response → global debug key (Shift+Ctrl+D) → overlay focus resolution → focused component's `handleInput` (key-release filtered unless `wantsKeyRelease`).

### The editor component (`components/editor.ts`)

The most complex component. `Editor implements Component, Focusable`. State is `{lines: string[], cursorLine, cursorCol}` (`editor.ts:209-213`).

- **Multi-line + wrapping**: `wordWrapLine(line, maxWidth, preSegmented?)` (`editor.ts:114-`) produces `TextChunk[]` with `{text,startIndex,endIndex}`, wrapping at whitespace, backtracking to the last wrap opportunity, force-breaking long tokens, and allowing CJK break between adjacent CJK graphemes (`cjkBreakRegex`). `layoutText(contentWidth)` (`editor.ts:874`) flattens logical lines into `LayoutLine[]` with `{text, hasCursor, cursorPos}`.
- **Render** (`editor.ts:460-`): draws a top/bottom horizontal border (`borderColor("─")`), reserves cursor column when unpadded, vertical scroll with `maxVisibleLines = max(5, floor(rows*0.3))`, scroll indicators `─── ↑ N more`, inverse-video fake cursor `\x1b[7m…\x1b[0m` + `CURSOR_MARKER` when focused, and appends the autocomplete dropdown below the border.
- **Cursor movement**: char/word/line-start/line-end/vertical with sticky preferred-visual-column (`preferredVisualCol`), page up/down, jump-to-char (`ctrl+]`). Word nav delegates to `word-navigation.ts`.
- **Undo**: `UndoStack<EditorState>` (`undo-stack.ts`) with clone-on-push; word-typing coalesces into one undo unit (`lastAction === "type-word"`).
- **Kill-ring**: `KillRing` (`kill-ring.ts`) — `ctrl+k`/`ctrl+u`/`ctrl+w`/`alt+d` push (with prepend/accumulate), `ctrl+y` yank, `alt+y` yank-pop (rotate). Pure emacs semantics.
- **Paste markers**: large pastes are stored in `pastes: Map<number,string>` and represented in the buffer as an atomic marker so word-nav/wrapping treat them as one unit (`isPasteMarker`, `expandPasteMarkers`, `validPasteIds`). Bracketed paste handled in `handleInput` (`editor.ts:611-635`).
- **History**: up/down navigates prior submissions when on first/last visual line.
- **Autocomplete integration** (`editor.ts:574-707, 2063-2300`): owns a `SelectList` dropdown, a debounced async request pipeline against the `AutocompleteProvider` (with `AbortController`, request-id token guarding), triggered by `@`/`#`/`/` patterns or explicit Tab; `applyCompletion` rewrites lines+cursor.
- `EditorComponent` interface (`editor-component.ts`) is the public contract custom editors (e.g. vim mode) implement: `getText/setText/handleInput/onSubmit/onChange` + optional history/insert/autocomplete/appearance.

### Other component contracts

| Component | Constructor / props | `render` output |
|---|---|---|
| **Text** (`text.ts`) | `(text, paddingX=1, paddingY=1, bgFn?)` | Word-wrapped (`wrapTextWithAnsi`) lines + padding, optional bg via `applyBackgroundToLine`, cached by `(text,width)`. |
| **Box** (`box.ts`) | `(paddingX=1, paddingY=1, bgFn?)` + `children` | Renders children at `width-2*paddingX`, left-pads, applies bg+padding to every line; caches with bg-sample invalidation. |
| **Spacer** (`spacer.ts`) | `(lines=1)` | N empty strings. |
| **TruncatedText** (`truncated-text.ts`) | `(text, paddingX=0, paddingY=0)` | First line only, `truncateToWidth`, padded to width. |
| **Input** (`input.ts`) | single-line; `onSubmit`/`onEscape`, `focused` | One line with `> ` prompt, horizontal scroll keeping cursor centered, inverse cursor + `CURSOR_MARKER`. Full kill-ring + undo + word-nav + bracketed paste (newlines stripped). |
| **SelectList** (`select-list.ts`) | `(items: SelectItem[], maxVisible, theme, layout?)`; `onSelect`/`onCancel`/`onSelectionChange` | Scrollable window (centers selection), `→ ` prefix on selected, two-column label/description with computed primary-column width, `(i/n)` scroll info, wraps at ends. `SelectItem = {value,label,description?}`; theme is 5 color fns. |
| **SettingsList** (`settings-list.ts`) | `(items: SettingItem[], maxVisible, theme, onChange, onCancel, {enableSearch})` | Toggle/cycle values per row; optional fuzzy search via embedded `Input`; submenu support. `SettingItem = {id,label,description?,currentValue,values?,submenu?}`. |
| **Markdown** (`markdown.ts`) | `(text, paddingX, paddingY, theme, defaultTextStyle?, options?)` | Parses with `marked` (custom strict-strikethrough tokenizer), renders headings/links(OSC8)/code/quote/hr/lists/bold/italic with `MarkdownTheme` fns + optional `highlightCode(code,lang)` syntax hook + `codeBlockIndent`. Cached by `(text,width)`. |
| **Loader** (`loader.ts`) | `(tui, spinnerColorFn, messageColorFn, message, indicator?)` extends `Text` | Animated braille spinner (`⠋⠙⠹…`, 80ms) + message; `setInterval` calls `tui.requestRender()`. `LoaderIndicatorOptions = {frames?, intervalMs?}`. |
| **CancellableLoader** (`cancellable-loader.ts`) | extends `Loader` | Adds `AbortController`; `escape`/`ctrl+c` aborts and fires `onAbort`. |
| **Image** (`image.ts`) | `(base64, mimeType, theme, options?, dimensions?)` | Kitty/iTerm2 sequence + reserved blank rows, or `imageFallback` text when unsupported. `ImageOptions = {maxWidthCells?, maxHeightCells?, filename?, imageId?}`. |

### Overlays (`tui.ts:485-1045`)

`showOverlay(component, options)` returns an `OverlayHandle` (`hide/setHidden/focus/unfocus/isFocused`). Overlays are an ordered stack with focus-restore bookkeeping; `OverlayOptions` supports anchor (9 positions), width/minWidth/maxHeight as number or `"50%"`, row/col absolute or percentage, margins, responsive `visible(w,h)`, `nonCapturing`. `compositeOverlays` (`tui.ts:986-1045`) renders each overlay to its own width, pads the base to terminal height, and **horizontally splices** each overlay line into the base line at `(row,col)` via `compositeLineAt` (`tui.ts:1130-1178`) — the one place PI does true 2D composition, using `extractSegments`/`sliceWithWidth` to slice ANSI-aware substrings and pad. This is the closest PI gets to a cell grid.

### Width / wrapping / ANSI utilities (`utils.ts`)

The renderer's correctness rests on these:
- `visibleWidth(str)` (`utils.ts:216`): strips ANSI/OSC/APC via `extractAnsiCode`, sums grapheme widths via `Intl.Segmenter` + `get-east-asian-width` + emoji heuristics; caches non-ASCII results (512-entry LRU); fast-paths pure ASCII; tabs→3 spaces.
- `wrapTextWithAnsi(text, width)` (`utils.ts:694`): word-wrap preserving ANSI state across wrapped lines via an `AnsiCodeTracker` (reapplies active SGR codes at the start of each new line, emits line-end resets). Handles long-word break.
- `truncateToWidth(str, width, ellipsis?)`, `sliceByColumn(line, startCol, length, strict?)`, `sliceWithWidth`, `extractSegments` — ANSI-aware column slicing used by overlay compositing and horizontal scroll.
- `applyBackgroundToLine(line, width, bgFn)` — pads to width then wraps in a bg color fn.
- `getGraphemeSegmenter`/`getWordSegmenter`, `cjkBreakRegex`, `PUNCTUATION_REGEX`, `isWhitespaceChar`, `normalizeTerminalOutput` (Thai/Lao AM-vowel decomposition workaround).

### Autocomplete + fuzzy

- `fuzzy.ts`: `fuzzyMatch(query, text)` returns `{matches, score}` — subsequence match (chars in order, not necessarily contiguous); rewards consecutive runs (`-consecutive*5`), word-boundary matches (`-10`), exact match (`-100`); penalizes gaps (`+gap*2`) and later positions (`+i*0.1`); also tries alpha/digit-swapped query. `fuzzyFilter(items, query, getText)` tokenizes on whitespace/slash (all tokens must match) and sorts ascending by score.
- `autocomplete.ts`: `AutocompleteProvider` interface = `getSuggestions(lines, cursorLine, cursorCol, {signal, force})` → `{items, prefix}` + `applyCompletion(...)` + optional `shouldTriggerFileCompletion`. `CombinedAutocompleteProvider` handles slash-commands (`/cmd`, fuzzy-filtered) and `@file` path completion (quoted-path aware, shells out to `fd` when `fdPath` is set, else `fs` walk). `AutocompleteItem = {value,label,description?}`, `SlashCommand = {name,description?,argumentHint?,getArgumentCompletions?}`.

### Theme system

Themes are **not** in pi-tui itself — pi-tui components take *color functions* (`(s:string)=>string`) as props (e.g. `MarkdownTheme`, `SelectListTheme`, `EditorTheme`, `ImageTheme`). The concrete theme lives in the coding-agent layer: `docs/themes.md` documents JSON themes with `vars` + 51 required `colors` tokens (accent, border*, success/error/warning, muted/dim/text, selectedBg, userMessageBg/Text, tool*Bg, md* (10), toolDiff* (3), syntax* (9), thinking* (6), bashMode). Color values: hex `#rrggbb`, 256-color index, `vars` reference, or `""` (terminal default). The app converts a theme JSON → a `theme` object exposing `theme.fg(token, text)` / `theme.bg(token, text)` / `theme.bold(text)` that produce ANSI-wrapped strings, and passes the relevant fns into each component. On theme change the app calls `TUI.invalidate()` which recursively clears component caches; components that pre-bake theme colors must rebuild on `invalidate()` (documented pattern in `docs/tui.md`).

### Terminal setup constraints (`docs/terminal-setup.md`)

Kitty keyboard protocol is the happy path (Kitty/iTerm2/Ghostty/WezTerm/VSCode≥1.109.5). Fallbacks: Apple Terminal Shift+Enter via native modifiers; xterm `modifyOtherKeys`; per-terminal config for Shift+Enter/Alt+Enter (`\x1b[13;2u`/`\x1b[13;3u`). `PI_HARDWARE_CURSOR=1` for IME on some terminals.

---

## Rust implementation strategy ("Kivio Code" TUI)

### Recommendation: **crossterm for the terminal layer + a hand-ported PI-style differential line renderer. Do NOT use ratatui's widget/buffer model.**

Rationale:

1. **PI's renderer is line-based and ANSI-string-based, not cell-grid-based.** ratatui's whole model is a `Buffer` of `Cell { symbol, fg, bg, modifier }` cells diffed against a back-buffer, with widgets writing into a `Rect`. That is a *different and incompatible* rendering philosophy. To match PI's exact behavior (markdown with arbitrary SGR runs, OSC 8 hyperlinks, Kitty/iTerm2 inline images, IME hardware-cursor APC markers, synchronized-output framing, scrollback-preserving partial repaints that grow the terminal naturally instead of owning an alt-screen), you want PI's `string[]`-per-frame diff. ratatui assumes the alternate screen and a fixed grid; PI deliberately renders into the normal buffer and lets content scroll into scrollback (no alt-screen by default). Fighting ratatui to do that is more work than porting the ~600 lines of `doRender`.

2. **Use `crossterm` as the `Terminal` trait implementation** (the equivalent of `terminal.ts`). crossterm gives you: raw mode, `terminal::size()`, resize events (`crossterm::event` / SIGWINCH), cursor moves, clears, and cross-platform Windows console handling (it already sets `ENABLE_VIRTUAL_TERMINAL_PROCESSING`/`_INPUT`). But **bypass crossterm's `Event`/`KeyEvent` parser** — it does not understand the Kitty keyboard protocol's full CSI-u repertoire, alternate-keys (flag 4), or modifyOtherKeys the way PI needs. Read raw bytes from stdin yourself and port PI's `StdinBuffer` + `keys.ts` decoder. (crossterm has *some* Kitty support via `PushKeyboardEnhancementFlags`, but PI's negotiation + legacy fallbacks + base-layout-key matching are richer; porting `keys.ts` verbatim is the safe path to "matching PI's quality".)

   - Alternative considered: `termwiz` (WezTerm's lib) has the best Kitty-protocol input parsing in the Rust ecosystem and inline-image support. It's a reasonable substitute for the input layer if you don't want to port `keys.ts`. But its rendering model is also surface/cell-based and it's heavier. **Verdict: crossterm for raw I/O + ported PI decoder is the cleanest match.** Keep termwiz in mind only if porting `keys.ts` proves too costly.

3. **Do NOT reach for ratatui widgets** even for the "simple" components. PI's `Text`/`Box`/`SelectList`/`Markdown` all emit raw ANSI line strings that the differential renderer diffs; if half your components produce ratatui `Buffer`s and half produce strings, you have two renderers. Pick one model (PI's) and port every component to it.

### Crate choices (concrete)

- `crossterm` — terminal control (raw mode, size, cursor, clear, Windows). The `Terminal` trait impl.
- **Port, don't import**, the differential renderer (`tui.ts`), the editor, and the layout/width logic.
- `unicode-width` + `unicode-segmentation` — replace `Intl.Segmenter` + `get-east-asian-width` for `visibleWidth`/grapheme iteration/word segmentation. (Note: PI uses `Intl.Segmenter` word granularity for word-nav; `unicode-segmentation`'s `unicode_words`/word-bounds is the analog, but you'll re-tune the punctuation-boundary logic.)
- `vte` (the same parser Alacritty uses) — optional, to replace `stdin-buffer.ts`'s hand-rolled escape-sequence completeness check. Or port `StdinBuffer` directly (it's only ~435 lines and battle-tested for this exact use). **Recommendation: port `StdinBuffer`** — `vte` is a state-machine *parser* that doesn't natively give you "is this sequence complete / wait for more bytes with a 10ms timeout," which is the actual problem PI solves.
- `pulldown-cmark` — replace `marked` for the Markdown component (different token API; the renderer logic in `markdown.ts` must be re-implemented against it).
- `syntect` — syntax highlighting (the `highlightCode` hook). Theme tokens (`syntax*`) map to syntect scopes, or keep a simple custom highlighter.
- `image` + manual base64 — for Kitty/iTerm2 image encoding (`terminal-image.ts`). Inline-image protocols are just base64+escape framing; port directly. (`viuer`/`ratatui-image` exist but couple to other render models.)
- `notify`/manual file watch — for theme hot-reload (matches `docs/themes.md`), app-layer not TUI-layer.

### Component-by-component mapping

| PI component | Rust approach | ratatui equivalent? |
|---|---|---|
| `TUI` differential renderer | **Custom port** of `doRender` (the load-bearing work). | ❌ no equivalent — ratatui's `Terminal::draw`+`Buffer` is a different model. |
| `Terminal`/`ProcessTerminal` | `crossterm` behind a `Terminal` trait. | crossterm (which ratatui also uses as a backend). |
| `StdinBuffer` | Port verbatim (or `vte`). | ❌ |
| `keys.ts` decoder + `keybindings.ts` | Port verbatim; `matchesKey`→`fn matches_key(&str, KeyId)`. Big static tables become `phf`/`match`. | crossterm `KeyEvent` is too lossy — don't use. |
| `utils.ts` width/wrap/slice | Port; back with `unicode-width`/`unicode-segmentation`. | ratatui has its own width but not ANSI-aware slicing. |
| `Text`/`Box`/`Spacer`/`TruncatedText` | Trivial ports — they emit padded ANSI line strings. | ratatui `Paragraph`/`Block` (different model — skip). |
| `SelectList`/`SettingsList` | Port the scrolling+two-column logic; emit strings. | ratatui `List` (different model — skip). |
| `Editor`/`Input` | **Custom port** — biggest component work after the renderer. Port `UndoStack`, `KillRing`, `word-navigation`, paste markers, autocomplete pipeline. | `tui-textarea` exists but won't match PI's emacs kill-ring + paste-markers + autocomplete + IME marker; port PI's. |
| `Markdown` | Custom: re-implement `markdown.ts` against `pulldown-cmark` + theme fns + `syntect`. | ❌ |
| `Loader`/`CancellableLoader` | Trivial port; spinner timer via a render-tick + `tokio`/thread. `AbortController`→`tokio_util::CancellationToken` or `AtomicBool`. | ❌ |
| `Image` + `terminal-image.ts` | Port Kitty/iTerm2 encoders; `image` crate for dimensions. | `ratatui-image` (different model — skip). |
| Overlays | Port the overlay stack + `compositeLineAt` ANSI-aware splice. | ratatui has popups via layered `render` — different model. |
| Theme | App-layer: `theme.fg(token, &str)->String` returning SGR-wrapped strings; pass `Box<dyn Fn(&str)->String>` color fns into components (Rust closures/`Arc<dyn Fn>`). | n/a |

### Rust-specific design notes

- `Component` trait: `fn render(&mut self, width: usize) -> Vec<String>; fn handle_input(&mut self, data: &str) {}; fn wants_key_release(&self) -> bool { false }; fn invalidate(&mut self);`. Components are mutable (caching), so `&mut self` for render. Tree is `Vec<Box<dyn Component>>`.
- `Focusable`: a `focused: bool` field; emit `CURSOR_MARKER` ("\x1b_pi:c\x07") when set. TUI scans/strips it exactly like PI.
- Color functions: `type ColorFn = Arc<dyn Fn(&str) -> String + Send + Sync>;` — direct analog of PI's `(s)=>string`.
- Synchronized output `\x1b[?2026h/l`, scrollback clear `\x1b[3J`, relative cursor moves — all just `write!` to the terminal; no crate needed.
- Width-overflow guard: keep PI's "throw on `visible_width(line) > width`" as a `debug_assert!`/panic-with-crashlog — it catches the most common porting bug class.
- Render scheduling: PI's `process.nextTick`+16ms throttle → a coalescing render request flag + a 16ms timer on the event loop (`tokio` interval or a manual deadline); never render synchronously in `handle_input`.

---

## Prioritized TUI component build checklist (Kivio Code)

Build in this order — each layer depends on the previous:

1. **`Terminal` trait + crossterm impl** — raw mode, size, resize events, write, cursor/clear, bracketed paste enable, OSC 9;4 progress, Kitty-protocol negotiation (`\x1b[>7u\x1b[?u\x1b[c` + response parse → Kitty vs modifyOtherKeys vs legacy), Windows VT input, `drainInput` on exit. *(port `terminal.ts`)*
2. **Width/ANSI utils** — `visible_width`, `extract_ansi_code`, `truncate_to_width`, `slice_by_column`, `slice_with_width`, `extract_segments`, `wrap_text_with_ansi` (+ ANSI state tracker), grapheme/word segmentation, CJK break, east-asian width. *(port `utils.ts`; back with `unicode-width`/`unicode-segmentation`)* — **everything else depends on `visible_width` being exact.**
3. **`StdinBuffer`** — escape-sequence completeness + bracketed-paste extraction + 10ms flush. *(port `stdin-buffer.ts`)*
4. **Key decoder + keybindings** — `matches_key`, Kitty CSI-u + modifyOtherKeys + legacy tables, event-type detection, `Key` builder, `KeybindingsManager` + default table. *(port `keys.ts` + `keybindings.ts`)*
5. **Differential renderer (`TUI`)** — `Component`/`Focusable`/`Container` traits, `do_render` (first/width/height/diff paths, synchronized output, scroll, deleted-line clearing, width-overflow guard), `request_render` throttle, `CURSOR_MARKER` extraction + hardware-cursor positioning, input routing + global listeners. *(port `tui.ts` minus overlays/images first)*
6. **Primitive components** — `Spacer`, `Text`, `Box`, `TruncatedText` (with bg-fn + padding + caching).
7. **`Input`** (single-line) — horizontal scroll, kill-ring, undo, word-nav, bracketed paste, IME marker. *(port `input.ts` + `kill-ring.ts` + `undo-stack.ts` + `word-navigation.ts`)*
8. **`SelectList`** — scrolling window, two-column layout, scroll info, wrap-at-ends. *(port `select-list.ts`)* — needed by the editor's autocomplete dropdown.
9. **Fuzzy + Autocomplete** — `fuzzy_match`/`fuzzy_filter`, `AutocompleteProvider` trait, `CombinedAutocompleteProvider` (slash + `@file` via `fd`/walk). *(port `fuzzy.ts` + `autocomplete.ts`)*
10. **`Editor`** (multi-line) — layout/`word_wrap_line`, vertical scroll, cursor model (sticky col, word/line/page/jump), history, kill-ring, undo, paste markers, border + scroll indicators, autocomplete pipeline (debounced + cancellable + request-id guard), `EditorComponent` trait. *(port `editor.ts` + `editor-component.ts`)* — **largest component.**
11. **`Loader` / `CancellableLoader`** — animated spinner via render tick, abort token.
12. **`Markdown`** — `pulldown-cmark` + `MarkdownTheme` color fns + `syntect` highlight hook + OSC 8 links + code-block fences. *(re-implement `markdown.ts`)*
13. **Overlays** — overlay stack, `OverlayOptions` (anchor/percent/margin/responsive), `composite_line_at` ANSI-aware horizontal splice, focus-restore state machine. *(port `tui.ts:485-1178`)*
14. **`SettingsList`** — toggle/cycle, embedded search `Input`, submenus. *(port `settings-list.ts`)*
15. **Images** — Kitty/iTerm2 encoders, capability detection, cell-size query, image-line diffing in the renderer, `imageFallback`, OSC 8 hyperlinks. *(port `terminal-image.ts` + `image.ts` + the Kitty bits of `tui.ts`)* — **lowest priority; do last** (most terminals/text-first usage don't need it, and it complicates the diff path).
16. **Theme layer (app-side)** — JSON theme → `theme.fg/bg/bold` SGR wrappers + the 51 tokens, color-fn injection into components, `invalidate()` cascade on theme change, hot-reload. *(per `docs/themes.md`)*

---

## Caveats / Not Found

- The `theme` object implementation (the function that turns a theme JSON into `theme.fg(token, text)`) lives in `packages/coding-agent/src/modes/interactive/theme/` (referenced by `docs/themes.md` but not read here) — pi-tui only consumes color *functions*. If you need the exact token→ANSI mapping (256-color fallback, truecolor emission), read that directory.
- `terminal-image.ts` was surveyed by grep (protocols, capability detection, encoders confirmed) but not read line-by-line; the exact Kitty/iTerm2 escape framing should be read directly before porting (`encodeKitty`/`encodeITerm2`/`renderImage` at `terminal-image.ts:160+`).
- `markdown.ts` and the back half of `editor.ts` (lines ~800-2350: deletion/yank/undo/autocomplete internals) were surveyed structurally (function list + key sections) rather than read in full; behavior is well-determined by the documented contracts and the sections read, but exact escaping/edge cases should be cross-checked against the source when porting those two files.
- `unicode-segmentation` word boundaries are *not* identical to `Intl.Segmenter` word granularity for all scripts; PI's word-nav punctuation logic will need re-tuning and its own tests in Rust.
- No existing Rust crate replicates PI's differential **line-string** renderer; this is genuinely custom work and the single biggest risk/effort item.
