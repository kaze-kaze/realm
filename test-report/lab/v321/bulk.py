"""TCP bulk A/B with source and sink in separate pinned processes.
usage: bulk.py ROUNDS GIB name=binary ...   (relay on cpus 0-1, client cpu 2, server cpu 3)"""
import json, os, socket, statistics, subprocess, sys, time
ROUNDS, GIB = int(sys.argv[1]), int(sys.argv[2])
VARIANTS = [v.split("=", 1) for v in sys.argv[3:]]
LP, RP = 39440, 39441
TICK = os.sysconf("SC_CLK_TCK")
BUF = b"x" * (1 << 20)

def cpu(pid):
    f = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
    return (int(f[11]) + int(f[12])) / TICK

def run(binary, upload):
    ls = socket.socket(); ls.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    ls.bind(("127.0.0.1", RP)); ls.listen(8)
    r = subprocess.Popen(["taskset", "-c", "0,1"] + binary.split() + ["-l", f"127.0.0.1:{LP}", "-r", f"127.0.0.1:{RP}"],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(0.5)
    rd, wr = os.pipe()
    pid = os.fork()
    if pid == 0:  # server side
        os.sched_setaffinity(0, {3}); os.close(rd)
        c, _ = ls.accept(); n = 0
        if upload:
            while True:
                d = c.recv(1 << 20)
                if not d: break
                n += len(d)
        else:
            for _ in range(GIB * 1024): c.sendall(BUF)
        c.close(); os.write(wr, str(n).encode()); os._exit(0)
    os.close(wr); ls.close()
    os.sched_setaffinity(0, {2})
    c = socket.create_connection(("127.0.0.1", LP))
    c0 = cpu(r.pid); t = time.time(); n = 0
    if upload:
        for _ in range(GIB * 1024): c.sendall(BUF)
        c.close(); n = int(os.read(rd, 32) or b"0")
    else:
        while True:
            d = c.recv(1 << 20)
            if not d: break
            n += len(d)
        c.close(); os.read(rd, 32)
    dt = time.time() - t; c1 = cpu(r.pid)
    os.waitpid(pid, 0); os.close(rd)
    r.terminate(); r.wait(); time.sleep(0.2)
    assert n == GIB << 30, n
    return n * 8 / dt / 1e9, (c1 - c0) / GIB

out = {}
for upload in (False, True):
    res = {n: [] for n, _ in VARIANTS}
    for i in range(ROUNDS):
        order = VARIANTS if i % 2 == 0 else VARIANTS[::-1]
        for n, b in order:
            res[n].append(run(b, upload))
    name = "upload" if upload else "download"; out[name] = res
    base = None
    for n, _ in VARIANTS:
        g = [x[0] for x in res[n]]; c = [x[1] for x in res[n]]
        mg, mc = statistics.median(g), statistics.median(c)
        if base is None: base = (mg, mc)
        print(f"{name:8s} {n:8s} {mg:6.2f} Gbit/s ({min(g):.1f}..{max(g):.1f}, mean {statistics.mean(g):.2f}) {100*(mg-base[0])/base[0]:+6.1f}% | "
              f"{mc:.4f} cpu s/GiB ({min(c):.3f}..{max(c):.3f}, mean {statistics.mean(c):.4f}) {100*(mc-base[1])/base[1]:+6.1f}%", flush=True)
json.dump(out, open(os.environ.get("AB_OUT", "/dev/null"), "w"))
