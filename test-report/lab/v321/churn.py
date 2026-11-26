"""Three-way short-connection comparison: old, new, new with half-close 0 (and optional extras)."""
import json, os, statistics, subprocess, sys, time
DEVNULL = subprocess.DEVNULL
FOT = sys.argv[1]; ROUNDS = int(sys.argv[2]); SECS = sys.argv[3]
VARIANTS = [v.split("=", 1) for v in sys.argv[4:]]  # name=cmd with args separated by spaces
LP, RP = 39411, 39412
TICK = os.sysconf("SC_CLK_TCK")

def cpu(pid):
    f = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
    return (int(f[11]) + int(f[12])) / TICK

def run(cmd):
    srv = subprocess.Popen([FOT, "srv", "tag=A", f"tcp=127.0.0.1:{RP}", "threads=2"], stdout=DEVNULL, stderr=DEVNULL)
    time.sleep(0.4)
    r = subprocess.Popen(cmd.split() + ["-l", f"127.0.0.1:{LP}", "-r", f"127.0.0.1:{RP}"], stdout=DEVNULL, stderr=DEVNULL)
    time.sleep(0.6)
    assert r.poll() is None
    c0 = cpu(r.pid)
    out = subprocess.run([FOT, "churn", f"target=127.0.0.1:{LP}", "conns=32", f"secs={SECS}", "size=64", "threads=2"],
                         capture_output=True, text=True).stdout
    c1 = cpu(r.pid)
    r.terminate(); r.wait(); srv.terminate(); srv.wait(); time.sleep(0.3)
    j = json.loads(out.strip().splitlines()[-1])
    return j["rate"], (c1 - c0) * 1e6 / max(1, j["total_ok"]), j["total_err"]

res = {n: [] for n, _ in VARIANTS}
for i in range(ROUNDS):
    order = VARIANTS[i % len(VARIANTS):] + VARIANTS[:i % len(VARIANTS)]
    if i % 2: order = order[::-1]
    for n, c in order:
        m = run(c); res[n].append(m)
        print(i, n, [round(x, 2) for x in m], flush=True)
print("=== medians (min..max): conn/s | cpu us/conn | errors")
base = None
for n, _ in VARIANTS:
    r = [x[0] for x in res[n]]; c = [x[1] for x in res[n]]; e = sum(x[2] for x in res[n])
    mr, mc = statistics.median(r), statistics.median(c)
    if base is None: base = (mr, mc)
    print(f"{n:14s} {mr:9.1f} ({min(r):.0f}..{max(r):.0f}) {100*(mr-base[0])/base[0]:+6.1f}% | {mc:7.2f} ({min(c):.1f}..{max(c):.1f}) {100*(mc-base[1])/base[1]:+6.1f}% | {e}")
