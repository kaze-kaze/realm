#!/usr/bin/env python3
"""Client-side services inside rbt-cli.  cli.py up <via> [xray|hy2|hy2-obfs|wg ...] | down [via] | ls"""
import os
import signal
import subprocess
import sys
import time

import gen
import topo

RUN, BIN, LAB = topo.RUN, topo.BIN, topo.LAB
ME = os.environ.get("RBT_ME") or open("/rbt/me").read().strip()
PID = RUN + "/pid"
KINDS = ["xray", "hy2", "hy2-obfs", "wg"]


def wgdev(via):
    return "w%d" % topo.via_index(via)


def _spawn(name, argv, env=None):
    os.makedirs(PID, exist_ok=True)
    os.makedirs(RUN + "/log", exist_ok=True)
    log = open("%s/log/cli-%s.log" % (RUN, name), "ab")
    e = dict(os.environ)
    e.update(env or {})
    p = subprocess.Popen(argv, stdout=log, stderr=log, stdin=subprocess.DEVNULL, start_new_session=True, env=e)
    open("%s/cli-%s.pid" % (PID, name), "w").write(str(p.pid))


def _kill(name):
    f = "%s/cli-%s.pid" % (PID, name)
    try:
        os.kill(int(open(f).read()), signal.SIGKILL)
    except (OSError, ValueError):
        pass
    try:
        os.remove(f)
    except OSError:
        pass


def alive(name):
    try:
        os.kill(int(open("%s/cli-%s.pid" % (PID, name)).read()), 0)
        return True
    except (OSError, ValueError):
        return False


def up(via, kinds=None, wg_keepalive=None, wg_mtu=1420):
    kinds = kinds or KINDS
    gen.gen_cli(ME, [via])
    for k in kinds:
        name = "%s-%s" % (via, k)
        if k == "wg":
            dev = wgdev(via)
            idx = topo.via_index(via)
            addr = open("%s/cli/%s.wg.addr" % (RUN, via)).read().strip()
            subprocess.run(["ip", "link", "del", dev], capture_output=True)
            cmds = [["ip", "link", "add", dev, "type", "wireguard"],
                    ["wg", "setconf", dev, "%s/cli/%s.wg.conf" % (RUN, via)],
                    ["ip", "addr", "add", addr + "/32", "dev", dev],
                    ["ip", "link", "set", dev, "mtu", str(wg_mtu), "up"],
                    ["ip", "route", "add", "10.99.0.1/32", "dev", dev, "metric", str(100 + idx)],
                    ["ip", "route", "add", "default", "dev", dev, "metric", str(1000 + idx)],
                    ["sysctl", "-qw", "net.ipv4.conf.%s.rp_filter=0" % dev, "net.ipv4.conf.all.rp_filter=0"]]
            if wg_keepalive:
                pub = [l.split("=", 1)[1].strip() for l in open("%s/cli/%s.wg.conf" % (RUN, via)) if l.startswith("PublicKey")][0]
                cmds.insert(2, ["wg", "set", dev, "peer", pub, "persistent-keepalive", str(wg_keepalive)])
            for c in cmds:
                r = subprocess.run(c, capture_output=True, text=True)
                if r.returncode:
                    print("wg setup failed:", " ".join(c), r.stderr.strip())
                    break
            continue
        if alive(name):
            continue
        if k == "xray":
            _spawn(name, [BIN + "/xray", "run", "-c", "%s/cli/%s.xray.json" % (RUN, via)],
                   {"SSL_CERT_FILE": LAB + "/pki/ca.crt"})
        else:
            _spawn(name, [BIN + "/hysteria", "client", "-c", "%s/cli/%s.%s.yaml" % (RUN, via, k)])


def wgin_dev(via, proto):
    return "x%d" % (topo.wgin_slot(via, proto) - 100)


def wgin_up(via, proto, mtu=1420):
    """A wireguard tunnel whose packets travel inside `proto` (through `via`) to the landing."""
    up(via, ["xray"])
    dev = wgin_dev(via, proto)
    base = "%s/cli/%s.%s.wgin" % (RUN, via, proto)
    addr = open(base + ".addr").read().strip()
    subprocess.run(["ip", "link", "del", dev], capture_output=True)
    for c in (["ip", "link", "add", dev, "type", "wireguard"], ["wg", "setconf", dev, base + ".conf"],
              ["ip", "addr", "add", addr + "/32", "dev", dev], ["ip", "link", "set", dev, "mtu", str(mtu), "up"],
              ["ip", "route", "add", "10.99.0.1/32", "dev", dev, "metric", str(500 + topo.wgin_slot(via, proto))],
              ["sysctl", "-qw", "net.ipv4.conf.%s.rp_filter=0" % dev, "net.ipv4.conf.all.rp_filter=0"]):
        r = subprocess.run(c, capture_output=True, text=True)
        if r.returncode:
            print("wgin setup failed:", " ".join(c), r.stderr.strip())
            break
    return dev


def down(via=None):
    for f in os.listdir(PID) if os.path.isdir(PID) else []:
        if f.startswith("cli-") and (via is None or f.startswith("cli-%s-" % via)):
            _kill(f[4:-4])
    for v in ([via] if via else topo.default_vias(ME)):
        subprocess.run(["ip", "link", "del", wgdev(v)], capture_output=True)
    for v, p in topo.wgin_combos():
        if via is None or v == via:
            subprocess.run(["ip", "link", "del", wgin_dev(v, p)], capture_output=True)


def ls():
    out = []
    for f in sorted(os.listdir(PID)) if os.path.isdir(PID) else []:
        if f.startswith("cli-"):
            out.append("%s %s" % (f[4:-4], "up" if alive(f[4:-4]) else "DEAD"))
    r = subprocess.run(["wg", "show", "interfaces"], capture_output=True, text=True)
    return out, r.stdout.split()


if __name__ == "__main__":
    c = sys.argv[1]
    if c == "up":
        up(sys.argv[2], sys.argv[3:] or None)
        time.sleep(1.5)
        print(ls())
    elif c == "down":
        down(sys.argv[2] if len(sys.argv) > 2 else None)
    elif c == "ls":
        print(ls())
