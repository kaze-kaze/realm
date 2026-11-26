use serde::{Serialize, Deserialize};
use realm_core::endpoint::{BindOpts, ConnectOpts};

use super::Config;
use crate::consts::{TCP_TIMEOUT, UDP_TIMEOUT};
use crate::consts::{TCP_KEEPALIVE, TCP_KEEPALIVE_PROBE};
#[cfg(feature = "proxy")]
use crate::consts::{PROXY_PROTOCOL_VERSION, PROXY_PROTOCOL_TIMEOUT};

#[derive(Serialize, Debug, Deserialize, Clone, Copy, Default)]
pub struct NetConf {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_tcp: Option<bool>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_udp: Option<bool>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv6_only: Option<bool>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_mptcp: Option<bool>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept_mptcp: Option<bool>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_proxy: Option<bool>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept_proxy: Option<bool>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_proxy_version: Option<usize>,

    #[serde(default)]
    pub accept_proxy_timeout: Option<usize>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tcp_keepalive: Option<usize>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tcp_keepalive_probe: Option<usize>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tcp_timeout: Option<usize>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub half_close_timeout: Option<usize>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tcp_user_timeout: Option<usize>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub udp_timeout: Option<usize>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub udp_no_fragment: Option<bool>,
}

#[derive(Debug)]
pub struct NetInfo {
    pub bind_opts: BindOpts,
    pub conn_opts: ConnectOpts,
    pub no_tcp: bool,
    pub use_udp: bool,
}

impl Config for NetConf {
    type Output = NetInfo;

    fn is_empty(&self) -> bool {
        crate::empty![self =>
            no_tcp, use_udp, ipv6_only,
            send_mptcp, accept_mptcp,
            send_proxy, accept_proxy, send_proxy_version, accept_proxy_timeout,
            tcp_keepalive, tcp_keepalive_probe, tcp_timeout, half_close_timeout, tcp_user_timeout,
            udp_timeout, udp_no_fragment
        ]
    }

    fn build(self) -> Self::Output {
        macro_rules! unbox {
            ($field: ident) => {
                self.$field.unwrap_or_default()
            };
            ($field: ident, $value: expr) => {
                self.$field.unwrap_or($value)
            };
        }

        let no_tcp = unbox!(no_tcp);
        let use_udp = unbox!(use_udp);
        let ipv6_only = unbox!(ipv6_only);
        let send_mptcp = unbox!(send_mptcp);
        let accept_mptcp = unbox!(accept_mptcp);
        let tcp_kpa = unbox!(tcp_keepalive, TCP_KEEPALIVE);
        let tcp_kpa_probe = unbox!(tcp_keepalive_probe, TCP_KEEPALIVE_PROBE);
        let tcp_timeout = unbox!(tcp_timeout, TCP_TIMEOUT);
        let tcp_user_timeout = unbox!(tcp_user_timeout);
        let udp_timeout = unbox!(udp_timeout, UDP_TIMEOUT);
        let udp_no_fragment = unbox!(udp_no_fragment);

        let bind_opts = BindOpts {
            ipv6_only,
            accept_mptcp,
            bind_interface: None,
        };
        let conn_opts = ConnectOpts {
            send_mptcp,
            tcp_keepalive: tcp_kpa,
            tcp_keepalive_probe: tcp_kpa_probe,
            connect_timeout: tcp_timeout,
            half_close_timeout: self.half_close_timeout,
            tcp_user_timeout,
            associate_timeout: udp_timeout,
            udp_no_fragment,

            // from endpoint
            bind_address: None,
            bind_interface: None,

            #[cfg(feature = "balance")]
            balancer: Default::default(),

            // from endpoint
            failover: None,

            #[cfg(feature = "transport")]
            transport: None,

            #[cfg(feature = "proxy")]
            proxy_opts: {
                use realm_core::endpoint::ProxyOpts;
                let send_proxy = unbox!(send_proxy);
                let send_proxy_version = unbox!(send_proxy_version, PROXY_PROTOCOL_VERSION);
                let accept_proxy = unbox!(accept_proxy);
                let accept_proxy_timeout = unbox!(accept_proxy_timeout, PROXY_PROTOCOL_TIMEOUT);
                ProxyOpts {
                    send_proxy,
                    accept_proxy,
                    send_proxy_version,
                    accept_proxy_timeout,
                }
            },
        };

        NetInfo {
            bind_opts,
            conn_opts,
            no_tcp,
            use_udp,
        }
    }

    fn rst_field(&mut self, other: &Self) -> &mut Self {
        use crate::rst;
        let other = *other;

        rst!(self, no_tcp, other);
        rst!(self, use_udp, other);
        rst!(self, ipv6_only, other);
        rst!(self, send_mptcp, other);
        rst!(self, accept_mptcp, other);
        rst!(self, tcp_keepalive, other);
        rst!(self, tcp_keepalive_probe, other);
        rst!(self, tcp_timeout, other);
        rst!(self, half_close_timeout, other);
        rst!(self, tcp_user_timeout, other);
        rst!(self, udp_timeout, other);
        rst!(self, udp_no_fragment, other);
        rst!(self, send_proxy, other);
        rst!(self, accept_proxy, other);
        rst!(self, send_proxy_version, other);
        rst!(self, accept_proxy_timeout, other);
        self
    }

    fn take_field(&mut self, other: &Self) -> &mut Self {
        use crate::take;
        let other = *other;

        take!(self, no_tcp, other);
        take!(self, use_udp, other);
        take!(self, ipv6_only, other);
        take!(self, send_mptcp, other);
        take!(self, accept_mptcp, other);
        take!(self, tcp_keepalive, other);
        take!(self, tcp_keepalive_probe, other);
        take!(self, tcp_timeout, other);
        take!(self, half_close_timeout, other);
        take!(self, tcp_user_timeout, other);
        take!(self, udp_timeout, other);
        take!(self, udp_no_fragment, other);
        take!(self, send_proxy, other);
        take!(self, accept_proxy, other);
        take!(self, send_proxy_version, other);
        take!(self, accept_proxy_timeout, other);
        self
    }

    fn from_cmd_args(matches: &clap::ArgMatches) -> Self {
        macro_rules! unpack {
            ($key: expr) => {
                if matches.get_flag($key) {
                    Some(true)
                } else {
                    None
                }
            };
        }

        let no_tcp = unpack!("no_tcp");
        let use_udp = unpack!("use_udp");
        let use_mptcp = unpack!("use_mptcp");
        let ipv6_only = unpack!("ipv6_only");

        let send_mptcp = use_mptcp;
        let accept_mptcp = use_mptcp;

        let tcp_keepalive = matches.get_one::<usize>("tcp_keepalive").copied();
        let tcp_keepalive_probe = matches.get_one::<usize>("tcp_keepalive_probe").copied();
        let tcp_timeout = matches.get_one::<usize>("tcp_timeout").copied();
        let half_close_timeout = matches.get_one::<usize>("half_close_timeout").copied();
        let tcp_user_timeout = matches.get_one::<usize>("tcp_user_timeout").copied();
        let udp_timeout = matches.get_one::<usize>("udp_timeout").copied();

        let send_proxy = matches.get_one::<bool>("send_proxy").copied();
        let send_proxy_version = matches.get_one::<usize>("send_proxy_version").copied();

        let accept_proxy = matches.get_one::<bool>("accept_proxy").copied();
        let accept_proxy_timeout = matches.get_one::<usize>("accept_proxy_timeout").copied();

        Self {
            no_tcp,
            use_udp,
            ipv6_only,
            send_mptcp,
            accept_mptcp,
            tcp_keepalive,
            tcp_keepalive_probe,
            tcp_timeout,
            half_close_timeout,
            tcp_user_timeout,
            udp_timeout,
            udp_no_fragment: None,
            send_proxy,
            accept_proxy,
            send_proxy_version,
            accept_proxy_timeout,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tcp_timeout_defaults_follow_the_build_features() {
        let info = NetConf::default().build();
        assert_eq!(info.conn_opts.tcp_user_timeout, 0);
        let expected = if cfg!(feature = "brutal-shutdown") {
            "10s"
        } else {
            "unlimited"
        };
        assert!(info
            .conn_opts
            .to_string()
            .contains(&format!("half-close-timeout={}", expected)));
    }
}
