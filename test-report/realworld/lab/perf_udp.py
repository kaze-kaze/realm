#!/usr/bin/env python3
"""Relay throughput on loopback, order balanced: the same iperf3 server reached through each
realm build in turn.  perf_udp.py <rounds> <name=binary> ...   (run inside rbt-relay)"""
import json
import os
import signal
import subprocess
import sys
import time

BIN = "/rbt/bin"
SRV = 26100


def iperf(port, *extra, secs=6):
    cmd = ["iperf3", "-c", "127.0.0.1", "-p", str(port), "-t", str(secs), "-J"] + list(extra)
    try:
        j = json.loads(subprocess.run(cmd, capture_output=True, text=True, timeout=secs + 30).stdout)
        if j.get("error"):
            return None
        return j["end"]
    except Exception:
        return None


def main():
    rounds = int(sys.argv[1])
    variants = [a.split("=") for a in sys.argv[2:]]
    procs = [subprocess.Popen(["iperf3", "-s", "-p", str(SRV)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)]
    ports = {}
    for i, (name, binary) in enumerate(variants):
        port = 26000 + i
        ports[name] = port
        cfg = "/tmp/perf-%s.toml" % name
        open(cfg, "w").write('[log]\nlevel = "error"\n[network]\nuse_udp = true\n[[endpoints]]\n'
                             'listen = "127.0.0.1:%d"\nremote = "127.0.0.1:%d"\n' % (port, SRV))
        procs.append(subprocess.Popen(["%s/%s" % (BIN, binary), "-c", cfg], stdout=subprocess.DEVNULL,
                                      stderr=subprocess.DEVNULL))
    time.sleep(1.5)
    tests = {
        "udp_up_1400": ["-u", "-b", "0", "-l", "1400"],
        "udp_down_1400": ["-u", "-b", "0", "-l", "1400", "-R"],
        "udp_up_200": ["-u", "-b", "0", "-l", "200"],
        "tcp_up": [],
        "tcp_down": ["-R"],
    }
    res = {t: {n: [] for n, _ in variants} for t in tests}
    try:
        for r in range(rounds):
            order = variants[r % len(variants):] + variants[:r % len(variants)]
            for t, args in tests.items():
                for name, _ in order:
                    e = iperf(ports[name], *args)
                    if not e:
                        continue
                    if args and args[0] == "-u":
                        s = e.get("sum_received") or e["sum"]
                        # delivered rate at the receiver
                        res[t][name].append(s["bits_per_second"] * (1 - s.get("lost_percent", 0) / 100.0)
                                            if "sum_received" not in e else s["bits_per_second"])
                    else:
                        res[t][name].append(e["sum_received"]["bits_per_second"])
            print("round", r, "done", flush=True)
    finally:
        for p in procs:
            p.send_signal(signal.SIGKILL)
    out = {}
    for t, by in res.items():
        line = "%-14s" % t
        for name, v in by.items():
            if v:
                v2 = sorted(v)
                med = v2[len(v2) // 2] / 1e6
                line += "  %s median %.0f (min %.0f max %.0f) Mbit/s n=%d" % (name, med, v2[0] / 1e6, v2[-1] / 1e6, len(v))
                out.setdefault(t, {})[name] = {"median_mbps": round(med), "min": round(v2[0] / 1e6), "max": round(v2[-1] / 1e6), "n": len(v)}
        print(line, flush=True)
    os.makedirs("/rbt/run/res", exist_ok=True)
    json.dump(out, open("/rbt/run/res/perf-udp.json", "w"), indent=1)


if __name__ == "__main__":
    main()
