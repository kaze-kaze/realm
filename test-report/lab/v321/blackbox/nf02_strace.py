#!/usr/bin/env python3
"""NF-02: count fcntl(F_GETPIPE_SZ) in realm over N short TCP connections.

realm is started under `strace -f -e trace=fcntl` (ptrace only reaches our own
children).  usage: python3 -I nf02_strace.py [connections=200]
writes results/nf02.json
"""
import json
import os
import re
import socket
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import common as c

PORT = 39640


class Echo(threading.Thread):
    def __init__(self):
        super().__init__(daemon=True)
        self.sock = socket.socket()
        self.sock.bind(("127.0.0.1", 0))
        self.sock.listen(256)
        self.sock.settimeout(0.2)
        self.port = self.sock.getsockname()[1]
        self.stop = False
        self.served = 0

    def run(self):
        while not self.stop:
            try:
                conn, _ = self.sock.accept()
            except socket.timeout:
                continue
            except OSError:
                return
            conn.settimeout(5)
            try:
                data = conn.recv(4096)
                if data:
                    conn.sendall(data)
                    self.served += 1
            except OSError:
                pass
            finally:
                conn.close()


def run(which, n, echo):
    conf = os.path.join(c.TMP, "nf02-%s.toml" % which)
    trace = os.path.join(c.TMP, "nf02-%s.strace" % which)
    with open(conf, "w") as f:
        f.write('[[endpoints]]\nlisten = "127.0.0.1:%d"\nremote = "127.0.0.1:%d"\n' % (PORT, echo.port))
    out_p, err_p = os.path.join(c.TMP, "nf02-%s.out" % which), os.path.join(c.TMP, "nf02-%s.err" % which)
    proc = c.start(c.BINS[which], ["-c", conf], out_p, err_p,
                   prefix=["strace", "-f", "-e", "trace=fcntl", "-o", trace])
    res = {"connections": n}
    try:
        if not c.wait_tcp(PORT, 15):
            res["error"] = "not listening: rc=%r %s" % (proc.poll(), c.read(err_p)[-400:])
            return res
        time.sleep(0.5)
        before = echo.served
        # the readiness probe above was one relayed connection (it carried no data)
        ok = 0
        for i in range(n):
            try:
                with socket.create_connection(("127.0.0.1", PORT), timeout=5) as s:
                    s.settimeout(5)
                    msg = b"ping-%d" % i
                    s.sendall(msg)
                    if s.recv(100) == msg:
                        ok += 1
            except OSError as e:
                res.setdefault("errors", []).append(repr(e))
        time.sleep(1.0)
        res["echoed"] = ok
        res["upstream_served"] = echo.served - before
        res["alive_at_end"] = proc.poll() is None
    finally:
        c.stop(proc, group=True)
        text = c.read(trace)
        cmds = {}
        for m in re.finditer(r"fcntl\(\d+, ([A-Z_0-9]+)", text):
            cmds[m.group(1)] = cmds.get(m.group(1), 0) + 1
        res["fcntl_by_command"] = cmds
        res["F_GETPIPE_SZ"] = cmds.get("F_GETPIPE_SZ", 0)
        res["F_GETPIPE_SZ_lines"] = [l for l in text.splitlines() if "F_GETPIPE_SZ" in l][:12]
        res["stderr"] = c.read(err_p)[-600:]
        for p in (conf, trace, out_p, err_p):
            if os.path.exists(p):
                os.remove(p)
    return res


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 200
    os.makedirs(c.TMP, exist_ok=True)
    echo = Echo()
    echo.start()
    results = {}
    for which in ("old", "new", "dbg"):
        results[which] = run(which, n, echo)
        r = results[which]
        print(which, "echoed=%s F_GETPIPE_SZ=%s fcntl=%s %s" % (
            r.get("echoed"), r.get("F_GETPIPE_SZ"), r.get("fcntl_by_command"), r.get("error", "")), flush=True)
    echo.stop = True
    with open(os.path.join(c.WORK, "results", "nf02.json"), "w") as f:
        json.dump(results, f, indent=1)


if __name__ == "__main__":
    try:
        main()
    finally:
        c.stop_all()
