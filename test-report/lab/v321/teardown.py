"""How long does a killed backend keep accepting connections, depending on what the
peers of its 5000 connections do when they see its FIN?  mode: close | keep"""
import resource, selectors, socket, subprocess, sys, threading, time
FOT, MODE = sys.argv[1], sys.argv[2]
resource.setrlimit(resource.RLIMIT_NOFILE, (200000, 200000))
A = ("127.0.0.11", 39511)
srv = subprocess.Popen([FOT, "srv", "tag=A", "tcp=127.0.0.11:39511", "udp=127.0.0.11:39511", "health=127.0.0.11:39512",
                        "hudp=127.0.0.11:39512", "ctl=127.0.0.1:39513", "threads=2"], stdout=subprocess.PIPE, text=True,
                       preexec_fn=lambda: resource.setrlimit(resource.RLIMIT_NOFILE, (200000, 200000)))
while "READY" not in srv.stdout.readline(): pass
sel = selectors.DefaultSelector()
conns = []
for _ in range(5000):
    s = socket.create_connection(A); s.setblocking(False); sel.register(s, selectors.EVENT_READ); conns.append(s)
time.sleep(1.0)
stop = False; eofs = [0]; last_eof = [0.0]
def reader():
    while not stop:
        for key, _ in sel.select(0.05):
            s = key.fileobj
            try:
                d = s.recv(4096)
            except BlockingIOError:
                continue
            except OSError:
                d = b""
            if not d:
                eofs[0] += 1; last_eof[0] = time.time()
                sel.unregister(s)
                if MODE == "close": s.close()
                else: s.shutdown(socket.SHUT_RD) if False else None
threading.Thread(target=reader, daemon=True).start()
t0 = time.time(); srv.kill()
ok = []; refused = None
while time.time() - t0 < 1.5:
    c = socket.socket(); c.settimeout(0.5)
    try:
        c.connect(A); ok.append(time.time() - t0)
    except OSError as e:
        if refused is None: refused = time.time() - t0
    c.close()
    time.sleep(0.0002)
stop = True; time.sleep(0.1)
print(f"{MODE:5s}: connects accepted after the kill: {len(ok)}, last one at {1000 * (ok[-1] if ok else 0):.1f} ms, first refusal at {1000 * (refused or 0):.1f} ms, peers saw {eofs[0]} FINs, last FIN at {1000 * (last_eof[0] - t0):.1f} ms")
