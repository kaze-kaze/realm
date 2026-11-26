use std::io::{Result, Error, ErrorKind};
use std::net::{SocketAddr, TcpListener};
use std::time::Duration;
use std::future::Future;

use realm_syscall::socket2::Socket;
use tokio::net::{TcpSocket, TcpStream};
use tokio::time::{Instant, timeout_at};

use crate::dns::resolve_addr;
use crate::time::timeoutfut;
use crate::endpoint::{RemoteAddr, BindOpts, ConnectOpts};
use crate::failover::{Failover, timed_out};

fn new_socket(addr: &SocketAddr, mptcp: bool) -> Result<Socket> {
    #[cfg(target_os = "linux")]
    {
        let call = if mptcp {
            realm_syscall::new_mptcp_socket
        } else {
            realm_syscall::new_tcp_socket
        };
        call(addr)
    }

    #[cfg(not(target_os = "linux"))]
    {
        realm_syscall::new_tcp_socket(addr)
    }
}

pub fn bind(laddr: &SocketAddr, bind_opts: BindOpts) -> Result<TcpListener> {
    let BindOpts {
        accept_mptcp,
        ipv6_only,
        bind_interface,
        ..
    } = bind_opts;
    let socket = new_socket(laddr, accept_mptcp)?;

    // ipv6_only
    if let SocketAddr::V6(_) = laddr {
        socket.set_only_v6(ipv6_only)?;
    }

    // bind interface
    #[cfg(target_os = "linux")]
    if let Some(iface) = bind_interface {
        realm_syscall::bind_to_device(&socket, &iface)?;
    }

    // Winsock also allows a second listener when address reuse is set.
    #[cfg(unix)]
    let _ = socket.set_reuse_address(true);

    socket.bind(&(*laddr).into())?;
    socket.listen(1024)?;

    Ok(socket.into())
}

/// Create an outbound socket for `addr` with the configured egress options.
pub fn prepare(
    addr: &SocketAddr,
    conn_opts: &ConnectOpts,
    keepalive: Option<&keepalive::TcpKeepalive>,
) -> Result<TcpSocket> {
    let ConnectOpts {
        send_mptcp,
        bind_address,

        #[cfg(target_os = "linux")]
        bind_interface,
        ..
    } = conn_opts;

    let socket = new_socket(addr, *send_mptcp)?;

    // ignore error
    let _ = socket.set_tcp_nodelay(true);
    let _ = socket.set_reuse_address(true);

    if let Some(addr) = *bind_address {
        socket.bind(&addr.into())?;
    }

    #[cfg(target_os = "linux")]
    if let Some(iface) = bind_interface {
        realm_syscall::bind_to_device(&socket, iface)?;
    }

    if let Some(kpa) = keepalive {
        socket.set_tcp_keepalive(kpa)?;
    }

    #[cfg(target_os = "linux")]
    if conn_opts.tcp_user_timeout > 0 {
        set_tcp_user_timeout(&socket, conn_opts.tcp_user_timeout);
    } else if keepalive.is_some() && conn_opts.failover.is_some() {
        // Give up on unacknowledged data after the same time as an idle peer.
        let _ = socket.set_tcp_user_timeout(Some(keepalive::dead_after(conn_opts)));
    }

    Ok(TcpSocket::from_std_stream(socket.into()))
}

#[cfg(target_os = "linux")]
pub fn set_tcp_user_timeout(socket: &Socket, seconds: usize) {
    use crate::rate_limit::RateLimit;

    static USER_TIMEOUT_FAILURE: RateLimit = RateLimit::new();
    if let Err(e) = socket.set_tcp_user_timeout(Some(Duration::from_secs(seconds as u64))) {
        if let Some(suppressed) = USER_TIMEOUT_FAILURE.allow() {
            log::warn!(
                "[tcp]failed to set TCP_USER_TIMEOUT to {}s: {} ({} suppressed)",
                seconds,
                e,
                suppressed
            );
        }
    }
}

pub async fn connect(raddr: &RemoteAddr, conn_opts: &ConnectOpts) -> Result<TcpStream> {
    connect_addrs(raddr, resolve_addr(raddr).await?.iter(), conn_opts, TcpSocket::connect).await
}

async fn connect_addrs<F>(
    raddr: &RemoteAddr,
    addresses: impl IntoIterator<Item = SocketAddr>,
    conn_opts: &ConnectOpts,
    mut dial: impl FnMut(TcpSocket, SocketAddr) -> F,
) -> Result<TcpStream>
where
    F: Future<Output = Result<TcpStream>>,
{
    use crate::rate_limit::RateLimit;

    static DIAL_FAILURE: RateLimit = RateLimit::new();
    let connect_timeout = conn_opts.connect_timeout;

    let mut last_err = None;
    let keepalive = keepalive::build(conn_opts);

    for addr in addresses {
        log::debug!("[tcp]{} resolved as {}", raddr, addr);

        let (err, deadline_elapsed) = match prepare(&addr, conn_opts, keepalive.as_ref()) {
            Ok(socket) => match timeoutfut(dial(socket, addr), connect_timeout).await {
                Ok(Ok(stream)) => {
                    log::debug!("[tcp]connect to {} as {}", raddr, addr);
                    return Ok(stream);
                }
                Ok(Err(e)) => (e, false),
                Err(e) => (e, true),
            },
            Err(e) => (e, false),
        };
        if let Some(suppressed) = DIAL_FAILURE.allow() {
            log::warn!(
                "[tcp]connect to {} as {}: {}, try next ip ({} suppressed)",
                raddr,
                addr,
                err,
                suppressed
            );
        }
        // Keep a concrete socket error when another address only exhausted its deadline.
        if last_err.is_none() || !deadline_elapsed {
            last_err = Some(err);
        }
    }

    Err(last_err.unwrap_or_else(|| Error::new(ErrorKind::InvalidInput, "could not connect to any address")))
}

/// Connect to a failover peer: the active one, or `want`, the balancer's choice.
///
/// A failed dial is retried on other ready peers, at most three dials in all, on
/// the same client connection. The peers to retry on are looked up when
/// they are needed: a dial that timed out took long enough for the probes to
/// have learned more. `connect_timeout` (0 = none) covers all dials together; a
/// dial that can be retried elsewhere gets half of what is left of it, so a retry
/// always has time. Nothing has been read from the client at this point, so
/// retrying is lossless.
pub async fn connect_failover(
    fo: &Failover,
    conn_opts: &ConnectOpts,
    want: Option<usize>,
) -> Result<(TcpStream, usize)> {
    /// The first peer and two more: enough for two peers failing at once.
    const MAX_DIALS: usize = 3;

    let keepalive = keepalive::build(conn_opts);
    let total = Duration::from_secs(conn_opts.connect_timeout as u64);
    let end = Instant::now() + total;

    let mut tried = [0usize; MAX_DIALS];
    let mut dials = 0;
    let mut peer = want.unwrap_or_else(|| fo.snapshot().active());
    loop {
        tried[dials] = peer;
        dials += 1;
        let retry = |tried: &[usize]| {
            if dials < MAX_DIALS {
                fo.retry_peer(fo.snapshot(), tried, want.is_some())
            } else {
                None
            }
        };

        let deadline = (!total.is_zero()).then(|| {
            let now = Instant::now();
            let left = end.saturating_duration_since(now);
            now + if retry(&tried[..dials]).is_some() {
                left / 2
            } else {
                left
            }
        });
        let err = match dial(fo.peer(peer), conn_opts, keepalive.as_ref(), deadline).await {
            Ok(stream) => return Ok((stream, peer)),
            Err(e) => e,
        };

        let Some(next) = retry(&tried[..dials]) else {
            return Err(err);
        };
        log::warn!(
            "[tcp]connect to peer#{} {}: {}, retry peer#{} {}",
            peer,
            fo.peer(peer),
            err,
            next,
            fo.peer(next)
        );
        peer = next;
    }
}

async fn dial(
    addr: SocketAddr,
    conn_opts: &ConnectOpts,
    keepalive: Option<&keepalive::TcpKeepalive>,
    deadline: Option<Instant>,
) -> Result<TcpStream> {
    let socket = prepare(&addr, conn_opts, keepalive)?;
    match deadline {
        Some(deadline) => timeout_at(deadline, socket.connect(addr))
            .await
            .map_err(|_| timed_out())?,
        None => socket.connect(addr).await,
    }
}

pub(crate) mod keepalive {
    use super::*;
    pub use realm_syscall::socket2::{SockRef, TcpKeepalive};
    pub fn build(conn_opts: &ConnectOpts) -> Option<TcpKeepalive> {
        let ConnectOpts { tcp_keepalive, .. } = conn_opts;
        if *tcp_keepalive == 0 {
            return None;
        };
        let secs = Duration::from_secs(*tcp_keepalive as u64);
        let mut kpa = TcpKeepalive::new().with_time(secs);
        #[cfg(not(target_os = "openbsd"))]
        {
            kpa = TcpKeepalive::with_interval(kpa, secs);
        }
        #[cfg(not(target_os = "openbsd"))]
        {
            kpa = TcpKeepalive::with_retries(kpa, probes(conn_opts));
        }

        Some(kpa)
    }

    /// Keepalive probes before the peer counts as dead, at least one: linux
    /// refuses a count of zero, and the socket option with it, so no connection
    /// could be made at all.
    fn probes(conn_opts: &ConnectOpts) -> u32 {
        (conn_opts.tcp_keepalive_probe as u32).max(1)
    }

    /// Time until keepalive declares an idle peer dead: one idle period plus all probes.
    #[cfg(target_os = "linux")]
    pub fn dead_after(conn_opts: &ConnectOpts) -> Duration {
        Duration::from_secs(conn_opts.tcp_keepalive as u64) * (probes(conn_opts) + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connect_tries_another_address_if_socket_preparation_fails() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let wrong_family = SocketAddr::new(std::net::Ipv6Addr::LOCALHOST.into(), addr.port());
        let opts = ConnectOpts {
            bind_address: Some("127.0.0.1:0".parse().unwrap()),
            ..Default::default()
        };
        assert!(prepare(&wrong_family, &opts, None).is_err());
        let stream = tokio::time::timeout(
            Duration::from_secs(1),
            connect_addrs(
                &RemoteAddr::SocketAddr(addr),
                [wrong_family, addr],
                &opts,
                TcpSocket::connect,
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(stream.peer_addr().unwrap(), addr);
    }

    #[tokio::test(start_paused = true)]
    async fn connect_returns_timed_out_when_every_dial_times_out() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let opts = ConnectOpts {
            connect_timeout: 1,
            ..Default::default()
        };
        let mut dials = 0;
        let err = connect_addrs(&RemoteAddr::SocketAddr(addr), [addr, addr], &opts, |_, _| {
            dials += 1;
            std::future::pending()
        })
        .await
        .unwrap_err();
        assert_eq!(dials, 2);
        assert_eq!(err.kind(), ErrorKind::TimedOut);
    }

    #[tokio::test(start_paused = true)]
    async fn connect_reports_a_socket_error_when_another_dial_times_out() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let wrong_family = SocketAddr::new(std::net::Ipv6Addr::LOCALHOST.into(), addr.port());
        let opts = ConnectOpts {
            connect_timeout: 1,
            bind_address: Some("127.0.0.1:0".parse().unwrap()),
            ..Default::default()
        };
        let expected = prepare(&wrong_family, &opts, None).unwrap_err().kind();
        let err = connect_addrs(&RemoteAddr::SocketAddr(addr), [wrong_family, addr], &opts, |_, _| {
            std::future::pending()
        })
        .await
        .unwrap_err();
        assert_eq!(err.kind(), expected);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn explicit_tcp_user_timeout_is_independent_of_keepalive() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let timeout_on_socket = |keepalive_enabled, tcp_user_timeout| {
            let opts = ConnectOpts {
                tcp_keepalive: if keepalive_enabled { 5 } else { 0 },
                tcp_keepalive_probe: 2,
                failover: Some(Default::default()),
                tcp_user_timeout,
                ..Default::default()
            };
            let keepalive = keepalive::build(&opts);
            let socket = prepare(&addr, &opts, keepalive.as_ref()).unwrap();
            keepalive::SockRef::from(&socket).tcp_user_timeout().unwrap()
        };
        assert_eq!(timeout_on_socket(false, 21), Some(Duration::from_secs(21)));
        assert_eq!(timeout_on_socket(true, 21), Some(Duration::from_secs(21)));
        assert_eq!(timeout_on_socket(true, 0), Some(Duration::from_secs(15)));
        assert_eq!(timeout_on_socket(false, 0), None);

        let _client = TcpStream::connect(addr).await.unwrap();
        let (accepted, _) = listener.accept().await.unwrap();
        let socket = keepalive::SockRef::from(&accepted);
        set_tcp_user_timeout(&socket, 21);
        assert_eq!(socket.tcp_user_timeout().unwrap(), Some(Duration::from_secs(21)));
    }
}
