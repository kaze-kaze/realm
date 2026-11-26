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
use std::net::{SocketAddr, TcpListener};
use std::time::Duration;
use tokio::net::TcpStream;

use crate::trick::Ref;
use crate::endpoint::Endpoint;
use crate::failover::{Failover, Proto, literal_peers};

use middle::connect_and_relay;
use socket::keepalive::{SockRef, TcpKeepalive};

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
        ..
    } = endpoint;

    let keepalive = socket::keepalive::build(&conn_opts);
    #[cfg(target_os = "linux")]
    let tcp_user_timeout = conn_opts.tcp_user_timeout;

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
        let accepted = accept(
            &lis,
            keepalive.as_ref(),
            #[cfg(target_os = "linux")]
            tcp_user_timeout,
        )
        .await;
        let (local, addr) = match accepted {
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

        tokio::spawn(async move {
            match connect_and_relay(local, raddr, conn_opts, extra_raddrs, failover).await {
                Ok(..) => log::debug!("[tcp]{} => {}, finish", addr, raddr.as_ref()),
                Err(e) => log::error!("[tcp]{} => {}, error: {}", addr, raddr.as_ref(), e),
            }
        });
    }
}

async fn accept(
    lis: &tokio::net::TcpListener,
    keepalive: Option<&TcpKeepalive>,
    #[cfg(target_os = "linux")] tcp_user_timeout: usize,
) -> Result<(TcpStream, SocketAddr)> {
    let (local, addr) = lis.accept().await?;
    let _ = local.set_nodelay(true);
    if let Some(kpa) = keepalive {
        if let Err(e) = SockRef::from(&local).set_tcp_keepalive(kpa) {
            log::warn!("[tcp]failed to set keepalive for {}: {}", addr, e);
        }
    }

    #[cfg(target_os = "linux")]
    if tcp_user_timeout > 0 {
        socket::set_tcp_user_timeout(&SockRef::from(&local), tcp_user_timeout);
    }
    Ok((local, addr))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::endpoint::ConnectOpts;
    use tokio::time::timeout;

    #[tokio::test]
    async fn accepted_socket_uses_connect_options_for_tcp_user_timeout() {
        let lis = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        for (seconds, expected) in [(0, None), (21, Some(Duration::from_secs(21)))] {
            let opts = ConnectOpts {
                tcp_user_timeout: seconds,
                ..Default::default()
            };
            let _client = TcpStream::connect(lis.local_addr().unwrap()).await.unwrap();
            let (accepted, _) = timeout(
                Duration::from_secs(1),
                accept(&lis, socket::keepalive::build(&opts).as_ref(), opts.tcp_user_timeout),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(SockRef::from(&accepted).tcp_user_timeout().unwrap(), expected);
        }
    }
}
