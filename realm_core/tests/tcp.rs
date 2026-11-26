use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::{TcpStream, TcpListener};
use tokio::time::sleep;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use realm_core::tcp::run_tcp;
use realm_core::endpoint::{Endpoint, RemoteAddr};

#[tokio::test]
async fn tcp() {
    env_logger::init();
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

    tokio::spawn(run_tcp(endpoint));

    let task1 = async {
        sleep(Duration::from_millis(500)).await;
        let mut stream = TcpStream::connect("127.0.0.1:10000").await.unwrap();

        let mut buf = vec![0; 32];

        for _ in 0..20 {
            stream.write_all(b"Ping Ping Ping").await.unwrap();
            let n = stream.read(&mut buf).await.unwrap();
            log::debug!("a got: {:?}", std::str::from_utf8(&buf[..n]).unwrap());
            assert_eq!(b"Pong Pong Pong", &buf[..n]);
        }
    };

    let task2 = async {
        let lis = TcpListener::bind("127.0.0.1:20000").await.unwrap();
        let (mut stream, _) = lis.accept().await.unwrap();

        let mut buf = vec![0; 32];

        for _ in 0..20 {
            let n = stream.read(&mut buf).await.unwrap();
            log::debug!("b got: {:?}", std::str::from_utf8(&buf[..n]).unwrap());
            assert_eq!(b"Ping Ping Ping", &buf[..n]);
            stream.write_all(b"Pong Pong Pong").await.unwrap();
        }
    };

    tokio::join!(task1, task2);
}

async fn connect_through_relay(
    half_close_timeout: usize,
) -> (TcpStream, TcpStream, tokio::task::JoinHandle<std::io::Result<()>>) {
    use realm_core::tcp;
    use realm_core::endpoint::ConnectOpts;

    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        laddr: "127.0.0.1:0".parse().unwrap(),
        raddr: RemoteAddr::SocketAddr(upstream.local_addr().unwrap()),
        conn_opts: ConnectOpts {
            half_close_timeout: Some(half_close_timeout),
            ..Default::default()
        },
        bind_opts: Default::default(),
        extra_raddrs: Vec::new(),
    };
    let listener = tcp::bind(&endpoint).unwrap();
    let relay = listener.local_addr().unwrap();
    let task = tokio::spawn(tcp::run(listener, endpoint));
    let client = TcpStream::connect(relay).await.unwrap();
    let (server, _) = upstream.accept().await.unwrap();
    (client, server, task)
}

#[tokio::test]
async fn client_half_close_still_allows_a_delayed_reply() {
    tokio::time::timeout(Duration::from_secs(3), async {
        let (mut client, mut server, task) = connect_through_relay(2).await;
        client.write_all(b"request").await.unwrap();
        client.shutdown().await.unwrap();
        let mut request = Vec::new();
        server.read_to_end(&mut request).await.unwrap();
        assert_eq!(request, b"request");

        sleep(Duration::from_millis(50)).await;
        let reply = vec![0x5a; 128 * 1024];
        let respond = async {
            server.write_all(&reply).await.unwrap();
            server.shutdown().await.unwrap();
        };
        let mut received = Vec::new();
        tokio::join!(respond, async { client.read_to_end(&mut received).await.unwrap() });
        assert_eq!(received, reply);
        task.abort();
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn server_half_close_delivers_the_reply_and_allows_the_upload_to_finish() {
    tokio::time::timeout(Duration::from_secs(3), async {
        let (mut client, mut server, task) = connect_through_relay(2).await;
        let reply = vec![0x5a; 1024 * 1024];
        let upload_tail = vec![0x31; 256 * 1024];
        let serve = async {
            server.write_all(&reply).await.unwrap();
            server.shutdown().await.unwrap();
            let mut upload = Vec::new();
            server.read_to_end(&mut upload).await.unwrap();
            assert_eq!(&upload[..4], b"head");
            assert_eq!(&upload[4..], upload_tail);
        };
        let exchange = async {
            client.write_all(b"head").await.unwrap();
            let mut received = Vec::new();
            client.read_to_end(&mut received).await.unwrap();
            assert_eq!(received, reply);
            client.write_all(&upload_tail).await.unwrap();
            client.shutdown().await.unwrap();
        };
        tokio::join!(serve, exchange);
        task.abort();
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn half_close_expiration_closes_the_idle_direction() {
    tokio::time::timeout(Duration::from_secs(3), async {
        let (mut client, mut server, task) = connect_through_relay(1).await;
        let start = tokio::time::Instant::now();
        client.shutdown().await.unwrap();
        assert_eq!(
            server.read_u8().await.unwrap_err().kind(),
            std::io::ErrorKind::UnexpectedEof
        );
        assert_eq!(
            client.read_u8().await.unwrap_err().kind(),
            std::io::ErrorKind::UnexpectedEof
        );
        assert!(start.elapsed() >= Duration::from_secs(1));
        task.abort();
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn zero_half_close_timeout_closes_both_directions_immediately() {
    tokio::time::timeout(Duration::from_secs(1), async {
        let (mut client, mut server, task) = connect_through_relay(0).await;
        client.shutdown().await.unwrap();
        assert_eq!(
            server.read_u8().await.unwrap_err().kind(),
            std::io::ErrorKind::UnexpectedEof
        );
        assert_eq!(
            client.read_u8().await.unwrap_err().kind(),
            std::io::ErrorKind::UnexpectedEof
        );
        task.abort();
    })
    .await
    .unwrap();
}
