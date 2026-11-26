use std::io::{Error, ErrorKind, Result};
use std::time::Duration;
use tokio::time::{Instant, timeout_at};
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

/// A deadline shared by the groups of one receive batch. Reading the clock
/// and arming a timer are only needed once a send encounters backpressure.
pub struct SendBudget {
    duration: Duration,
    deadline: Option<Instant>,
}

impl SendBudget {
    pub const fn new(duration: Duration) -> Self {
        Self {
            duration,
            deadline: None,
        }
    }

    fn expired(&self) -> bool {
        self.deadline.is_some_and(|deadline| Instant::now() >= deadline)
    }

    fn deadline(&mut self) -> Instant {
        *self.deadline.get_or_insert_with(|| Instant::now() + self.duration)
    }
}

#[derive(Debug, Default)]
pub struct SendStats {
    pub sent: usize,
    pub dropped: usize,
    pub error: Option<Error>,
}

pub struct SendBatch {
    stats: SendStats,
    total: usize,
    fragmented: bool,
}

impl SendBatch {
    fn new(total: usize) -> Self {
        Self {
            stats: SendStats::default(),
            total,
            fragmented: false,
        }
    }

    fn processed(&self) -> usize {
        self.stats.sent + self.stats.dropped
    }

    fn is_done(&self) -> bool {
        self.processed() == self.total
    }

    fn discard_rest(&mut self, error: Error) {
        self.stats.dropped += self.total - self.processed();
        self.stats.error = Some(error);
    }

    pub fn sent(&self) -> usize {
        self.stats.sent
    }

    /// Process the packets covered by the prepared send buffers. A short
    /// `sendmmsg` only reports the sent prefix; the next call tries the rest.
    #[inline]
    fn drive(
        &mut self,
        end: usize,
        no_fragment: bool,
        one_attempt: bool,
        mut send: impl FnMut(usize, bool) -> Result<usize>,
    ) {
        let start = self.processed();
        loop {
            match send(self.processed() - start, self.fragmented) {
                Ok(n) => {
                    self.stats.sent += n;
                    self.fragmented = false;
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == ErrorKind::WouldBlock => return,
                Err(e) if no_fragment && !self.fragmented && needs_fragments(&e) => {
                    // A datagram larger than the outgoing MTU may have reached
                    // us in fragments. Try to send this one the same way.
                    self.fragmented = true;
                }
                Err(e) => {
                    self.stats.dropped += 1;
                    self.stats.error = Some(e);
                    self.fragmented = false;
                }
            }
            if one_attempt || self.processed() == end {
                return;
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn needs_fragments(error: &Error) -> bool {
    error.raw_os_error() == Some(libc::EMSGSIZE)
}

#[cfg(not(target_os = "linux"))]
fn needs_fragments(_error: &Error) -> bool {
    false
}

pub fn send_now<'a, 'b>(
    sock: &UdpSocket,
    pkts: impl ExactSizeIterator<Item = PacketRef<'a, 'b>>,
    no_fragment: bool,
    budget: &mut SendBudget,
) -> SendBatch {
    start_send(pkts.len(), budget, |one_attempt, batch| {
        send_available(sock, pkts, no_fragment, false, one_attempt, batch);
    })
}

#[inline]
fn start_send(total: usize, budget: &mut SendBudget, send: impl FnOnce(bool, &mut SendBatch)) -> SendBatch {
    let mut batch = SendBatch::new(total);
    let one_attempt = budget.expired();
    send(one_attempt, &mut batch);
    if !batch.is_done() {
        if one_attempt {
            batch.discard_rest(ErrorKind::TimedOut.into());
        } else {
            budget.deadline();
        }
    }
    batch
}

pub fn retry_send<'a, 'b>(
    sock: &UdpSocket,
    pkts: impl ExactSizeIterator<Item = PacketRef<'a, 'b>>,
    no_fragment: bool,
    batch: &mut SendBatch,
) {
    send_available(sock, pkts, no_fragment, true, false, batch);
}

/// The first retry consumes stale write readiness on WouldBlock before waiting
/// for another event. `writable` registers all waiters on a shared socket.
pub async fn finish_send(
    sock: &UdpSocket,
    budget: &mut SendBudget,
    mut batch: SendBatch,
    mut retry: impl FnMut(&mut SendBatch) -> bool,
) -> SendStats {
    while !batch.is_done() {
        let deadline = budget.deadline();
        let error = if Instant::now() >= deadline {
            ErrorKind::TimedOut.into()
        } else if !retry(&mut batch) {
            ErrorKind::NotConnected.into()
        } else if batch.is_done() {
            break;
        } else {
            match timeout_at(deadline, sock.writable()).await {
                Ok(Ok(())) => continue,
                Ok(Err(e)) => e,
                Err(_) => ErrorKind::TimedOut.into(),
            }
        };
        batch.discard_rest(error);
    }
    batch.stats
}

pub async fn send_all<'a, 'b, I>(sock: &UdpSocket, pkts: I, no_fragment: bool, budget: &mut SendBudget) -> SendStats
where
    I: ExactSizeIterator<Item = PacketRef<'a, 'b>> + Clone,
{
    let batch = send_now(sock, pkts.clone(), no_fragment, budget);
    finish_send(sock, budget, batch, |batch| {
        retry_send(sock, pkts.clone(), no_fragment, batch);
        true
    })
    .await
}

#[cfg(not(all(target_os = "linux", feature = "batched-udp")))]
pub use common::recv_some;
#[cfg(not(all(target_os = "linux", feature = "batched-udp")))]
use common::send_available;
#[cfg(not(all(target_os = "linux", feature = "batched-udp")))]
mod common {
    use super::*;
    use std::cell::RefCell;

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

    pub(super) fn send_available<'a, 'b>(
        sock: &UdpSocket,
        pkts: impl ExactSizeIterator<Item = PacketRef<'a, 'b>>,
        no_fragment: bool,
        _check_ready: bool,
        one_attempt: bool,
        batch: &mut SendBatch,
    ) {
        for pkt in pkts.skip(batch.processed()) {
            let whole;
            let bytes = if pkt.tail.is_empty() {
                pkt.buf
            } else {
                whole = [pkt.buf, pkt.tail].concat();
                &whole
            };
            let end = batch.processed() + 1;
            batch.drive(end, no_fragment, one_attempt, |_, fragmented| {
                let send = || sock.try_send_to(bytes, pkt.addr.inner).map(|_| 1);
                #[cfg(target_os = "linux")]
                if fragmented {
                    return crate::udp::socket::no_fragment::lifted(sock, send);
                }
                let _ = fragmented;
                send()
            });
            if one_attempt || batch.processed() < end {
                return;
            }
        }
    }
}

#[cfg(all(target_os = "linux", feature = "batched-udp"))]
pub use linux::recv_some;
#[cfg(all(target_os = "linux", feature = "batched-udp"))]
use linux::send_available;
#[cfg(all(target_os = "linux", feature = "batched-udp"))]
mod linux {
    use super::*;
    use std::cell::RefCell;
    use std::io::IoSlice;
    use std::mem::MaybeUninit;
    use std::os::unix::io::{AsRawFd, RawFd};
    use tokio::io::Interest;
    use realm_io::AsyncRawIO;
    use realm_io::mmsg::{MmsgHdr, MmsgHdrMut};
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

            let mut iovs = [const { MaybeUninit::uninit() }; MAX_PKTS];
            let mut msgs = [const { MaybeUninit::uninit() }; MAX_PKTS];

            for (i, ((pkt, iov), msg)) in pkts.iter_mut().zip(&mut iovs).zip(&mut msgs).enumerate() {
                // SAFETY: Each iovec points at a different writable buffer, and
                // the spill pointer lies within the allocation reserved above.
                unsafe {
                    let iov = iov.write([
                        libc::iovec {
                            iov_base: pkt.buf.as_mut_ptr().cast(),
                            iov_len: PACKET_SIZE,
                        },
                        libc::iovec {
                            iov_base: spill.add(i * SPILL_SIZE).cast(),
                            iov_len: SPILL_SIZE,
                        },
                    ]);
                    msg.write(MmsgHdrMut::new().with_addr(&mut pkt.addr.inner).with_raw_iovec(iov));
                }
            }

            // SAFETY: The initialized prefix contains one `mmsghdr` per packet.
            // Its pointers remain valid for the call.
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

            let n = n as usize;
            let mut bytes = [MaybeUninit::uninit(); MAX_PKTS];
            for (msg, byte) in msgs[..n].iter().zip(&mut bytes) {
                // SAFETY: The call returned a message from the initialized prefix.
                byte.write(unsafe { msg.assume_init_ref() }.nbytes() as usize);
            }
            for (i, (pkt, byte)) in pkts.iter_mut().zip(&bytes[..n]).enumerate() {
                // SAFETY: The byte count of each received message was copied above.
                let len = unsafe { byte.assume_init() };
                // SAFETY: the kernel has just written these bytes of slot `i`
                let rest =
                    unsafe { std::slice::from_raw_parts(spill.add(i * SPILL_SIZE), len.saturating_sub(PACKET_SIZE)) };
                pkt.received(len, rest);
            }
            n as isize
        })
    }

    pub(super) fn send_available<'a, 'b>(
        sock: &UdpSocket,
        pkts: impl ExactSizeIterator<Item = PacketRef<'a, 'b>>,
        no_fragment: bool,
        check_ready: bool,
        one_attempt: bool,
        batch: &mut SendBatch,
    ) {
        let pkts = pkts.skip(batch.processed());
        let pkt_amt = pkts.len();
        debug_assert!(pkt_amt <= MAX_PACKETS);
        if pkt_amt == 0 {
            return;
        }

        let mut iovs = [const { MaybeUninit::uninit() }; MAX_PACKETS];
        let mut msgs = [const { MaybeUninit::uninit() }; MAX_PACKETS];
        for ((pkt, iov), msg) in pkts.zip(&mut iovs).zip(&mut msgs) {
            let iovcnt = if pkt.tail.is_empty() { 1 } else { 2 };
            let iov = iov.write([IoSlice::new(pkt.buf), IoSlice::new(pkt.tail)]);
            msg.write(MmsgHdr::new().with_addr(&pkt.addr.inner).with_iovec(&iov[..iovcnt]));
        }

        // SAFETY: Every header in this prefix has been written, and the
        // addresses, iovecs, and packet data they point to remain live below.
        let msgs = unsafe { std::slice::from_raw_parts_mut(msgs.as_mut_ptr().cast::<MmsgHdr>(), pkt_amt) };
        let fd = sock.as_raw_fd();
        batch.drive(batch.total, no_fragment, one_attempt, |cursor, fragmented| {
            let end = if fragmented { cursor + 1 } else { pkt_amt };
            let mut send = || send_batch(fd, &mut msgs[cursor..end]);
            let send = || {
                if fragmented {
                    no_fragment::lifted(sock, send)
                } else {
                    send()
                }
            };
            if check_ready {
                sock.try_io(Interest::WRITABLE, send)
            } else {
                send()
            }
        });
    }

    #[inline]
    fn send_batch(fd: RawFd, msgs: &mut [MmsgHdr<'_, '_, '_, '_>]) -> Result<usize> {
        // SAFETY: These headers and their backing buffers are valid until the
        // call returns, and the slice contains at least one header.
        match unsafe { libc::sendmmsg(fd, msgs.as_mut_ptr().cast(), msgs.len() as u32, 0) } {
            n if n < 0 => Err(Error::last_os_error()),
            n => Ok(n as usize),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::sync::Arc;

        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_sends_with_a_small_socket_buffer_return() {
            const TASKS: usize = 12;
            let rx = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let to: SockAddrStore = rx.local_addr().unwrap().into();
            let tx = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
            let size: libc::c_int = 1;
            // SAFETY: SO_SNDBUF takes an integer and the socket stays alive.
            let result = unsafe {
                libc::setsockopt(
                    tx.as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_SNDBUF,
                    (&size as *const libc::c_int).cast(),
                    std::mem::size_of_val(&size) as _,
                )
            };
            assert_eq!(result, 0);

            use futures::FutureExt;
            let (start, ready) = futures::channel::oneshot::channel();
            let ready = ready.shared();
            let senders = (0..TASKS)
                .map(|_| {
                    let (tx, to, ready) = (tx.clone(), to.clone(), ready.clone());
                    tokio::spawn(async move {
                        let mut batch: Vec<_> = (0..MAX_PACKETS).map(|_| Packet::new()).collect();
                        for pkt in &mut batch {
                            pkt.received(1200, &[]);
                        }
                        ready.await.unwrap();
                        let mut sent = 0;
                        for _ in 0..32 {
                            let mut budget = SendBudget::new(Duration::from_millis(200));
                            let stats =
                                send_all(&tx, batch.iter().map(|pkt| pkt.ref_with_addr(&to)), false, &mut budget).await;
                            assert_eq!(stats.sent + stats.dropped, batch.len());
                            sent += stats.sent;
                        }
                        sent
                    })
                })
                .collect::<Vec<_>>();
            start.send(()).unwrap();
            let results = tokio::time::timeout(Duration::from_secs(3), futures::future::join_all(senders))
                .await
                .unwrap();
            assert!(results.into_iter().all(|result| result.unwrap() > 0));
        }

        #[tokio::test]
        async fn would_block_drops_at_the_deadline_and_next_groups_do_not_wait() {
            let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            sock.writable().await.unwrap();
            let started = Instant::now();
            let duration = Duration::from_millis(5);
            let mut budget = SendBudget::new(duration);
            let begin = |budget: &mut SendBudget, expired: bool| {
                start_send(3, budget, |one_attempt, batch| {
                    assert_eq!(one_attempt, expired);
                    batch.drive(3, false, one_attempt, |_, _| Err(ErrorKind::WouldBlock.into()));
                })
            };
            let batch = begin(&mut budget, false);
            let deadline = budget.deadline.unwrap();
            let mut attempts = 0;
            let stats = tokio::time::timeout(
                Duration::from_secs(1),
                finish_send(&sock, &mut budget, batch, |batch| {
                    // Loopback does not reliably fill the send queue. Simulate a
                    // send returning EAGAIN and exercise Tokio's real readiness.
                    attempts += 1;
                    batch.drive(3, false, false, |_, _| {
                        sock.try_io(Interest::WRITABLE, || Err(ErrorKind::WouldBlock.into()))
                    });
                    true
                }),
            )
            .await
            .unwrap();
            assert_eq!(attempts, 1);
            assert_eq!((stats.sent, stats.dropped), (0, 3));
            assert_eq!(stats.error.unwrap().kind(), ErrorKind::TimedOut);
            assert!(Instant::now() >= deadline);

            for _ in 0..MAX_PACKETS - 1 {
                let batch = begin(&mut budget, true);
                let stats = finish_send(&sock, &mut budget, batch, |_| panic!("another group waited")).await;
                assert_eq!((stats.sent, stats.dropped), (0, 3));
            }
            assert_eq!(budget.deadline, Some(deadline));
            assert!(started.elapsed() < Duration::from_millis(200));
        }

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
                            let mut budget = SendBudget::new(Duration::from_millis(200));
                            let stats =
                                send_all(&tx, pkts.iter().map(|p| p.ref_with_addr(&to)), true, &mut budget).await;
                            assert_eq!(stats.sent, pkts.len(), "{stats:?}");
                            tokio::task::yield_now().await;
                        }
                    })
                })
                .collect();
            for sender in senders {
                sender.await.unwrap();
            }

            assert!(no_fragment::is_set(&*tx));
            // loopback may drop some under this load, but what arrives is whole
            let large = received.await.unwrap();
            println!("{} of {} large datagrams arrived", large, TASKS * ROUNDS * 2);
            assert!(large > 0);
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_invalid_datagram_does_not_discard_the_following_one() {
        let rx = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let tx = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let to = rx.local_addr().unwrap().into();
        let invalid = SocketAddr::from(([127, 0, 0, 1], 0)).into();
        let mut pkts = [Packet::new(), Packet::new(), Packet::new()];
        for (i, pkt) in pkts.iter_mut().enumerate() {
            pkt.buf[0] = i as u8;
            pkt.received(1, &[]);
        }
        let group = pkts
            .iter()
            .enumerate()
            .map(|(i, pkt)| pkt.ref_with_addr(if i == 1 { &invalid } else { &to }));
        let mut budget = SendBudget::new(Duration::from_millis(200));
        let stats = send_all(&tx, group, false, &mut budget).await;
        assert_eq!((stats.sent, stats.dropped), (2, 1));
        assert_eq!(stats.error.unwrap().kind(), ErrorKind::InvalidInput);
        for expected in [0, 2] {
            let mut buf = [0; 1];
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(1), rx.recv(&mut buf))
                    .await
                    .unwrap()
                    .unwrap(),
                1
            );
            assert_eq!(buf, [expected]);
        }
    }
}
