use std::io::{Result, Error, ErrorKind};
use std::net::SocketAddr;
use std::time::Duration;

use realm_syscall::socket2::Socket;
use tokio::net::{TcpSocket, TcpStream, TcpListener};
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

    TcpListener::from_std(socket.into())
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

/// Dial the active failover peer, or with `balance` the peer picked for `want`; if
/// that fails, retry once on the next ready peer.
///
/// Both attempts share one deadline of `connect_timeout` (0 = none). When a retry is
/// possible the first attempt only gets half of it, so the retry always has time left.
/// Nothing has been read from the client at this point, so retrying is lossless.
pub async fn connect_failover(
    fo: &Failover,
    conn_opts: &ConnectOpts,
    want: Option<usize>,
) -> Result<(TcpStream, usize)> {
    let snap = fo.snapshot();
    let (first, next) = match want {
        Some(want) => {
            let first = fo.pick(snap, want);
            (first, fo.next_ready(snap, first))
        }
        None => {
            let first = snap.active();
            (first, fo.fallback(snap, first))
        }
    };

    let keepalive = keepalive::build(conn_opts);
    let total = Duration::from_secs(conn_opts.connect_timeout as u64);
    let start = Instant::now();
    let deadline = |budget: Duration| (!total.is_zero()).then(|| start + budget);

    let budget = if next.is_some() { total / 2 } else { total };
    let err = match dial(fo.peer(first), conn_opts, keepalive.as_ref(), deadline(budget)).await {
        Ok(stream) => return Ok((stream, first)),
        Err(e) => e,
    };

    let Some(next) = next else { return Err(err) };
    log::warn!(
        "[tcp]connect to peer#{} {}: {}, retry peer#{} {}",
        first,
        fo.peer(first),
        err,
        next,
        fo.peer(next)
    );
    dial(fo.peer(next), conn_opts, keepalive.as_ref(), deadline(total))
        .await
        .map(|stream| (stream, next))
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
        let ConnectOpts {
            tcp_keepalive,
            tcp_keepalive_probe,
            ..
        } = conn_opts;
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
            let probe = *tcp_keepalive_probe as u32;
            kpa = TcpKeepalive::with_retries(kpa, probe);
        }

        Some(kpa)
    }

    /// Time until keepalive declares an idle peer dead: one idle period plus all probes.
    #[cfg(target_os = "linux")]
    pub fn dead_after(conn_opts: &ConnectOpts) -> Duration {
        Duration::from_secs(conn_opts.tcp_keepalive as u64) * (conn_opts.tcp_keepalive_probe as u32 + 1)
    }
}
