"""Replies of many udp associations through a listening socket whose send buffer is as
small as the kernel allows, so that sendmmsg on it returns EAGAIN on loopback.
Then: which clients still get answers, and are the associations released?

Usage: lostwake.py REALM SNDBUF_SHIM_SO OUTDIR [clients=12] [sndbuf=1|default] [seconds=3]
The shim is built with: gcc -shared -fPIC -O2 -o sndbuf.so shim/sndbuf.c -ldl"""
import os, socket, subprocess, sys, threading, time
REALM, SHIM, W = sys.argv[1], os.path.abspath(sys.argv[2]), os.path.abspath(sys.argv[3])
NCLI = int(sys.argv[4]) if len(sys.argv) > 4 else 12
SNDBUF = sys.argv[5] if len(sys.argv) > 5 else '1'
BLAST = float(sys.argv[6]) if len(sys.argv) > 6 else 3.0
LEVEL, LP, RP = 'info', 39160, 39161
os.makedirs(W + '/out', exist_ok=True)

def udp(bind=None, family=socket.AF_INET, timeout=None):
    s = socket.socket(family, socket.SOCK_DGRAM)
    if bind: s.bind(bind)
    if timeout is not None: s.settimeout(timeout)
    return s

def start(listen, remote, tag, timeout=10, extra=(), env=None, level='debug'):
    log = '%s/out/%s.realm.log' % (W, tag)
    if os.path.exists(log): os.unlink(log)
    cmd = ['nice', '-n', '10', REALM, '-u', '--ntcp', '-l', listen, '-r', remote, '--udp-timeout', str(timeout),
           '--log-level', level, '--log-output', log] + list(extra)
    e = dict(os.environ); e.update(env or {})
    p = subprocess.Popen(cmd, env=e, stdout=open('%s/out/%s.stdout' % (W, tag), 'w'), stderr=subprocess.STDOUT)
    time.sleep(0.7)
    assert p.poll() is None, 'realm exited'
    return p, log

def stop(p):
    p.terminate()
    try: p.wait(3)
    except Exception: p.kill()

def nfd(p):
    # realm runs under nice: p.pid is realm itself (nice execs)
    return len(os.listdir('/proc/%d/fd' % p.pid))

def rss_kb(p):
    for l in open('/proc/%d/status' % p.pid):
        if l.startswith('VmRSS'): return int(l.split()[1])

TMO = 8
tag = 'x5-n%d-sb%s-%s' % (NCLI, SNDBUF, LEVEL)
stat = '%s/out/%s.stat' % (W, tag)
env = {'LD_PRELOAD': SHIM, 'SHIM_PORT': str(LP), 'SHIM_STAT': stat}
if SNDBUF != 'default': env['SHIM_SNDBUF'] = SNDBUF

def snmp():
    ls = [l.split() for l in open('/proc/net/snmp') if l.startswith('Udp:')]
    return dict(zip(ls[0][1:], map(int, ls[1][1:])))

up = udp(('127.0.0.1', RP), timeout=0.2)
assoc = {}          # realm upstream addr -> client index
run = True
def upstream():
    while run:
        try: d, src = up.recvfrom(2000)
        except OSError: continue
        if d.startswith(b'hello'): assoc[src] = int(d[5:])
        elif d.startswith(b'ping'): up.sendto(b'pong' + d[4:], src)
threading.Thread(target=upstream, daemon=True).start()

p, log = start('127.0.0.1:%d' % LP, '127.0.0.1:%d' % RP, tag, timeout=TMO, env=env, level=LEVEL)
fd0 = nfd(p)
cl = [udp(('127.0.0.1', 0), timeout=0.05) for _ in range(NCLI)]
rxn = [0] * NCLI; pong = [set() for _ in range(NCLI)]
def drain(i):
    c = cl[i]
    while run:
        try: d = c.recv(2000)
        except OSError: continue
        if d.startswith(b'pong'): pong[i].add(d)
        else: rxn[i] += 1
for i in range(NCLI): threading.Thread(target=drain, args=(i,), daemon=True).start()
for i, c in enumerate(cl): c.sendto(b'hello%d' % i, ('127.0.0.1', LP))
time.sleep(0.5)
assert len(assoc) == NCLI, len(assoc)
fd1 = nfd(p)
s0 = snmp()

# reply traffic to every association at once
blast = udp(('127.0.0.1', 0)); blast.close()
payload = b'D' * 1200; dsts = list(assoc); sent = [0]
def blaster():
    t_end = time.time() + BLAST; n = 0
    while time.time() < t_end:
        for d in dsts:
            for _ in range(8): up.sendto(payload, d)
        n += 8 * len(dsts)
    sent[0] += n
bs = [threading.Thread(target=blaster) for _ in range(2)]
[b.start() for b in bs]; [b.join() for b in bs]
time.sleep(1.0)
s1 = snmp()
print('%s: %d clients, listener SO_SNDBUF=%s, upstream sent %d datagrams in %.0fs, clients received %d' % (tag, NCLI, SNDBUF, sent[0], BLAST, sum(rxn)))
print('   listener sendmmsg: %s | system UdpSndbufErrors +%d' % (open(stat).read().strip(), s1['SndbufErrors'] - s0['SndbufErrors']))

def ping(label):
    for i in range(NCLI): pong[i].clear()
    for k in range(3):
        for i, c in enumerate(cl): c.sendto(b'ping%d-%s-%d' % (i, label.encode(), k), ('127.0.0.1', LP))
        time.sleep(0.3)
    time.sleep(0.5)
    return [i for i in range(NCLI) if not pong[i]]
dead1 = ping('a')
print('   right after the burst : clients with NO answer to 3 pings: %s   realm fds %d (idle %d, with %d associations %d)' % (dead1, nfd(p), fd0, NCLI, fd1))
time.sleep(TMO + 3)
fd2 = nfd(p)
print('   %ds later (udp_timeout=%d, nothing sent meanwhile): realm fds %d -> %d association sockets never closed' % (TMO + 3, TMO, fd2, fd2 - fd0))
dead2 = ping('b')
print('   same clients, same source ports, ping again: NO answer: %s' % dead2)
time.sleep(TMO + 3)
dead3 = ping('c')
print('   and %ds after that: NO answer: %s   realm fds before this ping round %s' % (TMO + 3, dead3, fd2))
if LEVEL == 'debug':
    rm = sum(1 for l in open(log) if 'remove association' in l); new = sum(1 for l in open(log) if 'new association' in l)
    print('   log: new association %d, remove association %d' % (new, rm))
run = False; stop(p)
