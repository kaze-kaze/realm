"""Print markdown fragments from result json. Usage: mktables.py <scratch dir>"""
import json, os, sys
S = sys.argv[1]
def load(p):
    p = f"{S}/{p}"
    return json.load(open(p)) if os.path.exists(p) else None

def perf_table(d, title, names):
    rows = [
        ("TCP 单连接上行 (Gbit/s)", "tcp1_gbps", 1), ("TCP 单连接下行 (Gbit/s)", "tcp1_rev_gbps", 1), ("TCP 8 连接 (Gbit/s)", "tcp8_gbps", 1),
        ("UDP 64B 上行 (千包/秒)", "udp64_kpps", 0), ("UDP 1400B 上行 (千包/秒)", "udp1400_kpps", 0), ("UDP 1400B 下行 (千包/秒)", "udp1400_rev_kpps", 0),
        ("短连接速率 (连接/秒)", "churn_cps", 0), ("短连接 p50 (µs)", "churn_p50_us", 0), ("短连接 p99 (µs)", "churn_p99_us", 0),
        ("每千连接 CPU (ms)", "fixed_cpu_ms_per_1k_conn", 0), ("5000 连接/秒时 p99 (µs)", "fixed_p99_us", 0),
        ("1 MiB 往返校验 (MiB/s)", "xfer_mib_s", 0), ("空闲 RSS (kB)", "rss_idle_kb", 0), ("压测后 RSS (kB)", "rss_after_kb", 0),
        ("线程数", "threads", 0), ("失败数（短连接+定速+大包）", None, 0),
    ]
    s = d["summary"]
    names = [n for n in names if n in s]
    print(f"\n**{title}**（中位数，括号内为最小~最大，{len(d['runs'][names[0]])} 轮）\n")
    print("| 指标 | " + " | ".join(names) + " |\n| --- |" + " --- |" * len(names))
    for label, key, prec in rows:
        cells = []
        for n in names:
            if key is None:
                tot = sum(r.get("churn_err", 0) + r.get("fixed_err", 0) + r.get("xfer_err", 0) for r in d["runs"][n])
                cells.append(str(tot))
            elif key in s[n]:
                v = s[n][key]
                cells.append(f"{v['median']:.{prec}f} ({v['min']:.{prec}f}~{v['max']:.{prec}f})")
            else:
                cells.append("—")
        if any(c != "—" for c in cells):
            print(f"| {label} | " + " | ".join(cells) + " |")

def fp_table(before, after):
    print("\n| 链路状况 | TCP 误切 修复前 | TCP 误切 修复后 | UDP 误切 修复前 | UDP 误切 修复后 | 备节点误判 |\n| --- | --- | --- | --- | --- | --- |")
    a = {l["name"]: l for l in after["levels"]} if after else {}
    for l in before["levels"]:
        x = a.get(l["name"])
        f = lambda v: "—" if v is None else str(v)
        print(f"| {l['label']} | {l['tcp_switch']} | {f(x and x['tcp_switch'])} | {l['udp_switch']} | {f(x and x['udp_switch'])} | {l['tcp_down1'] + l['udp_down1']} / {f(x and (x['tcp_down1'] + x['udp_down1']))} |")
    for name, d in (("修复前", before), ("修复后", after)):
        if d:
            print(f"\n{name}：{d['endpoints']} 个端点 × {d['secs']}s（每端点每协议 {d['rounds_per_endpoint']:.0f} 轮探测），{d['monitors']} 个探测任务，CPU {d['cpu_pct_of_one_core']:.1f}% 单核，RSS {d['rss_kb'] // 1024} MB，存活 {d['alive']}")

p0, p1 = load("out/perf-baseline.json"), load("out-final/perf.json")
if p0: perf_table(p0, "修复前（已合并的 565df28）", ["orig", "new", "new+failover", "orig+balance", "new+balance"])
if p1: perf_table(p1, "修复后", ["orig", "new", "new+failover", "orig+balance", "new+balance"])
ab = load("out-final/perf-ab.json")
if ab:
    print("\n**顺序平衡 A/B（短连接）**\n\n| 版本 | 连接/秒 | 每千连接 CPU (ms) | p99 (µs) | 失败 |\n| --- | --- | --- | --- | --- |")
    for n, s in ab["summary"].items():
        print(f"| {n} | {s['cps']['median']:.0f} ({s['cps']['min']:.0f}~{s['cps']['max']:.0f}) | {s['cpu_ms_per_1k']['median']:.0f} ({s['cpu_ms_per_1k']['min']:.0f}~{s['cpu_ms_per_1k']['max']:.0f}) | {s['p99_us']['median']:.0f} | {s['err']['max']:.0f} |")
f0, f1 = load("out/fp-baseline.json"), load("out-fix/fp.json")
if f0: fp_table(f0, f1)
fl = load("out-fix/fp-low.json")
if fl:
    print(f"\n**低丢包长时统计（修复后）**：{fl['endpoints']} 端点 × {fl['secs']}s\n\n| 链路状况 | 端点·小时 | TCP 误判下线 | TCP 误切 | UDP 误判下线 | UDP 误切 | 折合每端点每天 TCP/UDP |\n| --- | --- | --- | --- | --- | --- | --- |")
    for l in fl["levels"]:
        hrs = fl["endpoints"] * fl["secs"] / 3600
        print(f"| {l['label']} | {hrs:.0f} | {l['tcp_down0']} | {l['tcp_switch']} | {l['udp_down0']} | {l['udp_switch']} | {l['tcp_switch_per_day']:.2f} / {l['udp_switch_per_day']:.2f} |")
