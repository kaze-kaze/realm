#!/usr/bin/env python3
"""Test suites, run inside rbt-cli.  suite.py <name> '<json args>'  -> /rbt/run/res/<tag>.json + table"""
import asyncio
import json
import os
import sys
import time

import cli
import topo
import wl

RES = topo.RUN + "/res"
ME = cli.ME
ALL = topo.XRAY_PROTOS + topo.HY_PROTOS + ["wg", "tgt"]


def path(via, proto):
    """(tcp path, udp path) for a protocol through a via. 'tgt' = the target port relayed as-is (no proxy)."""
    if proto == "wg":
        p = "raw:10.99.0.1:%d%%%s" % (topo.lport("tgt"), cli.wgdev(via))
    elif proto == "tgt":
        p = "raw:%s:%d" % topo.via_addr(via, "tgt")
    else:
        p = "socks:%d" % topo.cport(via, proto)
    return p, p


def has_tcp(proto):
    return True


def ensure(via, protos, **kw):
    kinds = set()
    for p in protos:
        if p in topo.XRAY_PROTOS:
            kinds.add("xray")
        elif p in topo.HY_PROTOS or p == "wg":
            kinds.add(p)
    cli.up(via, sorted(kinds), **kw)
    time.sleep(2.5 if kinds else 0)


def save(tag, data):
    os.makedirs(RES, exist_ok=True)
    f = "%s/%s.json" % (RES, tag)
    json.dump(data, open(f, "w"), indent=1)
    return f


def short(d, keys):
    return " ".join("%s=%s" % (k, d.get(k)) for k in keys if d.get(k) not in (None, {}, [], 0, False) or k == "ok")


# ------------------------------------------------------------------ smoke
async def smoke_one(via, proto, dl=2 << 20, ul=1 << 20, udp_s=5):
    tp, up = path(via, proto)
    r = {}
    r["id"] = await wl.t_id(tp, n=20, conc=5, timeout=6)
    r["dl"] = await wl.t_dl(tp, dl)
    r["ul"] = await wl.t_ul(tp, ul)
    r["echo"] = await wl.t_echo(tp, count=10, interval=0.05)
    r["udp"] = await wl.u_flow(up, pps=20, size=200, dur=udp_s)
    return r


def smoke(vias, protos=None, **kw):
    protos = protos or ALL
    out = {}
    for via in vias:
        ensure(via, protos)
        for p in protos:
            t = time.time()
            try:
                r = asyncio.run(asyncio.wait_for(smoke_one(via, p, **kw), 90))
            except asyncio.TimeoutError:
                out["%s/%s" % (via, p)] = {"timeout": True}
                print("%-12s %-16s TIMEOUT (90s)" % (via, p), flush=True)
                continue
            out["%s/%s" % (via, p)] = r
            u = r["udp"]
            print("%-12s %-16s id %2d/20 lid=%s p50=%sms | dl %s %sMbps | ul %s %sMbps | echo %s p50=%sms | udp %s/%s rtt=%s lid=%s  [%.0fs]" % (
                via, p, r["id"]["ok"], ",".join(map(str, r["id"]["lids"])), (r["id"]["lat_ms"] or {}).get("p50"),
                "OK" if r["dl"]["ok"] else "FAIL:%s" % (r["dl"]["err"] or "bad@%s" % r["dl"]["bad_off"]), r["dl"]["mbps"],
                "OK" if r["ul"]["ok"] else "FAIL:%s" % r["ul"]["err"], r["ul"]["mbps"],
                "OK" if r["echo"]["ok"] else "FAIL:%s" % r["echo"]["err"], (r["echo"]["rtt_ms"] or {}).get("p50"),
                u.get("recv"), u.get("sent"), (u.get("rtt_ms") or {}).get("p50"), ",".join(map(str, u.get("lids", {}))),
                time.time() - t), flush=True)
    return out


# ------------------------------------------------------------------ application scenarios
REAL_SITES = ["https://www.google.com/generate_204", "https://www.cloudflare.com/cdn-cgi/trace",
              "https://www.wikipedia.org/", "https://github.com/", "https://www.youtube.com/",
              "https://www.apple.com/", "https://www.bing.com/", "https://store.steampowered.com/"]
REAL_DL = "https://speed.cloudflare.com/__down?bytes=30000000"


def curl_args(via, proto):
    if proto == "wg":
        return {"dev": cli.wgdev(via)}
    if proto == "tgt":
        return None
    return {"socks": topo.cport(via, proto)}


def iperf_args(via, proto):
    idx = topo.HOSTS[ME]["idx"]
    if proto == "wg":
        return {"host": "10.99.0.1", "port": topo.lport("iperf") + idx, "dev": cli.wgdev(via)}
    if proto == "tgt":
        ip, port = topo.via_addr(via, "iperf")
        return {"host": ip, "port": port}
    return {"port": topo.cport(via, proto, "fwd")}


def swarm_targets(via, proto, n=9):
    if proto == "tgt":
        return None
    ip = "10.99.0.1" if proto == "wg" else "127.0.0.1"
    return [[ip, topo.lport("tgt") + i] for i in range(n)]


async def apps_one(via, proto, secs=30, real=True):
    tp, up = path(via, proto)
    th = asyncio.to_thread
    r = {}
    # A: everything latency sensitive at once, like a busy household
    names = ["game", "voice", "vcall", "web", "chat", "push"]
    res = await asyncio.gather(
        wl.u_flow(up, pps=60, size=100, dur=secs, sub=[30, 250]),
        wl.u_flow(up, pps=50, size=172, dur=secs),
        wl.u_flow(up, pps=150, size=1100, dur=secs, sub=[150, 1100]),
        wl.t_id(tp, n=10 * secs, conc=30, timeout=10),
        wl.t_echo(tp, count=2 * secs, interval=0.5, size=200),
        wl.t_push(tp, interval=secs / 2.5, count=2, size=300))
    r.update(zip(names, res))
    # B: P2P pattern, many peers at once
    tg = swarm_targets(via, proto)
    jobs = [wl.t_id(tp, n=200, conc=50, timeout=10)] + [wl.t_dl(tp, 4 << 20) for _ in range(8)]
    if tg:
        jobs.append(wl.u_swarm(up, tg, pps=5, dur=10))
    res = await asyncio.gather(*jobs)
    dls = res[1:9]
    r["p2p_tcp"] = {"conns": res[0], "dl_ok": sum(1 for d in dls if d["ok"]), "dl_n": len(dls),
                    "dl_mbps_sum": round(sum(d["mbps"] for d in dls), 1), "dl_bad": [d for d in dls if not d["ok"]]}
    r["p2p_udp"] = res[9] if tg else None
    # C: video
    r["video"] = await wl.video(tp, segs=6, seg=3 << 20, every=2.0)
    # D: speed test
    ia = iperf_args(via, proto)
    r["down"] = await th(wl.iperf, reverse=True, secs=6, **ia)
    r["up"] = await th(wl.iperf, secs=6, **ia)
    r["udp_down"] = await th(wl.iperf, reverse=True, udp=True, bitrate="20M", length=1200, secs=5, **ia)
    # E: the real internet through the landing
    ca = curl_args(via, proto)
    if real and ca:
        r["sites"] = [dict(await th(wl.curl, u, timeout=20, **ca), url=u) for u in REAL_SITES]
        r["cdn"] = await th(wl.curl, REAL_DL, timeout=40, **ca)
    return r


def _u(d):
    if not d or not d.get("rtt_ms"):
        return "DEAD(%s)" % (d or {}).get("err")
    s = "%.2f%%/%s/%s" % (d["loss_pct"], d["rtt_ms"]["p50"], d["rtt_ms"]["p99"])
    if d.get("push"):
        s += " dn%.2f%%" % d["push"]["loss_pct"]
    return s


def apps_line(via, proto, r, dt):
    web, chat, pt, pu, v = r["web"], r["chat"], r["p2p_tcp"], r["p2p_udp"], r["video"]
    sites = r.get("sites")
    line = "%-7s %-15s game %s | voice %s | vcall %s | web %d/%d p50=%s p99=%s | chat %s p99=%s | push %s | p2p tcp %d/%d dl %d/%d udp %s | video stalls=%d fail=%d | down %s up %s udp %s/%s%% | " % (
        via, proto, _u(r["game"]), _u(r["voice"]), _u(r["vcall"]),
        web["ok"], web["n"], (web["lat_ms"] or {}).get("p50"), (web["lat_ms"] or {}).get("p99"),
        "ok" if chat["ok"] else "FAIL:%s" % chat["err"], (chat["rtt_ms"] or {}).get("p99"),
        "ok" if r["push"]["ok"] else "FAIL:%s" % r["push"]["err"],
        pt["conns"]["ok"], pt["conns"]["n"], pt["dl_ok"], pt["dl_n"],
        ("dead=%d/%d" % (pu["dead_dests"], pu["dests"]) if pu and "dests" in pu else pu and pu.get("err")),
        v["stalls"], v["failed"],
        r["down"].get("mbps", r["down"].get("err")), r["up"].get("mbps", r["up"].get("err")),
        r["udp_down"].get("mbps", r["udp_down"].get("err")), r["udp_down"].get("loss_pct"))
    if sites is not None:
        ok = [x for x in sites if x.get("ok")]
        line += "sites %d/%d ttfb=%s cdn %s" % (len(ok), len(sites),
                                                round(sum(x["ttfb_ms"] for x in ok) / len(ok)) if ok else None,
                                                r["cdn"].get("mbps", r["cdn"].get("err")))
    return line + "  [%.0fs]" % dt


def apps(vias, protos=None, secs=30, real=True, loops=1, **up_kw):
    """Per protocol, run each via back to back so paths are compared under the same conditions."""
    protos = protos or ALL
    out = {}
    for via in vias:
        ensure(via, protos, **up_kw)
    time.sleep(3)
    if loops > 1:
        for i in range(loops):
            out[i] = _apps_pass(vias, protos, secs, real)
            print("pass %d done %.0f" % (i, time.time()), flush=True)
        return out
    return _apps_pass(vias, protos, secs, real)


def _apps_pass(vias, protos, secs, real):
    out = {}
    for p in protos:
        for via in vias:
            t = time.time()
            try:
                r = asyncio.run(asyncio.wait_for(apps_one(via, p, secs, real), secs * 2 + 260))
            except asyncio.TimeoutError:
                out["%s/%s" % (via, p)] = {"timeout": True}
                print("%-7s %-15s TIMEOUT" % (via, p), flush=True)
                continue
            out["%s/%s" % (via, p)] = r
            print(apps_line(via, p, r, time.time() - t), flush=True)
    return out


# ------------------------------------------------------------------ failover drills
def drill(via=None, vias=None, protos=None, dur=170, udp_pps=20, tcp_ivl=0.25, fresh=False, env=None,
          start_at=None, pairs=None, **up_kw):
    """Watch every protocol through each via at the same time while a fault is injected on a landing.
    Add a via the fault does not touch (e.g. p2-<relay>) as a control. Fault times come from the
    landing's fault.log; pass them to drill_report afterwards."""
    protos = protos or ALL
    vias = vias or [via]
    # pairs: explicit [via, protocol] list, for vias that only carry some protocols
    keys = [tuple(x) for x in pairs] if pairs else [(v, p) for v in vias for p in protos]
    os.environ.update(env or {})
    for v in dict.fromkeys(k[0] for k in keys):
        if fresh:
            cli.down(v)
        ensure(v, [p for vv, p in keys if vv == v], **up_kw)
    time.sleep(3)
    if start_at:
        time.sleep(max(0, start_at - time.time()))

    async def run():
        return await asyncio.gather(*[
            wl.monitor(path(v, p)[0], dur=dur, tcp_ivl=tcp_ivl, udp_pps=udp_pps, upath=path(v, p)[1])
            for v, p in keys])

    print("drill start %.2f, %d paths, dur %s" % (time.time(), len(keys), dur), flush=True)
    res = asyncio.run(run())
    return {"%s/%s" % k: r for k, r in zip(keys, res)}


def drill_report(tag, on, off=None, primary=None, slack=3.5):
    """Summarise a saved drill against the fault window [on, off] (epoch seconds). Requests are
    stamped with their start time, so one that started up to `slack` before the fault can already
    be answered by the other landing."""
    d = json.load(open("%s/%s.json" % (RES, tag)))["res"]
    rows = {}
    for p, m in d.items():
        sh, lg, u = m["short"], m["long"], m.get("udp") or {}
        prim = primary
        if prim is None:
            prim = sh["lid_changes"][0][2] if sh["lid_changes"] else None
        sw = next((c for c in sh["lid_changes"] if c[0] > on - slack and c[2] != prim), None)
        back = next((c for c in sh["lid_changes"] if off and c[0] > off - slack and c[2] == prim), None)
        fw = [w for w in sh["fail_windows"] if w["to"] is None or w["to"] > on - slack]
        usw = next((c for c in u.get("lid_changes", []) if c[0] > on and c[2] != prim), None)
        uback = next((c for c in u.get("lid_changes", []) if off and c[0] > off and c[2] == prim), None)
        uout = [o for o in u.get("outages", []) if o["from"] + o["s"] > on]
        br = [b for b in lg["breaks"] if (b["back"] or 1e18) > on]
        r1 = lambda x: None if x is None else round(x, 1)
        row = {
            "tcp_switch_s": r1(sw[0] - on) if sw else None, "tcp_fail": sum(w["fails"] for w in fw),
            "tcp_fail_total": sh["fail"], "tcp_n": sh["n"],
            "tcp_dead_s": round(sum(w["s"] for w in fw), 1),
            "tcp_dead_until_s": r1(max([(w["to"] or 1e18) - on for w in fw])) if fw else None,
            "tcp_back_s": r1(back[0] - off) if back else None,
            "long_breaks": [{"at": round(b["broke"] - on, 1), "down_s": b["down_s"], "to": b["lid_after"]} for b in br],
            "udp_switch_s": r1(usw[0] - on) if usw else None, "udp_back_s": r1(uback[0] - off) if uback else None,
            "udp_out": [{"at": round(o["from"] - on, 1), "s": o["s"]} for o in uout],
            "udp_loss": u.get("loss_pct"), "udp_reopens": u.get("reopens"), "udp_foreign": u.get("foreign"),
            "udp_dup": u.get("dup"), "udp_damaged": u.get("damaged"), "errs": sh["errs"],
            "lat_p50": (sh["lat_ms"] or {}).get("p50"),
        }
        rows[p] = row
        print("%-23s tcp: other landing at %5s s, %3d/%d failed, last failure %5s s, back %5s s | long: %s | udp: moved %5s s, gaps %s, back %5s s, loss %s%% reopen %s" % (
            p, row["tcp_switch_s"], row["tcp_fail"], row["tcp_n"], row["tcp_dead_until_s"], row["tcp_back_s"],
            ",".join("%+.0fs/%.0fs->%s" % (b["at"], b["down_s"], b["to"]) for b in row["long_breaks"]) or "-",
            row["udp_switch_s"], ",".join("%+.0fs/%.0fs" % (o["at"], o["s"]) for o in row["udp_out"]) or "-",
            row["udp_back_s"], row["udp_loss"], row["udp_reopens"]), flush=True)
    save(tag + ".report", {"on": on, "off": off, "rows": rows})
    return rows


# ------------------------------------------------------------------ datagram size sweep
def mtu(vias, sizes=None, proto="tgt", pps=20, dur=2.5):
    """UDP echo with growing datagram sizes through each via: shows where large datagrams die."""
    sizes = sizes or [1200, 1400, 1422, 1423, 1452, 1472, 1473, 2000, 4000]
    out = {}
    ensure_list = [v for v in vias]
    if proto != "tgt":
        for v in ensure_list:
            ensure(v, [proto])
    for via in vias:
        row = {}
        for sz in sizes:
            r = asyncio.run(wl.u_flow(path(via, proto)[1], pps=pps, size=sz, dur=dur, reopen=0))
            row[sz] = [r.get("recv", 0), r.get("sent", 0), r.get("damaged", 0)]
        out[via] = row
        print("%-11s %s" % (via, "  ".join("%d:%s" % (sz, "ok" if v[0] == v[1] else "%d/%d%s" % (
            v[0], v[1], " DAMAGED=%d" % v[2] if v[2] else "")) for sz, v in row.items())), flush=True)
    return out


# ------------------------------------------------------------------ load balancing
def lb(vias, protos=None, rounds=6, gap=20, n=40, flows=8, timeout=6):
    """Where sessions land, round after round, while a landing may be failed from outside:
    n short tcp connections and `flows` separate udp associations per via and protocol."""
    protos = protos or ["vless-reality", "ss-2022", "tgt"]
    for v in vias:
        ensure(v, protos)
    time.sleep(3)
    keys = [(v, p) for v in vias for p in protos]
    start = time.time()
    out = []

    async def one(v, p):
        tp, up = path(v, p)
        res = await asyncio.gather(wl.t_id(tp, n=n, conc=8, timeout=timeout),
                                   *[wl.u_flow(up, pps=10, size=100, dur=3, reopen=0) for _ in range(flows)])
        fl = res[1:]
        per, mixed, dead = {}, 0, 0
        for f in fl:
            lids = f.get("lids") or {}
            if not lids:
                dead += 1
            elif len(lids) > 1:
                mixed += 1
            else:
                k = next(iter(lids))
                per[k] = per.get(k, 0) + 1
        return {"tcp_lids": res[0]["lids"], "tcp_fail": res[0]["fail"], "tcp_errs": res[0]["errs"],
                "tcp_p50": (res[0]["lat_ms"] or {}).get("p50"), "udp_flows": per, "udp_mixed": mixed, "udp_dead": dead}

    async def rnd():
        return await asyncio.gather(*[one(v, p) for v, p in keys])

    print("lb start %.2f" % start, flush=True)
    for r in range(rounds):
        t0 = time.time()
        res = asyncio.run(rnd())
        row = {"t": round(t0, 2), "res": {"%s/%s" % k: x for k, x in zip(keys, res)}}
        out.append(row)
        for (v, p), x in zip(keys, res):
            print("round %d +%3.0fs %-7s %-14s tcp %s fail=%d p50=%s | udp flows %s mixed=%d dead=%d" % (
                r, t0 - start, v, p, json.dumps(x["tcp_lids"], sort_keys=True), x["tcp_fail"], x["tcp_p50"],
                json.dumps(x["udp_flows"], sort_keys=True), x["udp_mixed"], x["udp_dead"]), flush=True)
        d = t0 + gap - time.time()
        if d > 0 and r < rounds - 1:
            time.sleep(d)
    return out


# ------------------------------------------------------------------ wireguard inside a proxy
def wgin(vias, protos=None, mtus=(1420, 1280)):
    """A wireguard tunnel carried inside a proxy's udp channel, as when a user runs a wireguard
    vpn through the proxy. Full-size tunnel packets exceed 1500 bytes once the proxy wraps them."""
    protos = protos or topo.WGIN_PROTOS
    out = {}
    for via in vias:
        for proto in protos:
            for mtu in mtus:
                dev = cli.wgin_up(via, proto, mtu)
                time.sleep(2.5)
                p = "raw:10.99.0.1:%d%%%s" % (topo.lport("tgt"), dev)

                async def run():
                    i = await wl.t_id(p, n=6, conc=1, timeout=6)
                    e = await wl.t_echo(p, count=10, interval=0.1, size=64, timeout=6)
                    d = await wl.t_dl(p, 3 << 20, stall=10)
                    u = await wl.t_ul(p, 3 << 20, stall=10)
                    return i, e, d, u

                i, e, d, u = asyncio.run(run())
                out["%s/%s/%d" % (via, proto, mtu)] = {"id": i, "echo": e, "dl": d, "ul": u}
                print("%-11s wg in %-8s mtu %d: handshake+small %d/6, echo %s | download %s %.1f Mbps | upload %s %.1f Mbps" % (
                    via, proto, mtu, i["ok"], "ok" if e["ok"] else "FAIL",
                    "OK" if d["ok"] else "FAIL(%s, %d of %d bytes)" % (d["err"], d["bytes"], d["want"]), d["mbps"],
                    "OK" if u["ok"] else "FAIL(%s)" % u["err"], u["mbps"]), flush=True)
                subprocess_del(dev)
    return out


def subprocess_del(dev):
    import subprocess
    subprocess.run(["ip", "link", "del", dev], capture_output=True)


SUITES = {"smoke": smoke, "apps": apps, "drill": drill, "drill_report": drill_report, "mtu": mtu, "lb": lb,
          "wgin": wgin}

if __name__ == "__main__":
    name = sys.argv[1]
    kw = json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}
    tag = kw.pop("tag", "%s-%s-%d" % (name, ME, time.time()))
    if name == "drill_report":
        drill_report(tag, **kw)
        sys.exit(0)
    res = SUITES[name](**kw)
    print("saved", save(tag, {"suite": name, "me": ME, "args": kw, "t": time.time(), "res": res}))
