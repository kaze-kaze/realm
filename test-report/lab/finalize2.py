"""Regenerate appendix A and the soak block of REPORT.md from a result directory.
Usage: finalize2.py result_dir REPORT.md"""
import json, re, sys
from collections import Counter
S, REPORT = sys.argv[1], sys.argv[2]
ICON = {"PASS": "通过", "FAIL": "**失败**", "INFO": "说明"}
load = lambda p: json.load(open(f"{S}/{p}"))

def table(rs, with_expect=True):
    head = "| 编号 | 场景 | 预期 | 实测 | 结果 |\n| --- | --- | --- | --- | --- |\n" if with_expect else "| 编号 | 场景 | 实测 | 结果 |\n| --- | --- | --- | --- |\n"
    rows = []
    for r in rs:
        obs = r["observed"].replace("|", "\\|").replace("\n", " ")
        if with_expect:
            rows.append(f"| {r['id']} | {r['title']} | {r['expect']} | {obs} | {ICON.get(r['status'], r['status'])} |")
        else:
            rows.append(f"| {r['id']} | {r['title']} | {obs} | {ICON.get(r['status'], r['status'])} |")
    return head + "\n".join(rows) + "\n"

fo, lb, ex = load("fo.json"), load("lb.json"), load("ex.json")
soak = load("soak.json")
tally = lambda rs: Counter(r["status"] for r in rs)
tf, tl, te = tally(fo), tally(lb), tally(ex)
bad = [r["id"] for r in fo + lb + ex if r["status"] == "FAIL"]

s = soak; L = s["load"]; n = len(s["samples"])
six = [s["samples"][i * n // 6:(i + 1) * n // 6] for i in range(6)]
rss6 = [round(sum(x["rss_kb"] for x in c) / len(c) / 1024, 1) for c in six if c]
faults = Counter(f[1] for f in s["faults"])
mism = sum(L[k]["err"].get("mismatch", 0) for k in ("small", "large", "hold"))
soak_txt = f"""**长稳测试**（最终版，{s['secs'] // 60} 分钟，加速参数）：短连接 500/秒、256 kB 大包 10/秒、500 条长连接、200 个 UDP 会话共 2000 包/秒，同时每 12~25 秒随机注入一次故障，共 {len(s['faults'])} 次（杀主 {faults['f_kill_a']}、杀备 {faults['f_kill_b']}、主备全挂 {faults['f_both_down']}、主黑洞 {faults['f_blackhole_a']}、主假死 {faults['f_mute_a']}、主丢包 25% {faults['f_loss_a']}、主链路抖动 {faults['f_flap_a']}、主变慢 {faults['f_slow_a']}）。

| 项目 | 结果 |
| --- | --- |
| 进程 | {'全程存活' if s['alive'] else '**中途退出**'}，panic {len(s['panics'])} 次，线程数 {sorted(set(x['threads'] for x in s['samples']))} |
| 句柄 | 基线 {s['base']['fd']}，峰值 {s['fd_max']}，结束后回到 {s['end']['fd']} |
| 内存 RSS | 每 {s['secs'] // 360} 分钟的均值依次为 {' → '.join(str(x) for x in rss6)} MB |
| 切换次数 | TCP {s['switches']['tcp']} 次，UDP {s['switches']['udp']} 次 |
| 数据校验 | 短连接 {L['small']['ok']} 次、大包 {L['large']['ok']} 次、长连接请求 {L['hold']['ok']} 次，逐字节校验错误 {mism} 个 |
| TCP 长连接 | 同一条连接换后端 {L['hold']['tag_changed']} 次；结束时 {L['hold']['alive_at_end']}/500 在线 |
| UDP | 送达 {L['udp']['ok']}/{L['udp']['sent']}（{L['udp']['ok'] / L['udp']['sent']:.1%}，丢的都在故障窗口内），乱序旧回包 {L['udp']['stale']}，重复 {L['udp']['dup']}，收到别的会话的包 {L['udp']['foreign']} |

"""

appendix = f"""### 附录 A：全部场景明细（最终版）

故障转移场景：{tf['PASS']} 项通过，{tf['INFO']} 项说明，{tf['FAIL']} 项失败。

{table(fo)}
负载均衡回归：{tl['PASS']} 项通过，{tl['INFO']} 项说明，{tl['FAIL']} 项失败。

{table(lb)}
极端场景：{te['PASS']} 项通过，{te['INFO']} 项说明，{te['FAIL']} 项失败。

{table(ex, with_expect=False)}
两处说明：

- **FO-05 的「断 0.3s / 通 0.3s」一行**出现了 2 次切换和 17 个请求超时（0.36%），而去掉清零规则之前的几轮都是 0 次切换、0 个超时。追查的结论是这与规则无关：这样快速地反复断网卡，偶尔会让主节点一侧解析不出中继的 MAC 地址，主节点变成真的不可达。那一段时间里，从中继侧每 50 毫秒发一次的独立 ping 回复率是 0%（正常轮次是 49%），探测如实判了下线并切走，超时的是那一刻已经发到主节点上的请求。去掉规则前后的两个构建都遇到过，合计 24 轮里 3 次。正常轮次里这一行是 0 次切换、0 个超时、UDP 送达 64%，见第四节的最后一条局限。
- **EX-03 里的原版对照**这一轮不正常：只送达 16%，17.8 万个包送给了别的会话，采样时只剩 1 个上游套接字。原版对照累计十余轮里出现过 2 次，事后单独复跑 6 轮没有重现（都是送达 97%~98%、串包 4575~5451 个、内存 1078 MB），原因没有查明。被测版本在所有轮次里都没有出现过这种情况。正文里原版的内存和串包数字取自正常轮次。

"""

r = open(REPORT).read()
a, b = r.index("### 附录 A："), r.index("### 附录 B：")
r = r[:a] + appendix + r[b:]
a, b = r.index("**长稳测试**"), r.index("故障期间的失败是真实故障造成的")
r = r[:a] + soak_txt + r[b:]
open(REPORT, "w").write(r)
print("tally", dict(tf), dict(tl), dict(te), "FAILED:", bad)
print("soak", {"alive": s["alive"], "panics": len(s["panics"]), "fd": (s["base"]["fd"], s["fd_max"], s["end"]["fd"]), "rss6": rss6, "sw": s["switches"], "mismatch": mism,
               "udp": {k: L["udp"].get(k) for k in ("ok", "sent", "stale", "dup", "foreign")}, "hold": {k: L["hold"].get(k) for k in ("tag_changed", "alive_at_end", "died")}})
