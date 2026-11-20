"""Failover with more than two peers, plain and balanced. Usage: scen_multi.py [case-id-prefix ...]"""
import sys
import time
import traceback

import scen_fo
from lab import *
from scen_fo import Case, FAST, R, gap, stray

EXTRA = {"y": 14, "z": 15, "p": 16, "q": 17, "s": 18}
# the test tool tells A..D apart, every other tag is counted as "other"
scen_fo.TAG.update({"y": "D", "z": "E", "p": "F", "q": "G", "s": "H"})
NS = {v: k for k, v in scen_fo.TAG.items()}

CASES = {}


def case(fn):
    CASES[fn.__name__.replace("_", "-", 1)] = fn
    return fn


def chain(c, proto="tcp"):
    """Switches as the relay logged them: ["0>1", "1>2", ...]."""
    out = []
    for e in c.ev(proto, "switch"):
        s = e[3].split("switch ")[1]
        out.append(s.split(" ")[0].replace("peer#", "") + ">" + s.split("=> ")[1].split(" ")[0].replace("peer#", ""))
    return out


def phase(res, c, a, b):
    """ok per tag and errors between two points of the case clock (seconds)."""
    t0 = rel_ms(res, c.t0 + a)
    return count(res, t0, rel_ms(res, c.t0 + b))


def even(d, tags):
    """Exactly these tags took traffic, and none took more than 1.6 times the least."""
    return set(d) == set(tags) and max(d.values()) <= 1.6 * min(d.values())


def probe_tag(secs=1):
    """Which peer does a new connection of this client reach right now?"""
    r = tool("c", f"churn target={R} conns=4 rate=80 secs={secs} timeout=3000")
    d = {k: v for k, v in r["ok"].items() if v}
    return max(d, key=d.get) if d else None, d


@case
def mn01_chain_of_four():
    """priority chain A > B > C > D: fail one by one, come back out of order"""
    peers = ("a", "b", "x", "y")
    c = Case(fo_conf(**FAST, peers=peers, udp_send="hc:"), start=peers)
    c.run("tcp", f"churn target={R} conns=16 rate=300 secs=34 timeout=8000")
    c.run("udp", f"udp target={R} socks=20 pps=300 secs=34")
    for t, name, ns in ((3, "killA", "a"), (8, "killB", "b"), (13, "killC", "x")):
        c.at(t)
        c.mark(name)
        c.kill(ns)
    c.at(18)
    c.mark("backB")
    c.start("b")
    c.at(26)
    c.mark("backA")
    c.start("a")
    tcp, udp = c.res("tcp"), c.res("udp")
    alive = c.close()
    ch, uch = chain(c), chain(c, "udp")
    kills = [c.marks[k] for k in ("killA", "killB", "killC")]
    delays = {k: c.delay(k, "tcp") for k in ("killA", "killB", "killC", "backB", "backA")}
    want = ["0>1", "1>2", "2>3", "3>1", "1>0"]
    ok = alive and ch == want and uch == want and stray(tcp, kills) == 0 and tcp["total_err"] <= 48
    ok = ok and udp["stale"] == 0 and set(udp["tag_seqs"]) <= {"ABCD"}
    record(
        "MN-01", "四节点优先级链 A > B > C > D，逐台故障，乱序恢复", "PASS" if ok else "FAIL",
        "A 挂切 B，B 挂切 C，C 挂切 D；B 先恢复就回 B（它比 D 优先），A 恢复后回 A；故障瞬间之外新建 TCP 0 失败；UDP 老会话依次挪到 D 后留在 D",
        f"TCP 切换序列 {ch}，UDP {uch}；各步用时 {delays}；TCP 失败 {tcp['total_err']}/{tcp['sent']}（故障瞬间之外 {stray(tcp, kills)}），"
        f"分布 {count(tcp)['ok']}；UDP 路径 {udp['tag_seqs']}，迟到旧包 {udp['stale']}，丢 {udp['sent'] - udp['total_ok']}/{udp['sent']}",
        {"events": [e[3] for e in c.ev()]},
    )


@case
def mn02_standby_dies_first():
    """the first backup dies while it is idle, then the primary dies: go straight to the second backup"""
    peers = ("a", "b", "x")
    c = Case(fo_conf(**FAST, peers=peers, udp_send="hc:"), start=peers)
    c.run("tcp", f"churn target={R} conns=16 rate=300 secs=16 timeout=8000")
    c.run("udp", f"udp target={R} socks=20 pps=300 secs=16")
    c.at(3)
    c.mark("killB")
    c.kill("b")
    c.at(8)
    c.mark("killA")
    c.kill("a")
    tcp, udp = c.res("tcp"), c.res("udp")
    alive = c.close()
    ch = chain(c)
    early = [e for e in c.ev("tcp", "switch") if e[0] < c.marks["killA"]]
    d = count(tcp)["ok"]
    ok = alive and ch == ["0>2"] and not early and "B" not in d and stray(tcp, [c.marks["killA"]]) == 0
    ok = ok and udp["stale"] == 0 and set(udp["tag_seqs"]) <= {"AC"}
    record(
        "MN-02", "闲置的第一备机先挂，之后主机挂", "PASS" if ok else "FAIL",
        "备机挂时不发生切换；主机挂时越过已知不可用的 B 直接切到 C；故障瞬间之外新建 TCP 0 失败",
        f"切换序列 {ch}（主机挂之前的切换 {len(early)} 次），主机挂后 {c.delay('killA', 'tcp')}s 切换；"
        f"TCP 失败 {tcp['total_err']}/{tcp['sent']}（故障瞬间之外 {stray(tcp, [c.marks['killA']])}），分布 {d}；UDP 路径 {udp['tag_seqs']}",
        {"events": [e[3] for e in c.ev()]},
    )


def two_together(sid, title, how, params, secs):
    peers = ("a", "b", "x")
    c = Case(fo_conf(**params, peers=peers, udp_send="hc:"), start=peers)
    c.run("tcp", f"churn target={R} conns=16 rate=300 secs={secs} timeout=8000")
    c.run("udp", f"udp target={R} socks=20 pps=300 secs={secs}")
    c.at(3)
    c.mark("f")
    if how == "kill":
        c.kill("a")
        c.kill("b")
    else:
        blackhole("a")
        blackhole("b")
    tcp, udp = c.res("tcp"), c.res("udp")
    alive = c.close()
    m = rel_ms(tcp, c.marks["f"])
    first_c = first_ok(tcp, "C", m)
    outage = None if first_c is None else round(max(0, first_c - m) / 1000, 1)
    after = count(tcp, m - 150)
    ugap, _ = gap(udp, c.marks["f"], "A", "C")
    ch = chain(c)
    errs = after["err"]
    ok = alive and ch and ch[-1].endswith(">2") and first_c is not None
    # requests in flight on a peer that goes silent run into the client's own timeout
    ok = ok and not errs.get("reset") and not errs.get("refused") and sum(errs.values()) <= 2
    return record(
        sid, title, "PASS" if ok else "FAIL",
        "最终落到 C。探测判定之前，拨号失败的连接依次补试其余可用节点，新建 TCP 不失败（静默失联时多等一个拨号超时）",
        f"切换序列 {ch}，故障后 {c.delay('f', 'tcp')}s 完成切换；新建 TCP 中断约 {outage}s，失败 {sum(after['err'].values())}/{after['sent']}（{after['err']}）；"
        f"UDP 中断 {ugap}ms，路径 {udp['tag_seqs']}",
        {"events": [e[3] for e in c.ev()], "outage_s": outage, "errs": after["err"]},
    )


@case
def mn03_two_die_together():
    """primary and first backup fail at the same moment"""
    two_together("MN-03a", "主机和第一备机同时被杀（端口拒绝），快速参数", "kill", FAST, 12)
    two_together("MN-03b", "主机和第一备机同时静默失联，快速参数", "bh", FAST, 14)
    two_together("MN-03c", "主机和第一备机同时被杀，默认参数", "kill", {}, 22)


@case
def mn04_balance_four():
    """round robin over four peers with failover: two die, then come back"""
    peers = ("a", "b", "x", "y")
    conf = fo_conf(**FAST, peers=peers, udp_send="hc:", extra='balance = "roundrobin: 1, 1, 1, 1"')
    c = Case(conf, start=peers)
    c.run("tcp", f"churn target={R} conns=16 rate=400 secs=30 timeout=8000")
    c.run("udp", f"udp target={R} socks=40 pps=400 secs=30")
    c.at(3)
    c.mark("killA")
    c.kill("a")
    c.at(9)
    c.mark("killB")
    c.kill("b")
    c.at(15)
    c.mark("backA")
    c.start("a")
    c.at(21)
    c.mark("backB")
    c.start("b")
    tcp, udp = c.res("tcp"), c.res("udp")
    alive = c.close()
    kills = [c.marks["killA"], c.marks["killB"]]
    ph = {
        "全部正常": (phase(tcp, c, 0.5, 2.8), "ABCD"),
        "A 挂": (phase(tcp, c, 6, 8.8), "BCD"),
        "A、B 挂": (phase(tcp, c, 12, 14.8), "CD"),
        "A 恢复": (phase(tcp, c, 19, 20.8), "ACD"),
        "全部恢复": (phase(tcp, c, 25, 29.5), "ABCD"),
    }
    uph = {
        "A 挂": (phase(udp, c, 6, 8.8), "BCD"),
        "A、B 挂": (phase(udp, c, 12, 14.8), "CD"),
    }
    ok = alive and stray(tcp, kills) == 0 and all(even(p["ok"], list(tags)) for p, tags in ph.values())
    # replies are counted when they arrive, so allow the few in flight at a window edge
    ok = ok and all(set(p["ok"]) <= set(tags) and p["sent"] - sum(p["ok"].values()) <= 4 for p, tags in uph.values())
    ok = ok and udp["stale"] == 0 and not chain(c)
    record(
        "MN-04", "四节点轮询 + 健康检查，两台先后挂掉再先后恢复", "PASS" if ok else "FAIL",
        "新连接只分到存活的节点且大致均匀；节点恢复后重新参与；故障瞬间之外新建 TCP 0 失败；UDP 会话离开故障节点后稳态 0 丢包",
        "TCP 各阶段分布 " + "；".join(f"{k} {p['ok']}" for k, (p, _t) in ph.items())
        + f"；TCP 失败 {tcp['total_err']}/{tcp['sent']}（故障瞬间之外 {stray(tcp, kills)}）；"
        + "UDP " + "；".join(f"{k} {p['ok']} 丢 {p['sent'] - sum(p['ok'].values())}" for k, (p, _t) in uph.items())
        + f"；UDP 会话路径 {udp['tag_seqs']}，迟到旧包 {udp['stale']}",
        {"events": [e[3] for e in c.ev()]},
    )


@case
def mn05_iphash_four():
    """ip hash over four peers with failover: the client's peer dies, then the one it moved to"""
    peers = ("a", "b", "x", "y")
    conf = fo_conf(**FAST, peers=peers, udp_send="hc:", extra='balance = "iphash: 1, 1, 1, 1"')
    c = Case(conf, start=peers)
    home, d0 = probe_tag()
    c.kill(NS[home])
    time.sleep(3)
    second, d1 = probe_tag()
    c.kill(NS[second])
    time.sleep(3)
    third, d2 = probe_tag()
    c.start(NS[home])
    time.sleep(4.5)
    back, d3 = probe_tag()
    alive = c.close()
    ok = alive and len(d0) == 1 and len(d1) == 1 and len(d2) == 1 and len(d3) == 1
    ok = ok and len({home, second, third}) == 3 and back == home
    record(
        "MN-05", "四节点 IP 哈希 + 健康检查，客户端所在节点连挂两次", "PASS" if ok else "FAIL",
        "同一客户端始终只落在一台上；它的节点挂了就换到下一台存活的，再挂再换；原节点恢复后回到原节点",
        f"初始 {d0}；该节点挂后 {d1}；再挂后 {d2}；原节点恢复后 {d3}",
        {"events": [e[3] for e in c.ev()]},
    )


@case
def mn06_chain_of_eight():
    """the maximum of eight peers: seven fail in order, then the primary returns"""
    peers = ("a", "b", "x", "y", "z", "p", "q", "s")
    c = Case(fo_conf(**FAST, peers=peers, udp_send="hc:"), start=peers)
    c.run("tcp", f"churn target={R} conns=16 rate=300 secs=32 timeout=8000")
    c.run("udp", f"udp target={R} socks=20 pps=300 secs=32")
    kills = []
    for i, ns in enumerate(peers[:7]):
        c.at(3 + 3 * i)
        c.mark(f"kill{i}")
        c.kill(ns)
        kills.append(c.marks[f"kill{i}"])
    c.at(24)
    c.mark("backA")
    c.start("a")
    tcp, udp = c.res("tcp"), c.res("udp")
    alive = c.close()
    ch, uch = chain(c), chain(c, "udp")
    want = [f"{i}>{i + 1}" for i in range(7)] + ["7>0"]
    delays = [c.delay(f"kill{i}", "tcp") for i in range(7)]
    ok = alive and ch == want and uch == want and stray(tcp, kills) == 0 and udp["stale"] == 0
    ok = ok and set(udp["tag_seqs"]) <= {"ABCDEFGH"}
    record(
        "MN-06", "八节点（上限）优先级链，前七台依次挂掉，之后主机恢复", "PASS" if ok else "FAIL",
        "每挂一台切到下一台，直到最后一台；主机恢复后回主机；故障瞬间之外新建 TCP 0 失败",
        f"TCP 切换序列 {ch}，UDP {uch}；每次切换用时 {delays}s，回主机 {c.delay('backA', 'tcp')}s；"
        f"TCP 失败 {tcp['total_err']}/{tcp['sent']}（故障瞬间之外 {stray(tcp, kills)}），分布 {count(tcp)['ok']}；"
        f"UDP 路径 {udp['tag_seqs']}，迟到旧包 {udp['stale']}，丢 {udp['sent'] - udp['total_ok']}/{udp['sent']}",
        {"events": [e[3] for e in c.ev()]},
    )


if __name__ == "__main__":
    want = sys.argv[1:]
    net_up(EXTRA)
    for name, fn in CASES.items():
        if want and not any(name.startswith(w) for w in want):
            continue
        t = time.time()
        try:
            fn()
        except Exception as e:
            traceback.print_exc()
            record(name, fn.__doc__ or name, "FAIL", "-", f"脚本异常: {e}")
        print(f"    ({name}: {time.time() - t:.0f}s)", flush=True)
    save("multi" + ("-" + "-".join(want) if want else ""))
    net_down()
    bad = [r["id"] for r in RESULTS if r["status"] == "FAIL"]
    print("FAILED:", bad if bad else "none")
