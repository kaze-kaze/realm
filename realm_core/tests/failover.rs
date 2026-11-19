use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering::SeqCst};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::time::{sleep, timeout};

use realm_core::endpoint::{ConnectOpts, Endpoint, RemoteAddr};
use realm_core::failover::{FailoverOpts, UdpProbe};
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

#[tokio::test]
async fn udp_all_down_keeps_associations() {
    let _ = env_logger::try_init();
    let relay = addr("127.0.0.1:31030");
    let (hits_a, hits_b) = (Arc::<AtomicUsize>::default(), Arc::<AtomicUsize>::default());
    // health echoes on their own ports: a huge delay makes a peer look dead while its data port works
    let (mute_a, mute_b) = (Arc::new(AtomicU32::new(0)), Arc::new(AtomicU32::new(0)));
    tokio::spawn(udp_echo("127.0.0.1:31031", Default::default(), hits_a.clone()));
    tokio::spawn(udp_echo("127.0.0.1:31032", Default::default(), hits_b.clone()));
    tokio::spawn(udp_echo("127.0.0.1:31033", mute_a.clone(), Default::default()));
    tokio::spawn(udp_echo("127.0.0.1:31034", mute_b.clone(), Default::default()));
    tokio::spawn(run_udp(endpoint(
        "127.0.0.1:31030",
        &["127.0.0.1:31031", "127.0.0.1:31032"],
        FailoverOpts {
            probe: vec![addr("127.0.0.1:31033"), addr("127.0.0.1:31034")],
            ..opts(100, 150)
        },
    )));
    sleep(ms(300)).await;

    let c = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    c.connect(relay).await.unwrap();
    c.send(b"data:1").await.unwrap();
    assert_eq!(udp_recv(&c, 500).await.as_deref(), Some(&b"data:1"[..]));
    assert_eq!((hits_a.load(SeqCst), hits_b.load(SeqCst)), (1, 0));

    // primary looks dead: the association moves to the backup
    mute_a.store(60_000, SeqCst);
    sleep(ms(800)).await;
    c.send(b"data:2").await.unwrap();
    assert_eq!(udp_recv(&c, 500).await.as_deref(), Some(&b"data:2"[..]));
    assert_eq!((hits_a.load(SeqCst), hits_b.load(SeqCst)), (1, 1));

    // primary is back and active again, the association stays on the backup
    mute_a.store(0, SeqCst);
    sleep(ms(1000)).await;

    // now every peer looks dead: nothing better to move to, so it must not be shuffled
    mute_a.store(60_000, SeqCst);
    mute_b.store(60_000, SeqCst);
    sleep(ms(800)).await;
    c.send(b"data:3").await.unwrap();
    assert_eq!(udp_recv(&c, 500).await.as_deref(), Some(&b"data:3"[..]));
    assert_eq!((hits_a.load(SeqCst), hits_b.load(SeqCst)), (1, 2));
}

/// A QUIC server as far as the probe is concerned: answers an unknown version with a
/// Version Negotiation packet, unless muted. Echoes everything else as "application" data.
async fn quic_like(listen: &str, mute: Arc<AtomicBool>, hits: Arc<AtomicUsize>) {
    let sock = UdpSocket::bind(listen).await.unwrap();
    let mut buf = [0u8; 1500];
    loop {
        let Ok((n, peer)) = sock.recv_from(&mut buf).await else {
            continue;
        };
        let pkt = &buf[..n];
        if pkt.starts_with(b"data:") {
            hits.fetch_add(1, SeqCst);
            let _ = sock.send_to(pkt, peer).await;
            continue;
        }
        // long header, unknown version, large enough: version negotiation
        if n >= 1200 && pkt[0] & 0x80 != 0 && pkt[1..5] != [0, 0, 0, 1] && !mute.load(SeqCst) {
            let dcid = &pkt[6..6 + pkt[5] as usize];
            let scid_at = 6 + dcid.len();
            let scid = &pkt[scid_at + 1..scid_at + 1 + pkt[scid_at] as usize];
            let mut vn = vec![0x80, 0, 0, 0, 0, scid.len() as u8];
            vn.extend_from_slice(scid);
            vn.push(dcid.len() as u8);
            vn.extend_from_slice(dcid);
            vn.extend_from_slice(&[0, 0, 0, 1]);
            let _ = sock.send_to(&vn, peer).await;
        }
    }
}

/// `udp_probe = quic`: a QUIC service is probed on its own port, no echo responder.
#[tokio::test]
async fn udp_quic_probe_switches_without_echo() {
    let _ = env_logger::try_init();
    let relay = addr("127.0.0.1:31040");
    let (mute_a, hits_a, hits_b) = (
        Arc::new(AtomicBool::new(false)),
        Arc::<AtomicUsize>::default(),
        Arc::<AtomicUsize>::default(),
    );
    tokio::spawn(quic_like("127.0.0.1:31041", mute_a.clone(), hits_a.clone()));
    tokio::spawn(quic_like("127.0.0.1:31042", Default::default(), hits_b.clone()));
    tokio::spawn(run_udp(endpoint(
        "127.0.0.1:31040",
        &["127.0.0.1:31041", "127.0.0.1:31042"],
        FailoverOpts {
            udp_probe: UdpProbe::Quic,
            ..opts(100, 150)
        },
    )));
    sleep(ms(600)).await;

    // both answer version negotiation: the primary stays active
    let c = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    c.connect(relay).await.unwrap();
    c.send(b"data:1").await.unwrap();
    assert_eq!(udp_recv(&c, 500).await.as_deref(), Some(&b"data:1"[..]));
    assert_eq!((hits_a.load(SeqCst), hits_b.load(SeqCst)), (1, 0));

    // the primary stops answering the probe: the association moves
    mute_a.store(true, SeqCst);
    sleep(ms(800)).await;
    c.send(b"data:2").await.unwrap();
    assert_eq!(udp_recv(&c, 500).await.as_deref(), Some(&b"data:2"[..]));
    assert_eq!((hits_a.load(SeqCst), hits_b.load(SeqCst)), (1, 1));
}

#[cfg(feature = "balance")]
mod balanced {
    use super::*;
    use realm_core::balance::{Balancer, Strategy};

    fn balanced(listen: &str, peers: &[&str], strategy: Strategy, opts: FailoverOpts) -> Endpoint {
        let mut ep = endpoint(listen, peers, opts);
        ep.conn_opts.balancer = Balancer::new(strategy, &vec![1; peers.len()]);
        ep
    }

    /// Round robin over three peers; a dead one gets no new connection once it is down
    /// and takes its share again after it came back.
    #[tokio::test]
    async fn tcp_balance_skips_down_peer() {
        let _ = env_logger::try_init();
        let mutes: Vec<_> = (0..3).map(|_| Arc::new(AtomicBool::new(false))).collect();
        tokio::spawn(tcp_server("127.0.0.1:31051", b"OK-A", mutes[0].clone()));
        tokio::spawn(tcp_server("127.0.0.1:31052", b"OK-B", mutes[1].clone()));
        tokio::spawn(tcp_server("127.0.0.1:31053", b"OK-C", mutes[2].clone()));
        tokio::spawn(run_tcp(balanced(
            "127.0.0.1:31050",
            &["127.0.0.1:31051", "127.0.0.1:31052", "127.0.0.1:31053"],
            Strategy::RoundRobin,
            opts(100, 150),
        )));
        sleep(ms(400)).await;

        let count = |replies: &[Option<Vec<u8>>]| {
            let n = |tag: &[u8]| replies.iter().filter(|r| r.as_deref() == Some(tag)).count();
            (n(b"OK-A"), n(b"OK-B"), n(b"OK-C"))
        };
        let ask = |n: usize| async move {
            let mut v = Vec::new();
            for _ in 0..n {
                v.push(tcp_ask("127.0.0.1:31050").await);
            }
            v
        };

        // all alive: an even share
        assert_eq!(count(&ask(30).await), (10, 10, 10));

        // B stops answering: once it is down, every connection lands on A or C
        mutes[1].store(true, SeqCst);
        sleep(ms(800)).await;
        let (a, b, c) = count(&ask(30).await);
        assert_eq!((a + c, b), (30, 0), "a={} b={} c={}", a, b, c);
        assert!(a >= 10 && c >= 10, "a={} c={}", a, c);

        // B is back: it gets its share again
        mutes[1].store(false, SeqCst);
        sleep(ms(800)).await;
        assert_eq!(count(&ask(30).await), (10, 10, 10));
    }

    /// Udp associations are balanced too, stay on their peer, and move when it goes down.
    #[tokio::test]
    async fn udp_balance_spreads_sticks_and_moves() {
        let _ = env_logger::try_init();
        let relay = addr("127.0.0.1:31060");
        let (delay_a, hits_a, hits_b) = (
            Arc::new(AtomicU32::new(0)),
            Arc::<AtomicUsize>::default(),
            Arc::<AtomicUsize>::default(),
        );
        tokio::spawn(udp_echo("127.0.0.1:31061", delay_a.clone(), hits_a.clone()));
        tokio::spawn(udp_echo("127.0.0.1:31062", Default::default(), hits_b.clone()));
        tokio::spawn(run_udp(balanced(
            "127.0.0.1:31060",
            &["127.0.0.1:31061", "127.0.0.1:31062"],
            Strategy::RoundRobin,
            opts(100, 150),
        )));
        sleep(ms(300)).await;
        let hits = || (hits_a.load(SeqCst), hits_b.load(SeqCst));

        // four clients, two per peer
        let mut clients = Vec::new();
        for _ in 0..4 {
            let c = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            c.connect(relay).await.unwrap();
            c.send(b"data:1").await.unwrap();
            assert_eq!(udp_recv(&c, 500).await.as_deref(), Some(&b"data:1"[..]));
            clients.push(c);
        }
        assert_eq!(hits(), (2, 2));

        // an association keeps its peer
        for c in &clients {
            c.send(b"data:2").await.unwrap();
            assert_eq!(udp_recv(c, 500).await.as_deref(), Some(&b"data:2"[..]));
        }
        assert_eq!(hits(), (4, 4));

        // A looks dead: its associations move to B, new ones only go to B
        delay_a.store(60_000, SeqCst);
        sleep(ms(800)).await;
        for c in &clients {
            c.send(b"data:3").await.unwrap();
            assert_eq!(udp_recv(c, 500).await.as_deref(), Some(&b"data:3"[..]));
        }
        assert_eq!(hits(), (4, 8));
        for _ in 0..2 {
            let c = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            c.connect(relay).await.unwrap();
            c.send(b"data:4").await.unwrap();
            assert_eq!(udp_recv(&c, 500).await.as_deref(), Some(&b"data:4"[..]));
        }
        assert_eq!(hits(), (4, 10));
    }
}
