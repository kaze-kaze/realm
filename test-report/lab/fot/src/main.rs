//! fot: failover test tool.
//!
//!   fot srv tag=A tcp=0.0.0.0:5000 udp=0.0.0.0:5000 health=0.0.0.0:5001 hudp=0.0.0.0:5001 ctl=127.0.0.1:5999
//!   fot ctl 127.0.0.1:5999 tcp.mode=mute udp.delay=1500 stats
//!   fot churn target=10.9.0.2:6000 conns=64 secs=30 size=64 timeout=5000 rate=0 out=x.json
//!   fot hold  target=10.9.0.2:6000 conns=1000 secs=60 interval=1000 size=64 reconnect=0
//!   fot udp   target=10.9.0.2:6000 socks=100 pps=1000 secs=30 size=64
//!   fot abuse target=10.9.0.2:6000 conns=64 secs=10 kind=rst|half|junk
//!
//! tcp data protocol: request = "RQ" + u16 + u32 len + payload, reply = tag byte + payload.
//! "ping" is answered with "OK" + tag. udp: "DATA.." is answered with "TAG" + tag + rest,
//! anything else is echoed verbatim (health probes).

use std::collections::{BTreeMap, HashMap};
use std::io::{self, ErrorKind};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpSocket, TcpStream, UdpSocket};
use tokio::time::{sleep, sleep_until, timeout, Instant};

type Args = HashMap<String, String>;

fn get<T: std::str::FromStr>(a: &Args, k: &str, d: T) -> T {
    a.get(k).and_then(|v| v.parse().ok()).unwrap_or(d)
}

fn ms(x: u64) -> Duration {
    Duration::from_millis(x)
}

// ---------------------------------------------------------------- server

#[derive(Default)]
struct Mode {
    /// 0 ok, 1 mute, 2 close, 3 rst
    kind: AtomicU32,
    delay_ms: AtomicU32,
    drop_pct: AtomicU32,
    conns: AtomicU64,
    active: AtomicU64,
    reqs: AtomicU64,
    pings: AtomicU64,
    pkts: AtomicU64,
    data_pkts: AtomicU64,
}

async fn serve_tcp(lis: TcpListener, tag: u8, m: Arc<Mode>) {
    loop {
        let Ok((s, _)) = lis.accept().await else {
            sleep(ms(10)).await;
            continue;
        };
        m.conns.fetch_add(1, Relaxed);
        match m.kind.load(Relaxed) {
            2 => continue,
            3 => {
                let _ = s.set_linger(Some(Duration::ZERO));
                continue;
            }
            _ => {}
        }
        let m = m.clone();
        tokio::spawn(async move {
            m.active.fetch_add(1, Relaxed);
            let _ = handle_tcp(s, tag, &m).await;
            m.active.fetch_sub(1, Relaxed);
        });
    }
}

async fn handle_tcp(mut s: TcpStream, tag: u8, m: &Mode) -> io::Result<()> {
    let _ = s.set_nodelay(true);
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let mut head = [0u8; 4];
        if s.read_exact(&mut head).await.is_err() {
            return Ok(());
        }
        if m.kind.load(Relaxed) == 1 {
            // application hang: keep reading, never answer
            loop {
                if s.read(&mut buf).await? == 0 {
                    return Ok(());
                }
            }
        }
        let d = m.delay_ms.load(Relaxed) as u64;
        if &head == b"ping" {
            m.pings.fetch_add(1, Relaxed);
            if d > 0 {
                sleep(ms(d)).await;
            }
            s.write_all(&[b'O', b'K', tag]).await?;
            continue;
        }
        if &head[..2] != b"RQ" {
            // unknown protocol: plain echo
            s.write_all(&head).await?;
            loop {
                let n = s.read(&mut buf).await?;
                if n == 0 {
                    return Ok(());
                }
                s.write_all(&buf[..n]).await?;
            }
        }
        let mut lenb = [0u8; 4];
        s.read_exact(&mut lenb).await?;
        let mut left = u32::from_le_bytes(lenb) as usize;
        m.reqs.fetch_add(1, Relaxed);
        if d > 0 {
            sleep(ms(d)).await;
        }
        s.write_all(&[tag]).await?;
        while left > 0 {
            let cap = left.min(buf.len());
            let n = s.read(&mut buf[..cap]).await?;
            if n == 0 {
                return Ok(());
            }
            s.write_all(&buf[..n]).await?;
            left -= n;
        }
    }
}

fn cheap_rand() -> u32 {
    static S: AtomicU32 = AtomicU32::new(0x9e3779b9);
    let mut x = S.load(Relaxed);
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    S.store(x, Relaxed);
    x
}

async fn serve_udp(sock: UdpSocket, tag: u8, m: Arc<Mode>) {
    let sock = Arc::new(sock);
    let mut buf = vec![0u8; 65536];
    loop {
        let Ok((n, peer)) = sock.recv_from(&mut buf).await else {
            continue;
        };
        m.pkts.fetch_add(1, Relaxed);
        if m.kind.load(Relaxed) == 1 {
            continue;
        }
        let pct = m.drop_pct.load(Relaxed);
        if pct > 0 && cheap_rand() % 100 < pct {
            continue;
        }
        if n >= 4 && &buf[..4] == b"DATA" {
            m.data_pkts.fetch_add(1, Relaxed);
            buf[..3].copy_from_slice(b"TAG");
            buf[3] = tag;
        }
        let d = m.delay_ms.load(Relaxed) as u64;
        if d == 0 {
            let _ = sock.send_to(&buf[..n], peer).await;
        } else {
            let data = buf[..n].to_vec();
            let sock = sock.clone();
            tokio::spawn(async move {
                sleep(ms(d)).await;
                let _ = sock.send_to(&data, peer).await;
            });
        }
    }
}

async fn serve_ctl(lis: TcpListener, modes: Arc<BTreeMap<&'static str, Arc<Mode>>>) {
    loop {
        let Ok((s, _)) = lis.accept().await else { continue };
        let modes = modes.clone();
        tokio::spawn(async move {
            let (r, mut w) = s.into_split();
            let mut lines = BufReader::new(r).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim().to_string();
                let out = if line == "stats" {
                    let mut o = String::from("{");
                    for (i, (k, m)) in modes.iter().enumerate() {
                        if i > 0 {
                            o.push(',');
                        }
                        o.push_str(&format!(
                            "\"{}\":{{\"conns\":{},\"active\":{},\"reqs\":{},\"pings\":{},\"pkts\":{},\"data_pkts\":{}}}",
                            k,
                            m.conns.load(Relaxed),
                            m.active.load(Relaxed),
                            m.reqs.load(Relaxed),
                            m.pings.load(Relaxed),
                            m.pkts.load(Relaxed),
                            m.data_pkts.load(Relaxed)
                        ));
                    }
                    o.push('}');
                    o
                } else if let Some((lhs, v)) = line.split_once('=') {
                    let (who, field) = lhs.split_once('.').unwrap_or((lhs, "mode"));
                    match modes.get(who) {
                        None => format!("ERR unknown target {}", who),
                        Some(m) => match field {
                            "mode" => {
                                let k = match v {
                                    "ok" => 0,
                                    "mute" => 1,
                                    "close" => 2,
                                    "rst" => 3,
                                    _ => 99,
                                };
                                if k == 99 {
                                    format!("ERR bad mode {}", v)
                                } else {
                                    m.kind.store(k, Relaxed);
                                    "OK".into()
                                }
                            }
                            "delay" => {
                                m.delay_ms.store(v.parse().unwrap_or(0), Relaxed);
                                "OK".into()
                            }
                            "drop" => {
                                m.drop_pct.store(v.parse().unwrap_or(0), Relaxed);
                                "OK".into()
                            }
                            _ => format!("ERR bad field {}", field),
                        },
                    }
                } else {
                    "ERR".into()
                };
                if w.write_all(format!("{}\n", out).as_bytes()).await.is_err() {
                    break;
                }
            }
        });
    }
}

fn listen_tcp(addr: &str) -> TcpListener {
    let addr: SocketAddr = addr.parse().unwrap();
    let s = if addr.is_ipv4() { TcpSocket::new_v4() } else { TcpSocket::new_v6() }.unwrap();
    s.set_reuseaddr(true).unwrap();
    s.bind(addr).unwrap_or_else(|e| panic!("bind {}: {}", addr, e));
    s.listen(8192).unwrap()
}

async fn cmd_srv(a: Args) {
    let tag = get(&a, "tag", "A".to_string()).as_bytes()[0];
    let mut modes: BTreeMap<&'static str, Arc<Mode>> = BTreeMap::new();
    for k in ["tcp", "health"] {
        if let Some(addr) = a.get(k) {
            let m = Arc::new(Mode::default());
            modes.insert(k, m.clone());
            tokio::spawn(serve_tcp(listen_tcp(addr), tag, m));
        }
    }
    for k in ["udp", "hudp"] {
        if let Some(addr) = a.get(k) {
            let m = Arc::new(Mode::default());
            modes.insert(k, m.clone());
            let sock = UdpSocket::bind(addr).await.unwrap_or_else(|e| panic!("bind {}: {}", addr, e));
            tokio::spawn(serve_udp(sock, tag, m));
        }
    }
    if let Some(addr) = a.get("ctl") {
        tokio::spawn(serve_ctl(listen_tcp(addr), Arc::new(modes)));
    }
    println!("READY");
    std::future::pending::<()>().await;
}

async fn cmd_ctl(argv: Vec<String>) {
    let s = TcpStream::connect(&argv[0]).await.expect("ctl connect");
    let (r, mut w) = s.into_split();
    let mut lines = BufReader::new(r).lines();
    for c in &argv[1..] {
        w.write_all(format!("{}\n", c).as_bytes()).await.unwrap();
        let reply = lines.next_line().await.unwrap().unwrap_or_default();
        println!("{}", reply);
    }
}

// ---------------------------------------------------------------- stats

const TAGS: usize = 5; // A B C D other
const TAG_NAMES: [&str; TAGS] = ["A", "B", "C", "D", "other"];
const ERRS: usize = 8;
const ERR_NAMES: [&str; ERRS] = [
    "refused",
    "conn_timeout",
    "read_timeout",
    "reset",
    "eof",
    "mismatch",
    "conn_other",
    "io_other",
];

#[derive(Clone, Copy, Debug)]
enum E {
    Refused = 0,
    ConnTimeout = 1,
    ReadTimeout = 2,
    Reset = 3,
    Eof = 4,
    Mismatch = 5,
    ConnOther = 6,
    IoOther = 7,
}

fn tag_idx(t: u8) -> usize {
    match t {
        b'A'..=b'D' => (t - b'A') as usize,
        _ => 4,
    }
}

#[derive(Default)]
struct Bucket {
    ok: [AtomicU64; TAGS],
    err: [AtomicU64; ERRS],
    sent: AtomicU64,
    lat_max: AtomicU64,
}

const HIST: usize = 64 * 8;

struct Stats {
    start: Instant,
    epoch_ms: u128,
    bucket_ms: u64,
    buckets: Vec<Bucket>,
    hist: Vec<AtomicU64>,
    first_errs: Mutex<Vec<String>>,
}

impl Stats {
    fn new(secs: u64, bucket_ms: u64) -> Arc<Self> {
        let n = ((secs + 30) * 1000 / bucket_ms) as usize;
        Arc::new(Self {
            start: Instant::now(),
            epoch_ms: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis(),
            bucket_ms,
            buckets: (0..n).map(|_| Bucket::default()).collect(),
            hist: (0..HIST).map(|_| AtomicU64::new(0)).collect(),
            first_errs: Mutex::new(Vec::new()),
        })
    }

    fn bucket(&self) -> &Bucket {
        let i = (self.start.elapsed().as_millis() as u64 / self.bucket_ms) as usize;
        &self.buckets[i.min(self.buckets.len() - 1)]
    }

    fn ok(&self, tag: u8, lat: Duration) {
        let b = self.bucket();
        b.ok[tag_idx(tag)].fetch_add(1, Relaxed);
        let us = lat.as_micros() as u64;
        b.lat_max.fetch_max(us, Relaxed);
        self.hist[hist_idx(us)].fetch_add(1, Relaxed);
    }

    fn err(&self, e: E, detail: &dyn std::fmt::Display) {
        self.bucket().err[e as usize].fetch_add(1, Relaxed);
        let mut v = self.first_errs.lock().unwrap();
        if v.len() < 12 {
            v.push(format!("{:?}@{}ms: {}", e, self.start.elapsed().as_millis(), detail));
        }
    }

    fn sent(&self, n: u64) {
        self.bucket().sent.fetch_add(n, Relaxed);
    }

    fn pct(&self, p: f64) -> u64 {
        let total: u64 = self.hist.iter().map(|x| x.load(Relaxed)).sum();
        if total == 0 {
            return 0;
        }
        let want = (total as f64 * p).ceil() as u64;
        let mut acc = 0;
        for (i, x) in self.hist.iter().enumerate() {
            acc += x.load(Relaxed);
            if acc >= want {
                return hist_upper(i);
            }
        }
        0
    }

    fn json(&self, cmd: &str, elapsed: f64, extra: &str) -> String {
        let mut ok = [0u64; TAGS];
        let mut err = [0u64; ERRS];
        let mut sent = 0u64;
        let mut lat_max = 0u64;
        let mut rows = String::new();
        let last = self
            .buckets
            .iter()
            .rposition(|b| {
                b.sent.load(Relaxed) > 0
                    || b.ok.iter().any(|x| x.load(Relaxed) > 0)
                    || b.err.iter().any(|x| x.load(Relaxed) > 0)
            })
            .unwrap_or(0);
        for (i, b) in self.buckets[..=last].iter().enumerate() {
            let o: Vec<u64> = b.ok.iter().map(|x| x.load(Relaxed)).collect();
            let e: Vec<u64> = b.err.iter().map(|x| x.load(Relaxed)).collect();
            let s = b.sent.load(Relaxed);
            let lm = b.lat_max.load(Relaxed);
            for k in 0..TAGS {
                ok[k] += o[k];
            }
            for k in 0..ERRS {
                err[k] += e[k];
            }
            sent += s;
            lat_max = lat_max.max(lm);
            if i > 0 {
                rows.push(',');
            }
            rows.push_str(&format!(
                "[{},{},{:?},{:?},{}]",
                i as u64 * self.bucket_ms,
                s,
                o,
                e,
                lm
            ));
        }
        let kv = |names: &[&str], vals: &[u64]| {
            names
                .iter()
                .zip(vals)
                .map(|(n, v)| format!("\"{}\":{}", n, v))
                .collect::<Vec<_>>()
                .join(",")
        };
        let total_ok: u64 = ok.iter().sum();
        let total_err: u64 = err.iter().sum();
        let errs = self
            .first_errs
            .lock()
            .unwrap()
            .iter()
            .map(|s| format!("\"{}\"", s.replace('\\', "/").replace('"', "'")))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"cmd\":\"{}\",\"epoch_ms\":{},\"elapsed\":{:.3},\"bucket_ms\":{},\"sent\":{},\"total_ok\":{},\"total_err\":{},\"rate\":{:.1},\"ok\":{{{}}},\"err\":{{{}}},\"lat_us\":{{\"p50\":{},\"p90\":{},\"p99\":{},\"p999\":{},\"max\":{}}},\"first_errs\":[{}]{},\"buckets\":[{}]}}",
            cmd,
            self.epoch_ms,
            elapsed,
            self.bucket_ms,
            sent,
            total_ok,
            total_err,
            total_ok as f64 / elapsed,
            kv(&TAG_NAMES, &ok),
            kv(&ERR_NAMES, &err),
            self.pct(0.50),
            self.pct(0.90),
            self.pct(0.99),
            self.pct(0.999),
            lat_max,
            errs,
            extra,
            rows
        )
    }
}

fn hist_idx(us: u64) -> usize {
    if us < 16 {
        return us as usize;
    }
    let p = 63 - us.leading_zeros() as usize; // >= 4
    let idx = (p - 3) * 8 + ((us >> (p - 3)) & 7) as usize;
    idx.min(HIST - 1)
}

fn hist_upper(i: usize) -> u64 {
    if i < 16 {
        return i as u64;
    }
    let p = i / 8 + 3;
    let sub = (i % 8) as u64;
    ((8 + sub + 1) << (p - 3)) - 1
}

fn emit(a: &Args, s: String) {
    match a.get("out") {
        Some(p) => std::fs::write(p, s).unwrap(),
        None => println!("{}", s),
    }
}

// ---------------------------------------------------------------- tcp clients

fn payload(seed: u64, size: usize) -> Vec<u8> {
    let mut x = seed.wrapping_mul(0x9E3779B97F4A7C15) | 1;
    let mut v = Vec::with_capacity(size);
    while v.len() < size {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        v.extend_from_slice(&x.to_le_bytes());
    }
    v.truncate(size);
    v
}

fn classify_conn(e: &io::Error) -> E {
    match e.kind() {
        ErrorKind::ConnectionRefused => E::Refused,
        ErrorKind::TimedOut => E::ConnTimeout,
        ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted | ErrorKind::BrokenPipe => E::Reset,
        _ => E::ConnOther,
    }
}

fn classify_io(e: &io::Error) -> E {
    match e.kind() {
        ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted | ErrorKind::BrokenPipe => E::Reset,
        ErrorKind::UnexpectedEof => E::Eof,
        ErrorKind::TimedOut => E::ReadTimeout,
        _ => E::IoOther,
    }
}

static BIND: std::sync::OnceLock<Option<std::net::IpAddr>> = std::sync::OnceLock::new();

async fn connect_from(target: SocketAddr) -> io::Result<TcpStream> {
    match BIND.get().copied().flatten() {
        None => TcpStream::connect(target).await,
        Some(ip) => {
            let s = if ip.is_ipv4() { TcpSocket::new_v4() } else { TcpSocket::new_v6() }?;
            s.bind(SocketAddr::new(ip, 0))?;
            s.connect(target).await
        }
    }
}

async fn dial(target: SocketAddr, to: Duration) -> Result<TcpStream, (E, String)> {
    match timeout(to, connect_from(target)).await {
        Err(_) => Err((E::ConnTimeout, "connect timeout".into())),
        Ok(Err(e)) => Err((classify_conn(&e), e.to_string())),
        Ok(Ok(s)) => {
            let _ = s.set_nodelay(true);
            Ok(s)
        }
    }
}

/// One request on an open stream. Returns the backend tag.
async fn request(s: &mut TcpStream, seed: u64, size: usize, to: Duration) -> Result<u8, (E, String)> {
    let body = payload(seed, size);
    let mut req = Vec::with_capacity(8 + size.min(65536));
    req.extend_from_slice(b"RQ\0\0");
    req.extend_from_slice(&(size as u32).to_le_bytes());
    let run = async {
        if size <= 65536 {
            req.extend_from_slice(&body);
            s.write_all(&req).await?;
            let mut resp = vec![0u8; 1 + size];
            s.read_exact(&mut resp).await?;
            if resp[1..] != body[..] {
                return Ok(Err(resp[0]));
            }
            Ok::<_, io::Error>(Ok(resp[0]))
        } else {
            // large body: write and read concurrently
            s.write_all(&req).await?;
            let (mut r, mut w) = s.split();
            let wr = async {
                w.write_all(&body).await?;
                Ok::<_, io::Error>(())
            };
            let rd = async {
                let mut resp = vec![0u8; 1 + size];
                r.read_exact(&mut resp).await?;
                Ok::<_, io::Error>(resp)
            };
            let (a, b) = tokio::join!(wr, rd);
            a?;
            let resp = b?;
            if resp[1..] != body[..] {
                return Ok(Err(resp[0]));
            }
            Ok(Ok(resp[0]))
        }
    };
    match timeout(to, run).await {
        Err(_) => Err((E::ReadTimeout, "request timeout".into())),
        Ok(Err(e)) => Err((classify_io(&e), e.to_string())),
        Ok(Ok(Err(tag))) => Err((E::Mismatch, format!("payload mismatch from {}", tag as char))),
        Ok(Ok(Ok(tag))) => Ok(tag),
    }
}

async fn cmd_churn(a: Args) {
    let target: SocketAddr = get(&a, "target", String::new()).parse().expect("target");
    let conns: u64 = get(&a, "conns", 64);
    let secs: u64 = get(&a, "secs", 10);
    let size: usize = get(&a, "size", 64);
    let to = ms(get(&a, "timeout", 5000));
    let rate: f64 = get(&a, "rate", 0.0);
    let st = Stats::new(secs, get(&a, "bucket", 100));
    let end = st.start + Duration::from_secs(secs);
    let gap = if rate > 0.0 {
        Some(Duration::from_secs_f64(conns as f64 / rate))
    } else {
        None
    };

    let mut hs = Vec::new();
    for w in 0..conns {
        let st = st.clone();
        hs.push(tokio::spawn(async move {
            let mut i = 0u64;
            let mut next = Instant::now() + gap.map_or(Duration::ZERO, |g| g.mul_f64(w as f64 / conns as f64));
            while Instant::now() < end {
                if gap.is_some() {
                    sleep_until(next).await;
                    if Instant::now() >= end {
                        break;
                    }
                }
                let t0 = Instant::now();
                st.sent(1);
                let seed = w << 32 | i;
                let res = match dial(target, to).await {
                    Err(e) => Err(e),
                    Ok(mut s) => request(&mut s, seed, size, to).await,
                };
                match res {
                    Ok(tag) => st.ok(tag, t0.elapsed()),
                    Err((e, d)) => st.err(e, &d),
                }
                i += 1;
                if let Some(g) = gap {
                    next += g;
                    if next < Instant::now() {
                        next = Instant::now();
                    }
                }
            }
        }));
    }
    for h in hs {
        let _ = h.await;
    }
    let el = st.start.elapsed().as_secs_f64();
    emit(&a, st.json("churn", el, ""));
}

async fn cmd_hold(a: Args) {
    let target: SocketAddr = get(&a, "target", String::new()).parse().expect("target");
    let conns: u64 = get(&a, "conns", 100);
    let secs: u64 = get(&a, "secs", 10);
    let size: usize = get(&a, "size", 64);
    let to = ms(get(&a, "timeout", 5000));
    let interval = ms(get(&a, "interval", 1000));
    let ramp = ms(get(&a, "ramp", 1000));
    let reconnect: u32 = get(&a, "reconnect", 0);
    // idle=1: one request, then stay silent and only watch for the connection being closed
    let idle: u32 = get(&a, "idle", 0);
    let st = Stats::new(secs, get(&a, "bucket", 100));
    let end = st.start + Duration::from_secs(secs);
    let opened = Arc::new(AtomicU64::new(0));
    let died = Arc::new(AtomicU64::new(0));
    let tag_changed = Arc::new(AtomicU64::new(0));
    let alive = Arc::new(AtomicU64::new(0));
    let peak = Arc::new(AtomicU64::new(0));

    let mut hs = Vec::new();
    for w in 0..conns {
        let (st, opened, died, tag_changed, alive, peak) = (
            st.clone(),
            opened.clone(),
            died.clone(),
            tag_changed.clone(),
            alive.clone(),
            peak.clone(),
        );
        hs.push(tokio::spawn(async move {
            sleep(ramp.mul_f64(w as f64 / conns as f64)).await;
            let mut i = 0u64;
            'outer: while Instant::now() < end {
                let mut s = match dial(target, to).await {
                    Ok(s) => s,
                    Err((e, d)) => {
                        st.err(e, &d);
                        if reconnect == 0 {
                            return;
                        }
                        sleep(ms(200)).await;
                        continue;
                    }
                };
                opened.fetch_add(1, Relaxed);
                let n = alive.fetch_add(1, Relaxed) + 1;
                peak.fetch_max(n, Relaxed);
                let mut conn_tag = 0u8;
                // spread requests over the interval
                if idle == 0 {
                    sleep(interval.mul_f64((w % 97) as f64 / 97.0)).await;
                }
                let mut first = true;
                while Instant::now() < end {
                    if idle == 1 && !first {
                        let mut b = [0u8; 1];
                        if let Ok(r) = tokio::time::timeout_at(end, s.read(&mut b)).await {
                            st.err(E::Eof, &format!("closed while idle: {:?}", r.map(|_| ())));
                            died.fetch_add(1, Relaxed);
                            alive.fetch_sub(1, Relaxed);
                            return;
                        }
                        break;
                    }
                    first = false;
                    let t0 = Instant::now();
                    st.sent(1);
                    match request(&mut s, w << 32 | i, size, to).await {
                        Ok(tag) => {
                            st.ok(tag, t0.elapsed());
                            if conn_tag != 0 && conn_tag != tag {
                                tag_changed.fetch_add(1, Relaxed);
                            }
                            conn_tag = tag;
                        }
                        Err((e, d)) => {
                            st.err(e, &d);
                            died.fetch_add(1, Relaxed);
                            alive.fetch_sub(1, Relaxed);
                            if reconnect == 0 {
                                return;
                            }
                            continue 'outer;
                        }
                    }
                    i += 1;
                    sleep(interval).await;
                }
                alive.fetch_sub(1, Relaxed);
            }
        }));
    }
    // report alive count right before the end
    sleep_until(end - ms(300)).await;
    let alive_end = alive.load(Relaxed);
    for h in hs {
        let _ = h.await;
    }
    let el = st.start.elapsed().as_secs_f64();
    let extra = format!(
        ",\"opened\":{},\"died\":{},\"tag_changed\":{},\"alive_at_end\":{},\"peak_alive\":{}",
        opened.load(Relaxed),
        died.load(Relaxed),
        tag_changed.load(Relaxed),
        alive_end,
        peak.load(Relaxed)
    );
    emit(&a, st.json("hold", el, &extra));
}

/// Misbehaving clients: must never hurt the relay or look like a backend failure.
async fn cmd_abuse(a: Args) {
    let target: SocketAddr = get(&a, "target", String::new()).parse().expect("target");
    let conns: u64 = get(&a, "conns", 64);
    let secs: u64 = get(&a, "secs", 10);
    let kind = get(&a, "kind", "rst".to_string());
    let end = Instant::now() + Duration::from_secs(secs);
    let done = Arc::new(AtomicU64::new(0));
    let mut hs = Vec::new();
    for w in 0..conns {
        let (kind, done) = (kind.clone(), done.clone());
        hs.push(tokio::spawn(async move {
            let mut i = 0u64;
            while Instant::now() < end {
                i += 1;
                let Ok(Ok(mut s)) = timeout(ms(3000), TcpStream::connect(target)).await else {
                    sleep(ms(20)).await;
                    continue;
                };
                match kind.as_str() {
                    // connect and reset at once
                    "rst" => {
                        let _ = s.set_linger(Some(Duration::ZERO));
                    }
                    // half a header, then half-close and wait for the peer
                    "half" => {
                        let _ = s.write_all(b"RQ\0").await;
                        let _ = s.shutdown().await;
                        let mut b = [0u8; 16];
                        let _ = timeout(ms(500), s.read(&mut b)).await;
                    }
                    // request, then reset in the middle of the reply
                    "midrst" => {
                        let mut req = b"RQ\0\0".to_vec();
                        req.extend_from_slice(&(262144u32).to_le_bytes());
                        req.extend_from_slice(&payload(w << 32 | i, 262144));
                        let _ = timeout(ms(200), s.write_all(&req)).await;
                        let _ = s.set_linger(Some(Duration::ZERO));
                    }
                    // connect and stay silent for a while
                    _ => {
                        sleep(ms(300)).await;
                    }
                }
                drop(s);
                done.fetch_add(1, Relaxed);
            }
        }));
    }
    for h in hs {
        let _ = h.await;
    }
    println!("{{\"cmd\":\"abuse\",\"kind\":\"{}\",\"done\":{}}}", kind, done.load(Relaxed));
}

// ---------------------------------------------------------------- udp client

async fn cmd_udp(a: Args) {
    let target: SocketAddr = get(&a, "target", String::new()).parse().expect("target");
    let socks: u64 = get(&a, "socks", 10);
    let secs: u64 = get(&a, "secs", 10);
    let pps: f64 = get(&a, "pps", 100.0);
    let size: usize = get::<usize>(&a, "size", 64).max(28);
    let drain = ms(get(&a, "drain", 2500));
    let start_spread = ms(get(&a, "spread", 1000));
    let st = Stats::new(secs + 5, get(&a, "bucket", 100));
    let end = st.start + Duration::from_secs(secs);
    let gap = Duration::from_secs_f64(socks as f64 / pps);
    let stale = Arc::new(AtomicU64::new(0));
    let dup = Arc::new(AtomicU64::new(0));
    let foreign = Arc::new(AtomicU64::new(0));
    let seqs: Arc<Mutex<BTreeMap<String, u64>>> = Arc::new(Mutex::new(BTreeMap::new()));

    let mut hs = Vec::new();
    for id in 0..socks {
        let (st, stale, dup, foreign, seqs) = (st.clone(), stale.clone(), dup.clone(), foreign.clone(), seqs.clone());
        hs.push(tokio::spawn(async move {
            let bind = if target.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" };
            let sock = match UdpSocket::bind(bind).await {
                Ok(s) => s,
                Err(e) => {
                    st.err(E::ConnOther, &e);
                    return;
                }
            };
            sock.connect(target).await.unwrap();
            let mut next = Instant::now() + start_spread.mul_f64(id as f64 / socks as f64);
            let mut seq = 0u64;
            let mut seen: Vec<bool> = Vec::new();
            let mut tags: Vec<u8> = Vec::new();
            let mut max_seq = 0u64;
            let mut max_tag = 0u8;
            let mut pkt = vec![0u8; size];
            let mut buf = vec![0u8; 65536];
            let mut sending = true;
            let stop = end + drain;
            loop {
                tokio::select! {
                    _ = sleep_until(if sending { next } else { stop }) => {
                        if !sending { break; }
                        if next >= end { sending = false; continue; }
                        pkt[..4].copy_from_slice(b"DATA");
                        pkt[4..8].copy_from_slice(&(id as u32).to_le_bytes());
                        pkt[8..16].copy_from_slice(&seq.to_le_bytes());
                        pkt[16..24].copy_from_slice(&(st.start.elapsed().as_micros() as u64).to_le_bytes());
                        match sock.send(&pkt).await {
                            Ok(_) => st.sent(1),
                            Err(e) => st.err(classify_conn(&e), &e),
                        }
                        seen.push(false);
                        seq += 1;
                        next += gap;
                    }
                    r = sock.recv(&mut buf) => match r {
                        Ok(n) if n >= 24 && &buf[..3] == b"TAG" => {
                            let tag = buf[3];
                            let rid = u32::from_le_bytes(buf[4..8].try_into().unwrap()) as u64;
                            let rseq = u64::from_le_bytes(buf[8..16].try_into().unwrap());
                            let t_us = u64::from_le_bytes(buf[16..24].try_into().unwrap());
                            if rid != id || rseq as usize >= seen.len() || n != size {
                                foreign.fetch_add(1, Relaxed);
                                continue;
                            }
                            if seen[rseq as usize] {
                                dup.fetch_add(1, Relaxed);
                                continue;
                            }
                            seen[rseq as usize] = true;
                            let now = st.start.elapsed().as_micros() as u64;
                            st.ok(tag, Duration::from_micros(now.saturating_sub(t_us)));
                            if tags.last() != Some(&tag) { tags.push(tag); }
                            if rseq >= max_seq {
                                max_seq = rseq;
                                max_tag = tag;
                            } else if tag != max_tag {
                                // an older datagram answered by another peer after we moved on
                                stale.fetch_add(1, Relaxed);
                            }
                        }
                        Ok(_) => { foreign.fetch_add(1, Relaxed); }
                        Err(e) => st.err(classify_conn(&e), &e),
                    }
                }
            }
            let key = String::from_utf8_lossy(&tags).to_string();
            *seqs.lock().unwrap().entry(key).or_insert(0) += 1;
        }));
    }
    for h in hs {
        let _ = h.await;
    }
    let el = (end - st.start).as_secs_f64();
    let seq_json = seqs
        .lock()
        .unwrap()
        .iter()
        .map(|(k, v)| format!("\"{}\":{}", if k.is_empty() { "-" } else { k }, v))
        .collect::<Vec<_>>()
        .join(",");
    let extra = format!(
        ",\"stale\":{},\"dup\":{},\"foreign\":{},\"tag_seqs\":{{{}}}",
        stale.load(Relaxed),
        dup.load(Relaxed),
        foreign.load(Relaxed),
        seq_json
    );
    emit(&a, st.json("udp", el, &extra));
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.is_empty() {
        eprintln!("usage: fot srv|ctl|churn|hold|udp|abuse k=v ..");
        std::process::exit(2);
    }
    let cmd = argv[0].clone();
    let args: Args = argv[1..]
        .iter()
        .filter_map(|s| s.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
        .collect();
    let threads: usize = get(&args, "threads", 4);
    let _ = BIND.set(args.get("bind").and_then(|x| x.parse().ok()));
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(threads)
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        match cmd.as_str() {
            "srv" => cmd_srv(args).await,
            "ctl" => cmd_ctl(argv[1..].to_vec()).await,
            "churn" => cmd_churn(args).await,
            "hold" => cmd_hold(args).await,
            "udp" => cmd_udp(args).await,
            "abuse" => cmd_abuse(args).await,
            _ => {
                eprintln!("unknown command {}", cmd);
                std::process::exit(2);
            }
        }
    });
}
