"""Shared topology, ports and helpers for the real-world test bench."""
import json
import os

LAB = os.path.dirname(os.path.abspath(__file__))
RUN = os.environ.get("RBT_RUN", "/rbt/run")
BIN = os.environ.get("RBT_BIN", "/rbt/bin")

HOSTS = {
    # name: public ip, landing index (reported by targets), wg tunnel /24 index.
    # The addresses are placeholders from the documentation range: the real ones are
    # not kept in the repository. Put them in hosts.local.json, {"chi": "a.b.c.d", ...}.
    "chi": {"ip": "198.51.100.11", "idx": 1},
    "ams": {"ip": "198.51.100.12", "idx": 2},
    "po0": {"ip": "198.51.100.13", "idx": 3},
    "mac": {"ip": None, "idx": 4},
}
_local = os.path.join(LAB, "hosts.local.json")
if os.path.exists(_local):
    for _name, _ip in json.load(open(_local)).items():
        HOSTS[_name]["ip"] = _ip
IDX2HOST = {v["idx"]: k for k, v in HOSTS.items()}

# relay -> [primary landing, backup landing]
RELAYS = {
    "chi": ["po0", "ams"],
    "po0": ["chi", "ams"],
    "ams": ["chi", "po0"],
}

LBASE = 31000  # landing ports = LBASE + off
# name, off, tcp, udp
SVC = [
    ("vmess-tcp", 1, 1, 0),
    ("vmess-ws", 2, 1, 0),
    ("vless-enc", 3, 1, 0),
    ("vless-reality", 4, 1, 0),
    ("vless-ws-tls", 5, 1, 0),
    ("vless-xhttp-tls", 6, 1, 0),
    ("trojan-tls", 7, 1, 0),
    ("ss-aead", 8, 1, 1),
    ("ss-2022", 9, 1, 1),
    ("vmess-kcp", 10, 0, 1),
    ("vless-grpc-tls", 11, 1, 0),
    ("vless-hu-tls", 12, 1, 0),
    ("hy2", 20, 0, 1),
    ("hy2-obfs", 21, 0, 1),
    ("wg", 30, 0, 1),
    ("health", 40, 1, 1),
    ("iperf", 50, 1, 1),
    ("tgt", 60, 1, 1),
]
OFF = {n: o for n, o, _, _ in SVC}
XRAY_PROTOS = [n for n, o, _, _ in SVC if o <= 12]
HY_PROTOS = ["hy2", "hy2-obfs"]
# protocols whose proxied UDP travels as real UDP between client and landing
NATIVE_UDP = {"ss-aead", "ss-2022", "vmess-kcp", "hy2", "hy2-obfs", "wg"}

# relay listen bases
VIA_BASE = {
    "fo": 21000,   # v3 failover: primary + backup
    "pl": 22000,   # v3 plain -> primary
    "p2": 22500,   # v3 plain -> backup
    "og": 23000,   # upstream v2.9.6 plain -> primary
    "rr": 24000,   # v3 roundrobin over both
    "ih": 24500,   # v3 iphash over both
    # endpoints below need the build with the new features (relay-up opts {"new": true})
    "fd": 21500,   # failover probing the service ports themselves: tcp handshake, quic for hy2
    "rh": 24200,   # roundrobin + health checks
    "hh": 24700,   # iphash + health checks
}
# services an "fd" endpoint can probe on their own port, and whether their udp side is relayed
FD_SVC = {"hy2": "quic"}

TGT_LOCAL = "127.0.0.1"   # targets as seen from a landing's proxy
WG_NET = "10.99"          # server 10.99.0.1/16, clients 10.99.<host idx>.<slot>


def secrets():
    path = os.path.join(LAB, "secrets.json")
    if not os.path.exists(path):
        raise SystemExit("no test credentials in %s: run mksecrets.py first" % LAB)
    return json.load(open(path))


def lport(name):
    return LBASE + OFF[name]


def via_addr(via, name):
    """via: 'fo-chi' | 'pl-chi' | 'og-chi' | 'rr-chi' | 'ih-chi' | 'p2-chi' | 'direct-po0' -> (ip, port)"""
    kind, host = via.split("-", 1)
    if kind == "direct":
        return HOSTS[host]["ip"], LBASE + OFF[name]
    return HOSTS[host]["ip"], VIA_BASE[kind] + OFF[name]


def default_vias(me):
    """Client instances started on host `me` (order fixes local port blocks)."""
    out = []
    for r in ("chi", "po0", "ams"):
        for k in ("fo", "pl", "og", "rr", "ih", "p2"):
            out.append("%s-%s" % (k, r))
    for l in ("chi", "po0", "ams"):
        out.append("direct-%s" % l)
    for r in ("chi", "po0", "ams"):
        for k in ("fd", "rh", "hh"):
            out.append("%s-%s" % (k, r))
    return out


def via_index(via):
    return default_vias(None).index(via)


def cport(via, name, kind="socks"):
    """Local client port for (via, protocol). kind: socks | fwd (iperf forward) | fwd2 (tgt forward)"""
    base = 10000 + 100 * via_index(via)
    add = {"socks": 0, "fwd": 30, "fwd2": 60, "fwd3": 80}[kind]
    o = OFF[name]
    if o >= 20:           # hy2 20,21 -> 13,14 ; wg 30 -> 15
        o = {20: 13, 21: 14, 30: 15}[o]
    return base + add + o


def wg_slot(via):
    return 2 + via_index(via)


# WireGuard carried inside a proxy's udp channel ("wg in ss"): the tunnel's endpoint is a local
# port that the proxy forwards to the landing's own wg port.
WGIN_PROTOS = ["ss-2022", "ss-aead"]


def wgin_combos():
    out = []
    for r in ("chi", "po0", "ams"):
        for k in ("pl", "og", "fo"):
            for p in WGIN_PROTOS:
                out.append(("%s-%s" % (k, r), p))
    for l in ("chi", "po0", "ams"):
        for p in WGIN_PROTOS:
            out.append(("direct-%s" % l, p))
    return out


def wgin_slot(via, proto):
    return 100 + wgin_combos().index((via, proto))
