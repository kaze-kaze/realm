#!/usr/bin/env python3
"""Loopback versions of two lab scenarios, without network namespaces.
  ex.py cpu  FOT name=realm[,extra toml line;...] ...   relay on one core, saturated with tcp + udp
  ex.py herd FOT name=realm[,extra toml line;...] ...   5000 held connections lose the primary at once
Backends are `fot srv` on 127.0.0.11 / 127.0.0.12. Ports 39500-39560."""
import json, os, resource, signal, subprocess, sys, time

MODE, FOT = sys.argv[1], sys.argv[2]
VARIANTS = [v.split("=", 1) for v in sys.argv[3:]]
SECS = int(os.environ.get("EX_SECS", "20"))
W = os.environ.get("EX_OUT", ".")
A, B, R = "127.0.0.11", "127.0.0.12", "127.0.0.1:39500"
DEVNULL = subprocess.DEVNULL


def nofile():
    resource.setrlimit(resource.RLIMIT_NOFILE, (200000, 200000))


def spawn(cmd, **kw):
    return subprocess.Popen(cmd, shell=isinstance(cmd, str), preexec_fn=nofile, **kw)


def backend(ip, tag, ctl):
    p = spawn([FOT, "srv", f"tag={tag}", f"tcp={ip}:39511", f"udp={ip}:39511", f"health={ip}:39512",
               f"hudp={ip}:39512", f"ctl=127.0.0.1:{ctl}", "threads=2"], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    for _ in range(100):
        line = p.stdout.readline()
        if "READY" in line:
            return p
    raise RuntimeError("backend did not start")


def conf(extra, endpoints, fast=True, tcp_probe=True):
    fo = f"""[endpoints.failover]
interval = {0.5 if fast else 3}
timeout = {0.3 if fast else 1.5}
fall = 3
window = 5
rise = 5
hold = 5
udp_send = "hc:"
""" + ('tcp_send = "ping"\ntcp_expect = "OK"\n' if tcp_probe else "")
    s = '[log]\nlevel = "' + os.environ.get('EX_LOG', 'warn') + '"\noutput = "stdout"\n[network]\nuse_udp = true\ntcp_timeout = 4\nudp_timeout = 30\n' + extra + "\n"
    for i in range(endpoints):
        s += f'[[endpoints]]\nlisten = "127.0.0.1:{39500 if i == 0 else 39530 + i}"\nremote = "{A}:39511"\nextra_remotes = ["{B}:39511"]\n' + fo
    return s


def cpu_ticks(pid):
    f = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
    return (int(f[11]) + int(f[12])) / os.sysconf("SC_CLK_TCK")


def last_json(out):
    return json.loads([x for x in out.split("\n") if x.startswith("{")][-1])


def relay(binary, text, name, prefix=()):
    path = os.path.join(W, f"{name}.toml")
    open(path, "w").write(text)
    log = open(os.path.join(W, f"{name}.log"), "w")
    p = spawn(list(prefix) + [binary, "-c", path], stdout=log, stderr=subprocess.STDOUT)
    time.sleep(0.6)
    assert p.poll() is None, "relay exited"
    return p


def events(name):
    return [x for x in open(os.path.join(W, f"{name}.log")) if "[failover]" in x]


def cpu(name, binary, extra):
    out = {}
    for label, fast in (("fast", True),):
        a, b = backend(A, "A", 39513), backend(B, "B", 39523)
        r = relay(binary, conf(extra, 21, fast), f"{name}-{label}", ("taskset", "-c", "1"))
        time.sleep(4)
        t0 = cpu_ticks(r.pid)
        gen = ["taskset", "-c", "2-3"]
        tcp = spawn(gen + [FOT, "churn", f"target={R}", "conns=128", f"secs={SECS}", "timeout=8000", "threads=4"], stdout=subprocess.PIPE, text=True)
        udp = spawn(gen + [FOT, "udp", f"target={R}", f"socks={os.environ.get('EX_SOCKS', '64')}", f"pps={os.environ.get('EX_PPS', '150000')}", f"secs={SECS}", "threads=4"], stdout=subprocess.PIPE, text=True)
        tj, uj = last_json(tcp.communicate()[0]), last_json(udp.communicate()[0])
        c = (cpu_ticks(r.pid) - t0) / SECS * 100
        ev = events(f"{name}-{label}")
        out[label] = dict(cpu_pct=round(c), tcp_rate=round(tj["rate"]), tcp_err=tj["total_err"], udp_ok=uj["total_ok"], udp_sent=uj["sent"], events=len(ev), alive=r.poll() is None)
        for p in (r, a, b):
            p.kill(); p.wait()
        time.sleep(0.5)
    print(name, json.dumps(out), flush=True)


def herd(name, binary, extra):
    a, b = backend(A, "A", 39513), backend(B, "B", 39523)
    r = relay(binary, conf(extra, 1, True, tcp_probe=False), name)
    hold = spawn([FOT, "hold", f"target={R}", "conns=5000", "secs=26", "interval=1000", "ramp=4000", "timeout=10000", "reconnect=1", "threads=6"], stdout=subprocess.PIPE, text=True)
    time.sleep(12)
    a.kill(); a.wait()
    j = last_json(hold.communicate()[0])
    keep = {k: v for k, v in j.items() if not isinstance(v, (list, dict)) or k in ("err", "errors", "ok")}
    print(name, json.dumps(keep)[:900], "| relay alive", r.poll() is None, flush=True)
    for p in (r, b):
        p.kill(); p.wait()
    time.sleep(0.5)


for name, spec in VARIANTS:
    binary, _, extra = spec.partition(",")
    (cpu if MODE == "cpu" else herd)(name, binary, extra.replace(";", "\n"))
