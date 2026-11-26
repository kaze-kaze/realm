#!/usr/bin/env python3
"""Turn results/rf02-*.json into results/rf02-summary.md."""
import glob
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
ORDER = ["new-default", "new-timeout0", "new-timeout2", "new-v4first-timeout0", "new-flap-timeout0", "new-flap-timeout2",
         "new-mode-6then4", "new-nodata", "dbg-default", "dbg-timeout0", "old-default", "old-timeout0",
         "old-v4first-default", "old-v4first-timeout0", "old-nodata", "new-through-v4", "new-through-v6",
         "old-through-v4", "old-through-v6", "new-failover", "dbg-failover", "old-failover"]
out = []
for label in ORDER:
    p = os.path.join(HERE, "results", "rf02-%s.json" % label)
    if not os.path.exists(p):
        continue
    r = json.load(open(p))
    out.append("### %s (binary %s, options %s)" % (label, r["binary"], json.dumps(r["options"])))
    out.append("")
    if r.get("error"):
        out.append("ERROR: " + r["error"])
        continue
    out.append("| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |")
    out.append("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |")
    for ph in r["phases"]:
        socks = [n for _, n in ph["udp_sockets"]]
        for i, s in enumerate(ph["clients"]):
            late = sum(1 for e in ph["events"] if e[1] == i and "stray reply" in e[2])
            v4 = s["v4"] + s.get("b4", 0)
            out.append("| %s | %g | %d | %d | %d | %d | %d | %d | %s | %s | %s |" % (
                ph["dns"], ph["seconds"], i, s["sent"], v4, s["v6"], s["lost"], late,
                ", ".join("%s %.2f" % kv for kv in sorted(s["first"].items())) or "-",
                " ".join(str(x) for x in s["ports"]) or "-",
                "%s..%s (max %s)" % (socks[0], socks[-1], max(socks)) if socks else "-"))
        if "tcp" in ph:
            out.append("| | | tcp | | | | | | %s | | |" % ph["tcp"])
    out.append("")
    assoc = [l.split("]", 4)[-1] for l in r["log_assoc"] if "new association" in l]
    out.append("log: %d `new association` lines, %d `remove association` lines; resolution changes: %s" % (
        len(assoc), sum(1 for l in r["log_assoc"] if "remove association" in l),
        " -> ".join(l.split("resolved as ")[1] for l in r["log_resolution_changes"]) or "-"))
    warn = [l for l in r["log_assoc"] if "WARN" in l or "ERROR" in l]
    if warn:
        out.append("")
        out.append("log warnings/errors (first 3): " + " ;; ".join(w.split("]", 3)[-1] for w in warn[:3]))
    if r.get("stderr", "").strip():
        out.append("")
        out.append("stderr: " + r["stderr"].strip()[:300])
    out.append("alive at end: %s" % r.get("alive_at_end"))
    out.append("")
open(os.path.join(HERE, "results", "rf02-summary.md"), "w").write("\n".join(out) + "\n")
print("\n".join(out))
