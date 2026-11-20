"""Fast flapping (0.3s/0.3s): when every probe fails in a row, is the primary really unreachable?
An independent ping (every 50ms, from the relay namespace) and the neighbour tables are recorded next to the probe log."""
import json, re, sys, threading, time
from collections import Counter
from lab import *
from scen_fo import Case, FAST, R, link
net_up()
MAXR = int(sys.argv[1]) if len(sys.argv) > 1 else 14
caught = 0
out = []
for rnd in range(MAXR):
    c = Case(fo_conf(**FAST, fall=3, udp_send="hc:"), level="debug")
    secs = 30
    c.run("tcp", f"churn target={R} conns=64 rate=200 secs={secs} timeout=8000")
    c.at(3)
    ping = Proc("r", f"ping -D -n -i 0.05 -W 0.2 {ip4('a')}", "ping")
    neigh = []
    stop = threading.Event()
    def sample():
        while not stop.is_set():
            neigh.append((time.time(), sh(f"ip netns exec r ip neigh show {ip4('a')}", check=False), sh(f"ip netns exec a ip neigh show {ip4('r')}", check=False)))
            time.sleep(0.25)
    th = threading.Thread(target=sample, daemon=True); th.start()
    t0 = time.time()
    flaps = 0
    while time.time() - c.t0 < secs - 6:
        link("a", False); time.sleep(0.3); link("a", True); time.sleep(0.3)
        flaps += 1
    t1 = time.time()
    stop.set(); th.join()
    ping.kill()
    tcp = c.res("tcp")
    probes = []  # (t, result of primary)
    for t, x in c.r.lines:
        m = re.search(r"\[failover\]\[tcp\]\S+ probe: \[(\w+), (\w+)\]", x)
        if m and t0 <= t <= t1:
            probes.append((t, m.group(1)[0]))
    seq = "".join(p for _, p in probes)
    replies = []
    for t, x in ping.lines:
        m = re.match(r"\[(\d+\.\d+)\] \d+ bytes from", x)
        if m:
            replies.append(float(m.group(1)))
    ratio = lambda a, b: round(sum(1 for t in replies if a <= t < b) / max(1, (b - a) / 0.05), 2)
    row = {"round": rnd, "flaps": flaps, "probe_tcp": seq, "switches": len(c.ev("tcp", "switch")), "tcp_err": tcp["total_err"], "tcp_sent": tcp["sent"],
           "ping_reply_ratio_whole": ratio(t0, t1)}
    m = re.search(r"F{6,}", seq)
    if m:
        caught += 1
        a, b = probes[m.start()][0], probes[m.end() - 1][0]
        msgs = Counter()
        for t, x in c.r.lines:
            mm = re.search(r"\[failover\]\[(tcp|udp)\]probe (\S+): (.*)$", x)
            if mm and a - 0.5 <= t <= b and mm.group(2).startswith(ip4("a")):
                msgs[f"{mm.group(1)}: {mm.group(3)}"] += 1
        row.update({"stuck_from_s": round(a - t0, 1), "stuck_to_s": round(b - t0, 1), "ping_reply_ratio_before_stuck": ratio(t0, a), "ping_reply_ratio_in_stuck": ratio(a, b),
                    "probe_failure_reasons_in_stuck": dict(msgs),
                    "neigh_r_about_a_in_stuck": dict(Counter(" ".join(x.split()[-1:]) or "none" for t, x, _ in neigh if a <= t <= b)),
                    "neigh_a_about_r_in_stuck": dict(Counter(" ".join(y.split()[-1:]) or "none" for t, _, y in neigh if a <= t <= b)),
                    "neigh_r_about_a_before": dict(Counter(" ".join(x.split()[-1:]) or "none" for t, x, _ in neigh if t < a)),
                    "link_state_a_now": sh("ip -o link show v-a | cut -c1-120", check=False)})
    c.close()
    out.append(row)
    print(json.dumps(row, ensure_ascii=False), flush=True)
    if caught >= 2:
        break
json.dump(out, open(f"{OUT}/flap03-diag2.json", "w"), indent=1, ensure_ascii=False)
net_down()
