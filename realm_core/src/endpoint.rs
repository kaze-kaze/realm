//! Relay endpoint.

use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use std::time::Duration;

#[cfg(feature = "transport")]
use kaminari::mix::{MixAccept, MixConnect};

#[cfg(feature = "balance")]
use realm_lb::Balancer;

use crate::failover::FailoverOpts;

/// Remote address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteAddr {
    SocketAddr(SocketAddr),
    DomainName(String, u16),
}

/// Proxy protocol options.
#[cfg(feature = "proxy")]
#[derive(Debug, Default, Clone, Copy)]
pub struct ProxyOpts {
    pub send_proxy: bool,
    pub accept_proxy: bool,
    pub send_proxy_version: usize,
    pub accept_proxy_timeout: usize,
}

#[cfg(feature = "proxy")]
impl ProxyOpts {
    #[inline]
    pub(crate) const fn enabled(&self) -> bool {
        self.send_proxy || self.accept_proxy
    }
}

/// Connect or associate options.
#[derive(Debug, Default, Clone)]
pub struct ConnectOpts {
    pub send_mptcp: bool,
    pub connect_timeout: usize,
    /// Idle seconds after one direction shuts down. Zero closes immediately.
    pub half_close_timeout: Option<usize>,
    /// Linux TCP user timeout in seconds. Zero leaves the existing behavior.
    pub tcp_user_timeout: usize,
    pub associate_timeout: usize,
    pub tcp_keepalive: usize,
    pub tcp_keepalive_probe: usize,
    /// Udp: forward a datagram that fits one frame as don't-fragment and never
    /// fragment it on this host, see [`crate::udp`]. Linux only.
    pub udp_no_fragment: bool,
    pub bind_address: Option<SocketAddr>,
    pub bind_interface: Option<String>,

    #[cfg(feature = "proxy")]
    pub proxy_opts: ProxyOpts,

    #[cfg(feature = "transport")]
    pub transport: Option<(MixAccept, MixConnect)>,

    #[cfg(feature = "balance")]
    pub balancer: Balancer,

    /// Health checked failover over `[raddr, extra_raddrs..]`: active-standby, or
    /// with a balancer, shared by the peers that are alive.
    pub failover: Option<FailoverOpts>,
}

impl ConnectOpts {
    pub const DEFAULT_HALF_CLOSE_TIMEOUT: Option<usize> = if cfg!(feature = "brutal-shutdown") {
        Some(10)
    } else {
        None
    };

    pub(crate) fn half_close_timeout(&self) -> Option<Duration> {
        self.half_close_timeout
            .or(Self::DEFAULT_HALF_CLOSE_TIMEOUT)
            .map(|secs| Duration::from_secs(secs as u64))
    }
}

#[derive(Debug, Default, Clone)]
pub struct BindOpts {
    pub ipv6_only: bool,
    pub accept_mptcp: bool,
    pub tcp_user_timeout: usize,
    pub bind_interface: Option<String>,
}

/// Relay endpoint.
#[derive(Debug, Clone)]
pub struct Endpoint {
    pub laddr: SocketAddr,
    pub raddr: RemoteAddr,
    pub bind_opts: BindOpts,
    pub conn_opts: ConnectOpts,
    pub extra_raddrs: Vec<RemoteAddr>,
}

// display impl below

impl Display for RemoteAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use RemoteAddr::*;
        match self {
            SocketAddr(addr) => write!(f, "{}", addr),
            DomainName(host, port) => write!(f, "{}:{}", host, port),
        }
    }
}

impl Display for Endpoint {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} -> [{}", self.laddr, self.raddr)?;
        for raddr in self.extra_raddrs.iter() {
            write!(f, "|{}", raddr)?;
        }
        write!(f, "]; options: {}; {}", self.bind_opts, self.conn_opts)
    }
}

impl Display for BindOpts {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let BindOpts {
            accept_mptcp,
            ipv6_only,
            bind_interface,
            ..
        } = self;
        if let Some(iface) = bind_interface {
            write!(f, "listen-iface={}, ", iface)?;
        }
        write!(f, "ipv6-only={}, ", ipv6_only)?;
        write!(f, "accept-mptcp={}", accept_mptcp)?;
        Ok(())
    }
}

impl Display for ConnectOpts {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let ConnectOpts {
            send_mptcp,
            connect_timeout,
            tcp_user_timeout,
            associate_timeout,
            tcp_keepalive,
            tcp_keepalive_probe,
            udp_no_fragment,
            bind_address,
            bind_interface,

            #[cfg(feature = "proxy")]
            proxy_opts,

            #[cfg(feature = "transport")]
            transport,

            #[cfg(feature = "balance")]
            balancer,

            failover,
            ..
        } = self;

        if let Some(iface) = bind_interface {
            write!(f, "send-iface={}, ", iface)?;
        }

        if let Some(send_through) = bind_address {
            write!(f, "send-through={}, ", send_through)?;
        }

        write!(f, "send-mptcp={}; ", send_mptcp)?;

        #[cfg(feature = "proxy")]
        {
            let ProxyOpts {
                send_proxy,
                accept_proxy,
                send_proxy_version,
                accept_proxy_timeout,
            } = proxy_opts;
            write!(
                f,
                "send-proxy={0}, send-proxy-version={2}, accept-proxy={1}, accept-proxy-timeout={3}s; ",
                send_proxy, accept_proxy, send_proxy_version, accept_proxy_timeout
            )?;
        }

        write!(
            f,
            "tcp-keepalive={}s[{}] connect-timeout={}s, associate-timeout={}s; ",
            tcp_keepalive, tcp_keepalive_probe, connect_timeout, associate_timeout
        )?;

        match self.half_close_timeout() {
            Some(timeout) => write!(f, "half-close-timeout={}s, ", timeout.as_secs())?,
            None => write!(f, "half-close-timeout=unlimited, ")?,
        }
        write!(f, "tcp-user-timeout={}s; ", tcp_user_timeout)?;

        if *udp_no_fragment {
            write!(f, "udp-no-fragment; ")?;
        }

        #[cfg(feature = "transport")]
        if let Some((ac, cc)) = transport {
            write!(f, "transport={}||{}; ", ac, cc)?;
        }

        #[cfg(feature = "balance")]
        write!(f, "balance={}", balancer.strategy())?;

        if let Some(failover) = failover {
            #[cfg(feature = "balance")]
            write!(f, "; ")?;
            write!(f, "failover: {}", failover)?;
        }
        Ok(())
    }
}
