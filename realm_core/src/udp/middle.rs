use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;

use futures::future::{Either, select};
use tokio::net::UdpSocket;
use tokio::time::{Instant, sleep};

use super::{RateLimit, SockMap, batched, socket};
use super::sockmap::{Assoc, activity_tick};

use crate::trick::Ref;
use crate::dns::resolve_addr;
use crate::endpoint::{RemoteAddr, ConnectOpts};
use crate::failover::Failover;

use batched::{Packet, SendBudget, SockAddrStore};
use registry::Registry;

const UPSTREAM_WAIT: Duration = Duration::from_millis(5);
const CLIENT_WAIT: Duration = Duration::from_millis(200);
const RECV_BACKOFF_MIN: Duration = Duration::from_millis(10);
const RECV_BACKOFF_MAX: Duration = Duration::from_secs(1);

async fn back_off_receive(delay: &mut Duration) {
    sleep(*delay).await;
    *delay = (*delay * 2).min(RECV_BACKOFF_MAX);
}

mod registry {
    use super::*;
    use std::ops::Range;

    const MIN_PACKETS: usize = 8;
    const SMALL_BATCHES: u8 = 32;
    const MAX_PACKETS: usize = batched::MAX_PACKETS;
    const IDENTITY: [u8; MAX_PACKETS] = {
        let mut order = [0; MAX_PACKETS];
        let mut i = 0;
        while i < MAX_PACKETS {
            order[i] = i as u8;
            i += 1;
        }
        order
    };

    struct Growth {
        max: usize,
        last_batch: Option<usize>,
        small_batches: u8,
    }

    impl Growth {
        fn resize(&mut self, pkts: &mut Vec<Packet>) {
            let Some(n) = self.last_batch.take() else { return };
            let capacity = pkts.len();
            if n == capacity && capacity < self.max {
                pkts.resize_with((capacity * 2).min(self.max), Packet::new);
            }
            if n * 4 < capacity && capacity > MIN_PACKETS {
                self.small_batches += 1;
                if self.small_batches == SMALL_BATCHES {
                    let target = (capacity / 2).max(MIN_PACKETS);
                    pkts.truncate(target);
                    pkts.shrink_to(target);
                    self.small_batches = 0;
                }
            } else {
                self.small_batches = 0;
            }
        }
    }

    pub struct Registry {
        pkts: Vec<Packet>,
        groups: Vec<Range<usize>>,
        order: [u8; MAX_PACKETS],
        mixed: bool,
        cursor: usize,
        growth: Option<Growth>,
        /// Some slot holds a large datagram, see [`Packet::release`].
        large: bool,
    }

    impl Registry {
        pub fn new(npkts: usize) -> Self {
            debug_assert!(npkts <= MAX_PACKETS);
            Self {
                pkts: (0..npkts).map(|_| Packet::new()).collect(),
                groups: Vec::with_capacity(npkts),
                order: [0; MAX_PACKETS],
                mixed: false,
                cursor: 0,
                growth: None,
                large: false,
            }
        }

        /// Start small because most associations never receive large batches.
        pub fn growing(max: usize) -> Self {
            debug_assert!(max <= MAX_PACKETS);
            Self {
                pkts: (0..MIN_PACKETS.min(max)).map(|_| Packet::new()).collect(),
                groups: Vec::new(),
                order: [0; MAX_PACKETS],
                mixed: false,
                cursor: 0,
                growth: Some(Growth {
                    max,
                    last_batch: None,
                    small_batches: 0,
                }),
                large: false,
            }
        }

        pub async fn batched_recv_on(&mut self, sock: &UdpSocket) -> std::io::Result<()> {
            // Give up the previous batch's large datagrams before waiting for more.
            if self.large {
                self.pkts.iter_mut().for_each(Packet::release);
                self.large = false;
            }
            if let Some(growth) = &mut self.growth {
                growth.resize(&mut self.pkts);
            }

            let n = batched::recv_some(sock, &mut self.pkts).await?;
            self.cursor = n;
            self.large = self.pkts[..n].iter().any(Packet::is_large);
            if let Some(growth) = &mut self.growth {
                growth.last_batch = Some(n);
            }
            Ok(())
        }

        pub fn group_by_addr(&mut self) {
            self.groups.clear();
            self.mixed = group_by_inner(&self.pkts[..self.cursor], &mut self.groups, &mut self.order, |a, b| {
                a.addr == b.addr
            });
        }

        pub fn group_iter(&self) -> impl Iterator<Item = Group<'_>> {
            let order = if self.mixed { &self.order } else { &IDENTITY };
            self.groups.iter().map(move |range| Group {
                pkts: &self.pkts,
                order: &order[range.clone()],
            })
        }

        pub fn iter(&self) -> std::slice::Iter<'_, Packet> {
            self.pkts[..self.cursor].iter()
        }

        /// Keep received packets matching `keep`, in order.
        pub fn retain(&mut self, keep: impl Fn(&Packet) -> bool) {
            let mut w = 0;
            for r in 0..self.cursor {
                if keep(&self.pkts[r]) {
                    if w != r {
                        self.pkts.swap(w, r);
                    }
                    w += 1;
                }
            }
            self.cursor = w;
        }

        pub const fn count(&self) -> usize {
            self.cursor
        }
    }

    pub struct Group<'a> {
        pkts: &'a [Packet],
        order: &'a [u8],
    }

    impl Group<'_> {
        pub fn first(&self) -> &Packet {
            &self.pkts[self.order[0] as usize]
        }

        pub fn iter(&self) -> impl ExactSizeIterator<Item = &Packet> + Clone {
            self.order.iter().map(|&index| &self.pkts[index as usize])
        }
    }

    /// Pack indices in source order, visiting the remaining packet slots in
    /// arrival order. The single-source batch uses the identity permutation.
    fn group_by_inner<T>(
        data: &[T],
        groups: &mut Vec<Range<usize>>,
        order: &mut [u8; MAX_PACKETS],
        eq: impl Fn(&T, &T) -> bool,
    ) -> bool {
        let Some((first, rest)) = data.split_first() else {
            return false;
        };
        if rest.iter().all(|item| eq(first, item)) {
            groups.push(0..data.len());
            return false;
        }

        let mut pending = u128::MAX >> (MAX_PACKETS - data.len());
        let mut end = 0;
        while pending != 0 {
            let start = end;
            let first = pending.trailing_zeros() as usize;
            let mut candidates = pending;
            while candidates != 0 {
                let i = candidates.trailing_zeros() as usize;
                candidates &= candidates - 1;
                if eq(&data[first], &data[i]) {
                    order[end] = i as u8;
                    end += 1;
                    pending ^= 1 << i;
                }
            }
            groups.push(start..end);
        }
        true
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn groups_are_stable_and_keep_first_source_order() {
            for (data, expected, ranges) in [
                (&b"ABBA"[..], &[0, 3, 1, 2][..], &[0..2, 2..4][..]),
                (&b"CBACBC"[..], &[0, 3, 5, 1, 4, 2][..], &[0..3, 3..5, 5..6][..]),
                (&b"AAA"[..], &[0, 1, 2][..], std::slice::from_ref(&(0..3))),
            ] {
                let mut groups = Vec::new();
                let mut order = [0; MAX_PACKETS];
                let mixed = group_by_inner(data, &mut groups, &mut order, |a, b| a == b);
                let order = if mixed { &order } else { &IDENTITY };
                assert_eq!(&order[..data.len()], expected);
                assert_eq!(&groups, ranges);
            }
        }

        #[test]
        fn growing_registry_gives_back_capacity_after_small_batches() {
            let mut registry = Registry::growing(MAX_PACKETS);
            let growth = registry.growth.as_mut().unwrap();
            for size in [8, 16, 32, 64] {
                growth.last_batch = Some(size);
                growth.resize(&mut registry.pkts);
                assert_eq!(registry.pkts.len(), size * 2);
            }
            for size in [64, 32, 16, 8] {
                for _ in 0..SMALL_BATCHES {
                    growth.last_batch = Some(1);
                    growth.resize(&mut registry.pkts);
                }
                assert_eq!(registry.pkts.len(), size);
                assert_eq!(registry.pkts.capacity(), size);
            }
        }
    }
}

fn select_upstream(mut addrs: impl Iterator<Item = SocketAddr>, through: Option<SocketAddr>) -> Option<SocketAddr> {
    match through {
        Some(through) => addrs.find(|addr| addr.is_ipv4() == through.is_ipv4()),
        None => addrs.next(),
    }
}

fn remember_resolution(
    rname: &RemoteAddr,
    last: &mut Option<SocketAddr>,
    result: std::io::Result<Option<SocketAddr>>,
) -> Option<SocketAddr> {
    static LOG: RateLimit = RateLimit::new();
    match result.and_then(|addr| addr.ok_or_else(|| Error::from(ErrorKind::NotFound))) {
        Ok(addr) => *last = Some(addr),
        Err(error) => {
            if let Some(suppressed) = LOG.allow() {
                log::warn!(
                    "[udp]failed to resolve {}: {} (last address: {:?}, {} suppressed)",
                    rname,
                    error,
                    last,
                    suppressed
                );
            }
        }
    }
    *last
}

pub async fn associate_and_relay(
    lis: Ref<UdpSocket>,
    rname: Ref<RemoteAddr>,
    conn_opts: Ref<ConnectOpts>,
    sockmap: Arc<SockMap>,
    failover: Ref<Option<Failover>>,
) -> ! {
    static RECV_LOG: RateLimit = RateLimit::new();
    static ASSOC_LOG: RateLimit = RateLimit::new();
    static SEND_LOG: RateLimit = RateLimit::new();
    let mut registry = Registry::new(batched::MAX_PACKETS);
    let mut last_resolved = None;
    let renew = conn_opts.associate_timeout != 0;
    let no_fragment = conn_opts.udp_no_fragment;

    loop {
        let mut backoff = RECV_BACKOFF_MIN;
        while let Err(e) = registry.batched_recv_on(&lis).await {
            if let Some(suppressed) = RECV_LOG.allow() {
                log::error!(
                    "[udp]entry recvfrom failed: {}, retry in {:?} ({} suppressed)",
                    e,
                    backoff,
                    suppressed
                );
            }
            back_off_receive(&mut backoff).await;
        }
        log::debug!("[udp]entry batched recvfrom[{}]", registry.count());

        let (route, fixed) = match failover.as_ref() {
            Some(fo) => (Some((fo, fo.snapshot())), None),
            None => {
                let raddr = match rname.as_ref() {
                    RemoteAddr::SocketAddr(addr) => *addr,
                    RemoteAddr::DomainName(..) => {
                        let result = resolve_addr(&rname)
                            .await
                            .map(|addrs| select_upstream(addrs.iter(), conn_opts.bind_address));
                        let Some(addr) = remember_resolution(&rname, &mut last_resolved, result) else {
                            continue;
                        };
                        addr
                    }
                };
                log::debug!("[udp]{} resolved as {}", *rname, raddr);
                (None, Some(raddr))
            }
        };

        registry.group_by_addr();
        let tick = renew.then(activity_tick);
        let mut budget = SendBudget::new(UPSTREAM_WAIT);
        for pkts in registry.group_iter() {
            let laddr = pkts.first().addr.to_socket_addr();

            // Keep the lookup and any successful send together so an idle
            // association cannot be removed before its activity is recorded.
            let mut start = |assoc: &Arc<Assoc>| {
                let raddr = match (route, fixed) {
                    (Some((fo, snap)), _) => {
                        let cur = assoc.peer();
                        let next = if snap.is_up(cur) {
                            cur
                        } else {
                            match fo.balanced_peer(&conn_opts, &laddr.ip(), snap) {
                                Some(next) => Failover::repick_balanced(snap, cur, next),
                                None => Failover::repick(snap, cur),
                            }
                        };
                        if next != cur {
                            assoc.move_to(next);
                            log::info!(
                                "[udp]{} moved from peer#{} to peer#{} {}",
                                laddr,
                                cur,
                                next,
                                fo.peer(next)
                            );
                        }
                        fo.peer(next)
                    }
                    (None, Some(raddr)) => raddr,
                    (None, None) => unreachable!(),
                };
                let to = raddr.into();
                let batch = batched::send_now(
                    &assoc.sock,
                    pkts.iter().map(|p| p.ref_with_addr(&to)),
                    no_fragment,
                    &mut budget,
                );
                if let Some(tick) = tick {
                    if batch.sent() > 0 {
                        assoc.touch(tick);
                    }
                }
                (Arc::clone(assoc), raddr, batch)
            };

            let (assoc, raddr, batch) = match sockmap.with_assoc(&laddr, &mut start) {
                Some(active) => active,
                None => {
                    let (peer, raddr) = match (route, fixed) {
                        (Some((fo, snap)), _) => {
                            let peer = fo
                                .balanced_peer(&conn_opts, &laddr.ip(), snap)
                                .unwrap_or_else(|| snap.active());
                            (peer, fo.peer(peer))
                        }
                        (None, Some(raddr)) => (0, raddr),
                        (None, None) => unreachable!(),
                    };
                    let sock = match socket::associate(&raddr, &conn_opts) {
                        Ok(sock) => sock,
                        Err(e) => {
                            if let Some(suppressed) = ASSOC_LOG.allow() {
                                log::error!(
                                    "[udp]failed to associate for {}: {} ({} suppressed)",
                                    laddr,
                                    e,
                                    suppressed
                                );
                            }
                            continue;
                        }
                    };
                    let assoc = Arc::new(Assoc::new(sock, peer, tick.unwrap_or_default()));
                    sockmap.insert(laddr, Arc::clone(&assoc));
                    let active = start(&assoc);
                    tokio::spawn(send_back(
                        lis,
                        laddr,
                        assoc,
                        renew,
                        conn_opts,
                        sockmap.clone(),
                        failover,
                    ));
                    match route {
                        Some(_) => log::info!("[udp]new association {} => peer#{} {}", laddr, peer, raddr),
                        None => log::info!("[udp]new association {} => {} as {}", laddr, *rname, raddr),
                    }
                    active
                }
            };

            let stats = batched::finish_send(&assoc.sock, &mut budget, batch, |batch| {
                sockmap
                    .with_assoc(&laddr, |current| {
                        if !Arc::ptr_eq(current, &assoc) {
                            return false;
                        }
                        let sent = batch.sent();
                        let to = raddr.into();
                        batched::retry_send(
                            &assoc.sock,
                            pkts.iter().map(|p| p.ref_with_addr(&to)),
                            no_fragment,
                            batch,
                        );
                        if renew && batch.sent() > sent {
                            assoc.touch(activity_tick());
                        }
                        true
                    })
                    .unwrap_or(false)
            })
            .await;
            if let Some(e) = stats.error {
                if let Some(suppressed) = SEND_LOG.allow() {
                    log::error!(
                        "[udp]failed to sendto {} for {}: {} ({} sent, {} dropped, {} suppressed)",
                        raddr,
                        laddr,
                        e,
                        stats.sent,
                        stats.dropped,
                        suppressed
                    );
                }
            }
        }
    }
}

struct RemoveOnDrop<'a> {
    // The listener can be dropped before Tokio drops this task.
    sockmap: Arc<SockMap>,
    addr: SocketAddr,
    assoc: &'a Arc<Assoc>,
    removed: bool,
}

impl Drop for RemoveOnDrop<'_> {
    fn drop(&mut self) {
        if !self.removed {
            self.sockmap.remove(&self.addr, self.assoc);
        }
    }
}

async fn send_back(
    lsock: Ref<UdpSocket>,
    laddr: SocketAddr,
    assoc: Arc<Assoc>,
    renew: bool,
    conn_opts: Ref<ConnectOpts>,
    sockmap: Arc<SockMap>,
    failover: Ref<Option<Failover>>,
) {
    static RECV_LOG: RateLimit = RateLimit::new();
    static SEND_LOG: RateLimit = RateLimit::new();
    let mut cleanup = RemoveOnDrop {
        sockmap,
        addr: laddr,
        assoc: &assoc,
        removed: false,
    };
    let mut registry = Registry::growing(batched::MAX_PACKETS);
    let timeout = Duration::from_secs(conn_opts.associate_timeout as u64);
    let laddr_s: SockAddrStore = laddr.into();
    let mut idle = pin!(renew.then(|| sleep(timeout)));
    let mut idle = idle.as_mut().as_pin_mut();

    loop {
        let receive = registry.batched_recv_on(&assoc.sock);
        let received = if let Some(timer) = &mut idle {
            match select(pin!(receive), timer.as_mut()).await {
                Either::Left((received, _)) => Some(received),
                Either::Right(_) => None,
            }
        } else {
            Some(receive.await)
        };
        if let Some(ref received) = received {
            if let Err(e) = received {
                if let Some(suppressed) = RECV_LOG.allow() {
                    log::error!("[udp]rear recvfrom failed: {} ({} suppressed)", e, suppressed);
                }
                break;
            }
            log::debug!("[udp]rear batched recvfrom[{}]", registry.count());

            // A peer that this association left must not deliver or renew it.
            let forward = if let (Some(fo), Some(cur)) = (failover.as_ref(), assoc.moved_peer()) {
                registry.retain(|pkt| !fo.is_stale(&pkt.addr.to_socket_addr(), cur));
                registry.count() != 0
            } else {
                true
            };
            if forward {
                if renew {
                    assoc.touch(activity_tick());
                }
                let pkts = registry.iter().map(|pkt| pkt.ref_with_addr(&laddr_s));
                let mut budget = SendBudget::new(CLIENT_WAIT);
                let stats = batched::send_all(&lsock, pkts, conn_opts.udp_no_fragment, &mut budget).await;
                if let Some(e) = stats.error {
                    if let Some(suppressed) = SEND_LOG.allow() {
                        log::error!(
                            "[udp]failed to sendto client {}: {} ({} sent, {} dropped, {} suppressed)",
                            laddr,
                            e,
                            stats.sent,
                            stats.dropped,
                            suppressed
                        );
                    }
                }
                continue;
            }
        }

        let Some(timer) = &mut idle else { continue };
        // A stream of stale replies would keep winning the receive-first
        // selection without renewing the association.
        if received.is_some() && futures::poll!(timer.as_mut()).is_pending() {
            continue;
        }
        let Some(remaining) = cleanup.sockmap.expire(&laddr, &assoc, timeout) else {
            cleanup.removed = true;
            break;
        };
        timer.as_mut().reset(Instant::now() + remaining);
    }
    log::debug!("[udp]remove association for {}", laddr);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::failover::{FailoverOpts, Proto};

    #[test]
    fn selects_the_first_upstream_matching_through() {
        let ipv4 = "127.0.0.1:0".parse().unwrap();
        let ipv6 = "[::1]:0".parse().unwrap();
        let other_ipv4 = "127.0.0.2:0".parse().unwrap();
        let other_ipv6 = "[::2]:0".parse().unwrap();
        let ipv4_first = [ipv4, ipv6, other_ipv4, other_ipv6];
        let ipv6_first = [ipv6, ipv4, other_ipv6, other_ipv4];
        assert_eq!(select_upstream(ipv4_first.into_iter(), None), Some(ipv4));
        assert_eq!(select_upstream(ipv6_first.into_iter(), None), Some(ipv6));
        assert_eq!(select_upstream(ipv4_first.into_iter(), Some(ipv6)), Some(ipv6));
        assert_eq!(select_upstream(ipv6_first.into_iter(), Some(ipv4)), Some(ipv4));
    }

    #[test]
    fn only_remembers_addresses_that_match_through() {
        let rname = RemoteAddr::DomainName("localhost".into(), 0);
        let ipv4 = "127.0.0.1:0".parse().unwrap();
        let ipv6 = "[::1]:0".parse().unwrap();
        let through = Some(ipv6);
        let mut last = None;
        let opposite_family = || Ok(select_upstream([ipv4].into_iter(), through));
        assert_eq!(remember_resolution(&rname, &mut last, opposite_family()), None);
        let both = Ok(select_upstream([ipv4, ipv6].into_iter(), through));
        assert_eq!(remember_resolution(&rname, &mut last, both), Some(ipv6));
        assert_eq!(remember_resolution(&rname, &mut last, opposite_family()), Some(ipv6));
    }

    #[test]
    fn resolution_failure_or_empty_result_preserves_last_address() {
        let rname = RemoteAddr::DomainName("localhost".into(), 0);
        let first = "127.0.0.1:0".parse().unwrap();
        let next = "127.0.0.2:0".parse().unwrap();
        let mut last = None;
        let failure = || Err(ErrorKind::TimedOut.into());
        assert_eq!(remember_resolution(&rname, &mut last, failure()), None);
        assert_eq!(remember_resolution(&rname, &mut last, Ok(None)), None);
        assert_eq!(remember_resolution(&rname, &mut last, Ok(Some(first))), Some(first));
        assert_eq!(remember_resolution(&rname, &mut last, failure()), Some(first));
        assert_eq!(remember_resolution(&rname, &mut last, Ok(None)), Some(first));
        assert_eq!(remember_resolution(&rname, &mut last, Ok(Some(next))), Some(next));
    }

    #[tokio::test(start_paused = true)]
    async fn receive_errors_wait_with_capped_exponential_backoff() {
        let mut delay = RECV_BACKOFF_MIN;
        for millis in [10, 20, 40, 80, 160, 320, 640, 1000, 1000] {
            let expected = Duration::from_millis(millis);
            assert_eq!(delay, expected);
            let started = Instant::now();
            back_off_receive(&mut delay).await;
            assert!((expected..=expected + Duration::from_millis(1)).contains(&started.elapsed()));
        }
    }

    async fn receive_at_idle_deadline(stale: bool) {
        let listener = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let old_peer = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let current_peer = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let laddr = client.local_addr().unwrap();
        let opts = ConnectOpts {
            associate_timeout: 1,
            ..Default::default()
        };
        let failover = Some(Failover::start(
            Proto::Udp,
            laddr.to_string(),
            vec![old_peer.local_addr().unwrap(), current_peer.local_addr().unwrap()],
            &FailoverOpts::default(),
            &opts,
        ));
        let upstream = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let to = upstream.local_addr().unwrap();
        let assoc = Arc::new(Assoc::new(upstream, 0, activity_tick().wrapping_sub(20)));
        assoc.move_to(1);
        let sockmap = Arc::new(SockMap::new());
        sockmap.insert(laddr, assoc.clone());
        let mut rear = pin!(send_back(
            Ref::new(&listener),
            laddr,
            assoc.clone(),
            true,
            Ref::new(&opts),
            sockmap.clone(),
            Ref::new(&failover),
        ));

        assert!(futures::poll!(rear.as_mut()).is_pending());
        tokio::time::advance(Duration::from_millis(1001)).await;
        let mut byte = [0];
        if stale {
            const BURST: usize = 16;
            for _ in 0..BURST {
                old_peer.send_to(b"s", to).unwrap();
            }
            assoc.sock.readable().await.unwrap();
            assert!(futures::poll!(rear.as_mut()).is_ready());
            assert!(sockmap.with_assoc(&laddr, |_| ()).is_none());
            let queued = std::iter::from_fn(|| assoc.sock.try_recv(&mut byte).ok()).count();
            assert!((1..BURST).contains(&queued));
            assert_eq!(client.try_recv(&mut byte).unwrap_err().kind(), ErrorKind::WouldBlock);
        } else {
            current_peer.send_to(b"r", to).unwrap();
            assoc.sock.readable().await.unwrap();
            assert!(futures::poll!(rear.as_mut()).is_pending());
            assert_eq!(
                sockmap.with_assoc(&laddr, |current| Arc::ptr_eq(current, &assoc)),
                Some(true)
            );
            let n = tokio::time::timeout(Duration::from_secs(1), client.recv(&mut byte))
                .await
                .unwrap()
                .unwrap();
            assert_eq!((n, byte), (1, *b"r"));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_current_peer_reply_wins_over_the_idle_timer() {
        receive_at_idle_deadline(false).await;
    }

    #[tokio::test(start_paused = true)]
    async fn an_association_expires_with_old_peer_replies_still_queued() {
        receive_at_idle_deadline(true).await;
    }

    #[tokio::test]
    async fn a_panicking_reply_task_removes_only_its_own_association() {
        let sockmap = Arc::new(SockMap::new());
        let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = sock.local_addr().unwrap();
        let assoc = Arc::new(Assoc::new(sock, 0, 0));
        let replacement = Arc::new(Assoc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap(), 0, 0));

        for current in [&assoc, &replacement] {
            sockmap.insert(addr, current.clone());
            let outcome = std::panic::catch_unwind(|| {
                let _cleanup = RemoveOnDrop {
                    sockmap: sockmap.clone(),
                    addr,
                    assoc: &assoc,
                    removed: false,
                };
                panic!("reply task panicked");
            });
            assert!(outcome.is_err());
            let replacement_remains = sockmap.with_assoc(&addr, |current| Arc::ptr_eq(current, &replacement));
            assert_eq!(replacement_remains, Arc::ptr_eq(current, &replacement).then_some(true));
        }
    }
}
