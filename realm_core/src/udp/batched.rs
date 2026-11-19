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

    pub async fn send_all<'a, 'b, I>(sock: &UdpSocket, pkts: I) -> Result<()>
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
    use std::io::{IoSlice, IoSliceMut};
    use std::mem::MaybeUninit;
    use std::os::unix::io::{AsRawFd, RawFd};
    use realm_io::AsyncRawIO;
    use realm_io::mmsg::{MmsgHdr, MmsgHdrMut};
    use realm_io::mmsg::send_mul_pkts;

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

    pub async fn send_all<'a, 'b, I>(sock: &UdpSocket, pkts: I) -> Result<()>
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
            let n = send_mul_pkts(sock, &mut msgs[cursor..pkt_amt]).await?;
            cursor += n;
        }
        Ok(())
    }
}
