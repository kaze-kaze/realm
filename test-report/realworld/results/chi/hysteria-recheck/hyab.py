"""Hysteria 2 throughput through each realm build on the same relay host, order balanced."""
import subprocess, sys, time
sys.path.insert(0, '/rbt/lab')
import gen, wl, topo
S = gen.S
PATHS = {'new-busy': 22020, 'og-busy': 23020, 'v300-single': 26020, 'new-single': 26021, 'og-single': 26022}
procs, port = [], {}
for i, (n, p) in enumerate(PATHS.items()):
    lp = 15000 + i
    port[n] = lp
    y = ("server: 198.51.100.13:%d\nauth: %s\ntls:\n  sni: rbt.test\n  ca: /rbt/lab/pki/ca.crt\n"
         "tcpForwarding:\n  - listen: 127.0.0.1:%d\n    remote: 127.0.0.1:31051\n") % (p, S['hy_pw'], lp)
    f = '/tmp/hyab-%s.yaml' % n
    open(f, 'w').write(y)
    procs.append(subprocess.Popen(['/rbt/bin/hysteria', 'client', '-c', f, '--disable-update-check'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
time.sleep(4)
names = list(PATHS)
res = {n: {'down': [], 'up': []} for n in names}
try:
    for r in range(int(sys.argv[1])):
        order = names[r % len(names):] + names[:r % len(names)]
        for n in order:
            d = wl.iperf(port[n], reverse=True, secs=5, timeout=20)
            u = wl.iperf(port[n], secs=5, timeout=20)
            res[n]['down'].append(d.get('mbps')); res[n]['up'].append(u.get('mbps'))
            print(r, n, 'down', d.get('mbps', d.get('err')), 'up', u.get('mbps', u.get('err')), flush=True)
finally:
    for p in procs: p.kill()
def summ(v):
    ok = sorted(x for x in v if x is not None)
    return 'median %.0f (min %.0f max %.0f) ok %d/%d' % (ok[len(ok) // 2], ok[0], ok[-1], len(ok), len(v)) if ok else 'all failed'
for n in names:
    print('TOTAL %-12s down %s | up %s' % (n, summ(res[n]['down']), summ(res[n]['up'])))
