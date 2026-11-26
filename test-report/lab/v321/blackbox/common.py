"""Shared helpers for the black-box checks of the 3.2.1 final revision.

Loopback only, realm listen ports 39600-39699, every step has a timeout.
Run the scripts with `python3 -I`.
"""
import os
import re
import signal
import socket
import subprocess
import time

# Results go to WORK/results, scratch files to WORK/tmp. The three binaries are
# v3.2.0, the release build under test and its debug build.
WORK = os.environ.get("BLACKBOX_WORK", os.path.dirname(os.path.abspath(__file__)))
TMP = os.path.join(WORK, "tmp")
BINS = {
    "old": os.environ.get("REALM_OLD", "realm-3.2.0"),
    "new": os.environ.get("REALM_NEW", "realm"),
    "dbg": os.environ.get("REALM_DBG", "realm-debug"),
}

_live = []


def clean_env(extra=None):
    env = {k: v for k, v in os.environ.items() if k != "REALM_CONF"}
    if extra:
        env.update(extra)
    return env


def start(binary, args, out_path, err_path, env=None, prefix=None):
    out = open(out_path, "wb")
    err = open(err_path, "wb")
    cmd = (prefix or []) + [binary] + list(args)
    proc = subprocess.Popen(
        cmd, stdin=subprocess.DEVNULL, stdout=out, stderr=err, env=clean_env(env), cwd=TMP, start_new_session=True
    )
    out.close()
    err.close()
    _live.append(proc)
    return proc


def stop(proc, group=False):
    if proc.poll() is None:
        try:
            if group:
                os.killpg(proc.pid, signal.SIGKILL)
            else:
                proc.kill()
        except ProcessLookupError:
            pass
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        proc.wait(timeout=5)
    if proc in _live:
        _live.remove(proc)
    return proc.returncode


def stop_all():
    for proc in list(_live):
        stop(proc, group=True)


def read(path):
    try:
        with open(path, "rb") as f:
            return f.read().decode("utf-8", "replace")
    except FileNotFoundError:
        return ""


def listeners(pids):
    """{'tcp': [addr...], 'udp': [addr...]} of sockets owned by the given pids."""
    found = {"tcp": set(), "udp": set()}
    try:
        text = subprocess.run(
            ["ss", "-H", "-lntup"], capture_output=True, text=True, timeout=5, stdin=subprocess.DEVNULL
        ).stdout
    except Exception:
        return {"tcp": [], "udp": []}
    for line in text.splitlines():
        owners = {int(x) for x in re.findall(r"pid=(\d+)", line)}
        if not owners & set(pids):
            continue
        cols = line.split()
        if cols[0] in found:
            found[cols[0]].add(cols[4])
    return {k: sorted(v) for k, v in found.items()}


def wait_tcp(port, deadline=5.0, host="127.0.0.1"):
    end = time.monotonic() + deadline
    while time.monotonic() < end:
        try:
            socket.create_connection((host, port), timeout=0.3).close()
            return True
        except OSError:
            time.sleep(0.05)
    return False


def wait_udp_bound(pid, port, deadline=5.0):
    end = time.monotonic() + deadline
    while time.monotonic() < end:
        if any(a.endswith(":%d" % port) for a in listeners([pid])["udp"]):
            return True
        time.sleep(0.05)
    return False


def count_fds(pid, kind="socket"):
    n = 0
    base = "/proc/%d/fd" % pid
    try:
        for name in os.listdir(base):
            try:
                if os.readlink(os.path.join(base, name)).startswith(kind):
                    n += 1
            except OSError:
                pass
    except OSError:
        return -1
    return n


def udp_sockets(pid):
    """Local addresses of every UDP socket of the process (listening or not)."""
    try:
        text = subprocess.run(
            ["ss", "-H", "-anup"], capture_output=True, text=True, timeout=5, stdin=subprocess.DEVNULL
        ).stdout
    except Exception:
        return []
    out = []
    for line in text.splitlines():
        if ("pid=%d," % pid) in line:
            out.append(line.split()[3])
    return sorted(out)
