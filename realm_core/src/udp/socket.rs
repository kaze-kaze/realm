use std::io::Result;
use std::net::SocketAddr;

use tokio::net::UdpSocket;
use realm_syscall::new_udp_socket;

use crate::endpoint::{BindOpts, ConnectOpts};

pub fn bind(laddr: &SocketAddr, bind_opts: BindOpts) -> Result<UdpSocket> {
    let BindOpts {
        ipv6_only,
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

    // ignore error
    let _ = socket.set_reuse_address(true);

    socket.bind(&(*laddr).into())?;

    UdpSocket::from_std(socket.into())
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

    if let Some(addr) = *bind_address {
        socket.bind(&addr.into())?;
    }

    #[cfg(target_os = "linux")]
    if let Some(iface) = bind_interface {
        realm_syscall::bind_to_device(&socket, iface)?;
    }

    UdpSocket::from_std(socket.into())
}
