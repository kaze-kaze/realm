use std::net::SocketAddr;
use std::time::Duration;

use serde::{Serialize, Deserialize};
use realm_core::failover::{FailoverOpts, UdpProbe, MAX_PEERS};

/// Health checked failover: `remote` is the primary and `extra_remotes` are backups in priority
/// order, or with `balance`, all of them share new sessions while they are alive.
///
/// A key that is not known is an error. Left alone, a misspelt one would fall back to
/// its default without a word: `udp_prob = "quic"` probes with echo, no peer answers,
/// and udp never fails over.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailoverConf {
    /// Health check targets, one per peer; empty probes the data addresses.
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub probe: Vec<String>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tcp_send: Option<String>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tcp_expect: Option<String>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub udp_send: Option<String>,

    /// `echo` (default) or `quic`.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub udp_probe: Option<String>,

    /// Seconds.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval: Option<f64>,

    /// Seconds.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<f64>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fall: Option<u8>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<u8>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rise: Option<u8>,

    /// Seconds.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold: Option<f64>,
}

impl FailoverConf {
    /// Validate against the endpoint and build core options. Panics on invalid config.
    pub fn build(&self, listen: &str, remotes: &[&str], use_udp: bool) -> FailoverOpts {
        let fail = |msg: String| -> ! { panic!("endpoint {}: failover: {}", listen, msg) };
        let literal = |s: &str| {
            s.parse::<SocketAddr>()
                .unwrap_or_else(|_| fail(format!("{} must be an ip:port literal", s)))
        };
        const MAX_SECONDS: f64 = 86_400.0;
        let secs = |name: &str, v: Option<f64>, default: Duration| match v {
            Some(x) if x > MAX_SECONDS => fail(format!("{} must be at most {} seconds, got {}", name, MAX_SECONDS, x)),
            None => default,
            Some(x) => Duration::try_from_secs_f64(x)
                .ok()
                .filter(|_| x > 0.0)
                .unwrap_or_else(|| fail(format!("{} must be a positive number of seconds, got {}", name, x))),
        };

        let peers: Vec<SocketAddr> = remotes.iter().map(|r| literal(r)).collect();
        if !(2..=MAX_PEERS).contains(&peers.len()) {
            fail(format!(
                "needs remote plus 1..={} extra_remotes, got {} peer(s)",
                MAX_PEERS - 1,
                peers.len()
            ));
        }
        for (i, peer) in peers.iter().enumerate() {
            if peers[..i].contains(peer) {
                fail(format!("duplicate data address {}", peer));
            }
        }
        if use_udp && peers.iter().any(|p| p.is_ipv4() != peers[0].is_ipv4()) {
            fail("udp peers must share one address family".into());
        }

        let probe: Vec<SocketAddr> = self.probe.iter().map(|p| literal(p)).collect();
        if !probe.is_empty() && probe.len() != peers.len() {
            fail(format!(
                "probe has {} target(s), expect one per peer ({})",
                probe.len(),
                peers.len()
            ));
        }

        let udp_probe = match self.udp_probe.as_deref() {
            None | Some("echo") => UdpProbe::Echo,
            Some("quic") => UdpProbe::Quic,
            Some(x) => fail(format!("udp_probe must be \"echo\" or \"quic\", got {:?}", x)),
        };

        let df = FailoverOpts::default();
        let bytes = |v: &Option<String>| v.as_deref().unwrap_or_default().as_bytes().to_vec();
        let opts = FailoverOpts {
            probe,
            tcp_send: bytes(&self.tcp_send),
            tcp_expect: bytes(&self.tcp_expect),
            udp_send: bytes(&self.udp_send),
            udp_probe,
            interval: secs("interval", self.interval, df.interval),
            timeout: secs("timeout", self.timeout, df.timeout),
            fall: self.fall.unwrap_or(df.fall),
            window: self.window.unwrap_or(df.window),
            rise: self.rise.unwrap_or(df.rise),
            hold: secs("hold", self.hold, df.hold),
        };

        if opts.interval.is_zero() {
            fail("interval rounds to zero".into());
        }
        if opts.timeout.is_zero() {
            fail("timeout rounds to zero".into());
        }

        if !(1..=8).contains(&opts.window) {
            fail(format!("window must be 1..=8, got {}", opts.window));
        }
        if !(1..=opts.window).contains(&opts.fall) {
            fail(format!("fall must be 1..=window({}), got {}", opts.window, opts.fall));
        }
        if opts.rise == 0 {
            fail("rise must be at least 1".into());
        }
        opts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let conf: FailoverConf = toml::from_str("").unwrap();
        let opts = conf.build("l", &["10.0.0.1:1", "10.0.0.2:1"], true);
        assert!(opts.probe.is_empty() && opts.tcp_send.is_empty() && opts.udp_send.is_empty());
        assert_eq!(opts.udp_probe, UdpProbe::Echo);
        assert_eq!(opts.interval, Duration::from_secs(3));
        assert_eq!(opts.timeout, Duration::from_millis(1500));
        assert_eq!((opts.fall, opts.window, opts.rise), (3, 5, 5));
        assert_eq!(opts.hold, Duration::from_secs(30));
    }

    #[test]
    fn udp_probe_kinds() {
        let build = |s: &str| {
            toml::from_str::<FailoverConf>(s)
                .unwrap()
                .build("l", &["10.0.0.1:1", "10.0.0.2:1"], true)
                .udp_probe
        };
        assert_eq!(build(r#"udp_probe = "echo""#), UdpProbe::Echo);
        assert_eq!(build(r#"udp_probe = "quic""#), UdpProbe::Quic);
    }

    #[test]
    #[should_panic(expected = "udp_probe must be")]
    fn reject_unknown_udp_probe() {
        let conf: FailoverConf = toml::from_str(r#"udp_probe = "ping""#).unwrap();
        conf.build("l", &["10.0.0.1:1", "10.0.0.2:1"], true);
    }

    /// A misspelt key must not pass for a default.
    #[test]
    fn reject_unknown_key() {
        use crate::conf::FullConf;
        let conf = |failover: &str| {
            format!(
                "[[endpoints]]\nlisten = \"127.0.0.1:1\"\nremote = \"10.0.0.1:1\"\n\
                 extra_remotes = [\"10.0.0.2:1\"]\n[endpoints.failover]\n{}\n",
                failover
            )
        };
        assert!(FullConf::from_conf_str(&conf("udp_probe = \"quic\"")).is_ok());
        let err = FullConf::from_conf_str(&conf("udp_prob = \"quic\""))
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown field `udp_prob`"), "{}", err);
        // json as well
        let json = |key: &str| {
            format!(
                r#"{{"endpoints":[{{"listen":"127.0.0.1:1","remote":"10.0.0.1:1","extra_remotes":["10.0.0.2:1"],"failover":{{"{}":2}}}}]}}"#,
                key
            )
        };
        assert!(FullConf::from_conf_str(&json("interval")).is_ok());
        let err = FullConf::from_conf_str(&json("intervall")).unwrap_err().to_string();
        assert!(err.contains("unknown field `intervall`"), "{}", err);
    }

    #[test]
    fn full() {
        let conf: FailoverConf = toml::from_str(
            r#"
            probe = ["10.0.0.1:9", "10.0.0.2:9"]
            tcp_send = "GET / HTTP/1.0\r\n\r\n"
            tcp_expect = "HTTP/1.1 204"
            udp_send = "hc"
            interval = 0.5
            timeout = 0.25
            fall = 2
            window = 3
            rise = 4
            hold = 10
            "#,
        )
        .unwrap();
        let opts = conf.build("l", &["10.0.0.1:1", "10.0.0.2:1"], true);
        assert_eq!(opts.probe.len(), 2);
        assert_eq!(opts.tcp_send, b"GET / HTTP/1.0\r\n\r\n");
        assert_eq!(opts.tcp_expect, b"HTTP/1.1 204");
        assert_eq!(opts.udp_send, b"hc");
        assert_eq!(opts.interval, Duration::from_millis(500));
        assert_eq!(opts.timeout, Duration::from_millis(250));
        assert_eq!((opts.fall, opts.window, opts.rise), (2, 3, 4));
        assert_eq!(opts.hold, Duration::from_secs(10));
    }

    #[test]
    #[should_panic(expected = "ip:port literal")]
    fn reject_domain() {
        FailoverConf::default().build("l", &["a.com:1", "10.0.0.2:1"], false);
    }

    #[test]
    #[should_panic(expected = "extra_remotes")]
    fn reject_single_peer() {
        FailoverConf::default().build("l", &["10.0.0.1:1"], false);
    }

    #[test]
    #[should_panic(expected = "address family")]
    fn reject_mixed_family_udp() {
        FailoverConf::default().build("l", &["10.0.0.1:1", "[::1]:1"], true);
    }

    #[test]
    fn mixed_family_tcp_ok() {
        FailoverConf::default().build("l", &["10.0.0.1:1", "[::1]:1"], false);
    }

    #[test]
    #[should_panic(expected = "one per peer")]
    fn reject_probe_count() {
        let conf = FailoverConf {
            probe: vec!["10.0.0.1:9".into()],
            ..Default::default()
        };
        conf.build("l", &["10.0.0.1:1", "10.0.0.2:1"], false);
    }

    #[test]
    #[should_panic(expected = "interval rounds to zero")]
    fn rejects_subnanosecond_interval() {
        let conf: FailoverConf = toml::from_str("interval = 1e-12").unwrap();
        conf.build("127.0.0.1:0", &["127.0.0.1:0", "127.0.0.2:0"], false);
    }

    #[test]
    #[should_panic(expected = "timeout rounds to zero")]
    fn rejects_subnanosecond_timeout() {
        let conf: FailoverConf = serde_json::from_str(r#"{"timeout":1e-12}"#).unwrap();
        conf.build("127.0.0.1:0", &["127.0.0.1:0", "127.0.0.2:0"], false);
    }

    #[test]
    #[should_panic(expected = "duplicate data address [::1]:0")]
    fn rejects_duplicate_peer_addresses() {
        FailoverConf::default().build("127.0.0.1:0", &["[::1]:0", "[0:0:0:0:0:0:0:1]:0"], false);
    }

    #[test]
    fn failover_durations_are_capped_at_one_day() {
        for key in ["interval", "timeout", "hold"] {
            let build = |secs| {
                let conf: FailoverConf = toml::from_str(&format!("{} = {}", key, secs)).unwrap();
                conf.build("127.0.0.1:0", &["127.0.0.1:0", "127.0.0.2:0"], false)
            };
            build("86400.0");
            for seconds in ["86400.000001", "1e19"] {
                let message = std::panic::catch_unwind(|| build(seconds))
                    .unwrap_err()
                    .downcast::<String>()
                    .unwrap();
                assert!(
                    message.contains(&format!("{} must be at most 86400 seconds", key)),
                    "{}",
                    message
                );
            }
        }
    }

    #[test]
    fn example_configs() {
        use crate::conf::{Config, FullConf};
        for file in ["examples/failover.toml", "examples/failover.json"] {
            let path = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), file);
            let mut conf = FullConf::from_conf_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            conf.apply_global_opts();
            let info = conf.endpoints.pop().unwrap().build();
            let opts = info.endpoint.conn_opts.failover.expect(file);
            assert!(info.use_udp);
            assert_eq!(info.endpoint.conn_opts.connect_timeout, 4);
            assert_eq!(opts.probe.len(), 2);
            assert_eq!(opts.tcp_send, b"GET / HTTP/1.0\r\n\r\n");
            assert_eq!(opts.udp_send, b"realm-hc:");
        }
    }

    /// `balance` and `failover` together: balancing over the peers that are alive.
    #[cfg(feature = "balance")]
    #[test]
    fn with_balance() {
        use crate::conf::{Config, EndpointConf};
        use realm_core::balance::Strategy;
        let ep: EndpointConf = toml::from_str(
            r#"
            listen = "127.0.0.1:1"
            remote = "10.0.0.1:1"
            extra_remotes = ["10.0.0.2:1"]
            balance = "roundrobin: 1, 1"
            failover = {}
            "#,
        )
        .unwrap();
        let conn_opts = ep.build().endpoint.conn_opts;
        assert!(conn_opts.failover.is_some());
        assert_eq!(conn_opts.balancer.strategy(), Strategy::RoundRobin);
    }

    #[cfg(feature = "balance")]
    #[test]
    #[should_panic(expected = "one weight per peer")]
    fn reject_balance_weight_count() {
        use crate::conf::{Config, EndpointConf};
        let ep: EndpointConf = toml::from_str(
            r#"
            listen = "127.0.0.1:1"
            remote = "10.0.0.1:1"
            extra_remotes = ["10.0.0.2:1"]
            balance = "roundrobin: 1, 1, 1"
            failover = {}
            "#,
        )
        .unwrap();
        ep.build();
    }

    #[test]
    #[should_panic(expected = "fall must be")]
    fn reject_fall_over_window() {
        let conf = FailoverConf {
            fall: Some(6),
            ..Default::default()
        };
        conf.build("l", &["10.0.0.1:1", "10.0.0.2:1"], false);
    }
}
