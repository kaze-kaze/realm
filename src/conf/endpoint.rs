use serde::{Serialize, Deserialize};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};

use realm_core::endpoint::{Endpoint, RemoteAddr};

#[cfg(feature = "balance")]
use realm_core::balance::{Balancer, Strategy};

#[cfg(feature = "transport")]
use realm_core::kaminari::mix::{MixAccept, MixConnect};

use super::{Config, NetConf, NetInfo, FailoverConf};

#[derive(Debug, Serialize, Deserialize)]
pub struct EndpointConf {
    pub listen: String,

    pub remote: String,

    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extra_remotes: Vec<String>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<String>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failover: Option<FailoverConf>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub through: Option<String>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listen_interface: Option<String>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listen_transport: Option<String>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_transport: Option<String>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Config::is_empty")]
    pub network: NetConf,
}

impl EndpointConf {
    fn build_local(&self) -> SocketAddr {
        self.listen
            .to_socket_addrs()
            .expect("invalid local address")
            .next()
            .unwrap()
    }

    fn build_remote(&self) -> RemoteAddr {
        Self::build_remote_x(&self.remote)
    }

    fn build_remote_x(remote: &str) -> RemoteAddr {
        if let Ok(sockaddr) = remote.parse::<SocketAddr>() {
            RemoteAddr::SocketAddr(sockaddr)
        } else {
            let mut iter = remote.rsplitn(2, ':');
            let port = iter.next().unwrap().parse::<u16>().unwrap();
            let addr = iter.next().unwrap().to_string();
            RemoteAddr::DomainName(addr, port)
        }
    }

    fn build_send_through(&self) -> Option<SocketAddr> {
        let through = self.through.as_deref()?;
        if let Some(addr) = through.to_socket_addrs().ok().and_then(|mut addrs| addrs.next()) {
            return Some(addr);
        }
        let ip = through
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .unwrap_or(through)
            .parse::<IpAddr>()
            .unwrap_or_else(|_| panic!("endpoint {}: invalid through address {:?}", self.listen, through));
        Some(SocketAddr::new(ip, 0))
    }

    #[cfg(feature = "balance")]
    fn build_balancer(&self, warn: &mut impl FnMut(String)) -> Balancer {
        let Some(balance) = &self.balance else {
            return Balancer::default();
        };
        let (strategy, weights) = balance.split_once(':').unwrap_or_else(|| {
            panic!(
                "endpoint {}: balance requires a strategy and weights separated by ':'",
                self.listen
            )
        });
        let strategy = Strategy::from(strategy.trim());
        let weights = weights.trim();
        let weights = weights.strip_suffix(',').unwrap_or(weights).trim_end();
        let weights: Vec<u8> = if weights.is_empty() {
            Vec::new()
        } else {
            weights
                .split(',')
                .map(|weight| {
                    let weight = weight.trim();
                    weight.parse().unwrap_or_else(|_| {
                        panic!(
                            "endpoint {}: invalid balance weight {:?}, expected 0..=255",
                            self.listen, weight
                        )
                    })
                })
                .collect()
        };

        if strategy != Strategy::Off {
            let peers = 1 + self.extra_remotes.len();
            let count = weights.len();
            if self.failover.is_some() && count != peers {
                panic!(
                    "endpoint {}: failover with balance needs one weight per peer, got {} weight(s) for {} peer(s)",
                    self.listen, count, peers
                );
            }
            assert!(
                count <= peers,
                "endpoint {}: balance has {} weights for {} peers",
                self.listen,
                count,
                peers
            );
            assert!(
                count <= u8::MAX.into(),
                "endpoint {}: balance supports at most {} weights",
                self.listen,
                u8::MAX
            );
            if count < peers {
                warn(format!(
                    "endpoint {}: balance has only {} weight(s) for {} peers",
                    self.listen, count, peers
                ));
            }
        }
        Balancer::new(strategy, &weights)
    }

    #[cfg(feature = "transport")]
    fn build_transport(&self) -> Option<(MixAccept, MixConnect)> {
        use realm_core::kaminari::mix::{MixClientConf, MixServerConf};
        use realm_core::kaminari::opt::get_ws_conf;
        use realm_core::kaminari::opt::get_tls_client_conf;
        use realm_core::kaminari::opt::get_tls_server_conf;

        let Self {
            listen_transport,
            remote_transport,
            ..
        } = self;

        let listen_ws = listen_transport.as_ref().and_then(|s| get_ws_conf(s));
        let listen_tls = listen_transport.as_ref().and_then(|s| get_tls_server_conf(s));

        let remote_ws = remote_transport.as_ref().and_then(|s| get_ws_conf(s));
        let remote_tls = remote_transport.as_ref().and_then(|s| get_tls_client_conf(s));

        if matches!(
            (&listen_ws, &listen_tls, &remote_ws, &remote_tls),
            (None, None, None, None)
        ) {
            None
        } else {
            let ac = MixAccept::new_shared(MixServerConf {
                ws: listen_ws,
                tls: listen_tls,
            });
            let cc = MixConnect::new_shared(MixClientConf {
                ws: remote_ws,
                tls: remote_tls,
            });
            Some((ac, cc))
        }
    }
}

/// Report a setting that is ignored or corrected. This must not depend on the
/// log, which is off by default: without one it goes to stderr.
fn warn(msg: String) {
    if log::log_enabled!(log::Level::Warn) {
        log::warn!("{}", msg);
    } else {
        eprintln!("warning: {}", msg);
    }
}

#[derive(Debug)]
pub struct EndpointInfo {
    pub no_tcp: bool,
    pub use_udp: bool,
    pub endpoint: Endpoint,
}

impl EndpointConf {
    fn build_with_warnings(self, mut warn: impl FnMut(String)) -> EndpointInfo {
        let laddr = self.build_local();
        let raddr = self.build_remote();

        let extra_raddrs: Vec<_> = self.extra_remotes.iter().map(|r| Self::build_remote_x(r)).collect();
        let bind_address = self.build_send_through();
        if let Some(source) = bind_address {
            for remote in std::iter::once(&raddr).chain(&extra_raddrs) {
                if let RemoteAddr::SocketAddr(addr) = remote {
                    assert!(
                        source.is_ipv4() == addr.is_ipv4(),
                        "endpoint {}: through address {} and remote {} must share one address family",
                        self.listen,
                        source,
                        addr
                    );
                }
            }
        }

        if let Some(version) = self.network.send_proxy_version {
            assert!(
                matches!(version, 1 | 2),
                "endpoint {}: send_proxy_version must be 1 or 2, got {}",
                self.listen,
                version
            );
        }

        // build partial conn_opts from netconf
        let NetInfo {
            mut bind_opts,
            mut conn_opts,
            no_tcp,
            use_udp,
        } = self.network.build();

        if no_tcp && !use_udp {
            warn(format!("endpoint {}: no TCP or UDP listener is enabled", self.listen));
        }

        // the kernel takes at least one probe; linux refuses 0 and every connection with it
        if conn_opts.tcp_keepalive > 0 && conn_opts.tcp_keepalive_probe == 0 {
            warn(format!(
                "endpoint {}: tcp_keepalive_probe = 0 is not valid, using 1; to turn keepalive off, set tcp_keepalive = 0",
                self.listen
            ));
            conn_opts.tcp_keepalive_probe = 1;
        }

        #[cfg(target_os = "linux")]
        if conn_opts.tcp_keepalive > 0
            && conn_opts.tcp_user_timeout > 0
            && conn_opts.tcp_user_timeout <= conn_opts.tcp_keepalive
        {
            warn(format!(
                "endpoint {}: tcp_user_timeout <= tcp_keepalive; one unanswered probe can close an idle connection",
                self.listen
            ));
        }

        #[cfg(feature = "balance")]
        {
            conn_opts.balancer = self.build_balancer(&mut warn);
        }

        #[cfg(not(feature = "balance"))]
        if self.balance.is_some() {
            warn(format!(
                "endpoint {}: balance is ignored, this build has no balance feature (the slim builds do not): {}",
                self.listen,
                match self.failover {
                    Some(_) => "the peers are primary and backups",
                    None => "only remote is used",
                }
            ));
        }

        if let Some(failover) = &self.failover {
            let remotes: Vec<&str> = std::iter::once(self.remote.as_str())
                .chain(self.extra_remotes.iter().map(String::as_str))
                .collect();
            conn_opts.failover = Some(failover.build(&self.listen, &remotes, use_udp));

            // the dial deadline is shared by all dials of a connection
            let t = conn_opts.connect_timeout;
            if !no_tcp && (t == 0 || t > 10) {
                let stall = match t {
                    0 => {
                        "no tcp_timeout, a blackholed peer stalls new connections until the system gives up".to_string()
                    }
                    t => format!("tcp_timeout={}s, a blackholed peer stalls new connections that long", t),
                };
                warn(format!(
                    "endpoint {}: failover with {}; 3-5s is recommended",
                    self.listen, stall
                ));
            }
        }

        #[cfg(feature = "transport")]
        {
            conn_opts.transport = self.build_transport();
        }

        // build left fields of bind_opts and conn_opts
        conn_opts.bind_address = bind_address;
        conn_opts.bind_interface = self.interface;
        bind_opts.bind_interface = self.listen_interface;

        EndpointInfo {
            no_tcp,
            use_udp,
            endpoint: Endpoint {
                laddr,
                raddr,
                bind_opts,
                conn_opts,
                extra_raddrs,
            },
        }
    }
}

impl Config for EndpointConf {
    type Output = EndpointInfo;

    fn is_empty(&self) -> bool {
        false
    }

    fn build(self) -> Self::Output {
        self.build_with_warnings(warn)
    }

    fn rst_field(&mut self, _: &Self) -> &mut Self {
        unreachable!()
    }

    fn take_field(&mut self, _: &Self) -> &mut Self {
        unreachable!()
    }

    fn from_cmd_args(matches: &clap::ArgMatches) -> Self {
        let listen = matches.get_one("local").cloned().unwrap();
        let remote = matches.get_one("remote").cloned().unwrap();
        let through = matches.get_one("through").cloned();
        let interface = matches.get_one("interface").cloned();
        let listen_interface = matches.get_one("listen_interface").cloned();
        let listen_transport = matches.get_one("listen_transport").cloned();
        let remote_transport = matches.get_one("remote_transport").cloned();

        EndpointConf {
            listen,
            remote,
            through,
            interface,
            listen_interface,
            listen_transport,
            remote_transport,
            network: Default::default(),
            extra_remotes: Vec::new(),
            balance: None,
            failover: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(conf: &str) -> EndpointInfo {
        toml::from_str::<EndpointConf>(conf).unwrap().build()
    }

    fn config_with(options: &str) -> EndpointConf {
        toml::from_str(&format!("listen = '127.0.0.1:0'\nremote = '127.0.0.1:0'\n{}", options)).unwrap()
    }

    fn build_with(options: &str) -> EndpointInfo {
        config_with(options).build()
    }

    #[test]
    #[should_panic(expected = "endpoint 127.0.0.1:0: invalid through address")]
    fn rejects_invalid_through_address() {
        build_with("through = '10.300.1.1'");
    }

    #[test]
    fn accepts_through_addresses_with_or_without_a_port() {
        for (input, expected) in [
            ("127.0.0.1", "127.0.0.1:0"),
            ("[::1]", "[::1]:0"),
            ("::1", "[::1]:0"),
            ("::1:4040", "[::1]:4040"),
            ("[::1]:4040", "[::1]:4040"),
            ("[::1:4040]", "[::1:4040]:0"),
        ] {
            let opts = build(&format!(
                "listen = '127.0.0.1:0'\nremote = '{}'\nthrough = '{}'",
                expected, input
            ))
            .endpoint
            .conn_opts;
            assert_eq!(opts.bind_address, Some(expected.parse().unwrap()));
        }
    }

    #[test]
    #[should_panic(expected = "through address [::1]:0 and remote 127.0.0.1:0 must share one address family")]
    fn rejects_through_address_from_another_family() {
        build_with("through = '::1'");
    }

    #[test]
    #[should_panic(expected = "through address 127.0.0.1:0 and remote [::1]:0 must share one address family")]
    fn rejects_extra_remote_from_another_through_address_family() {
        build_with("through = '127.0.0.1'\nextra_remotes = ['[::1]:0']");
    }

    #[test]
    fn leaves_domain_family_to_resolution() {
        build_with("through = '127.0.0.1'\nextra_remotes = ['localhost:0']");
    }

    #[cfg(feature = "balance")]
    #[test]
    #[should_panic(expected = "invalid balance weight")]
    fn rejects_nonnumeric_balance_weight() {
        build_with("extra_remotes = ['127.0.0.2:0']\nbalance = 'roundrobin: 1, x'");
    }

    #[cfg(feature = "balance")]
    #[test]
    #[should_panic(expected = "balance has 3 weights for 2 peers")]
    fn rejects_excess_balance_weights() {
        build_with("extra_remotes = ['127.0.0.2:0']\nbalance = 'roundrobin: 1, 1, 1'");
    }

    #[cfg(feature = "balance")]
    #[test]
    fn missing_balance_weights_leave_peer_selection_unchanged() {
        let mut warnings = Vec::new();
        let ep = config_with("extra_remotes = ['127.0.0.2:0']\nbalance = 'roundrobin: 1,'")
            .build_with_warnings(|msg| warnings.push(msg));
        assert_eq!(
            warnings,
            ["endpoint 127.0.0.1:0: balance has only 1 weight(s) for 2 peers"]
        );
        assert_eq!(ep.endpoint.conn_opts.balancer.total(), 1);
        assert_eq!(ep.endpoint.extra_raddrs.len(), 1);
    }

    #[test]
    #[should_panic(expected = "send_proxy_version must be 1 or 2")]
    fn rejects_invalid_proxy_version() {
        build_with("network = { send_proxy_version = 3 }");
    }

    #[test]
    fn allows_a_rule_with_no_listeners() {
        let mut warnings = Vec::new();
        let ep = config_with("network = { no_tcp = true }").build_with_warnings(|msg| warnings.push(msg));
        assert_eq!(warnings, ["endpoint 127.0.0.1:0: no TCP or UDP listener is enabled"]);
        assert!(ep.no_tcp && !ep.use_udp);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn warns_when_tcp_user_timeout_allows_one_unanswered_keepalive_probe() {
        for (user_timeout, keepalive, warn) in [(15, 15, true), (16, 15, false), (0, 15, false), (15, 0, false)] {
            let mut warnings = Vec::new();
            config_with(&format!(
                "network = {{ tcp_user_timeout = {}, tcp_keepalive = {} }}",
                user_timeout, keepalive
            ))
            .build_with_warnings(|msg| warnings.push(msg));
            if warn {
                assert_eq!(warnings, ["endpoint 127.0.0.1:0: tcp_user_timeout <= tcp_keepalive; one unanswered probe can close an idle connection"]);
            } else {
                assert!(warnings.is_empty(), "{:?}", warnings);
            }
        }
    }

    /// A keepalive of zero probes is no keepalive the kernel accepts.
    #[test]
    fn keepalive_probe_is_at_least_one() {
        let ep = |network: &str| {
            let conf = format!(
                "listen = \"127.0.0.1:0\"\nremote = \"127.0.0.1:0\"\nnetwork = {{ {} }}",
                network
            );
            let opts = build(&conf).endpoint.conn_opts;
            (opts.tcp_keepalive, opts.tcp_keepalive_probe)
        };
        assert_eq!(ep(""), (15, 3));
        assert_eq!(ep("tcp_keepalive_probe = 0"), (15, 1));
        assert_eq!(ep("tcp_keepalive = 5, tcp_keepalive_probe = 2"), (5, 2));
        // keepalive off: the count is not used
        assert_eq!(ep("tcp_keepalive = 0, tcp_keepalive_probe = 0"), (0, 0));
    }
}
