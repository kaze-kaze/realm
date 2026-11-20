import time
from lab import *
net_up()
A = backend("a", "A"); B = backend("b", "B")
print("direct to backend:", tool("c", f"abuse target={ip4('a')}:5000 conns=200 secs=3 kind=rst"))
for name, binary, conf in (("orig", ORIG, f'[[endpoints]]\nlisten = "0.0.0.0:6000"\nremote = "{ip4("a")}:5000"\n'),
                           ("new+failover", NEW, fo_conf(interval=0.5, timeout=0.3, hold=5))):
    r = realm(conf, binary=binary, level="warn", name="rst")
    time.sleep(1)
    out = tool("c", f"abuse target={ip4('r')}:6000 conns=200 secs=3 kind=rst")
    warn = len(r.grep("WARN")); err = len(r.grep("ERROR"))
    sample = [x for _, x in r.lines if "WARN" in x or "ERROR" in x][:3]
    print(name, out, "warn", warn, "error", err, sample, "syn drops:", nsx("r", "nstat -az TcpExtListenDrops TcpExtListenOverflows | tail -2").replace("\n", " | "))
    r.kill()
net_down()
