#!/usr/bin/env python3
"""RF-02 side check: the failover path of the UDP relay still moves an existing
association to the backup and back (this revision rewrote the route selection).

Two exact echo services (so the default `echo` probe passes) on 127.0.0.1.
One client socket sends all the time; the primary's UDP echo is taken away and
brought back.  The upstreams record the relay source port they see.
usage: python3 -I rf02b_failover.py   -> results/rf02b-failover.json
"""
import json
import os
import socket
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import common as c

PORT = 39625


class Echo:
    def __init__(self, name):
        self.name = name
        self.tcp = socket.socket()
        self.tcp.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.tcp.bind(("127.0.0.1", 0))
        self.port = self.tcp.getsockname()[1]
        self.tcp.listen(64)
        self.tcp.settimeout(0.2)
        self.udp = None
        self.data_count = 0
        self.ports = []
        self.stop = False
        self.open_udp()
        threading.Thread(target=self.run_tcp, daemon=True).start()
        threading.Thread(target=self.run_udp, daemon=True).start()

    def open_udp(self):
        u = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        u.bind(("127.0.0.1", self.port))
        u.settimeout(0.1)
        self.udp = u

    def close_udp(self):
        u, self.udp = self.udp, None
        u.close()

    def run_udp(self):
        while not self.stop:
            u = self.udp
            if u is None:
                time.sleep(0.05)
                continue
            try:
                data, peer = u.recvfrom(65536)
                u.sendto(data, peer)
            except socket.timeout:
                continue
            except OSError:
                continue
            if data.startswith(b"DATA"):
                self.data_count += 1
                if peer[1] not in self.ports:
                    self.ports.append(peer[1])

    def run_tcp(self):
        while not self.stop:
            try:
                conn, _ = self.tcp.accept()
            except socket.timeout:
                continue
            except OSError:
                return
            conn.settimeout(2)
            try:
                data = conn.recv(4096)
                if data:
                    conn.sendall(self.name.encode() + b":" + data)
            except OSError:
                pass
            conn.close()


def tcp_once():
    try:
        with socket.create_connection(("127.0.0.1", PORT), timeout=3) as s:
            s.settimeout(3)
            s.sendall(b"ping")
            return s.recv(100).decode()
    except OSError as e:
        return "error %r" % (e,)


def run(which):
    a, b = Echo("primary"), Echo("backup")
    conf = os.path.join(c.TMP, "rf02b-%s.toml" % which)
    log = os.path.join(c.TMP, "rf02b-%s.log" % which)
    if os.path.exists(log):
        os.remove(log)
    with open(conf, "w") as f:
        f.write('[log]\nlevel = "info"\noutput = "%s"\n[network]\nuse_udp = true\n[[endpoints]]\n'
                'listen = "127.0.0.1:%d"\nremote = "127.0.0.1:%d"\nextra_remotes = ["127.0.0.1:%d"]\n'
                '[endpoints.failover]\ninterval = 0.5\ntimeout = 0.3\nfall = 2\nwindow = 3\nrise = 2\nhold = 1.0\n'
                % (log, PORT, a.port, b.port))
    out_p, err_p = os.path.join(c.TMP, "rf02b.out"), os.path.join(c.TMP, "rf02b.err")
    proc = c.start(c.BINS[which], ["-c", conf], out_p, err_p)
    res = {"steps": []}
    try:
        if not (c.wait_tcp(PORT, 8) and c.wait_udp_bound(proc.pid, PORT, 8)):
            res["error"] = c.read(err_p)[-400:]
            return res
        time.sleep(1.5)
        cli = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        cli.bind(("127.0.0.1", 0))
        cli.settimeout(0.12)
        seq = 0

        def phase(name, secs):
            nonlocal seq
            a0, b0 = a.data_count, b.data_count
            sent = got = 0
            end = time.monotonic() + secs
            while time.monotonic() < end:
                seq += 1
                msg = b"DATA%06d" % seq
                cli.sendto(msg, ("127.0.0.1", PORT))
                sent += 1
                try:
                    while True:
                        if cli.recvfrom(2048)[0] == msg:
                            got += 1
                            break
                except socket.timeout:
                    pass
                time.sleep(0.05)
            step = {"step": name, "sent": sent, "replies": got, "to_primary": a.data_count - a0,
                    "to_backup": b.data_count - b0, "tcp": tcp_once(), "realm_udp_sockets": len(c.udp_sockets(proc.pid))}
            res["steps"].append(step)
            print(which, step, flush=True)

        phase("both up", 3)
        a.close_udp()
        phase("primary udp echo gone", 5)
        a.open_udp()
        phase("primary back", 6)
        res["relay_source_ports_seen_by_primary"] = a.ports
        res["relay_source_ports_seen_by_backup"] = b.ports
        res["alive_at_end"] = proc.poll() is None
        cli.close()
    finally:
        c.stop(proc)
        a.stop = b.stop = True
        res["log"] = [l.split("]", 2)[-1] for l in c.read(log).splitlines() if "failover" in l or "association" in l][:30]
        res["stderr"] = c.read(err_p)[-300:]
        for p in (conf, log, out_p, err_p):
            if os.path.exists(p):
                os.remove(p)
    return res


def main():
    os.makedirs(c.TMP, exist_ok=True)
    out = {}
    for which in ("old", "new", "dbg"):
        out[which] = run(which)
        r = out[which]
        print(which, "ports primary=%s backup=%s" % (r.get("relay_source_ports_seen_by_primary"), r.get("relay_source_ports_seen_by_backup")), r.get("error", ""))
        for l in r.get("log", []):
            print("    ", l)
    with open(os.path.join(c.WORK, "results", "rf02b-failover.json"), "w") as f:
        json.dump(out, f, indent=1)


if __name__ == "__main__":
    try:
        main()
    finally:
        c.stop_all()
