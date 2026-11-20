"""Why does a plain (no failover) endpoint deliver so little with 5000 udp clients?"""
import json, sys, time
from lab import *
from scen_fo import Case, FAST, R
from extreme import PLAIN, sample
net_up()
for name, binary, conf, timeout in (("orig udp_timeout=5", ORIG, PLAIN, None), ("fix  udp_timeout=5", NEW, PLAIN, None),
                                    ("orig udp_timeout=30", ORIG, PLAIN.replace("udp_timeout = 5", "udp_timeout = 30"), None),
                                    ("fix  udp_timeout=30", NEW, PLAIN.replace("udp_timeout = 5", "udp_timeout = 30"), None)):
    c = Case(conf, binary=binary, level="warn")
    b0 = sample(c.pid)
    c.run("udp", f"udp target={R} socks=5000 pps=25000 secs=10 spread=3000 threads=6")
    c.at(8)
    p0 = sample(c.pid)
    u = c.res("udp")
    sa = stats("a")
    logs = {}
    for _, x in c.r.lines:
        if "ERROR" in x or "WARN" in x:
            k = x.split("]")[-1][:70]
            logs[k] = logs.get(k, 0) + 1
    drops = nsx("r", "nstat -az UdpRcvbufErrors UdpInErrors UdpSndbufErrors | tail -3").replace("\n", " | ")
    c.close()
    print(name, "delivered", u["total_ok"], "/", u["sent"], "rss", b0["rss_kb"] // 1024, "->", p0["rss_kb"] // 1024, "MB fd", p0["fd"],
          "backend got", sa["udp"]["data_pkts"], "p99", u["lat_us"]["p99"], "logs", dict(list(logs.items())[:4]), "| relay udp drops:", drops, flush=True)
net_down()
