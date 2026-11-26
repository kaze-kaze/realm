#!/usr/bin/env python3
"""RF-03 sweep: for every value of the family/transport byte and both commands,
send only the 16 byte fixed header, keep the connection open for 1s and record
whether realm closed it at once.

Expected from proxy-protocol 0.5.0 (version2.rs): close at once iff family > 3,
transport > 2, or declared length < the family minimum (0/12/36/216).
usage: python3 -I rf03b_fixed_header_sweep.py   -> results/rf03b.json
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import common as c
import rf03_proxy as r

r.PORT = 39631


def sweep(which, echo):
    conf = os.path.join(c.TMP, "rf03b-%s.toml" % which)
    with open(conf, "w") as f:
        f.write('[network]\naccept_proxy = true\n[[endpoints]]\nlisten = "127.0.0.1:%d"\nremote = "127.0.0.1:%d"\n'
                % (r.PORT, echo.port))
    out_p, err_p = os.path.join(c.TMP, "rf03b-%s.out" % which), os.path.join(c.TMP, "rf03b-%s.err" % which)
    proc = c.start(c.BINS[which], ["-c", conf], out_p, err_p)
    rows, wrong = [], []
    try:
        if not c.wait_tcp(r.PORT, 8):
            return {"error": c.read(err_p)[-300:]}
        for cmd in (0x20, 0x21):
            for b13 in range(256):
                fam, proto = b13 >> 4, b13 & 0x0F
                base = r.MIN.get(fam, 12)
                for length in sorted({base, max(base - 1, 0)}):
                    invalid = fam > 3 or proto > 2 or length < base
                    outcome, secs, _ = r.probe([r.fixed(cmd, b13, length)], hold=1.0)
                    closed_at_once = outcome.startswith("closed") and secs < 0.5
                    rows.append((cmd, b13, length, outcome, secs))
                    if closed_at_once != invalid:
                        wrong.append((cmd, b13, length, outcome, secs, "expected %s" % ("close at once" if invalid else "keep waiting")))
        alive = proc.poll() is None
    finally:
        c.stop(proc)
        for p in (conf, out_p, err_p):
            if os.path.exists(p):
                os.remove(p)
    summary = {}
    for row in rows:
        key = "%s%s" % (row[3], " <0.5s" if row[4] < 0.5 else "")
        summary[key] = summary.get(key, 0) + 1
    return {"probes": len(rows), "summary": summary, "unexpected": wrong, "alive_at_end": alive}


def main():
    echo = r.Echo()
    echo.start()
    out = {}
    for which in sys.argv[1:] or ("new", "dbg", "old"):
        out[which] = sweep(which, echo)
        print(which, out[which].get("probes"), out[which].get("summary"), "unexpected:", len(out[which].get("unexpected", [])), flush=True)
        for w in out[which].get("unexpected", [])[:10]:
            print("   ", w)
    echo.stop = True
    with open(os.path.join(c.WORK, "results", "rf03b.json"), "w") as f:
        json.dump(out, f, indent=1)


if __name__ == "__main__":
    try:
        main()
    finally:
        c.stop_all()
