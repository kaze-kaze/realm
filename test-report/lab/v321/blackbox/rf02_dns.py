#!/usr/bin/env python3
"""RF-02: a UDP rule whose upstream is a hostname, while the hostname changes
its address family.

A tiny loopback DNS responder answers the test name (A=127.0.0.1, AAAA=::1,
TTL 1s) and can be switched to answer only one family.  An echo service sits on
127.0.0.1 and on ::1 with the same port and tags its replies with the family and
the source port it saw, so the client can tell which upstream answered and
whether the relay's upstream socket changed.

usage: python3 -I rf02_dns.py <old|new|dbg> <label> [key=value ...]
  udp_timeout=<n>|default   network.udp_timeout
  phases=both:3,v6:8,...    DNS mode and seconds of each phase
  through=<ip>              endpoint.through
  mode=<dns.mode>           dns.mode
  failover=1                literal peers + failover table instead of a hostname
  tcp=1                     also send one TCP request per phase
  clients=<n>               number of client sockets (default 2)
writes results/rf02-<label>.json
"""
import json
import os
import socket
import struct
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import common as c

NAME = "target.rf02.test"
T0 = time.monotonic()


def now():
    return round(time.monotonic() - T0, 3)


class Dns(threading.Thread):
    def __init__(self):
        super().__init__(daemon=True)
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.sock.bind(("127.0.0.1", 0))
        self.sock.settimeout(0.2)
        self.port = self.sock.getsockname()[1]
        self.mode = "both"
        self.log = []
        self.stop = False

    def run(self):
        while not self.stop:
            try:
                data, peer = self.sock.recvfrom(2048)
            except socket.timeout:
                continue
            except OSError:
                return
            try:
                self.sock.sendto(self.answer(data), peer)
            except Exception as e:  # keep serving
                self.log.append((now(), "error", repr(e), self.mode, 0))

    def answer(self, q):
        ident, flags, qd = struct.unpack("!HHH", q[:6])
        pos, labels = 12, []
        while q[pos]:
            n = q[pos]
            labels.append(q[pos + 1:pos + 1 + n].decode("ascii", "replace"))
            pos += 1 + n
        pos += 1
        qtype, qclass = struct.unpack("!HH", q[pos:pos + 4])
        question = q[12:pos + 4]
        name = ".".join(labels).lower()
        mode = self.mode
        answers = b""
        rcode = 0
        if name != NAME:
            rcode = 3
        elif qtype == 1 and mode in ("both", "v4"):
            answers = b"\xc0\x0c" + struct.pack("!HHIH", 1, 1, 1, 4) + socket.inet_aton("127.0.0.1")
        elif qtype == 28 and mode in ("both", "v6"):
            answers = b"\xc0\x0c" + struct.pack("!HHIH", 28, 1, 1, 16) + socket.inet_pton(socket.AF_INET6, "::1")
        ancount = 1 if answers else 0
        authority = b""
        nscount = 0
        if not answers:
            # SOA with a 1 second minimum so that an empty answer is not cached for long
            zone = b"\x04rf02\x04test\x00"
            rdata = b"\x02ns" + zone + b"\x04host" + zone + struct.pack("!IIIII", 1, 60, 60, 60, 1)
            authority = zone + struct.pack("!HHIH", 6, 1, 1, len(rdata)) + rdata
            nscount = 1
        self.log.append((now(), name, qtype, mode, ancount if rcode == 0 else -rcode))
        hdr = struct.pack("!HHHHHH", ident, 0x8400 | (flags & 0x0100) | 0x0080 | rcode, 1, ancount, nscount, 0)
        return hdr + question + answers + authority


class UdpEcho(threading.Thread):
    def __init__(self, family, host, port, tag):
        super().__init__(daemon=True)
        self.sock = socket.socket(family, socket.SOCK_DGRAM)
        self.sock.bind((host, port))
        self.sock.settimeout(0.2)
        self.port = self.sock.getsockname()[1]
        self.tag = tag
        self.count = 0
        self.stop = False

    def run(self):
        while not self.stop:
            try:
                data, peer = self.sock.recvfrom(65536)
            except socket.timeout:
                continue
            except OSError:
                return
            self.count += 1
            self.sock.sendto(b"%s|%d|" % (self.tag, peer[1]) + data, peer)


class TcpEcho(threading.Thread):
    def __init__(self, family, host, port, tag):
        super().__init__(daemon=True)
        self.sock = socket.socket(family, socket.SOCK_STREAM)
        self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.sock.bind((host, port))
        self.sock.listen(64)
        self.sock.settimeout(0.2)
        self.tag = tag
        self.count = 0
        self.stop = False

    def run(self):
        while not self.stop:
            try:
                conn, _ = self.sock.accept()
            except socket.timeout:
                continue
            except OSError:
                return
            threading.Thread(target=self.serve, args=(conn,), daemon=True).start()

    def serve(self, conn):
        conn.settimeout(3)
        try:
            data = conn.recv(4096)
            if data:
                self.count += 1
                conn.sendall(self.tag + b"|" + data)
        except OSError:
            pass
        finally:
            conn.close()


def echo_pair(tags=(b"v4", b"v6")):
    for _ in range(50):
        a = UdpEcho(socket.AF_INET, "127.0.0.1", 0, tags[0])
        try:
            b = UdpEcho(socket.AF_INET6, "::1", a.port, tags[1])
            ta = TcpEcho(socket.AF_INET, "127.0.0.1", a.port, tags[0])
            tb = TcpEcho(socket.AF_INET6, "::1", a.port, tags[1])
        except OSError:
            a.sock.close()
            continue
        return a, b, ta, tb
    raise RuntimeError("no free port pair")


def tcp_once(port):
    try:
        with socket.create_connection(("127.0.0.1", port), timeout=3) as s:
            s.settimeout(3)
            s.sendall(b"tcp-ping")
            return s.recv(100).decode("ascii", "replace")
    except OSError as e:
        return "error: %r" % (e,)


def main():
    which, label = sys.argv[1], sys.argv[2]
    opt = dict(a.split("=", 1) for a in sys.argv[3:])
    udp_timeout = opt.get("udp_timeout", "default")
    phases = [(p.split(":")[0], float(p.split(":")[1])) for p in opt.get("phases", "both:3,v6:8,v4:8,v6:8,both:6").split(",")]
    nclients = int(opt.get("clients", "2"))
    port = int(opt.get("port", "39620"))
    os.makedirs(c.TMP, exist_ok=True)
    os.makedirs(os.path.join(c.WORK, "results"), exist_ok=True)

    dns = Dns()
    e4, e6, t4, t6 = echo_pair()
    threads = [dns, e4, e6, t4, t6]
    extra = None
    if opt.get("failover"):
        extra = echo_pair((b"b4", b"b6"))
        threads += list(extra)
    for t in threads:
        t.start()

    log_path = os.path.join(c.TMP, "rf02-%s.log" % label)
    if os.path.exists(log_path):
        os.remove(log_path)
    conf = ['[log]', 'level = "%s"' % opt.get("log", "debug"), 'output = "%s"' % log_path,
            '[dns]', 'protocol = "udp"', 'nameservers = ["127.0.0.1:%d"]' % dns.port, 'min_ttl = 0', 'max_ttl = 1']
    if "mode" in opt:
        conf.append('mode = "%s"' % opt["mode"])
    conf += ['[network]', 'use_udp = true', 'no_tcp = %s' % ("false" if opt.get("tcp") else "true")]
    if udp_timeout != "default":
        conf.append("udp_timeout = %s" % udp_timeout)
    conf += ['[[endpoints]]', 'listen = "127.0.0.1:%d"' % port]
    if opt.get("failover"):
        conf += ['remote = "127.0.0.1:%d"' % e4.port, 'extra_remotes = ["127.0.0.1:%d"]' % extra[0].port,
                 '[endpoints.failover]', 'interval = 1.0', 'timeout = 0.5']
    else:
        conf.append('remote = "%s:%d"' % (NAME, e4.port))
        if "through" in opt:
            conf.append('through = "%s"' % opt["through"])
    conf_path = os.path.join(c.TMP, "rf02-%s.toml" % label)
    with open(conf_path, "w") as f:
        f.write("\n".join(conf) + "\n")

    out_p = os.path.join(c.TMP, "rf02-%s.out" % label)
    err_p = os.path.join(c.TMP, "rf02-%s.err" % label)
    proc = c.start(c.BINS[which], ["-c", conf_path], out_p, err_p)
    result = {"binary": which, "label": label, "options": opt, "config": conf, "phases": [], "echo_port": e4.port}
    try:
        if not c.wait_udp_bound(proc.pid, port, 8):
            result["error"] = "realm did not bind udp %d; rc=%r stderr=%s" % (port, proc.poll(), c.read(err_p)[-600:])
            return result
        if opt.get("failover"):
            time.sleep(1.5)  # let a probe round finish
        result["inited"] = [l for l in c.read(out_p).splitlines() if l.startswith(("inited", "dns"))]

        clients = []
        for _ in range(nclients):
            s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            s.bind(("127.0.0.1", 0))
            s.settimeout(0.12)
            clients.append(s)
        result["client_ports"] = [s.getsockname()[1] for s in clients]
        seq = 0
        for mode, secs in phases:
            dns.mode = mode
            start = now()
            end = time.monotonic() + secs
            ph = {"dns": mode, "start": start, "seconds": secs, "clients": [], "events": []}
            stats = [{"sent": 0, "v4": 0, "v6": 0, "b4": 0, "b6": 0, "lost": 0, "ports": [], "first": {},
                      "lost_before_first_change": None} for _ in clients]
            last_tag = [None] * nclients
            fd_samples = []
            next_fd = 0.0
            while time.monotonic() < end:
                tick = time.monotonic()
                for i, s in enumerate(clients):
                    seq += 1
                    payload = b"%07d" % seq
                    st = stats[i]
                    st["sent"] += 1
                    try:
                        s.sendto(payload, ("127.0.0.1", port))
                    except OSError as e:
                        ph["events"].append((now(), i, "send error %r" % (e,)))
                    got = None
                    wait_until = time.monotonic() + 0.12
                    while time.monotonic() < wait_until:
                        try:
                            data, _ = s.recvfrom(2048)
                        except socket.timeout:
                            break
                        except OSError as e:
                            ph["events"].append((now(), i, "recv error %r" % (e,)))
                            break
                        parts = data.split(b"|", 2)
                        if len(parts) == 3 and parts[2] == payload:
                            got = (parts[0].decode(), int(parts[1]))
                            break
                        ph["events"].append((now(), i, "stray reply %r" % (data[:40],)))
                    if got is None:
                        st["lost"] += 1
                        if last_tag[i] != "lost":
                            ph["events"].append((now(), i, "loss begins at seq %d" % seq))
                        last_tag[i] = "lost"
                        continue
                    tag, sport = got
                    st[tag] += 1
                    if tag not in st["first"]:
                        st["first"][tag] = round(now() - start, 3)
                    if sport not in st["ports"]:
                        st["ports"].append(sport)
                        ph["events"].append((now(), i, "upstream %s sees relay source port %d" % (tag, sport)))
                    if last_tag[i] != tag:
                        ph["events"].append((now(), i, "replies now from %s (after %d sent, %d lost in this phase)" % (
                            tag, st["sent"], st["lost"])))
                    last_tag[i] = tag
                if time.monotonic() >= next_fd:
                    fd_samples.append((now(), len(c.udp_sockets(proc.pid))))
                    next_fd = time.monotonic() + 1.0
                if proc.poll() is not None:
                    ph["events"].append((now(), -1, "realm exited rc=%r" % proc.returncode))
                    break
                time.sleep(max(0.0, 0.1 - (time.monotonic() - tick)))
            if opt.get("tcp"):
                ph["tcp"] = tcp_once(port)
            ph["clients"] = stats
            ph["udp_sockets"] = fd_samples
            result["phases"].append(ph)
            print("%s %-22s dns=%-4s %5.1fs  %s  udp-sockets=%s%s" % (
                which, label, mode, secs,
                " | ".join("sent=%d v4=%d v6=%d%s lost=%d ports=%s" % (
                    s["sent"], s["v4"], s["v6"], (" b4=%d" % s["b4"]) if s["b4"] else "", s["lost"], s["ports"]) for s in stats),
                [n for _, n in fd_samples][-3:], ("  tcp=" + ph["tcp"]) if "tcp" in ph else ""), flush=True)
        result["alive_at_end"] = proc.poll() is None
        result["udp_sockets_at_end"] = c.udp_sockets(proc.pid)
    finally:
        rc = c.stop(proc)
        for t in threads:
            t.stop = True
        result["stderr"] = c.read(err_p)[-2000:]
        log = c.read(log_path).splitlines()
        result["log_assoc"] = [l for l in log if "association" in l or "warn" in l.lower() or "error" in l.lower()][:80]
        resolved, prev = [], None
        for l in log:
            if "resolved as" in l:
                cur = l.split("resolved as", 1)[1].strip()
                if cur != prev:
                    resolved.append(l)
                    prev = cur
        result["log_resolution_changes"] = resolved[:60]
        result["dns_queries"] = len(dns.log)
        result["dns_log_head"] = dns.log[:12]
        with open(os.path.join(c.WORK, "results", "rf02-%s.json" % label), "w") as f:
            json.dump(result, f, indent=1, ensure_ascii=False)
        for p in (log_path, out_p, err_p, conf_path):
            if os.path.exists(p) and not opt.get("keep"):
                os.remove(p)
    return result


if __name__ == "__main__":
    try:
        r = main()
        if r and r.get("error"):
            print("ERROR", r["error"])
    finally:
        c.stop_all()
