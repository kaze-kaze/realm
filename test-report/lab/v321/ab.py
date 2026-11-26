#!/usr/bin/env python3
"""Loopback A/B of two realm binaries: udp up / down (1 and 4 clients), tcp bulk, tcp short connections.
Order-balanced rounds (ABBA). Reports throughput and the relay's cpu time per unit of work.
Usage: ab.py OLD NEW [rounds] [fot]"""
import json, multiprocessing as mp, os, socket, statistics, subprocess, sys, threading, time

OLD, NEW = sys.argv[1], sys.argv[2]
ROUNDS = int(sys.argv[3]) if len(sys.argv) > 3 else 4
FOT = sys.argv[4] if len(sys.argv) > 4 else None
LP, RP = 39400, 39401
WIN = 3.0            # measured window, seconds
PKT = 1400
TICK = os.sysconf("SC_CLK_TCK")
DEVNULL = subprocess.DEVNULL
mp.set_start_method("fork")


def cpu(pid):
    f = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
    return (int(f[11]) + int(f[12])) / TICK


def start(binary):
    p = subprocess.Popen([binary, "-u", "-l", f"127.0.0.1:{LP}", "-r", f"127.0.0.1:{RP}"], stdout=DEVNULL, stderr=DEVNULL)
    time.sleep(0.6)
    assert p.poll() is None, "relay exited"
    return p


def stop(p):
    p.terminate()
    try:
        p.wait(3)
    except Exception:
        p.kill()
    time.sleep(0.3)


def udp(bind=None, rcvbuf=None):
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    if rcvbuf:
        s.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, rcvbuf)
    if bind:
        s.bind(bind)
    return s


def flood(dsts, size, secs):
    """Sender process: one socket per destination, as fast as python goes."""
    socks = []
    for d in dsts:
        s = udp(); s.connect(d); s.setblocking(True); socks.append(s)
    payload = b"x" * size
    end = time.time() + secs
    while time.time() < end:
        for _ in range(128):
            for s in socks:
                try:
                    s.send(payload)
                except OSError:
                    pass


def count(sock, t0, t1, out, idx):
    """Count datagrams that arrive inside [t0, t1)."""
    sock.settimeout(0.2)
    n = 0
    while True:
        now = time.time()
        if now >= t1:
            break
        try:
            sock.recv(65536)
        except OSError:
            continue
        if t0 <= time.time() < t1:
            n += 1
    out[idx] = n


def udp_up(binary, nclients):
    back = udp(("127.0.0.1", RP), 4 << 20)
    r = start(binary)
    senders = [mp.Process(target=flood, args=([("127.0.0.1", LP)] * 1, PKT, WIN + 2.0)) for _ in range(nclients)]
    for s in senders:
        s.start()
    t0 = time.time() + 1.0
    time.sleep(max(0, t0 - time.time()))
    c0 = cpu(r.pid)
    out = [0]
    count(back, t0, t0 + WIN, out, 0)
    c1 = cpu(r.pid)
    for s in senders:
        s.join()
    stop(r); back.close()
    return out[0], c1 - c0


def udp_down(binary, nclients):
    back = udp(("127.0.0.1", RP))
    back.settimeout(2)
    r = start(binary)
    clients = [udp(("127.0.0.1", 0), 4 << 20) for _ in range(nclients)]
    for c in clients:
        c.sendto(b"hello", ("127.0.0.1", LP))
    assoc = []
    for _ in clients:
        _, a = back.recvfrom(100)
        assoc.append(a)
    back.close()
    senders = [mp.Process(target=flood, args=([a], PKT, WIN + 2.0)) for a in assoc]
    for s in senders:
        s.start()
    t0 = time.time() + 1.0
    out = [0] * nclients
    ths = [threading.Thread(target=count, args=(c, t0, t0 + WIN, out, i)) for i, c in enumerate(clients)]
    for t in ths:
        t.start()
    time.sleep(max(0, t0 - time.time()))
    c0 = cpu(r.pid)
    for t in ths:
        t.join()
    c1 = cpu(r.pid)
    for s in senders:
        s.join()
    stop(r)
    for c in clients:
        c.close()
    return sum(out), c1 - c0


def tcp_bulk(binary, upload):
    GIB = 2
    ls = socket.socket(); ls.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    ls.bind(("127.0.0.1", RP)); ls.listen(8)
    r = start(binary)
    buf = b"x" * (1 << 20)
    res = {}

    def server():
        c, _ = ls.accept()
        if upload:
            n = 0
            while True:
                d = c.recv(1 << 20)
                if not d:
                    break
                n += len(d)
            res["n"] = n
        else:
            for _ in range(GIB * 1024):
                c.sendall(buf)
        c.close()

    th = threading.Thread(target=server); th.start()
    c = socket.create_connection(("127.0.0.1", LP))
    c0 = cpu(r.pid); t = time.time(); n = 0
    if upload:
        for _ in range(GIB * 1024):
            c.sendall(buf)
        c.close(); th.join(); n = res["n"]
    else:
        while True:
            d = c.recv(1 << 20)
            if not d:
                break
            n += len(d)
        c.close(); th.join()
    dt = time.time() - t; c1 = cpu(r.pid)
    stop(r); ls.close()
    assert n == GIB << 30, n
    return n * 8 / dt / 1e9, (c1 - c0) / GIB


def tcp_churn(binary):
    srv = subprocess.Popen([FOT, "srv", "tag=A", f"tcp=127.0.0.1:{RP}", "threads=2"], stdout=DEVNULL, stderr=DEVNULL)
    time.sleep(0.4)
    r = start(binary)
    c0 = cpu(r.pid)
    out = subprocess.run([FOT, "churn", f"target=127.0.0.1:{LP}", "conns=32", "secs=4", "size=64", "threads=2"],
                         capture_output=True, text=True).stdout
    c1 = cpu(r.pid)
    stop(r); srv.terminate(); srv.wait()
    j = json.loads(out.strip().splitlines()[-1])
    return j["rate"], (c1 - c0) * 1e6 / max(1, j["total_ok"]), j["total_err"]


TESTS = {
    "udp up, 1 client (Mbit/s | cpu us/pkt)": lambda b: (lambda n, c: (n * PKT * 8 / WIN / 1e6, c * 1e6 / max(1, n)))(*udp_up(b, 1)),
    "udp up, 4 clients (Mbit/s | cpu us/pkt)": lambda b: (lambda n, c: (n * PKT * 8 / WIN / 1e6, c * 1e6 / max(1, n)))(*udp_up(b, 4)),
    "udp down, 1 client (Mbit/s | cpu us/pkt)": lambda b: (lambda n, c: (n * PKT * 8 / WIN / 1e6, c * 1e6 / max(1, n)))(*udp_down(b, 1)),
    "udp down, 4 clients (Mbit/s | cpu us/pkt)": lambda b: (lambda n, c: (n * PKT * 8 / WIN / 1e6, c * 1e6 / max(1, n)))(*udp_down(b, 4)),
    "tcp download (Gbit/s | cpu s/GiB)": lambda b: tcp_bulk(b, False),
    "tcp upload (Gbit/s | cpu s/GiB)": lambda b: tcp_bulk(b, True),
}
if FOT:
    TESTS["tcp short conns (conn/s | cpu us/conn | errors)"] = tcp_churn

only = os.environ.get("AB_ONLY")
runs = {k: {"old": [], "new": []} for k in TESTS if not only or only in k}
for i in range(ROUNDS):
    order = [("old", OLD), ("new", NEW)] if i % 2 == 0 else [("new", NEW), ("old", OLD)]
    for name in runs:
        for tag, binary in order:
            m = TESTS[name](binary)
            runs[name][tag].append(m)
            print(i, name, tag, [round(x, 2) for x in m], flush=True)

print("\n=== medians (min..max), old vs new")
for name, r in runs.items():
    cols = len(r["old"][0])
    line = []
    for k in range(cols):
        o = [m[k] for m in r["old"]]; n = [m[k] for m in r["new"]]
        mo, mn = statistics.median(o), statistics.median(n)
        d = (mn - mo) / mo * 100 if mo else 0
        line.append(f"old {mo:.2f} ({min(o):.2f}..{max(o):.2f}) new {mn:.2f} ({min(n):.2f}..{max(n):.2f}) {d:+.1f}%")
    print(f"{name}\n    " + "\n    ".join(line))
json.dump(runs, open(os.environ.get("AB_OUT", "/dev/null"), "w"))
