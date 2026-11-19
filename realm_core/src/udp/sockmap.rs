use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering::Relaxed};
use std::collections::HashMap;

use tokio::net::UdpSocket;

/// Upstream association of one client.
pub struct Assoc {
    pub sock: UdpSocket,
    /// Failover peer index, `MOVED` once the association left its first peer.
    /// Written only by the client-side task, so one relaxed atomic is enough.
    route: AtomicU8,
}

impl Assoc {
    const MOVED: u8 = 0x80;

    pub fn new(sock: UdpSocket, peer: usize) -> Self {
        Self {
            sock,
            route: AtomicU8::new(peer as u8),
        }
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

pub struct SockMap(RwLock<HashMap<SocketAddr, Arc<Assoc>>>, AtomicBool);

impl SockMap {
    pub fn new() -> Self {
        Self(RwLock::new(HashMap::new()), AtomicBool::new(false))
    }

    /// Set once the endpoint has carried a datagram longer than one ethernet frame.
    /// Its sockets then read every datagram with room for a large one.
    #[inline]
    pub fn seen_large(&self) -> &AtomicBool {
        &self.1
    }

    #[inline]
    pub fn find(&self, addr: &SocketAddr) -> Option<Arc<Assoc>> {
        // fetch the lock

        let sockmap = self.0.read().unwrap();

        sockmap.get(addr).cloned()

        // drop the lock
    }

    #[inline]
    pub fn insert(&self, addr: SocketAddr, assoc: Arc<Assoc>) {
        // fetch the lock
        let mut sockmap = self.0.write().unwrap();

        let _ = sockmap.insert(addr, assoc);

        // drop the lock
    }

    #[inline]
    pub fn find_or_insert<E, F>(&self, addr: &SocketAddr, f: F) -> Result<Arc<Assoc>, E>
    where
        F: Fn() -> Result<Arc<Assoc>, E>,
    {
        match self.find(addr) {
            Some(x) => Ok(x),
            None => {
                let assoc = f()?;
                self.insert(*addr, Arc::clone(&assoc));
                Ok(assoc)
            }
        }
    }

    #[inline]
    pub fn remove(&self, addr: &SocketAddr) {
        // fetch the lock
        let mut sockmap = self.0.write().unwrap();

        let _ = sockmap.remove(addr);

        // drop the lock
    }
}
