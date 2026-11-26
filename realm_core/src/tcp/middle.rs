use std::io::Result;
use tokio::net::TcpStream;

use super::socket;
use super::plain;

#[cfg(feature = "hook")]
use super::hook;

#[cfg(feature = "proxy")]
use super::proxy;

#[cfg(feature = "transport")]
use super::transport;

use crate::trick::Ref;
use crate::endpoint::{RemoteAddr, ConnectOpts};
use crate::failover::Failover;

#[allow(unused)]
pub async fn connect_and_relay(
    mut local: TcpStream,
    raddr: Ref<RemoteAddr>,
    conn_opts: Ref<ConnectOpts>,
    extra_raddrs: Ref<Vec<RemoteAddr>>,
    failover: Ref<Option<Failover>>,
) -> Result<()> {
    let ConnectOpts {
        #[cfg(feature = "proxy")]
        proxy_opts,

        #[cfg(feature = "transport")]
        transport,

        #[cfg(feature = "balance")]
        balancer,

        #[cfg(feature = "transport")]
        connect_timeout,
        ..
    } = conn_opts.as_ref();

    let mut remote = match failover.as_ref() {
        // failover: the hook may still reject, peer selection belongs to failover,
        // which follows the balancer among the peers that are alive.
        Some(fo) => {
            #[cfg(feature = "hook")]
            hook::pre_connect_hook(&mut local, raddr.as_ref(), extra_raddrs.as_ref()).await?;

            let want = fo.balanced_peer(conn_opts.as_ref(), &local.peer_addr()?.ip(), fo.snapshot());
            let (remote, idx) = socket::connect_failover(fo, conn_opts.as_ref(), want).await?;
            log::info!("[tcp]{} => peer#{} {}", local.peer_addr()?, idx, fo.peer(idx));
            remote
        }
        None => {
            // before connect:
            // - pre-connect hook
            // - load balance
            // ..
            let raddr = {
                #[cfg(feature = "hook")]
                {
                    // accept or deny connection.
                    #[cfg(feature = "balance")]
                    {
                        hook::pre_connect_hook(&mut local, raddr.as_ref(), extra_raddrs.as_ref()).await?;
                    }

                    // accept or deny connection, or select a remote peer.
                    #[cfg(not(feature = "balance"))]
                    {
                        hook::pre_connect_hook(&mut local, raddr.as_ref(), extra_raddrs.as_ref()).await?
                    }
                }

                #[cfg(feature = "balance")]
                {
                    use realm_lb::{Token, BalanceCtx};
                    let token = balancer.next(BalanceCtx {
                        src_ip: &local.peer_addr()?.ip(),
                    });
                    log::debug!("[tcp]select remote peer, token: {:?}", token);
                    match token {
                        None | Some(Token(0)) => raddr.as_ref(),
                        Some(Token(idx)) => &extra_raddrs.as_ref()[idx as usize - 1],
                    }
                }

                #[cfg(not(any(feature = "hook", feature = "balance")))]
                raddr.as_ref()
            };

            // connect!
            let remote = socket::connect(raddr, conn_opts.as_ref()).await?;
            log::info!("[tcp]{} => {} as {}", local.peer_addr()?, raddr, remote.peer_addr()?);
            remote
        }
    };

    // after connected
    // ..
    #[cfg(feature = "proxy")]
    if proxy_opts.enabled() {
        proxy::handle_proxy(&mut local, &mut remote, *proxy_opts).await?;
    }

    // relay
    let half_close_timeout = conn_opts.as_ref().half_close_timeout();
    let res = {
        #[cfg(feature = "transport")]
        {
            if let Some((ac, cc)) = transport {
                transport::run_relay(local, remote, ac, cc, *connect_timeout, half_close_timeout).await
            } else {
                plain::run_relay(local, remote, half_close_timeout).await
            }
        }
        #[cfg(not(feature = "transport"))]
        {
            plain::run_relay(local, remote, half_close_timeout).await
        }
    };

    // ignore relay error
    if let Err(e) = res {
        log::debug!("[tcp]forward error: {}, ignored", e);
    }

    Ok(())
}
