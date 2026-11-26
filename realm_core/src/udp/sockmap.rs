use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering::Relaxed};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;

const TICK_MILLIS: u64 = 100;

pub(super) fn activity_tick() -> u32 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    let now = Instant::now();
    let elapsed = now.saturating_duration_since(*EPOCH.get_or_init(|| now));
    (elapsed.as_secs() as u32)
        .wrapping_mul(10)
        .wrapping_add(elapsed.subsec_millis() / TICK_MILLIS as u32)
}

/// Upstream association of one client.
pub struct Assoc {
    pub sock: UdpSocket,
    /// Failover peer index, `MOVED` once the association left its first peer.
    /// Written only by the client-side task, so one relaxed atomic is enough.
    route: AtomicU8,
    activity: AtomicU32,
}

impl Assoc {
    const MOVED: u8 = 0x80;

    pub fn new(sock: UdpSocket, peer: usize, tick: u32) -> Self {
        Self {
            sock,
            route: AtomicU8::new(peer as u8),
            activity: AtomicU32::new(tick),
        }
    }

    pub fn touch(&self, tick: u32) {
        let mut last = self.activity.load(Relaxed);
        // Compare wrapping ticks so a sample from either direction cannot
        // replace a newer one, including at the wrap boundary.
        while (tick.wrapping_sub(last) as i32) > 0 {
            match self.activity.compare_exchange_weak(last, tick, Relaxed, Relaxed) {
                Ok(_) => break,
                Err(updated) => last = updated,
            }
        }
    }

    fn idle_remaining(&self, timeout: Duration) -> Option<Duration> {
        let last = self.activity.load(Relaxed);
        let ticks = activity_tick().wrapping_sub(last);
        // Activity could have occurred at the end of its tick.
        let idle = Duration::from_millis(u64::from(ticks.saturating_sub(1)) * TICK_MILLIS);
        timeout.checked_sub(idle).filter(|remaining| !remaining.is_zero())
    }

    #[inline]
    pub fn peer(&self) -> usize {
        (self.route.load(Relaxed) & !Self::MOVED) as usize
    }

    /// Point the association to another peer.
    #[inline]
    pub fn move_to(&self, peer: usize) {
        self.route.store(peer as u8 | Self::MOVED, Relaxed);
    }

    /// Current peer, only if the association has ever moved.
    #[inline]
    pub fn moved_peer(&self) -> Option<usize> {
        let route = self.route.load(Relaxed);
        (route & Self::MOVED != 0).then_some((route & !Self::MOVED) as usize)
    }
}

pub struct SockMap(RwLock<HashMap<SocketAddr, Arc<Assoc>>>);

impl SockMap {
    pub fn new() -> Self {
        Self(RwLock::new(HashMap::new()))
    }

    pub fn with_assoc<T>(&self, addr: &SocketAddr, f: impl FnOnce(&Arc<Assoc>) -> T) -> Option<T> {
        let sockets = self.0.read().unwrap();
        sockets.get(addr).map(f)
    }

    pub fn insert(&self, addr: SocketAddr, assoc: Arc<Assoc>) {
        self.0.write().unwrap().insert(addr, assoc);
    }

    /// Return the remaining idle time if the association must stay. Recheck
    /// expiry under the lock that protects an upstream send and its renewal.
    pub fn expire(&self, addr: &SocketAddr, assoc: &Arc<Assoc>, timeout: Duration) -> Option<Duration> {
        if let Some(remaining) = assoc.idle_remaining(timeout) {
            return Some(remaining);
        }
        let mut sockets = self.0.write().unwrap();
        if !sockets.get(addr).is_some_and(|current| Arc::ptr_eq(current, assoc)) {
            return None;
        }
        let remaining = assoc.idle_remaining(timeout);
        if remaining.is_none() {
            sockets.remove(addr);
        }
        remaining
    }

    pub fn remove(&self, addr: &SocketAddr, assoc: &Arc<Assoc>) {
        let mut sockets = self.0.write().unwrap_or_else(|e| e.into_inner());
        if sockets.get(addr).is_some_and(|current| Arc::ptr_eq(current, assoc)) {
            sockets.remove(addr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn activity_does_not_go_backwards_when_ticks_arrive_out_of_order() {
        let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let assoc = Assoc::new(sock, 0, u32::MAX - 1);
        for (sample, latest) in [(u32::MAX, u32::MAX), (0, 0), (u32::MAX, 0), (0, 0), (1, 1), (0, 1)] {
            assoc.touch(sample);
            assert_eq!(assoc.activity.load(Relaxed), latest);
        }
    }

    #[tokio::test]
    async fn an_active_association_does_not_acquire_the_map_write_lock() {
        use std::sync::mpsc;

        let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = sock.local_addr().unwrap();
        let assoc = Arc::new(Assoc::new(sock, 0, activity_tick()));
        let sockmap = SockMap::new();
        sockmap.insert(addr, assoc.clone());

        let timeout = Duration::from_secs(2);
        let read = sockmap.0.read().unwrap();
        let (reply, result) = mpsc::channel();
        let remaining = std::thread::scope(|scope| {
            scope.spawn(|| reply.send(sockmap.expire(&addr, &assoc, timeout)).unwrap());
            let remaining = result.recv_timeout(Duration::from_secs(1));
            drop(read);
            remaining
        });
        assert!(remaining.unwrap().is_some());
    }
}
