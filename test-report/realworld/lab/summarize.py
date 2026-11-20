#!/usr/bin/env python3
"""Turn saved suite results into the tables used in the report.

  summarize.py apps <apps.json> [<label-for-via>=<via> ...]
  summarize.py soak <soak.json>
  summarize.py smoke <smoke.json> ...
"""
import json
import statistics
import sys


def med(v):
    v = [x for x in v if x is not None]
    return round(statistics.median(v), 1) if v else None


def u(d):
    """(loss %, rtt p50, rtt p99, downstream loss %) of a udp flow result."""
    if not d or not d.get("rtt_ms"):
        return (100.0, None, None, None)
    return (d["loss_pct"], d["rtt_ms"]["p50"], d["rtt_ms"]["p99"], (d.get("push") or {}).get("loss_pct"))


def row(r):
    g, vo, vc = u(r.get("game")), u(r.get("voice")), u(r.get("vcall"))
    web = r.get("web") or {}
    sites = r.get("sites") or []
    pu = r.get("p2p_udp") or {}
    return {
        "game_loss": g[0], "game_p50": g[1], "game_p99": g[2], "game_dn_loss": g[3],
        "voice_loss": vo[0], "voice_p99": vo[2],
        "vcall_loss": vc[0], "vcall_p99": vc[2], "vcall_dn_loss": vc[3],
        "web_ok": web.get("ok"), "web_n": web.get("n"), "web_p50": (web.get("lat_ms") or {}).get("p50"),
        "web_p99": (web.get("lat_ms") or {}).get("p99"),
        "chat_ok": (r.get("chat") or {}).get("ok"), "chat_p99": ((r.get("chat") or {}).get("rtt_ms") or {}).get("p99"),
        "push_ok": (r.get("push") or {}).get("ok"),
        "p2p_tcp_ok": (r.get("p2p_tcp") or {}).get("conns", {}).get("ok"),
        "p2p_dl_ok": (r.get("p2p_tcp") or {}).get("dl_ok"),
        "p2p_udp_dead": pu.get("dead_dests") if "dests" in pu else None,
        "video_stalls": (r.get("video") or {}).get("stalls"), "video_failed": (r.get("video") or {}).get("failed"),
        "down": (r.get("down") or {}).get("mbps"), "up": (r.get("up") or {}).get("mbps"),
        "udp_down": (r.get("udp_down") or {}).get("mbps"), "udp_down_loss": (r.get("udp_down") or {}).get("loss_pct"),
        "sites_ok": sum(1 for s in sites if s.get("ok")), "sites_n": len(sites),
        "sites_ttfb": med([s.get("ttfb_ms") for s in sites if s.get("ok")]),
        "cdn": (r.get("cdn") or {}).get("mbps"),
        "damaged": sum((r.get(k) or {}).get("damaged", 0) or 0 for k in ("game", "voice", "vcall")),
        "foreign": sum((r.get(k) or {}).get("foreign", 0) or 0 for k in ("game", "voice", "vcall")),
        "bad_data": sum(1 for d in (r.get("p2p_tcp") or {}).get("dl_bad", []) if d.get("bad_off") is not None),
    }


def apps(path):
    res = json.load(open(path))["res"]
    if all(k.isdigit() for k in res):          # looped run: {pass: {via/proto: result}}
        passes = [res[k] for k in sorted(res, key=int)]
    else:
        passes = [res]
    rows = {}
    for i, p in enumerate(passes):
        for key, r in p.items():
            if r.get("timeout"):
                rows[(i, key)] = None
                continue
            rows[(i, key)] = row(r)
    return rows


def fmt(x, nd=1):
    return "—" if x is None else ("%.*f" % (nd, x) if isinstance(x, float) else str(x))


def print_apps(path):
    rows = apps(path)
    vias = list(dict.fromkeys(k[1].split("/")[0] for k in rows))
    protos = list(dict.fromkeys(k[1].split("/")[1] for k in rows))
    print("| 协议 | 线路 | 游戏 丢包% / p50 / p99 ms | 语音 丢包% / p99 | 视频通话 丢包% 上/下 | 网页 成功 / p50 ms | 长连接 p99 ms | P2P 连接 / 下载 / UDP 对端失联 | 视频卡顿 | 下行 / 上行 Mbit/s | 真实网站 | CDN Mbit/s |")
    print("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |")
    for i in sorted(set(k[0] for k in rows)):
        for p in protos:
            for v in vias:
                r = rows.get((i, "%s/%s" % (v, p)))
                if r is None:
                    print("| %s | %s | 超时 | | | | | | | | | |" % (p, v))
                    continue
                print("| %s | %s | %s / %s / %s | %s / %s | %s / %s | %s/%s / %s | %s | %s / %s / %s | %s | %s / %s | %s/%s | %s |" % (
                    p, v, fmt(r["game_loss"], 2), fmt(r["game_p50"]), fmt(r["game_p99"]),
                    fmt(r["voice_loss"], 2), fmt(r["voice_p99"]),
                    fmt(r["vcall_loss"], 2), fmt(r["vcall_dn_loss"], 2),
                    r["web_ok"], r["web_n"], fmt(r["web_p50"]), fmt(r["chat_p99"]),
                    r["p2p_tcp_ok"], r["p2p_dl_ok"], fmt(r["p2p_udp_dead"]),
                    fmt(r["video_stalls"]), fmt(r["down"]), fmt(r["up"]),
                    r["sites_ok"], r["sites_n"], fmt(r["cdn"])))
    print()
    # aggregate per via
    print("| 线路 | 协议数 | 游戏丢包% 中位 | 游戏 p50 中位 | 游戏 p99 中位 | 语音 p99 中位 | 网页失败合计 | 网页 p50 中位 | 视频卡顿合计 | 下行中位 | 上行中位 | 真实网站失败合计 | CDN 中位 | 损坏 / 串包 / 数据错 |")
    print("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |")
    for v in vias:
        rs = [r for (i, k), r in rows.items() if r and k.split("/")[0] == v]
        if not rs:
            continue
        print("| %s | %d | %s | %s | %s | %s | %d / %d | %s | %d | %s | %s | %d / %d | %s | %d / %d / %d |" % (
            v, len(rs), fmt(med([r["game_loss"] for r in rs]), 2), fmt(med([r["game_p50"] for r in rs])),
            fmt(med([r["game_p99"] for r in rs])), fmt(med([r["voice_p99"] for r in rs])),
            sum((r["web_n"] or 0) - (r["web_ok"] or 0) for r in rs), sum(r["web_n"] or 0 for r in rs),
            fmt(med([r["web_p50"] for r in rs])), sum(r["video_stalls"] or 0 for r in rs),
            fmt(med([r["down"] for r in rs])), fmt(med([r["up"] for r in rs])),
            sum(r["sites_n"] - r["sites_ok"] for r in rs), sum(r["sites_n"] for r in rs),
            fmt(med([r["cdn"] for r in rs])),
            sum(r["damaged"] for r in rs), sum(r["foreign"] for r in rs), sum(r["bad_data"] for r in rs)))


def print_smoke(paths):
    for path in paths:
        res = json.load(open(path))["res"]
        by = {}
        for key, r in res.items():
            via, proto = key.split("/")
            ok = (not r.get("timeout") and r["id"]["ok"] == r["id"]["n"] and r["dl"]["ok"] and r["ul"]["ok"]
                  and r["echo"]["ok"] and r["udp"].get("recv") == r["udp"].get("sent"))
            by.setdefault(via, []).append((proto, ok))
        for via, v in by.items():
            bad = [p for p, ok in v if not ok]
            print("%-12s %2d/%d  %s" % (via, len(v) - len(bad), len(v), ("fail: " + ", ".join(bad)) if bad else ""))


if __name__ == "__main__":
    if sys.argv[1] in ("apps", "soak"):
        print_apps(sys.argv[2])
    elif sys.argv[1] == "smoke":
        print_smoke(sys.argv[2:])
