use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering::Relaxed};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;

const TICK_MILLIS: u64 = 100;

fn activity_tick() -> u32 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    let elapsed = EPOCH.get_or_init(Instant::now).elapsed();
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

    pub fn new(sock: UdpSocket, peer: usize) -> Self {
        Self {
            sock,
            route: AtomicU8::new(peer as u8),
            activity: AtomicU32::new(activity_tick()),
        }
    }

    pub fn touch(&self) {
        // Sample again if the other direction renewed the association while we
        // read the clock, so a delayed update cannot move its activity backwards.
        let mut last = self.activity.load(Relaxed);
        while let Err(updated) = self
            .activity
            .compare_exchange_weak(last, activity_tick(), Relaxed, Relaxed)
        {
            last = updated;
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

    /// Check activity under the same lock that protects an upstream send and
    /// its renewal. Return the remaining idle time if the association must stay.
    pub fn expire(&self, addr: &SocketAddr, assoc: &Arc<Assoc>, timeout: Duration) -> Option<Duration> {
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
        let mut sockets = self.0.write().unwrap();
        if sockets.get(addr).is_some_and(|current| Arc::ptr_eq(current, assoc)) {
            sockets.remove(addr);
        }
    }
}
