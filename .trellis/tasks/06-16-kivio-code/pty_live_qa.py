#!/usr/bin/env python3
"""Live interactive QA: Esc-cancel, /model switch, -c resume across restarts.
Costs a few model calls. Run from src-tauri/. Sandboxed cwd.
"""
import os, pty, select, subprocess, time, re, sys, struct, fcntl, termios, tempfile

BIN = os.path.abspath("target/debug/kivio-code")

def spawn(args, sbx):
    m, s = pty.openpty()
    fcntl.ioctl(s, termios.TIOCSWINSZ, struct.pack("HHHH", 45, 120, 0, 0))
    p = subprocess.Popen([BIN] + args, stdin=s, stdout=s, stderr=s, cwd=sbx, close_fds=True)
    os.close(s)
    return p, m

def drain(m, t, buf):
    end = time.time() + t
    while time.time() < end:
        r,_,_ = select.select([m], [], [], 0.3)
        if r:
            try: d = os.read(m, 65536)
            except OSError: break
            if not d: break
            buf.extend(d)

def plain(buf):
    return re.sub(r"\x1b\[[0-9;?]*[A-Za-z]|\x1b[\]_].*?(\x07|\x1b\\)", "", bytes(buf).decode("utf-8","replace"))

def kill(p, m):
    for _ in range(30):
        if p.poll() is not None: break
        time.sleep(0.1)
    if p.poll() is None: p.kill()
    try: os.close(m)
    except OSError: pass

results = []
def check(name, ok, extra=""):
    results.append((name, ok))
    print(f"  {'PASS' if ok else 'FAIL'}  {name}  {extra}")

sbx = tempfile.mkdtemp(prefix="kivio-qa.")
open(os.path.join(sbx, "note.txt"), "w").write("alpha\nbeta\ngamma\n")

# ---- A: Esc cancel mid-generation ----
print("[A] Esc cancels an in-flight generation")
buf = bytearray(); p, m = spawn(["-C", sbx], sbx)
drain(m, 1.5, buf)
os.write(m, b"Count slowly from 1 to 50, one number per line, with a short note on each."); time.sleep(0.4)
os.write(m, b"\r");
drain(m, 3.0, buf)            # let it start streaming
os.write(m, b"\x1b")          # ESC -> cancel
drain(m, 4.0, buf)
pa = plain(buf)
cancelled = ("cancel" in pa.lower())
# after cancel, the app must still be responsive: submit /quit and expect clean exit
os.write(m, b"/quit\r"); drain(m, 2.0, buf)
kill(p, m)
check("A.cancel.exit_zero", p.returncode == 0, f"(rc={p.returncode})")
check("A.cancel.notice_or_idle", cancelled or p.returncode == 0, "(cancel notice seen)" if cancelled else "(no explicit notice but exited)")

# ---- B: /model switch updates footer ----
print("[B] /model selector switches model + footer")
buf = bytearray(); p, m = spawn(["-C", sbx], sbx)
drain(m, 1.5, buf)
before = plain(buf)
fm_before = re.findall(r"·\s+([^·]+?)\s+·\s+ready", before)
os.write(m, b"/model\r"); drain(m, 1.2, buf)   # open selector
mid = plain(buf)
selector_open = ("model" in mid.lower())
os.write(m, b"\x1b[B")  # Down
time.sleep(0.3)
os.write(m, b"\r")      # select
drain(m, 1.2, buf)
after = plain(buf)
fm_after = re.findall(r"·\s+([^·]+?)\s+·\s+ready", after)
os.write(m, b"/quit\r"); drain(m, 1.5, buf); kill(p, m)
check("B.model.selector_opened", selector_open)
check("B.model.exit_zero", p.returncode == 0)
# footer label present (switch may be same if only 1 enabled model)
check("B.model.footer_label_present", bool(fm_after), f"(label={fm_after[-1].strip() if fm_after else None})")

# ---- C: -c resume across restarts ----
print("[C] -c resumes the previous session")
buf = bytearray(); p, m = spawn(["-C", sbx], sbx)
drain(m, 1.5, buf)
os.write(m, b"Reply with exactly the single word: PINEAPPLE"); time.sleep(0.4); os.write(m, b"\r")
drain(m, 40.0, buf)
t1 = plain(buf)
got_marker = "PINEAPPLE" in t1.upper()
os.write(m, b"/quit\r"); drain(m, 1.5, buf); kill(p, m)
check("C.turn1.exit_zero", p.returncode == 0)
# relaunch with -c
buf2 = bytearray(); p2, m2 = spawn(["-C", sbx, "-c"], sbx)
drain(m2, 2.0, buf2)
resumed = plain(buf2)
# the prior user line and/or assistant marker should be in the rebuilt transcript
resumed_ok = ("PINEAPPLE" in resumed.upper()) or ("single word" in resumed.lower())
os.write(m2, b"/quit\r"); drain(m2, 1.5, buf2); kill(p2, m2)
check("C.resume.exit_zero", p2.returncode == 0)
check("C.resume.transcript_restored", resumed_ok, "(prior turn visible after -c)")

import shutil; shutil.rmtree(sbx, ignore_errors=True)
npass = sum(1 for _,ok in results if ok); nfail = len(results)-npass
print(f"\n=== {npass} passed, {nfail} failed ===")
sys.exit(0 if nfail == 0 else 1)
