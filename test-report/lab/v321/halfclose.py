#!/usr/bin/env python3
"""Loopback experiments on how the relay ends a tcp connection. No external hosts."""
import socket, subprocess, sys, threading, time, os

REALM = sys.argv[1]
LP, RP = 39000, 39001          # relay listen, server
MB = 1 << 20

def server(kind, ready, out):
    ls = socket.socket(); ls.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    ls.bind(("127.0.0.1", RP)); ls.listen(8); ready.set()
    c, _ = ls.accept(); ls.close()
    got = 0
    try:
        if kind == "A":                       # read request to EOF, then answer
            while True:
                d = c.recv(65536)
                if not d: break
                got += len(d)
            time.sleep(0.5)
            c.sendall(b"r" * MB)
            c.close()
        elif kind in ("B", "C"):              # send 8 MiB, half-close, drain
            c.sendall(b"r" * (8 * MB))
            c.shutdown(socket.SHUT_WR)
            while True:
                d = c.recv(65536)
                if not d: break
                got += len(d)
            c.close()
        elif kind == "D":                     # slow reader of an upload
            c.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 32768)
            time.sleep(1.5)
            while True:
                d = c.recv(65536)
                if not d: break
                got += len(d)
            c.close()
    except OSError as e:
        out["server_err"] = repr(e)
    out["server_got"] = got

def client(kind, port, out):
    s = socket.socket()
    if kind in ("B", "C"):
        s.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 32768)
    s.connect(("127.0.0.1", port))
    got = 0
    try:
        if kind == "A":
            s.sendall(b"q" * 100_000); s.shutdown(socket.SHUT_WR)
        elif kind == "B":                     # keep uploading while the reply comes
            def up():
                try:
                    while True: s.sendall(b"u" * 16384); time.sleep(0.001)
                except OSError: pass
            threading.Thread(target=up, daemon=True).start()
            time.sleep(1.5)
        elif kind == "C":
            time.sleep(1.5)
        elif kind == "D":
            s.sendall(b"u" * (8 * MB)); s.close(); out["client_got"] = 0; return
        while True:
            d = s.recv(65536)
            if not d: break
            got += len(d)
    except OSError as e:
        out["client_err"] = repr(e)
    out["client_got"] = got
    try: s.close()
    except OSError: pass

def run(kind, via_relay):
    out = {}; ready = threading.Event()
    t = threading.Thread(target=server, args=(kind, ready, out)); t.start(); ready.wait()
    client(kind, LP if via_relay else RP, out)
    t.join(20)
    return out

relay = subprocess.Popen([REALM, "-l", f"127.0.0.1:{LP}", "-r", f"127.0.0.1:{RP}"],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(0.5)
try:
    want = {"A": "client_got=1048576 (reply after client half-close)",
            "B": "client_got=8388608 (reply tail while client still uploads)",
            "C": "client_got=8388608 (plain download, slow reader)",
            "D": "server_got=8388608 (plain upload, slow reader)"}
    for kind in "ABCD":
        for rep in range(3):
            d = run(kind, False); r = run(kind, True)
            print(f"{kind}#{rep} expect {want[kind]}\n    direct={d}\n    relay ={r}", flush=True)
finally:
    relay.terminate(); relay.wait(5)
