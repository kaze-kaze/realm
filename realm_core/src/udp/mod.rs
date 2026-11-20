//! UDP relay entrance.

pub(crate) mod socket;
mod sockmap;
mod middle;
mod batched;

use std::io::Result;

use crate::trick::Ref;
use crate::endpoint::Endpoint;
use crate::failover::{Failover, Proto, literal_peers};

use sockmap::SockMap;
use middle::associate_and_relay;

/// Launch a udp relay.
pub async fn run_udp(endpoint: Endpoint) -> Result<()> {
    let Endpoint {
        laddr,
        raddr,
        bind_opts,
        conn_opts,
        extra_raddrs,
    } = endpoint;

    let sockmap = SockMap::new();

    let lis = socket::bind(&laddr, bind_opts).unwrap_or_else(|e| panic!("[udp]failed to bind {}: {}", laddr, e));
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
    let sockmap = Ref::new(&sockmap);
    let failover = Ref::new(&failover);
    loop {
        if let Err(e) = associate_and_relay(lis, raddr, conn_opts, sockmap, failover).await {
            log::error!("[udp]error: {}", e);
        }
    }
}
