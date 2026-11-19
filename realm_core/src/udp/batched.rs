use std::io::Result;
use std::net::SocketAddr;
use tokio::net::UdpSocket;

/// Bytes of a datagram kept inline in every slot: whatever fits one ethernet frame.
pub const PACKET_SIZE: usize = 1500;
/// Room for the rest of a larger datagram. With it a slot holds more than any udp
/// payload (65527 bytes at most), so such a slot is never filled up.
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
/// Most datagrams fit `buf`. A slot can also get a spill, which takes what does not
/// fit: without one, a datagram longer than `buf` is cut short by the kernel.
#[derive(Debug)]
pub struct Packet {
    pub(super) buf: [u8; PACKET_SIZE],
    /// Empty and without capacity (no spill), or `SPILL_SIZE` of capacity holding the
    /// tail of the datagram. Never zeroed: an unused spill costs address space only.
    spill: Vec<u8>,
    /// The whole datagram sits in `spill` (set where one datagram is read into one buffer).
    whole: bool,
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
            spill: Vec::new(),
            whole: false,
            addr: SockAddrStore::new(),
            cursor: 0u16,
        }
    }

    pub fn ref_with_addr<'a>(&self, addr: &'a SockAddrStore) -> PacketRef<'_, 'a> {
        let (buf, tail): (&[u8], &[u8]) = if self.whole {
            (&self.spill, &[])
        } else {
            (&self.buf[..self.len().min(PACKET_SIZE)], &self.spill)
        };
        PacketRef { buf, tail, addr }
    }

    /// Length of the datagram.
    #[inline]
    pub fn len(&self) -> usize {
        self.cursor as usize
    }

    #[inline]
    pub fn has_spill(&self) -> bool {
        self.spill.capacity() != 0
    }

    /// Let this slot take datagrams longer than `buf`.
    pub fn add_spill(&mut self) {
        if !self.has_spill() {
            self.spill.reserve_exact(SPILL_SIZE);
        }
    }

    /// Longer than `buf`, and complete.
    #[inline]
    pub fn is_large(&self) -> bool {
        self.len() > PACKET_SIZE
    }

    /// The datagram filled a slot that has no spill, so its end may be missing.
    #[inline]
    pub fn maybe_cut(&self) -> bool {
        !self.has_spill() && self.len() >= PACKET_SIZE
    }

    /// The spill as a receive buffer.
    #[cfg(not(all(target_os = "linux", feature = "batched-udp")))]
    #[inline]
    fn spill_mut(&mut self) -> &mut [u8] {
        // SAFETY: within the allocation; only written by the kernel before `received`
        // exposes the written part.
        unsafe { std::slice::from_raw_parts_mut(self.spill.as_mut_ptr(), self.spill.capacity()) }
    }

    /// Record a datagram of `n` bytes that was received into `buf` and then the spill,
    /// or (`whole`) into the spill alone.
    fn received(&mut self, n: usize, whole: bool) {
        let n = n.min(u16::MAX as usize);
        let in_spill = if whole { n } else { n.saturating_sub(PACKET_SIZE) };
        debug_assert!(in_spill <= self.spill.capacity());
        self.cursor = n as u16;
        self.whole = whole;
        // SAFETY: the kernel has just written these bytes
        unsafe { self.spill.set_len(in_spill.min(self.spill.capacity())) }
    }
}

#[cfg(not(all(target_os = "linux", feature = "batched-udp")))]
pub use common::{recv_some, send_all};
#[cfg(not(all(target_os = "linux", feature = "batched-udp")))]
mod common {
    use super::*;
    pub async fn recv_some(sock: &UdpSocket, pkts: &mut [Packet]) -> Result<usize> {
        debug_assert!(!pkts.is_empty());
        let pkt = &mut pkts[0];
        // one buffer per datagram here: with a spill, all of it goes there
        let whole = pkt.has_spill();
        let res = if whole {
            sock.recv_from(pkt.spill_mut()).await
        } else {
            sock.recv_from(&mut pkt.buf).await
        };
        let (bytes, addr) = match res {
            Ok(x) => x,
            // windows reports a datagram that does not fit instead of cutting it
            #[cfg(windows)]
            Err(e) if !whole && e.raw_os_error() == Some(10040) => {
                pkt.received(PACKET_SIZE, false);
                return Ok(1);
            }
            Err(e) => return Err(e),
        };
        pkt.addr.inner = addr;
        pkt.received(bytes, whole);
        Ok(1)
    }

    pub async fn send_all<'a, 'b, I>(sock: &UdpSocket, pkts: I) -> Result<()>
    where
        I: ExactSizeIterator<Item = PacketRef<'a, 'b>>,
    {
        for pkt in pkts {
            debug_assert!(pkt.tail.is_empty());
            let _ = sock.send_to(pkt.buf, &pkt.addr.inner).await?;
        }
        Ok(())
    }
}

#[cfg(all(target_os = "linux", feature = "batched-udp"))]
pub use linux::{recv_some, send_all};
#[cfg(all(target_os = "linux", feature = "batched-udp"))]
mod linux {
    use super::*;
    use std::io::{IoSlice, IoSliceMut};
    use std::mem::MaybeUninit;
    use realm_io::mmsg::{MmsgHdr, MmsgHdrMut};
    use realm_io::mmsg::{send_mul_pkts, recv_mul_pkts};

    pub async fn recv_some(sock: &UdpSocket, pkts: &mut [Packet]) -> Result<usize> {
        const MAX_PKTS: usize = MAX_PACKETS;
        debug_assert!(pkts.len() <= MAX_PKTS);

        let pkt_amt = pkts.len();
        let mut iovs: MaybeUninit<[[IoSliceMut; 2]; MAX_PKTS]> = MaybeUninit::uninit();
        let mut msgs: MaybeUninit<[MmsgHdrMut; MAX_PKTS]> = MaybeUninit::uninit();
        let iovs = unsafe { iovs.assume_init_mut() };
        let msgs = unsafe { msgs.assume_init_mut() };

        for ((pkt, iov), msg) in pkts.iter_mut().zip(iovs.iter_mut()).zip(msgs.iter_mut()) {
            // the kernel fills `buf`, then the spill if there is one
            let spill = unsafe { std::slice::from_raw_parts_mut(pkt.spill.as_mut_ptr(), pkt.spill.capacity()) };
            let iovcnt = if spill.is_empty() { 1 } else { 2 };
            iov[0] = IoSliceMut::new(&mut pkt.buf);
            iov[1] = IoSliceMut::new(spill);
            *msg = MmsgHdrMut::new()
                .with_addr(&mut pkt.addr.inner)
                .with_iovec(&mut iov[..iovcnt])
        }

        let pkt_amt = recv_mul_pkts(sock, &mut msgs[..pkt_amt]).await?;
        {
            let mut bytes: [u32; MAX_PKTS] = unsafe { std::mem::zeroed() };
            for (msg, byte) in msgs.iter().zip(bytes.iter_mut()).take(pkt_amt) {
                *byte = msg.get_ref().nbytes()
            }

            for (pkt, byte) in pkts.iter_mut().zip(bytes).take(pkt_amt) {
                pkt.received(byte as usize, false)
            }
        }
        Ok(pkt_amt)
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
