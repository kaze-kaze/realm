#!/usr/bin/env python3
"""Landing-side targets.

TCP <port>: one command line, then
  ID                    -> "<lid>\n"
  SRC <n> <seed>        -> n bytes of det.stream
  SINK <n> <seed>       -> reads n bytes, replies "OK|BAD <got> <first bad off> <lid>\n"
  ECHO                  -> "<lid>\n", then echoes everything
  PUSH <ivl> <cnt> <sz> -> "<lid>\n", then cnt server-initiated messages, ivl seconds apart
UDP <port>..<port+extra>: header !2sBBIId (magic, lid, kind, flow, seq, ts)
  kind 0: echo, reply magic RS with lid filled in
  kind 1: subscribe, body !HHf (pps, size, seconds): server pushes RP packets to the sender
"""
import asyncio
import os
import struct
import sys
import time

import det

HDR = struct.Struct("!2sBBIId")
LID = int(open(os.environ.get("RBT_RUN", "/rbt/run") + "/land/id").read())


async def handle(r, w):
    try:
        line = await asyncio.wait_for(r.readline(), 30)
        p = line.decode("ascii", "replace").split()
        if not p:
            return
        cmd = p[0]
        if cmd == "ID":
            w.write(b"%d\n" % LID)
            await w.drain()
        elif cmd == "SRC":
            n, seed = int(p[1]), int(p[2])
            for chunk in det.stream(seed, n):
                w.write(chunk)
                await w.drain()
        elif cmd == "SINK":
            n, seed = int(p[1]), int(p[2])
            ck = det.Checker(seed)
            while ck.off < n:
                d = await r.read(min(262144, n - ck.off))
                if not d:
                    break
                ck.feed(d)
            ok = ck.off == n and ck.bad is None
            w.write(b"%s %d %d %d\n" % (b"OK" if ok else b"BAD", ck.off, -1 if ck.bad is None else ck.bad, LID))
            await w.drain()
        elif cmd == "ECHO":
            w.write(b"%d\n" % LID)
            while True:
                d = await r.read(65536)
                if not d:
                    break
                w.write(d)
                await w.drain()
        elif cmd == "PUSH":
            ivl, cnt, sz = float(p[1]), int(p[2]), int(p[3])
            w.write(b"%d\n" % LID)
            await w.drain()
            for i in range(cnt):
                await asyncio.sleep(ivl)
                w.write(struct.pack("!Id", i, time.time()) + b"p" * max(0, sz - 12))
                await w.drain()
    except (ConnectionError, asyncio.TimeoutError, asyncio.IncompleteReadError, ValueError, IndexError):
        pass
    finally:
        try:
            w.close()
        except Exception:
            pass


class Udp(asyncio.DatagramProtocol):
    def connection_made(self, tr):
        self.tr = tr

    def datagram_received(self, data, addr):
        if len(data) < HDR.size:
            return
        magic, _, kind, flow, seq, ts = HDR.unpack_from(data)
        if magic != b"RQ":
            return
        self.tr.sendto(HDR.pack(b"RS", LID, kind, flow, seq, ts) + data[HDR.size:], addr)
        if kind == 1 and len(data) >= HDR.size + 8:
            pps, size, dur = struct.unpack_from("!HHf", data, HDR.size)
            asyncio.ensure_future(self.push(addr, flow, pps, size, min(dur, 3600.0)))

    async def push(self, addr, flow, pps, size, dur):
        loop = asyncio.get_running_loop()
        pad = b"s" * max(0, size - HDR.size)
        t0 = loop.time()
        n = int(pps * dur)
        for i in range(n):
            due = t0 + i / pps
            d = due - loop.time()
            if d > 0:
                await asyncio.sleep(d)
            self.tr.sendto(HDR.pack(b"RP", LID, 1, flow, i, time.time()) + pad, addr)


async def main():
    port = int(sys.argv[1])
    extra = int(sys.argv[2]) if len(sys.argv) > 2 else 8
    loop = asyncio.get_running_loop()
    await asyncio.start_server(handle, "0.0.0.0", port, backlog=1024, limit=1 << 20)
    for i in range(extra + 1):
        await loop.create_datagram_endpoint(Udp, local_addr=("0.0.0.0", port + i))
    print("tgt up lid", LID, flush=True)
    await asyncio.Event().wait()


if __name__ == "__main__":
    asyncio.run(main())
