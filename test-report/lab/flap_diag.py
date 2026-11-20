import re, sys, time
from lab import *
from scen_fo import Case, FAST, R, link
net_up()
for name, binary in (("clear=2", f"{BIN}/realm-fix2"), ("noclear", f"{BIN}/realm-noclear")):
    c = Case(fo_conf(**FAST, udp_send="hc:"), binary=binary, level="debug")
    c.at(2)
    t0 = time.time()
    while time.time() - t0 < 16:
        link("a", False); time.sleep(1); link("a", True); time.sleep(1)
    seq = {"tcp": "", "udp": ""}
    for t, x in c.r.lines:
        m = re.search(r"\[failover\]\[(tcp|udp)\]\S+ probe: \[(\w+), (\w+)\]", x)
        if m and t >= t0:
            seq[m.group(1)] += m.group(2)[0]
    ev = [e[3] for e in c.ev()]
    c.close()
    print(name, "udp:", seq["udp"], "| tcp:", seq["tcp"], "| events:", ev, flush=True)
net_down()
