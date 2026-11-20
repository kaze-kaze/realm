"""Control: Hysteria 2 straight to the chi landing, no relay in between. Same transfers as hyab4."""
import asyncio, subprocess, sys, time
sys.path.insert(0, '/rbt/lab')
import gen, wl
S = gen.S
names = ['direct-a', 'direct-b', 'direct-c']
procs, port = [], {}
for i, n in enumerate(names):
    lp = 15200 + i
    port[n] = lp
    y = ("server: 198.51.100.11:31020\nauth: %s\ntls:\n  sni: rbt.test\n  ca: /rbt/lab/pki/ca.crt\n"
         "tcpForwarding:\n  - listen: 127.0.0.1:%d\n    remote: 127.0.0.1:31060\n") % (S['hy_pw'], lp)
    f = '/tmp/hyd-%s.yaml' % n
    open(f, 'w').write(y)
    procs.append(subprocess.Popen(['/rbt/bin/hysteria', 'client', '-c', f, '--disable-update-check'], stdout=open('/rbt/run/res/hyd-%s.log' % n, 'w'), stderr=subprocess.STDOUT))
time.sleep(4)
async def one(lp):
    d = await asyncio.wait_for(wl.t_dl('fwd:%d' % lp, 80 << 20, stall=8), 40)
    u = await asyncio.wait_for(wl.t_ul('fwd:%d' % lp, 60 << 20, stall=8), 40)
    return d, u
res = {n: {'down': [], 'up': [], 'bad': 0} for n in names}
try:
    for r in range(int(sys.argv[1])):
        for n in names[r % 3:] + names[:r % 3]:
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
