#!/usr/bin/env python3
"""A series of failover drills on one schedule.  campaign.py <base-epoch> <relay> [period] [dur]

Drill k runs from base + k*period for dur seconds on fo-<relay> with p2-<relay> as control.
The landings get their faults from fault.sh sched with the same base (see FAULTS)."""
import json
import sys
import time

import suite

NAMES = ["drain", "refuse", "loss30", "delay800", "flap", "alldown", "loss10", "svcstop"]
PROTOS = ["vless-reality", "trojan-tls", "vless-grpc-tls", "ss-2022", "hy2", "wg", "tgt"]
# what an endpoint that probes the service ports themselves can carry
FD_PROTOS = ["vless-reality", "trojan-tls", "hy2"]


def faults(period=170, lead=30, length=60):
    """Fault schedule relative to base: (primary landing entries, backup landing entries)."""
    t = lambda k, d=0: k * period + lead + d
    prim = ["%d health bad" % t(0), "%d health good" % t(0, length),
            "%d rej on" % t(1), "%d rej off" % t(1, length),
            "%d loss 30" % t(2), "%d loss off" % t(2, length),
            "%d netem delay 800ms" % t(3), "%d netem off" % t(3, length)]
    for i in range(6):
        prim += ["%d bh on" % t(4, i * 10), "%d bh off" % t(4, i * 10 + 5)]
    prim += ["%d bh on" % t(5), "%d bh off" % t(5, 40),
             "%d loss 10" % t(6), "%d loss off" % t(6, length),
             "%d stop xray" % t(7), "%d stop hy2" % t(7), "%d start xray" % t(7, length), "%d start hy2" % t(7, length)]
    back = ["%d bh on" % t(5), "%d bh off" % t(5, 40)]
    return prim, back


if __name__ == "__main__":
    if sys.argv[1] == "faults":
        prim, back = faults()
        print("PRIMARY: " + " ".join('"%s"' % x for x in prim))
        print("BACKUP:  " + " ".join('"%s"' % x for x in back))
        sys.exit(0)
    base, relay = float(sys.argv[1]), sys.argv[2]
    period = int(sys.argv[3]) if len(sys.argv) > 3 else 170
    dur = int(sys.argv[4]) if len(sys.argv) > 4 else 150
    pairs = ([["fo-" + relay, p] for p in PROTOS] + [["p2-" + relay, p] for p in PROTOS]
             + [["fd-" + relay, p] for p in FD_PROTOS])
    for k, name in enumerate(NAMES):
        tag = "c-%s-%s" % (relay, name)
        res = suite.drill(pairs=pairs, dur=dur, fresh=(k == 0), start_at=base + k * period)
        suite.save(tag, {"suite": "drill", "me": suite.ME, "t": time.time(), "base": base, "k": k, "res": res})
        print("saved", tag, flush=True)
