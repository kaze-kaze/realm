"""Backend alive but frozen (not reading) while uploads are pending: when does the relay give up?"""
import json, os, signal, sys, time
from lab import *
from scen_fo import Case, FAST, R
net_up()
out = {}
for name, binary in (("before", f"{BIN}/realm-fix"), ("after", f"{BIN}/realm-fix2")):
    c = Case(fo_conf(**FAST, tcp_send="ping", tcp_expect="OK", udp_send="hc:"), binary=binary)
    c.run("tcp", f"churn target={R} conns=8 secs=100 size=8388608 timeout=95000 bucket=1000 threads=2")
    c.at(4)
    c.mark("freeze")
    os.kill(c.back["a"].pid(), signal.SIGSTOP)
    samples = []
    while time.time() - c.t0 < 98:
        n = nsx("r", f"ss -Htn state established dst {ip4('a')}:5000 | wc -l", check=False)
        samples.append((round(time.time() - c.marks["freeze"]), int(n or 0)))
        time.sleep(5)
    tcp = c.res("tcp")
    m = rel_ms(tcp, c.marks["freeze"])
    errs = [(round((t - m) / 1000), sum(e)) for t, _s, _o, e, _l in tcp["buckets"] if sum(e)]
    sw = c.delay("freeze", "tcp")
    os.kill(c.back["a"].pid(), signal.SIGCONT)
    c.close()
    out[name] = {"switch_s": sw, "stuck_uploads_failed_at_s": errs[:6], "relay_sockets_to_frozen_peer": samples, "errs": tcp["first_errs"][:2], "ok_after": count(tcp, m + 3000)["ok"]}
    print(name, json.dumps(out[name], ensure_ascii=False), flush=True)
json.dump(out, open(f"{OUT}/zero-window.json", "w"), indent=1, ensure_ascii=False)
net_down()
