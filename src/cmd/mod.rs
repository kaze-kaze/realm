use clap::{Command, ArgMatches};

use crate::conf::CmdOverride;
use crate::conf::EndpointConf;
use crate::conf::{Config, LogConf, DnsConf, NetConf};

use crate::VERSION;
use crate::consts::FEATURES;

mod sub;
mod flag;

#[allow(clippy::large_enum_variant)]
pub enum CmdInput {
    Config(String, CmdOverride),
    Endpoint(EndpointConf, CmdOverride),
    None,
}

#[derive(Default)]
pub struct Startup {
    daemon: bool,
    #[cfg(feature = "hook")]
    pre_conn_hook: Option<String>,
}

impl Startup {
    pub fn daemon(&self) -> bool {
        cfg!(unix) && self.daemon
    }

    pub fn enter(self) {
        #[cfg(unix)]
        if self.daemon {
            realm_core::realm_syscall::daemonize("realm is running in the background");
        }

        #[cfg(feature = "hook")]
        if let Some(path) = self.pre_conn_hook {
            realm_core::hook::pre_conn::load_dylib(&path);
            println!("hook: {}", path);
        }
    }
}

pub fn scan() -> CmdInput {
    let (input, startup) = scan_startup();
    startup.enter();
    input
}

/// Read startup options without forking, so the caller can check its listeners first.
pub fn scan_startup() -> (CmdInput, Startup) {
    let ver = format!("{} {}", VERSION, FEATURES);
    let app = Command::new("Realm").about("A high efficiency relay tool").version(ver);

    let app = app
        .disable_help_flag(true)
        .disable_help_subcommand(true)
        .disable_version_flag(true)
        .arg_required_else_help(true)
        .override_usage("realm [FLAGS] [OPTIONS]");

    let app = flag::add_all(app);
    let app = sub::add_all(app);

    // do other things
    let mut app2 = app.clone();
    let matches = app.get_matches();

    if matches.get_flag("help") {
        app2.print_help().unwrap();
        return (CmdInput::None, Startup::default());
    }

    if matches.get_flag("version") {
        print!("{}", app2.render_version());
        return (CmdInput::None, Startup::default());
    }

    #[allow(clippy::single_match)]
    match matches.subcommand() {
        Some(("convert", sub_matches)) => {
            sub::handle_convert(sub_matches);
            return (CmdInput::None, Startup::default());
        }
        _ => {}
    };

    let startup = Startup {
        daemon: matches.get_flag("daemon"),
        #[cfg(feature = "hook")]
        pre_conn_hook: matches.get_one("pre_conn_hook").cloned(),
    };
    (handle_matches(matches), startup)
}

fn handle_matches(matches: ArgMatches) -> CmdInput {
    #[cfg(all(unix, not(target_os = "android")))]
    if let Some(&nofile) = matches.get_one::<u64>("nofile") {
        if let Err(e) = realm_core::realm_syscall::set_nofile_limit(nofile) {
            eprintln!("warning: failed to set nofile limit to {}: {}", nofile, e);
        }
    }

    #[cfg(target_os = "linux")]
    if let Some(page) = matches.get_one::<usize>("pipe_page") {
        let size = page * flag::PIPE_PAGE_SIZE;
        realm_core::realm_io::set_pipe_size(size);
        println!("pipe capacity: {}", size);
    }

    let opts = parse_global_opts(&matches);

    if let Some(config) = matches.get_one("config").cloned() {
        return CmdInput::Config(config, opts);
    }

    if matches.contains_id("local") && matches.contains_id("remote") {
        let ep = EndpointConf::from_cmd_args(&matches);
        return CmdInput::Endpoint(ep, opts);
    }

    CmdInput::None
}

fn parse_global_opts(matches: &ArgMatches) -> CmdOverride {
    let log = LogConf::from_cmd_args(matches);
    let dns = DnsConf::from_cmd_args(matches);
    let network = NetConf::from_cmd_args(matches);
    CmdOverride { log, dns, network }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_numeric_options() {
        let app = flag::add_all(Command::new("realm").disable_help_flag(true).disable_version_flag(true));
        for option in [
            "--nofile",
            "--pipe-page",
            "--dns-min-ttl",
            "--dns-max-ttl",
            "--dns-cache-size",
            "--send-proxy-version",
            "--accept-proxy-timeout",
            "--tcp-timeout",
            "--udp-timeout",
            "--tcp-keepalive",
            "--tcp-keepalive-probe",
        ] {
            let error = app
                .clone()
                .try_get_matches_from(["realm", option, "invalid"])
                .unwrap_err();
            assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation, "{}", option);
            assert!(error.to_string().contains(option));
        }
        let overflow = usize::MAX.to_string();
        assert!(app
            .clone()
            .try_get_matches_from(["realm", "--pipe-page", &overflow])
            .is_err());
        assert!(app
            .try_get_matches_from(["realm", "--send-proxy-version", "3"])
            .is_err());
    }

    #[test]
    fn rejects_invalid_enum_and_boolean_options() {
        let app = flag::add_all(Command::new("realm").disable_help_flag(true).disable_version_flag(true));
        for option in [
            "--send-proxy",
            "--accept-proxy",
            "--dns-mode",
            "--dns-protocol",
            "--log-level",
        ] {
            let error = app
                .clone()
                .try_get_matches_from(["realm", option, "invalid"])
                .expect_err(option);
            assert!(error.to_string().contains(option));
        }
    }

    #[test]
    fn parses_explicit_enum_and_boolean_options() {
        let opts = parse_global_opts(
            &flag::add_all(Command::new("realm").disable_help_flag(true).disable_version_flag(true))
                .try_get_matches_from([
                    "realm",
                    "--send-proxy",
                    "true",
                    "--accept-proxy",
                    "false",
                    "--dns-mode",
                    "IPV6_ONLY",
                    "--dns-protocol",
                    "tcp+udp",
                    "--log-level",
                    "Debug",
                ])
                .unwrap(),
        );
        assert_eq!(opts.network.send_proxy, Some(true));
        assert_eq!(opts.network.accept_proxy, Some(false));
        assert!(matches!(opts.dns.mode, Some(crate::conf::DnsMode::Ipv6Only)));
        assert_eq!(opts.dns.protocol, Some(crate::conf::DnsProtocol::TcpAndUdp));
        assert!(matches!(opts.log.level, Some(crate::conf::LogLevel::Debug)));
    }

    /// `--tcp-keepalive` once set the probe count as well, and `--tcp-keepalive-probe` did nothing.
    #[test]
    fn keepalive_options_are_separate() {
        let net = |args: &[&str]| {
            let app = Command::new("realm").disable_help_flag(true).disable_version_flag(true);
            let matches = flag::add_all(app)
                .try_get_matches_from(std::iter::once(&"realm").chain(args))
                .unwrap();
            let net = NetConf::from_cmd_args(&matches);
            (net.tcp_keepalive, net.tcp_keepalive_probe)
        };
        assert_eq!(net(&[]), (None, None));
        assert_eq!(net(&["--tcp-keepalive", "30"]), (Some(30), None));
        assert_eq!(net(&["--tcp-keepalive-probe", "2"]), (None, Some(2)));
        assert_eq!(
            net(&["--tcp-keepalive", "5", "--tcp-keepalive-probe", "2"]),
            (Some(5), Some(2))
        );
    }
}
