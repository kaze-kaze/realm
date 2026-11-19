use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::time::{sleep, timeout};

use realm_core::udp::run_udp;
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
    for round in 0..3u8 {
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
