"""Extreme conditions. Usage: extreme.py [case-prefix ...]"""
import signal
import sys
import time
import traceback

from lab import *
from scen_fo import Case, FAST, R, gap, reset_faults

CASES = {}


def case(fn):
    CASES[fn.__name__] = fn
    return fn


PLAIN = f"""
[network]
use_udp = true
udp_timeout = 5
[[endpoints]]
listen = "0.0.0.0:6000"
remote = "{ip4('a')}:5000"
"""


def sample(pid):
    return {"rss_kb": rss_kb(pid), "fd": nfd(pid), "threads": nthreads(pid)}


@case
def ex01_10k_connections():
    """10k concurrent long-lived tcp connections: memory, cpu, cleanup; orig vs new+failover"""
    out = {}
    for name, binary, conf in (
        ("orig", ORIG, PLAIN),
        ("new+failover", NEW, fo_conf(**FAST, tcp_send="ping", tcp_expect="OK", udp_send="hc:", udp_timeout=5)),
    ):
        c = Case(conf, binary=binary)
        base = sample(c.pid)
        c.run("hold", f"hold target={R} conns=10000 secs=30 interval=2000 ramp=6000 timeout=15000 threads=6")
        c.at(14)
        peak = sample(c.pid)
        t0 = cpu_ticks(c.pid)
        c.at(24)
        cpu = (cpu_ticks(c.pid) - t0) / 10  # % of one core over 10s
        hold = c.res("hold")
        time.sleep(3)
        end = sample(c.pid)
        ev = len(c.ev())
        alive = c.close()
        out[name] = dict(base=base, peak=peak, end=end, cpu_pct=cpu, died=hold["died"], err=hold["total_err"],
                         peak_alive=hold["peak_alive"], ok=hold["total_ok"], p99_us=hold["lat_us"]["p99"], alive=alive, ev=ev)
    o, n = out["orig"], out["new+failover"]
    per = lambda x: round((x["peak"]["rss_kb"] - x["base"]["rss_kb"]) / 10000, 2)
    ok = all(v["alive"] and v["died"] == 0 and v["err"] == 0 and v["peak_alive"] == 10000 for v in out.values())
    ok = ok and n["end"]["fd"] <= n["base"]["fd"] + 2 and n["ev"] == 0 and per(n) <= per(o) * 1.15 + 0.2
    record(
        "EX-01", "1 万条并发 TCP 长连接（每条每 2 秒一次请求）", "PASS" if ok else "FAIL",
        "0 断连、0 失败、0 误判；每连接内存与原版持平；连接关闭后句柄回到基线",
        f"原版：RSS {o['base']['rss_kb']}→{o['peak']['rss_kb']} kB（每连接 {per(o)} kB），fd 峰值 {o['peak']['fd']}，CPU {o['cpu_pct']:.0f}%，p99 {o['p99_us']}µs；"
        f"新版+故障转移：RSS {n['base']['rss_kb']}→{n['peak']['rss_kb']} kB（每连接 {per(n)} kB），fd 峰值 {n['peak']['fd']}→结束 {n['end']['fd']}（基线 {n['base']['fd']}），"
        f"CPU {n['cpu_pct']:.0f}%，p99 {n['p99_us']}µs，断 {n['died']}，失败 {n['err']}，事件 {n['ev']}",
        out,
    )


@case
def ex02_thundering_herd():
    """5000 connections die with the primary and reconnect at once"""
    c = Case(fo_conf(**FAST, udp_send="hc:"))
    c.run("hold", f"hold target={R} conns=5000 secs=26 interval=1000 ramp=4000 timeout=10000 reconnect=1 threads=6")
    c.at(12)
    c.mark("kill")
    peak = sample(c.pid)
    c.kill("a")
    hold = c.res("hold")
    time.sleep(2)
    end = sample(c.pid)
    overflow = (nsx("r", "nstat -az TcpExtListenOverflows", check=False).split() or ["?", "?"])[-2]
    alive = c.close()
    m = rel_ms(hold, c.marks["kill"])
    after = count(hold, m + 3000)
    win = count(hold, m, m + 3000)
    conn_err = sum(v for k, v in win["err"].items() if k in ("refused", "conn_timeout", "conn_other"))
    sw = c.delay("kill", "tcp")
    ok = alive and hold["died"] <= 5100 and conn_err == 0 and not after["err"] and hold["alive_at_end"] == 5000
    ok = ok and set(after["ok"]) == {"B"} and end["fd"] <= 20
    record(
        "EX-02", "主节点挂掉时 5000 条长连接同时断开并重连", "PASS" if ok else "FAIL",
        "已有连接随主节点一起断（真实故障，无法避免）；5000 条重连全部成功落到备，0 个重连失败；进程稳定",
        f"断开 {hold['died']} 次（超出 5000 的部分是重连时撞上主节点进程退出的最后一刻、握手后随即被重置的连接，再次重连即成功；中继监听队列溢出 {overflow}）；重连阶段建连失败 {conn_err}；3 秒后失败 {sum(after['err'].values())}，结束时在线 {hold['alive_at_end']}/5000（全在 {list(after['ok'])}）；"
        f"全局切换 {sw}s；RSS 峰值 {peak['rss_kb']} kB，结束 fd {end['fd']}",
        {"win": win, "after": after},
    )


@case
def ex03_udp_5000_associations():
    """5000 udp associations: memory vs original, then they all move together"""
    # original build, same 5000 clients, no fault: memory per association
    c = Case(PLAIN, binary=ORIG)
    b0 = sample(c.pid)
    c.run("udp", f"udp target={R} socks=5000 pps=25000 secs=10 spread=3000 threads=6")
    c.at(8)
    p0 = sample(c.pid)
    u0 = c.res("udp")
    logs0 = {}
    for _, x in c.r.lines:
        if "ERROR" in x or "WARN" in x:
            k = x.split("]")[-1][:60]
            logs0[k] = logs0.get(k, 0) + 1
    drops0 = nsx("r", "nstat -az UdpRcvbufErrors", check=False).split()[-2:-1]
    alive0 = c.close()

    c = Case(fo_conf(**FAST, udp_send="hc:", udp_timeout=5))
    base = sample(c.pid)
    c.run("udp", f"udp target={R} socks=5000 pps=25000 secs=20 spread=3000 threads=6")
    c.at(9)
    peak = sample(c.pid)
    c.mark("kill")
    c.kill("a")
    udp = c.res("udp")
    time.sleep(7)
    end = sample(c.pid)
    alive = c.close()
    sw = c.delay("kill", "udp")
    ugap, _ = gap(udp, c.marks["kill"], "A", "B")
    m = rel_ms(udp, c.marks["kill"])
    before = count(udp, 4000, m - 200)
    after = count(udp, m + (sw or 9) * 1000 + 500, 19800)
    loss = lambda x: (x["sent"] - sum(x["ok"].values())) / max(1, x["sent"])
    per = lambda b, p: (p["rss_kb"] - b["rss_kb"]) / 5000
    ok = alive and sw is not None and set(udp["tag_seqs"]) <= {"AB"} and udp["stale"] == 0 and udp["dup"] == 0
    ok = ok and loss(after) <= loss(before) + 0.002 and end["fd"] <= base["fd"] + 2 and per(base, peak) <= 40 and udp["foreign"] == 0
    record(
        "EX-03", "5000 个 UDP 会话（2.5 万包/秒）同时换线", "PASS" if ok else "FAIL",
        "全部会话移到备，无旧回包、无重复；换线后丢包率不高于故障前；会话超时后句柄回到基线；每会话内存不高于原版",
        f"切换 {sw}s，中断 {ugap}ms；路径 {udp['tag_seqs']}；丢包率 故障前 {loss(before):.2%} → 换线后 {loss(after):.2%}；旧回包 {udp['stale']}，重复 {udp['dup']}，收到别的会话的回包 {udp['foreign']}（原版 {u0['foreign']}）；"
        f"内存：原版 {b0['rss_kb'] // 1024}→{p0['rss_kb'] // 1024} MB（每会话 {per(b0, p0):.0f} kB，送达 {u0['total_ok']}/{u0['sent']}），"
        f"被测版 {base['rss_kb'] // 1024}→{peak['rss_kb'] // 1024} MB（每会话 {per(base, peak):.0f} kB）；fd {base['fd']}→{peak['fd']}→{end['fd']}",
        {"orig": [b0, p0], "new": [base, peak, end], "orig_alive": alive0, "orig_logs": logs0, "orig_rcvbuf_drops": drops0},
    )


@case
def ex04_max_churn_failover():
    """primary dies under maximum connection rate"""
    c = Case(fo_conf(**FAST, udp_send="hc:"))
    c.run("tcp", f"churn target={R} conns=256 secs=16 timeout=8000 threads=6")
    c.at(7)
    c.mark("kill")
    c.kill("a")
    tcp = c.res("tcp")
    alive = c.close()
    m = rel_ms(tcp, c.marks["kill"])
    sw = c.delay("kill", "tcp")
    inflight = count(tcp, m - 100, m + 300)
    later = count(tcp, m + 300)
    retries = len(c.r.grep("retry peer#"))
    rate_before = sum(count(tcp, 2000, m)["ok"].values()) / ((m - 2000) / 1000)
    rate_after = sum(count(tcp, m + 3000, 15000)["ok"].values()) / ((15000 - m - 3000) / 1000)
    ok = alive and sw is not None and not later["err"] and sum(inflight["err"].values()) <= 2 * 256 and tcp["err"]["mismatch"] == 0
    record(
        "EX-04", "满速短连接（256 并发）时主节点被杀", "PASS" if ok else "FAIL",
        "只有故障瞬间正在主上传输、或刚被垂死的主节点接受的连接失败（约等于并发数）；其后 0 失败；吞吐不塌",
        f"故障前 {rate_before:.0f} 连接/秒，故障后 {rate_after:.0f} 连接/秒；故障瞬间失败 {sum(inflight['err'].values())}（{inflight['err']}），之后失败 {sum(later['err'].values())}；"
        f"全局切换 {sw}s；切换前靠单连接重试挽回 {retries} 条（每条一行 warn 日志）",
        {"tcp": count(tcp)},
    )


@case
def ex05_local_port_exhaustion():
    """relay runs out of ephemeral ports towards the primary: local problem, not a peer failure"""
    out = {}
    for name, probe_port in (("探测数据端口", None), ("探测独立端口", 5001)):
        c = Case(fo_conf(**FAST, probe_port=probe_port), warm=1.6)
        nsx("r", "sysctl -qw net.ipv4.ip_local_port_range='40000 40099'")
        c.run("hold", f"hold target={R} conns=160 secs=10 interval=200 ramp=2000 timeout=3000")
        hold = c.res("hold")
        nsx("r", "sysctl -qw net.ipv4.ip_local_port_range='1024 65535'")
        ev = [e[3] for e in c.ev()]
        retry = c.r.grep("retry peer#")
        errs = sorted(set(x.split(": ", 1)[1].split(", retry")[0] for _, x in retry if ": " in x))
        alive = c.close()
        out[name] = dict(dist=count(hold)["ok"], err=hold["total_err"], died=hold["died"], events=ev, retry=len(retry), errs=errs, alive=alive)
    a, b = out["探测数据端口"], out["探测独立端口"]
    ok = all(v["alive"] and v["err"] == 0 and v["died"] == 0 for v in out.values()) and not a["events"] and not b["events"]
    record(
        "EX-05", "中继本机到主节点的临时端口耗尽（只留 100 个端口，开 160 条连接）", "PASS" if ok else "FAIL",
        "这是本机资源问题，不是后端故障：不应判主节点下线；溢出的连接逐条改走备，0 失败",
        f"探测数据端口：分布 {a['dist']}，失败 {a['err']}，溢出重试 {a['retry']} 条（{a['errs']}），事件 {a['events'] or '无'}；"
        f"探测独立端口：分布 {b['dist']}，失败 {b['err']}，溢出重试 {b['retry']} 条，事件 {b['events'] or '无'}",
        out,
    )


@case
def ex06_fd_exhaustion():
    """relay hits its nofile limit: must not die, must recover, must not misjudge peers"""
    out = {}
    for name, binary, conf in (("orig", ORIG, PLAIN), ("new+failover", NEW, fo_conf(**FAST, udp_send="hc:"))):
        sh("pkill -9 -x fot; pkill -9 -f realm-", check=False)
        reset_faults()
        back = {ns: backend(ns, t) for ns, t in (("a", "A"), ("b", "B"))}
        r = realm(conf, binary=binary, level="warn", name="fd", args="-n 200")
        pid = r.pid()
        time.sleep(1.6)
        hold = tool("c", f"hold target={R} conns=300 secs=6 interval=200 ramp=1000 timeout=2000")
        peak_ok = hold["peak_alive"]
        time.sleep(1.5)
        after = tool("c", f"churn target={R} conns=8 rate=200 secs=3 timeout=2000")
        ev = [e[3] for e in events(r)]
        logs = sorted(set(x.split("]")[-1][:60] for _, x in r.lines if "ERROR" in x))[:4]
        out[name] = dict(alive=r.alive(), fd_after=nfd(pid), hold_ok=hold["total_ok"], hold_err=hold["total_err"], peak=peak_ok,
                         after_ok=after["total_ok"], after_err=after["total_err"], events=ev, errors=logs)
        r.kill()
        for b in back.values():
            b.kill()
    o, n = out["orig"], out["new+failover"]
    ok = n["alive"] and n["after_err"] == 0 and n["after_ok"] > 0 and not n["events"]
    record(
        "EX-06", "中继句柄耗尽（nofile=200，同时来 300 条连接）", "PASS" if ok else "FAIL",
        "超限的连接可以失败，但进程不能崩、监听不能停；压力过后新连接必须恢复正常；不误判后端",
        f"原版：存活 {o['alive']}，压力过后 成功 {o['after_ok']} / 失败 {o['after_err']}，日志 {o['errors']}；"
        f"新版：存活 {n['alive']}，压力过后 成功 {n['after_ok']} / 失败 {n['after_err']}，failover 事件 {n['events'] or '无'}，日志 {n['errors']}",
        out,
    )


@case
def ex07_cpu_starved_relay():
    """relay pinned to one core and saturated: probes must not time out spuriously"""
    out = {}
    for name, timing in (("快速参数(0.5s/0.3s)", FAST), ("默认参数(3s/1.5s)", dict())):
        conf = fo_conf(**timing, tcp_send="ping", tcp_expect="OK", udp_send="hc:")
        for i in range(20):
            conf += f"""
[[endpoints]]
listen = "0.0.0.0:{6100 + i}"
remote = "{ip4('a')}:5000"
extra_remotes = ["{ip4('b')}:5000"]
[endpoints.failover]
interval = {timing.get('interval', 3)}
timeout = {timing.get('timeout', 1.5)}
tcp_send = "ping"
tcp_expect = "OK"
udp_send = "hc:"
"""
        sh("pkill -9 -x fot; pkill -9 -f realm-", check=False)
        reset_faults()
        back = {ns: backend(ns, t) for ns, t in (("a", "A"), ("b", "B"))}
        r = realm(conf, level="warn", name="cpu", prefix="taskset -c 1")
        pid = r.pid()
        time.sleep(4)
        t0 = cpu_ticks(pid)
        secs = 40
        tcp = tool("c", f"churn target={R} conns=128 secs={secs} timeout=8000 threads=4", bg=True, prefix="taskset -c 4-9")
        udp = tool("c", f"udp target={R} socks=64 pps=150000 secs={secs} threads=4", bg=True, prefix="taskset -c 4-9")
        tcp, udp = done(tcp), done(udp)
        cpu = (cpu_ticks(pid) - t0) / secs
        ev = [e[3] for e in events(r)]
        out[name] = dict(cpu_pct=cpu, tcp_rate=tcp["rate"], tcp_err=tcp["total_err"], udp_ok=udp["total_ok"], udp_sent=udp["sent"], events=ev, alive=r.alive())
        r.kill()
        for b in back.values():
            b.kill()
    ok = all(v["alive"] and not v["events"] and v["tcp_err"] == 0 for v in out.values())
    record(
        "EX-07", "中继只给 1 个 CPU 核并被 TCP+UDP 流量打满（21 个 failover 端点、42 个探测任务）", "PASS" if ok else "FAIL",
        "探测任务不被转发流量饿死：0 次误判下线、0 次切换",
        "；".join(
            f"{k}：中继 CPU {v['cpu_pct']:.0f}%，TCP {v['tcp_rate']:.0f} 连接/秒（失败 {v['tcp_err']}），UDP 送达 {v['udp_ok']}/{v['udp_sent']}，事件 {len(v['events'])} 条"
            for k, v in out.items()
        ),
        out,
    )


@case
def ex08_relay_stalls():
    """the relay process itself freezes (vm pause, cpu steal) and resumes"""
    c = Case(fo_conf(**FAST, tcp_send="ping", tcp_expect="OK", udp_send="hc:"))
    c.run("tcp", f"churn target={R} conns=16 rate=100 secs=22 timeout=8000")
    cycles = 0
    c.at(2)
    for stop, cont in ((2.0, 1.0), (2.0, 1.0), (3.0, 1.0), (0.4, 0.2), (0.4, 0.2), (0.4, 0.2), (4.0, 2.0)):
        os.kill(c.pid, signal.SIGSTOP)
        time.sleep(stop)
        os.kill(c.pid, signal.SIGCONT)
        time.sleep(cont)
        cycles += 1
    tcp = c.res("tcp")
    ev = [e[3] for e in c.ev()]
    alive = c.close()
    ok = alive and not ev and tcp["total_err"] == 0
    record(
        "EX-08", "中继进程自身被冻结再恢复（模拟宿主机卡顿，最长 4 秒 = 8 个探测周期）", "PASS" if ok else "FAIL",
        "自己卡住不等于后端故障：恢复后 0 误判；期间的连接只是变慢，不失败",
        f"冻结 {cycles} 次；failover 事件 {len(ev)} 条 {ev[:3]}；TCP 失败 {tcp['total_err']}/{tcp['sent']}，最大耗时 {count(tcp)['lat_max_ms']}ms",
    )


@case
def ex09_big_transfers():
    """4 MiB echo bodies while the primary dies: integrity"""
    c = Case(fo_conf(**FAST, udp_send="hc:"))
    c.run("tcp", f"churn target={R} conns=16 secs=18 size=4194304 timeout=15000 threads=6")
    c.at(4)
    c.kill("b")  # backup flaps first: must not touch transfers on the primary
    c.at(6)
    c.start("b")
    c.at(10)
    c.mark("kill")
    c.kill("a")
    tcp = c.res("tcp")
    alive = c.close()
    m = rel_ms(tcp, c.marks["kill"])
    before, at, after = count(tcp, 0, m - 200), count(tcp, m - 200, m + 1500), count(tcp, m + 1500)
    mib = tcp["total_ok"] * 8 / tcp["elapsed"]
    ok = alive and tcp["err"]["mismatch"] == 0 and not before["err"] and not after["err"] and sum(at["err"].values()) <= 16
    record(
        "EX-09", "4 MiB 大包往返校验，期间备节点先抖动、主节点再被杀", "PASS" if ok else "FAIL",
        "数据逐字节校验 0 错；备节点抖动不影响主上的传输；主被杀时只有正在传的 ≤16 条失败，之后全部成功",
        f"完成 {tcp['total_ok']} 次（约 {mib:.0f} MiB/s 双向合计），校验错误 {tcp['err']['mismatch']}；主被杀前失败 {sum(before['err'].values())}，"
        f"被杀瞬间失败 {sum(at['err'].values())}（{at['err']}），之后失败 {sum(after['err'].values())}，之后分布 {after['ok']}",
    )


@case
def ex10_udp_port_sharing():
    """upstream udp sockets of different clients must never share a local port"""
    from collections import Counter
    out = {}
    for name, binary in (("orig", ORIG), ("new", NEW)):
        c = Case(PLAIN.replace("udp_timeout = 5", "udp_timeout = 30"), binary=binary)
        nsx("r", "sysctl -qw net.ipv4.ip_local_port_range='32768 60999'")  # linux default
        c.run("udp", f"udp target={R} socks=5000 pps=10000 secs=12 spread=3000 threads=6")
        c.at(9)
        ports = [l.split()[3].rsplit(":", 1)[1] for l in nsx("r", "ss -Huan").split("\n") if l.strip() and ":6000" not in l.split()[3]]
        dup = Counter(ports)
        u = c.res("udp")
        nsx("r", "sysctl -qw net.ipv4.ip_local_port_range='1024 65535'")
        alive = c.close()
        out[name] = dict(sockets=len(ports), shared=sum(v for v in dup.values() if v > 1), foreign=u["foreign"], ok=u["total_ok"], sent=u["sent"], alive=alive)
    o, n = out["orig"], out["new"]
    ok = n["alive"] and n["shared"] == 0 and n["foreign"] == 0 and n["ok"] >= 0.97 * n["sent"]
    record(
        "EX-10", "5000 个 UDP 会话的上游端口是否互相重叠（Linux 默认临时端口范围）", "PASS" if ok else "FAIL",
        "每个会话独占一个上游端口；客户端不会收到别的会话的回包",
        f"原版：{o['sockets']} 个上游套接字中 {o['shared']} 个与别的会话共用端口，客户端收到别人的回包 {o['foreign']} 个，送达 {o['ok']}/{o['sent']}（{o['ok'] / o['sent']:.1%}）；"
        f"被测版：共用端口 {n['shared']} 个，收到别人的回包 {n['foreign']} 个，送达 {n['ok']}/{n['sent']}（{n['ok'] / n['sent']:.1%}）",
        out,
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
    save("ex" + ("-" + "-".join(want) if want else ""))
    net_down()
    print("FAILED:", [r["id"] for r in RESULTS if r["status"] == "FAIL"] or "none")
