use std::io::Result;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

use tokio::net::UdpSocket;
use realm_syscall::new_udp_socket;

use crate::endpoint::{BindOpts, ConnectOpts};

pub fn bind(laddr: &SocketAddr, bind_opts: BindOpts) -> Result<std::net::UdpSocket> {
    let BindOpts {
        ipv6_only,
        #[cfg(target_os = "linux")]
        bind_interface,
        ..
    } = bind_opts;
    let socket = new_udp_socket(laddr)?;

    // ipv6_only
    if let SocketAddr::V6(_) = laddr {
        socket.set_only_v6(ipv6_only)?;
    }

    #[cfg(target_os = "linux")]
    if let Some(iface) = bind_interface {
        realm_syscall::bind_to_device(&socket, &iface)?;
    }

    socket.bind(&(*laddr).into())?;

    Ok(socket.into())
}

pub fn associate(raddr: &SocketAddr, conn_opts: &ConnectOpts) -> Result<UdpSocket> {
    let ConnectOpts {
        bind_address,

        #[cfg(target_os = "linux")]
        bind_interface,
        ..
    } = conn_opts;

    let socket = new_udp_socket(raddr)?;

    // A fixed source port (`through = ip:port`) is shared by every association, which
    // needs SO_REUSEADDR. Never set it otherwise: with it the kernel may auto-assign a
    // port that another association already holds, and replies reach the wrong client.
    if bind_address.is_some_and(|addr| addr.port() != 0) {
        // ignore error
        let _ = socket.set_reuse_address(true);
    }

    #[cfg(target_os = "linux")]
    if let Some(iface) = bind_interface {
        realm_syscall::bind_to_device(&socket, iface)?;
    }

    // Allocate the source port before the first send so port exhaustion cannot
    // look like a full send buffer on an unbound socket.
    let addr = bind_address.unwrap_or_else(|| match raddr {
        SocketAddr::V4(_) => SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)),
        SocketAddr::V6(_) => SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)),
    });
    socket.bind(&addr.into())?;

    #[cfg(target_os = "linux")]
    if conn_opts.udp_no_fragment {
        no_fragment::set(&socket, true);
    }

    UdpSocket::from_std(socket.into())
}

/// `udp_no_fragment`: forward the way a router does.
///
/// By default the kernel marks our datagrams don't-fragment only until it learns
/// that the path to the destination is narrower, then it fragments them itself. A
/// sender that looks for the path MTU by sending large don't-fragment datagrams
/// (every QUIC stack does) is told they arrived, settles on a size the path cannot
/// carry whole, and from then on each of its datagrams leaves here in two pieces.
/// Many networks carry fragments badly or not at all.
///
/// With this on, a datagram that fits one frame of the outgoing interface always
/// leaves as don't-fragment and is never fragmented here: if the path is narrower
/// it is lost, like behind any router, and the sender backs off. A datagram that
/// needs more than one frame reached us in fragments, so its sender allowed
/// fragmentation and it is still fragmented (see `batched::send_all`).
#[cfg(target_os = "linux")]
pub(crate) mod no_fragment {
    use std::mem::size_of;
    use std::os::unix::io::AsRawFd;
    use std::sync::Mutex;
    use super::super::RateLimit;
    use libc::{c_int, c_void, socklen_t};

    fn sockopt(fd: c_int, level: c_int, name: c_int, val: c_int) -> bool {
        // SAFETY: plain integer option
        unsafe {
            libc::setsockopt(
                fd,
                level,
                name,
                &val as *const c_int as *const c_void,
                size_of::<c_int>() as socklen_t,
            ) == 0
        }
    }

    /// Turn it on, or back to fragmenting whenever needed. The socket may be ipv4,
    /// ipv6 or both, so both levels are set and the one that does not apply fails.
    pub fn set<S: AsRawFd>(sock: &S, on: bool) {
        let fd = sock.as_raw_fd();
        let (v4, v6) = if on {
            (libc::IP_PMTUDISC_PROBE, libc::IPV6_PMTUDISC_PROBE)
        } else {
            (libc::IP_PMTUDISC_DONT, libc::IPV6_PMTUDISC_DONT)
        };
        let a = sockopt(fd, libc::IPPROTO_IP, libc::IP_MTU_DISCOVER, v4);
        let b = sockopt(fd, libc::IPPROTO_IPV6, libc::IPV6_MTU_DISCOVER, v6);
        static LOG: RateLimit = RateLimit::new();
        if !a && !b {
            let error = std::io::Error::last_os_error();
            if let Some(suppressed) = LOG.allow() {
                log::warn!(
                    "[udp]failed to set path mtu discovery: {} ({} suppressed)",
                    error,
                    suppressed
                );
            }
        }
    }

    /// Run `send` with fragmenting allowed on `sock`, then turn [`set`] back on.
    ///
    /// The option belongs to the socket, and the listening socket is shared by the
    /// tasks of all associations. Two of them lifting it at the same moment would
    /// each find it as the other one just left it: one sends while the option is
    /// already back on and its datagram is refused again. So this is done by one
    /// task at a time, and `send` must not block or yield.
    pub fn lifted<S: AsRawFd, T>(sock: &S, send: impl FnOnce() -> T) -> T {
        static LIFTING: Mutex<()> = Mutex::new(());
        let _one_at_a_time = LIFTING.lock().unwrap_or_else(|e| e.into_inner());
        set(sock, false);
        let res = send();
        set(sock, true);
        res
    }

    /// Whether [`set`] is in effect on this socket.
    #[cfg(test)]
    pub fn is_set<S: AsRawFd>(sock: &S) -> bool {
        let fd = sock.as_raw_fd();
        let get = |level, name| {
            let mut val: c_int = -1;
            let mut len = size_of::<c_int>() as socklen_t;
            // SAFETY: plain integer option
            let ok = unsafe { libc::getsockopt(fd, level, name, &mut val as *mut c_int as *mut c_void, &mut len) == 0 };
            ok.then_some(val)
        };
        get(libc::IPPROTO_IP, libc::IP_MTU_DISCOVER) == Some(libc::IP_PMTUDISC_PROBE)
            || get(libc::IPPROTO_IPV6, libc::IPV6_MTU_DISCOVER) == Some(libc::IPV6_PMTUDISC_PROBE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn association_has_a_local_port_before_sending() {
        let raddr = "127.0.0.1:0".parse().unwrap();
        let sock = associate(&raddr, &ConnectOpts::default()).unwrap();
        assert_ne!(sock.local_addr().unwrap().port(), 0);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn no_fragment_is_opt_in() {
        let raddr: SocketAddr = "127.0.0.1:9".parse().unwrap();
        let plain = associate(&raddr, &ConnectOpts::default()).unwrap();
        assert!(!no_fragment::is_set(&plain));

        let opts = ConnectOpts {
            udp_no_fragment: true,
            ..Default::default()
        };
        let sock = associate(&raddr, &opts).unwrap();
        assert!(no_fragment::is_set(&sock));
        // and it can be lifted for one datagram that needs fragments
        no_fragment::set(&sock, false);
        assert!(!no_fragment::is_set(&sock));
    }
}
