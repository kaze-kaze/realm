#!/usr/bin/env python3
"""Where `iphash` sends clients, measured on a running realm.

Clients connect from random loopback addresses (linux answers on all of
127.0.0.0/8), each backend replies with its own letter, and the letters are
counted. Needs nothing but loopback, so any container will do:

    python3 iphash_dist.py [clients]

REALM_NEW names the binary under test (default /work/bin/realm-new).
"""
import asyncio
import json
import os
import random
import subprocess
import sys
import tempfile

REALM = os.environ.get("REALM_NEW", "/work/bin/realm-new")
CLIENTS = int(sys.argv[1]) if len(sys.argv) > 1 else 40000
LISTEN = 21000
FAST = "interval = 0.3\ntimeout = 0.2\nfall = 2\nwindow = 3\nrise = 2"

# (weights, peers that are dead, failover on)
CASES = [
    ([1, 1], [], True),
    ([1, 1, 1], [], True),
    ([1, 1, 1, 1], [], True),
    ([2, 1, 1], [], True),
    ([4, 2, 1], [], True),
    ([4, 2, 1], [], False),
    ([2, 1, 1], [0], True),
    ([4, 2, 1], [1], True),
    ([1, 1, 1, 1], [3], True),
    ([1, 2, 3, 2], [2], True),
]


async def backend(port, letter):
    async def serve(reader, writer):
        writer.write(letter)
        try:
            await writer.drain()
        except OSError:
            pass
        writer.close()

    return await asyncio.start_server(serve, "127.0.0.1", port, backlog=4096)


async def ask(ip, sem, count):
    async with sem:
        for _ in range(3):
            try:
                reader, writer = await asyncio.open_connection("127.0.0.1", LISTEN, local_addr=(ip, 0))
                letter = await asyncio.wait_for(reader.read(1), 5)
                writer.close()
                if letter:
                    count[letter] = count.get(letter, 0) + 1
                    return
            except (OSError, asyncio.TimeoutError):
                await asyncio.sleep(0.05)
        count[b"-"] = count.get(b"-", 0) + 1


async def run_case(weights, dead, failover, rng):
    n = len(weights)
    ports = [LISTEN + 1 + i for i in range(n)]
    servers = [await backend(p, bytes([65 + i])) for i, p in enumerate(ports) if i not in dead]
    conf = "[log]\nlevel = \"error\"\n\n[[endpoints]]\n"
    conf += f"listen = \"127.0.0.1:{LISTEN}\"\nremote = \"127.0.0.1:{ports[0]}\"\n"
    conf += "extra_remotes = [" + ", ".join(f"\"127.0.0.1:{p}\"" for p in ports[1:]) + "]\n"
    conf += "balance = \"iphash: " + ", ".join(map(str, weights)) + "\"\n"
    if failover:
        conf += "\n[endpoints.failover]\n" + FAST + "\n"
    with tempfile.NamedTemporaryFile("w", suffix=".toml", delete=False) as f:
        f.write(conf)
    proc = subprocess.Popen([REALM, "-c", f.name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        # let the probes find the dead peers
        await asyncio.sleep(2.0 if dead else 1.0)
        ips = set()
        while len(ips) < CLIENTS:
            ips.add(f"127.{rng.randrange(256)}.{rng.randrange(256)}.{rng.randrange(1, 255)}")
        count, sem = {}, asyncio.Semaphore(200)
        await asyncio.gather(*(ask(ip, sem, count) for ip in ips))
    finally:
        proc.terminate()
        proc.wait()
        os.unlink(f.name)
        for s in servers:
            s.close()
            await s.wait_closed()
    alive = sum(w for i, w in enumerate(weights) if i not in dead)
    got = [count.get(bytes([65 + i]), 0) * 100 / CLIENTS for i in range(n)]
    want = [0 if i in dead else w * 100 / alive for i, w in enumerate(weights)]
    return {
        "weights": weights,
        "dead": dead,
        "failover": failover,
        "clients": CLIENTS,
        "failed": count.get(b"-", 0),
        "got": [round(x, 1) for x in got],
        "want": [round(x, 1) for x in want],
        "worst": round(max(abs(g - w) for g, w in zip(got, want)), 2),
    }


async def main():
    rng = random.Random(20261120)
    out = []
    for weights, dead, failover in CASES:
        res = await run_case(weights, dead, failover, rng)
        out.append(res)
        mode = "balance + failover" if failover else "balance only"
        down = f", peer {'/'.join(str(i) for i in dead)} dead" if dead else ""
        print(
            f"{' : '.join(map(str, weights)):<14} {mode}{down}: "
            + " / ".join(f"{x:.1f}%" for x in res["got"])
            + "   expected "
            + " / ".join(f"{x:.1f}%" for x in res["want"])
            + f"   worst {res['worst']:.2f}, failed {res['failed']}",
            flush=True,
        )
    path = os.path.join(os.environ.get("LAB_OUT", "."), "iphash_dist.json")
    with open(path, "w") as f:
        json.dump(out, f, indent=1)


asyncio.run(main())
