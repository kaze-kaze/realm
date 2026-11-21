use std::env;
use cfg_if::cfg_if;

use realm::cmd;
use realm::conf::{Config, FullConf, LogConf, DnsConf, EndpointInfo};
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

fn main() {
    let conf = 'blk: {
        if let Ok(conf_str) = env::var(ENV_CONFIG) {
            match conf_from_env(&conf_str) {
                Ok(conf) => break 'blk conf,
                // the command line is used instead, as before, but not without a word
                Err(e) => eprintln!("warning: {} is not a valid config, ignored: {}", ENV_CONFIG, e),
            }
        };

        use cmd::CmdInput;
        match cmd::scan() {
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
            CmdInput::None => std::process::exit(0),
        }
    };

    start_from_conf(conf);
}

/// The config in `REALM_CONF`. Its global options reach the endpoints like those of
/// a config file do.
fn conf_from_env(conf_str: &str) -> std::io::Result<FullConf> {
    let mut conf = FullConf::from_conf_str(conf_str)?;
    conf.apply_global_opts();
    Ok(conf)
}

fn start_from_conf(full: FullConf) {
    let FullConf {
        log: log_conf,
        dns: dns_conf,
        endpoints: endpoints_conf,
        ..
    } = full;

    setup_log(log_conf);
    setup_dns(dns_conf);
    setup_transport();

    let endpoints: Vec<EndpointInfo> = endpoints_conf
        .into_iter()
        .map(Config::build)
        .inspect(|x| println!("inited: {}", x.endpoint))
        .collect();

    execute(endpoints);
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

fn execute(eps: Vec<EndpointInfo>) {
    #[cfg(feature = "multi-thread")]
    {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(run(eps))
    }

    #[cfg(not(feature = "multi-thread"))]
    {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(run(eps))
    }
}

async fn run(endpoints: Vec<EndpointInfo>) {
    use realm::core::tcp::run_tcp;
    use realm::core::udp::run_udp;
    use futures::future::join_all;

    let mut workers = Vec::with_capacity(2 * endpoints.len());

    for EndpointInfo {
        endpoint,
        no_tcp,
        use_udp,
    } in endpoints
    {
        if use_udp {
            workers.push(tokio::spawn(run_udp(endpoint.clone())));
        }

        if !no_tcp {
            workers.push(tokio::spawn(run_tcp(endpoint)));
        }
    }

    workers.shrink_to_fit();

    join_all(workers).await;
}

#[cfg(test)]
mod tests {
    use super::*;

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
