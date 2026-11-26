use std::fmt::{Formatter, Display};
use std::net::ToSocketAddrs;

use serde::{Serialize, Deserialize};
use realm_core::dns::config;
use config::{LookupIpStrategy, NameServerConfig, ConnectionConfig, ProtocolConfig};
use config::{ResolverConfig, ResolverOpts};

use super::Config;

// dns mode
#[derive(Debug, Default, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum DnsMode {
    Ipv4Only,
    Ipv6Only,
    #[default]
    Ipv4AndIpv6,
    Ipv4ThenIpv6,
    Ipv6ThenIpv4,
}

impl Display for DnsMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use DnsMode::*;
        let s = match self {
            Ipv4Only => "ipv4_only",
            Ipv6Only => "ipv6_only",
            Ipv4AndIpv6 => "ipv4_and_ipv6",
            Ipv4ThenIpv6 => "ipv4_then_ipv6",
            Ipv6ThenIpv4 => "ipv6_then_ipv4",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for DnsMode {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        use DnsMode::*;
        match value.to_ascii_lowercase().as_str() {
            "ipv4_only" => Ok(Ipv4Only),
            "ipv6_only" => Ok(Ipv6Only),
            "ipv4_and_ipv6" => Ok(Ipv4AndIpv6),
            "ipv4_then_ipv6" => Ok(Ipv4ThenIpv6),
            "ipv6_then_ipv4" => Ok(Ipv6ThenIpv4),
            _ => Err("expected one of: ipv4_only, ipv6_only, ipv4_and_ipv6, ipv4_then_ipv6, ipv6_then_ipv4"),
        }
    }
}

impl From<String> for DnsMode {
    fn from(value: String) -> Self {
        value.parse().unwrap_or_default()
    }
}

impl From<DnsMode> for LookupIpStrategy {
    fn from(mode: DnsMode) -> Self {
        match mode {
            DnsMode::Ipv4Only => LookupIpStrategy::Ipv4Only,
            DnsMode::Ipv6Only => LookupIpStrategy::Ipv6Only,
            DnsMode::Ipv4AndIpv6 => LookupIpStrategy::Ipv4AndIpv6,
            DnsMode::Ipv4ThenIpv6 => LookupIpStrategy::Ipv4thenIpv6,
            DnsMode::Ipv6ThenIpv4 => LookupIpStrategy::Ipv6thenIpv4,
        }
    }
}

// dns protocol
#[derive(Debug, Default, Serialize, Deserialize, Eq, PartialEq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum DnsProtocol {
    Tcp,
    Udp,
    #[default]
    TcpAndUdp,
}

impl Display for DnsProtocol {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use DnsProtocol::*;
        let s = match self {
            Tcp => "tcp",
            Udp => "udp",
            TcpAndUdp => "tcp+udp",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for DnsProtocol {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        use DnsProtocol::*;
        match value.to_ascii_lowercase().as_str() {
            "tcp" => Ok(Tcp),
            "udp" => Ok(Udp),
            "tcp+udp" | "tcp_and_udp" => Ok(TcpAndUdp),
            _ => Err("expected one of: tcp, udp, tcp+udp | tcp_and_udp"),
        }
    }
}

impl From<String> for DnsProtocol {
    fn from(value: String) -> Self {
        value.parse().unwrap_or_default()
    }
}

impl From<DnsProtocol> for Vec<ProtocolConfig> {
    fn from(x: DnsProtocol) -> Self {
        use DnsProtocol::*;
        match x {
            Tcp => vec![ProtocolConfig::Tcp],
            Udp => vec![ProtocolConfig::Udp],
            TcpAndUdp => vec![ProtocolConfig::Tcp, ProtocolConfig::Udp],
        }
    }
}

// dns config
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct DnsConf {
    // ResolverOpts
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<DnsMode>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_ttl: Option<u32>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_ttl: Option<u32>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_size: Option<u64>,

    // ResolverConfig
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<DnsProtocol>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nameservers: Option<Vec<String>>,
}

impl Display for DnsConf {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        macro_rules! default {
            ($ref: expr) => {
                match $ref {
                    Some(x) => *x,
                    None => Default::default(),
                }
            };
            ($ref: expr, $value: expr) => {
                match $ref {
                    Some(x) => *x,
                    None => $value,
                }
            };
        }
        let DnsConf {
            mode,
            min_ttl,
            max_ttl,
            cache_size,
            protocol,
            nameservers,
        } = self;

        let mode = default!(mode);

        let min_ttl = default!(min_ttl, 0_u32);

        let max_ttl = default!(max_ttl, config::MAX_TTL);

        let cache_size = default!(cache_size, ResolverOpts::default().cache_size);

        let protocol = default!(protocol);

        let nameservers = match nameservers {
            Some(s) => s.join(", "),
            None => String::from("system"),
        };

        write!(f, "mode={}, protocol={}, ", mode, protocol).unwrap();
        write!(
            f,
            "min-ttl={}, max-ttl={}, cache-size={}, ",
            min_ttl, max_ttl, cache_size
        )
        .unwrap();
        write!(f, "servers={}", nameservers)
    }
}

impl Config for DnsConf {
    type Output = (Option<ResolverConfig>, Option<ResolverOpts>);

    fn build(self) -> Self::Output {
        use crate::empty;
        use std::time::Duration;

        let DnsConf {
            mode,
            protocol,
            nameservers,
            min_ttl,
            max_ttl,
            cache_size,
        } = self;

        let min = min_ttl.unwrap_or_default();
        let max = max_ttl.unwrap_or(config::MAX_TTL);
        assert!(min <= max, "dns.min_ttl ({}) exceeds dns.max_ttl ({})", min, max);

        let opts = if empty![mode, min_ttl, max_ttl, cache_size] {
            None
        } else {
            let mut opts = realm_core::dns::DnsConf::default().opts;
            if let Some(mode) = mode {
                opts.ip_strategy = mode.into();
            }
            if let Some(ttl) = min_ttl {
                opts.positive_min_ttl = Some(Duration::from_secs(ttl.into()));
            }
            if let Some(ttl) = max_ttl {
                opts.positive_max_ttl = Some(Duration::from_secs(ttl.into()));
            }
            if let Some(cache_size) = cache_size {
                opts.cache_size = cache_size;
            }
            Some(opts)
        };

        if matches!((&nameservers, &protocol), (&None, &None)) {
            return (None, opts);
        }

        // parse into ResolverConfig
        fn tcp_and_udp(port: u16) -> [ConnectionConfig; 2] {
            [ConnectionConfig::udp(), ConnectionConfig::tcp()].map(|mut cc| {
                cc.port = port;
                cc
            })
        }
        let mut nameservers = match nameservers {
            None => {
                use realm_core::dns::DnsConf as TrustDnsConf;
                let TrustDnsConf { conf, .. } = TrustDnsConf::default();
                conf.name_servers
            }
            // [ip1:port1, ip2:port2] => [ip1:[port1, port2]]
            Some(addrs) => addrs
                .into_iter()
                .map(|x| x.to_socket_addrs().unwrap().next().unwrap())
                .fold(Vec::new(), |mut nss: Vec<NameServerConfig>, addr| {
                    if let Some(slot) = nss.iter_mut().find(|ns| ns.ip == addr.ip()) {
                        slot.connections.extend_from_slice(&tcp_and_udp(addr.port()))
                    } else {
                        nss.push(NameServerConfig::new(addr.ip(), true, tcp_and_udp(addr.port()).into()))
                    }
                    nss
                }),
        };

        let protocols: Vec<ProtocolConfig> = protocol.unwrap_or_default().into();
        nameservers.retain_mut(|nss| {
            nss.connections
                .dedup_by(|a, b| a.port == b.port && a.protocol == b.protocol);
            nss.connections.retain(|c| protocols.contains(&c.protocol));
            !nss.connections.is_empty()
        });
        (Some(ResolverConfig::from_parts(None, Vec::new(), nameservers)), opts)
    }

    fn rst_field(&mut self, other: &Self) -> &mut Self {
        use crate::rst;
        let other = other.clone();
        rst!(self, mode, other);
        rst!(self, min_ttl, other);
        rst!(self, max_ttl, other);
        rst!(self, cache_size, other);
        rst!(self, protocol, other);
        rst!(self, nameservers, other);
        self
    }

    fn take_field(&mut self, other: &Self) -> &mut Self {
        use crate::take;
        let other = other.clone();
        take!(self, mode, other);
        take!(self, min_ttl, other);
        take!(self, max_ttl, other);
        take!(self, cache_size, other);
        take!(self, protocol, other);
        take!(self, nameservers, other);
        self
    }

    fn from_cmd_args(matches: &clap::ArgMatches) -> Self {
        let mode = matches.get_one::<DnsMode>("dns_mode").copied();

        let min_ttl = matches.get_one::<u32>("dns_min_ttl").copied();
        let max_ttl = matches.get_one::<u32>("dns_max_ttl").copied();
        let cache_size = matches.get_one::<u64>("dns_cache_size").copied();

        let protocol = matches.get_one::<DnsProtocol>("dns_protocol").copied();

        let nameservers = matches
            .get_one::<String>("dns_servers")
            .map(|x| x.split(',').map(String::from).collect());

        Self {
            mode,
            min_ttl,
            max_ttl,
            cache_size,
            protocol,
            nameservers,
        }
    }

    fn is_empty(&self) -> bool {
        crate::empty![self => mode, min_ttl, max_ttl, cache_size, protocol, nameservers]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dns_uses_both_address_families_with_system_settings() {
        let config = DnsConf::default();
        assert!(config.is_empty());
        assert!(config
            .to_string()
            .contains(&format!("cache-size={}", ResolverOpts::default().cache_size)));
        let (servers, opts) = config.build();
        assert!(servers.is_none() && opts.is_none());
        let defaults = realm_core::dns::DnsConf::default().opts;
        assert_eq!(defaults.ip_strategy, LookupIpStrategy::Ipv4AndIpv6);
    }

    #[test]
    fn partial_dns_config_overrides_only_the_specified_resolver_options() {
        let expected = realm_core::dns::DnsConf::default().opts;
        let (_, opts) = DnsConf {
            cache_size: Some(16),
            ..Default::default()
        }
        .build();
        let opts = opts.unwrap();
        assert_eq!(opts.ip_strategy, LookupIpStrategy::Ipv4AndIpv6);
        assert_eq!(
            (opts.ndots, opts.timeout, opts.attempts),
            (expected.ndots, expected.timeout, expected.attempts)
        );
        assert_eq!(opts.cache_size, 16);
        let (_, opts) = DnsConf {
            mode: Some(DnsMode::Ipv4Only),
            ..Default::default()
        }
        .build();
        assert_eq!(opts.unwrap().ip_strategy, LookupIpStrategy::Ipv4Only);
    }

    #[test]
    fn protocol_only_and_nameservers_only_are_not_empty() {
        assert!(!DnsConf {
            protocol: Some(DnsProtocol::Tcp),
            ..Default::default()
        }
        .is_empty());
        assert!(!DnsConf {
            nameservers: Some(Vec::new()),
            ..Default::default()
        }
        .is_empty());
    }

    #[test]
    #[should_panic(expected = "dns.min_ttl (11) exceeds dns.max_ttl (10)")]
    fn rejects_minimum_greater_than_configured_maximum() {
        toml::from_str::<DnsConf>("min_ttl = 11\nmax_ttl = 10").unwrap().build();
    }

    #[test]
    #[should_panic(expected = "dns.min_ttl (86401) exceeds dns.max_ttl (86400)")]
    fn rejects_minimum_greater_than_default_maximum() {
        toml::from_str::<DnsConf>("min_ttl = 86401").unwrap().build();
    }
}
