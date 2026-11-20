"""Failover scenarios, tcp + udp. Usage: scen_fo.py [case-id-prefix ...]"""
import sys
import time
import traceback

from lab import *

TAG = {"a": "A", "b": "B", "x": "C"}
R = f"{ip4('r')}:6000"
FAST = dict(interval=0.5, timeout=0.3, rise=5, hold=5)


def reset_faults():
    for ns in HOSTS:
        netem_clear(ns)
        nsx(ns, "iptables -F", check=False)
        nsx(ns, "ip6tables -F", check=False)
        link(ns, True)


class Case:
    def __init__(self, conf, start=("a", "b"), level="warn", binary=NEW, warm=None):
        sh("pkill -9 -x fot; pkill -9 -f realm-", check=False)
        reset_faults()
        time.sleep(0.2)
        self.back = {ns: backend(ns, TAG[ns]) for ns in start}
        self.r = realm(conf, level=level, binary=binary)
        self.pid = self.r.pid()
        self.marks = {}
        self.bg = {}
        time.sleep(warm if warm is not None else 1.6)
        self.t0 = time.time()

    def kill(self, ns):
        self.back[ns].kill()

    def start(self, ns):
        self.back[ns] = backend(ns, TAG[ns])

    def mark(self, name):
        self.marks[name] = time.time()

    def at(self, t):
        d = self.t0 + t - time.time()
        if d > 0:
            time.sleep(d)

    def run(self, name, args):
        self.bg[name] = tool("c", args, bg=True)

    def res(self, name):
        return done(self.bg[name], timeout=300)

    def ev(self, proto=None, kind=None, since=None):
        s = self.marks.get(since, 0) if isinstance(since, str) else (since or 0)
        return [e for e in events(self.r, s) if (not proto or e[1] == proto) and (not kind or e[2] == kind)]

    def delay(self, mark, proto, kind="switch", nth=0):
        """Seconds from a mark to the nth matching failover event after it."""
        ev = self.ev(proto, kind, since=mark)
        if len(ev) > nth:
            return round(ev[nth][0] - self.marks[mark], 2)
        return None

    def close(self):
        alive = self.r.alive()
        self.r.kill()
        for b in self.back.values():
            b.kill()
        reset_faults()
        return alive


def gap(res, mark_wall, old, new):
    """Blackout seen by clients: ms between the last reply of `old` and the first of `new` after the fault."""
    m = rel_ms(res, mark_wall)
    f = first_ok(res, new, m)
    lo = None
    i = TAGS.index(old)
    for t, _s, ok, _e, _l in res["buckets"]:
        if ok[i] > 0 and (f is None or t <= f):
            lo = t
    return None if f is None or lo is None else int(f - lo), (None if f is None else max(0, int(f - m)))


def stray(res, walls, before=150, after=500):
    """Errors outside the short windows around the given fault times (in-flight requests die with their backend)."""
    n = 0
    wins = [(rel_ms(res, w) - before, rel_ms(res, w) + after) for w in walls]
    for t, _s, _ok, err, _l in res["buckets"]:
        if sum(err) and not any(a <= t < b for a, b in wins):
            n += sum(err)
    return n


CASES = {}


def case(fn):
    CASES[fn.__name__.replace("_", "-", 1)] = fn
    return fn


# ---------------------------------------------------------------- cases


@case
def fo01_kill_primary():
    """backend process dies: tcp refused, udp icmp unreachable"""
    c = Case(fo_conf(**FAST, udp_send="hc:"))
    c.run("tcp", f"churn target={R} conns=16 rate=400 secs=14 timeout=8000")
    c.run("udp", f"udp target={R} socks=20 pps=400 secs=14")
    c.at(5)
    c.mark("kill")
    c.kill("a")
    tcp, udp = c.res("tcp"), c.res("udp")
    alive = c.close()
    tgap, tfirst = gap(tcp, c.marks["kill"], "A", "B")
    ugap, ufirst = gap(udp, c.marks["kill"], "A", "B")
    sw_t, sw_u = c.delay("kill", "tcp"), c.delay("kill", "udp")
    lost = udp["sent"] - udp["total_ok"]
    ok = alive and stray(tcp, [c.marks["kill"]]) == 0 and tcp["total_err"] <= 16 and tcp["ok"]["B"] > 0 and sw_t is not None and sw_u is not None and udp["stale"] == 0
    ok = ok and set(udp["tag_seqs"]) <= {"AB"} and sw_t <= 2.5 and sw_u <= 2.5
    record(
        "FO-01", "主节点进程被杀（TCP 拒绝 / UDP 不可达）", "PASS" if ok else "FAIL",
        "新建 TCP 连接 0 失败（单连接立即重试备；只有被杀瞬间正在主上收发的请求会断）；约 1~2 秒内全局切换；UDP 全部移到备且无旧回包",
        f"TCP 失败 {tcp['total_err']}/{tcp['sent']}（{count(tcp)['err'] or '无'}），首个到备的连接在故障后 {tfirst}ms 内（100ms 粒度）；全局切换 TCP {sw_t}s / UDP {sw_u}s；"
        f"UDP 中断 {ugap}ms、丢 {lost}/{udp['sent']} 包，路径序列 {udp['tag_seqs']}，旧回包 {udp['stale']}",
        {"tcp": count(tcp), "udp": count(udp), "events": [e[3] for e in c.ev()]},
    )


@case
def fo02_blackhole_primary():
    """host unreachable: packets silently dropped"""
    c = Case(fo_conf(**FAST, udp_send="hc:", tcp_timeout=4))
    c.run("tcp", f"churn target={R} conns=64 rate=400 secs=14 timeout=8000")
    c.run("udp", f"udp target={R} socks=20 pps=400 secs=14")
    c.at(5)
    c.mark("drop")
    blackhole("a")
    tcp, udp = c.res("tcp"), c.res("udp")
    alive = c.close()
    m = rel_ms(tcp, c.marks["drop"])
    sw_t, sw_u = c.delay("drop", "tcp"), c.delay("drop", "udp")
    win = count(tcp, m, m + (sw_t or 0) * 1000 + 300)
    after = count(tcp, m + (sw_t or 0) * 1000 + 2500)
    ugap, _ = gap(udp, c.marks["drop"], "A", "B")
    lost = udp["sent"] - udp["total_ok"]
    ok = alive and tcp["total_err"] == 0 and sw_t is not None and sw_t <= 3 and sw_u is not None and sw_u <= 3
    ok = ok and not after["err"] and after["lat_max_ms"] < 500 and udp["stale"] == 0
    record(
        "FO-02", "主节点黑洞（丢包不回应）", "PASS" if ok else "FAIL",
        "TCP 0 失败：切换前的连接等 tcp_timeout/2=2s 后重试备；约 1.5~2.5 秒全局切换后恢复低延迟",
        f"TCP 失败 {tcp['total_err']}/{tcp['sent']}；切换前窗口内最大建连+往返 {win['lat_max_ms']}ms，切换后 {after['lat_max_ms']}ms；"
        f"全局切换 TCP {sw_t}s / UDP {sw_u}s；UDP 中断 {ugap}ms、丢 {lost}/{udp['sent']} 包",
        {"tcp": count(tcp), "udp": count(udp), "events": [e[3] for e in c.ev()]},
    )


@case
def fo03_app_hang_with_expect():
    """process accepts but never answers; probe uses tcp_send/tcp_expect"""
    c = Case(fo_conf(**FAST, tcp_send="ping", tcp_expect="OK", udp_send="hc:"))
    c.run("tcp", f"churn target={R} conns=64 rate=300 secs=14 timeout=1500")
    c.run("udp", f"udp target={R} socks=20 pps=400 secs=14")
    c.at(5)
    c.mark("mute")
    ctl("a", "tcp.mode=mute", "udp.mode=mute")
    tcp, udp = c.res("tcp"), c.res("udp")
    alive = c.close()
    m = rel_ms(tcp, c.marks["mute"])
    sw_t, sw_u = c.delay("mute", "tcp"), c.delay("mute", "udp")
    # a request that started right before the switch only fails 1.5s later (client timeout)
    after = count(tcp, m + (sw_t or 9) * 1000 + 1500 + 300)
    ok = alive and sw_t is not None and sw_t <= 3 and sw_u is not None and sw_u <= 3 and not after["err"]
    record(
        "FO-03", "主节点应用假死（能握手不回数据），探测带 tcp_send/tcp_expect", "PASS" if ok else "FAIL",
        "探测识别假死并切换；切换前落到主的连接无法挽救（已握手成功），切换后 0 失败",
        f"全局切换 TCP {sw_t}s / UDP {sw_u}s；切换前失败 {tcp['total_err']} 个（{count(tcp)['err']}），切换后失败 {sum(after['err'].values())}",
        {"tcp": count(tcp), "udp": count(udp), "events": [e[3] for e in c.ev()]},
    )


@case
def fo03b_app_hang_connect_only():
    """same hang, default connect-only probe: by design not detected"""
    c = Case(fo_conf(**FAST))
    c.run("tcp", f"churn target={R} conns=32 rate=100 secs=8 timeout=1000")
    c.at(2)
    c.mark("mute")
    ctl("a", "tcp.mode=mute")
    tcp = c.res("tcp")
    c.close()
    sw = c.delay("mute", "tcp")
    record(
        "FO-03b", "主节点应用假死，但只用默认握手探测（不配 tcp_expect）", "INFO",
        "设计局限：握手探测看不出应用层假死，不会切换；需要配 tcp_send/tcp_expect 或独立健康端口",
        f"切换：{'无' if sw is None else str(sw) + 's'}；假死期间失败 {tcp['total_err']}/{tcp['sent']}",
        {"tcp": count(tcp)},
    )


@case
def fo04_failback():
    """primary returns: rise + hold, established sessions untouched"""
    c = Case(fo_conf(**FAST, udp_send="hc:"))
    c.run("tcp", f"churn target={R} conns=16 rate=300 secs=24 timeout=8000")
    c.run("udp", f"udp target={R} socks=20 pps=300 secs=24")
    c.at(3)
    c.mark("kill")
    c.kill("a")
    c.at(6)
    # long-lived tcp opened while on the backup
    c.run("hold", f"hold target={R} conns=100 secs=17 interval=250 ramp=500")
    c.at(9)
    c.mark("back")
    c.start("a")
    c.at(19)
    # new udp clients after failback
    c.run("udp2", f"udp target={R} socks=10 pps=200 secs=3")
    tcp, udp, hold, udp2 = c.res("tcp"), c.res("udp"), c.res("hold"), c.res("udp2")
    alive = c.close()
    up = c.delay("back", "tcp", "up")
    fb_t, fb_u = c.delay("back", "tcp", "switch"), c.delay("back", "udp", "switch")
    sw = c.delay("kill", "tcp")
    hold_wait = None if fb_t is None or sw is None else round((c.marks["back"] + fb_t) - (c.marks["kill"] + sw), 2)
    mb = rel_ms(tcp, c.marks["back"])
    tail = count(tcp, mb + (fb_t or 99) * 1000 + 500)
    ok = alive and stray(tcp, [c.marks["kill"]]) == 0 and tcp["total_err"] <= 16 and fb_t is not None and up is not None
    ok = ok and 2.0 <= up <= 4.0 and hold_wait is not None and hold_wait >= 5.0
    ok = ok and hold["died"] == 0 and hold["tag_changed"] == 0 and set(k for k, v in hold["ok"].items() if v) == {"B"}
    ok = ok and set(udp["tag_seqs"]) <= {"AB"} and set(udp2["tag_seqs"]) == {"A"} and set(tail["ok"]) == {"A"}
    record(
        "FO-04", "主节点恢复后回切（rise + hold），已有会话不动", "PASS" if ok else "FAIL",
        "连续 5 次探测成功（约 2.5s）后才算恢复，且距上次切换满 hold=5s 才回切；回切 0 失败；"
        "已建在备上的 TCP 长连接不断、UDP 老会话留在备，新会话回主",
        f"恢复判定 {up}s，回切在恢复后 {fb_t}s（距上次切换 {hold_wait}s）；TCP 失败 {tcp['total_err']}/{tcp['sent']}，回切后新连接 {tail['ok']}；"
        f"长连接 100 条：断 {hold['died']}、换后端 {hold['tag_changed']}、分布 {count(hold)['ok']}；"
        f"UDP 老会话 {udp['tag_seqs']}，回切后新会话 {udp2['tag_seqs']}",
        {"events": [e[3] for e in c.ev()]},
    )


@case
def fo05_flapping():
    """primary link flaps with different duty cycles"""
    rows = []
    okall = True
    for down, up, fall in ((1, 1, 3), (0.3, 0.3, 3), (1, 3, 3), (2, 1, 3), (3, 3, 3), (1, 1, 2)):
        c = Case(fo_conf(**FAST, fall=fall, udp_send="hc:"))
        secs = 30
        c.run("tcp", f"churn target={R} conns=64 rate=200 secs={secs} timeout=8000")
        c.run("udp", f"udp target={R} socks=20 pps=200 secs={secs}")
        c.at(3)
        flaps = 0
        while time.time() - c.t0 < secs - 6:
            link("a", False)
            time.sleep(down)
            link("a", True)
            time.sleep(up)
            flaps += 1
        tcp, udp = c.res("tcp"), c.res("udp")
        alive = c.close()
        gaps = []
        for proto in ("tcp", "udp"):
            sw = c.ev(proto, "switch")
            for prev, cur in zip(sw, sw[1:]):
                if "=> peer#0" in cur[3]:
                    gaps.append(cur[0] - prev[0])
        sw_t, sw_u = len(c.ev("tcp", "switch")), len(c.ev("udp", "switch"))
        # invariant: a failback never happens sooner than `hold` after the previous switch
        # a request already sent to the flapping primary can lose every retransmission until the client gives up
        only_stuck = tcp["total_err"] == tcp["err"]["read_timeout"] and tcp["total_err"] <= 0.005 * tcp["sent"]
        okall = okall and alive and only_stuck and all(g >= 4.9 for g in gaps) and max(sw_t, sw_u) <= flaps + 1
        rows.append(
            f"断{down}s/通{up}s×{flaps}次（fall={fall}）：切换 TCP {sw_t} / UDP {sw_u} 次，回切距上次切换最短 "
            f"{min(gaps):.1f}s" if gaps else f"断{down}s/通{up}s×{flaps}次（fall={fall}）：切换 TCP {sw_t} / UDP {sw_u} 次，无回切"
        )
        rows[-1] += f"；TCP 失败 {tcp['total_err']}/{tcp['sent']}{'（' + str(count(tcp)['err']) + '）' if tcp['total_err'] else ''}（最大耗时 {count(tcp)['lat_max_ms']:.0f}ms），UDP 送达 {udp['total_ok'] / udp['sent']:.0%}"
    record(
        "FO-05", "主节点链路反复抖动（探测间隔 0.5s；时长 ×6 即对应默认 3s 间隔）", "PASS" if okall else "FAIL",
        "新建 TCP 连接靠单连接重试不失败（已发到抖动主节点上的请求可能偶发超时，≤0.5%）；不会比 hold 更频繁地回切；每个抖动周期至多切一次",
        "<br>".join(rows),
    )


@case
def fo06_both_down():
    """total outage, then the non-active peer comes back first"""
    c = Case(fo_conf(**FAST, udp_send="hc:"))
    c.run("tcp", f"churn target={R} conns=16 rate=200 secs=26 timeout=3000")
    c.run("udp", f"udp target={R} socks=10 pps=200 secs=26")
    c.at(3)
    c.mark("killA")
    c.kill("a")
    c.at(8)
    c.mark("killB")
    c.kill("b")
    c.at(13)
    c.mark("backA")
    c.start("a")
    c.at(20)
    c.mark("backB")
    c.start("b")
    tcp, udp = c.res("tcp"), c.res("udp")
    alive = c.close()
    sw1 = c.delay("killA", "tcp")
    during = c.ev("tcp", "switch", since="killB")
    stay = [e for e in during if e[0] < c.marks["backA"]]
    rec = c.delay("backA", "tcp")
    recu = c.delay("backA", "udp")
    f = first_ok(tcp, "A", rel_ms(tcp, c.marks["backA"]))
    first_a = None if f is None else round((f - rel_ms(tcp, c.marks["backA"])) / 1000, 2)
    tail = count(tcp, rel_ms(tcp, c.marks["backA"]) + (rec or 99) * 1000 + 300)
    ok = alive and sw1 is not None and not stay and rec is not None and rec <= 4.5 and not tail["err"]
    record(
        "FO-06", "主备全挂，随后主先恢复（此时活动节点是备）", "PASS" if ok else "FAIL",
        "全挂期间不乱切、进程不崩；主恢复后经 rise（约 2.5s）切回主，无需等 hold；之后 0 失败",
        f"A 挂后 {sw1}s 切到备；全挂期间切换 {len(stay)} 次，失败 {count(tcp, rel_ms(tcp, c.marks['killB']), rel_ms(tcp, c.marks['backA']))['err']}；"
        f"主恢复后 TCP {rec}s / UDP {recu}s 切回（首个成功连接 {first_a}s）；恢复后失败 {sum(tail['err'].values())}；备稍后恢复未引起切换",
        {"events": [e[3] for e in c.ev()], "tcp": count(tcp), "udp": count(udp)},
    )


@case
def fo07_backup_down_only():
    """backup dies while the primary is healthy: nothing may change"""
    c = Case(fo_conf(**FAST, udp_send="hc:"))
    c.run("tcp", f"churn target={R} conns=16 rate=300 secs=10 timeout=3000")
    c.run("udp", f"udp target={R} socks=20 pps=300 secs=10")
    c.at(3)
    c.kill("b")
    tcp, udp = c.res("tcp"), c.res("udp")
    alive = c.close()
    sw = len(c.ev(kind="switch"))
    down = [e[3] for e in c.ev(kind="down")]
    ok = alive and sw == 0 and tcp["total_err"] == 0 and udp["total_ok"] == udp["sent"] and len(down) == 2
    record(
        "FO-07", "只有备节点挂，主节点健康", "PASS" if ok else "FAIL",
        "流量 0 影响，不切换，只记录备节点 down",
        f"切换 {sw} 次；TCP 失败 {tcp['total_err']}/{tcp['sent']}；UDP 送达 {udp['total_ok']}/{udp['sent']}；日志 {len(down)} 条 down",
        {"events": [e[3] for e in c.ev()]},
    )


@case
def fo08_backup_never_ready():
    """backup was never healthy; primary dies; backup shows up later"""
    c = Case(fo_conf(**FAST, udp_send="hc:"), start=("a",), warm=3.0)
    c.run("tcp", f"churn target={R} conns=16 rate=200 secs=16 timeout=3000")
    c.at(3)
    c.mark("kill")
    c.kill("a")
    c.at(8)
    c.mark("startB")
    c.start("b")
    tcp = c.res("tcp")
    alive = c.close()
    early = [e for e in c.ev("tcp", "switch", since="kill") if e[0] < c.marks["startB"]]
    sw = c.delay("startB", "tcp")
    tail = count(tcp, rel_ms(tcp, c.marks["startB"]) + (sw or 99) * 1000 + 300)
    ok = alive and not early and sw is not None and sw <= 4.5 and not tail["err"] and set(tail["ok"]) == {"B"}
    record(
        "FO-08", "备节点从未健康过，主节点挂；之后备节点上线", "PASS" if ok else "FAIL",
        "备不可用时不切（切过去也没用）；备上线并连续成功 rise 次后切换",
        f"备上线前切换 {len(early)} 次；备上线后 {sw}s 切换；此后失败 {sum(tail['err'].values())}，流量 {tail['ok']}",
        {"events": [e[3] for e in c.ev()], "tcp": count(tcp)},
    )


@case
def fo09_three_peers():
    """priority chain A > B > C"""
    c = Case(fo_conf(**FAST, peers=("a", "b", "x"), udp_send="hc:"), start=("a", "b", "x"))
    c.run("tcp", f"churn target={R} conns=16 rate=300 secs=24 timeout=8000")
    c.run("udp", f"udp target={R} socks=20 pps=300 secs=24")
    c.at(3)
    c.mark("killA")
    c.kill("a")
    c.at(8)
    c.mark("killB")
    c.kill("b")
    c.at(12)
    c.mark("backA")
    c.start("a")
    tcp, udp = c.res("tcp"), c.res("udp")
    alive = c.close()
    seq = [e[3].split("switch ")[1] for e in c.ev("tcp", "switch")]
    chain = [s.split(" ")[0] + ">" + s.split("=> ")[1].split(" ")[0] for s in seq]
    ok = alive and stray(tcp, [c.marks["killA"], c.marks["killB"]]) == 0 and tcp["total_err"] <= 32 and chain == ["peer#0>peer#1", "peer#1>peer#2", "peer#2>peer#0"]
    ok = ok and set(udp["tag_seqs"]) <= {"ABC"} and udp["stale"] == 0
    record(
        "FO-09", "三节点优先级链 A > B > C", "PASS" if ok else "FAIL",
        "A 挂切 B，B 挂切 C，A 恢复后回 A（越过仍然挂着的 B）；新建 TCP 连接 0 失败（只有被杀瞬间在途的请求会断）",
        f"切换序列 {chain}；TCP 失败 {tcp['total_err']}/{tcp['sent']}（{count(tcp)['err'] or '无'}，{tcp['first_errs'][:2]}），分布 {count(tcp)['ok']}；UDP 路径 {udp['tag_seqs']}（老会话留在 C）",
        {"events": [e[3] for e in c.ev()]},
    )


@case
def fo10_probe_port_vs_data_port():
    """separate health port: what happens when only one of the two fails"""
    # a) data port refused, health port fine
    c = Case(fo_conf(**FAST, probe_port=5001, tcp_send="ping", tcp_expect="OK", udp_send="hc:"))
    c.run("tcp", f"churn target={R} conns=16 rate=200 secs=8 timeout=3000")
    c.run("udp", f"udp target={R} socks=10 pps=200 secs=8")
    c.at(3)
    c.mark("f")
    nsx("a", "iptables -I INPUT -p tcp --dport 5000 -j REJECT --reject-with tcp-reset")
    nsx("a", "iptables -I INPUT -p udp --dport 5000 -j DROP")
    tcp, udp = c.res("tcp"), c.res("udp")
    c.close()
    sw = len(c.ev(kind="switch"))
    m = rel_ms(udp, c.marks["f"])
    u_after = count(udp, m + 500)
    record(
        "FO-10a", "独立健康端口正常，但数据端口坏了", "INFO",
        "探测看不到数据端口，不会全局切换；TCP 被拒绝时靠单连接重试仍然成功；UDP 无法感知（局限）",
        f"切换 {sw} 次；TCP 失败 {tcp['total_err']}/{tcp['sent']}，分布 {count(tcp)['ok']}；UDP 故障后送达 {sum(u_after['ok'].values())}/{u_after['sent']}",
    )
    # b) health port dies, data port fine
    c = Case(fo_conf(**FAST, probe_port=5001, tcp_send="ping", tcp_expect="OK", udp_send="hc:"))
    c.run("hold", f"hold target={R} conns=100 secs=9 interval=200 ramp=500")
    c.at(1)
    c.run("tcp", f"churn target={R} conns=16 rate=200 secs=8 timeout=3000")
    c.run("udp", f"udp target={R} socks=10 pps=200 secs=8")
    c.at(4)
    c.mark("f")
    ctl("a", "health.mode=close", "hudp.mode=mute")
    tcp, udp, hold = c.res("tcp"), c.res("udp"), c.res("hold")
    alive = c.close()
    sw_t, sw_u = c.delay("f", "tcp"), c.delay("f", "udp")
    ok = alive and sw_t is not None and sw_u is not None and tcp["total_err"] == 0 and hold["died"] == 0
    ok = ok and hold["tag_changed"] == 0 and udp["total_ok"] == udp["sent"]
    record(
        "FO-10b", "健康端口失效但数据端口正常（探测判定下线）", "PASS" if ok else "FAIL",
        "按探测结果切换；过程 0 丢失：新连接去备，主上已建的 100 条 TCP 长连接继续可用、不迁移",
        f"切换 TCP {sw_t}s / UDP {sw_u}s；TCP 失败 {tcp['total_err']}/{tcp['sent']}；长连接断 {hold['died']}、换后端 {hold['tag_changed']}、"
        f"分布 {count(hold)['ok']}；UDP 送达 {udp['total_ok']}/{udp['sent']}，路径 {udp['tag_seqs']}",
        {"events": [e[3] for e in c.ev()]},
    )


@case
def fo11_slow_primary_default_timing():
    """latency below / above the probe timeout, default 3s/1.5s timing"""
    c = Case(fo_conf(tcp_send="ping", tcp_expect="OK", udp_send="hc:"), warm=4)
    netem("a", "delay 300ms")  # rtt 600ms: probe = 2 rtt = 1.2s < 1.5s
    c.run("tcp", f"churn target={R} conns=8 rate=10 secs=45 timeout=8000")
    c.run("udp", f"udp target={R} socks=5 pps=20 secs=45")
    tcp, udp = c.res("tcp"), c.res("udp")
    n_ev = len(c.ev())
    c.mark("slow")
    netem("a", "delay 1000ms")  # rtt 2s > timeout
    c.run("tcp2", f"churn target={R} conns=8 rate=10 secs=20 timeout=8000")
    tcp2 = c.res("tcp2")
    alive = c.close()
    sw_t, sw_u = c.delay("slow", "tcp"), c.delay("slow", "udp")
    ok = alive and n_ev == 0 and tcp["total_err"] == 0 and udp["total_ok"] == udp["sent"]
    ok = ok and sw_t is not None and 6 <= sw_t <= 12 and sw_u is not None and tcp2["total_err"] == 0
    record(
        "FO-11", "主节点变慢（默认参数：3s 间隔 / 1.5s 超时）", "PASS" if ok else "FAIL",
        "往返 600ms（探测耗时约 1.2s，未超时）：45 秒内 0 事件、0 失败；往返 2s（超过超时）：约 7.5~10.5s 切换",
        f"慢但未超时：事件 {n_ev} 条，TCP 失败 {tcp['total_err']}/{tcp['sent']}，UDP 送达 {udp['total_ok']}/{udp['sent']}；"
        f"超时后切换 TCP {sw_t}s / UDP {sw_u}s，期间 TCP 失败 {tcp2['total_err']}/{tcp2['sent']}",
        {"events": [e[3] for e in c.ev()]},
    )


@case
def fo13_abusive_clients():
    """misbehaving clients must not look like a backend failure or leak"""
    c = Case(fo_conf(**FAST, tcp_send="ping", tcp_expect="OK", udp_send="hc:"))
    fd0, rss0 = nfd(c.pid), rss_kb(c.pid)
    done_n = {}
    for kind in ("rst", "half", "midrst", "idle"):
        out = tool("c", f"abuse target={R} conns=200 secs=6 kind={kind}")
        done_n[kind] = out["done"]
    time.sleep(3)
    fd1, rss1 = nfd(c.pid), rss_kb(c.pid)
    tcp = tool("c", f"churn target={R} conns=16 rate=300 secs=3")
    n_ev = len(c.ev())
    alive = c.close()
    ok = alive and n_ev == 0 and tcp["total_err"] == 0 and fd1 <= fd0 + 2
    record(
        "FO-13", "异常客户端（连上即 RST、半包后半关闭、传输中途 RST、连上不说话）", "PASS" if ok else "FAIL",
        "客户端行为不构成后端故障：0 事件；不泄漏句柄；之后正常请求 0 失败",
        f"异常连接数 {done_n}；failover 事件 {n_ev} 条；fd {fd0} → {fd1}，RSS {rss0} → {rss1} kB；之后 TCP 失败 {tcp['total_err']}/{tcp['sent']}",
    )


@case
def fo14_idle_and_silent():
    """no traffic / silent sessions with default timing"""
    c = Case(fo_conf(tcp_send="ping", tcp_expect="OK", udp_send="hc:", udp_timeout=5), warm=4)
    # one long-lived tcp connection speaking every 20s, one udp socket every 8s (> udp_timeout)
    c.run("hold", f"hold target={R} conns=5 secs=65 interval=20000 ramp=100")
    c.run("udp", f"udp target={R} socks=2 pps=0.25 secs=65 spread=100")
    hold, udp = c.res("hold"), c.res("udp")
    n_ev = len(c.ev())
    alive = c.close()
    ok = alive and n_ev == 0 and hold["died"] == 0 and hold["total_err"] == 0 and udp["total_ok"] == udp["sent"]
    record(
        "FO-14", "长时间空闲 / 静默会话（默认参数，65 秒）", "PASS" if ok else "FAIL",
        "没有流量不是故障：0 事件；20 秒才说一句话的 TCP 长连接不断；UDP 会话超时重建后照常",
        f"事件 {n_ev} 条；TCP 长连接断 {hold['died']}、请求 {hold['total_ok']}/{hold['sent']}；UDP 送达 {udp['total_ok']}/{udp['sent']}",
    )


@case
def fo15_ipv6():
    """ipv6 peers"""
    c = Case(fo_conf(**FAST, v6=True, listen="[::]:6000", udp_send="hc:"))
    sh("true")
    for ns in ("a", "b"):
        c.back[ns].kill()
    time.sleep(0.2)
    c.back = {ns: backend(ns, TAG[ns], v6=True) for ns in ("a", "b")}
    time.sleep(6)  # both went down while we swapped the backends: wait for rise
    c.t0 = time.time()
    r6 = f"[{ip6('r')}]:6000"
    c.run("tcp", f"churn target={r6} conns=16 rate=300 secs=10 timeout=8000")
    c.run("udp", f"udp target={r6} socks=20 pps=300 secs=10")
    c.at(4)
    c.mark("kill")
    c.kill("a")
    tcp, udp = c.res("tcp"), c.res("udp")
    alive = c.close()
    sw_t, sw_u = c.delay("kill", "tcp"), c.delay("kill", "udp")
    ok = alive and stray(tcp, [c.marks["kill"]]) == 0 and tcp["total_err"] <= 16 and sw_t is not None and sw_u is not None and set(udp["tag_seqs"]) <= {"AB"}
    record(
        "FO-15", "IPv6 节点", "PASS" if ok else "FAIL",
        "与 IPv4 行为一致",
        f"TCP 失败 {tcp['total_err']}/{tcp['sent']}，切换 TCP {sw_t}s / UDP {sw_u}s；UDP 路径 {udp['tag_seqs']}",
        {"events": [e[3] for e in c.ev()]},
    )


@case
def fo16_udp_late_replies():
    """replies of the old peer arriving after the move must not reach the client"""
    c = Case(fo_conf(**FAST, udp_send="hc:"))
    c.run("udp", f"udp target={R} socks=50 pps=1000 secs=12")
    c.at(4)
    c.mark("slow")
    netem("a", "delay 400ms")  # probes time out (rtt 800ms > 300ms), data replies arrive 0.8s late
    udp = c.res("udp")
    sa = stats("a")
    alive = c.close()
    sw = c.delay("slow", "udp")
    m = rel_ms(udp, c.marks["slow"])
    a_before = count(udp, 0, m)["ok"].get("A", 0)
    a_total = udp["ok"]["A"]
    echoed = sa["udp"]["data_pkts"]
    dropped = echoed - a_total
    ok = alive and sw is not None and udp["stale"] == 0 and set(udp["tag_seqs"]) <= {"AB"} and dropped > 0
    record(
        "FO-16", "UDP 换线后，旧节点的迟到回包", "PASS" if ok else "FAIL",
        "换线后旧节点晚到的回包被丢弃，客户端看不到乱序的旧节点应答（同一会话只会 A→B）",
        f"切换 {sw}s；A 实际回了 {echoed} 个包，客户端收到 {a_total} 个（其中故障前 {a_before}），其余 {dropped} 个迟到包被拦下；"
        f"客户端乱序旧回包 {udp['stale']}，路径 {udp['tag_seqs']}",
        {"udp": count(udp)},
    )


@case
def fo17_udp_backend_without_echo():
    """default udp probe against services that do not echo"""
    c = Case(fo_conf(**FAST, probe_port=5001, tcp_send="ping", tcp_expect="OK"), warm=0.5)
    ctl("a", "hudp.mode=mute")
    ctl("b", "hudp.mode=mute")
    time.sleep(3)
    c.t0 = time.time()
    c.run("tcp", f"churn target={R} conns=16 rate=200 secs=8 timeout=3000")
    c.run("udp", f"udp target={R} socks=10 pps=200 secs=8")
    c.at(3)
    c.mark("kill")
    c.kill("a")
    tcp, udp = c.res("tcp"), c.res("udp")
    alive = c.close()
    downs = [e[3] for e in c.ev("udp", "down")]
    sw_t, sw_u = c.delay("kill", "tcp"), c.delay("kill", "udp")
    m = rel_ms(udp, c.marks["kill"])
    before = count(udp, 0, m)
    ok = alive and sw_t is not None and sw_u is None and before["ok"].get("A", 0) == before["sent"] and tcp["total_err"] == 0
    record(
        "FO-17", "UDP 后端不回显探测包（没有 echo 服务）", "INFO" if ok else "FAIL",
        "UDP 探测全失败 → 两个节点都判 down → 保持在主上照常转发，但 UDP 不具备切换能力；TCP 不受影响、照常切换",
        f"UDP down 日志 {len(downs)} 条；故障前 UDP 送达 {sum(before['ok'].values())}/{before['sent']}；主挂后 UDP 切换："
        f"{'无' if sw_u is None else sw_u}，TCP 切换 {sw_t}s、失败 {tcp['total_err']}/{tcp['sent']}",
        {"events": [e[3] for e in c.ev()]},
    )


@case
def fo18_udp_all_down_no_move():
    """an association on the backup must not be shuffled when every peer looks down"""
    c = Case(fo_conf(**FAST, probe_port=5001, udp_send="hc:"))
    c.run("udp", f"udp target={R} socks=20 pps=400 secs=17")
    c.at(2)
    ctl("a", "hudp.mode=mute")  # only the health port: data keeps working everywhere
    c.at(5)
    ctl("a", "hudp.mode=ok")  # A comes back, new sessions fail back to A after hold
    c.at(11.5)
    c.mark("alldown")
    ctl("a", "hudp.mode=mute")
    ctl("b", "hudp.mode=mute")
    udp = c.res("udp")
    alive = c.close()
    fb = [e for e in c.ev("udp", "switch") if e[0] < c.marks["alldown"]]
    downs = c.ev("udp", "down", since="alldown")
    ok = alive and len(fb) == 2 and len(downs) == 2 and set(udp["tag_seqs"]) == {"AB"} and udp["total_ok"] == udp["sent"]
    record(
        "FO-18", "UDP 老会话留在备上，随后主备探测同时失败（全部判 down）", "PASS" if ok else "FAIL",
        "全部 down 时没有更好的去处：老会话原地不动（路径保持 A→B），0 丢包",
        f"全 down 前切换 {len(fb)} 次（去备、回主）；全 down 日志 {len(downs)} 条；会话路径 {udp['tag_seqs']}；送达 {udp['total_ok']}/{udp['sent']}，旧回包 {udp['stale']}",
        {"events": [e[3] for e in c.ev()]},
    )


@case
def fo19_through_with_port():
    """through = ip:port pins the udp source port: probes must not take replies away from the session"""
    extra = f'through = "{ip4("r")}:40000"\n[endpoints.network]\nno_tcp = true\nuse_udp = true\n'
    c = Case(fo_conf(**FAST, udp_send="hc:", extra=extra))
    netem("a", "delay 20ms")
    netem("b", "delay 20ms")
    time.sleep(1.5)
    c.t0 = time.time()
    c.run("udp", f"udp target={R} socks=1 pps=2000 secs=10 spread=1")
    udp = c.res("udp")
    ev = [e[3] for e in c.ev()]
    alive = c.close()
    lost = udp["sent"] - udp["total_ok"]
    ok = alive and lost == 0 and not ev
    record(
        "FO-19", "UDP 固定出口端口（through = ip:port），后端往返 40ms", "PASS" if ok else "FAIL",
        "探测不能和业务会话抢同一个本地端口：业务回包 0 丢失、0 误判",
        f"送达 {udp['total_ok']}/{udp['sent']}（丢 {lost}，{lost / max(1, udp['sent']):.1%}）；failover 事件 {len(ev)} 条 {ev[:2]}",
    )


@case
def fo22_tcp_udp_independent():
    """only udp of the primary dies"""
    c = Case(fo_conf(**FAST, udp_send="hc:"))
    c.run("tcp", f"churn target={R} conns=16 rate=300 secs=10 timeout=3000")
    c.run("udp", f"udp target={R} socks=20 pps=300 secs=10")
    c.at(4)
    c.mark("f")
    ctl("a", "udp.mode=mute")
    tcp, udp = c.res("tcp"), c.res("udp")
    alive = c.close()
    sw_t, sw_u = c.delay("f", "tcp"), c.delay("f", "udp")
    ok = alive and sw_t is None and sw_u is not None and tcp["total_err"] == 0 and set(count(tcp)["ok"]) == {"A"}
    record(
        "FO-22", "只有主节点的 UDP 服务挂了", "PASS" if ok else "FAIL",
        "TCP 与 UDP 各自独立判定：UDP 切到备，TCP 留在主且 0 失败",
        f"UDP 切换 {sw_u}s，路径 {udp['tag_seqs']}；TCP 切换 {'无' if sw_t is None else sw_t}，失败 {tcp['total_err']}/{tcp['sent']}，分布 {count(tcp)['ok']}",
    )


@case
def fo23_default_timing_full_cycle():
    """real default numbers: detect, rise, hold, failback"""
    c = Case(fo_conf(tcp_send="ping", tcp_expect="OK", udp_send="hc:"), warm=4)
    c.run("tcp", f"churn target={R} conns=64 rate=100 secs=75 timeout=8000")
    c.run("udp", f"udp target={R} socks=20 pps=100 secs=75")
    c.at(5)
    c.mark("drop")
    blackhole("a")
    c.at(25)
    c.mark("back")
    blackhole("a", False)
    tcp, udp = c.res("tcp"), c.res("udp")
    alive = c.close()
    sw_t, sw_u = c.delay("drop", "tcp"), c.delay("drop", "udp")
    up = c.delay("back", "tcp", "up")
    fb = c.delay("back", "tcp", "switch")
    hold_wait = None if fb is None or sw_t is None else round((c.marks["back"] + fb) - (c.marks["drop"] + sw_t), 1)
    ugap, _ = gap(udp, c.marks["drop"], "A", "B")
    ok = alive and tcp["total_err"] == 0 and sw_t is not None and 6 <= sw_t <= 11 and sw_u is not None and 6 <= sw_u <= 11
    ok = ok and up is not None and 12 <= up <= 16.5 and hold_wait is not None and hold_wait >= 30
    record(
        "FO-23", "默认参数完整周期：黑洞 → 切换 → 恢复 → 回切", "PASS" if ok else "FAIL",
        "黑洞后 7.5~10.5s 切换；恢复后连续 5 次成功（12~15s）判定恢复；距上次切换满 30s 才回切；TCP 全程 0 失败",
        f"切换 TCP {sw_t}s / UDP {sw_u}s（UDP 中断 {ugap}ms，丢 {udp['sent'] - udp['total_ok']}/{udp['sent']}）；"
        f"恢复判定 {up}s；回切距上次切换 {hold_wait}s；TCP 失败 {tcp['total_err']}/{tcp['sent']}，最大耗时 {count(tcp)['lat_max_ms']}ms",
        {"events": [e[3] for e in c.ev()], "tcp": count(tcp)},
    )


if __name__ == "__main__":
    want = sys.argv[1:]
    net_up()
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
    save("fo" + ("-" + "-".join(want) if want else ""))
    net_down()
    bad = [r["id"] for r in RESULTS if r["status"] == "FAIL"]
    print("FAILED:", bad if bad else "none")
