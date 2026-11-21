use std::io::Result;
use std::net::SocketAddr;
use tokio::net::UdpSocket;

/// Bytes of a datagram kept inline in every slot: whatever fits one ethernet frame.
pub const PACKET_SIZE: usize = 1500;
/// Room for the rest of a larger datagram while it is received. Together with the
/// inline part this exceeds any udp payload (65527 bytes at most), so a datagram
/// is never cut short.
const SPILL_SIZE: usize = 1 << 16;
pub const MAX_PACKETS: usize = 128;

#[repr(transparent)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SockAddrStore {
    #[cfg(all(target_os = "linux", feature = "batched-udp"))]
    inner: realm_io::mmsg::SockAddrStore,

    #[cfg(not(all(target_os = "linux", feature = "batched-udp")))]
    inner: std::net::SocketAddr,
}

impl SockAddrStore {
    pub fn new() -> Self {
        Self {
            #[cfg(all(target_os = "linux", feature = "batched-udp"))]
            inner: realm_io::mmsg::SockAddrStore::new(),

            #[cfg(not(all(target_os = "linux", feature = "batched-udp")))]
            inner: {
                use std::net::{IpAddr, Ipv4Addr};
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0)
            },
        }
    }
}

impl SockAddrStore {
    /// Decode a kernel-filled address.
    #[inline]
    pub fn to_socket_addr(&self) -> SocketAddr {
        self.inner.clone().into()
    }
}

impl Default for SockAddrStore {
    fn default() -> Self {
        Self::new()
    }
}

impl From<SocketAddr> for SockAddrStore {
    fn from(value: SocketAddr) -> Self {
        SockAddrStore { inner: value.into() }
    }
}

impl From<SockAddrStore> for SocketAddr {
    fn from(value: SockAddrStore) -> Self {
        value.inner.into()
    }
}

/// One received datagram.
///
/// Nearly all datagrams fit `buf`. What exceeds it is received into a buffer shared
/// by the thread and copied to `tail`, which lives until the next batch is read. So
/// a slot costs `PACKET_SIZE` bytes however large datagrams may get, and memory for
/// large ones is only held while they are in flight.
#[derive(Debug)]
pub struct Packet {
    pub(super) buf: [u8; PACKET_SIZE],
    /// The part of the datagram beyond `buf`, empty for nearly all of them.
    tail: Vec<u8>,
    pub(super) addr: SockAddrStore,
    pub(super) cursor: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct PacketRef<'buf, 'addr> {
    buf: &'buf [u8],
    tail: &'buf [u8],
    addr: &'addr SockAddrStore,
}

impl Packet {
    pub fn new() -> Self {
        Self {
            buf: [0u8; PACKET_SIZE],
            tail: Vec::new(),
            addr: SockAddrStore::new(),
            cursor: 0u16,
        }
    }

    pub fn ref_with_addr<'a>(&self, addr: &'a SockAddrStore) -> PacketRef<'_, 'a> {
        PacketRef {
            buf: &self.buf[..self.len().min(PACKET_SIZE)],
            tail: &self.tail,
            addr,
        }
    }

    /// Length of the datagram.
    #[inline]
    pub fn len(&self) -> usize {
        self.cursor as usize
    }

    /// Longer than `buf`: part of it is in `tail`.
    #[inline]
    pub fn is_large(&self) -> bool {
        !self.tail.is_empty()
    }

    /// Give back the memory of a large datagram.
    #[inline]
    pub fn release(&mut self) {
        self.tail = Vec::new();
    }

    /// Record a datagram of `len` bytes: `buf` is already filled, `rest` is what
    /// did not fit into it.
    fn received(&mut self, len: usize, rest: &[u8]) {
        debug_assert_eq!(len.saturating_sub(PACKET_SIZE), rest.len());
        self.cursor = len.min(u16::MAX as usize) as u16;
        self.tail = if rest.is_empty() { Vec::new() } else { rest.to_vec() };
    }
}

#[cfg(not(all(target_os = "linux", feature = "batched-udp")))]
pub use common::{recv_some, send_all};
#[cfg(not(all(target_os = "linux", feature = "batched-udp")))]
mod common {
    use super::*;
    use std::cell::RefCell;
    use std::io::ErrorKind;

    thread_local! {
        /// Receive buffer for one datagram of any size. Only used between a socket
        /// turning readable and the datagram being copied out, never across an await.
        static SCRATCH: RefCell<Vec<u8>> = RefCell::new(vec![0u8; PACKET_SIZE + SPILL_SIZE]);
    }

    pub async fn recv_some(sock: &UdpSocket, pkts: &mut [Packet]) -> Result<usize> {
        debug_assert!(!pkts.is_empty());
        let pkt = &mut pkts[0];
        loop {
            sock.readable().await?;
            let res: Result<SocketAddr> = SCRATCH.with(|scratch| {
                let mut scratch = scratch.borrow_mut();
                let (n, addr) = sock.try_recv_from(&mut scratch)?;
                let inline = n.min(PACKET_SIZE);
                pkt.buf[..inline].copy_from_slice(&scratch[..inline]);
                pkt.received(n, &scratch[inline..n]);
                Ok(addr)
            });
            match res {
                Ok(addr) => {
                    pkt.addr.inner = addr;
                    return Ok(1);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => continue,
                Err(e) => return Err(e),
            }
        }
    }

    pub async fn send_all<'a, 'b, I>(sock: &UdpSocket, pkts: I, _no_fragment: bool) -> Result<()>
    where
        I: ExactSizeIterator<Item = PacketRef<'a, 'b>>,
    {
        for pkt in pkts {
            if pkt.tail.is_empty() {
                let _ = sock.send_to(pkt.buf, &pkt.addr.inner).await?;
            } else {
                let whole = [pkt.buf, pkt.tail].concat();
                let _ = sock.send_to(&whole, &pkt.addr.inner).await?;
            }
        }
        Ok(())
    }
}

#[cfg(all(target_os = "linux", feature = "batched-udp"))]
pub use linux::{recv_some, send_all};
#[cfg(all(target_os = "linux", feature = "batched-udp"))]
mod linux {
    use super::*;
    use std::cell::RefCell;
    use std::io::{Error, ErrorKind, IoSlice, IoSliceMut};
    use std::mem::MaybeUninit;
    use std::os::unix::io::{AsRawFd, RawFd};
    use tokio::io::Interest;
    use realm_io::AsyncRawIO;
    use realm_io::mmsg::{MmsgHdr, MmsgHdrMut};
    use realm_io::mmsg::send_mul_pkts;
    use crate::udp::socket::no_fragment;

    thread_local! {
        /// Spill area of a batch: `SPILL_SIZE` bytes behind each of its slots, for the
        /// part of a datagram beyond `PACKET_SIZE`. Shared by every socket the thread
        /// serves, which works because it is only used inside `recv_batch`: filled by
        /// the kernel and copied out before that returns.
        ///
        /// Never zeroed and only written for large datagrams, so it takes address
        /// space (8 MiB) but no memory until such datagrams arrive.
        static SPILL: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }

    pub async fn recv_some(sock: &UdpSocket, pkts: &mut [Packet]) -> Result<usize> {
        debug_assert!(pkts.len() <= MAX_PACKETS);
        let fd = sock.as_raw_fd();
        std::future::poll_fn(|cx| sock.poll_read_raw(cx, || recv_batch(fd, pkts))).await
    }

    /// One `recvmmsg`, returning its result as is. Every message gets `buf` of its slot
    /// and that slot's share of the spill area, so no datagram is ever cut short.
    fn recv_batch(fd: RawFd, pkts: &mut [Packet]) -> isize {
        const MAX_PKTS: usize = MAX_PACKETS;
        SPILL.with(|spill| {
            let mut spill = spill.borrow_mut();
            if spill.capacity() == 0 {
                spill.reserve_exact(MAX_PKTS * SPILL_SIZE);
            }
            let spill = spill.as_mut_ptr();
            let pkt_amt = pkts.len();

            let mut iovs: MaybeUninit<[[IoSliceMut; 2]; MAX_PKTS]> = MaybeUninit::uninit();
            let mut msgs: MaybeUninit<[MmsgHdrMut; MAX_PKTS]> = MaybeUninit::uninit();
            let iovs = unsafe { iovs.assume_init_mut() };
            let msgs = unsafe { msgs.assume_init_mut() };

            for (i, ((pkt, iov), msg)) in pkts.iter_mut().zip(iovs.iter_mut()).zip(msgs.iter_mut()).enumerate() {
                // SAFETY: slot `i` of the spill area, inside its allocation. The kernel
                // writes it, and only what it wrote is read back below.
                let rest = unsafe { std::slice::from_raw_parts_mut(spill.add(i * SPILL_SIZE), SPILL_SIZE) };
                iov[0] = IoSliceMut::new(&mut pkt.buf);
                iov[1] = IoSliceMut::new(rest);
                *msg = MmsgHdrMut::new().with_addr(&mut pkt.addr.inner).with_iovec(iov)
            }

            // SAFETY: `MmsgHdrMut` is a transparent `mmsghdr`, all of its pointers were
            // set above and stay valid for the call.
            let n = unsafe {
                libc::recvmmsg(
                    fd,
                    msgs.as_mut_ptr() as *mut libc::mmsghdr,
                    pkt_amt as u32,
                    0,
                    std::ptr::null_mut(),
                )
            };
            if n <= 0 {
                return n as isize;
            }

            let mut bytes: [u32; MAX_PKTS] = unsafe { std::mem::zeroed() };
            for (msg, byte) in msgs.iter().zip(bytes.iter_mut()).take(n as usize) {
                *byte = msg.get_ref().nbytes()
            }
            for (i, (pkt, byte)) in pkts.iter_mut().zip(bytes).take(n as usize).enumerate() {
                let len = byte as usize;
                // SAFETY: the kernel has just written these bytes of slot `i`
                let rest =
                    unsafe { std::slice::from_raw_parts(spill.add(i * SPILL_SIZE), len.saturating_sub(PACKET_SIZE)) };
                pkt.received(len, rest);
            }
            n as isize
        })
    }

    /// Send every datagram. `no_fragment`: the socket has `udp_no_fragment` on.
    pub async fn send_all<'a, 'b, I>(sock: &UdpSocket, pkts: I, no_fragment: bool) -> Result<()>
    where
        I: ExactSizeIterator<Item = PacketRef<'a, 'b>>,
    {
        const MAX_PKTS: usize = MAX_PACKETS;
        debug_assert!(pkts.len() <= MAX_PKTS);

        let pkt_amt = pkts.len();
        let mut iovs: MaybeUninit<[[IoSlice; 2]; MAX_PKTS]> = MaybeUninit::uninit();
        let mut msgs: MaybeUninit<[MmsgHdr; MAX_PKTS]> = MaybeUninit::uninit();
        let iovs = unsafe { iovs.assume_init_mut() };
        let msgs = unsafe { msgs.assume_init_mut() };

        for ((pkt, iov), msg) in pkts.zip(iovs.iter_mut()).zip(msgs.iter_mut()) {
            let iovcnt = if pkt.tail.is_empty() { 1 } else { 2 };
            iov[0] = IoSlice::new(pkt.buf);
            iov[1] = IoSlice::new(pkt.tail);
            *msg = MmsgHdr::new().with_addr(&pkt.addr.inner).with_iovec(&iov[..iovcnt])
        }

        let mut cursor = 0;
        while cursor < pkt_amt {
            let n = match send_mul_pkts(sock, &mut msgs[cursor..pkt_amt]).await {
                Ok(n) => n,
                // `udp_no_fragment` refuses a datagram that needs more than one frame.
                // It came to us in fragments, so send this one the same way.
                Err(e) if no_fragment && e.raw_os_error() == Some(libc::EMSGSIZE) => {
                    send_fragmented(sock, &mut msgs[cursor]).await?
                }
                Err(e) => return Err(e),
            };
            cursor += n;
        }
        Ok(())
    }

    /// Send one datagram of a `udp_no_fragment` socket in fragments, see
    /// [`no_fragment::lifted`]. Returns 1, the number of datagrams sent.
    async fn send_fragmented(sock: &UdpSocket, msg: &mut MmsgHdr<'_, '_, '_, '_>) -> Result<usize> {
        let fd = sock.as_raw_fd();
        loop {
            sock.writable().await?;
            let sent = no_fragment::lifted(sock, || {
                sock.try_io(Interest::WRITABLE, || {
                    let msg = &mut *msg as *mut MmsgHdr as *mut libc::mmsghdr;
                    // SAFETY: `MmsgHdr` is a transparent `mmsghdr` that `send_all` filled
                    // in, and what it points to outlives this call.
                    match unsafe { libc::sendmmsg(fd, msg, 1, 0) } {
                        n if n < 0 => Err(Error::last_os_error()),
                        n => Ok(n as usize),
                    }
                })
            });
            match sent {
                Err(e) if e.kind() == ErrorKind::WouldBlock => continue,
                sent => return sent,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::sync::Arc;

        /// Many tasks at once send datagrams that need fragments through one
        /// `udp_no_fragment` socket, the way associations share the listening socket.
        /// Every one of them must go out, and the option must be on in the end.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn no_fragment_large_datagrams_from_many_tasks() {
            const TASKS: usize = 8;
            const ROUNDS: usize = 300;
            const LEN: usize = 3000;
            const SMALL: usize = 100;

            // Loopback carries any datagram in one piece. An ipv6 socket can be given
            // a smaller mtu of its own, which makes large datagrams need fragments.
            let Ok(rx) = UdpSocket::bind("[::1]:0").await else {
                eprintln!("no ipv6 loopback, skipped");
                return;
            };
            let to: SockAddrStore = rx.local_addr().unwrap().into();
            let tx = Arc::new(UdpSocket::bind("[::1]:0").await.unwrap());
            let mtu: libc::c_int = 1280;
            // SAFETY: plain integer option
            let ok = unsafe {
                libc::setsockopt(
                    tx.as_raw_fd(),
                    libc::IPPROTO_IPV6,
                    libc::IPV6_MTU,
                    &mtu as *const libc::c_int as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                )
            };
            assert_eq!(ok, 0);
            no_fragment::set(&*tx, true);

            // as it is, such a datagram is refused
            let refused = tx.send_to(&[0u8; LEN], rx.local_addr().unwrap()).await;
            assert_eq!(refused.unwrap_err().raw_os_error(), Some(libc::EMSGSIZE));

            let received = tokio::spawn(async move {
                let mut buf = vec![0u8; LEN + 1];
                let mut large = 0;
                while let Ok(Ok(n)) = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv(&mut buf)).await {
                    assert!(n == LEN || n == SMALL, "a datagram of {} bytes", n);
                    large += (n == LEN) as usize;
                }
                large
            });

            let senders: Vec<_> = (0..TASKS)
                .map(|_| {
                    let (tx, to) = (tx.clone(), to.clone());
                    tokio::spawn(async move {
                        // one that fits a frame between two that do not
                        let mut pkts = [Packet::new(), Packet::new(), Packet::new()];
                        pkts[0].received(LEN, &[1u8; LEN - PACKET_SIZE]);
                        pkts[1].received(SMALL, &[]);
                        pkts[2].received(LEN, &[1u8; LEN - PACKET_SIZE]);
                        for _ in 0..ROUNDS {
                            send_all(&tx, pkts.iter().map(|p| p.ref_with_addr(&to)), true).await?;
                            tokio::task::yield_now().await;
                        }
                        Ok::<_, Error>(())
                    })
                })
                .collect();
            for sender in senders {
                sender.await.unwrap().expect("a large datagram was refused");
            }

            assert!(no_fragment::is_set(&*tx));
            // loopback may drop some under this load, but what arrives is whole
            let large = received.await.unwrap();
            println!("{} of {} large datagrams arrived", large, TASKS * ROUNDS * 2);
            assert!(large > 0);
        }
    }
}
