use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::time::{sleep, timeout};

use realm_core::udp::{self, run_udp};
use realm_core::endpoint::{Endpoint, RemoteAddr};

#[tokio::test]
async fn udp() {
    let _ = env_logger::try_init();
    let endpoint = Endpoint {
        laddr: "127.0.0.1:10000".parse().unwrap(),
        raddr: "127.0.0.1:20000"
            .parse::<SocketAddr>()
            .map(RemoteAddr::SocketAddr)
            .unwrap(),
        conn_opts: Default::default(),
        bind_opts: Default::default(),
        extra_raddrs: Vec::new(),
    };

    tokio::spawn(run_udp(endpoint));

    let task1 = async {
        sleep(Duration::from_millis(500)).await;

        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();

        let mut buf = vec![0; 32];
        let peer: SocketAddr = "127.0.0.1:10000".parse().unwrap();

        for _ in 0..20 {
            socket.send_to(b"Ping Ping Ping", &peer).await.unwrap();
            let (n, peer2) = socket.recv_from(&mut buf).await.unwrap();
            assert_eq!(peer, peer2);
            log::debug!("a got: {:?}", std::str::from_utf8(&buf[..n]).unwrap());
            assert_eq!(b"Pong Pong Pong", &buf[..n]);
        }
    };

    let task2 = async {
        let socket = UdpSocket::bind("127.0.0.1:20000").await.unwrap();

        let mut buf = vec![0; 32];

        for _ in 0..20 {
            let (n, peer) = socket.recv_from(&mut buf).await.unwrap();
            log::debug!("b got: {:?}", std::str::from_utf8(&buf[..n]).unwrap());
            assert_eq!(b"Ping Ping Ping", &buf[..n]);
            socket.send_to(b"Pong Pong Pong", peer).await.unwrap();
        }
    };

    tokio::join!(task1, task2);
}

fn pattern(len: usize, tag: u8) -> Vec<u8> {
    (0..len).map(|i| (i as u8).wrapping_mul(31).wrapping_add(tag)).collect()
}

/// Datagrams longer than one ethernet frame are relayed whole, in both directions.
#[tokio::test]
async fn udp_large_datagrams() {
    let _ = env_logger::try_init();
    let relay: SocketAddr = "127.0.0.1:10001".parse().unwrap();
    let echo = UdpSocket::bind("127.0.0.1:20001").await.unwrap();

    tokio::spawn(run_udp(Endpoint {
        laddr: relay,
        raddr: RemoteAddr::SocketAddr("127.0.0.1:20001".parse().unwrap()),
        conn_opts: Default::default(),
        bind_opts: Default::default(),
        extra_raddrs: Vec::new(),
    }));
    tokio::spawn(async move {
        let mut buf = vec![0u8; 1 << 16];
        loop {
            let (n, peer) = echo.recv_from(&mut buf).await.unwrap();
            echo.send_to(&buf[..n], peer).await.unwrap();
        }
    });
    sleep(Duration::from_millis(500)).await;

    let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let mut buf = vec![0u8; 1 << 16];

    // one at a time: around the inline size, then far beyond it
    let mut sizes = vec![1, 1472, 1499, 1500, 1501, 1600, 4000, 9000];
    if cfg!(target_os = "linux") {
        // the largest udp payload; other systems cap datagrams on loopback well below
        sizes.push(65507);
    }
    for len in sizes {
        let msg = pattern(len, 7);
        sock.send_to(&msg, relay).await.unwrap();
        let (n, _) = timeout(Duration::from_secs(2), sock.recv_from(&mut buf))
            .await
            .unwrap_or_else(|_| panic!("no reply for {} bytes", len))
            .unwrap();
        assert_eq!(n, len);
        assert!(buf[..n] == msg[..], "{} bytes came back altered", len);
    }

    // several large ones in a row share a batch
    burst(&sock, relay, 3).await;
}

/// Large datagrams as the very first traffic of an endpoint, many at once: none is lost
/// or cut while buffers adapt, because there is nothing to adapt.
#[tokio::test]
async fn udp_large_burst_first() {
    let _ = env_logger::try_init();
    let relay: SocketAddr = "127.0.0.1:10002".parse().unwrap();
    let echo = UdpSocket::bind("127.0.0.1:20002").await.unwrap();

    tokio::spawn(run_udp(Endpoint {
        laddr: relay,
        raddr: RemoteAddr::SocketAddr("127.0.0.1:20002".parse().unwrap()),
        conn_opts: Default::default(),
        bind_opts: Default::default(),
        extra_raddrs: Vec::new(),
    }));
    tokio::spawn(async move {
        let mut buf = vec![0u8; 1 << 16];
        loop {
            let (n, peer) = echo.recv_from(&mut buf).await.unwrap();
            echo.send_to(&buf[..n], peer).await.unwrap();
        }
    });
    sleep(Duration::from_millis(500)).await;

    let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    burst(&sock, relay, 2).await;
}

/// Send bursts of 5000 byte datagrams and require every one back, unaltered.
async fn burst(sock: &UdpSocket, relay: SocketAddr, rounds: u8) {
    let mut buf = vec![0u8; 1 << 16];
    for round in 0..rounds {
        const BURST: u8 = 12;
        for i in 0..BURST {
            sock.send_to(&pattern(5000, round * BURST + i), relay).await.unwrap();
        }
        let mut seen = [false; BURST as usize];
        for _ in 0..BURST {
            let (n, _) = timeout(Duration::from_secs(2), sock.recv_from(&mut buf))
                .await
                .unwrap_or_else(|_| panic!("round {}: missing replies, got {:?}", round, seen))
                .unwrap();
            assert_eq!(n, 5000);
            let i = buf[0].wrapping_sub(round * BURST);
            assert!(i < BURST && buf[..n] == pattern(5000, round * BURST + i)[..]);
            seen[i as usize] = true;
        }
        assert!(seen.iter().all(|x| *x));
    }
}

#[tokio::test]
async fn udp_upload_to_a_silent_upstream_keeps_the_source_port_until_idle() {
    let upstream = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        laddr: "127.0.0.1:0".parse().unwrap(),
        raddr: RemoteAddr::SocketAddr(upstream.local_addr().unwrap()),
        conn_opts: realm_core::endpoint::ConnectOpts {
            associate_timeout: 1,
            ..Default::default()
        },
        bind_opts: Default::default(),
        extra_raddrs: Vec::new(),
    };
    let listener = udp::bind(&endpoint).unwrap();
    let relay = listener.local_addr().unwrap();
    let task = tokio::spawn(udp::run(listener, endpoint));
    let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let mut source = None;
    let mut buf = [0; 1];
    for i in 0..=16 {
        if i > 0 {
            sleep(Duration::from_millis(200)).await;
        }
        client.send_to(&[i], relay).await.unwrap();
        let (n, from) = timeout(Duration::from_millis(750), upstream.recv_from(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert_eq!((n, buf[0]), (1, i));
        assert_eq!(from, *source.get_or_insert(from));
    }

    // The upstream port is exclusive until the association is dropped.
    let source = source.unwrap();
    assert_eq!(
        std::net::UdpSocket::bind(source).unwrap_err().kind(),
        std::io::ErrorKind::AddrInUse
    );
    timeout(Duration::from_millis(1600), async {
        loop {
            match std::net::UdpSocket::bind(source) {
                Ok(_) => break,
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => sleep(Duration::from_millis(25)).await,
                Err(e) => panic!("failed to check upstream port {source}: {e}"),
            }
        }
    })
    .await
    .expect("idle association held its upstream port");
    task.abort();
}
