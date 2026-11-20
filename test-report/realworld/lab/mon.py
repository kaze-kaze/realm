#!/usr/bin/env python3
"""Relay-side sampler: realm processes (rss, cpu, fds, threads) and host UDP counters, one JSON line per 5 s."""
import json
import os
import time

RUN = os.environ.get("RBT_RUN", "/rbt/run")
TICK = os.sysconf("SC_CLK_TCK")


def procs():
    out = {}
    for pid in os.listdir("/proc"):
        if not pid.isdigit():
            continue
        try:
            comm = open("/proc/%s/comm" % pid).read().strip()
            if not comm.startswith("realm"):
                continue
            comm = "realmog" if comm == "realmog" else "realm3"
            st = open("/proc/%s/stat" % pid).read().rsplit(")", 1)[1].split()
            rss = 0
            for line in open("/proc/%s/status" % pid):
                if line.startswith("VmRSS"):
                    rss = int(line.split()[1])
            out[comm] = {"pid": int(pid), "cpu_s": round((int(st[11]) + int(st[12])) / TICK, 2), "threads": int(st[17]),
                         "rss_kb": rss, "fds": len(os.listdir("/proc/%s/fd" % pid))}
        except (OSError, IndexError, ValueError):
            pass
    return out


def udp():
    lines = [l.split() for l in open("/proc/net/snmp") if l.startswith("Udp:")]
    return dict(zip(lines[0][1:], map(int, lines[1][1:])))


with open(RUN + "/relay/mon.jsonl", "a") as f:
    while True:
        u = udp()
        f.write(json.dumps({"t": round(time.time(), 1), "p": procs(),
                            "udp": {k: u.get(k) for k in ("InDatagrams", "OutDatagrams", "InErrors", "RcvbufErrors",
                                                          "SndbufErrors", "NoPorts")}}) + "\n")
        f.flush()
        time.sleep(5)
