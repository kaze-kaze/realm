#!/usr/bin/env python3
"""Generate configs. Usage: gen.py land <host> | relay <host> [opts-json] | cli <host> <via> [...]"""
import base64
import hashlib
import json
import os
import subprocess
import sys

import topo
from topo import HOSTS, RELAYS, SVC, OFF, LBASE, VIA_BASE, XRAY_PROTOS, RUN, BIN, LAB

S = topo.secrets()
PKI = os.path.join(LAB, "pki")
TLS_NAME = "rbt.test"
REALITY_SNI = "www.microsoft.com"
# VLESS Encryption (xray refuses plain VLESS to a public address); X25519 authentication, key shared with REALITY
VLESS_DEC = "mlkem768x25519plus.native.600s."
VLESS_ENC = "mlkem768x25519plus.native.0rtt."


def sh(*a, inp=None):
    return subprocess.run(a, input=inp, capture_output=True, text=True, check=True).stdout.strip()


def wg_pub(priv):
    return sh("wg", "pubkey", inp=priv + "\n")


def wg_client_key(host, via):
    return base64.b64encode(hashlib.sha256(("%s|%s|%s" % (S["wg_server"], host, via)).encode()).digest()).decode()


def reality_pub():
    out = sh(os.path.join(BIN, "xray"), "x25519", "-i", S["reality_priv_raw"])
    for line in out.splitlines():
        low = line.lower()
        if "public" in low or low.startswith("password"):
            return line.split(":", 1)[1].strip()
    raise SystemExit("cannot parse xray x25519 output: %r" % out)


def w(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(text)


# ---------------------------------------------------------------- landing
def tls_srv(alpn=None):
    t = {"certificates": [{"certificateFile": PKI + "/leaf.crt", "keyFile": PKI + "/leaf.key"}]}
    if alpn:
        t["alpn"] = alpn
    return t


def xray_server():
    u = S["uuid"]
    vl = {"clients": [{"id": u}], "decryption": "none"}
    vm = {"clients": [{"id": u}]}
    ib = []

    def add(name, proto, settings, stream=None):
        d = {"tag": name, "listen": "0.0.0.0", "port": LBASE + OFF[name], "protocol": proto, "settings": settings}
        if stream:
            d["streamSettings"] = stream
        ib.append(d)

    add("vmess-tcp", "vmess", vm, {"network": "raw"})
    add("vmess-ws", "vmess", vm, {"network": "ws", "wsSettings": {"path": "/vmws"}})
    add("vless-enc", "vless", {"clients": [{"id": u}], "decryption": VLESS_DEC + S["reality_priv_raw"]},
        {"network": "raw"})
    add("vless-reality", "vless", {"clients": [{"id": u, "flow": "xtls-rprx-vision"}], "decryption": "none"},
        {"network": "raw", "security": "reality",
         "realitySettings": {"target": REALITY_SNI + ":443", "serverNames": [REALITY_SNI],
                             "privateKey": S["reality_priv_raw"], "shortIds": [S["reality_sid"]]}})
    add("vless-ws-tls", "vless", vl, {"network": "ws", "security": "tls", "tlsSettings": tls_srv(),
                                      "wsSettings": {"path": "/vlws"}})
    add("vless-xhttp-tls", "vless", vl, {"network": "xhttp", "security": "tls",
                                         "tlsSettings": tls_srv(["h2", "http/1.1"]),
                                         "xhttpSettings": {"path": "/xh"}})
    add("trojan-tls", "trojan", {"clients": [{"password": S["trojan_pw"]}]},
        {"network": "raw", "security": "tls", "tlsSettings": tls_srv()})
    add("ss-aead", "shadowsocks", {"method": "aes-256-gcm", "password": S["ss_pw"], "network": "tcp,udp"})
    add("ss-2022", "shadowsocks", {"method": "2022-blake3-aes-128-gcm", "password": S["ss2022_key"],
                                   "network": "tcp,udp"})
    add("vmess-kcp", "vmess", vm, {"network": "kcp", "kcpSettings": {}})
    add("vless-grpc-tls", "vless", vl, {"network": "grpc", "security": "tls", "tlsSettings": tls_srv(["h2"]),
                                        "grpcSettings": {"serviceName": "rbtgrpc"}})
    add("vless-hu-tls", "vless", vl, {"network": "httpupgrade", "security": "tls", "tlsSettings": tls_srv(),
                                      "httpupgradeSettings": {"path": "/hu"}})
    return {"log": {"loglevel": "warning", "error": RUN + "/land/xray.log"},
            "inbounds": ib,
            # xray >= 26 refuses loopback/private targets unless allowed; our test targets live on 127.0.0.1
            "outbounds": [{"protocol": "freedom", "tag": "direct",
                           "settings": {"finalRules": [{"action": "allow", "ip": ["127.0.0.0/8"]}]}}]}


def gen_land(host):
    d = RUN + "/land"
    w(d + "/xray.json", json.dumps(xray_server(), indent=1))
    hy = ("listen: :%d\ntls:\n  cert: %s/leaf.crt\n  key: %s/leaf.key\nauth:\n  type: password\n  password: %s\n"
          "masquerade:\n  type: string\n  string:\n    content: ok\n    statusCode: 200\n")
    # RBT_HY_SRV_IDLE: quic idle timeout on the server; clients notice a dead server after this long
    idle = os.environ.get("RBT_HY_SRV_IDLE")
    q = "quic:\n  maxIdleTimeout: %s\n" % idle if idle else ""
    w(d + "/hy2.yaml", hy % (LBASE + OFF["hy2"], PKI, PKI, S["hy_pw"]) + q)
    w(d + "/hy2-obfs.yaml", hy % (LBASE + OFF["hy2-obfs"], PKI, PKI, S["hy_pw"])
      + "obfs:\n  type: salamander\n  salamander:\n    password: %s\n" % S["hy_obfs"] + q)
    wg = "[Interface]\nPrivateKey = %s\nListenPort = %d\n" % (S["wg_server"], LBASE + OFF["wg"])
    for h, hv in HOSTS.items():
        for via in topo.default_vias(h):
            wg += "[Peer]\nPublicKey = %s\nAllowedIPs = %s.%d.%d/32\n" % (
                wg_pub(wg_client_key(h, via)), topo.WG_NET, hv["idx"], topo.wg_slot(via))
        for via, proto in topo.wgin_combos():
            wg += "[Peer]\nPublicKey = %s\nAllowedIPs = %s.%d.%d/32\n" % (
                wg_pub(wg_client_key(h, "wgin|%s|%s" % (via, proto))), topo.WG_NET, hv["idx"],
                topo.wgin_slot(via, proto))
    w(d + "/wg0.conf", wg)
    w(d + "/id", "%d\n" % HOSTS[host]["idx"])
    print("land ok", host)


# ---------------------------------------------------------------- relay
def ep(listen, remote, extra=None, tcp=1, udp=1, balance=None, failover=None, net=None):
    s = '[[endpoints]]\nlisten = "0.0.0.0:%d"\nremote = "%s"\n' % (listen, remote)
    if extra:
        s += "extra_remotes = [%s]\n" % ", ".join('"%s"' % e for e in extra)
    if balance:
        s += 'balance = "%s"\n' % balance
    n = {"no_tcp": not tcp, "use_udp": bool(udp)}
    n.update(net or {})
    s += "network = { %s }\n" % ", ".join(
        "%s = %s" % (k, json.dumps(v)) for k, v in n.items())
    if failover is not None:
        s += "[endpoints.failover]\n"
        for k, v in failover.items():
            s += "%s = %s\n" % (k, json.dumps(v))
    return s + "\n"


def gen_relay(host, opts):
    l1, l2 = RELAYS[host]
    ip1, ip2 = HOSTS[l1]["ip"], HOSTS[l2]["ip"]
    lvl = opts.get("log", "info")
    d = RUN + "/relay"
    fo_base = {"probe": ["%s:%d" % (ip1, LBASE + OFF["health"]), "%s:%d" % (ip2, LBASE + OFF["health"])],
               "tcp_send": "GET / HTTP/1.0\r\n\r\n", "tcp_expect": "HTTP/1.1 204"}
    fo_base.update(opts.get("failover", {}))
    gnet = {"no_tcp": False, "use_udp": True}
    gnet.update(opts.get("network", {}))
    head = '[log]\nlevel = "%s"\noutput = "%s/%s.log"\n\n[network]\n%s\n\n'
    gs = "\n".join("%s = %s" % (k, json.dumps(v)) for k, v in gnet.items())
    v3 = head % (lvl, d, "v3", gs)
    og = head % (lvl, d, "og", gs)
    for name, off, tcp, udp in SVC:
        r1, r2 = "%s:%d" % (ip1, LBASE + off), "%s:%d" % (ip2, LBASE + off)
        fo = dict(fo_base)
        if opts.get("probe_data"):
            fo = {k: v for k, v in fo.items() if k not in ("probe", "tcp_send", "tcp_expect")}
        v3 += "# %s\n" % name
        v3 += ep(VIA_BASE["fo"] + off, r1, [r2], tcp, udp, failover=fo, net={"tcp_timeout": 4})
        v3 += ep(VIA_BASE["pl"] + off, r1, None, tcp, udp)
        v3 += ep(VIA_BASE["p2"] + off, r2, None, tcp, udp)
        v3 += ep(VIA_BASE["rr"] + off, r1, [r2], tcp, udp, balance="roundrobin: 1, 1")
        v3 += ep(VIA_BASE["ih"] + off, r1, [r2], tcp, udp, balance="iphash: 1, 1")
        og += ep(VIA_BASE["og"] + off, r1, None, tcp, udp)
        if opts.get("new"):
            # balancing over the landings that pass the health check
            v3 += ep(VIA_BASE["rh"] + off, r1, [r2], tcp, udp, balance="roundrobin: 1, 1", failover=fo,
                     net={"tcp_timeout": 4})
            v3 += ep(VIA_BASE["hh"] + off, r1, [r2], tcp, udp, balance="iphash: 1, 1", failover=fo,
                     net={"tcp_timeout": 4})
            # no health port: probe the service itself. tcp by handshake, hysteria by a quic probe;
            # other udp services cannot answer a probe, so their udp side is left out here
            quic = topo.FD_SVC.get(name) == "quic"
            if tcp or quic:
                v3 += ep(VIA_BASE["fd"] + off, r1, [r2], tcp, 1 if quic else 0,
                         failover={"udp_probe": "quic"} if quic else {}, net={"tcp_timeout": 4})
    v3 += opts.get("extra_toml", "")
    w(d + "/v3.toml", v3)
    w(d + "/og.toml", og)
    print("relay ok", host, "primary", l1, "backup", l2)


# ---------------------------------------------------------------- client
def iperf_port(host):
    """Each client host gets its own iperf3 server on the landing, so hosts do not collide."""
    return LBASE + OFF["iperf"] + HOSTS[host]["idx"]


def xray_client(host, via, pub):
    u = S["uuid"]
    ib, ob, rules = [], [], []

    def out(name, proto, settings, stream=None):
        d = {"tag": "o-" + name, "protocol": proto, "settings": settings}
        if stream:
            d["streamSettings"] = stream
        ob.append(d)
        ib.append({"tag": "s-" + name, "listen": "0.0.0.0", "port": topo.cport(via, name, "socks"),
                   "protocol": "socks", "settings": {"auth": "noauth", "udp": True}})
        ib.append({"tag": "f-" + name, "listen": "127.0.0.1", "port": topo.cport(via, name, "fwd"),
                   "protocol": "dokodemo-door",
                   "settings": {"address": topo.TGT_LOCAL, "port": iperf_port(host), "network": "tcp,udp"}})
        ib.append({"tag": "g-" + name, "listen": "127.0.0.1", "port": topo.cport(via, name, "fwd2"),
                   "protocol": "dokodemo-door",
                   "settings": {"address": topo.TGT_LOCAL, "port": LBASE + OFF["tgt"], "network": "tcp,udp"}})
        tags = ["s-" + name, "f-" + name, "g-" + name]
        if name in topo.WGIN_PROTOS:
            # udp to the landing's own wireguard port, for a tunnel that runs inside this proxy
            ib.append({"tag": "h-" + name, "listen": "127.0.0.1", "port": topo.cport(via, name, "fwd3"),
                       "protocol": "dokodemo-door",
                       "settings": {"address": topo.TGT_LOCAL, "port": LBASE + OFF["wg"], "network": "udp"}})
            tags.append("h-" + name)
        rules.append({"type": "field", "inboundTag": tags, "outboundTag": "o-" + name})

    def vnext(name, **user):
        ip, port = topo.via_addr(via, name)
        usr = {"id": u}
        usr.update(user)
        return {"vnext": [{"address": ip, "port": port, "users": [usr]}]}

    def servers(name, **kw):
        ip, port = topo.via_addr(via, name)
        d = {"address": ip, "port": port}
        d.update(kw)
        return {"servers": [d]}

    tls = {"serverName": TLS_NAME, "disableSystemRoot": True,
           "certificates": [{"usage": "verify", "certificateFile": PKI + "/ca.crt"}]}
    out("vmess-tcp", "vmess", vnext("vmess-tcp", security="auto"), {"network": "raw"})
    out("vmess-ws", "vmess", vnext("vmess-ws", security="auto"), {"network": "ws", "wsSettings": {"path": "/vmws"}})
    out("vless-enc", "vless", vnext("vless-enc", encryption=VLESS_ENC + pub), {"network": "raw"})
    out("vless-reality", "vless", vnext("vless-reality", encryption="none", flow="xtls-rprx-vision"),
        {"network": "raw", "security": "reality",
         "realitySettings": {"serverName": REALITY_SNI, "fingerprint": "chrome", "publicKey": pub,
                             "shortId": S["reality_sid"]}})
    out("vless-ws-tls", "vless", vnext("vless-ws-tls", encryption="none"),
        {"network": "ws", "security": "tls", "tlsSettings": tls, "wsSettings": {"path": "/vlws"}})
    out("vless-xhttp-tls", "vless", vnext("vless-xhttp-tls", encryption="none"),
        {"network": "xhttp", "security": "tls", "tlsSettings": dict(tls, alpn=["h2"]),
         "xhttpSettings": {"path": "/xh"}})
    out("trojan-tls", "trojan", servers("trojan-tls", password=S["trojan_pw"]),
        {"network": "raw", "security": "tls", "tlsSettings": tls})
    out("ss-aead", "shadowsocks", servers("ss-aead", method="aes-256-gcm", password=S["ss_pw"]))
    out("ss-2022", "shadowsocks", servers("ss-2022", method="2022-blake3-aes-128-gcm", password=S["ss2022_key"]))
    out("vmess-kcp", "vmess", vnext("vmess-kcp", security="auto"), {"network": "kcp", "kcpSettings": {}})
    out("vless-grpc-tls", "vless", vnext("vless-grpc-tls", encryption="none"),
        {"network": "grpc", "security": "tls", "tlsSettings": dict(tls, alpn=["h2"]),
         "grpcSettings": {"serviceName": "rbtgrpc"}})
    out("vless-hu-tls", "vless", vnext("vless-hu-tls", encryption="none"),
        {"network": "httpupgrade", "security": "tls", "tlsSettings": tls, "httpupgradeSettings": {"path": "/hu"}})
    return {"log": {"loglevel": "warning", "error": "%s/cli/%s.xray.log" % (RUN, via)},
            "inbounds": ib, "outbounds": ob, "routing": {"rules": rules}}


def gen_cli(host, vias):
    d = RUN + "/cli"
    pub = reality_pub()
    for via in vias:
        w("%s/%s.xray.json" % (d, via), json.dumps(xray_client(host, via, pub), indent=1))
        for name in topo.HY_PROTOS:
            ip, port = topo.via_addr(via, name)
            y = ("server: %s:%d\nauth: %s\ntls:\n  sni: %s\n  ca: %s/ca.crt\n"
                 "socks5:\n  listen: 0.0.0.0:%d\n"
                 "tcpForwarding:\n  - listen: 127.0.0.1:%d\n    remote: %s:%d\n"
                 "  - listen: 127.0.0.1:%d\n    remote: %s:%d\n"
                 "udpForwarding:\n  - listen: 127.0.0.1:%d\n    remote: %s:%d\n    timeout: 120s\n"
                 "  - listen: 127.0.0.1:%d\n    remote: %s:%d\n    timeout: 120s\n") % (
                ip, port, S["hy_pw"], TLS_NAME, PKI,
                topo.cport(via, name, "socks"),
                topo.cport(via, name, "fwd"), topo.TGT_LOCAL, iperf_port(host),
                topo.cport(via, name, "fwd2"), topo.TGT_LOCAL, LBASE + OFF["tgt"],
                topo.cport(via, name, "fwd"), topo.TGT_LOCAL, iperf_port(host),
                topo.cport(via, name, "fwd2"), topo.TGT_LOCAL, LBASE + OFF["tgt"])
            if name == "hy2-obfs":
                y += "obfs:\n  type: salamander\n  salamander:\n    password: %s\n" % S["hy_obfs"]
            q = os.environ.get("RBT_HY_QUIC")       # "<maxIdleTimeout>,<keepAlivePeriod>", e.g. "8s,2s"
            if q:
                y += "quic:\n  maxIdleTimeout: %s\n  keepAlivePeriod: %s\n" % tuple(q.split(","))
            w("%s/%s.%s.yaml" % (d, via, name), y)
        ip, port = topo.via_addr(via, "wg")
        key = wg_client_key(host, via)
        w("%s/%s.wg.conf" % (d, via),
          "[Interface]\nPrivateKey = %s\n[Peer]\nPublicKey = %s\nEndpoint = %s:%d\nAllowedIPs = 0.0.0.0/0\n" % (
              key, wg_pub(S["wg_server"]), ip, port))
        w("%s/%s.wg.addr" % (d, via), "%s.%d.%d\n" % (topo.WG_NET, HOSTS[host]["idx"], topo.wg_slot(via)))
        for proto in topo.WGIN_PROTOS:
            if (via, proto) in topo.wgin_combos():
                w("%s/%s.%s.wgin.conf" % (d, via, proto),
                  "[Interface]\nPrivateKey = %s\n[Peer]\nPublicKey = %s\nEndpoint = 127.0.0.1:%d\n"
                  "AllowedIPs = 0.0.0.0/0\n" % (wg_client_key(host, "wgin|%s|%s" % (via, proto)),
                                                wg_pub(S["wg_server"]), topo.cport(via, proto, "fwd3")))
                w("%s/%s.%s.wgin.addr" % (d, via, proto),
                  "%s.%d.%d\n" % (topo.WG_NET, HOSTS[host]["idx"], topo.wgin_slot(via, proto)))
    print("cli ok", host, len(vias))


if __name__ == "__main__":
    role = sys.argv[1]
    if role == "land":
        gen_land(sys.argv[2])
    elif role == "relay":
        gen_relay(sys.argv[2], json.loads(sys.argv[3]) if len(sys.argv) > 3 else {})
    elif role == "cli":
        gen_cli(sys.argv[2], sys.argv[3:] or topo.default_vias(sys.argv[2]))
