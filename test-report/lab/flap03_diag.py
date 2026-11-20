"""Fast flapping (0.3s down / 0.3s up): when do client timeouts happen relative to the switches?"""
import json, re, sys, time
from lab import *
from scen_fo import Case, FAST, R, link
net_up()
out = []
for rnd in range(3):
    for name, binary in (("旧规则（清零）", f"{BIN}/realm-fix3"), ("最终版", f"{BIN}/realm-fix4")):
        c = Case(fo_conf(**FAST, fall=3, udp_send="hc:"), binary=binary, level="debug")
        secs = 30
        c.run("tcp", f"churn target={R} conns=64 rate=200 secs={secs} timeout=8000")
        c.run("udp", f"udp target={R} socks=20 pps=200 secs={secs}")
        c.at(3)
        t0 = time.time()
        flaps = 0
        while time.time() - c.t0 < secs - 6:
            link("a", False); time.sleep(0.3); link("a", True); time.sleep(0.3)
            flaps += 1
        t1 = time.time()
        tcp, udp = c.res("tcp"), c.res("udp")
        seq = {"tcp": "", "udp": ""}
        for t, x in c.r.lines:
            m = re.search(r"\[failover\]\[(tcp|udp)\]\S+ probe: \[(\w+), (\w+)\]", x)
            if m and t0 <= t <= t1:
                seq[m.group(1)] += m.group(2)[0]
        ev = [(round(e[0] - t0, 1), e[1], e[3].split("]")[-1].strip()[-40:]) for e in c.ev()]
        a0 = rel_ms(tcp, t0)
        # an error is recorded when the client gives up: 8s after the request was sent
        errs = [(round((t - a0) / 1000, 1), sum(e)) for t, _s, _o, e, _l in tcp["buckets"] if sum(e)]
        row = {"round": rnd, "config": name, "flaps": flaps, "flap_end_s": round(t1 - t0, 1), "events": ev,
               "tcp_err": tcp["total_err"], "tcp_sent": tcp["sent"], "err_kinds": count(tcp)["err"], "err_recorded_at_s": errs,
               "dist": count(tcp)["ok"], "udp_delivered": round(udp["total_ok"] / udp["sent"], 3),
               "probe_tcp": seq["tcp"], "probe_udp": seq["udp"]}
        c.close()
        out.append(row)
        print(json.dumps(row, ensure_ascii=False), flush=True)
json.dump(out, open(f"{OUT}/flap03-diag.json", "w"), indent=1, ensure_ascii=False)
net_down()
