use std::fs;
use std::io::{Result, Error};
use std::collections::BTreeMap;

use walkdir::WalkDir;
use clap::ArgMatches;
use serde::{Serialize, Deserialize};
use serde::de::IgnoredAny;

mod log;
pub use self::log::{LogLevel, LogConf};

mod dns;
pub use dns::{DnsMode, DnsProtocol, DnsConf};

mod net;
pub use net::{NetConf, NetInfo};

mod endpoint;
pub use endpoint::{EndpointConf, EndpointInfo};

mod failover;
pub use failover::FailoverConf;

mod legacy;
pub use legacy::LegacyConf;

#[allow(clippy::too_long_first_doc_paragraph)]
/// Conig Architecture
/// cmd | file => LogConf => { level, output }
/// cmd | file => DnsConf => { resolve cinfig, opts }
/// cmd | file => NetConf
///                      \
/// cmd | file => EndpointConf => { [local, remote, conn_opts] }
pub trait Config {
    type Output;

    fn is_empty(&self) -> bool;

    fn build(self) -> Self::Output;

    // override self if other not empty
    // e.g.: cmd argument overrides global and local option
    fn rst_field(&mut self, other: &Self) -> &mut Self;

    // take other only if self empty & other not empty
    // e.g.: local field takes global option
    fn take_field(&mut self, other: &Self) -> &mut Self;

    fn from_cmd_args(matches: &ArgMatches) -> Self;
}

#[derive(Debug, Default)]
pub struct CmdOverride {
    pub log: LogConf,
    pub dns: DnsConf,
    pub network: NetConf,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FullConf {
    #[serde(default)]
    #[serde(skip_serializing_if = "Config::is_empty")]
    pub log: LogConf,

    #[serde(default)]
    #[serde(skip_serializing_if = "Config::is_empty")]
    pub dns: DnsConf,

    #[serde(default)]
    #[serde(skip_serializing_if = "Config::is_empty")]
    pub network: NetConf,

    pub endpoints: Vec<EndpointConf>,
}

type UnknownKeys = BTreeMap<String, IgnoredAny>;

// Keep diagnostics on the input side so they cannot be inherited by other rules
// or written back by configuration serialization.
#[derive(Default, Deserialize)]
struct Checked<T> {
    #[serde(flatten)]
    values: T,
    #[serde(flatten)]
    unknown: UnknownKeys,
}

#[derive(Deserialize)]
struct CheckedEndpoint {
    #[serde(default)]
    network: Checked<NetConf>,
    #[serde(flatten)]
    endpoint: Checked<EndpointConf>,
}

#[derive(Deserialize)]
struct CheckedConf {
    #[serde(default)]
    log: Checked<LogConf>,
    #[serde(default)]
    dns: Checked<DnsConf>,
    #[serde(default)]
    network: Checked<NetConf>,
    endpoints: Vec<CheckedEndpoint>,
    #[serde(flatten)]
    unknown: UnknownKeys,
}

impl CheckedConf {
    fn into_config(self, mut warn: impl FnMut(String)) -> FullConf {
        let mut report = |keys: UnknownKeys, context: &str, prefix: &str| {
            for key in keys.into_keys() {
                warn(format!("{}unknown key {}{}, ignored", context, prefix, key));
            }
        };
        report(self.unknown, "top level: ", "");
        report(self.log.unknown, "", "log.");
        report(self.dns.unknown, "", "dns.");
        report(self.network.unknown, "", "network.");

        let endpoints = self
            .endpoints
            .into_iter()
            .map(|ep| {
                let mut endpoint = ep.endpoint.values;
                let context = format!("endpoint {}: ", endpoint.listen);
                report(ep.endpoint.unknown, &context, "");
                report(ep.network.unknown, &context, "network.");
                endpoint.network = ep.network.values;
                endpoint
            })
            .collect();

        FullConf {
            log: self.log.values,
            dns: self.dns.values,
            network: self.network.values,
            endpoints,
        }
    }
}

impl FullConf {
    #[allow(unused)]
    pub fn new(log: LogConf, dns: DnsConf, network: NetConf, endpoints: Vec<EndpointConf>) -> Self {
        FullConf {
            log,
            dns,
            network,
            endpoints,
        }
    }

    pub fn from_conf_file(file: &str) -> Self {
        let mtd = fs::metadata(file).unwrap_or_else(|e| panic!("failed to open {}: {}", file, e));

        if mtd.is_file() {
            let conf = fs::read_to_string(file).unwrap_or_else(|e| panic!("failed to open {}: {}", file, e));
            match Self::from_conf_str(&conf) {
                Ok(x) => return x,
                Err(e) => panic!("failed to parse {}: {}", file, e),
            }
        }

        let mut full_conf = FullConf::default();
        for entry in WalkDir::new(file)
            .follow_links(true)
            .into_iter()
            .filter_entry(|e| e.file_name().to_str().is_some_and(|x| !x.starts_with('.')))
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| e.path().extension().is_some_and(|s| s == "toml" || s == "json"))
        {
            let conf_part = fs::read_to_string(entry.path())
                .unwrap_or_else(|e| panic!("failed to open {}: {}", entry.path().to_string_lossy(), e));

            let conf_part = Self::from_conf_str(&conf_part)
                .unwrap_or_else(|e| panic!("failed to parse {}: {}", entry.path().to_string_lossy(), e));
            full_conf.take_fields(conf_part);
        }
        full_conf
    }

    pub fn from_conf_str(s: &str) -> Result<Self> {
        Self::parse(s, |msg| eprintln!("warning: {}", msg))
    }

    fn parse(s: &str, warn: impl FnMut(String)) -> Result<Self> {
        let toml_err = match toml::from_str::<CheckedConf>(s) {
            Ok(x) => return Ok(x.into_config(warn)),
            Err(e) => e,
        };

        let json_err = match serde_json::from_str::<CheckedConf>(s) {
            Ok(x) => return Ok(x.into_config(warn)),
            Err(e) => e,
        };

        // to be compatible with old version
        let legacy_err = match serde_json::from_str::<LegacyConf>(s) {
            Ok(x) => {
                eprintln!("attention: you are using a legacy config file!");
                return Ok(x.into());
            }
            Err(e) => e,
        };

        Err(Error::other(format!(
            "parse as toml: {0}; parse as json: {1}; parse as legacy: {2}",
            toml_err, json_err, legacy_err
        )))
    }

    fn take_fields(&mut self, other: Self) {
        self.log.take_field(&other.log);
        self.dns.take_field(&other.dns);
        self.network.take_field(&other.network);
        self.endpoints.extend(other.endpoints);
    }

    pub fn add_endpoint(&mut self, endpoint: EndpointConf) -> &mut Self {
        self.endpoints.push(endpoint);
        self
    }

    // override
    pub fn apply_cmd_opts(&mut self, opts: CmdOverride) -> &mut Self {
        let CmdOverride {
            ref log,
            ref dns,
            ref network,
        } = opts;

        self.log.rst_field(log);
        self.dns.rst_field(dns);
        self.endpoints.iter_mut().for_each(|x| {
            x.network.rst_field(network);
        });

        self
    }

    // take inner global opts
    pub fn apply_global_opts(&mut self) -> &mut Self {
        self.endpoints.iter_mut().for_each(|x| {
            x.network.take_field(&self.network);
        });

        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_unknown_keys_once_per_table() {
        let json = r#"{
            "extra": [1, {"object": true}],
            "log": {"outpuut": "stderr"},
            "dns": {"cache_size": 18446744073709551615, "max_ttls": 1},
            "network": {"use_udp": true, "use_udpp": true},
            "endpoints": [
                {"listen": "127.0.0.1:39305", "remote": "127.0.0.1:39309", "remotte": 3,
                 "network": {"use_udp": false, "use_udpp": true}},
                {"listen": "127.0.0.1:39306", "remote": "127.0.0.1:39309"}
            ]
        }"#;
        let toml = r#"
            extra = 1979-05-27T07:32:00Z
            log = { outpuut = "stderr" }
            dns = { cache_size = 18446744073709551615, max_ttls = 1 }
            network = { use_udp = true, use_udpp = true }
            [[endpoints]]
            listen = "127.0.0.1:39305"
            remote = "127.0.0.1:39309"
            remotte = 3
            network = { use_udp = false, use_udpp = true }
            [[endpoints]]
            listen = "127.0.0.1:39306"
            remote = "127.0.0.1:39309"
        "#;
        for input in [json, toml] {
            let mut warnings = Vec::new();
            let mut config = FullConf::parse(input, |msg| warnings.push(msg)).unwrap();
            config.apply_global_opts();
            assert_eq!(
                warnings,
                [
                    "top level: unknown key extra, ignored",
                    "unknown key log.outpuut, ignored",
                    "unknown key dns.max_ttls, ignored",
                    "unknown key network.use_udpp, ignored",
                    "endpoint 127.0.0.1:39305: unknown key remotte, ignored",
                    "endpoint 127.0.0.1:39305: unknown key network.use_udpp, ignored",
                ]
            );
            assert_eq!(config.dns.cache_size, Some(u64::MAX));
            assert_eq!(config.endpoints[0].network.use_udp, Some(false));
            assert_eq!(config.endpoints[1].network.use_udp, Some(true));
        }
    }

    #[test]
    fn reads_legacy_config() {
        let input = r#"{
            "listening_addresses":["127.0.0.1"], "listening_ports":["39307"],
            "remote_addresses":["127.0.0.1"], "remote_ports":["39309"]
        }"#;
        let config = FullConf::parse(input, |msg| panic!("unexpected warning: {}", msg)).unwrap();
        assert_eq!(config.endpoints[0].listen, "127.0.0.1:39307");
        assert_eq!(config.endpoints[0].remote, "127.0.0.1:39309");
    }
}

#[macro_export]
macro_rules! rst {
    ($this: ident, $field: ident, $other: ident) => {
        let Self { $field, .. } = $other;
        if $field.is_some() {
            $this.$field = $field;
        }
    };
}

#[macro_export]
macro_rules! take {
    ($this: ident, $field: ident, $other: ident) => {
        let Self { $field, .. } = $other;
        if $this.$field.is_none() && $field.is_some() {
            $this.$field = $field;
        }
    };
}

#[macro_export]
macro_rules! empty {
    ( $this: expr => $( $field: ident ),* ) => {{
        let mut res = true;
        $(
            res = res && $this.$field.is_none();
        )*
        res
    }};
    ( $( $value: expr ),* ) => {{
        let mut res = true;
        $(
            res = res && $value.is_none();
        )*
        res
    }}
}
