use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering::SeqCst};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::time::{sleep, timeout};

use realm_core::endpoint::{ConnectOpts, Endpoint, RemoteAddr};
use realm_core::failover::FailoverOpts;
use realm_core::tcp::run_tcp;
use realm_core::udp::run_udp;

fn addr(s: &str) -> SocketAddr {
    s.parse().unwrap()
}

fn ms(x: u64) -> Duration {
    Duration::from_millis(x)
}

fn endpoint(listen: &str, peers: &[&str], opts: FailoverOpts) -> Endpoint {
    Endpoint {
        laddr: addr(listen),
        raddr: RemoteAddr::SocketAddr(addr(peers[0])),
        extra_raddrs: peers[1..].iter().map(|p| RemoteAddr::SocketAddr(addr(p))).collect(),
        bind_opts: Default::default(),
        conn_opts: ConnectOpts {
            connect_timeout: 2,
            associate_timeout: 10,
            failover: Some(opts),
            ..Default::default()
        },
    }
}

fn opts(interval: u64, timeout: u64) -> FailoverOpts {
    FailoverOpts {
        tcp_send: b"ping".to_vec(),
        tcp_expect: b"OK".to_vec(),
        udp_send: b"hc:".to_vec(),
        interval: ms(interval),
        timeout: ms(timeout),
        fall: 3,
        window: 5,
        rise: 2,
        hold: ms(300),
        ..Default::default()
    }
}

/// Answer every request with `reply`, or hold the connection silently while `mute` is set.
async fn tcp_server(listen: &str, reply: &'static [u8], mute: Arc<AtomicBool>) {
    let lis = TcpListener::bind(listen).await.unwrap();
    loop {
        let (mut s, _) = lis.accept().await.unwrap();
        let mute = mute.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 64];
            if s.read(&mut buf).await.unwrap_or(0) == 0 {
                return;
            }
            if mute.load(SeqCst) {
                sleep(Duration::from_secs(30)).await;
                return;
            }
            let _ = s.write_all(reply).await;
        });
    }
}

async fn tcp_ask(relay: &str) -> Option<Vec<u8>> {
    let mut s = TcpStream::connect(relay).await.ok()?;
    s.write_all(b"ping").await.ok()?;
    let mut buf = vec![0u8; 16];
    // windows retries a refused connect for ~2s, so the relay may spend
    // half of its 2s dial budget on a refused primary before the retry
    let n = timeout(Duration::from_secs(3), s.read(&mut buf)).await.ok()?.ok()?;
    Some(buf[..n].to_vec())
}

/// Echo every datagram after `delay` ms, count those starting with `data:`.
async fn udp_echo(listen: &str, delay: Arc<AtomicU32>, hits: Arc<AtomicUsize>) {
    let sock = Arc::new(UdpSocket::bind(listen).await.unwrap());
    let mut buf = [0u8; 1500];
    loop {
        // windows reports an icmp unreachable (late echo to a closed probe socket)
        // as WSAECONNRESET on the next recv, skip it
        let Ok((n, peer)) = sock.recv_from(&mut buf).await else {
            continue;
        };
        let data = buf[..n].to_vec();
        if data.starts_with(b"data:") {
            hits.fetch_add(1, SeqCst);
        }
        let d = delay.load(SeqCst) as u64;
        let sock = sock.clone();
        tokio::spawn(async move {
            if d > 0 {
                sleep(ms(d)).await;
            }
            let _ = sock.send_to(&data, peer).await;
        });
    }
}

async fn udp_recv(sock: &UdpSocket, wait: u64) -> Option<Vec<u8>> {
    let mut buf = [0u8; 64];
    let n = timeout(ms(wait), sock.recv(&mut buf)).await.ok()?.ok()?;
    Some(buf[..n].to_vec())
}

#[tokio::test]
async fn tcp_refused_primary_retries_backup_per_connection() {
    let _ = env_logger::try_init();
    let relay = "127.0.0.1:31000";
    // primary 31001 has no listener
    tokio::spawn(tcp_server("127.0.0.1:31002", b"OK-B", Default::default()));
    // slow probing: the global switch has not happened yet when we connect
    tokio::spawn(run_tcp(endpoint(
        relay,
        &["127.0.0.1:31001", "127.0.0.1:31002"],
        opts(5000, 200),
    )));
    sleep(ms(300)).await;

    assert_eq!(tcp_ask(relay).await.as_deref(), Some(&b"OK-B"[..]));
}

#[tokio::test]
async fn tcp_silent_primary_switch_and_failback() {
    let _ = env_logger::try_init();
    let relay = "127.0.0.1:31010";
    let mute_a = Arc::new(AtomicBool::new(false));
    tokio::spawn(tcp_server("127.0.0.1:31011", b"OK-A", mute_a.clone()));
    tokio::spawn(tcp_server("127.0.0.1:31012", b"OK-B", Default::default()));
    tokio::spawn(run_tcp(endpoint(
        relay,
        &["127.0.0.1:31011", "127.0.0.1:31012"],
        opts(100, 100),
    )));
    sleep(ms(300)).await;
    assert_eq!(tcp_ask(relay).await.as_deref(), Some(&b"OK-A"[..]));

    // primary still accepts connections but no longer answers
    mute_a.store(true, SeqCst);
    sleep(ms(1000)).await;
    assert_eq!(tcp_ask(relay).await.as_deref(), Some(&b"OK-B"[..]));

    // primary answers again: rise + hold, then new connections go back
    mute_a.store(false, SeqCst);
    sleep(ms(1000)).await;
    assert_eq!(tcp_ask(relay).await.as_deref(), Some(&b"OK-A"[..]));
}

#[tokio::test]
async fn udp_move_sticky_and_drop_late_replies() {
    let _ = env_logger::try_init();
    let relay = addr("127.0.0.1:31020");
    let (delay_a, hits_a, hits_b) = (
        Arc::new(AtomicU32::new(0)),
        Arc::<AtomicUsize>::default(),
        Arc::<AtomicUsize>::default(),
    );
    tokio::spawn(udp_echo("127.0.0.1:31021", delay_a.clone(), hits_a.clone()));
    tokio::spawn(udp_echo("127.0.0.1:31022", Default::default(), hits_b.clone()));
    tokio::spawn(run_udp(endpoint(
        "127.0.0.1:31020",
        &["127.0.0.1:31021", "127.0.0.1:31022"],
        opts(100, 150),
    )));
    sleep(ms(300)).await;

    let c = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    c.connect(relay).await.unwrap();
    c.send(b"data:1").await.unwrap();
    assert_eq!(udp_recv(&c, 500).await.as_deref(), Some(&b"data:1"[..]));
    assert_eq!(hits_a.load(SeqCst), 1);

    // primary turns slow: its replies arrive 1.5s late, probes time out
    delay_a.store(1500, SeqCst);
    c.send(b"data:2").await.unwrap();
    sleep(ms(800)).await;

    // same client socket: the next datagram goes to the backup
    c.send(b"data:3").await.unwrap();
    assert_eq!(udp_recv(&c, 500).await.as_deref(), Some(&b"data:3"[..]));
    assert_eq!(hits_b.load(SeqCst), 1);
    // the late reply to data:2 from the old peer is dropped
    assert_eq!(udp_recv(&c, 1200).await, None);

    // primary recovers and becomes active again
    delay_a.store(0, SeqCst);
    sleep(ms(1000)).await;

    // the existing association sticks to the healthy backup
    c.send(b"data:4").await.unwrap();
    assert_eq!(udp_recv(&c, 500).await.as_deref(), Some(&b"data:4"[..]));
    assert_eq!(hits_b.load(SeqCst), 2);

    // a new client goes to the primary
    let d = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    d.connect(relay).await.unwrap();
    d.send(b"data:5").await.unwrap();
    assert_eq!(udp_recv(&d, 500).await.as_deref(), Some(&b"data:5"[..]));
    assert_eq!(hits_a.load(SeqCst), 3);
}
