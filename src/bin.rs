use std::env;
use std::io;
use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::process::ExitCode;
use cfg_if::cfg_if;

use realm::cmd;
use realm::conf::{Config, FullConf, LogConf, DnsConf, EndpointConf, EndpointInfo};
use realm::core::{tcp, udp};
use realm::core::endpoint::Endpoint;
use realm::core::failover::Proto;
use realm::ENV_CONFIG;

cfg_if! {
    if #[cfg(feature = "mi-malloc")] {
        use mimalloc::MiMalloc;
        #[global_allocator]
        static GLOBAL: MiMalloc = MiMalloc;
    } else if #[cfg(all(feature = "jemalloc", not(target_env = "msvc")))] {
        use jemallocator::Jemalloc;
        #[global_allocator]
        static GLOBAL: Jemalloc = Jemalloc;
    } else if #[cfg(all(feature = "page-alloc", unix))] {
        use mmap_allocator::MmapAllocator;
        #[global_allocator]
        static GLOBAL: MmapAllocator = MmapAllocator::new();
    }
}

fn main() -> ExitCode {
    let (conf, startup) = 'blk: {
        if let Ok(conf_str) = env::var(ENV_CONFIG) {
            match conf_from_env(&conf_str) {
                Ok(conf) => break 'blk (conf, cmd::Startup::default()),
                // the command line is used instead, as before, but not without a word
                Err(e) => eprintln!("warning: {} is not a valid config, ignored: {}", ENV_CONFIG, e),
            }
        };

        use cmd::CmdInput;
        let (input, startup) = cmd::scan_startup();
        let conf = match input {
            CmdInput::Endpoint(ep, opts) => {
                let mut conf = FullConf::default();
                conf.add_endpoint(ep).apply_global_opts().apply_cmd_opts(opts);
                conf
            }
            CmdInput::Config(conf, opts) => {
                let mut conf = FullConf::from_conf_file(&conf);
                conf.apply_global_opts().apply_cmd_opts(opts);
                conf
            }
            CmdInput::None => return ExitCode::SUCCESS,
        };
        (conf, startup)
    };

    setup_nofile();
    start_from_conf(conf, startup)
}

/// The config in `REALM_CONF`. Its global options reach the endpoints like those of
/// a config file do.
fn conf_from_env(conf_str: &str) -> std::io::Result<FullConf> {
    let mut conf = FullConf::from_conf_str(conf_str)?;
    conf.apply_global_opts();
    Ok(conf)
}

fn start_from_conf(full: FullConf, startup: cmd::Startup) -> ExitCode {
    let FullConf {
        log: log_conf,
        dns: dns_conf,
        endpoints: endpoints_conf,
        ..
    } = full;

    // A daemon configures its logger after the fork sets the working umask.
    let deferred_log = if startup.daemon() {
        Some(log_conf)
    } else {
        setup_log(log_conf);
        None
    };
    setup_dns(dns_conf);
    setup_transport();

    let endpoints = build_endpoints(endpoints_conf);
    let Some(listeners) = bind_listeners(&endpoints) else {
        return ExitCode::FAILURE;
    };

    startup.enter();
    if let Some(log) = deferred_log {
        setup_log(log);
    }
    for ep in &endpoints {
        println!("inited: {}", ep.endpoint);
    }
    execute(listeners)
}

fn build_endpoints(configs: Vec<EndpointConf>) -> Vec<EndpointInfo> {
    let endpoints: Vec<_> = configs.into_iter().map(Config::build).collect();
    assert!(
        endpoints.iter().any(|ep| ep.use_udp || !ep.no_tcp),
        "no endpoints listen on TCP or UDP"
    );
    endpoints
}

fn setup_nofile() {
    #[cfg(all(unix, not(target_os = "android")))]
    {
        use realm::core::realm_syscall::{bump_nofile_limit, get_nofile_limit};

        if let Err(e) = bump_nofile_limit() {
            eprintln!("warning: failed to raise nofile limit: {}", e);
        }
        if let Ok((soft, hard)) = get_nofile_limit() {
            println!("fd: soft={}, hard={}", soft, hard);
        }
    }
}

fn setup_log(log: LogConf) {
    println!("log: {}", log);

    let (level, output) = log.build();
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}]{}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        .level(level)
        .chain(output)
        .apply()
        .unwrap_or_else(|e| panic!("failed to setup logger: {}", e))
}

fn setup_dns(dns: DnsConf) {
    println!("dns: {}", dns);

    let (conf, opts) = dns.build();
    realm::core::dns::build_lazy(conf, opts);
}

fn setup_transport() {
    #[cfg(feature = "transport")]
    {
        realm::core::kaminari::install_tls_provider();
    }
}

fn execute(listeners: BoundListeners<'_>) -> ! {
    #[cfg(feature = "multi-thread")]
    {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(run(listeners))
    }

    #[cfg(not(feature = "multi-thread"))]
    {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(run(listeners))
    }
}

enum Listener {
    Tcp(TcpListener),
    Udp(UdpSocket),
}

impl Listener {
    async fn run(self, endpoint: Endpoint) -> io::Result<()> {
        match self {
            Self::Tcp(lis) => tcp::run(lis, endpoint).await,
            Self::Udp(lis) => udp::run(lis, endpoint).await,
        }
    }
}

type BoundListeners<'a> = Vec<(Proto, &'a Endpoint, Listener)>;

fn bind_listeners(endpoints: &[EndpointInfo]) -> Option<BoundListeners<'_>> {
    let mut listeners = Vec::new();
    let mut failed = false;

    for EndpointInfo {
        endpoint,
        no_tcp,
        use_udp,
    } in endpoints
    {
        let bindings = [
            (Proto::Udp, use_udp.then(|| udp::bind(endpoint).map(Listener::Udp))),
            (Proto::Tcp, (!no_tcp).then(|| tcp::bind(endpoint).map(Listener::Tcp))),
        ];
        for (proto, binding) in bindings {
            match binding {
                Some(Ok(listener)) => listeners.push((proto, endpoint, listener)),
                Some(Err(e)) => {
                    eprintln!("[{}] failed to bind {}: {}", proto, endpoint.laddr, e);
                    failed = true;
                }
                None => {}
            }
        }
    }
    (!failed).then_some(listeners)
}

async fn run(listeners: BoundListeners<'_>) -> ! {
    let workers = listeners.into_iter().map(|(proto, endpoint, listener)| {
        let worker = tokio::spawn(listener.run(endpoint.clone()));
        (proto, endpoint.laddr, worker)
    });
    supervise(workers).await
}

type ListenerTask = (Proto, SocketAddr, tokio::task::JoinHandle<io::Result<()>>);

async fn supervise(workers: impl IntoIterator<Item = ListenerTask>) -> ! {
    use futures::stream::{FuturesUnordered, StreamExt};

    let mut workers: FuturesUnordered<_> = workers
        .into_iter()
        .map(|(proto, addr, worker)| async move { (proto, addr, worker.await) })
        .collect();

    let (proto, addr, result) = workers.next().await.expect("no listeners to supervise");
    match result {
        Ok(Ok(())) => eprintln!("[{}] listener {} exited", proto, addr),
        Ok(Err(e)) => eprintln!("[{}] listener {} exited: {}", proto, addr, e),
        Err(e) => eprintln!("[{}] listener {} exited: {}", proto, addr, e),
    }
    // The service manager must restart the whole process when any listener exits.
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(port: u16) -> EndpointInfo {
        let conf = format!("listen = '127.0.0.1:{}'\nremote = '127.0.0.1:39309'", port);
        toml::from_str::<EndpointConf>(&conf).unwrap().build()
    }

    #[tokio::test]
    async fn tcp_binding_failure_is_returned() {
        let ep = endpoint(39300).endpoint;
        let _listener = tcp::bind(&ep).unwrap();
        let error = tcp::run_tcp(ep).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AddrInUse);
    }

    #[tokio::test]
    async fn udp_listeners_cannot_share_an_address() {
        let ep = endpoint(39301).endpoint;
        let _listener = udp::bind(&ep).unwrap();
        let error = udp::run_udp(ep).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AddrInUse);
    }

    #[test]
    fn failed_startup_releases_successful_bindings() {
        let available = endpoint(39302);
        let occupied = endpoint(39303);
        let _listener = tcp::bind(&occupied.endpoint).unwrap();
        let ep = available.endpoint.clone();
        assert!(bind_listeners(&[available, occupied]).is_none());
        assert!(tcp::bind(&ep).is_ok());
    }

    #[test]
    fn listener_exit_stops_process() {
        const EXIT_MODE: &str = "REALM_TEST_LISTENER_EXIT";
        if let Ok(mode) = env::var(EXIT_MODE) {
            tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .unwrap()
                .block_on(async {
                    let pending = tokio::spawn(std::future::pending());
                    let finished = tokio::spawn(async move {
                        match mode.as_str() {
                            "return" => Ok(()),
                            "error" => Err(io::Error::other("test listener error")),
                            _ => panic!("test listener panic"),
                        }
                    });
                    let workers = [
                        (Proto::Udp, "127.0.0.1:39310".parse().unwrap(), pending),
                        (Proto::Tcp, "127.0.0.1:39311".parse().unwrap(), finished),
                    ];
                    tokio::time::timeout(std::time::Duration::from_secs(1), supervise(workers))
                        .await
                        .expect("supervisor waited for another listener");
                });
        }

        for mode in ["return", "error", "panic"] {
            let child = std::process::Command::new(env::current_exe().unwrap())
                .args(["--exact", "tests::listener_exit_stops_process", "--nocapture"])
                .env(EXIT_MODE, mode)
                .output()
                .unwrap();
            let stderr = String::from_utf8_lossy(&child.stderr);
            assert_eq!(child.status.code(), Some(1), "{}: {}", mode, stderr);
            assert!(stderr.contains("[tcp] listener 127.0.0.1:39311 exited"), "{}", stderr);
        }
    }

    #[test]
    #[should_panic(expected = "no endpoints listen on TCP or UDP")]
    fn rejects_config_without_listeners() {
        let conf = conf_from_env(
            r#"{"network":{"no_tcp":true},"endpoints":[{"listen":"127.0.0.1:39304","remote":"127.0.0.1:39309"}]}"#,
        )
        .unwrap();
        build_endpoints(conf.endpoints);
    }

    /// The global `network` of a config in `REALM_CONF` was once left out: `use_udp`
    /// there started no udp relay.
    #[test]
    fn env_config_takes_global_options() {
        let conf = conf_from_env(
            r#"{
                "network": { "use_udp": true, "tcp_timeout": 4 },
                "endpoints": [
                    { "listen": "127.0.0.1:1", "remote": "10.0.0.1:1" },
                    { "listen": "127.0.0.1:2", "remote": "10.0.0.1:2", "network": { "tcp_timeout": 7 } }
                ]
            }"#,
        )
        .unwrap();
        let eps: Vec<EndpointInfo> = conf.endpoints.into_iter().map(Config::build).collect();
        assert!(eps[0].use_udp && eps[1].use_udp);
        assert_eq!(eps[0].endpoint.conn_opts.connect_timeout, 4);
        // an endpoint's own option still wins
        assert_eq!(eps[1].endpoint.conn_opts.connect_timeout, 7);

        assert!(conf_from_env("not a config").is_err());
    }
}
