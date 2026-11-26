//! UDP relay entrance.

pub(crate) mod socket;
mod sockmap;
mod middle;
mod batched;

use std::io::Result;
use std::net::UdpSocket;
use std::sync::Arc;

use crate::trick::Ref;
use crate::endpoint::Endpoint;
use crate::failover::{Failover, Proto, literal_peers};

use sockmap::SockMap;
use middle::associate_and_relay;
use crate::rate_limit::RateLimit;

/// Bind a UDP relay to its listening address without starting the runtime.
pub fn bind(endpoint: &Endpoint) -> Result<UdpSocket> {
    socket::bind(&endpoint.laddr, endpoint.bind_opts.clone())
}

/// Bind and run a UDP relay.
pub async fn run_udp(endpoint: Endpoint) -> Result<()> {
    let lis = bind(&endpoint)?;
    run(lis, endpoint).await
}

/// Run a UDP relay on a nonblocking socket created by [`bind`].
pub async fn run(lis: UdpSocket, endpoint: Endpoint) -> Result<()> {
    let lis = tokio::net::UdpSocket::from_std(lis)?;
    let Endpoint {
        laddr,
        raddr,
        conn_opts,
        extra_raddrs,
        ..
    } = endpoint;

    let sockmap = Arc::new(SockMap::new());

    #[cfg(target_os = "linux")]
    if conn_opts.udp_no_fragment {
        socket::no_fragment::set(&lis, true);
    }

    // without failover, udp only uses `raddr`
    let failover = conn_opts.failover.as_ref().map(|opts| {
        let peers = literal_peers(&raddr, &extra_raddrs);
        // associations reuse one upstream socket across peers
        assert!(
            peers.iter().all(|p| p.is_ipv4() == peers[0].is_ipv4()),
            "[udp]{}: failover peers must share one address family",
            laddr
        );
        Failover::start(Proto::Udp, laddr.to_string(), peers, opts, &conn_opts)
    });

    let lis = Ref::new(&lis);
    let raddr = Ref::new(&raddr);
    let conn_opts = Ref::new(&conn_opts);
    let failover = Ref::new(&failover);
    associate_and_relay(lis, raddr, conn_opts, sockmap, failover).await
}
