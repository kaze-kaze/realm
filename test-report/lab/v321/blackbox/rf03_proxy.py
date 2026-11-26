#!/usr/bin/env python3
"""RF-03: PROXY v2 headers against `accept_proxy = true` (accept_proxy_timeout
left at its default of 5s), v3.2.0 against the rc2 builds.

usage: python3 -I rf03_proxy.py
writes results/rf03.json
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

SIG = b"\r\n\r\n\0\r\nQUIT\n"
PORT = 39630
DATA = b"hello-rf03\n"
MIN = {0: 0, 1: 12, 2: 36, 3: 216}


def fixed(cmd, fam_proto, length):
    return SIG + bytes([cmd, fam_proto]) + struct.pack("!H", length)


def v4_payload():
    return socket.inet_aton("192.0.2.1") + socket.inet_aton("192.0.2.2") + struct.pack("!HH", 1111, 2222)


def v6_payload():
    a = socket.inet_pton(socket.AF_INET6, "2001:db8::1")
    b = socket.inet_pton(socket.AF_INET6, "2001:db8::2")
    return a + b + struct.pack("!HH", 1111, 2222)


class Echo(threading.Thread):
    def __init__(self):
        super().__init__(daemon=True)
        self.sock = socket.socket()
        self.sock.bind(("127.0.0.1", 0))
        self.sock.listen(128)
        self.sock.settimeout(0.2)
        self.port = self.sock.getsockname()[1]
        self.stop = False
        self.accepted = 0

    def run(self):
        while not self.stop:
            try:
                conn, _ = self.sock.accept()
            except socket.timeout:
                continue
            except OSError:
                return
            self.accepted += 1
            threading.Thread(target=self.serve, args=(conn,), daemon=True).start()

    @staticmethod
    def serve(conn):
        conn.settimeout(15)
        try:
            while True:
                data = conn.recv(65536)
                if not data:
                    break
                conn.sendall(data)
        except OSError:
            pass
        finally:
            conn.close()


def probe(chunks, hold=8.0, expect=None):
    """Send the chunks (bytes, or a float to sleep), then wait for an echo or a close.

    Returns (outcome, seconds since the last byte was sent, detail)."""
    s = socket.create_connection(("127.0.0.1", PORT), timeout=3)
    try:
        t0 = time.monotonic()
        for i, ch in enumerate(chunks):
            if isinstance(ch, (int, float)):
                time.sleep(ch)
                continue
            try:
                s.sendall(ch)
            except OSError as e:
                return ("closed-while-sending", round(time.monotonic() - t0, 3), "chunk %d: %r" % (i, e))
            t0 = time.monotonic()
        s.settimeout(hold)
        got = b""
        while True:
            try:
                data = s.recv(65536)
            except socket.timeout:
                return ("still-open", round(time.monotonic() - t0, 3), got.hex() if got else "")
            except ConnectionResetError:
                return ("closed-rst", round(time.monotonic() - t0, 3), got.hex() if got else "")
            if not data:
                return ("closed-fin", round(time.monotonic() - t0, 3), got.hex() if got else "")
            got += data
            if expect is not None and len(got) >= len(expect):
                ok = got == expect
                return ("forwarded" if ok else "forwarded-WRONG-BYTES", round(time.monotonic() - t0, 3),
                        "" if ok else got.hex())
    finally:
        s.close()


def cases_timing():
    """Only the 16 byte fixed header is sent and the connection is held open."""
    return [
        ("T1 unknown family 0x4, PROXY, len 12", [fixed(0x21, 0x41, 12)]),
        ("T2 unknown transport 0x3, PROXY TCP4, len 12", [fixed(0x21, 0x13, 12)]),
        ("T3 TCP4 len 11 (<12)", [fixed(0x21, 0x11, 11)]),
        ("T4 TCP6 len 35 (<36)", [fixed(0x21, 0x21, 35)]),
        ("T5 UNIX len 215 (<216)", [fixed(0x21, 0x31, 215)]),
        ("T6 LOCAL unknown family 0x4, len 12", [fixed(0x20, 0x41, 12)]),
        ("T7 LOCAL TCP4 len 4 (<12)", [fixed(0x20, 0x11, 4)]),
        ("T8 unknown family 0xF, transport 0xF, len 4096", [fixed(0x21, 0xFF, 4096)]),
        ("T9 AF_UNSPEC unknown transport 0x3, len 8", [fixed(0x21, 0x03, 8)]),
        ("T10 control: valid TCP4 fixed header, payload never sent", [fixed(0x21, 0x11, 12)]),
        ("T11 control: valid AF_UNSPEC len 8, payload never sent", [fixed(0x21, 0x00, 8)]),
        ("T12 control: len 4097 (> 4096 cap)", [fixed(0x21, 0x11, 4097)]),
        ("T13 control: unknown command 0x22", [fixed(0x22, 0x11, 12)]),
        ("T14 control: version 1 in v2 frame (0x11)", [fixed(0x11, 0x11, 12)]),
    ]


def cases_accept():
    """Complete headers followed by application data."""
    v4, v6 = v4_payload(), v6_payload()
    return [
        ("A1 PROXY TCP4 + data, one segment", [fixed(0x21, 0x11, 12) + v4 + DATA]),
        ("A2 PROXY TCP6 + data", [fixed(0x21, 0x21, 36) + v6 + DATA]),
        ("A3 LOCAL AF_UNSPEC len 0 + data", [fixed(0x20, 0x00, 0) + DATA]),
        ("A4 PROXY TCP4 with 7 TLV bytes + data", [fixed(0x21, 0x11, 19) + v4 + b"\x04\x00\x04abcd" + DATA]),
        ("A5 PROXY UNIX stream len 216 + data", [fixed(0x21, 0x31, 216) + bytes(216) + DATA]),
        ("A6 LOCAL TCP4 len 12 + data", [fixed(0x20, 0x11, 12) + v4 + DATA]),
        ("A7 PROXY AF_UNSPEC len 0 + data", [fixed(0x21, 0x00, 0) + DATA]),
        ("A8 PROXY AF_UNSPEC len 5 + data", [fixed(0x21, 0x00, 5) + b"\0" * 5 + DATA]),
        ("A9 PROXY UDP4 len 12 + data", [fixed(0x21, 0x12, 12) + v4 + DATA]),
        ("A10 LOCAL AF_UNSPEC/STREAM len 0 + data", [fixed(0x20, 0x01, 0) + DATA]),
        ("A11 v1 TCP4 + data", [b"PROXY TCP4 192.0.2.1 192.0.2.2 1111 2222\r\n" + DATA]),
        ("A12 v1 UNKNOWN + data", [b"PROXY UNKNOWN\r\n" + DATA]),
        ("A13 PROXY TCP4, fixed header / payload / data in three segments",
         [fixed(0x21, 0x11, 12), 0.3, v4, 0.3, DATA]),
        ("A14 LOCAL len 0, header and data in two segments", [fixed(0x20, 0x00, 0), 0.3, DATA]),
        ("A15 PROXY TCP4 byte by byte, then data",
         sum(([bytes([b]), 0.01] for b in fixed(0x21, 0x11, 12) + v4), []) + [DATA]),
        ("A16 no header, plain data", [DATA]),
    ]


def cases_matrix():
    """Every command x family x transport x length, complete header + data in one segment."""
    out = []
    for cmd in (0x20, 0x21):
        for fam in (0, 1, 2, 3, 4, 0xF):
            for proto in (0, 1, 2, 3, 0xF):
                base = MIN.get(fam, 12)
                lengths = sorted({max(base - 1, 0), base, base + 5})
                for length in lengths:
                    name = "M cmd=%#04x fam=%#x proto=%#x len=%d" % (cmd, fam, proto, length)
                    out.append((name, [fixed(cmd, (fam << 4) | proto, length) + bytes(length) + DATA]))
    return out


def run(which, echo):
    conf = os.path.join(c.TMP, "rf03-%s.toml" % which)
    log = os.path.join(c.TMP, "rf03-%s.log" % which)
    if os.path.exists(log):
        os.remove(log)
    with open(conf, "w") as f:
        f.write('[log]\nlevel = "info"\noutput = "%s"\n[network]\naccept_proxy = true\n[[endpoints]]\n'
                'listen = "127.0.0.1:%d"\nremote = "127.0.0.1:%d"\n' % (log, PORT, echo.port))
    out_p, err_p = os.path.join(c.TMP, "rf03-%s.out" % which), os.path.join(c.TMP, "rf03-%s.err" % which)
    proc = c.start(c.BINS[which], ["-c", conf], out_p, err_p)
    res = {"timing": {}, "accept": {}, "matrix": {}}
    try:
        # the readiness connection itself sends no header: realm drops it after the timeout
        if not c.wait_tcp(PORT, 8):
            res["error"] = "not listening: rc=%r %s" % (proc.poll(), c.read(err_p)[-400:])
            return res
        res["inited"] = [l for l in c.read(out_p).splitlines() if l.startswith("inited")]
        for name, chunks in cases_timing():
            res["timing"][name] = probe(chunks, hold=8.0)
            print(which, name, res["timing"][name], flush=True)
        for name, chunks in cases_accept():
            res["accept"][name] = probe(chunks, hold=8.0, expect=DATA)
            print(which, name, res["accept"][name], flush=True)
        for name, chunks in cases_matrix():
            res["matrix"][name] = probe(chunks, hold=8.0, expect=DATA)
        res["alive_at_end"] = proc.poll() is None
    finally:
        c.stop(proc)
        res["stderr"] = c.read(err_p)[-1500:]
        lines = c.read(log).splitlines()
        res["log_lines"] = len(lines)
        res["log_sample"] = lines[:40]
        for p in (conf, log, out_p, err_p):
            if os.path.exists(p):
                os.remove(p)
    return res


def main():
    os.makedirs(c.TMP, exist_ok=True)
    os.makedirs(os.path.join(c.WORK, "results"), exist_ok=True)
    echo = Echo()
    echo.start()
    results = {}
    for which in ("old", "new", "dbg"):
        results[which] = run(which, echo)
    echo.stop = True
    diffs = []
    for name in results["old"].get("matrix", {}):
        o = results["old"]["matrix"][name]
        for tag in ("new", "dbg"):
            n = results[tag]["matrix"].get(name)
            if n is None or o[0].split("-")[0] != n[0].split("-")[0] or n[1] > 1.0 or o[1] > 1.0:
                diffs.append((name, tag, o, n))
    results["matrix_differences"] = diffs
    counts = {}
    for tag in ("old", "new", "dbg"):
        cnt = {}
        for v in results[tag].get("matrix", {}).values():
            cnt[v[0]] = cnt.get(v[0], 0) + 1
        counts[tag] = cnt
    results["matrix_counts"] = counts
    print("matrix counts", counts)
    print("matrix differences", len(diffs))
    for d in diffs[:40]:
        print("  ", d)
    with open(os.path.join(c.WORK, "results", "rf03.json"), "w") as f:
        json.dump(results, f, indent=1)


if __name__ == "__main__":
    try:
        main()
    finally:
        c.stop_all()
