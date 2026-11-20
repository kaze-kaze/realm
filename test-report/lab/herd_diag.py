"""Why do a few of 5000 reconnecting clients get dropped twice when the primary dies?"""
import re, time
from collections import Counter
from lab import *
from scen_fo import Case, FAST, R
net_up()
c = Case(fo_conf(**FAST, udp_send="hc:"))
c.run("hold", f"hold target={R} conns=5000 secs=22 interval=1000 ramp=4000 timeout=10000 reconnect=1 threads=6")
c.at(12)
c.mark("kill")
c.kill("a")
hold = c.res("hold")
m = rel_ms(hold, c.marks["kill"])
errs = Counter()
for t, x in c.r.lines:
    if t >= c.marks["kill"] and ("ERROR" in x or "WARN" in x) and "[failover]" not in x:
        msg = x.split("]", 4)[-1]
        msg = re.sub(r"\d+\.\d+\.\d+\.\d+:\d+", "ADDR", msg)
        errs[msg[:110]] += 1
print("died", hold["died"], "errors by second after kill:", [(round((t - m) / 1000, 1), sum(e)) for t, _s, _o, e, _l in hold["buckets"] if sum(e)][:14])
for k, v in errs.most_common(8):
    print(v, k)
print("overflow", nsx("r", "nstat -az TcpExtListenOverflows TcpExtListenDrops TcpExtTCPAbortOnData TcpExtTCPAbortOnClose | tail -4").replace("\n", " | "))
print("backend b", nsx("b", "nstat -az TcpExtListenOverflows TcpExtListenDrops | tail -2").replace("\n", " | "))
c.close()
net_down()
