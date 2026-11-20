"""Quiet-machine A/B of bulk tcp throughput through the relay, order balanced.
Usage: perf_tcp.py [rounds]"""
import json, os, statistics, sys, time
from lab import *

ROUNDS = int(sys.argv[1]) if len(sys.argv) > 1 else 6
net_down()
sh("iperf3 -s -p 5201 -D; iperf3 -s -p 5202 -D")
srv = [Proc(None, f"{FOT} srv tag={t} health=127.0.0.1:{h} hudp=127.0.0.1:{h} threads=2", "fot") for t, h in (("A", 7001), ("B", 7002))]
time.sleep(0.5)
PLAIN = '[network]\nuse_udp = true\n[[endpoints]]\nlisten = "127.0.0.1:6000"\nremote = "127.0.0.1:5201"\n'
FO = '[network]\nuse_udp = true\ntcp_timeout = 4\n[[endpoints]]\nlisten = "127.0.0.1:6000"\nremote = "127.0.0.1:5201"\nextra_remotes = ["127.0.0.1:5202"]\n[endpoints.failover]\nprobe = ["127.0.0.1:7001", "127.0.0.1:7002"]\ntcp_send = "ping"\ntcp_expect = "OK"\n'
VARS = {
    "orig": (ORIG, PLAIN),
    "final": (NEW, PLAIN),
    "final+failover": (NEW, FO),
    "merged+failover": (f"{BIN}/realm-new", FO),
}

def iperf(args):
    for _ in range(3):
        try:
            j = json.loads(sh(f"iperf3 -c 127.0.0.1 -p 6000 -J {args}", check=False))
            if "error" not in j:
                return j
        except Exception:
            pass
        time.sleep(1)
    return None

def bench(name):
    binary, conf = VARS[name]
    r = realm(conf, binary=binary, ns=None, level="off", name=f"tcp-{name}")
    time.sleep(3.5)
    m = {}
    for key, args in (("tcp1_up", "-t 5"), ("tcp1_down", "-t 5 -R"), ("tcp8", "-t 5 -P 8")):
        j = iperf(args)
        m[key] = j["end"]["sum_received"]["bits_per_second"] / 1e9 if j else None
    r.kill()
    time.sleep(0.5)
    return m

runs = {k: [] for k in VARS}
order = list(VARS)
for i in range(ROUNDS):
    for name in (order if i % 2 == 0 else order[::-1]):
        m = bench(name)
        runs[name].append(m)
        print(i, name, {k: round(v, 1) for k, v in m.items() if v}, flush=True)
summary = {n: {k: {"median": statistics.median(m[k] for m in ms_ if m[k]), "min": min(m[k] for m in ms_ if m[k]), "max": max(m[k] for m in ms_ if m[k])} for k in ms_[0]} for n, ms_ in runs.items()}
json.dump({"runs": runs, "summary": summary}, open(f"{OUT}/perf-tcp.json", "w"), indent=1)
for k in ("tcp1_up", "tcp1_down", "tcp8"):
    print(f"{k:10s} " + " | ".join(f"{n}={s[k]['median']:.1f} ({s[k]['min']:.1f}..{s[k]['max']:.1f})" for n, s in summary.items()))
for p in srv:
    p.kill()
sh("pkill iperf3", check=False)
