"""Order-balanced A/B of the 1 MiB echo round trip (new connection per request, byte-checked) through the relay.
Usage: perf_xfer.py [rounds]"""
import json, os, statistics, sys, time
from lab import *

ROUNDS = int(sys.argv[1]) if len(sys.argv) > 1 else 8
net_down()
sh("sysctl -qw net.ipv4.ip_local_port_range='1024 65535' net.core.somaxconn=8192 net.ipv4.tcp_max_syn_backlog=8192", check=False)
srv = [Proc(None, f"{FOT} srv tag={t} tcp=127.0.0.1:{p} udp=127.0.0.1:{p} health=127.0.0.1:{h} hudp=127.0.0.1:{h} threads=4", "fot")
       for t, p, h in (("A", 5301, 7001), ("B", 5302, 7002))]
time.sleep(0.5)
PLAIN = '[[endpoints]]\nlisten = "127.0.0.1:6001"\nremote = "127.0.0.1:5301"\n'
FO = '[network]\ntcp_timeout = 4\n' + PLAIN + 'extra_remotes = ["127.0.0.1:5302"]\n[endpoints.failover]\nprobe = ["127.0.0.1:7001", "127.0.0.1:7002"]\ntcp_send = "ping"\ntcp_expect = "OK"\n'
VARS = {"orig": (ORIG, PLAIN), "final": (NEW, PLAIN), "final+failover": (NEW, FO), "before-rule-removal+failover": (f"{BIN}/realm-fix3", FO)}

def bench(name):
    binary, conf = VARS[name]
    r = realm(conf, binary=binary, ns=None, level="off", name=f"xfer-{name}")
    pid = r.pid()
    time.sleep(1.5)
    t0 = cpu_ticks(pid)
    c = tool(None, "churn target=127.0.0.1:6001 conns=16 secs=6 size=1048576 threads=4")
    cpu = (cpu_ticks(pid) - t0) * 10 / max(1, c["total_ok"])
    r.kill()
    time.sleep(0.3)
    return {"mib_s": c["total_ok"] * 2 / c["elapsed"], "err": c["total_err"], "relay_cpu_ms_per_xfer": cpu, "p50_us": c["lat_us"]["p50"]}

runs = {k: [] for k in VARS}
order = list(VARS)
for i in range(ROUNDS):
    seq = order if i % 2 == 0 else order[::-1]   # ABBA: cancel drift and position effects
    for name in seq:
        m = bench(name)
        runs[name].append(m)
        print(i, name, {k: round(v, 2) for k, v in m.items()}, flush=True)
summary = {n: {k: {"median": statistics.median(m[k] for m in ms_), "mean": statistics.mean(m[k] for m in ms_), "min": min(m[k] for m in ms_), "max": max(m[k] for m in ms_)} for k in ms_[0]} for n, ms_ in runs.items()}
json.dump({"runs": runs, "summary": summary}, open(f"{OUT}/perf-xfer.json", "w"), indent=1)
for k in ("mib_s", "relay_cpu_ms_per_xfer", "p50_us", "err"):
    print(f"{k:22s} " + " | ".join(f"{n}={s[k]['median']:.2f} mean {s[k]['mean']:.2f} ({s[k]['min']:.2f}..{s[k]['max']:.2f})" for n, s in summary.items()))
for p in srv:
    p.kill()
