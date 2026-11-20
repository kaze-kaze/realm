import json, sys
d = json.load(open(sys.argv[1]))
print({k: v for k, v in d.items() if k not in ("samples", "faults", "load")})
for k, v in d["load"].items():
    print(" ", k, v)
s = d["samples"]; n = len(s)
six = [s[i * n // 6:(i + 1) * n // 6] for i in range(6)]
print("rss MB by sixths:", [round(sum(x["rss_kb"] for x in c) / len(c) / 1024, 1) for c in six if c])
print("fd range", min(x["fd"] for x in s), max(x["fd"] for x in s), "threads", sorted(set(x["threads"] for x in s)))
from collections import Counter
print("faults:", dict(Counter(f[1] for f in d["faults"])))
