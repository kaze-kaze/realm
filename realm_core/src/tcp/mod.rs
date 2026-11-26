//! TCP relay entrance.

pub(crate) mod socket;
mod middle;
mod plain;

#[cfg(feature = "hook")]
mod hook;

#[cfg(feature = "proxy")]
mod proxy;

#[cfg(feature = "transport")]
mod transport;

use std::io::{ErrorKind, Result};
use std::net::TcpListener;
use std::time::Duration;

use crate::trick::Ref;
use crate::endpoint::Endpoint;
use crate::failover::{Failover, Proto, literal_peers};

use middle::connect_and_relay;

/// Pause after a failed accept, doubled on each consecutive failure.
const ACCEPT_BACKOFF_MIN: Duration = Duration::from_millis(10);
const ACCEPT_BACKOFF_MAX: Duration = Duration::from_secs(1);

/// Bind a TCP relay to its listening address without starting the runtime.
pub fn bind(endpoint: &Endpoint) -> Result<TcpListener> {
    socket::bind(&endpoint.laddr, endpoint.bind_opts.clone())
}

/// Bind and run a TCP relay.
pub async fn run_tcp(endpoint: Endpoint) -> Result<()> {
    let lis = bind(&endpoint)?;
    run(lis, endpoint).await
}

/// Run a TCP relay on a nonblocking listener created by [`bind`].
pub async fn run(lis: TcpListener, endpoint: Endpoint) -> Result<()> {
    let lis = tokio::net::TcpListener::from_std(lis)?;
    let Endpoint {
        laddr,
        raddr,
        conn_opts,
        extra_raddrs,
        #[cfg(target_os = "linux")]
        bind_opts,
        ..
    } = endpoint;

    let keepalive = socket::keepalive::build(&conn_opts);

    let failover = conn_opts.failover.as_ref().map(|opts| {
        let peers = literal_peers(&raddr, &extra_raddrs);
        Failover::start(Proto::Tcp, laddr.to_string(), peers, opts, &conn_opts)
    });

    let raddr = Ref::new(&raddr);
    let conn_opts = Ref::new(&conn_opts);
    let extra_raddrs = Ref::new(&extra_raddrs);
    let failover = Ref::new(&failover);

    let mut backoff = ACCEPT_BACKOFF_MIN;
    loop {
        let (local, addr) = match lis.accept().await {
            Ok(x) => x,
            Err(e) if e.kind() == ErrorKind::ConnectionAborted => {
                log::warn!("[tcp]failed to accept: {}", e);
                continue;
            }
            // e.g. out of file descriptors: the listener itself is fine, so keep it
            // and retry once established connections have released resources
            Err(e) => {
                log::error!("[tcp]failed to accept: {}, retry in {:?}", e, backoff);
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(ACCEPT_BACKOFF_MAX);
                continue;
            }
        };
        backoff = ACCEPT_BACKOFF_MIN;

        // ignore error
        let _ = local.set_nodelay(true);
        // set tcp_keepalive, a failure only affects this connection
        if let Some(kpa) = &keepalive {
            use socket::keepalive::SockRef;
            if let Err(e) = SockRef::from(&local).set_tcp_keepalive(kpa) {
                log::warn!("[tcp]failed to set keepalive for {}: {}", addr, e);
            }
        }

        #[cfg(target_os = "linux")]
        if bind_opts.tcp_user_timeout > 0 {
            use socket::keepalive::SockRef;
            socket::set_tcp_user_timeout(&SockRef::from(&local), bind_opts.tcp_user_timeout);
        }

        tokio::spawn(async move {
            match connect_and_relay(local, raddr, conn_opts, extra_raddrs, failover).await {
                Ok(..) => log::debug!("[tcp]{} => {}, finish", addr, raddr.as_ref()),
                Err(e) => log::error!("[tcp]{} => {}, error: {}", addr, raddr.as_ref(), e),
            }
        });
    }
}
