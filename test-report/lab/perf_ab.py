"""Order-balanced A/B of short-connection rate and cpu cost: orig vs new, plain and balance paths.
Usage: perf_ab.py [rounds]"""
import json, os, statistics, sys, time
from lab import *

ROUNDS = int(sys.argv[1]) if len(sys.argv) > 1 else 6
net_down()
sh("sysctl -qw net.ipv4.ip_local_port_range='1024 65535' net.core.somaxconn=8192 net.ipv4.tcp_max_syn_backlog=8192", check=False)
srv = [Proc(None, f"{FOT} srv tag={t} tcp=127.0.0.1:{p} udp=127.0.0.1:{p} health=127.0.0.1:{h} hudp=127.0.0.1:{h} threads=4", "fot")
       for t, p, h in (("A", 5301, 7001), ("B", 5302, 7002))]
time.sleep(0.5)
PLAIN = '[[endpoints]]\nlisten = "127.0.0.1:6001"\nremote = "127.0.0.1:5301"\n'
BAL = PLAIN + 'extra_remotes = ["127.0.0.1:5302"]\nbalance = "roundrobin: 1, 1"\n'
FO = '[network]\ntcp_timeout = 4\n' + PLAIN + 'extra_remotes = ["127.0.0.1:5302"]\n[endpoints.failover]\nprobe = ["127.0.0.1:7001", "127.0.0.1:7002"]\ntcp_send = "ping"\ntcp_expect = "OK"\n'
VARS = {"orig": (ORIG, PLAIN), "new": (NEW, PLAIN), "orig+balance": (ORIG, BAL), "new+balance": (NEW, BAL), "new+failover": (NEW, FO)}

def bench(name):
    binary, conf = VARS[name]
    r = realm(conf, binary=binary, ns=None, level="off", name=f"ab-{name}")
    pid = r.pid()
    time.sleep(1.5)
    c = tool(None, "churn target=127.0.0.1:6001 conns=64 secs=6 size=64 threads=4")
    t0 = cpu_ticks(pid)
    f = tool(None, "churn target=127.0.0.1:6001 conns=32 secs=6 size=64 rate=5000 threads=4")
    cpu = (cpu_ticks(pid) - t0) * 10 / (f["total_ok"] / 1000)
    r.kill()
    time.sleep(0.3)
    return {"cps": c["rate"], "err": c["total_err"] + f["total_err"], "cpu_ms_per_1k": cpu, "p99_us": f["lat_us"]["p99"]}

runs = {k: [] for k in VARS}
order = list(VARS)
for i in range(ROUNDS):
    seq = order if i % 2 == 0 else order[::-1]   # ABBA: cancel drift and position effects
    for name in seq:
        m = bench(name)
        runs[name].append(m)
        print(i, name, {k: round(v, 1) for k, v in m.items()}, flush=True)
summary = {n: {k: {"median": statistics.median(m[k] for m in ms_), "min": min(m[k] for m in ms_), "max": max(m[k] for m in ms_)} for k in ms_[0]} for n, ms_ in runs.items()}
json.dump({"runs": runs, "summary": summary}, open(f"{OUT}/{os.environ.get('PERF_NAME', 'perf-ab')}.json", "w"), indent=1)
for k in ("cps", "cpu_ms_per_1k", "p99_us", "err"):
    print(f"{k:14s} " + " | ".join(f"{n}={s[k]['median']:.1f} ({s[k]['min']:.1f}..{s[k]['max']:.1f})" for n, s in summary.items()))
for p in srv:
    p.kill()
