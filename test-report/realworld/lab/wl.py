#!/usr/bin/env python3
"""Client-side workloads. Library + CLI:  wl.py <func> '<json kwargs>'

A path says how to reach the landing target (tgt.py):
  socks:<port>             socks5 at 127.0.0.1:<port>, target is the landing-local tgt
  fwd:<port>               local port forwarded to the landing tgt (TCP and UDP)
  raw:<ip>:<port>[%dev]    plain address: relay raw endpoint, direct landing, or wg tunnel (bind to dev)
"""
import asyncio
import json
import os
import random
import socket
import struct
import subprocess
import sys
import time

import det
import topo

HDR = struct.Struct("!2sBBIId")
TGT = (topo.TGT_LOCAL, topo.lport("tgt"))
SO_BINDTODEVICE = 25


def stats(v, scale=1000.0, nd=1):
    if not v:
        return None
    s = sorted(v)
    n = len(s)

    def p(q):
        return round(s[min(n - 1, int(q * n))] * scale, nd)

    return {"n": n, "min": round(s[0] * scale, nd), "p50": p(0.5), "p90": p(0.9), "p99": p(0.99),
            "max": round(s[-1] * scale, nd), "mean": round(sum(s) / n * scale, nd)}


class Path:
    def __init__(self, spec):
        self.spec = spec
        kind, rest = spec.split(":", 1)
        self.kind, self.dev = kind, None
        if kind in ("socks", "fwd"):
            self.host, self.port = "127.0.0.1", int(rest)
        else:
            if "%" in rest:
                rest, self.dev = rest.split("%", 1)
            h, p = rest.rsplit(":", 1)
            self.host, self.port = h, int(p)


def _path(p):
    return p if isinstance(p, Path) else Path(p)


def _sock(kind, dev):
    s = socket.socket(socket.AF_INET, kind)
    s.setblocking(False)
    if dev:
        s.setsockopt(socket.SOL_SOCKET, SO_BINDTODEVICE, dev.encode())
    return s


async def _socks_hello(r, w, cmd, host, port, timeout):
    w.write(b"\x05\x01\x00")
    rep = await asyncio.wait_for(r.readexactly(2), timeout)
    if rep != b"\x05\x00":
        raise ConnectionError("socks auth %r" % rep)
    w.write(b"\x05" + bytes([cmd]) + b"\x00\x01" + socket.inet_aton(host) + struct.pack("!H", port))
    rep = await asyncio.wait_for(r.readexactly(4), timeout)
    if rep[1] != 0:
        raise ConnectionError("socks rep %d" % rep[1])
    if rep[3] == 1:
        a = await asyncio.wait_for(r.readexactly(6), timeout)
        return socket.inet_ntoa(a[:4]), struct.unpack("!H", a[4:])[0]
    if rep[3] == 4:
        a = await asyncio.wait_for(r.readexactly(18), timeout)
        return "::", struct.unpack("!H", a[16:])[0]
    n = (await asyncio.wait_for(r.readexactly(1), timeout))[0]
    a = await asyncio.wait_for(r.readexactly(n + 2), timeout)
    return a[:n].decode(), struct.unpack("!H", a[n:])[0]


async def tcp_open(path, timeout=10, target=None):
    path = _path(path)
    loop = asyncio.get_running_loop()
    s = _sock(socket.SOCK_STREAM, path.dev)
    try:
        await asyncio.wait_for(loop.sock_connect(s, (path.host, path.port)), timeout)
    except BaseException:
        s.close()
        raise
    r, w = await asyncio.open_connection(sock=s, limit=1 << 20)
    if path.kind == "socks":
        try:
            h, p = target or TGT
            await _socks_hello(r, w, 1, h, p, timeout)
        except BaseException:
            w.close()
            raise
    return r, w


def _err(e):
    n = type(e).__name__
    if isinstance(e, OSError) and e.errno:
        n += ":%d" % e.errno
    if n == "ConnectionError":
        n += ":" + str(e)
    return n


# ----------------------------------------------------------------- TCP
async def _id_once(path, timeout):
    t0 = time.time()
    w = None
    try:
        r, w = await tcp_open(path, timeout)
        w.write(b"ID\n")
        line = await asyncio.wait_for(r.readline(), timeout)
        if not line.endswith(b"\n"):
            raise ConnectionError("eof")
        return True, int(line), time.time() - t0, None
    except Exception as e:
        return False, 0, time.time() - t0, _err(e)
    finally:
        if w:
            w.close()


async def t_id(path, n=100, conc=10, timeout=8):
    """n short connections (connect, one request, one reply), conc at a time."""
    sem = asyncio.Semaphore(conc)
    res = []

    async def one():
        async with sem:
            res.append(await _id_once(path, timeout))

    t0 = time.time()
    await asyncio.gather(*[one() for _ in range(n)])
    lids, errs = {}, {}
    for ok, lid, lat, e in res:
        if ok:
            lids[lid] = lids.get(lid, 0) + 1
        else:
            errs[e] = errs.get(e, 0) + 1
    return {"n": n, "ok": sum(1 for x in res if x[0]), "fail": sum(1 for x in res if not x[0]),
            "lids": lids, "errs": errs, "lat_ms": stats([x[2] for x in res if x[0]]),
            "secs": round(time.time() - t0, 2)}


async def t_dl(path, nbytes=20 << 20, seed=None, timeout=15, stall=30):
    seed = seed or random.getrandbits(48)
    t0 = time.time()
    ck = det.Checker(seed)
    ttfb, err, w = None, None, None
    try:
        r, w = await tcp_open(path, timeout)
        w.write(b"SRC %d %d\n" % (nbytes, seed))
        while ck.off < nbytes:
            d = await asyncio.wait_for(r.read(1 << 18), stall)
            if not d:
                err = "eof"
                break
            if ttfb is None:
                ttfb = time.time() - t0
            ck.feed(d)
    except Exception as e:
        err = _err(e)
    finally:
        if w:
            w.close()
    dt = time.time() - t0
    return {"ok": err is None and ck.off == nbytes and ck.bad is None, "bytes": ck.off, "want": nbytes,
            "bad_off": ck.bad, "err": err, "secs": round(dt, 3), "ttfb_ms": ttfb and round(ttfb * 1000, 1),
            "mbps": round(ck.off * 8 / dt / 1e6, 2)}


async def t_ul(path, nbytes=10 << 20, seed=None, timeout=15, stall=60):
    seed = seed or random.getrandbits(48)
    t0 = time.time()
    err, w, rep, sent = None, None, None, 0
    try:
        r, w = await tcp_open(path, timeout)
        w.write(b"SINK %d %d\n" % (nbytes, seed))
        for chunk in det.stream(seed, nbytes):
            w.write(chunk)
            await asyncio.wait_for(w.drain(), stall)
            sent += len(chunk)
        line = await asyncio.wait_for(r.readline(), stall)
        rep = line.decode().split()
        if not rep:
            err = "eof"
    except Exception as e:
        err = _err(e)
    finally:
        if w:
            w.close()
    dt = time.time() - t0
    ok = err is None and rep[0] == "OK" and int(rep[1]) == nbytes
    return {"ok": ok, "sent": sent, "reply": rep, "err": err, "secs": round(dt, 3),
            "lid": int(rep[3]) if rep and len(rep) > 3 else None, "mbps": round(sent * 8 / dt / 1e6, 2)}


async def t_echo(path, count=50, interval=0.2, size=64, timeout=10):
    """Request/response over one long-lived connection (interactive traffic)."""
    rtts, err, lid, w, done = [], None, None, None, 0
    t0 = time.time()
    try:
        r, w = await tcp_open(path, timeout)
        w.write(b"ECHO\n")
        lid = int(await asyncio.wait_for(r.readline(), timeout))
        for i in range(count):
            msg = struct.pack("!I", i) + os.urandom(max(0, size - 4))
            t = time.time()
            w.write(msg)
            got = await asyncio.wait_for(r.readexactly(len(msg)), timeout)
            if got != msg:
                err = "mismatch@%d" % i
                break
            rtts.append(time.time() - t)
            done += 1
            if interval:
                await asyncio.sleep(interval)
    except Exception as e:
        err = _err(e)
    finally:
        if w:
            w.close()
    return {"ok": err is None and done == count, "done": done, "lid": lid, "err": err,
            "rtt_ms": stats(rtts), "secs": round(time.time() - t0, 2)}


async def t_push(path, interval=30, count=4, size=200, timeout=10):
    """Server-initiated messages over an otherwise idle connection (push notifications)."""
    got, err, lid, w, delays = 0, None, None, None, []
    try:
        r, w = await tcp_open(path, timeout)
        w.write(b"PUSH %s %d %d\n" % (str(interval).encode(), count, size))
        lid = int(await asyncio.wait_for(r.readline(), timeout))
        for i in range(count):
            d = await asyncio.wait_for(r.readexactly(size), interval + timeout)
            _, ts = struct.unpack_from("!Id", d)
            delays.append(time.time() - ts)
            got += 1
    except Exception as e:
        err = _err(e)
    finally:
        if w:
            w.close()
    return {"ok": err is None and got == count, "got": got, "want": count, "lid": lid, "err": err,
            "delay_ms": stats(delays)}


# ----------------------------------------------------------------- UDP
class UdpEp(asyncio.DatagramProtocol):
    """UDP toward the landing tgt through a path; on_data(payload, t) gets every reply."""

    def __init__(self, on_data):
        self.on_data, self.tr, self.ctl, self.hdr, self.dst = on_data, None, None, b"", None

    def connection_made(self, tr):
        self.tr = tr

    def datagram_received(self, data, addr):
        now = time.time()
        if self.hdr:
            if len(data) < 10 or data[2] != 0:
                return
            at = data[3]
            off = 10 if at == 1 else 22 if at == 4 else 7 + data[4]
            data = data[off:]
        self.on_data(data, now)

    def error_received(self, exc):
        pass

    def send(self, payload):
        tr = self.tr
        if tr is None or tr.is_closing():      # closed, e.g. a failed re-association
            return
        try:
            tr.sendto(self.hdr + payload, self.dst)
        except Exception:
            pass

    def close(self):
        if self.tr:
            self.tr.close()
        if self.ctl:
            self.ctl.close()

    @classmethod
    async def open(cls, path, on_data, target=None, timeout=10):
        path = _path(path)
        loop = asyncio.get_running_loop()
        ep = cls(on_data)
        if path.kind == "socks":
            r, w = await asyncio.wait_for(asyncio.open_connection(path.host, path.port), timeout)
            ep.ctl = w
            try:
                h, p = await _socks_hello(r, w, 3, "0.0.0.0", 0, timeout)
            except BaseException:
                w.close()
                raise
            if h in ("0.0.0.0", "::", ""):
                h = path.host
            ep.dst = (h, p)
            th, tp = target or TGT
            ep.hdr = b"\x00\x00\x00\x01" + socket.inet_aton(th) + struct.pack("!H", tp)
        else:
            ep.dst = (path.host, path.port)
        s = _sock(socket.SOCK_DGRAM, path.dev)
        s.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 4 << 20)
        s.bind(("0.0.0.0", 0))
        await loop.create_datagram_endpoint(lambda: ep, sock=s)
        return ep


async def u_flow(path, pps=50, size=160, dur=30, sub=None, target=None, reopen=3.0, gap=1.0, detail=False):
    """Echo flow at pps/size for dur seconds. sub=[pps,size] also asks the landing to push a stream down.
    reopen: when replies stall this long on a socks path, re-associate like an app would."""
    path = _path(path)
    flow = random.getrandbits(32)
    sent_at = {}
    rtts, rx_t, lids, lid_changes, late, dup = [], [], {}, [], 0, 0
    seen = set()
    hi = [-1]
    last_lid = [None]
    p_rx, p_seen, p_t, p_lids, foreign = [0], set(), [], {}, [0]
    trunc = [0]
    want_len = max(size, HDR.size)
    pad = os.urandom(max(0, size - HDR.size))
    t_start = time.time()

    def on_data(data, now):
        if len(data) < HDR.size:
            foreign[0] += 1
            return
        magic, lid, kind, fl, seq, ts = HDR.unpack_from(data)
        if fl != flow or magic not in (b"RS", b"RP"):
            foreign[0] += 1
            return
        if magic == b"RP":
            if seq not in p_seen:
                p_seen.add(seq)
                p_rx[0] += 1
                p_t.append(now)
                p_lids[lid] = p_lids.get(lid, 0) + 1
            return
        if kind != 0:
            return
        nonlocal late, dup
        if len(data) != want_len or data[HDR.size:] != pad:
            trunc[0] += 1      # cut short or altered on the way: never count it as delivered
            return
        if seq in seen:
            dup += 1
            return
        seen.add(seq)
        if seq < hi[0]:
            late += 1
        hi[0] = max(hi[0], seq)
        st = sent_at.pop(seq, None)
        if st is not None:
            rtts.append(now - st)
        rx_t.append(now)
        lids[lid] = lids.get(lid, 0) + 1
        if lid != last_lid[0]:
            lid_changes.append([round(now - t_start, 2), last_lid[0], lid])
            last_lid[0] = lid

    reopens, err = 0, None
    try:
        ep = await UdpEp.open(path, on_data, target)
    except Exception as e:
        return {"ok": False, "err": _err(e), "sent": 0, "recv": 0}
    loop = asyncio.get_running_loop()
    if sub:
        ep.send(HDR.pack(b"RQ", 0, 1, flow, 0, time.time()) + struct.pack("!HHf", sub[0], sub[1], float(dur)))
    n = int(pps * dur)
    t0 = loop.time()
    last_open = time.time()
    for i in range(n):
        due = t0 + i / pps
        d = due - loop.time()
        if d > 0:
            await asyncio.sleep(d)
        now = time.time()
        if reopen and path.kind == "socks" and i > pps * reopen:
            last_rx = rx_t[-1] if rx_t else t_start
            if now - max(last_rx, last_open) > reopen:
                ep.close()
                try:
                    ep = await UdpEp.open(path, on_data, target, timeout=reopen)
                    reopens += 1
                    if sub:
                        left = dur - (now - t_start)
                        ep.send(HDR.pack(b"RQ", 0, 1, flow, 0, now) + struct.pack("!HHf", sub[0], sub[1], float(left)))
                except Exception as e:
                    err = _err(e)
                last_open = time.time()
        sent_at[i] = now
        ep.send(HDR.pack(b"RQ", 0, 0, flow, i, now) + pad)
    await asyncio.sleep(1.5)
    ep.close()
    recv = len(seen)
    outages, prev = [], t_start
    for t in rx_t + [t_start + dur]:
        if t - prev > gap:
            outages.append([round(prev - t_start, 2), round(t - prev, 2)])
        prev = t
    jit = [abs(rtts[i] - rtts[i - 1]) for i in range(1, len(rtts))]
    out = {"ok": recv > 0, "sent": n, "recv": recv, "loss_pct": round(100.0 * (n - recv) / max(1, n), 3),
           "rtt_ms": stats(rtts), "jitter_ms": round(sum(jit) / len(jit) * 1000, 2) if jit else None,
           "late": late, "dup": dup, "foreign": foreign[0], "damaged": trunc[0], "lids": lids,
           "lid_changes": lid_changes,
           "outages": outages, "reopens": reopens, "err": err, "t0": round(t_start, 3)}
    if sub:
        want = int(sub[0] * dur)
        gaps = [p_t[i] - p_t[i - 1] for i in range(1, len(p_t))]
        out["push"] = {"want": want, "recv": p_rx[0], "loss_pct": round(100.0 * (want - p_rx[0]) / max(1, want), 3),
                       "gap_ms": stats(gaps), "lids": p_lids}
    if detail:
        out["rx_rel"] = [round(t - t_start, 3) for t in rx_t]
    return out


async def u_swarm(path, targets, pps=5, size=120, dur=20):
    """One client socket talking to many destinations at once (P2P / DHT pattern, needs full cone).
    targets: [[ip, port], ...] as seen from the landing (socks) or as reachable addresses (raw)."""
    path = _path(path)
    flow = random.getrandbits(32)
    got = {}

    def on_data(data, now):
        if len(data) < HDR.size:
            return
        magic, lid, kind, fl, seq, ts = HDR.unpack_from(data)
        if fl == flow and magic == b"RS":
            k = seq >> 20
            got[k] = got.get(k, 0) + 1

    try:
        ep = await UdpEp.open(path, on_data)
    except Exception as e:
        return {"ok": False, "err": _err(e)}
    socks = path.kind == "socks"
    dsts = []
    for t in targets:
        if socks:
            dsts.append((b"\x00\x00\x00\x01" + socket.inet_aton(t[0]) + struct.pack("!H", t[1]), ep.dst))
        else:
            dsts.append((b"", (t[0], t[1])))
    loop = asyncio.get_running_loop()
    pad = os.urandom(max(0, size - HDR.size))
    n = int(pps * dur)
    t0 = loop.time()
    for i in range(n):
        d = t0 + i / pps - loop.time()
        if d > 0:
            await asyncio.sleep(d)
        for k, (hdr, dst) in enumerate(dsts):
            try:
                ep.tr.sendto(hdr + HDR.pack(b"RQ", 0, 0, flow, (k << 20) | i, time.time()) + pad, dst)
            except OSError:
                pass
    await asyncio.sleep(1.5)
    ep.close()
    per = [got.get(k, 0) for k in range(len(dsts))]
    return {"ok": all(x > 0 for x in per), "dests": len(dsts), "sent_each": n, "recv_each": per,
            "dead_dests": sum(1 for x in per if x == 0),
            "loss_pct": round(100.0 * (n * len(dsts) - sum(per)) / max(1, n * len(dsts)), 3)}


async def video(path, segs=10, seg=3 << 20, every=2.0, startup=4.0):
    """Segmented streaming: one segment every `every` seconds. A segment that is not complete by the
    time the player needs it (startup buffer + its slot) counts as a stall."""
    t0 = time.time()
    out, late, bad = [], 0, 0
    for i in range(segs):
        d = t0 + i * every - time.time()
        if d > 0:
            await asyncio.sleep(d)
        r = await t_dl(path, seg, stall=15)
        done = time.time() - t0
        if not r["ok"]:
            bad += 1
        if done > startup + (i + 1) * every:
            late += 1
        out.append(r["secs"])
    return {"ok": bad == 0, "segs": segs, "failed": bad, "stalls": late, "seg_s": stats(out, 1.0, 2),
            "secs": round(time.time() - t0, 1), "mbps_needed": round(seg * 8 / every / 1e6, 1)}


# ----------------------------------------------------------------- failover observation
async def monitor(path, dur=60, tcp_ivl=0.25, udp_pps=20, udp=True, timeout=3, upath=None):
    """Watch one path while faults are injected elsewhere: short TCP requests, one long-lived TCP
    connection that reconnects when it breaks, and a UDP echo flow. Times are epoch seconds."""
    t_start = time.time()
    t_end = t_start + dur
    shorts, breaks = [], []

    async def short_loop():
        pend = set()
        while time.time() < t_end:
            async def one():
                t = time.time()
                ok, lid, lat, e = await _id_once(path, timeout)
                shorts.append((t, ok, lid, lat, e))
            task = asyncio.ensure_future(one())
            pend.add(task)
            task.add_done_callback(pend.discard)
            await asyncio.sleep(tcp_ivl)
        if pend:
            await asyncio.wait(pend)

    async def long_loop():
        lid, w, down_at, first = None, None, None, True
        while time.time() < t_end:
            try:
                r, w = await tcp_open(path, timeout)
                w.write(b"ECHO\n")
                new = int(await asyncio.wait_for(r.readline(), timeout))
                if down_at is not None or first:
                    if not first:
                        breaks.append({"broke": round(down_at, 2), "back": round(time.time(), 2),
                                       "down_s": round(time.time() - down_at, 2), "lid_before": lid, "lid_after": new})
                    first, down_at = False, None
                lid = new
                i = 0
                while time.time() < t_end:
                    msg = struct.pack("!I", i) + b"x" * 60
                    w.write(msg)
                    got = await asyncio.wait_for(r.readexactly(len(msg)), timeout)
                    if got != msg:
                        raise ConnectionError("mismatch")
                    i += 1
                    await asyncio.sleep(0.5)
            except Exception:
                if down_at is None:
                    down_at = time.time()
                first = False
                await asyncio.sleep(0.3)
            finally:
                if w:
                    try:
                        w.close()
                    except Exception:
                        pass
        if down_at is not None:
            breaks.append({"broke": round(down_at, 2), "back": None, "down_s": round(time.time() - down_at, 2),
                           "lid_before": lid, "lid_after": None})

    tasks = [short_loop(), long_loop()]
    if udp:
        tasks.append(u_flow(upath or path, pps=udp_pps, size=120, dur=dur, gap=1.0))
    res = await asyncio.gather(*tasks)
    shorts.sort()
    fails = [s for s in shorts if not s[1]]
    windows, cur = [], None
    for t, ok, lid, lat, e in shorts:
        if not ok:
            if cur is None:
                cur = [t, t, 1]
            else:
                cur[1], cur[2] = t, cur[2] + 1
        elif cur is not None:
            windows.append({"from": round(cur[0], 2), "to": round(t, 2), "s": round(t - cur[0], 2), "fails": cur[2]})
            cur = None
    if cur is not None:
        windows.append({"from": round(cur[0], 2), "to": None, "s": round(t_end - cur[0], 2), "fails": cur[2]})
    changes, last = [], None
    for t, ok, lid, lat, e in shorts:
        if ok and lid != last:
            changes.append([round(t, 2), last, lid])
            last = lid
    errs = {}
    for s in fails:
        errs[s[4]] = errs.get(s[4], 0) + 1
    out = {"t0": round(t_start, 2), "dur": dur,
           "short": {"n": len(shorts), "fail": len(fails), "errs": errs, "fail_windows": windows,
                     "lid_changes": changes, "lat_ms": stats([s[3] for s in shorts if s[1]])},
           "long": {"breaks": breaks}}
    if udp:
        u = res[2]
        if u.get("lid_changes"):
            u["lid_changes"] = [[round(u["t0"] + c[0], 2), c[1], c[2]] for c in u["lid_changes"]]
        if u.get("outages"):
            u["outages"] = [{"from": round(u["t0"] + o[0], 2), "s": o[1]} for o in u["outages"]]
        out["udp"] = u
    return out


# ----------------------------------------------------------------- external tools
def iperf(port, host="127.0.0.1", reverse=False, udp=False, bitrate=None, secs=8, length=None, dev=None,
          parallel=1, timeout=None):
    cmd = ["iperf3", "-c", host, "-p", str(port), "-t", str(secs), "-J", "--connect-timeout", "8000"]
    if reverse:
        cmd.append("-R")
    if udp:
        cmd += ["-u", "-b", bitrate or "10M"]
    elif bitrate:
        cmd += ["-b", bitrate]
    if length:
        cmd += ["-l", str(length)]
    if dev:
        cmd += ["--bind-dev", dev]
    if parallel > 1:
        cmd += ["-P", str(parallel)]
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout or secs + 40)
        j = json.loads(p.stdout)
    except Exception as e:
        return {"ok": False, "err": _err(e)}
    if j.get("error"):
        return {"ok": False, "err": j["error"][:200]}
    e = j["end"]
    if udp:
        s = e.get("sum", {})
        return {"ok": True, "mbps": round(s.get("bits_per_second", 0) / 1e6, 2), "loss_pct": round(s.get("lost_percent", 0), 3),
                "jitter_ms": round(s.get("jitter_ms", 0), 3), "packets": s.get("packets")}
    return {"ok": True, "mbps": round(e["sum_received"]["bits_per_second"] / 1e6, 2),
            "sent_mbps": round(e["sum_sent"]["bits_per_second"] / 1e6, 2),
            "retrans": e["sum_sent"].get("retransmits")}


def curl(url, socks=None, dev=None, timeout=25, extra=None, out=False):
    fmt = "%{http_code} %{time_connect} %{time_starttransfer} %{time_total} %{size_download} %{speed_download} %{http_version}"
    cmd = ["curl", "-s", "-o", "-" if out else "/dev/null", "-w", ("\n" if out else "") + fmt, "--max-time", str(timeout),
           "-A", "Mozilla/5.0 (X11; Linux x86_64) rbt-test"]
    if socks:
        cmd += ["-x", "socks5h://127.0.0.1:%d" % socks]
    if dev:
        cmd += ["--interface", dev]
    cmd += (extra or []) + [url]
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout + 10)
        body, _, tail = p.stdout.rpartition("\n") if out else ("", "", p.stdout)
        f = tail.split()
        d = {"ok": p.returncode == 0 and f[0][0] in "23", "code": int(f[0]), "connect_ms": round(float(f[1]) * 1000),
             "ttfb_ms": round(float(f[2]) * 1000), "total_ms": round(float(f[3]) * 1000), "size": int(float(f[4])),
             "mbps": round(float(f[5]) * 8 / 1e6, 2), "http": f[6], "rc": p.returncode}
        if out:
            d["body"] = body[:2000]
        return d
    except Exception as e:
        return {"ok": False, "err": _err(e)}


if __name__ == "__main__":
    fn = globals()[sys.argv[1]]
    kw = json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}
    res = asyncio.run(fn(**kw)) if asyncio.iscoroutinefunction(fn) else fn(**kw)
    print(json.dumps(res))
