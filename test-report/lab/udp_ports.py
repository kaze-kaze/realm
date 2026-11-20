"""Do upstream udp sockets of different clients end up on the same local port?"""
import json, sys, time
from collections import Counter
from lab import *
from scen_fo import Case, R
from extreme import PLAIN
net_up()
for name, binary, rng in (("orig, ports 1024-65535", ORIG, "1024 65535"), ("orig, ports 32768-60999 (linux default)", ORIG, "32768 60999"),
                          ("final, ports 32768-60999", NEW, "32768 60999")):
    c = Case(PLAIN.replace("udp_timeout = 5", "udp_timeout = 30"), binary=binary)
    nsx("r", f"sysctl -qw net.ipv4.ip_local_port_range='{rng}'")
    c.run("udp", f"udp target={R} socks=5000 pps=10000 secs=12 spread=3000 threads=6")
    c.at(9)
    ports = [l.split()[3].rsplit(":", 1)[1] for l in nsx("r", "ss -Huan").split("\n") if l.strip() and ":6000" not in l.split()[3]]
    dup = Counter(ports)
    shared = sum(v for v in dup.values() if v > 1)
    u = c.res("udp")
    nsx("r", "sysctl -qw net.ipv4.ip_local_port_range='1024 65535'")
    c.close()
    print(f"{name}: upstream sockets {len(ports)}, distinct ports {len(dup)}, sockets sharing a port {shared}; client: delivered {u['total_ok']}/{u['sent']}, foreign (other client's reply) {u['foreign']}", flush=True)
net_down()
