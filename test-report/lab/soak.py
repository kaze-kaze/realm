"""Soak: mixed tcp/udp load with random faults; watch memory, handles, threads, data integrity.
Usage: soak.py [seconds]"""
import json
import os
import random
import sys
import time

from lab import *
from scen_fo import FAST, R, reset_faults

SECS = int(sys.argv[1]) if len(sys.argv) > 1 else 1200
random.seed(20261119)
net_up()
back = {"a": backend("a", "A"), "b": backend("b", "B")}
conf = fo_conf(**FAST, tcp_send="ping", tcp_expect="OK", udp_send="hc:", udp_timeout=5)
# SOAK_PLAIN=1: no failover, single remote. Lets the original build take the same load as a control.
if os.environ.get("SOAK_PLAIN"):
    conf = f"""
[network]
use_udp = true
tcp_timeout = 4
udp_timeout = 5
[[endpoints]]
listen = "0.0.0.0:6000"
remote = "{ip4('a')}:5000"
"""
r = realm(conf, level="warn", name=os.environ.get("SOAK_NAME", "soak"), binary=os.environ.get("SOAK_BIN", NEW))
pid = r.pid()
time.sleep(2)
base = {"rss_kb": rss_kb(pid), "fd": nfd(pid), "threads": nthreads(pid)}
t0 = time.time()

load = {
    "small": tool("c", f"churn target={R} conns=32 rate=500 secs={SECS} size=64 timeout=8000", bg=True),
    "large": tool("c", f"churn target={R} conns=8 rate=10 secs={SECS} size=262144 timeout=15000", bg=True),
    "hold": tool("c", f"hold target={R} conns=500 secs={SECS} interval=1000 ramp=3000 timeout=10000 reconnect=1", bg=True),
    "udp": tool("c", f"udp target={R} socks=200 pps=2000 secs={SECS}", bg=True),
}
# SOAK_ONLY=tcp / udp: run only that half of the load, to attribute memory growth
ONLY = os.environ.get("SOAK_ONLY")
if ONLY == "tcp":
    load.pop("udp").kill()
elif ONLY == "udp":
    for k in ("small", "large", "hold"):
        load.pop(k).kill()
NAME = os.environ.get("SOAK_NAME", "soak")

faults = []
samples = []


def restart(ns):
    back[ns].kill()
    time.sleep(0.1)
    back[ns] = backend(ns, "A" if ns == "a" else "B")


def f_kill_a():
    back["a"].kill(); time.sleep(random.uniform(3, 8)); back["a"] = backend("a", "A")

def f_kill_b():
    back["b"].kill(); time.sleep(random.uniform(3, 8)); back["b"] = backend("b", "B")

def f_blackhole_a():
    blackhole("a"); time.sleep(random.uniform(4, 8)); blackhole("a", False)

def f_mute_a():
    ctl("a", "tcp.mode=mute", "udp.mode=mute"); time.sleep(random.uniform(4, 7)); ctl("a", "tcp.mode=ok", "udp.mode=ok")

def f_loss_a():
    netem("a", "loss 25%"); time.sleep(random.uniform(6, 10)); netem_clear("a")

def f_both_down():
    back["a"].kill(); back["b"].kill(); time.sleep(random.uniform(2, 5))
    back["a"] = backend("a", "A"); back["b"] = backend("b", "B")

def f_flap_a():
    for _ in range(4):
        link("a", False); time.sleep(0.7); link("a", True); time.sleep(0.7)

def f_slow_a():
    netem("a", "delay 250ms 100ms"); time.sleep(random.uniform(5, 8)); netem_clear("a")

FAULTS = [f_kill_a, f_kill_b, f_blackhole_a, f_mute_a, f_loss_a, f_both_down, f_flap_a, f_slow_a]

next_sample = 0
next_fault = 15
while time.time() - t0 < SECS - 25:
    now = time.time() - t0
    if now >= next_sample:
        s = {"t": round(now), "rss_kb": rss_kb(pid), "fd": nfd(pid), "threads": nthreads(pid), "cpu": cpu_ticks(pid)}
        samples.append(s)
        next_sample += 10
    if now >= next_fault:
        f = random.choice(FAULTS)
        faults.append((round(now), f.__name__))
        print(f"t={now:.0f}s fault {f.__name__} rss={rss_kb(pid)} fd={nfd(pid)}", flush=True)
        f()
        next_fault = (time.time() - t0) + random.uniform(12, 25)
    time.sleep(0.5)
    if not r.alive():
        print("REALM DIED", flush=True)
        break

reset_faults()
res = {k: done(p, timeout=120) for k, p in load.items()}
time.sleep(10)  # udp_timeout 5s + margin: everything must be released
end = {"rss_kb": rss_kb(pid), "fd": nfd(pid), "threads": nthreads(pid)}
alive = r.alive()
ev = events(r)
panics = [x for _, x in r.lines if "panic" in x.lower()]
r.kill()
for b in back.values():
    b.kill()

summ = {
    "secs": SECS, "alive": alive, "base": base, "end": end, "faults": faults, "samples": samples,
    "switches": {"tcp": sum(1 for e in ev if e[1] == "tcp" and e[2] == "switch"), "udp": sum(1 for e in ev if e[1] == "udp" and e[2] == "switch")},
    "downs": sum(1 for e in ev if e[2] == "down"), "panics": panics,
    "load": {k: {"sent": v["sent"], "ok": v["total_ok"], "err": {a: b for a, b in v["err"].items() if b}, "dist": {a: b for a, b in v["ok"].items() if b},
                 "p50_us": v["lat_us"]["p50"], "p99_us": v["lat_us"]["p99"],
                 **{x: v[x] for x in ("died", "tag_changed", "alive_at_end", "stale", "dup", "foreign") if x in v}} for k, v in res.items()},
    "rss_min": min(s["rss_kb"] for s in samples), "rss_max": max(s["rss_kb"] for s in samples),
    "rss_first_third": sum(s["rss_kb"] for s in samples[: len(samples) // 3]) / max(1, len(samples) // 3),
    "rss_last_third": sum(s["rss_kb"] for s in samples[-(len(samples) // 3):]) / max(1, len(samples) // 3),
    "fd_max": max(s["fd"] for s in samples),
    "cpu_avg_pct": (samples[-1]["cpu"] - samples[0]["cpu"]) / max(1, samples[-1]["t"] - samples[0]["t"]),
}
json.dump(summ, open(f"{OUT}/{NAME}.json", "w"), indent=1, ensure_ascii=False)
print(json.dumps({k: v for k, v in summ.items() if k not in ("samples", "faults")}, indent=1, ensure_ascii=False))
print("faults:", len(faults))
net_down()
