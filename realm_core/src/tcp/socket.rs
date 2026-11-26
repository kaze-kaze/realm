use std::io::{Result, Error, ErrorKind};
use std::net::{SocketAddr, TcpListener};
use std::time::Duration;

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

    // ignore error
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

        // Keepalive only detects a dead peer on an idle connection. With data in
        // flight the kernel keeps retransmitting for about 15 minutes, and the client
        // never reconnects to the peer that took over. Give up after the same time
        // keepalive would. This also ends a connection whose peer stops reading for
        // that long while data is pending. Best effort, e.g. not on mptcp sockets.
        #[cfg(target_os = "linux")]
        if conn_opts.failover.is_some() {
            let _ = socket.set_tcp_user_timeout(Some(keepalive::dead_after(conn_opts)));
        }
    }

    Ok(TcpSocket::from_std_stream(socket.into()))
}

pub async fn connect(raddr: &RemoteAddr, conn_opts: &ConnectOpts) -> Result<TcpStream> {
    let connect_timeout = conn_opts.connect_timeout;

    let mut last_err = None;
    let keepalive = keepalive::build(conn_opts);

    for addr in resolve_addr(raddr).await?.iter() {
        log::debug!("[tcp]{} resolved as {}", raddr, addr);

        let socket = prepare(&addr, conn_opts, keepalive.as_ref())?;

        match timeoutfut(socket.connect(addr), connect_timeout).await {
            Ok(Ok(stream)) => {
                log::debug!("[tcp]connect to {} as {}", raddr, addr,);
                return Ok(stream);
            }
            Ok(Err(e)) => {
                log::warn!("[tcp]connect to {} as {}: {}, try next ip", raddr, addr, e);
                last_err = Some(e);
            }
            Err(_) => log::warn!("[tcp]connect to {} as {} timeout, try next ip", raddr, addr),
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
