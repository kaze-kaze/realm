#!/usr/bin/env python3
"""RF-01: really forward through a relay configured only with positional arrays.

Top level, `network` and the endpoint are all arrays; `network` carries
use_udp (slot 2) and udp_timeout (slot 13).  One TCP and one UDP round trip
per binary and per entry (-c file, -c dir, REALM_CONF).
usage: python3 -I rf01_forward.py   -> results/rf01-forward.json
"""
import json
import os
import shutil
import socket
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import common as c

PORT = 39610


def echo_servers():
    for _ in range(50):
        t = socket.socket()
        t.bind(("127.0.0.1", 0))
        port = t.getsockname()[1]
        u = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        try:
            u.bind(("127.0.0.1", port))
        except OSError:
            t.close()
            continue
        t.listen(16)
        return t, u, port
    raise RuntimeError("no port")


def serve_tcp(t):
    while True:
        try:
            conn, _ = t.accept()
        except OSError:
            return
        conn.settimeout(3)
        try:
            data = conn.recv(4096)
            conn.sendall(b"tcp-echo:" + data)
        except OSError:
            pass
        conn.close()


def serve_udp(u):
    while True:
        try:
            data, peer = u.recvfrom(4096)
        except OSError:
            return
        u.sendto(b"udp-echo:" + data, peer)


def main():
    os.makedirs(c.TMP, exist_ok=True)
    t, u, port = echo_servers()
    threading.Thread(target=serve_tcp, args=(t,), daemon=True).start()
    threading.Thread(target=serve_udp, args=(u,), daemon=True).start()
    conf = json.dumps([[], [], [False, True] + [None] * 10 + [17, None], [["127.0.0.1:%d" % PORT, "127.0.0.1:%d" % port]]])
    d = os.path.join(c.TMP, "rf01-forward")
    out = {"config": conf, "runs": []}
    for which in ("old", "new", "dbg"):
        for mode in ("file", "dir", "env"):
            shutil.rmtree(d, ignore_errors=True)
            os.makedirs(d)
            path = os.path.join(d, "positional.json")
            with open(path, "w") as f:
                f.write(conf)
            args, env = (["-c", path], None) if mode == "file" else (["-c", d], None) if mode == "dir" else ([], {"REALM_CONF": conf})
            out_p, err_p = os.path.join(c.TMP, "fwd.out"), os.path.join(c.TMP, "fwd.err")
            row = {"binary": which, "entry": mode}
            for attempt in range(6):
                proc = c.start(c.BINS[which], args, out_p, err_p, env=env)
                try:
                    ready = c.wait_tcp(PORT, 6) and c.wait_udp_bound(proc.pid, PORT, 6)
                    row["ready"] = ready
                    if ready:
                        with socket.create_connection(("127.0.0.1", PORT), timeout=3) as s:
                            s.settimeout(3)
                            s.sendall(b"hello-tcp")
                            row["tcp_reply"] = s.recv(100).decode()
                        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
                            s.settimeout(3)
                            s.sendto(b"hello-udp", ("127.0.0.1", PORT))
                            row["udp_reply"] = s.recvfrom(100)[0].decode()
                except OSError as e:
                    row["error"] = repr(e)
                finally:
                    c.stop(proc)
                row["inited"] = [l for l in c.read(out_p).splitlines() if l.startswith("inited")]
                row["stderr"] = c.read(err_p)[-300:]
                if "already in use" not in row["stderr"]:
                    break
                time.sleep(3)
            out["runs"].append(row)
            print(which, mode, row.get("tcp_reply"), row.get("udp_reply"), row.get("error", ""), row["inited"][0][-150:] if row["inited"] else row["stderr"], flush=True)
    shutil.rmtree(d, ignore_errors=True)
    for p in ("fwd.out", "fwd.err"):
        if os.path.exists(os.path.join(c.TMP, p)):
            os.remove(os.path.join(c.TMP, p))
    t.close()
    u.close()
    with open(os.path.join(c.WORK, "results", "rf01-forward.json"), "w") as f:
        json.dump(out, f, indent=1)


if __name__ == "__main__":
    try:
        main()
    finally:
        c.stop_all()
