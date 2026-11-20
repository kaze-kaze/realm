"""Load balance regression: original v2.9.6 vs new build must behave the same."""
import json, time, traceback
from lab import *

TAG = {"a": "A", "b": "B", "x": "C"}
R = f"{ip4('r')}:6000"


def lb_conf(balance, peers=("a", "b", "x"), udp=True):
    extra = ", ".join(f'"{ip4(p)}:5000"' for p in peers[1:])
    bal = f'balance = "{balance}"\n' if balance else ""
    return f"""
[network]
use_udp = {str(udp).lower()}
[[endpoints]]
listen = "0.0.0.0:6000"
remote = "{ip4(peers[0])}:5000"
extra_remotes = [{extra}]
{bal}"""


def up(conf, binary, start=("a", "b", "x")):
    sh("pkill -9 -x fot; pkill -9 -f realm-", check=False)
    time.sleep(0.2)
    back = {ns: backend(ns, TAG[ns]) for ns in start}
    r = realm(conf, binary=binary, level="warn", name="lb")
    time.sleep(0.5)
    return back, r


def dist(res):
    return {k: v for k, v in res["ok"].items() if v}


def both(conf, fn):
    out = {}
    for name, binary in (("orig", ORIG), ("new", NEW)):
        back, r = up(conf, binary)
        out[name] = fn(back, r)
        r.kill()
        for b in back.values():
            b.kill()
    return out


def lb01():
    f = lambda back, r: dist(tool("c", f"churn target={R} conns=1 rate=500 secs=6"))
    o = both(lb_conf("roundrobin: 1, 1, 1"), f)
    def even(d):
        v = list(d.values()); return len(v) == 3 and max(v) - min(v) <= 1
    record("LB-01", "轮询 1:1:1", "PASS" if even(o["orig"]) and even(o["new"]) else "FAIL",
           "三个后端均分（差值 ≤1），新旧版一致", f"原版 {o['orig']}；新版 {o['new']}")


def lb02():
    f = lambda back, r: dist(tool("c", f"churn target={R} conns=16 rate=2000 secs=4"))
    o = both(lb_conf("roundrobin: 4, 2, 1"), f)
    def ratio(d):
        t = sum(d.values()); return [round(d.get(k, 0) / t * 7, 2) for k in "ABC"]
    ok = all(abs(a - b) < 0.05 for d in o.values() for a, b in zip(ratio(d), (4, 2, 1)))
    record("LB-02", "加权轮询 4:2:1（16 并发）", "PASS" if ok else "FAIL",
           "按权重 4:2:1 分配，新旧版一致", f"原版 {o['orig']} → {ratio(o['orig'])}；新版 {o['new']} → {ratio(o['new'])}")


def lb03():
    ips = [f"{NET4}{100 + i}" for i in range(40)]
    for ip in ips:
        nsx("c", f"ip addr add {ip}/24 dev eth0", check=False)
    def f(back, r):
        m, sticky = {}, True
        for ip in ips:
            d = dist(tool("c", f"churn target={R} conns=4 rate=100 secs=1 bind={ip}"))
            sticky = sticky and len(d) == 1
            m[ip] = "".join(d)
        return m, sticky
    o = both(lb_conf("iphash: 2, 1, 1"), f)
    (mo, so), (mn, sn) = o["orig"], o["new"]
    cnt = lambda m: {k: list(m.values()).count(k) for k in "ABC"}
    ok = so and sn and mo == mn
    record("LB-03", "IP 哈希 2:1:1（40 个来源 IP）", "PASS" if ok else "FAIL",
           "同一来源 IP 始终落到同一后端；新旧版的映射逐个 IP 完全相同",
           f"原版：粘性 {so}，分布 {cnt(mo)}；新版：粘性 {sn}，分布 {cnt(mn)}；映射相同 {mo == mn}")


def lb04():
    def f(back, r):
        back["b"].kill()
        time.sleep(0.3)
        res = tool("c", f"churn target={R} conns=8 rate=600 secs=3 timeout=3000")
        return dist(res), {k: v for k, v in res["err"].items() if v}, res["sent"]
    o = both(lb_conf("roundrobin: 1, 1, 1"), f)
    frac = lambda x: round(sum(x[1].values()) / x[2], 3)
    ok = abs(frac(o["orig"]) - 1 / 3) < 0.02 and abs(frac(o["new"]) - 1 / 3) < 0.02
    record("LB-04", "轮询时一个后端挂掉", "INFO" if ok else "FAIL",
           "上游原有行为：balance 没有健康检查，分到死节点的 1/3 连接直接失败；新旧版一致（本次未改动）",
           f"原版 成功 {o['orig'][0]} 失败 {o['orig'][1]}（{frac(o['orig']):.1%}）；新版 成功 {o['new'][0]} 失败 {o['new'][1]}（{frac(o['new']):.1%}）")


def lb05():
    def f(back, r):
        res = tool("c", f"udp target={R} socks=30 pps=600 secs=3")
        return dist(res), res["sent"], res["total_ok"]
    o = both(lb_conf("roundrobin: 1, 1, 1"), f)
    ok = all(set(v[0]) == {"A"} and v[1] == v[2] for v in o.values())
    record("LB-05", "balance 模式下的 UDP", "PASS" if ok else "FAIL",
           "上游原有行为：UDP 不参与负载均衡，只发往 remote；新旧版一致",
           f"原版 {o['orig'][0]}（{o['orig'][2]}/{o['orig'][1]}）；新版 {o['new'][0]}（{o['new'][2]}/{o['new'][1]}）")


def lb06():
    """balance and failover on one endpoint: rejected by v3.0.0, allowed since v3.1.0"""
    conf = lb_conf("roundrobin: 1, 1").replace("[network]", "[network]\ntcp_timeout = 4") + "[endpoints.failover]\n"
    conf = conf.replace(f', "{ip4("x")}:5000"', "")
    back, r = up(conf, NEW, start=("a", "b"))
    time.sleep(2)
    res = tool("c", f"churn target={R} conns=8 rate=200 secs=3 timeout=3000")
    alive = r.alive()
    r.kill()
    for b in back.values():
        b.kill()
    d = dist(res)
    ok = alive and res["total_err"] == 0 and set(d) == {"A", "B"} and abs(d["A"] - d["B"]) <= 2
    record("LB-06", "同一 endpoint 同时配置 balance 和 failover", "PASS" if ok else "FAIL",
           "v3.1.0 起允许同时配置：正常启动，新连接在各节点间均分",
           f"进程存活 {alive}；失败 {res['total_err']}/{res['sent']}；分布 {d}")


def lb07():
    """no balance, no failover, but extra_remotes present: new build must behave like the original"""
    f = lambda back, r: dist(tool("c", f"churn target={R} conns=8 rate=500 secs=2"))
    o = both(lb_conf(None), f)
    ok = all(set(v) == {"A"} for v in o.values())
    record("LB-07", "不配 balance / failover（仅有 extra_remotes）", "PASS" if ok else "FAIL",
           "与原版一致：全部走 remote", f"原版 {o['orig']}；新版 {o['new']}")


def lb08():
    """failover table in a config read by the original binary / new binary with unknown key"""
    def f(back, r):
        res = tool("c", f"churn target={R} conns=64 secs=5")
        return round(res["rate"]), res["total_err"], res["lat_us"]["p99"]
    o = both(lb_conf("roundrobin: 1, 1, 1"), f)
    ok = o["new"][1] == 0 and o["orig"][1] == 0 and o["new"][0] > 0.9 * o["orig"][0]
    record("LB-08", "轮询模式短连接压测（veth 拓扑，64 并发）", "PASS" if ok else "FAIL",
           "新版在 balance 路径上的速率不低于原版的 90%，0 失败",
           f"原版 {o['orig'][0]} 连接/秒（失败 {o['orig'][1]}，p99 {o['orig'][2]}µs）；新版 {o['new'][0]} 连接/秒（失败 {o['new'][1]}，p99 {o['new'][2]}µs）")


if __name__ == "__main__":
    net_up()
    for fn in (lb01, lb02, lb03, lb04, lb05, lb06, lb07, lb08):
        try:
            fn()
        except Exception as e:
            traceback.print_exc()
            record(fn.__name__, fn.__doc__ or fn.__name__, "FAIL", "-", f"脚本异常: {e}")
    save("lb")
    net_down()
    print("FAILED:", [r["id"] for r in RESULTS if r["status"] == "FAIL"] or "none")
