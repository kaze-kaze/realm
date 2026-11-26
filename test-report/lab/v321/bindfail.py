#!/usr/bin/env python3
"""No local port for new udp associations, with the bind itself failing. Loopback only.
While the marker file exists, bindfail.so makes bind(UDP, port 0) fail with EADDRINUSE inside the relay:
what the kernel returns when no ephemeral port is left. v3.2.0 never binds an association, so for it
(pass ) the shim instead makes sendmmsg on the unbound socket fail with EAGAIN, as noport.so does.
Usage: bindfail.py REALM bindfail.so OUTDIR [old]
The shim is built with: gcc -shared -fPIC -O2 -o bindfail.so shim/bindfail.c -ldl"""
import os, socket, statistics, subprocess, sys, threading, time
REALM, SHIM, W = sys.argv[1], os.path.abspath(sys.argv[2]), os.path.abspath(sys.argv[3])
which = 'old' if len(sys.argv) > 4 and sys.argv[4] == 'old' else 'new'
os.makedirs(W, exist_ok=True)

def udp(bind=None, family=socket.AF_INET, timeout=None, rcvbuf=None):
    s = socket.socket(family, socket.SOCK_DGRAM)
    if rcvbuf: s.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, rcvbuf)
    if bind: s.bind(bind)
    if timeout is not None: s.settimeout(timeout)
    return s

def start(tag, listen, remote, timeout, env):
    log = '%s/%s.realm.log' % (W, tag)
    if os.path.exists(log): os.unlink(log)
    cmd = [REALM, '-u', '--ntcp', '-l', listen, '-r', remote, '--log-level', 'debug', '--log-output', log,
           '--udp-timeout', str(timeout)]
    e = dict(os.environ); e.update(env)
    p = subprocess.Popen(cmd, env=e, stdout=open('%s/%s.stdout' % (W, tag), 'w'), stderr=subprocess.STDOUT)
    time.sleep(0.8)
    assert p.poll() is None, 'realm exited: ' + open('%s/%s.stdout' % (W, tag)).read()
    return p, log

def stop(p):
    p.terminate()
    try: p.wait(3)
    except Exception: p.kill(); p.wait(3)

def nfd(p): return len(os.listdir('/proc/%d/fd' % p.pid))
def nudp(p):
    n = 0
    for f in os.listdir('/proc/%d/fd' % p.pid):
        try:
            if os.readlink('/proc/%d/fd/%s' % (p.pid, f)).startswith('socket:'): n += 1
        except OSError: pass
    return n
def rss_kb(p):
    for l in open('/proc/%d/status' % p.pid):
        if l.startswith('VmRSS'): return int(l.split()[1])

class Echo(threading.Thread):
    """UDP echo server. prefix: bytes put in front of every reply. Records the source of what it gets."""
    def __init__(self, addr, family=socket.AF_INET, prefix=b'', rcvbuf=4 << 20):
        super().__init__(daemon=True)
        self.addr, self.family, self.prefix, self.rcvbuf = addr, family, prefix, rcvbuf
        self.sock = None; self.run_ = True; self.answer = True; self.seen = []; self.count = 0
        self.open()
    def open(self):
        self.sock = udp(self.addr, self.family, timeout=0.1, rcvbuf=self.rcvbuf)
    def close(self):
        s, self.sock = self.sock, None
        if s: s.close()
    def run(self):
        while self.run_:
            s = self.sock
            if s is None: time.sleep(0.02); continue
            try: d, src = s.recvfrom(70000)
            except OSError: continue
            self.count += 1
            if len(self.seen) < 100000: self.seen.append((time.time(), src, d[:32]))
            if self.answer:
                try: s.sendto(self.prefix + d, src)
                except OSError: pass

def loglines(log, *subs):
    out = []
    try:
        for l in open(log, errors='replace'):
            if any(s in l for s in subs): out.append(l.rstrip())
    except FileNotFoundError: pass
    return out

def levels(log):
    c = {}
    for l in open(log, errors='replace'):
        for lv in ('[ERROR]', '[WARN]', '[INFO]', '[DEBUG]', 'panicked'):
            if lv in l: c[lv] = c.get(lv, 0) + 1; break
    return c


LP, RP = 39510, 39511
POISON = W + '/bindfail-%s.poison' % which
STAT = W + '/bindfail-%s.stat' % which
if os.path.exists(POISON): os.unlink(POISON)
e = Echo(('127.0.0.1', RP)); e.start()
env = {'LD_PRELOAD': SHIM, 'POISON_FILE': POISON, 'SHIM_STAT': STAT}
if which == 'old': env['POISON_SEND'] = '1'
p, log = start('bindfail-' + which, '127.0.0.1:%d' % LP, '127.0.0.1:%d' % RP, 10, env)
T0 = time.time()
def ask(s, tag, wait=1.0):
    s.settimeout(wait); t = time.time(); s.sendto(tag, ('127.0.0.1', LP))
    try:
        while True:
            d, _ = s.recvfrom(2048)
            if d == tag: return 'ok %.1f ms' % ((time.time() - t) * 1e3)
    except OSError: return 'NO ANSWER'

a = udp(('127.0.0.1', 0)); print('client A before         :', ask(a, b'a0'))
fd_a = nfd(p)
# A pings every 20 ms for the whole test
res = []; run = True
def pinger():
    a.settimeout(0.5); n = 0
    while run:
        n += 1; tag = b'A%06d' % n; t = time.time(); a.sendto(tag, ('127.0.0.1', LP))
        try:
            while True:
                d, _ = a.recvfrom(2048)
                if d == tag: res.append((t - T0, (time.time() - t) * 1e3)); break
        except OSError: res.append((t - T0, None))
        time.sleep(max(0, 0.02 - (time.time() - t)))
th = threading.Thread(target=pinger); th.start()
time.sleep(1.0)
open(POISON, 'w').close(); t_on = time.time() - T0
b = udp(('127.0.0.1', 0)); bres = []; newres = []; t_end = time.time() + 6
while time.time() < t_end:
    bres.append(ask(b, b'b-during', 0.05))            # one new client keeps trying
    n = udp(('127.0.0.1', 0)); newres.append(ask(n, b'n-during', 0.05)); n.close()   # and a fresh one each round
fd_poison = nfd(p)
os.unlink(POISON); t_off = time.time() - T0
c = udp(('127.0.0.1', 0))
print('poison on for %.1f s: client B asked %d times, answered %d; %d fresh clients, answered %d; realm fds %d (with A: %d)' % (
    t_off - t_on, len(bres), sum(r != 'NO ANSWER' for r in bres), len(newres), sum(r != 'NO ANSWER' for r in newres), fd_poison, fd_a))
print('marker removed, new client C at once :', ask(c, b'c1'))
print('marker removed, client B again       :', ask(b, b'b2'))
time.sleep(1.5); run = False; th.join()
def seg(lo, hi):
    r = [x for x in res if lo <= x[0] < hi]; ok = [x[1] for x in r if x[1] is not None]
    return 'sent %d, lost %d, rtt ms median %.2f max %.2f' % (len(r), len(r) - len(ok), statistics.median(ok) if ok else -1, max(ok) if ok else -1)
print('client A, 20 ms pings: before poison [%s] | during [%s] | after [%s]' % (seg(0, t_on), seg(t_on, t_off), seg(t_off, 1e9)))
print('shim:', open(STAT).read().strip() if os.path.exists(STAT) else 'no refusals')
fa = loglines(log, 'failed to associate')
print('log "failed to associate" lines: %d' % len(fa))
for l in fa[:3] + fa[-2:]: print('   ', l[:200])
ent = [l for l in loglines(log, 'entry batched recvfrom')]
print('log levels:', levels(log), '| realm alive:', p.poll() is None)
# after udp_timeout everything is released
time.sleep(11.5); print('12 s later: realm fds %d (idle baseline %d, plus %d for the shim counter file opened inside realm)' % (nfd(p), fd_a - 1, os.path.exists(STAT)))
d = udp(('127.0.0.1', 0)); print('new client D            :', ask(d, b'd1'))
stop(p); e.run_ = False
