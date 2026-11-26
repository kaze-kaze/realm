use std::io::{Error, ErrorKind, Result};
use std::cell::OnceCell;
use std::net::{SocketAddr, IpAddr, Ipv4Addr, Ipv6Addr};

use log::{info, debug};

use proxy_protocol::ProxyHeader;
use proxy_protocol::{version1 as v1, version2 as v2};
use proxy_protocol::{encode, parse};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::endpoint::ProxyOpts;
use crate::time::timeoutfut;

// TODO: replace the "proxy-protocol" crate, and then avoid heap allocation.

// client -> relay -> server
pub async fn handle_proxy(src: &mut TcpStream, dst: &mut TcpStream, opts: ProxyOpts) -> Result<()> {
    let ProxyOpts {
        send_proxy,
        accept_proxy,
        send_proxy_version,
        accept_proxy_timeout,
    } = opts;

    let client_addr = OnceCell::<SocketAddr>::new();
    let server_addr = OnceCell::<SocketAddr>::new();

    // parse PROXY header from client and write log
    // may not get src and dst addr
    if accept_proxy {
        let header = accept_header(src, accept_proxy_timeout).await?;
        if let Some((src, dst)) = handle_header(header) {
            client_addr.set(src).unwrap();
            server_addr.set(dst).unwrap();
        }

        // do not send header to server
        if !send_proxy {
            return Ok(());
        }
    }

    // use real addr
    if !client_addr.get().is_some() {
        client_addr.set(src.peer_addr()?).unwrap();
        // FIXME: what is the dst addr here? seems not defined in the doc
        // the doc only mentions that this field is similar to X-Origin-To
        // which is seldom used
        server_addr
            .set(match client_addr.get().unwrap() {
                SocketAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0),
                SocketAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0)), 0),
            })
            .unwrap();
    }

    // Safety: sockaddr is always initialized
    // either parse from PROXY header or use real addr
    let client_addr = client_addr.into_inner().unwrap();
    let server_addr = server_addr.into_inner().unwrap();

    // write header
    let header = encode(make_header(client_addr, server_addr, send_proxy_version)).map_err(Error::other)?;
    debug!("[tcp]send initial {} bytes: {:#x}", header.len(), &header);
    dst.write_all(&header).await?;

    Ok(())
}

macro_rules! unpack {
    ($addr: expr, sin4) => {
        match $addr {
            SocketAddr::V4(x) => x,
            _ => unreachable!(),
        }
    };
    ($addr: expr, sin6) => {
        match $addr {
            SocketAddr::V6(x) => x,
            _ => unreachable!(),
        }
    };
}

const V1_PREFIX: &[u8; 6] = b"PROXY ";
const V2_SIGNATURE: &[u8; 12] = b"\r\n\r\n\0\r\nQUIT\n";
const V1_MAX_LEN: usize = 107;
const V2_MAX_PAYLOAD: usize = 4096;

async fn accept_header(src: &mut TcpStream, timeout: usize) -> Result<ProxyHeader> {
    timeoutfut(read_header(src), timeout).await?
}

async fn read_prefix(src: &mut TcpStream, buf: &mut [u8], valid: impl Fn(&[u8]) -> bool) -> Result<()> {
    let mut len = 0;
    while len < buf.len() {
        let n = src.read(&mut buf[len..]).await?;
        if n == 0 {
            return Err(ErrorKind::UnexpectedEof.into());
        }
        len += n;
        if !valid(&buf[..len]) {
            return Err(Error::new(ErrorKind::InvalidData, "invalid PROXY header prefix"));
        }
    }
    Ok(())
}

async fn read_header(src: &mut TcpStream) -> Result<ProxyHeader> {
    let mut prefix = [0; 6];
    read_prefix(src, &mut prefix, |bytes| {
        V1_PREFIX.starts_with(bytes) || V2_SIGNATURE.starts_with(bytes)
    })
    .await?;
    if &prefix == V1_PREFIX {
        return read_v1_header(src, prefix).await;
    }

    let mut fixed = [0; 16];
    fixed[..6].copy_from_slice(&prefix);
    let signature = &V2_SIGNATURE[6..];
    read_prefix(src, &mut fixed[6..], |bytes| {
        signature.starts_with(&bytes[..bytes.len().min(signature.len())])
            && matches!(bytes.get(signature.len()), None | Some(0x20 | 0x21))
    })
    .await?;
    let payload_len = u16::from_be_bytes([fixed[14], fixed[15]]) as usize;
    if payload_len > V2_MAX_PAYLOAD {
        return Err(Error::new(ErrorKind::InvalidData, "PROXY v2 payload is too long"));
    }
    let mut buf = Vec::with_capacity(fixed.len() + payload_len);
    buf.extend_from_slice(&fixed);
    buf.resize(fixed.len() + payload_len, 0);
    src.read_exact(&mut buf[fixed.len()..]).await?;
    parse(&mut buf.as_slice()).map_err(Error::other)
}

async fn read_v1_header(src: &mut TcpStream, prefix: [u8; 6]) -> Result<ProxyHeader> {
    let mut buf = [0; V1_MAX_LEN];
    let mut len = prefix.len();
    buf[..len].copy_from_slice(&prefix);
    while len < buf.len() {
        let seen = src.peek(&mut buf[len..]).await?;
        if seen == 0 {
            return Err(ErrorKind::UnexpectedEof.into());
        }
        // Include the last consumed byte in case the CR and LF arrive separately.
        let end = buf[len - 1..len + seen].windows(2).position(|bytes| bytes == b"\r\n");
        let take = end.map_or(seen, |i| i + 1);
        // Consume every byte examined up to the delimiter so peek can wait for
        // fresh input and application data stays on the socket.
        src.read_exact(&mut buf[len..len + take]).await?;
        len += take;
        if end.is_some() {
            return parse(&mut &buf[..len]).map_err(Error::other);
        }
    }
    Err(Error::new(ErrorKind::InvalidData, "PROXY v1 header is too long"))
}

fn make_header(client_addr: SocketAddr, server_addr: SocketAddr, send_proxy_version: usize) -> ProxyHeader {
    match send_proxy_version {
        2 => make_header_v2(client_addr, server_addr),
        1 => make_header_v1(client_addr, server_addr),
        _ => unreachable!(),
    }
}

fn make_header_v1(client_addr: SocketAddr, server_addr: SocketAddr) -> ProxyHeader {
    debug!("[tcp]send proxy-protocol-v1: {} => {}", client_addr, server_addr);

    if client_addr.is_ipv4() {
        ProxyHeader::Version1 {
            addresses: v1::ProxyAddresses::Ipv4 {
                source: unpack!(client_addr, sin4),
                destination: unpack!(server_addr, sin4),
            },
        }
    } else {
        ProxyHeader::Version1 {
            addresses: v1::ProxyAddresses::Ipv6 {
                source: unpack!(client_addr, sin6),
                destination: unpack!(server_addr, sin6),
            },
        }
    }
}

fn make_header_v2(client_addr: SocketAddr, server_addr: SocketAddr) -> ProxyHeader {
    debug!("[tcp]send proxy-protocol-v2: {} => {}", client_addr, server_addr);

    ProxyHeader::Version2 {
        command: v2::ProxyCommand::Proxy,
        transport_protocol: v2::ProxyTransportProtocol::Stream,
        addresses: if client_addr.is_ipv4() {
            v2::ProxyAddresses::Ipv4 {
                source: unpack!(client_addr, sin4),
                destination: unpack!(server_addr, sin4),
            }
        } else {
            v2::ProxyAddresses::Ipv6 {
                source: unpack!(client_addr, sin6),
                destination: unpack!(server_addr, sin6),
            }
        },
    }
}

fn handle_header(header: ProxyHeader) -> Option<(SocketAddr, SocketAddr)> {
    use ProxyHeader::{Version1, Version2};
    match header {
        Version1 { addresses } => handle_header_v1(addresses),
        Version2 {
            command,
            transport_protocol,
            addresses,
        } => handle_header_v2(command, transport_protocol, addresses),
        _ => {
            info!("[tcp]accept proxy-protocol-v?");
            None
        }
    }
}

fn handle_header_v1(addr: v1::ProxyAddresses) -> Option<(SocketAddr, SocketAddr)> {
    use v1::ProxyAddresses::*;
    match addr {
        Unknown => {
            info!("[tcp]accept proxy-protocol-v1: unknown");
            None
        }
        Ipv4 { source, destination } => {
            info!("[tcp]accept proxy-protocol-v1: {} => {}", source, destination);
            Some((SocketAddr::V4(source), SocketAddr::V4(destination)))
        }
        Ipv6 { source, destination } => {
            info!("[tcp]accept proxy-protocol-v1: {} => {}", source, destination);
            Some((SocketAddr::V6(source), SocketAddr::V6(destination)))
        }
    }
}

fn handle_header_v2(
    cmd: v2::ProxyCommand,
    proto: v2::ProxyTransportProtocol,
    addr: v2::ProxyAddresses,
) -> Option<(SocketAddr, SocketAddr)> {
    use v2::ProxyCommand as Command;
    use v2::ProxyAddresses as Address;
    use v2::ProxyTransportProtocol as Protocol;

    // The connection endpoints are the sender and the receiver.
    // Such connections exist when the proxy sends health-checks to the server.
    // The receiver must accept this connection as valid and must use the
    // real connection endpoints and discard the protocol block including the
    // family which is ignored
    if let Command::Local = cmd {
        info!("[tcp]accept proxy-protocol-v2: command = LOCAL, ignore");
        return None;
    }

    // only get tcp address
    match proto {
        Protocol::Stream => {}
        Protocol::Unspec => {
            info!("[tcp]accept proxy-protocol-v2: protocol = UNSPEC, ignore");
            return None;
        }
        Protocol::Datagram => {
            info!("[tcp]accept proxy-protocol-v2: protocol = DGRAM, ignore");
            return None;
        }
    }

    match addr {
        Address::Ipv4 { source, destination } => {
            info!("[tcp]accept proxy-protocol-v2: {} => {}", source, destination);
            Some((SocketAddr::V4(source), SocketAddr::V4(destination)))
        }
        Address::Ipv6 { source, destination } => {
            info!("[tcp]accept proxy-protocol-v2: {} => {}", source, destination);
            Some((SocketAddr::V6(source), SocketAddr::V6(destination)))
        }
        Address::Unspec => {
            info!("[tcp]accept proxy-protocol-v2: af_family = AF_UNSPEC, ignore");
            None
        }
        Address::Unix { .. } => {
            info!("[tcp]accept proxy-protocol-v2: af_family = AF_UNIX, ignore");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::net::TcpListener;
    use tokio::time::{sleep, timeout};

    const V1: &[u8] = b"PROXY TCP4 127.0.0.1 127.0.0.2 123 456\r\n";
    const V2: &[u8] = b"\r\n\r\n\0\r\nQUIT\n\x21\x11\0\x10\x7f\0\0\x01\x7f\0\0\x02\0\x7b\x01\xc8\x04\0\x01\0";

    async fn pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let tx = TcpStream::connect(listener.local_addr().unwrap()).await.unwrap();
        let (rx, _) = listener.accept().await.unwrap();
        (tx, rx)
    }

    #[tokio::test]
    async fn split_proxy_headers_leave_application_bytes_unread() {
        for header in [V1, V2] {
            for split in 1..header.len() {
                let (mut tx, mut rx) = pair().await;
                let send = async {
                    tx.write_all(&header[..split]).await.unwrap();
                    sleep(Duration::from_millis(2)).await;
                    let tail = [&header[split..], b"app data"].concat();
                    tx.write_all(&tail).await.unwrap();
                    tx.shutdown().await.unwrap();
                };
                let receive = async {
                    let decoded = accept_header(&mut rx, 1).await.unwrap();
                    let source = "127.0.0.1:123".parse().unwrap();
                    let dest = "127.0.0.2:456".parse().unwrap();
                    assert_eq!(handle_header(decoded), Some((source, dest)), "split {}", split);
                    let mut data = Vec::new();
                    rx.read_to_end(&mut data).await.unwrap();
                    assert_eq!(data, b"app data");
                };
                timeout(Duration::from_secs(2), async { tokio::join!(send, receive) })
                    .await
                    .unwrap();
            }
        }
    }

    #[tokio::test]
    async fn rejects_truncated_malformed_and_overlong_proxy_headers() {
        let mut long_v1 = b"PROXY UNKNOWN ".to_vec();
        long_v1.resize(V1_MAX_LEN - 1, b'x');
        long_v1.extend_from_slice(b"\r\n");
        let mut long_v2 = V2[..16].to_vec();
        long_v2[14..16].copy_from_slice(&((V2_MAX_PAYLOAD + 1) as u16).to_be_bytes());
        let mut invalid_v2 = V2.to_vec();
        invalid_v2[12] = 0x22;
        for input in (0..V1.len())
            .map(|n| &V1[..n])
            .chain((0..V2.len()).map(|n| &V2[..n]))
            .chain([b"PROXY TCP4\r\n".as_slice(), &invalid_v2, &long_v1, &long_v2])
        {
            let (mut tx, mut rx) = pair().await;
            tx.write_all(input).await.unwrap();
            tx.shutdown().await.unwrap();
            let err = accept_header(&mut rx, 1).await.unwrap_err();
            assert_ne!(err.kind(), ErrorKind::TimedOut, "input {:?}", input);
        }
    }

    #[tokio::test]
    async fn rejects_an_impossible_signature_or_command_without_waiting_for_more_bytes() {
        for prefix in [
            b"x".as_slice(),
            b"PRx",
            b"\r\nx",
            b"\r\n\r\n\0\rx",
            b"\r\n\r\n\0\r\nQUIT\n\x22",
        ] {
            let (mut tx, mut rx) = pair().await;
            tx.write_all(prefix).await.unwrap();
            let err = timeout(Duration::from_secs(1), accept_header(&mut rx, 0))
                .await
                .unwrap()
                .unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidData, "prefix {:?}", prefix);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn proxy_header_has_one_deadline_across_reads() {
        let (mut tx, mut rx) = pair().await;
        let send = async {
            sleep(Duration::from_millis(400)).await;
            tx.write_all(&V2[..6]).await.unwrap();
            sleep(Duration::from_millis(400)).await;
            tx.write_all(&V2[6..16]).await.unwrap();
        };
        let start = tokio::time::Instant::now();
        let receive = async {
            let err = accept_header(&mut rx, 1).await.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::TimedOut);
            assert!((Duration::from_secs(1)..Duration::from_millis(1020)).contains(&start.elapsed()));
        };
        tokio::join!(send, receive);
    }
}
