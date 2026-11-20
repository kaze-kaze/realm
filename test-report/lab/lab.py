"""Test lab: network namespaces, fault injection, process and result helpers."""
import json
import os
import signal
import subprocess
import threading
import time

BIN = "/work/bin"
OUT = os.environ.get("LAB_OUT", "/work/out")
FOT = f"{BIN}/fot"
NEW = os.environ.get("REALM_NEW", f"{BIN}/realm-new")
ORIG = f"{BIN}/realm-orig"

# name -> last octet
HOSTS = {"c": 1, "r": 2, "a": 11, "b": 12, "x": 13}
NET4 = "10.9.0."
NET6 = "fd00:9::"


def ip4(ns):
    return f"{NET4}{HOSTS[ns]}"


def ip6(ns):
    return f"{NET6}{HOSTS[ns]}"


def sh(cmd, check=True, quiet=False):
    r = subprocess.run(cmd, shell=True, capture_output=True, text=True)
    if check and r.returncode != 0:
        raise RuntimeError(f"cmd failed: {cmd}\n{r.stdout}\n{r.stderr}")
    return r.stdout.strip()


def nsx(ns, cmd, check=True):
    return sh(f"ip netns exec {ns} {cmd}", check)


def net_down():
    sh("pkill -9 -x fot; pkill -9 -f realm-; pkill -9 iperf3", check=False)
    time.sleep(0.2)
    for ns in sh("ip netns list", check=False).split("\n"):
        ns = ns.split(" ")[0]
        if ns:
            sh(f"ip netns del {ns}", check=False)
    for dev in sh("ip -o link show | grep -o 'v-[a-z0-9]*' | sort -u", check=False).split():
        sh(f"ip link del {dev}", check=False)
    sh("ip link del br0", check=False)
    time.sleep(0.2)


def net_up(extra_hosts=None):
    """One bridge, one namespace per host. Every namespace gets ipv4 + ipv6."""
    net_down()
    if extra_hosts:
        HOSTS.update(extra_hosts)
    sh("ip link add br0 type bridge && ip link set br0 up")
    sh(f"ip addr add {NET4}254/24 dev br0")
    sh(f"ip -6 addr add {NET6}254/64 dev br0 nodad", check=False)
    sh("sysctl -qw net.bridge.bridge-nf-call-iptables=0", check=False)
    sh("sysctl -qw net.bridge.bridge-nf-call-ip6tables=0", check=False)
    for ns, last in HOSTS.items():
        sh(f"ip netns add {ns}")
        sh(f"ip link add v-{ns} type veth peer name eth0 netns {ns}")
        sh(f"ip link set v-{ns} master br0 up")
        nsx(ns, "ip link set lo up")
        nsx(ns, f"ip addr add {NET4}{last}/24 dev eth0")
        nsx(ns, f"ip -6 addr add {NET6}{last}/64 dev eth0 nodad")
        nsx(ns, "ip link set eth0 up")
        for k, v in {
            "net.ipv4.ip_local_port_range": "1024 65535",
            "net.ipv4.tcp_tw_reuse": "1",
            "net.core.somaxconn": "8192",
            "net.ipv4.tcp_max_syn_backlog": "8192",
            "net.ipv4.tcp_syncookies": "1",
        }.items():
            nsx(ns, f"sysctl -qw {k}='{v}'", check=False)
    time.sleep(0.3)


# ------------------------------------------------------------------ faults


def netem(ns, spec):
    """Impair both directions of a host link. spec e.g. 'loss 10%' or 'delay 200ms 50ms'."""
    nsx(ns, f"tc qdisc replace dev eth0 root netem {spec}")
    sh(f"tc qdisc replace dev v-{ns} root netem {spec}")


def netem_clear(ns):
    nsx(ns, "tc qdisc del dev eth0 root", check=False)
    sh(f"tc qdisc del dev v-{ns} root", check=False)


def blackhole(ns, on=True, proto=None, port=None):
    """Silently drop inbound traffic (host unreachable / firewall)."""
    m = ""
    if proto:
        m += f" -p {proto}"
    if port:
        m += f" --dport {port}"
    op = "-I" if on else "-D"
    nsx(ns, f"iptables {op} INPUT -i eth0{m} -j DROP", check=on)
    nsx(ns, f"ip6tables {op} INPUT -i eth0{m} -j DROP", check=False)


def link(ns, up):
    sh(f"ip link set v-{ns} {'up' if up else 'down'}")


# ------------------------------------------------------------------ processes


class Proc:
    def __init__(self, ns, argv, name, env=None, prefix=""):
        self.name = name
        self.lines = []  # (wall time, text)
        self.ns = ns
        cmd = f"{prefix} ip netns exec {ns} {argv}" if ns else f"{prefix} {argv}"
        self.p = subprocess.Popen(
            cmd,
            shell=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            preexec_fn=os.setsid,
            env=env,
        )
        self.t = threading.Thread(target=self._pump, daemon=True)
        self.t.start()

    def _pump(self):
        for line in self.p.stdout:
            self.lines.append((time.time(), line.rstrip("\n")))

    def pid(self):
        """Pid of the real program (child of the shell / ip netns exec)."""
        for _ in range(50):
            out = sh(f"pgrep -g {os.getpgid(self.p.pid)} -x '{self.name}'", check=False)
            if out:
                return int(out.split("\n")[0])
            time.sleep(0.02)
        return None

    def alive(self):
        return self.p.poll() is None

    def sig(self, s):
        try:
            os.killpg(os.getpgid(self.p.pid), s)
        except ProcessLookupError:
            pass

    def kill(self):
        self.sig(signal.SIGKILL)
        try:
            self.p.wait(timeout=3)
        except Exception:
            pass

    def wait(self, timeout=None):
        self.p.wait(timeout=timeout)
        self.t.join(timeout=2)
        return "\n".join(x for _, x in self.lines)

    def grep(self, *subs, since=0.0):
        return [(t, x) for t, x in self.lines if t >= since and all(s in x for s in subs)]

    def text(self):
        return "\n".join(x for _, x in self.lines)


def backend(ns, tag, port=5000, health=5001, v6=False):
    host = "[::]" if v6 else "0.0.0.0"
    p = Proc(
        ns,
        f"{FOT} srv tag={tag} tcp={host}:{port} udp={host}:{port} health={host}:{health} hudp={host}:{health} ctl=127.0.0.1:5999 threads=2",
        "fot",
    )
    for _ in range(100):
        if p.grep("READY"):
            return p
        time.sleep(0.02)
    raise RuntimeError(f"backend {tag} did not start: {p.text()}")


def ctl(ns, *cmds):
    return nsx(ns, f"{FOT} ctl 127.0.0.1:5999 {' '.join(cmds)}")


def stats(ns):
    return json.loads(ctl(ns, "stats"))


def realm(conf, binary=NEW, ns="r", level="warn", name=None, prefix="", args=""):
    """Start realm in the relay namespace with the given toml text."""
    os.makedirs(OUT, exist_ok=True)
    path = f"{OUT}/realm-{name or int(time.time() * 1000)}.toml"
    with open(path, "w") as f:
        f.write(f'[log]\nlevel = "{level}"\noutput = "stdout"\n' + conf)
    p = Proc(ns, f"{binary} -c {path} {args}", os.path.basename(binary), prefix=prefix)
    time.sleep(0.4)
    if not p.alive():
        raise RuntimeError(f"realm exited: {p.text()}")
    return p


def tool(ns, args, bg=False, prefix=""):
    """Run a fot client. Returns parsed json (or the Proc when bg)."""
    p = Proc(ns, f"{FOT} {args}", "fot", prefix=prefix)
    if bg:
        return p
    return done(p)


def done(p, timeout=None):
    out = p.wait(timeout)
    last = [x for x in out.split("\n") if x.startswith("{")]
    if not last:
        raise RuntimeError(f"tool gave no json: {out[-2000:]}")
    return json.loads(last[-1])


def rss_kb(pid):
    try:
        for line in open(f"/proc/{pid}/status"):
            if line.startswith("VmRSS"):
                return int(line.split()[1])
    except OSError:
        return None


def nfd(pid):
    try:
        return len(os.listdir(f"/proc/{pid}/fd"))
    except OSError:
        return None


def nthreads(pid):
    try:
        return len(os.listdir(f"/proc/{pid}/task"))
    except OSError:
        return None


def cpu_ticks(pid):
    try:
        f = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
        return int(f[11]) + int(f[12])  # utime + stime
    except OSError:
        return None


# ------------------------------------------------------------------ timeline helpers

TAGS = ["A", "B", "C", "D", "other"]
ERRS = ["refused", "conn_timeout", "read_timeout", "reset", "eof", "mismatch", "conn_other", "io_other"]


def rel_ms(res, wall):
    """Wall clock time -> ms relative to the tool start."""
    return wall * 1000 - res["epoch_ms"]


def first_ok(res, tag, after_ms=0):
    i = TAGS.index(tag)
    for t, _sent, ok, _err, _lat in res["buckets"]:
        if t + res["bucket_ms"] > after_ms and ok[i] > 0:
            return t
    return None


def last_ok(res, tag, after_ms=0):
    i = TAGS.index(tag)
    last = None
    for t, _sent, ok, _err, _lat in res["buckets"]:
        if t >= after_ms and ok[i] > 0:
            last = t
    return last


def count(res, t0=0, t1=1e18):
    """Sum ok per tag and errors per kind within [t0, t1) ms."""
    ok = dict.fromkeys(TAGS, 0)
    err = dict.fromkeys(ERRS, 0)
    sent = 0
    latmax = 0
    for t, s, o, e, lat in res["buckets"]:
        if t0 <= t < t1:
            sent += s
            latmax = max(latmax, lat)
            for k, v in zip(TAGS, o):
                ok[k] += v
            for k, v in zip(ERRS, e):
                err[k] += v
    return {
        "sent": sent,
        "ok": {k: v for k, v in ok.items() if v},
        "err": {k: v for k, v in err.items() if v},
        "lat_max_ms": round(latmax / 1000, 1),
    }


def events(p, since=0.0):
    """Failover log events of a realm process: (wall, proto, kind, text)."""
    out = []
    for t, x in p.lines:
        if t < since or "[failover]" not in x:
            continue
        proto = "tcp" if "[failover][tcp]" in x else "udp"
        if " switch " in x:
            kind = "switch"
        elif " is down" in x:
            kind = "down"
        elif " is up" in x:
            kind = "up"
        else:
            continue
        out.append((t, proto, kind, x.split("]", 5)[-1] if x.count("]") >= 5 else x))
    return out


# ------------------------------------------------------------------ results

RESULTS = []


def record(sid, title, status, expect, observed, detail=None):
    """status: PASS / FAIL / INFO"""
    r = {
        "id": sid,
        "title": title,
        "status": status,
        "expect": expect,
        "observed": observed,
        "detail": detail or {},
    }
    RESULTS.append(r)
    print(f"[{status}] {sid} {title}\n    expect:   {expect}\n    observed: {observed}", flush=True)
    return r


def save(name):
    os.makedirs(OUT, exist_ok=True)
    with open(f"{OUT}/{name}.json", "w") as f:
        json.dump(RESULTS, f, indent=1, ensure_ascii=False)


def fo_conf(
    listen="0.0.0.0:6000",
    peers=("a", "b"),
    port=5000,
    probe_port=None,
    interval=3,
    timeout=1.5,
    fall=3,
    window=5,
    rise=5,
    hold=30,
    tcp_timeout=4,
    udp=True,
    tcp_send=None,
    tcp_expect=None,
    udp_send=None,
    udp_timeout=30,
    v6=False,
    extra="",
):
    addr = (lambda ns, p: f"[{ip6(ns)}]:{p}") if v6 else (lambda ns, p: f"{ip4(ns)}:{p}")
    s = f"""
[network]
use_udp = {str(udp).lower()}
tcp_timeout = {tcp_timeout}
udp_timeout = {udp_timeout}

[[endpoints]]
listen = "{listen}"
remote = "{addr(peers[0], port)}"
extra_remotes = [{", ".join('"' + addr(p, port) + '"' for p in peers[1:])}]
{extra}
[endpoints.failover]
interval = {interval}
timeout = {timeout}
fall = {fall}
window = {window}
rise = {rise}
hold = {hold}
"""
    if probe_port:
        s += f"probe = [{', '.join(chr(34) + addr(p, probe_port) + chr(34) for p in peers)}]\n"
    if tcp_send is not None:
        s += f'tcp_send = "{tcp_send}"\n'
    if tcp_expect is not None:
        s += f'tcp_expect = "{tcp_expect}"\n'
    if udp_send is not None:
        s += f'udp_send = "{udp_send}"\n'
    return s
