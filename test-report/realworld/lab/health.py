#!/usr/bin/env python3
"""Health port for a landing server: TCP answers "HTTP/1.1 204", UDP echoes.

  health.py <port> [--gate FILE] [--check CMD] [--every SEC]

Unhealthy (TCP 503, UDP silent) while FILE exists or while CMD exits non-zero.
CMD lets the port follow the real services, e.g. --check "pgrep -x xray && pgrep -x hysteria".
"""
import asyncio
import os
import subprocess
import sys

OK = b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n"
BAD = b"HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
state = {"check_ok": True}
gate = None


def healthy():
    return state["check_ok"] and not (gate and os.path.exists(gate))


async def tcp(r, w):
    try:
        try:
            await asyncio.wait_for(r.readuntil(b"\r\n\r\n"), 2)
        except (asyncio.TimeoutError, asyncio.IncompleteReadError, asyncio.LimitOverrunError):
            pass
        w.write(OK if healthy() else BAD)
        await w.drain()
    except ConnectionError:
        pass
    finally:
        try:
            w.close()
        except Exception:
            pass


class Echo(asyncio.DatagramProtocol):
    def connection_made(self, tr):
        self.tr = tr

    def datagram_received(self, data, addr):
        if healthy():
            self.tr.sendto(data, addr)


async def checker(cmd, every):
    while True:
        p = await asyncio.create_subprocess_shell(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        state["check_ok"] = (await p.wait()) == 0
        await asyncio.sleep(every)


async def main():
    global gate
    a = sys.argv[1:]
    port = int(a[0])
    cmd, every = None, 2.0
    for i, x in enumerate(a):
        if x == "--gate":
            gate = a[i + 1]
        elif x == "--check":
            cmd = a[i + 1]
        elif x == "--every":
            every = float(a[i + 1])
    loop = asyncio.get_running_loop()
    await asyncio.start_server(tcp, "0.0.0.0", port, backlog=256)
    await loop.create_datagram_endpoint(Echo, local_addr=("0.0.0.0", port))
    if cmd:
        asyncio.ensure_future(checker(cmd, every))
    await asyncio.Event().wait()


if __name__ == "__main__":
    asyncio.run(main())
