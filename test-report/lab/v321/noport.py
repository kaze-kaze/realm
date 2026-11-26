#!/usr/bin/env python3
"""What does a udp endpoint do when a new association cannot get a local port? Loopback only."""
import os, socket, subprocess, sys, threading, time
REALM, SHIM, POISON = sys.argv[1], sys.argv[2], sys.argv[3]
LP, RP = 39020, 39021

def echo():
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.bind(("127.0.0.1", RP))
    while True:
        d, a = s.recvfrom(2048); s.sendto(d, a)
threading.Thread(target=echo, daemon=True).start()

def ask(sock, tag, wait=1.5):
    sock.settimeout(wait); sock.sendto(tag, ("127.0.0.1", LP))
    try:
        d, _ = sock.recvfrom(2048); return "echo ok" if d == tag else f"wrong {d!r}"
    except socket.timeout:
        return "NO ANSWER"

def client():
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.bind(("127.0.0.1", 0)); return s

if os.path.exists(POISON): os.remove(POISON)
env = dict(os.environ, LD_PRELOAD=SHIM, POISON_FILE=POISON)
relay = subprocess.Popen([REALM, "-u", "-l", f"127.0.0.1:{LP}", "-r", f"127.0.0.1:{RP}", "--udp-timeout", "10",
                          "--log-level", "debug", "--log-output", os.path.dirname(POISON) + "/noport-relay.log"],
                         env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(0.6)
try:
    a, b, c, d = client(), client(), client(), client()
    print("1. client A, ports available          :", ask(a, b"a1"), flush=True)
    open(POISON, "w").close()
    print("2. client B, no port left for it       :", ask(b, b"b1"), flush=True)
    print("3. client A, already associated        :", ask(a, b"a2"), flush=True)
    os.remove(POISON); time.sleep(2)
    print("4. ports are back: client A            :", ask(a, b"a3"), flush=True)
    print("5. ports are back: new client C        :", ask(c, b"c1"), flush=True)
    time.sleep(15)                         # past udp_timeout (10 s): associations expire
    print("6. 15 s later: client A again          :", ask(a, b"a4"), flush=True)
    print("7. 15 s later: new client D            :", ask(d, b"d1"), flush=True)
    print("relay still running:", relay.poll() is None, flush=True)
finally:
    relay.terminate(); relay.wait(5)
    if os.path.exists(POISON): os.remove(POISON)
