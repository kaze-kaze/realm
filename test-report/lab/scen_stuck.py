"""How long do established tcp connections on a silently dead primary stay stuck?
Usage: scen_stuck.py [seconds]"""
import json, sys, time
from lab import *
from scen_fo import Case, FAST, R

SECS = int(sys.argv[1]) if len(sys.argv) > 1 else 150
net_up()
out = {}
NAME = sys.argv[2] if len(sys.argv) > 2 else "stuck"
for name, idle in (("busy", 0), ("idle", 1)):
    # busy: every connection has a request outstanding when the primary vanishes
    # idle: connections are silent after their first request
    c = Case(fo_conf(**FAST, tcp_send="ping", tcp_expect="OK", udp_send="hc:"))
    c.run("hold", f"hold target={R} conns=50 secs={SECS} interval=1000 idle={idle} ramp=500 timeout={(SECS - 12) * 1000} bucket=1000")
    c.at(6)
    c.mark("drop")
    blackhole("a")
    # sample the relay's sockets towards the dead primary
    samples = []
    while time.time() - c.t0 < SECS - 2:
        n = nsx("r", f"ss -Htn dst {ip4('a')}:5000 | wc -l", check=False)
        samples.append((round(time.time() - c.marks["drop"], 1), int(n or 0)))
        time.sleep(5)
    hold = c.res("hold")
    sw = c.delay("drop", "tcp")
    m = rel_ms(hold, c.marks["drop"])
    deaths = [(round((t - m) / 1000), sum(e)) for t, _s, _o, e, _l in hold["buckets"] if sum(e)]
    c.close()
    gone = next((t for t, n in samples if n == 0), None)
    out[name] = {"switch_s": sw, "died": hold["died"], "alive_at_end": hold["alive_at_end"], "deaths_at_s": deaths[:12],
                 "relay_sockets_to_dead_peer": samples, "all_closed_after_s": gone, "errs": hold["first_errs"][:3]}
    print(name, json.dumps(out[name], ensure_ascii=False), flush=True)
json.dump(out, open(f"{OUT}/{NAME}.json", "w"), indent=1, ensure_ascii=False)
net_down()
