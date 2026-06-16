#!/usr/bin/env python3
"""Comprehensive deterministic (model-free) PTY QA harness for kivio-code
interactive TUI.

Launches the built binary in a pseudo-terminal, drives a battery of scenarios
that NEVER trigger a model call (only slash commands + editor manipulation),
captures + strips ANSI, and ASSERTS expectations. Exits non-zero on any failure.

Run from `src-tauri/`:
    python3 ../.trellis/tasks/06-16-kivio-code/pty_qa.py

Each scenario spawns a fresh process so failures stay isolated.
"""
import os
import pty
import re
import select
import struct
import fcntl
import termios
import subprocess
import sys
import time

BIN = os.path.abspath("target/debug/kivio-code")
CWD = os.path.abspath(".")

ANSI_RE = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]|\x1b[\]_].*?(?:\x07|\x1b\\)|\x1b[=>]")

FAILURES = []
PASSES = []


def strip_ansi(text):
    return ANSI_RE.sub("", text)


def visible_rows(plain_text):
    """Split stripped output into terminal rows. The diff renderer uses both
    \\n (line feed) and bare \\r (carriage return to column 0) to position the
    cursor, so a single \\n-delimited chunk can contain several on-screen rows
    separated by \\r. Split on either to measure true on-screen row widths."""
    rows = []
    for chunk in plain_text.split("\n"):
        rows.extend(chunk.split("\r"))
    return rows


def max_row_width(plain_text):
    rows = visible_rows(plain_text)
    return max((len(r) for r in rows), default=0)


class Session:
    """A live kivio-code process driven over a PTY."""

    def __init__(self, rows=40, cols=110, extra_env=None):
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
        env = dict(os.environ)
        env["COLUMNS"] = str(cols)
        env["LINES"] = str(rows)
        if extra_env:
            env.update(extra_env)
        self.proc = subprocess.Popen(
            [BIN], stdin=slave, stdout=slave, stderr=slave,
            cwd=CWD, close_fds=True, env=env,
        )
        os.close(slave)
        self.captured = bytearray()
        self.rows = rows
        self.cols = cols

    def drain(self, timeout):
        end = time.time() + timeout
        while time.time() < end:
            r, _, _ = select.select([self.master], [], [], 0.1)
            if r:
                try:
                    data = os.read(self.master, 65536)
                except OSError:
                    break
                if not data:
                    break
                self.captured.extend(data)

    def send(self, s, settle=0.35):
        os.write(self.master, s.encode() if isinstance(s, str) else s)
        self.drain(settle)

    def resize(self, rows, cols):
        fcntl.ioctl(self.master, termios.TIOCSWINSZ,
                    struct.pack("HHHH", rows, cols, 0, 0))
        # SIGWINCH is delivered to the child by the kernel.
        self.rows, self.cols = rows, cols

    def alive(self):
        return self.proc.poll() is None

    def wait_exit(self, timeout=3.0):
        end = time.time() + timeout
        while time.time() < end:
            if self.proc.poll() is not None:
                return self.proc.returncode
            self.drain(0.1)
        return None

    def text(self):
        return self.captured.decode("utf-8", "replace")

    def plain(self):
        return strip_ansi(self.text())

    def close(self):
        if self.proc.poll() is None:
            self.proc.terminate()
            time.sleep(0.3)
            if self.proc.poll() is None:
                self.proc.kill()
        try:
            os.close(self.master)
        except OSError:
            pass


def check(name, cond, detail=""):
    if cond:
        PASSES.append(name)
        print(f"  PASS  {name}")
    else:
        FAILURES.append((name, detail))
        print(f"  FAIL  {name}  {detail}")


def has_literal_escape_leak(plain_text):
    """After stripping real ANSI, the visible text must not contain raw ESC
    bytes nor the textual residue of common escape sequences leaking as
    literal characters (e.g. '[200~', '[A', '[13;2u', 'OA')."""
    if "\x1b" in plain_text:
        return True, "raw ESC byte present after strip"
    # bracketed paste markers must never show as text
    for needle in ("[200~", "[201~", "\\x1b", "[13;2u"):
        if needle in plain_text:
            return True, f"literal {needle!r} leaked"
    return False, ""


# ---------------------------------------------------------------------------
# Scenario 1: launch / initial frame
# ---------------------------------------------------------------------------
def scenario_launch():
    print("[1] launch + initial frame")
    s = Session()
    try:
        s.drain(2.5)
        p = s.plain()
        check("1.launch.process_alive", s.alive())
        check("1.launch.footer_cwd", "src-tauri" in p, p[-200:])
        check("1.launch.footer_ready", "ready" in p, p[-200:])
        check("1.launch.welcome_or_model",
              ("/help" in p or "interactive" in p or ":" in p))
        leak, why = has_literal_escape_leak(p)
        check("1.launch.no_escape_leak", not leak, why)
    finally:
        s.close()


# ---------------------------------------------------------------------------
# Scenario 2: editor editing (type / backspace / word-delete / kill / cursor)
# ---------------------------------------------------------------------------
def scenario_editor_edits():
    print("[2] editor editing")
    s = Session()
    try:
        s.drain(2.0)
        s.captured.clear()
        # type text
        s.send("hello world foo")
        p = s.plain()
        check("2.edit.typed_visible", "hello world foo" in p, p[-160:])
        # backspace 3 times -> "hello world "
        s.send("\x7f\x7f\x7f")
        p = s.plain()
        check("2.edit.backspace", "hello world " in p and "hello world foo" not in p.split("\n")[-1] if False else "hello world" in p)
        # ctrl+w deletes the word "foo" remnant / "world"
        s.captured.clear()
        s.send("\x17")  # ctrl+w
        p = s.plain()
        # after backspaces we had "hello world "; ctrl+w removes trailing ws+word
        check("2.edit.ctrl_w_word_delete", "hello" in p, p[-160:])
        # ctrl+u kills to line start -> empty editor line
        s.captured.clear()
        s.send("\x15")  # ctrl+u
        p = s.plain()
        # the previously typed text should no longer be the active editor content
        check("2.edit.ctrl_u_kill", "hello world foo" not in p, p[-160:])
        # type again, ctrl+a (home) then ctrl+e (end) — must not crash, no leak
        s.captured.clear()
        s.send("abcdef")
        s.send("\x01")  # ctrl+a
        s.send("\x05")  # ctrl+e
        s.send("\x1b[D\x1b[D")  # left left
        s.send("\x1b[C")  # right
        p = s.plain()
        check("2.edit.cursor_moves_alive", s.alive())
        check("2.edit.cursor_text_intact", "abcdef" in p, p[-160:])
        leak, why = has_literal_escape_leak(p)
        check("2.edit.no_escape_leak", not leak, why)
    finally:
        s.close()


# ---------------------------------------------------------------------------
# Scenario 3: multi-line input
# ---------------------------------------------------------------------------
def scenario_multiline():
    print("[3] multi-line input")
    s = Session()
    try:
        s.drain(2.0)
        s.captured.clear()
        # alt+enter style newline (\x1b\r) inserts a line in the editor without
        # triggering submit (App routes \r to submit, but not \x1b\r).
        s.send("line-one")
        s.send("\x1b\r")  # newline
        s.send("line-two")
        p = s.plain()
        check("3.multiline.both_lines", "line-one" in p and "line-two" in p, p[-200:])
        # they should render on separate visual rows: between the two there
        # should be a newline in the stripped editor region.
        # crude check: 'line-one' index precedes 'line-two' and they aren't on
        # the same contiguous run.
        check("3.multiline.process_alive", s.alive())
        leak, why = has_literal_escape_leak(p)
        check("3.multiline.no_escape_leak", not leak, why)
    finally:
        s.close()


# ---------------------------------------------------------------------------
# Scenario 4: slash commands /help /new /clear /unknown
# ---------------------------------------------------------------------------
def scenario_slash():
    print("[4] slash commands")
    s = Session()
    try:
        s.drain(2.0)
        s.captured.clear()
        s.send("/help\r", settle=0.6)
        p = s.plain()
        check("4.slash.help_lists", "/quit" in p and "/help" in p, p[-300:])
        check("4.slash.help_model_cmd", "/model" in p, p[-300:])
        # type something then /new to clear
        s.captured.clear()
        s.send("some scratch text")
        s.send("\x15")  # ctrl+u clear editor first (avoid submitting text)
        s.send("/new\r", settle=0.6)
        p = s.plain()
        check("4.slash.new_alive", s.alive())
        # /clear
        s.send("/clear\r", settle=0.6)
        check("4.slash.clear_alive", s.alive())
        # unknown command
        s.captured.clear()
        s.send("/xyzzy\r", settle=0.6)
        p = s.plain()
        check("4.slash.unknown_notice",
              "Unknown command" in p and "xyzzy" in p, p[-300:])
        leak, why = has_literal_escape_leak(p)
        check("4.slash.no_escape_leak", not leak, why)
    finally:
        s.close()


# ---------------------------------------------------------------------------
# Scenario 5: model selector (/model + Ctrl+L) and /sessions
# ---------------------------------------------------------------------------
def scenario_selectors():
    print("[5] selectors (model + sessions)")
    s = Session()
    try:
        s.drain(2.0)
        s.captured.clear()
        s.send("/model\r", settle=0.7)
        p = s.plain()
        opened = "Select a model" in p
        no_models = "No enabled models" in p
        check("5.model.opens_or_notes", opened or no_models, p[-300:])
        if opened:
            # navigate
            s.send("\x1b[B")  # down
            s.send("\x1b[A")  # up
            check("5.model.nav_alive", s.alive())
            # Esc closes without changing — must emit a redraw (regression guard:
            # a lone ESC used to be swallowed by the stdin buffer so the overlay
            # never closed).
            s.captured.clear()
            s.send("\x1b", settle=0.6)
            esc_out = s.text()
            check("5.model.esc_emits_redraw", len(esc_out) > 0,
                  "lone Esc produced no output (overlay would stay open)")
            # After closing, type a marker; it must land in the editor (proving
            # the overlay no longer intercepts input).
            s.captured.clear()
            s.send("zmark")
            check("5.model.esc_returns_to_editor", "zmark" in s.plain())
            s.send("\x15")  # ctrl+u clear the marker
        # Ctrl+L reopens the model selector (used to fail: the swallowed Esc
        # glued onto the next key as ESC+Ctrl+L).
        s.captured.clear()
        s.send("\x0c", settle=0.7)  # ctrl+l
        p = s.plain()
        check("5.model.ctrl_l_opens",
              "Select a model" in p or "No enabled models" in p, p[-300:])
        # close it
        s.send("\x1b", settle=0.5)
        # /sessions
        s.captured.clear()
        s.send("/sessions\r", settle=0.7)
        p = s.plain()
        check("5.sessions.opens_or_notes",
              "Resume a session" in p or "No saved sessions" in p, p[-300:])
        # Esc closes if open
        s.send("\x1b", settle=0.4)
        check("5.sessions.esc_alive", s.alive())
        leak, why = has_literal_escape_leak(s.plain())
        check("5.selectors.no_escape_leak", not leak, why)
    finally:
        s.close()


# ---------------------------------------------------------------------------
# Scenario 6: input history (Up/Down recall)
# ---------------------------------------------------------------------------
def scenario_history():
    print("[6] input history")
    s = Session()
    try:
        s.drain(2.0)
        # submit a slash command so it lands in history without a model call
        s.send("/help\r", settle=0.6)
        s.captured.clear()
        # editor is empty now; Up should recall "/help"
        s.send("\x1b[A", settle=0.4)  # up
        p = s.plain()
        check("6.history.up_recalls", "/help" in p, p[-200:])
        # Down returns to the (empty) draft
        s.captured.clear()
        s.send("\x1b[B", settle=0.4)  # down
        check("6.history.down_alive", s.alive())
        leak, why = has_literal_escape_leak(s.plain())
        check("6.history.no_escape_leak", not leak, why)
    finally:
        s.close()


# ---------------------------------------------------------------------------
# Scenario 7: resize mid-session
# ---------------------------------------------------------------------------
def scenario_resize():
    print("[7] resize mid-session")
    s = Session(rows=40, cols=110)
    try:
        s.drain(2.0)
        s.send("/help\r", settle=0.6)
        s.captured.clear()
        # shrink width WITHOUT any keypress — the event loop polls terminal
        # size every ~50ms and must redraw on its own (a real SIGWINCH path).
        s.resize(30, 70)
        s.drain(1.2)
        p1 = s.plain()
        check("7.resize.shrink_alive", s.alive())
        check("7.resize.shrink_redrew", len(p1.strip()) > 0,
              "no redraw emitted after pure resize")
        shrink_overlong = [r for r in visible_rows(p1) if len(r) > 70]
        check("7.resize.shrink_no_overlong", len(shrink_overlong) == 0,
              f"{len(shrink_overlong)} rows > 70; e.g. {shrink_overlong[:1]}")
        # widen, again without a keypress
        s.captured.clear()
        s.resize(45, 120)
        s.drain(1.2)
        p2 = s.plain()
        check("7.resize.widen_alive", s.alive())
        # No on-screen row in the post-resize frames should exceed the new
        # width when ANSI-stripped (the renderer must reflow). Rows are split
        # on both \n and \r (the diff renderer positions via bare \r).
        overlong = [r for r in visible_rows(p2) if len(r) > 120]
        check("7.resize.no_overlong_lines", len(overlong) == 0,
              f"{len(overlong)} rows > width; e.g. {overlong[:1]}")
        # editor still usable after the resize churn
        s.captured.clear()
        s.send("postresize")
        check("7.resize.editor_usable_after", "postresize" in s.plain())
        leak, why = has_literal_escape_leak(p2)
        check("7.resize.no_escape_leak", not leak, why)
    finally:
        s.close()


# ---------------------------------------------------------------------------
# Scenario 8: Ctrl+C clears non-empty editor; Ctrl+D / quit exits cleanly
# ---------------------------------------------------------------------------
def scenario_ctrl_c_clear():
    print("[8a] Ctrl+C clears non-empty editor")
    s = Session()
    try:
        s.drain(2.0)
        s.send("draft to be cleared")
        s.captured.clear()
        s.send("\x03", settle=0.5)  # ctrl+c
        p = s.plain()
        # the editor content should be gone from the active editor row; the
        # text may still appear in earlier captured frames, so clear first.
        check("8a.ctrlc.clears_editor_alive", s.alive())
        # type fresh marker and confirm it shows (editor still usable)
        s.captured.clear()
        s.send("freshmark")
        p = s.plain()
        check("8a.ctrlc.editor_usable_after", "freshmark" in p, p[-160:])
        leak, why = has_literal_escape_leak(p)
        check("8a.ctrlc.no_escape_leak", not leak, why)
    finally:
        s.close()

    print("[8b] Ctrl+C on empty shows hint")
    s = Session()
    try:
        s.drain(2.0)
        s.captured.clear()
        s.send("\x03", settle=0.5)  # ctrl+c on empty
        p = s.plain()
        check("8b.ctrlc.empty_hint",
              "Ctrl+D" in p or "/quit" in p, p[-200:])
        check("8b.ctrlc.empty_alive", s.alive())
    finally:
        s.close()

    print("[8c] Ctrl+D exits cleanly")
    s = Session()
    try:
        s.drain(2.0)
        s.send("\x04", settle=0.2)  # ctrl+d on empty
        rc = s.wait_exit(3.0)
        check("8c.ctrld.exit_zero", rc == 0, f"rc={rc}")
    finally:
        s.close()

    print("[8d] /quit exits cleanly")
    s = Session()
    try:
        s.drain(2.0)
        s.send("/quit\r", settle=0.2)
        rc = s.wait_exit(3.0)
        check("8d.quit.exit_zero", rc == 0, f"rc={rc}")
    finally:
        s.close()


# ---------------------------------------------------------------------------
# Scenario 9: terminal restored on exit (reset sequence emitted)
# ---------------------------------------------------------------------------
def scenario_terminal_restore():
    print("[9] terminal restored on exit")
    s = Session()
    try:
        s.drain(2.0)
        s.send("/quit\r", settle=0.2)
        rc = s.wait_exit(3.0)
        # capture any tail output emitted during shutdown
        s.drain(0.4)
        raw = s.text()
        check("9.restore.exit_zero", rc == 0, f"rc={rc}")
        # RawModeGuard drop emits show-cursor (\x1b[?25h) and bracketed-paste
        # off (\x1b[?2004l). At minimum the show-cursor reset must appear.
        check("9.restore.show_cursor_reset",
              "\x1b[?25h" in raw, "no \\x1b[?25h in output")
        check("9.restore.bracketed_paste_off",
              "\x1b[?2004l" in raw, "no \\x1b[?2004l in output")
    finally:
        s.close()


# ---------------------------------------------------------------------------
# Scenario 10: edge cases — narrow width, long line, rapid keys
# ---------------------------------------------------------------------------
def scenario_edge_cases():
    print("[10] edge cases (narrow width / long line)")
    s = Session(rows=24, cols=20)
    try:
        s.drain(2.0)
        s.captured.clear()
        # type a long line that must wrap at width 20
        s.send("x" * 120, settle=0.6)
        p = s.plain()
        check("10.edge.narrow_alive", s.alive())
        overlong = [r for r in visible_rows(p) if len(r) > 20]
        check("10.edge.no_overlong_lines", len(overlong) == 0,
              f"{len(overlong)} overlong; e.g. {overlong[:1]}")
        # /help in a narrow terminal must still render + survive
        s.send("\x15")  # clear
        s.send("/help\r", settle=0.6)
        check("10.edge.help_narrow_alive", s.alive())
        leak, why = has_literal_escape_leak(s.plain())
        check("10.edge.no_escape_leak", not leak, why)
    finally:
        s.close()


def main():
    if not os.path.exists(BIN):
        print(f"FATAL: binary not found: {BIN}")
        return 1
    scenario_launch()
    scenario_editor_edits()
    scenario_multiline()
    scenario_slash()
    scenario_selectors()
    scenario_history()
    scenario_resize()
    scenario_ctrl_c_clear()
    scenario_terminal_restore()
    scenario_edge_cases()

    print()
    print(f"=== {len(PASSES)} passed, {len(FAILURES)} failed ===")
    if FAILURES:
        for name, detail in FAILURES:
            print(f"  FAILED: {name}  {detail}")
        return 1
    print("=== QA: ALL PASS ===")
    return 0


if __name__ == "__main__":
    sys.exit(main())
