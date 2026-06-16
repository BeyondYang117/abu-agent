#!/usr/bin/env python3
"""Live PTY test: drive a real agent turn in interactive mode (one model call).
Sends a tool-using prompt, waits for streaming + a tool card, then /quit.
"""
import os, pty, select, subprocess, sys, time, re

BIN = os.path.abspath("target/debug/kivio-code")
CWD = os.path.abspath(".")

def main():
    master, slave = pty.openpty()
    import struct, fcntl, termios
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 110, 0, 0))
    p = subprocess.Popen([BIN], stdin=slave, stdout=slave, stderr=slave, cwd=CWD, close_fds=True)
    os.close(slave)
    cap = bytearray()
    def drain(t):
        end = time.time() + t
        while time.time() < end:
            r,_,_ = select.select([master], [], [], 0.3)
            if r:
                try: d = os.read(master, 65536)
                except OSError: break
                if not d: break
                cap.extend(d)
    def send(s):
        os.write(master, s.encode()); time.sleep(0.5)
    drain(2.0)
    send("List the .rs files directly under the kivio_code directory using a tool, then say done."); drain(0.5)
    send("\r")
    drain(75.0)   # allow model + tool round
    send("/quit\r"); drain(2.0)
    for _ in range(30):
        if p.poll() is not None: break
        time.sleep(0.1)
    if p.poll() is None:
        p.terminate(); time.sleep(0.5)
        if p.poll() is None: p.kill()
    os.close(master)
    text = bytes(cap).decode("utf-8","replace")
    plain = re.sub(r"\x1b\[[0-9;?]*[A-Za-z]|\x1b[\]_].*?(\x07|\x1b\\)", "", text)
    print("=== EXIT:", p.returncode, " bytes:", len(cap), "===")
    print("=== plain (tail 2500) ===")
    print(plain[-2500:])
    low = plain.lower()
    tool_seen = any(k in low for k in ["read", "ls", "grep", "find", "list_dir", "search", "✓", "running", "tool"])
    print("=== tool activity seen:", tool_seen, "| exited:", p.returncode is not None, "===")
    return 0

sys.exit(main())
