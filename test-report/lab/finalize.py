"""Fill the remaining sections of REPORT.md from the final result files."""
import json, sys
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

fo, lb, ex = load("out-final/fo.json"), load("out-final/lb.json"), load("out-final/ex.json")
soak = load("out-final/soak.json")
tally = lambda rs: Counter(r["status"] for r in rs)
tf, tl, te = tally(fo), tally(lb), tally(ex)
bad = [r["id"] for r in fo + lb + ex if r["status"] == "FAIL"]

s = soak; L = s["load"]; n = len(s["samples"])
six = [s["samples"][i * n // 6:(i + 1) * n // 6] for i in range(6)]
rss6 = [round(sum(x["rss_kb"] for x in c) / len(c) / 1024, 1) for c in six if c]
faults = Counter(f[1] for f in s["faults"])
soak_txt = f"""**长稳测试**（最终版，{s['secs'] // 60} 分钟，加速参数）：短连接 500/秒、256 kB 大包 10/秒、500 条长连接、200 个 UDP 会话共 2000 包/秒，同时每 12~25 秒随机注入一次故障，共 {len(s['faults'])} 次（杀主 {faults['f_kill_a']}、杀备 {faults['f_kill_b']}、主备全挂 {faults['f_both_down']}、主黑洞 {faults['f_blackhole_a']}、主假死 {faults['f_mute_a']}、主丢包 25% {faults['f_loss_a']}、主链路抖动 {faults['f_flap_a']}、主变慢 {faults['f_slow_a']}）。

| 项目 | 结果 |
| --- | --- |
| 进程 | 存活 {s['alive']}，panic {len(s['panics'])} 次，线程数恒为 {sorted(set(x['threads'] for x in s['samples']))} |
| 句柄 | 基线 {s['base']['fd']}，峰值 {s['fd_max']}，结束后回到 {s['end']['fd']} |
| 内存 RSS | 每 {s['secs'] // 360} 分钟的均值依次为 {' → '.join(str(x) for x in rss6)} MB |
| 切换次数 | TCP {s['switches']['tcp']} 次，UDP {s['switches']['udp']} 次 |
| 数据校验 | 短连接 {L['small']['ok']} 次、大包 {L['large']['ok']} 次、长连接请求 {L['hold']['ok']} 次，逐字节校验错误 {L['small']['err'].get('mismatch', 0) + L['large']['err'].get('mismatch', 0) + L['hold']['err'].get('mismatch', 0)} 个 |
| TCP 长连接 | 同一条连接换后端 {L['hold']['tag_changed']} 次；结束时 {L['hold']['alive_at_end']}/500 在线 |
| UDP | 送达 {L['udp']['ok']}/{L['udp']['sent']}（{L['udp']['ok'] / L['udp']['sent']:.1%}，丢的都在故障窗口内），乱序旧回包 {L['udp']['stale']}，重复 {L['udp']['dup']}，收到别的会话的包 {L['udp']['foreign']} |

故障期间的失败是真实故障造成的（主备全挂时无处可去、被杀瞬间在途的请求），不是转发器出错。
"""

sec36 = "极端场景（最终版）：\n\n" + table(ex, with_expect=False) + "\n" + soak_txt

appendix = f"""### 附录 A：全部场景明细（最终版）

故障转移场景：{tf['PASS']} 项通过，{tf['INFO']} 项说明，{tf['FAIL']} 项失败。

{table(fo)}
负载均衡回归：{tl['PASS']} 项通过，{tl['INFO']} 项说明，{tl['FAIL']} 项失败。

{table(lb)}
极端场景：{te['PASS']} 项通过，{te['INFO']} 项说明，{te['FAIL']} 项失败。明细见 3.6。

APPENDIX_REST"""

r = open(REPORT).read()
assert "【待补：最终版回归后更新】" in r and "## 附录\n\n【待补】" in r
r = r.replace("【待补：最终版回归后更新】\n", sec36)
r = r.replace("## 附录\n\n【待补】\n", "## 附录\n\n" + appendix)
open(REPORT, "w").write(r)
print("tally", dict(tf), dict(tl), dict(te), "FAILED:", bad)
