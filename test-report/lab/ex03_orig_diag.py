"""The control half of EX-03 (unmodified v2.9.6, 5000 udp sessions, no fault) sometimes collapses. How often, and what does it look like?
Usage: ex03_orig_diag.py [runs] [binary]"""
import json, sys, time
from collections import Counter
from lab import *
from scen_fo import Case, R
from extreme import PLAIN, sample
RUNS = int(sys.argv[1]) if len(sys.argv) > 1 else 6
BINARY = sys.argv[2] if len(sys.argv) > 2 else ORIG
net_up()
out = []
for i in range(RUNS):
    c = Case(PLAIN, binary=BINARY, level="info")
    b0 = sample(c.pid)
    c.run("udp", f"udp target={R} socks=5000 pps=25000 secs=10 spread=3000 threads=6")
    tl = []
    while time.time() - c.t0 < 9.5:
        ports = nsx("r", "ss -Huan", check=False).split("\n")
        local = [x.split()[3] for x in ports if len(x.split()) > 3]
        dup = sum(v for v in Counter(local).values() if v > 1)
        tl.append((round(time.time() - c.t0, 1), nfd(c.pid), rss_kb(c.pid) // 1024, len(local), dup))
        time.sleep(0.8)
    u = c.res("udp")
    kinds = Counter()
    for _, x in c.r.lines:
        if "DEBUG" in x:
            continue
        k = x.split("]", 2)[-1]
        k = "".join(ch for ch in k if not ch.isdigit())[:70]
        kinds[k] += 1
    drops = nsx("r", "nstat -az UdpRcvbufErrors UdpSndbufErrors UdpInErrors UdpNoPorts", check=False).split("\n")
    row = {"run": i, "binary": BINARY.split("/")[-1], "delivered": f"{u['total_ok']}/{u['sent']}", "ratio": round(u["total_ok"] / u["sent"], 3), "foreign": u["foreign"], "dup": u["dup"],
           "timeline_t_fd_rssMB_udpsocks_sharedports": tl, "log_kinds": dict(kinds.most_common(6)),
           "nstat": [" ".join(x.split()[:2]) for x in drops if x.startswith("Udp")], "alive": c.close()}
    out.append(row)
    print(json.dumps(row, ensure_ascii=False), flush=True)
json.dump(out, open(f"{OUT}/ex03-orig-diag.json", "w"), indent=1, ensure_ascii=False)
net_down()
