use std::io::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;

use super::SockMap;
use super::sockmap::Assoc;
use super::{socket, batched};

use crate::trick::Ref;
use crate::time::timeoutfut;
use crate::dns::resolve_addr;
use crate::endpoint::{RemoteAddr, ConnectOpts};
use crate::failover::Failover;

use batched::{Packet, SockAddrStore};
use registry::Registry;
mod registry {
    use super::*;
    type Range = std::ops::Range<u16>;

    pub struct Registry {
        pkts: Box<[Packet]>,
        groups: Vec<Range>,
        cursor: u16,
    }

    impl Registry {
        pub fn new(npkts: usize) -> Self {
            debug_assert!(npkts <= batched::MAX_PACKETS);
            Self {
                pkts: vec![Packet::new(); npkts].into_boxed_slice(),
                groups: Vec::with_capacity(npkts),
                cursor: 0u16,
            }
        }

        pub async fn batched_recv_on(&mut self, sock: &UdpSocket) -> Result<()> {
            let n = batched::recv_some(sock, &mut self.pkts).await?;
            self.cursor = n as u16;
            Ok(())
        }

        pub fn group_by_addr(&mut self) {
            let n = self.cursor as usize;
            self.groups.clear();
            group_by_inner(&mut self.pkts[..n], &mut self.groups, |a, b| a.addr == b.addr);
        }

        pub fn group_iter(&self) -> GroupIter<'_> {
            GroupIter {
                pkts: &self.pkts,
                ranges: self.groups.iter(),
            }
        }

        pub fn iter(&self) -> std::slice::Iter<'_, Packet> {
            self.pkts[..self.cursor as usize].iter()
        }

        /// Keep received packets matching `keep`, in order.
        pub fn retain<F: Fn(&Packet) -> bool>(&mut self, keep: F) {
            let n = self.cursor as usize;
            let mut w = 0;
            for r in 0..n {
                if keep(&self.pkts[r]) {
                    if w != r {
                        self.pkts.swap(w, r);
                    }
                    w += 1;
                }
            }
            self.cursor = w as u16;
        }

        pub const fn count(&self) -> usize {
            self.cursor as usize
        }
    }

    use std::slice::Iter;
    use std::iter::Iterator;
    pub struct GroupIter<'a> {
        pkts: &'a [Packet],
        ranges: Iter<'a, Range>,
    }

    impl<'a> Iterator for GroupIter<'a> {
        type Item = &'a [Packet];

        fn next(&mut self) -> Option<Self::Item> {
            self.ranges
                .next()
                .map(|Range { start, end }| &self.pkts[*start as usize..*end as usize])
        }
    }

    fn group_by_inner<T, F>(data: &mut [T], groups: &mut Vec<Range>, eq: F)
    where
        F: Fn(&T, &T) -> bool,
    {
        let maxn = data.len();
        let (mut beg, mut end) = (0, 1);
        while end < maxn {
            // go ahead if addr is same
            if eq(&data[end], &data[beg]) {
                end += 1;
                continue;
            }
            // pick packets afterwards
            let mut probe = end + 1;
            while probe < maxn {
                if eq(&data[probe], &data[beg]) {
                    data.swap(probe, end);
                    end += 1;
                }
                probe += 1;
            }
            groups.push(beg as _..end as _);
            (beg, end) = (end, end + 1);
        }
        groups.push(beg as _..end as _);
    }
}

pub async fn associate_and_relay(
    lis: Ref<UdpSocket>,
    rname: Ref<RemoteAddr>,
    conn_opts: Ref<ConnectOpts>,
    sockmap: Ref<SockMap>,
    failover: Ref<Option<Failover>>,
) -> Result<()> {
    let mut registry = Registry::new(batched::MAX_PACKETS);

    loop {
        registry.batched_recv_on(&lis).await?;
        log::debug!("[udp]entry batched recvfrom[{}]", registry.count());

        // one snapshot per batch with failover, otherwise resolve `rname` as before
        let (route, fixed) = match failover.as_ref() {
            Some(fo) => (Some((fo, fo.snapshot())), None),
            None => {
                let raddr = resolve_addr(&rname).await?.iter().next().unwrap();
                log::debug!("[udp]{} resolved as {}", *rname, raddr);
                (None, Some(raddr))
            }
        };

        registry.group_by_addr();
        for pkts in registry.group_iter() {
            let laddr = pkts[0].addr.clone().into();

            // peer for a new association
            let (peer, raddr) = match (route, fixed) {
                (Some((fo, snap)), _) => (snap.active(), fo.peer(snap.active())),
                (None, Some(raddr)) => (0, raddr),
                (None, None) => unreachable!(),
            };

            let assoc = sockmap.find_or_insert(&laddr, || {
                let assoc = Arc::new(Assoc::new(socket::associate(&raddr, &conn_opts)?, peer));
                tokio::spawn(send_back(lis, laddr, assoc.clone(), conn_opts, sockmap, failover));
                match route {
                    Some(_) => log::info!("[udp]new association {} => peer#{} {}", laddr, peer, raddr),
                    None => log::info!("[udp]new association {} => {} as {}", laddr, *rname, raddr),
                }
                Result::Ok(assoc)
            })?;

            // an existing association sticks to its peer until that peer is down
            let raddr = match route {
                Some((fo, snap)) => {
                    let cur = assoc.peer();
                    let next = Failover::repick(snap, cur);
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
                None => raddr,
            };

            let raddr: SockAddrStore = raddr.into();
            batched::send_all(&assoc.sock, pkts.iter().map(|x| x.ref_with_addr(&raddr))).await?;
        }
    }
}

async fn send_back(
    lsock: Ref<UdpSocket>,
    laddr: SocketAddr,
    assoc: Arc<Assoc>,
    conn_opts: Ref<ConnectOpts>,
    sockmap: Ref<SockMap>,
    failover: Ref<Option<Failover>>,
) {
    let mut registry = Registry::new(batched::MAX_PACKETS);
    let timeout = conn_opts.associate_timeout;
    let laddr_s: SockAddrStore = laddr.into();

    loop {
        match timeoutfut(registry.batched_recv_on(&assoc.sock), timeout).await {
            Err(_) => {
                log::debug!("[udp]rear recvfrom timeout");
                break;
            }
            Ok(Err(e)) => {
                log::error!("[udp]rear recvfrom failed: {}", e);
                break;
            }
            Ok(Ok(())) => {
                log::debug!("[udp]rear batched recvfrom[{}]", registry.count())
            }
        };

        // after a move, drop late replies from the peer(s) this association left
        if let (Some(fo), Some(cur)) = (failover.as_ref(), assoc.moved_peer()) {
            registry.retain(|pkt| !fo.is_stale(&pkt.addr.to_socket_addr(), cur));
            if registry.count() == 0 {
                continue;
            }
        }

        let pkts = registry.iter().map(|pkt| pkt.ref_with_addr(&laddr_s));
        if let Err(e) = batched::send_all(&lsock, pkts).await {
            log::error!("[udp]failed to sendto client{}: {}", laddr, e);
            break;
        }
    }

    sockmap.remove(&laddr);
    log::debug!("[udp]remove association for {}", laddr);
}
