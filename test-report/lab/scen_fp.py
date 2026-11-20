"""False positive statistics: many endpoints probing a healthy primary through an impaired link.
Usage: scen_fp.py [seconds] [endpoints-per-level]"""
import json, os, re, sys, time
from lab import *

SECS = int(sys.argv[1]) if len(sys.argv) > 1 else 600
N = int(sys.argv[2]) if len(sys.argv) > 2 else 30
# optional extra failover keys, e.g. "fall = 2", and an output name
EXTRA = sys.argv[3] if len(sys.argv) > 3 else ""
NAME = sys.argv[4] if len(sys.argv) > 4 else "fp"

LEVELS = [
    ("clean", None, "无损"),
    ("l1", "loss 1%", "随机丢包 1%"),
    ("l2", "loss 2%", "随机丢包 2%"),
    ("l5", "loss 5%", "随机丢包 5%"),
    ("l10", "loss 10%", "随机丢包 10%"),
    ("l20", "loss 20%", "随机丢包 20%"),
    ("l30", "loss 30%", "随机丢包 30%"),
    ("burst", "loss gemodel 2% 30% 80% 0%", "突发丢包（平均约 5%，成串）"),
    ("j1", "delay 100ms 80ms", "延迟 100±80ms"),
    ("j2", "delay 300ms 200ms", "延迟 300±200ms"),
    ("j3", "delay 500ms 300ms loss 2%", "延迟 500±300ms + 丢包 2%"),
]
if os.environ.get("FP_LEVELS"):
    LEVELS = [l for l in LEVELS if l[0] in os.environ["FP_LEVELS"].split(",")]
hosts = {name: 20 + i for i, (name, _, _) in enumerate(LEVELS)}
net_up(hosts)
procs = [backend("b", "B")]
conf = "[network]\nuse_udp = true\ntcp_timeout = 4\n"
port = 7000
owner = {}
for name, spec, _ in LEVELS:
    procs.append(Proc(name, f"{FOT} srv tag=A tcp=0.0.0.0:5000 udp=0.0.0.0:5000 threads=2", "fot"))
    if spec:
        netem(name, spec)
    for _ in range(N):
        owner[port] = name
        conf += f"""
[[endpoints]]
listen = "0.0.0.0:{port}"
remote = "{ip4(name)}:5000"
extra_remotes = ["{ip4('b')}:5000"]
[endpoints.failover]
tcp_send = "ping"
tcp_expect = "OK"
udp_send = "hc:"
{EXTRA}
"""
        port += 1
time.sleep(0.5)
r = realm(conf, level="warn", name=NAME)
t0 = time.time()
cpu0 = cpu_ticks(r.pid())
print(f"running {SECS}s with {N} endpoints x {len(LEVELS)} levels", flush=True)
time.sleep(SECS)
alive = r.alive()
rss = rss_kb(r.pid())
cpu_pct = (cpu_ticks(r.pid()) - cpu0) / (SECS * 100) * 100
thr, fds = nthreads(r.pid()), nfd(r.pid())
r.kill()
pat = re.compile(r"\[failover\]\[(tcp|udp)\]0\.0\.0\.0:(\d+) (?:peer#(\d) \S+ is (down|up)|switch peer#(\d))")
agg = {name: {"tcp": {"down0": 0, "down1": 0, "switch": 0, "back": 0}, "udp": {"down0": 0, "down1": 0, "switch": 0, "back": 0}} for name, _, _ in LEVELS}
for _, line in r.lines:
    m = pat.search(line)
    if not m:
        continue
    proto, p, peer, state, sw_from = m.groups()
    a = agg[owner[int(p)]][proto]
    if state == "down":
        a["down" + peer] += 1
    elif sw_from == "0":
        a["switch"] += 1
    elif sw_from == "1":
        a["back"] += 1
rounds = SECS / 3
out = {"secs": SECS, "endpoints": N, "rounds_per_endpoint": rounds, "alive": alive, "rss_kb": rss, "cpu_pct_of_one_core": cpu_pct, "threads": thr, "fds": fds, "monitors": 2 * N * len(LEVELS), "levels": []}
print(f"alive={alive} rss={rss}kB cpu={cpu_pct:.2f}% of one core, {2 * N * len(LEVELS)} monitors, threads={thr}, fds={fds}")
print(f"{'level':34s} {'tcp误判下线':>10s} {'tcp误切':>8s} {'udp误判下线':>10s} {'udp误切':>8s}  每端点每天(tcp/udp)")
for name, spec, label in LEVELS:
    a = agg[name]
    per_day = lambda n: n / N / SECS * 86400
    row = {"name": name, "label": label, "spec": spec, **{f"{p}_{k}": v for p in a for k, v in a[p].items()},
           "tcp_switch_per_day": per_day(a["tcp"]["switch"]), "udp_switch_per_day": per_day(a["udp"]["switch"])}
    out["levels"].append(row)
    print(f"{label:34s} {a['tcp']['down0']:10d} {a['tcp']['switch']:8d} {a['udp']['down0']:10d} {a['udp']['switch']:8d}  {row['tcp_switch_per_day']:.1f} / {row['udp_switch_per_day']:.1f}   backup-down tcp={a['tcp']['down1']} udp={a['udp']['down1']}")
out["extra"] = EXTRA
json.dump(out, open(f"{OUT}/{NAME}.json", "w"), indent=1, ensure_ascii=False)
for p in procs:
    p.kill()
net_down()
