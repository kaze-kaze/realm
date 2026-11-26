use std::io::Result;
#[cfg(target_os = "linux")]
use std::sync::Once;
#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use std::time::Duration;
use tokio::net::TcpStream;

pub async fn run_relay(local: TcpStream, remote: TcpStream, timeout: Option<Duration>) -> Result<()> {
    #[cfg(target_os = "linux")]
    return relay_with_pipes(local, remote, timeout, new_checked_pipe, realm_io::Pipe::new).await;

    #[cfg(not(target_os = "linux"))]
    relay_in_memory(local, remote, timeout).await
}

async fn relay_in_memory(mut local: TcpStream, mut remote: TcpStream, timeout: Option<Duration>) -> Result<()> {
    realm_io::bidi_copy_with_timeout(&mut local, &mut remote, timeout)
        .await
        .map(|_| ())
}

#[cfg(target_os = "linux")]
async fn relay_with_pipes(
    mut local: TcpStream,
    mut remote: TcpStream,
    timeout: Option<Duration>,
    make_forward: impl FnOnce() -> Result<realm_io::Pipe>,
    make_reverse: impl FnOnce() -> Result<realm_io::Pipe>,
) -> Result<()> {
    use crate::rate_limit::RateLimit;
    use realm_io::try_bidi_zero_copy_with_pipes;

    let pipes = make_forward().and_then(|forward| make_reverse().map(|reverse| (forward, reverse)));
    match pipes {
        Ok((forward, reverse)) => {
            if try_bidi_zero_copy_with_pipes(&mut local, &mut remote, forward, reverse, timeout)
                .await?
                .is_some()
            {
                return Ok(());
            }
        }
        Err(e) => {
            static PIPE_FAILURE: RateLimit = RateLimit::new();
            if let Some(suppressed) = PIPE_FAILURE.allow() {
                log::warn!(
                    "[tcp]failed to create relay pipes: {}; using memory copy ({} suppressed)",
                    e,
                    suppressed
                );
            }
        }
    }
    relay_in_memory(local, remote, timeout).await
}

#[cfg(target_os = "linux")]
fn new_checked_pipe() -> Result<realm_io::Pipe> {
    static SMALL_PIPE: Once = Once::new();
    static RELAYS: AtomicU32 = AtomicU32::new(0);
    let pipe = realm_io::Pipe::new()?;
    warn_if_small_pipe(&SMALL_PIPE, &RELAYS, realm_io::pipe_size(), || pipe.capacity());
    Ok(pipe)
}

#[cfg(target_os = "linux")]
fn warn_if_small_pipe(warned: &Once, relays: &AtomicU32, requested: usize, capacity: impl FnOnce() -> Result<usize>) {
    const SAMPLE_INTERVAL: u32 = 64;
    if warned.is_completed() || !relays.fetch_add(1, Relaxed).is_multiple_of(SAMPLE_INTERVAL) {
        return;
    }
    if let Ok(actual) = capacity() {
        if actual < requested {
            warned.call_once(|| log::warn!(
                "[tcp]relay pipe capacity is {} bytes (requested {}); run with CAP_SYS_RESOURCE or raise fs.pipe-user-pages-soft",
                actual, requested
            ));
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn pipe_capacity_is_sampled_until_the_first_shrink_warning() {
        let warned = Once::new();
        let relays = AtomicU32::new(0);
        let requested = 65536;
        let check = |actual| {
            let mut queried = false;
            warn_if_small_pipe(&warned, &relays, requested, || {
                queried = true;
                actual
            });
            queried
        };
        assert!(check(Ok(requested)));
        for _ in 1..64 {
            assert!(!check(Ok(requested)));
        }
        assert!(check(Err(std::io::Error::from_raw_os_error(libc::EBADF))));
        assert!(!warned.is_completed());
        for _ in 1..64 {
            assert!(!check(Ok(8192)));
        }
        assert!(check(Ok(8192)));
        assert!(warned.is_completed());

        relays.store(0, Relaxed);
        warn_if_small_pipe(&warned, &relays, requested, || {
            panic!("queried capacity after the warning")
        });
        assert_eq!(relays.load(Relaxed), 0);
    }

    #[tokio::test]
    async fn pipe_creation_failure_falls_back_before_any_data_is_consumed() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut client = TcpStream::connect(addr).await.unwrap();
        let (local, _) = listener.accept().await.unwrap();
        let remote = TcpStream::connect(addr).await.unwrap();
        let (mut server, _) = listener.accept().await.unwrap();
        let mut reverse_failed = false;
        let relay = relay_with_pipes(local, remote, None, realm_io::Pipe::new, || {
            reverse_failed = true;
            Err(std::io::Error::from_raw_os_error(libc::EMFILE))
        });
        let exchange = async {
            client.write_all(b"request").await.unwrap();
            client.shutdown().await.unwrap();
            let mut request = Vec::new();
            server.read_to_end(&mut request).await.unwrap();
            assert_eq!(request, b"request");
            server.write_all(b"reply").await.unwrap();
            server.shutdown().await.unwrap();
            let mut reply = Vec::new();
            client.read_to_end(&mut reply).await.unwrap();
            assert_eq!(reply, b"reply");
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(2), async { tokio::join!(relay, exchange) })
            .await
            .unwrap();
        result.unwrap();
        assert!(reverse_failed);
    }
}

#[cfg(test)]
mod half_close_tests {
    use std::io::Result;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use std::time::Duration;
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream, ReadBuf, duplex};
    use tokio::time::{Instant, sleep, timeout};

    #[tokio::test(start_paused = true)]
    async fn progress_restarts_the_half_close_idle_timer() {
        let (mut client, mut local) = duplex(64);
        let (mut remote, mut server) = duplex(64);
        let grace = Duration::from_millis(100);
        let relay = tokio::spawn(async move {
            realm_io::bidi_copy_with_timeout(&mut local, &mut remote, Some(grace))
                .await
                .unwrap()
        });

        server.write_all(b"a").await.unwrap();
        assert_eq!(client.read_u8().await.unwrap(), b'a');
        client.shutdown().await.unwrap();
        assert_eq!(server.read(&mut [0]).await.unwrap(), 0);
        // Start from a direction that has already forwarded data, then make progress
        // over more than one grace period before letting the connection go idle.
        for byte in b"bc" {
            sleep(Duration::from_millis(70)).await;
            server.write_all(&[*byte]).await.unwrap();
            assert_eq!(client.read_u8().await.unwrap(), *byte);
        }
        let start = Instant::now();
        assert_eq!(client.read(&mut [0]).await.unwrap(), 0);
        assert!((grace..grace + Duration::from_millis(20)).contains(&start.elapsed()));
        assert_eq!(relay.await.unwrap(), (0, 3));
    }

    #[tokio::test(start_paused = true)]
    async fn half_close_timeout_applies_when_the_remaining_writer_is_blocked() {
        let (mut client, mut local) = duplex(1);
        let (mut remote, mut server) = duplex(64);
        let grace = Duration::from_millis(100);
        let relay = tokio::spawn(async move {
            realm_io::bidi_copy_with_timeout(&mut local, &mut remote, Some(grace))
                .await
                .unwrap()
        });
        client.shutdown().await.unwrap();
        assert_eq!(server.read(&mut [0]).await.unwrap(), 0);
        server.write_all(b"reply").await.unwrap();
        let start = Instant::now();
        let copied = timeout(Duration::from_secs(1), relay).await.unwrap().unwrap();
        assert!((grace..grace + Duration::from_millis(20)).contains(&start.elapsed()));
        assert_eq!(copied, (0, 1));
        assert_eq!(client.read_u8().await.unwrap(), b'r');
        assert_eq!(client.read(&mut [0]).await.unwrap(), 0);
    }

    struct StalledWriter {
        stream: DuplexStream,
        stall_flush: bool,
    }

    impl AsyncRead for StalledWriter {
        fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<Result<()>> {
            Pin::new(&mut self.stream).poll_read(cx, buf)
        }
    }

    impl AsyncWrite for StalledWriter {
        fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, data: &[u8]) -> Poll<Result<usize>> {
            Pin::new(&mut self.stream).poll_write(cx, data)
        }

        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<()>> {
            if self.stall_flush {
                Poll::Pending
            } else {
                Poll::Ready(Ok(()))
            }
        }

        fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<()>> {
            Poll::Pending
        }
    }

    #[tokio::test(start_paused = true)]
    async fn half_close_timeout_applies_during_flush_and_shutdown() {
        for stall_flush in [true, false] {
            let (mut client, local) = duplex(64);
            let mut local = StalledWriter {
                stream: local,
                stall_flush,
            };
            let (mut remote, mut server) = duplex(64);
            let grace = Duration::from_millis(100);
            let relay = async {
                realm_io::bidi_copy_with_timeout(&mut local, &mut remote, Some(grace))
                    .await
                    .unwrap()
            };
            let exchange = async {
                client.shutdown().await.unwrap();
                assert_eq!(server.read(&mut [0]).await.unwrap(), 0);
                server.write_all(b"reply").await.unwrap();
                server.shutdown().await.unwrap();
                let mut reply = [0; 5];
                client.read_exact(&mut reply).await.unwrap();
                assert_eq!(&reply, b"reply");
            };
            let start = Instant::now();
            let (copied, ()) = timeout(Duration::from_secs(1), async { tokio::join!(relay, exchange) })
                .await
                .unwrap();
            assert!((grace..grace + Duration::from_millis(20)).contains(&start.elapsed()));
            assert_eq!(copied, (0, 5));
        }
    }
}
