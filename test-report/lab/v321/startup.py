#!/usr/bin/env python3
"""Acceptance of package S on the real binary: startup, supervision, config checks. Loopback only."""
import json, os, socket, subprocess, sys, time
REALM, W = sys.argv[1], sys.argv[2]
os.makedirs(W, exist_ok=True)
ok = True
def check(name, cond, detail=""):
    global ok
    ok &= bool(cond)
    print(("PASS " if cond else "FAIL ") + name + (("  | " + detail) if detail else ""), flush=True)

def run(args, env=None, wait=1.2):
    p = subprocess.Popen([REALM] + args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=dict(os.environ, **(env or {})))
    try:
        out, err = p.communicate(timeout=wait); return p.returncode, out, err
    except subprocess.TimeoutExpired:
        p.terminate(); out, err = p.communicate(timeout=5); return None, out, err      # None: was still running

def conf(name, obj, fmt="json"):
    path = os.path.join(W, f"{name}.{fmt}")
    open(path, "w").write(json.dumps(obj) if fmt == "json" else obj); return path

BACK = "127.0.0.1:39459"
occupied = socket.socket(); occupied.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1); occupied.bind(("127.0.0.1", 39441)); occupied.listen(1)

# S1: one of two rules cannot bind
rc, out, err = run(["-c", conf("two", {"endpoints": [{"listen": "127.0.0.1:39440", "remote": BACK}, {"listen": "127.0.0.1:39441", "remote": BACK}]})])
check("two rules, one tcp port taken: exits non-zero", rc not in (None, 0), f"rc={rc} stderr={err.strip()[:120]!r}")
check("  names the failing listener", "39441" in err and "tcp" in err.lower())
check("  the healthy rule is not left listening", socket.socket().connect_ex(("127.0.0.1", 39440)) != 0)
rc, out, err = run(["-l", "127.0.0.1:39441", "-r", BACK])
check("only rule cannot bind: exits non-zero", rc not in (None, 0), f"rc={rc}")
# all fine: keeps running, same banner
rc, out, err = run(["-c", conf("fine", {"network": {"use_udp": True}, "endpoints": [{"listen": "127.0.0.1:39442", "remote": BACK}]})])
check("valid config keeps running and prints inited", rc is None and "inited: 127.0.0.1:39442" in out, f"rc={rc} err={err.strip()[:100]!r}")
# S2: second instance on the same udp port
first = subprocess.Popen([REALM, "-u", "-t", "-l", "127.0.0.1:39443", "-r", BACK], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL); time.sleep(0.6)
rc, out, err = run(["-u", "-t", "-l", "127.0.0.1:39443", "-r", BACK])
check("second instance on the same udp port: exits non-zero", rc not in (None, 0), f"rc={rc} stderr={err.strip()[:100]!r}")
check("  first instance still running", first.poll() is None); first.terminate(); first.wait()
# S3: config validation
rc, out, err = run(["-c", conf("typo", {"network": {"use_udpp": True}, "endpoints": [{"listen": "127.0.0.1:39444", "remote": BACK}]})])
check("unknown key (json): warns and runs", rc is None and "use_udpp" in err, f"rc={rc} stderr={err.strip()[:140]!r}")
rc, out, err = run(["-c", conf("typo", '[network]\nuse_udpp = true\n[[endpoints]]\nlisten = "127.0.0.1:39444"\nremote = "%s"\nfoo = 1\n' % BACK, "toml")])
check("unknown keys (toml, network and endpoint): warns and runs", rc is None and "use_udpp" in err and "foo" in err, f"rc={rc} stderr={err.strip()[:200]!r}")
rc, out, err = run([], env={"REALM_CONF": json.dumps({"network": {"use_udpp": True}, "endpoints": [{"listen": "127.0.0.1:39444", "remote": BACK}]})})
check("unknown key (REALM_CONF): warns and runs", rc is None and "use_udpp" in err, f"rc={rc}")
rc, out, err = run(["-c", conf("legacy", '[network]\nzero_copy = true\nfast_open = false\n[[endpoints]]\nlisten = "127.0.0.1:39444"\nremote = "%s"\n' % BACK, "toml")])
check("deprecated keys zero_copy / fast_open: still starts", rc is None, f"rc={rc} stderr={err.strip()[:160]!r}")
rc, out, err = run(["-c", conf("through", {"endpoints": [{"listen": "127.0.0.1:39445", "remote": BACK, "through": "10.300.1.1"}]})])
check("invalid through: exits non-zero", rc not in (None, 0) and "through" in err, f"rc={rc} stderr={err.strip()[:140]!r}")
rc, out, err = run(["-c", conf("weights", {"endpoints": [{"listen": "127.0.0.1:39446", "remote": BACK, "extra_remotes": [BACK], "balance": "roundrobin: 1, 1, 1"}]})])
check("more weights than peers: exits non-zero", rc not in (None, 0), f"rc={rc} stderr={err.strip()[:140]!r}")
rc, out, err = run(["-c", conf("weights2", {"endpoints": [{"listen": "127.0.0.1:39446", "remote": BACK, "extra_remotes": [BACK], "balance": "roundrobin: 1, 1, "}]})])
check("matching weights with trailing comma: runs", rc is None, f"rc={rc} stderr={err.strip()[:140]!r}")
rc, out, err = run(["-c", conf("nolisten", {"endpoints": [{"listen": "127.0.0.1:39447", "remote": BACK, "network": {"no_tcp": True}}]})])
check("no rule listens on anything: exits non-zero", rc not in (None, 0), f"rc={rc} stderr={err.strip()[:140]!r}")
rc, out, err = run(["-c", conf("ppv", {"network": {"send_proxy": True, "send_proxy_version": 3}, "endpoints": [{"listen": "127.0.0.1:39448", "remote": BACK}]})])
check("send_proxy_version = 3: exits non-zero", rc not in (None, 0), f"rc={rc} stderr={err.strip()[:140]!r}")
rc, out, err = run(["-l", "127.0.0.1:39449", "-r", BACK, "--tcp-timeout", "5s"])
check("--tcp-timeout 5s: exits non-zero", rc not in (None, 0), f"rc={rc} stderr={err.strip()[:140]!r}")
# S4: REALM_CONF path raises nofile
import resource
soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
def low():
    resource.setrlimit(resource.RLIMIT_NOFILE, (1024, hard))
p = subprocess.Popen([REALM], env=dict(os.environ, REALM_CONF=json.dumps({"endpoints": [{"listen": "127.0.0.1:39450", "remote": BACK}]})), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, preexec_fn=low)
time.sleep(0.8)
lim = [l for l in open(f"/proc/{p.pid}/limits") if "open files" in l][0].split()
p.terminate(); p.communicate()
check("REALM_CONF path raises the soft nofile limit", lim[3] == lim[4] and int(lim[3]) > 1024, f"soft={lim[3]} hard={lim[4]}")
print("ALL PASS" if ok else "SOME FAILED")
