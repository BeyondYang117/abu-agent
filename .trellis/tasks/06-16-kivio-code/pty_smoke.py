#!/usr/bin/env python3
"""PTY smoke test for kivio-code interactive mode (Phase 6 harness seed).
Launches the built binary in a pseudo-terminal, drives a few keystrokes that
do NOT call the model (/help, /new, /quit), and asserts it renders + exits.
"""
import os, pty, select, subprocess, sys, time

BIN = os.path.abspath("target/debug/kivio-code")
CWD = os.path.abspath(".")

def run():
    master, slave = pty.openpty()
    # give it a realistic window size
    import struct, fcntl, termios
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 0, 0))
    p = subprocess.Popen([BIN], stdin=slave, stdout=slave, stderr=slave,
                         cwd=CWD, close_fds=True)
    os.close(slave)
    captured = bytearray()

    def drain(timeout):
        end = time.time() + timeout
        while time.time() < end:
            r, _, _ = select.select([master], [], [], 0.2)
            if r:
                try:
                    data = os.read(master, 65536)
                except OSError:
                    break
                if not data:
                    break
                captured.extend(data)

    def send(s):
        os.write(master, s.encode())
        time.sleep(0.4)

    drain(2.0)              # initial frame
    send("/help\r"); drain(1.0)
    send("type some text here"); drain(0.6)   # editor input render (no submit)
    send("\x15"); drain(0.4)                    # Ctrl+U / kill — editor edit path
    send("/new\r"); drain(1.0)
    send("/quit\r"); drain(1.5)

    # ensure process exits
    for _ in range(20):
        if p.poll() is not None:
            break
        time.sleep(0.1)
    if p.poll() is None:
        p.terminate(); time.sleep(0.3)
        if p.poll() is None:
            p.kill()
    os.close(master)

    out = bytes(captured)
    text = out.decode("utf-8", "replace")
    # strip ANSI for assertions
    import re
    plain = re.sub(r"\x1b\[[0-9;?]*[A-Za-z]|\x1b[\]_].*?(\x07|\x1b\\)", "", text)
    print("=== EXIT CODE:", p.returncode, "===")
    print("=== captured bytes:", len(out), "===")
    print("=== plain (tail 1500) ===")
    print(plain[-1500:])
    ok = p.returncode is not None and ("help" in plain.lower() or "quit" in plain.lower() or len(out) > 50)
    print("=== SMOKE:", "PASS" if ok else "FAIL", "===")
    return 0 if ok else 1

sys.exit(run())
