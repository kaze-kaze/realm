"""Random-phase flapping of the primary: which settings fail over, and what does traffic see?"""
import json, random, re, sys, time
from lab import *
from scen_fo import Case, FAST, R, link
net_up()
out = []
for mean in (1.0, 2.0):
    for name, binary, fall in (("默认 fall=3", f"{BIN}/realm-fix2", 3), ("去掉清零 fall=3", f"{BIN}/realm-noclear", 3), ("fall=2", f"{BIN}/realm-fix2", 2)):
        for seed in (1, 2):
            rnd = random.Random(seed)
            c = Case(fo_conf(**FAST, fall=fall, udp_send="hc:"), binary=binary, level="debug")
            secs = 40
            c.run("tcp", f"churn target={R} conns=64 rate=200 secs={secs} timeout=8000")
            c.run("udp", f"udp target={R} socks=20 pps=200 secs={secs}")
            c.at(3)
            t0 = time.time()
            flaps = 0
            while time.time() - c.t0 < secs - 4:
                link("a", False); time.sleep(rnd.uniform(0.7, 1.3) * mean)
                link("a", True); time.sleep(rnd.uniform(0.7, 1.3) * mean)
                flaps += 1
            tcp, udp = c.res("tcp"), c.res("udp")
            seq = ""
            for t, x in c.r.lines:
                m = re.search(r"\[failover\]\[udp\]\S+ probe: \[(\w+), (\w+)\]", x)
                if m and t >= t0:
                    seq += m.group(1)[0]
            first = c.ev("udp", "switch")
            row = {"mean_s": mean, "config": name, "seed": seed, "flaps": flaps, "udp_probe_fail_ratio": round(seq.count("F") / max(1, len(seq)), 2),
                   "udp_switches": len(first), "first_switch_s": round(first[0][0] - t0, 1) if first else None,
                   "tcp_switches": len(c.ev("tcp", "switch")), "udp_delivered": round(udp["total_ok"] / udp["sent"], 3),
                   "tcp_err": tcp["total_err"], "tcp_done": tcp["total_ok"], "tcp_max_ms": count(tcp)["lat_max_ms"]}
            c.close()
            out.append(row)
            print(json.dumps(row, ensure_ascii=False), flush=True)
json.dump(out, open(f"{OUT}/flap2.json", "w"), indent=1, ensure_ascii=False)
net_down()
