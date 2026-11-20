"""Throughput / efficiency A-B on loopback: orig vs new vs new+failover (+ balance)."""
import json, os, statistics, sys, time
from lab import *

ROUNDS = int(sys.argv[1]) if len(sys.argv) > 1 else 3
net_down()
sh("sysctl -qw net.ipv4.ip_local_port_range='1024 65535' net.core.somaxconn=8192 net.ipv4.tcp_max_syn_backlog=8192", check=False)

sh("iperf3 -s -p 5201 -D; iperf3 -s -p 5202 -D")
srv_a = Proc(None, f"{FOT} srv tag=A tcp=127.0.0.1:5301 udp=127.0.0.1:5301 health=127.0.0.1:7001 hudp=127.0.0.1:7001 threads=4", "fot")
srv_b = Proc(None, f"{FOT} srv tag=B tcp=127.0.0.1:5302 udp=127.0.0.1:5302 health=127.0.0.1:7002 hudp=127.0.0.1:7002 threads=4", "fot")
time.sleep(0.5)

PLAIN = """
[network]
use_udp = true
[[endpoints]]
listen = "127.0.0.1:6000"
remote = "127.0.0.1:5201"
[[endpoints]]
listen = "127.0.0.1:6001"
remote = "127.0.0.1:5301"
"""
FO = """
[network]
use_udp = true
tcp_timeout = 4
[[endpoints]]
listen = "127.0.0.1:6000"
remote = "127.0.0.1:5201"
extra_remotes = ["127.0.0.1:5202"]
[endpoints.failover]
probe = ["127.0.0.1:7001", "127.0.0.1:7002"]
tcp_send = "ping"
tcp_expect = "OK"
[[endpoints]]
listen = "127.0.0.1:6001"
remote = "127.0.0.1:5301"
extra_remotes = ["127.0.0.1:5302"]
[endpoints.failover]
probe = ["127.0.0.1:7001", "127.0.0.1:7002"]
tcp_send = "ping"
tcp_expect = "OK"
"""
BAL = """
[network]
use_udp = true
[[endpoints]]
listen = "127.0.0.1:6000"
remote = "127.0.0.1:5201"
[[endpoints]]
listen = "127.0.0.1:6001"
remote = "127.0.0.1:5301"
extra_remotes = ["127.0.0.1:5302"]
balance = "roundrobin: 1, 1"
"""
VARIANTS = [
    ("orig", ORIG, PLAIN),
    ("new", NEW, PLAIN),
    ("new+failover", NEW, FO),
    ("orig+balance", ORIG, BAL),
    ("new+balance", NEW, BAL),
]

def iperf(args):
    for _ in range(3):
        out = sh(f"iperf3 -c 127.0.0.1 -p 6000 -J {args}", check=False)
        try:
            j = json.loads(out)
            if "error" not in j:
                return j
        except Exception:
            pass
        time.sleep(1)
    return None

def bench(name, binary, conf):
    r = realm(conf, binary=binary, ns=None, level="off", name=f"perf-{name}")
    pid = r.pid()
    time.sleep(3.5)  # let the first probe rounds pass
    m = {"rss_idle_kb": rss_kb(pid), "threads": nthreads(pid), "fd_idle": nfd(pid)}
    if "balance" not in name:
        j = iperf("-t 5"); m["tcp1_gbps"] = j and j["end"]["sum_received"]["bits_per_second"] / 1e9
        j = iperf("-t 5 -R"); m["tcp1_rev_gbps"] = j and j["end"]["sum_received"]["bits_per_second"] / 1e9
        j = iperf("-t 5 -P 8"); m["tcp8_gbps"] = j and j["end"]["sum_received"]["bits_per_second"] / 1e9
        j = iperf("-u -R -b 0 -l 1400 -t 5")
        if j:
            x = j["end"]["sum"]
            m["udp1400_rev_kpps"] = (x["packets"] - x["lost_packets"]) / x["seconds"] / 1000
            m["udp1400_rev_loss_pct"] = x["lost_percent"]
        for size in (64, 1400):
            j = iperf(f"-u -b 0 -l {size} -t 5")
            if j:
                s = j["end"]["sum"]
                m[f"udp{size}_kpps"] = (s["packets"] - s["lost_packets"]) / s["seconds"] / 1000
                m[f"udp{size}_loss_pct"] = s["lost_percent"]
    # short connections: max rate
    c = tool(None, "churn target=127.0.0.1:6001 conns=64 secs=8 size=64 threads=4")
    m["churn_cps"] = c["rate"]; m["churn_err"] = c["total_err"]
    m["churn_p50_us"] = c["lat_us"]["p50"]; m["churn_p99_us"] = c["lat_us"]["p99"]; m["churn_ok"] = c["ok"]
    # fixed rate: cpu cost per connection and latency at moderate load
    t0 = cpu_ticks(pid)
    c = tool(None, "churn target=127.0.0.1:6001 conns=32 secs=8 size=64 rate=5000 threads=4")
    m["fixed_cps"] = c["rate"]; m["fixed_err"] = c["total_err"]
    m["fixed_cpu_ms_per_1k_conn"] = (cpu_ticks(pid) - t0) * 10 / (c["total_ok"] / 1000)
    m["fixed_p50_us"] = c["lat_us"]["p50"]; m["fixed_p99_us"] = c["lat_us"]["p99"]
    # 1MiB echo bodies: integrity + throughput through the tcp relay path
    c = tool(None, "churn target=127.0.0.1:6001 conns=16 secs=5 size=1048576 threads=4")
    m["xfer_mib_s"] = c["total_ok"] * 2 / c["elapsed"]; m["xfer_err"] = c["total_err"]
    m["rss_after_kb"] = rss_kb(pid); m["fd_after"] = nfd(pid)
    r.kill()
    time.sleep(0.5)
    return m

if os.environ.get("PERF_VARIANTS"):
    VARIANTS = [v for v in VARIANTS if v[0] in os.environ["PERF_VARIANTS"].split(",")]
runs = {v[0]: [] for v in VARIANTS}
for i in range(ROUNDS):
    for name, binary, conf in VARIANTS:
        m = bench(name, binary, conf)
        runs[name].append(m)
        print(i, name, json.dumps({k: (round(v, 2) if isinstance(v, float) else v) for k, v in m.items()}), flush=True)

summary = {}
for name, ms_ in runs.items():
    summary[name] = {}
    for k in ms_[0]:
        vals = [m[k] for m in ms_ if isinstance(m.get(k), (int, float))]
        if vals:
            summary[name][k] = {"median": statistics.median(vals), "min": min(vals), "max": max(vals)}
json.dump({"runs": runs, "summary": summary}, open(f"{OUT}/{os.environ.get('PERF_NAME', 'perf')}.json", "w"), indent=1)
print("\n== median (min..max) ==")
keys = list(next(iter(runs.values()))[0].keys())
for k in keys:
    row = []
    for name in runs:
        s = summary[name].get(k)
        row.append(f"{name}={s['median']:.2f} ({s['min']:.2f}..{s['max']:.2f})" if s else f"{name}=-")
    print(f"{k:28s} " + " | ".join(row))
srv_a.kill(); srv_b.kill(); sh("pkill iperf3", check=False)
