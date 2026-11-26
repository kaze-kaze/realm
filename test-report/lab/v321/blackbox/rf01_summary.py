#!/usr/bin/env python3
"""Turn results/rf01.json into results/rf01-summary.md (one row per sample and entry)."""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
rows = json.load(open(os.path.join(HERE, "results", "rf01.json")))
extra = os.path.join(HERE, "results", "rf01-x_.json")  # second pass: `rf01_conf.py x_`
if os.path.exists(extra):
    rows += json.load(open(extra))


def state(y):
    return "started" if y["state"] == "running" else "exit %s" % y["rc"]


def reason(y, toml):
    ls = [l for l in y["stderr"] if not l.startswith("note: run with") and "unknown key" not in l
          and not l.startswith("warning: endpoint")]
    text = " ".join(ls)
    key = "parse as toml: " if toml else "parse as json: "
    if key in text:
        text = text.split(key, 1)[1].split("; parse as ", 1)[0]
        text = re.sub(r"\s+\|.*\^+", "", text)
    text = re.sub(r"thread 'main' \(\d+\) panicked at ", "panic ", text)
    text = re.sub(r"/\S+/tmp/", "", text)
    return text.strip()[:140].replace("|", "\\|")


def listen(y):
    f = lambda xs: ",".join(a.rsplit(":", 1)[1] for a in xs)
    return "tcp:%s udp:%s" % (f(y["listen"]["tcp"]) or "-", f(y["listen"]["udp"]) or "-")


out = ["| sample | entry | v3.2.0 | rc2 | rc2-debug | verdict | listeners old / new | rc2 warnings | reason (old ; new) |",
       "| --- | --- | --- | --- | --- | --- | --- | --- | --- |"]
tally = {}
for x in rows:
    o, n, d = x["old"], x["new"], x["dbg"]
    toml = any(f.endswith(".toml") for f in x["files"]) and len(x["files"]) == 1
    warn = [l for l in n["stderr"] if l.startswith("warning:") or l.startswith("attention")]
    tally[x["verdict"]] = tally.get(x["verdict"], 0) + 1
    if x["verdict_dbg"] != x["verdict"]:
        tally["debug-differs"] = tally.get("debug-differs", 0) + 1
    out.append("| %s | %s | %s | %s | %s | %s | %s / %s | %d | %s ; %s |" % (
        x["name"], x["mode"], state(o), state(n), state(d), x["verdict"],
        listen(o) if o["state"] == "running" else "-", listen(n) if n["state"] == "running" else "-",
        len(warn), reason(o, toml) or "-", reason(n, toml) or "-"))
out.append("")
out.append("tally: %s" % json.dumps(tally))
open(os.path.join(HERE, "results", "rf01-summary.md"), "w").write("\n".join(out) + "\n")
print("\n".join(l for l in out if "| file |" in l or l.startswith("tally") or "| dir_" in l))
