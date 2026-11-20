"""Hysteria 2 throughput through each realm build on the same relay host: verified bulk transfer both ways."""
import asyncio, subprocess, sys, time
sys.path.insert(0, '/rbt/lab')
import gen, wl
S = gen.S
PATHS = {'new-a': 26020, 'og-a': 26021, 'new-b': 26022, 'og-b': 26023, 'new-c': 26024, 'og-c': 26025}
procs, port = [], {}
for i, (n, p) in enumerate(PATHS.items()):
    lp = 15200 + i
    port[n] = lp
    y = ("server: 198.51.100.13:%d\nauth: %s\ntls:\n  sni: rbt.test\n  ca: /rbt/lab/pki/ca.crt\n"
         "tcpForwarding:\n  - listen: 127.0.0.1:%d\n    remote: 127.0.0.1:31060\n") % (p, S['hy_pw'], lp)
    f = '/tmp/hyab5-%s.yaml' % n
    open(f, 'w').write(y)
    procs.append(subprocess.Popen(['/rbt/bin/hysteria', 'client', '-c', f, '--disable-update-check'], stdout=open('/rbt/run/res/hyab5-%s.log' % n, 'w'), stderr=subprocess.STDOUT))
time.sleep(4)
async def one(lp):
    d = await asyncio.wait_for(wl.t_dl('fwd:%d' % lp, 80 << 20, stall=8), 40)
    u = await asyncio.wait_for(wl.t_ul('fwd:%d' % lp, 60 << 20, stall=8), 40)
    return d, u
names = list(PATHS)
res = {n: {'down': [], 'up': [], 'bad': 0} for n in names}
try:
    for r in range(int(sys.argv[1])):
        order = names[r % len(names):] + names[:r % len(names)]
        for n in order:
            try:
                d, u = asyncio.run(one(port[n]))
            except Exception as e:
                res[n]['bad'] += 1
                print(r, n, 'FAILED', type(e).__name__, flush=True)
                continue
            res[n]['down'].append(d['mbps'] if d['ok'] else None); res[n]['up'].append(u['mbps'] if u['ok'] else None)
            if d['bad_off'] is not None: res[n]['bad'] += 100
            print(r, n, 'down %s %.0f Mbit/s' % ('ok' if d['ok'] else 'FAIL:%s' % d['err'], d['mbps']), '| up %s %.0f' % ('ok' if u['ok'] else 'FAIL:%s' % u['err'], u['mbps']), flush=True)
finally:
    for p in procs: p.kill()
def summ(v):
    ok = sorted(x for x in v if x is not None)
    return 'median %.0f (min %.0f max %.0f) ok %d/%d' % (ok[len(ok) // 2], ok[0], ok[-1], len(ok), len(v)) if ok else 'all failed'
for n in names:
    print('TOTAL %-12s down %s | up %s | errors %d' % (n, summ(res[n]['down']), summ(res[n]['up']), res[n]['bad']))
