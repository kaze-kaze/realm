//! Active-standby failover.
//!
//! `remote` is the primary peer and `extra_remotes` are backups in priority order.
//! Together with `balance`, new sessions are spread over the peers that are alive
//! instead (see [`Failover::pick`]).
//!
//! The control plane is one probe task per endpoint and protocol. It publishes a
//! packed [`Snapshot`] into a single atomic. The data plane only loads that atomic
//! when a tcp connection is dialed or a udp batch is dispatched, so relaying itself
//! (splice, batched udp) is untouched.
//!
//! Decisions are made on probe results only, never on user traffic:
//! - a peer goes down after `fall` failures in the last `window` probes;
//! - a down peer comes back after `rise` consecutive successes, with a clean record;
//! - traffic only moves to a peer whose latest probe succeeded;
//! - failing back to a higher priority peer waits `hold` after the last switch,
//!   unless the current peer is down;
//! - when no peer is usable, the current choice is kept.

use std::fmt::{Display, Formatter};
use std::future::Future;
use std::io::{Error, ErrorKind};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
// no 64-bit atomics: 32-bit mips targets lack them
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::future::join_all;
use futures::stream::{FuturesUnordered, StreamExt};
use tokio::net::UdpSocket;
use tokio::time::{Instant, MissedTickBehavior, interval, sleep_until, timeout_at};

use crate::endpoint::{ConnectOpts, RemoteAddr};

/// Max peers, `remote` included. Bounded by the packed snapshot.
pub const MAX_PEERS: usize = 8;

/// A probe round makes at most this many attempts (tcp connections, udp datagrams),
/// spread evenly over `timeout` and only while nothing has answered.
const TRIES: u32 = 3;

/// Failover options.
#[derive(Debug, Clone)]
pub struct FailoverOpts {
    /// Health check targets, one per peer, in the order of `[remote, extra_remotes..]`.
    /// Empty means probing the data address of each peer.
    pub probe: Vec<SocketAddr>,
    /// Tcp: bytes to send after connecting. Empty sends nothing.
    pub tcp_send: Vec<u8>,
    /// Tcp: expected prefix of the reply. Empty accepts a completed handshake.
    pub tcp_expect: Vec<u8>,
    /// Udp: payload prefix. A per-round nonce is appended and the peer must echo the datagram.
    pub udp_send: Vec<u8>,
    /// Udp: what to send and which answer counts.
    pub udp_probe: UdpProbe,
    pub interval: Duration,
    pub timeout: Duration,
    /// A peer goes down after `fall` failures in the last `window` probes.
    pub fall: u8,
    pub window: u8,
    /// A down peer comes back after `rise` consecutive successes.
    pub rise: u8,
    /// Minimum time before failing back to a higher priority peer.
    pub hold: Duration,
}

impl Default for FailoverOpts {
    fn default() -> Self {
        Self {
            probe: Vec::new(),
            tcp_send: Vec::new(),
            tcp_expect: Vec::new(),
            udp_send: Vec::new(),
            udp_probe: UdpProbe::Echo,
            interval: Duration::from_secs(3),
            timeout: Duration::from_millis(1500),
            fall: 3,
            window: 5,
            rise: 5,
            hold: Duration::from_secs(30),
        }
    }
}

impl Display for FailoverOpts {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "interval={:?}, timeout={:?}, fall={}/{}, rise={}, hold={:?}",
            self.interval, self.timeout, self.fall, self.window, self.rise, self.hold
        )?;
        if self.udp_probe == UdpProbe::Quic {
            write!(f, ", udp-probe=quic")?;
        }
        Ok(())
    }
}

/// What the udp probe sends and which answer counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UdpProbe {
    /// `udp_send` plus a nonce; the target must echo the datagram.
    #[default]
    Echo,
    /// A QUIC packet of a version no server speaks. Every QUIC server answers it with
    /// a Version Negotiation packet, so a QUIC service is probed on its own port and
    /// needs no echo responder next to it.
    Quic,
}

/// Protocol a failover state serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proto {
    Tcp,
    Udp,
}

impl Display for Proto {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Proto::Tcp => write!(f, "tcp"),
            Proto::Udp => write!(f, "udp"),
        }
    }
}

/// Packed health view: bits 0..8 up, bits 8..16 ready (up and latest probe ok), bits 16..24 active peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot(u32);

impl Snapshot {
    fn pack(active: usize, up: u8, ready: u8) -> Self {
        Self((active as u32) << 16 | (ready as u32) << 8 | up as u32)
    }

    /// Peer that new sessions use.
    #[inline]
    pub const fn active(self) -> usize {
        (self.0 >> 16) as u8 as usize
    }

    /// Peer is considered alive.
    #[inline]
    pub const fn is_up(self, i: usize) -> bool {
        self.0 >> i & 1 == 1
    }

    /// Peer is alive and its latest probe succeeded.
    #[inline]
    pub const fn is_ready(self, i: usize) -> bool {
        self.0 >> (8 + i) & 1 == 1
    }

    /// At least one peer is alive.
    #[inline]
    pub const fn any_up(self) -> bool {
        self.0 & 0xff != 0
    }
}

/// Runtime failover state of one endpoint and protocol.
pub struct Failover {
    peers: Box<[SocketAddr]>,
    state: Arc<AtomicU32>,
    /// `balance` is on: peers share new sessions, none of them stands by.
    #[cfg(feature = "balance")]
    balanced: bool,
}

impl Failover {
    /// Create the state and spawn its probe task. Must run inside a tokio runtime.
    pub fn start(
        proto: Proto,
        label: String,
        peers: Vec<SocketAddr>,
        opts: &FailoverOpts,
        conn_opts: &ConnectOpts,
    ) -> Self {
        assert!(
            (1..=MAX_PEERS).contains(&peers.len()),
            "[failover]{}: expect 1..={} peers, got {}",
            label,
            MAX_PEERS,
            peers.len()
        );
        let targets = if opts.probe.is_empty() {
            peers.clone()
        } else {
            opts.probe.clone()
        };
        assert_eq!(
            targets.len(),
            peers.len(),
            "[failover]{}: probe targets must match peers one to one",
            label
        );

        #[cfg(feature = "balance")]
        let balanced = conn_opts.balancer.strategy() != realm_lb::Strategy::Off;
        #[cfg(not(feature = "balance"))]
        let balanced = false;

        // probes leave from the `through` ip but never from its fixed port (udp may pin
        // one): sharing that port would take replies away from the associations
        let mut conn_opts = conn_opts.clone();
        if let Some(addr) = conn_opts.bind_address.as_mut() {
            addr.set_port(0);
        }

        let fsm = Fsm::new(peers.len(), opts);
        let state = Arc::new(AtomicU32::new(fsm.snapshot().0));
        let peers: Box<[SocketAddr]> = peers.into();

        tokio::spawn(monitor(
            Monitor {
                proto,
                label,
                peers: peers.clone(),
                targets,
                opts: opts.clone(),
                conn_opts,
                state: state.clone(),
                balanced,
            },
            fsm,
        ));

        Self {
            peers,
            state,
            #[cfg(feature = "balance")]
            balanced,
        }
    }

    #[inline]
    pub fn snapshot(&self) -> Snapshot {
        Snapshot(self.state.load(Relaxed))
    }

    #[inline]
    pub fn peer(&self, i: usize) -> SocketAddr {
        self.peers[i]
    }

    /// Peer to dial after the peers in `tried` failed, if any is ready: by priority,
    /// or with `balance` on (`ring`) the next one around after the last that was tried.
    pub fn retry_peer(&self, snap: Snapshot, tried: &[usize], ring: bool) -> Option<usize> {
        let n = self.peers.len();
        let ok = |i: &usize| snap.is_ready(*i) && !tried.contains(i);
        match tried.last() {
            Some(&last) if ring => (1..n).map(|k| (last + k) % n).find(ok),
            _ => (0..n).find(ok),
        }
    }

    /// Peer an existing udp association should use: stay unless its peer is down
    /// and the active peer is alive. When every peer is down there is nothing
    /// better to move to, so the association stays where it is.
    #[inline]
    pub const fn repick(snap: Snapshot, cur: usize) -> usize {
        let active = snap.active();
        if snap.is_up(cur) || !snap.is_up(active) {
            cur
        } else {
            active
        }
    }

    /// With `balance` on: the peer for a new session from `src`.
    ///
    /// It is the balancer's choice while that peer is alive, even right after one
    /// failed probe, so `iphash` stays sticky. When it is down, what the dead peer
    /// would get is shared by the others in their own proportions instead of all
    /// of it falling on its neighbour: round robin is asked again until it names a
    /// ready peer, ip hash names the peer that comes next for `src` among the ready
    /// ones, the same one every time, which keeps a client on one peer.
    pub(crate) fn balanced_peer(&self, conn_opts: &ConnectOpts, src: &IpAddr, snap: Snapshot) -> Option<usize> {
        #[cfg(feature = "balance")]
        if self.balanced {
            /// Enough to get past dead peers of a round robin whose weights sum up
            /// to this; beyond it the neighbour takes over as before.
            const TRIES: u32 = 32;
            let n = self.peers.len();
            let ask = |ip: &IpAddr| {
                let ctx = realm_lb::BalanceCtx { src_ip: ip };
                conn_opts.balancer.next(ctx).map(|t| t.0 as usize % n)
            };
            let want = ask(src)?;
            if snap.is_up(want) || !snap.any_up() {
                return Some(want);
            }
            if let realm_lb::Balancer::IpHash(iphash) = &conn_opts.balancer {
                let among = |ok: &dyn Fn(usize) -> bool| {
                    let next = iphash.next_among(src, |i| i < n && ok(i));
                    next.map(|t| t.0 as usize)
                };
                // ready peers first, then, as `pick_among` does, those merely alive
                let next = among(&|i| snap.is_ready(i)).or_else(|| among(&|i| snap.is_up(i)));
                return Some(next.unwrap_or(want));
            }
            for _ in 0..TRIES {
                match ask(src) {
                    Some(i) if snap.is_ready(i) => return Some(i),
                    _ => {}
                }
            }
            return Some(pick_among(n, snap, want));
        }
        let _ = (conn_opts, src, snap);
        None
    }

    /// The peer after `want` that takes over when `want` is down: `want` while it
    /// is alive, otherwise the next peer after it that is ready, or at least alive.
    /// With no peer alive, `want` is as good as any.
    #[inline]
    pub fn pick(&self, snap: Snapshot, want: usize) -> usize {
        pick_among(self.peers.len(), snap, want)
    }

    /// Balanced counterpart of [`Failover::repick`]: an association stays on its
    /// peer until that peer is down, then moves to `next`, the peer a new session
    /// of its client would get now. With no peer alive it stays where it is.
    #[inline]
    pub const fn repick_balanced(snap: Snapshot, cur: usize, next: usize) -> usize {
        if snap.is_up(cur) || !snap.is_up(next) {
            cur
        } else {
            next
        }
    }

    /// `src` is a peer other than `cur`, i.e. a late reply after the association moved.
    #[inline]
    pub fn is_stale(&self, src: &SocketAddr, cur: usize) -> bool {
        self.peers.iter().enumerate().any(|(i, p)| i != cur && p == src)
    }
}

fn pick_among(n: usize, snap: Snapshot, want: usize) -> usize {
    let want = want % n;
    if snap.is_up(want) {
        return want;
    }
    let after = |k: usize| (want + k) % n;
    (1..n)
        .map(after)
        .find(|&i| snap.is_ready(i))
        .or_else(|| (1..n).map(after).find(|&i| snap.is_up(i)))
        .unwrap_or(want)
}

/// Collect `[raddr, extra_raddrs..]` as socket addresses.
///
/// Failover peers must be `ip:port` literals: health is tracked per address,
/// and a dns failure must never look like a peer failure.
pub fn literal_peers(raddr: &RemoteAddr, extra_raddrs: &[RemoteAddr]) -> Vec<SocketAddr> {
    std::iter::once(raddr)
        .chain(extra_raddrs)
        .map(|r| match r {
            RemoteAddr::SocketAddr(addr) => *addr,
            RemoteAddr::DomainName(..) => panic!("[failover]peer {} must be an ip:port literal", r),
        })
        .collect()
}

/// Result of one probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Probe {
    Ok,
    Fail,
    /// Local error (socket, bind), says nothing about the peer.
    Unknown,
}

#[derive(Debug, Clone, Copy)]
struct Health {
    /// Shift register, 1 = failed probe, newest at bit 0.
    fails: u8,
    /// Consecutive successes.
    streak: u8,
    up: bool,
    /// Latest probe succeeded.
    fresh: bool,
}

/// Pure decision logic, driven by probe rounds.
#[derive(Debug, Clone)]
pub(crate) struct Fsm {
    peers: Vec<Health>,
    active: usize,
    switched_at: Option<Instant>,
    mask: u8,
    fall: u32,
    rise: u8,
    hold: Duration,
}

impl Fsm {
    pub(crate) fn new(n: usize, opts: &FailoverOpts) -> Self {
        let window = opts.window.clamp(1, 8);
        Self {
            peers: vec![
                Health {
                    fails: 0,
                    streak: 0,
                    up: true,
                    fresh: false,
                };
                n
            ],
            active: 0,
            switched_at: None,
            mask: (u16::MAX >> (16 - window as u32)) as u8,
            fall: opts.fall.clamp(1, window) as u32,
            rise: opts.rise.max(1),
            hold: opts.hold,
        }
    }

    pub(crate) fn update(&mut self, results: &[Probe], now: Instant) {
        for (p, r) in self.peers.iter_mut().zip(results) {
            match r {
                Probe::Unknown => {}
                Probe::Ok => {
                    p.fails = (p.fails << 1) & self.mask;
                    p.streak = p.streak.saturating_add(1);
                    p.fresh = true;
                    if !p.up && p.streak >= self.rise {
                        p.up = true;
                        // `rise` may be shorter than `window`: do not let the
                        // failures that took it down count against it again
                        p.fails = 0;
                    }
                }
                Probe::Fail => {
                    p.fails = ((p.fails << 1) | 1) & self.mask;
                    p.streak = 0;
                    p.fresh = false;
                    if p.up && p.fails.count_ones() >= self.fall {
                        p.up = false;
                    }
                }
            }
        }
        self.select(now);
    }

    fn ready(&self, i: usize) -> bool {
        self.peers[i].up && self.peers[i].fresh
    }

    fn select(&mut self, now: Instant) {
        let cur = self.active;
        let Some(best) = (0..self.peers.len()).find(|&i| self.ready(i)) else {
            // nothing usable: keep the current choice
            return;
        };
        let switch = if !self.peers[cur].up {
            best != cur
        } else {
            best < cur && self.switched_at.is_none_or(|t| now.duration_since(t) >= self.hold)
        };
        if switch {
            self.active = best;
            self.switched_at = Some(now);
        }
    }

    pub(crate) fn snapshot(&self) -> Snapshot {
        let (mut up, mut ready) = (0u8, 0u8);
        for i in 0..self.peers.len() {
            up |= (self.peers[i].up as u8) << i;
            ready |= (self.ready(i) as u8) << i;
        }
        Snapshot::pack(self.active, up, ready)
    }
}

struct Monitor {
    proto: Proto,
    label: String,
    peers: Box<[SocketAddr]>,
    targets: Vec<SocketAddr>,
    opts: FailoverOpts,
    conn_opts: ConnectOpts,
    state: Arc<AtomicU32>,
    balanced: bool,
}

async fn monitor(m: Monitor, mut fsm: Fsm) {
    let Monitor {
        proto,
        label,
        peers,
        targets,
        opts,
        conn_opts,
        state,
        balanced,
    } = m;

    let mut tick = interval(opts.interval);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tick.tick().await;

        let results = join_all(targets.iter().map(|t| probe(proto, *t, &opts, &conn_opts))).await;
        log::debug!("[failover][{}]{} probe: {:?}", proto, label, results);

        let before = fsm.snapshot();
        fsm.update(&results, Instant::now());
        let after = fsm.snapshot();

        if before != after {
            for (i, peer) in peers.iter().enumerate() {
                if before.is_up(i) != after.is_up(i) {
                    let state = if after.is_up(i) { "up" } else { "down" };
                    log::warn!("[failover][{}]{} peer#{} {} is {}", proto, label, i, peer, state);
                }
            }
            // balanced peers have no active one to switch
            if !balanced && before.active() != after.active() {
                let (from, to) = (before.active(), after.active());
                log::warn!(
                    "[failover][{}]{} switch peer#{} {} => peer#{} {}",
                    proto,
                    label,
                    from,
                    peers[from],
                    to,
                    peers[to]
                );
            }
            state.store(after.0, Relaxed);
        }
    }
}

async fn probe(proto: Proto, target: SocketAddr, opts: &FailoverOpts, conn_opts: &ConnectOpts) -> Probe {
    match proto {
        Proto::Tcp => probe_tcp(target, opts, conn_opts).await,
        Proto::Udp => probe_udp(target, opts, conn_opts).await,
    }
}

/// Connect, optionally send a request, and require the expected reply prefix.
///
/// A lost syn is only retransmitted after a second. So while nothing has answered,
/// another connection is started at each third of `timeout`, at most [`TRIES`] per
/// round. The first answer decides, be it a reply, a refusal or a local error.
async fn probe_tcp(target: SocketAddr, opts: &FailoverOpts, conn_opts: &ConnectOpts) -> Probe {
    let start = Instant::now();
    let mut attempts = FuturesUnordered::new();
    for k in 1..=TRIES {
        attempts.push(tcp_attempt(target, opts, conn_opts));
        let until = start + opts.timeout * k / TRIES;
        if let Ok(Some(res)) = timeout_at(until, attempts.next()).await {
            return res;
        }
    }
    log::debug!("[failover][tcp]probe {}: timeout", target);
    Probe::Fail
}

/// One probe connection, without a deadline of its own.
async fn tcp_attempt(target: SocketAddr, opts: &FailoverOpts, conn_opts: &ConnectOpts) -> Probe {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let socket = match crate::tcp::socket::prepare(&target, conn_opts, None) {
        Ok(x) => x,
        Err(e) => {
            log::debug!("[failover][tcp]probe {}: local error: {}", target, e);
            return Probe::Unknown;
        }
    };

    let run = async {
        let mut stream = socket.connect(target).await?;
        if !opts.tcp_send.is_empty() {
            stream.write_all(&opts.tcp_send).await?;
        }
        let want = &opts.tcp_expect[..];
        let mut buf = vec![0u8; want.len()];
        let mut n = 0;
        while n < want.len() {
            let end = n + stream.read(&mut buf[n..]).await?;
            if end == n || buf[n..end] != want[n..end] {
                return Ok(false);
            }
            n = end;
        }
        Ok::<_, Error>(true)
    };

    match run.await {
        Ok(true) => Probe::Ok,
        Ok(false) => {
            log::debug!("[failover][tcp]probe {}: unexpected reply", target);
            Probe::Fail
        }
        Err(e) if is_local(&e) => {
            log::debug!("[failover][tcp]probe {}: local error: {}", target, e);
            Probe::Unknown
        }
        Err(e) => {
            log::debug!("[failover][tcp]probe {}: {}", target, e);
            Probe::Fail
        }
    }
}

/// This host ran out of local ports towards the target. That says nothing about the
/// peer, and new connections still spill over to the next peer one by one.
fn is_local(e: &Error) -> bool {
    matches!(e.kind(), ErrorKind::AddrNotAvailable | ErrorKind::AddrInUse)
}

/// Version field of the QUIC probe: `0x?a?a?a?a` is reserved for forcing version
/// negotiation (RFC 9000, 15), no server ever speaks it.
const QUIC_PROBE_VERSION: [u8; 4] = [0x1a, 0x2a, 0x3a, 0x4a];
/// Servers ignore smaller datagrams of an unknown version (RFC 9000, 14.1).
const QUIC_PROBE_SIZE: usize = 1200;

/// A QUIC long header packet (RFC 8999, 5.1) of [`QUIC_PROBE_VERSION`] with the
/// nonce as its two connection ids.
fn quic_probe(nonce: &[u8; 16]) -> Vec<u8> {
    let mut pkt = Vec::with_capacity(QUIC_PROBE_SIZE);
    pkt.push(0xc0);
    pkt.extend_from_slice(&QUIC_PROBE_VERSION);
    pkt.push(8);
    pkt.extend_from_slice(&nonce[..8]);
    pkt.push(8);
    pkt.extend_from_slice(&nonce[8..]);
    pkt.resize(QUIC_PROBE_SIZE, 0);
    pkt
}

/// `reply` is the Version Negotiation packet (RFC 8999, 6) for this probe: a long
/// header of version 0 that returns both connection ids, swapped.
fn is_quic_version_negotiation(reply: &[u8], nonce: &[u8; 16]) -> bool {
    const HEADER: usize = 23;
    const VERSION: usize = 4;
    reply.len() >= HEADER + VERSION
        && (reply.len() - HEADER).is_multiple_of(VERSION)
        && reply[0] & 0x80 != 0
        && reply[1..5] == [0u8; 4]
        && reply[5] == 8
        && reply[6..14] == nonce[8..]
        && reply[14] == 8
        && reply[15..23] == nonce[..8]
}

/// Send a probe datagram and wait for its answer: `udp_send + nonce` that must come
/// back as is, or a QUIC packet that draws a Version Negotiation. Resend only while
/// unanswered, at most [`TRIES`] datagrams within `timeout`.
async fn probe_udp(target: SocketAddr, opts: &FailoverOpts, conn_opts: &ConnectOpts) -> Probe {
    let sock = match crate::udp::socket::associate(&target, conn_opts) {
        Ok(x) => x,
        Err(e) => {
            log::debug!("[failover][udp]probe {}: local error: {}", target, e);
            return Probe::Unknown;
        }
    };
    if let Err(e) = sock.connect(target).await {
        log::debug!("[failover][udp]probe {}: {}", target, e);
        return if is_local(&e) { Probe::Unknown } else { Probe::Fail };
    }

    let nonce = nonce();
    let quic = opts.udp_probe == UdpProbe::Quic;
    let msg = if quic {
        quic_probe(&nonce)
    } else {
        let mut msg = Vec::with_capacity(opts.udp_send.len() + 16);
        msg.extend_from_slice(&opts.udp_send);
        msg.extend_from_slice(&nonce);
        msg
    };
    // echo: one extra byte so that a longer datagram never matches
    let mut buf = vec![0u8; if quic { 1500 } else { msg.len() + 1 }];
    let answered = |reply: &[u8]| {
        if quic {
            is_quic_version_negotiation(reply, &nonce)
        } else {
            reply == &msg[..]
        }
    };

    let start = Instant::now();
    for k in 1..=TRIES {
        let until = start + opts.timeout * k / TRIES;
        if let Some(result) = udp_attempt(&sock, target, sock.send(&msg), &mut buf, answered, until).await {
            return result;
        }
    }
    log::debug!("[failover][udp]probe {}: no answer", target);
    Probe::Fail
}

/// A send and its replies share one deadline. No result means the round can retry.
async fn udp_attempt(
    sock: &UdpSocket,
    target: SocketAddr,
    send: impl Future<Output = std::io::Result<usize>>,
    buf: &mut [u8],
    answered: impl Fn(&[u8]) -> bool,
    until: Instant,
) -> Option<Probe> {
    match timeout_at(until, send).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            log::debug!("[failover][udp]probe {}: local send error: {}", target, e);
            return Some(Probe::Unknown);
        }
        Err(_) => return None,
    }

    loop {
        match timeout_at(until, sock.recv(buf)).await {
            Ok(Ok(n)) if answered(&buf[..n]) => return Some(Probe::Ok),
            Ok(Ok(_)) => continue,
            Ok(Err(e)) => {
                log::debug!("[failover][udp]probe {}: {}", target, e);
                sleep_until(until).await;
                return None;
            }
            Err(_) => return None,
        }
    }
}

/// Unique per probe round within this process.
fn nonce() -> [u8; 16] {
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let seq = SEQ.fetch_add(1, Relaxed);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let mut b = [0u8; 16];
    b[..12].copy_from_slice(&now.to_le_bytes()[..12]);
    b[12..].copy_from_slice(&seq.to_le_bytes());
    b
}

/// Error for a dial that ran out of its share of the deadline.
pub(crate) fn timed_out() -> Error {
    Error::new(ErrorKind::TimedOut, "connect timeout")
}

#[cfg(test)]
mod tests {
    use super::*;
    use Probe::{Fail as F, Ok as O, Unknown as U};

    fn opts() -> FailoverOpts {
        FailoverOpts {
            hold: Duration::from_secs(30),
            ..Default::default()
        }
    }

    /// Feed rounds of `[primary, backup]` results, one second apart.
    fn run(fsm: &mut Fsm, t: &mut Instant, rounds: &[[Probe; 2]]) {
        for r in rounds {
            *t += Duration::from_secs(1);
            fsm.update(r, *t);
        }
    }

    fn version_negotiation(nonce: &[u8], versions: &[u8]) -> Vec<u8> {
        let mut vn = vec![0x8f, 0, 0, 0, 0, 8];
        vn.extend_from_slice(&nonce[8..]);
        vn.push(8);
        vn.extend_from_slice(&nonce[..8]);
        vn.extend_from_slice(versions);
        vn
    }

    #[test]
    fn quic_probe_and_its_answer() {
        let nonce: [u8; 16] = std::array::from_fn(|i| i as u8 + 1);
        let pkt = quic_probe(&nonce);
        assert_eq!(pkt.len(), 1200);
        assert_eq!(pkt[0] & 0xc0, 0xc0);
        assert_eq!(pkt[1..5], QUIC_PROBE_VERSION);
        assert_eq!((pkt[5], &pkt[6..14]), (8, &nonce[..8]));
        assert_eq!((pkt[14], &pkt[15..23]), (8, &nonce[8..]));

        let versions = [0, 0, 0, 1];
        let mut vn = version_negotiation(&nonce, &versions);
        assert!(is_quic_version_negotiation(&vn, &nonce));

        // not an answer to this probe
        let mut other = nonce;
        other[0] ^= 1;
        assert!(!is_quic_version_negotiation(&vn, &other));
        // an echo of the probe is not a quic server
        assert!(!is_quic_version_negotiation(&pkt, &nonce));
        // short header, wrong version, truncated
        let mut bad = vn.clone();
        bad[0] = 0x4f;
        assert!(!is_quic_version_negotiation(&bad, &nonce));
        let mut bad = vn.clone();
        bad[4] = 1;
        assert!(!is_quic_version_negotiation(&bad, &nonce));
        for n in [22, 23, 24, 25, 26] {
            assert!(!is_quic_version_negotiation(&vn[..n], &nonce));
        }
        vn.push(0);
        assert!(!is_quic_version_negotiation(&vn, &nonce));
        vn.extend_from_slice(&[0, 0, 2]);
        assert!(is_quic_version_negotiation(&vn, &nonce));
    }

    #[tokio::test]
    async fn tcp_attempt_rejects_wrong_prefix_before_the_rest_arrives() {
        use tokio::io::AsyncWriteExt;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:39313").await.unwrap();
        let target = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            stream.write_all(b"!").await.unwrap();
            std::future::pending::<()>().await;
        });
        let opts = FailoverOpts {
            tcp_expect: b"OK".to_vec(),
            ..Default::default()
        };
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            tcp_attempt(target, &opts, &ConnectOpts::default()),
        )
        .await;
        server.abort();
        let _ = server.await;
        assert_eq!(result.unwrap(), F);
    }

    #[tokio::test]
    async fn udp_attempt_times_out_while_sending() {
        let socket = UdpSocket::bind("127.0.0.1:39314").await.unwrap();
        let target = "127.0.0.1:39315".parse().unwrap();
        let until = Instant::now() + Duration::from_millis(20);
        let mut buf = [0; 1];
        let attempt = udp_attempt(&socket, target, std::future::pending(), &mut buf, |_| false, until);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), attempt).await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn udp_probe_treats_a_local_send_error_as_unknown() {
        let peer = UdpSocket::bind("127.0.0.1:39316").await.unwrap();
        let opts = FailoverOpts {
            udp_send: vec![0; 65536],
            ..Default::default()
        };
        assert_eq!(
            probe_udp(peer.local_addr().unwrap(), &opts, &ConnectOpts::default()).await,
            U
        );
    }

    #[tokio::test]
    async fn udp_probe_accepts_a_large_quic_version_negotiation() {
        let peer = UdpSocket::bind("127.0.0.1:39317").await.unwrap();
        let target = peer.local_addr().unwrap();
        let respond = async {
            let mut buf = [0; QUIC_PROBE_SIZE];
            let (_, source) = peer.recv_from(&mut buf).await.unwrap();
            let nonce: Vec<_> = buf[6..14].iter().chain(&buf[15..23]).copied().collect();
            let versions: Vec<u8> = (1u32..=350).flat_map(u32::to_be_bytes).collect();
            let reply = version_negotiation(&nonce, &versions);
            peer.send_to(&reply, source).await.unwrap();
        };
        let opts = FailoverOpts {
            udp_probe: UdpProbe::Quic,
            ..Default::default()
        };
        let conn_opts = ConnectOpts::default();
        let result = tokio::time::timeout(Duration::from_secs(1), async {
            tokio::join!(respond, probe_udp(target, &opts, &conn_opts)).1
        })
        .await;
        assert_eq!(result.unwrap(), O);
    }

    #[test]
    fn balanced_pick() {
        let snap = |up: u8, ready: u8| Snapshot::pack(0, up, ready);
        // the wanted peer while it is alive, even right after one failed probe
        assert_eq!(pick_among(3, snap(0b111, 0b111), 1), 1);
        assert_eq!(pick_among(3, snap(0b111, 0b101), 1), 1);
        // down: the next ready peer after it, wrapping around
        assert_eq!(pick_among(3, snap(0b101, 0b101), 1), 2);
        assert_eq!(pick_among(3, snap(0b011, 0b011), 2), 0);
        assert_eq!(pick_among(3, snap(0b100, 0b100), 0), 2);
        // a ready peer wins over one that is only alive
        assert_eq!(pick_among(3, snap(0b110, 0b100), 0), 2);
        // nothing ready: at least an alive one
        assert_eq!(pick_among(3, snap(0b100, 0b000), 0), 2);
        // nothing alive: keep the wanted one
        assert_eq!(pick_among(3, snap(0b000, 0b000), 1), 1);
        // a weight list longer than the peers cannot index out of range
        assert_eq!(pick_among(2, snap(0b11, 0b11), 5), 1);
    }

    fn four_peers(balanced: bool) -> Failover {
        let _ = balanced;
        Failover {
            peers: (1..=4).map(|p| SocketAddr::from(([127, 0, 0, 1], p))).collect(),
            state: Arc::new(AtomicU32::new(0)),
            #[cfg(feature = "balance")]
            balanced,
        }
    }

    #[test]
    fn retry_peers() {
        let fo = four_peers(false);
        let snap = |ready: u8| Snapshot::pack(0, ready, ready);
        // by priority, never one that was tried or is not ready
        assert_eq!(fo.retry_peer(snap(0b1111), &[0], false), Some(1));
        assert_eq!(fo.retry_peer(snap(0b1111), &[0, 1], false), Some(2));
        assert_eq!(fo.retry_peer(snap(0b1001), &[3], false), Some(0));
        assert_eq!(fo.retry_peer(snap(0b1001), &[0], false), Some(3));
        assert_eq!(fo.retry_peer(snap(0b0001), &[0], false), None);
        // balanced: around the ring from the last one tried
        assert_eq!(fo.retry_peer(snap(0b1111), &[2], true), Some(3));
        assert_eq!(fo.retry_peer(snap(0b1111), &[2, 3], true), Some(0));
        assert_eq!(fo.retry_peer(snap(0b0111), &[2], true), Some(0));
        assert_eq!(fo.retry_peer(snap(0b0100), &[2], true), None);
    }

    /// What a dead peer would get is shared by the others, not handed to its neighbour.
    #[cfg(feature = "balance")]
    #[test]
    fn balanced_share_of_a_dead_peer() {
        use realm_lb::{Balancer, Strategy};
        use std::net::Ipv4Addr;

        let fo = four_peers(true);
        let snap = |up: u8| Snapshot::pack(0, up, up);
        let any: IpAddr = Ipv4Addr::new(10, 0, 0, 1).into();
        let count = |opts: &ConnectOpts, up: u8, rounds: usize| {
            let mut n = [0usize; 4];
            for _ in 0..rounds {
                n[fo.balanced_peer(opts, &any, snap(up)).unwrap()] += 1;
            }
            n
        };

        let mut opts = ConnectOpts {
            balancer: Balancer::new(Strategy::RoundRobin, &[1, 1, 1, 1]),
            ..Default::default()
        };
        assert_eq!(count(&opts, 0b1111, 400), [100, 100, 100, 100]);
        assert_eq!(count(&opts, 0b1110, 300), [0, 100, 100, 100]);
        assert_eq!(count(&opts, 0b1100, 300), [0, 0, 150, 150]);
        assert_eq!(count(&opts, 0b1000, 300), [0, 0, 0, 300]);
        // nothing alive: the balancer's own choice, as if all were
        assert_eq!(count(&opts, 0b0000, 400), [100, 100, 100, 100]);

        // weights keep their meaning among the peers that are left
        opts.balancer = Balancer::new(Strategy::RoundRobin, &[1, 3, 1, 1]);
        assert_eq!(count(&opts, 0b1110, 500), [0, 300, 100, 100]);
        assert_eq!(count(&opts, 0b1101, 300), [100, 0, 100, 100]);

        // ip hash: a client keeps its peer while that is alive, and has one fixed
        // substitute while it is not
        opts.balancer = Balancer::new(Strategy::IpHash, &[1, 1, 1, 1]);
        let clients: Vec<IpAddr> = (0..4000u32)
            .map(|i| Ipv4Addr::from(0x0a00_0000 + i * 7919).into())
            .collect();
        let on = |up: u8| -> Vec<usize> {
            clients
                .iter()
                .map(|ip| fo.balanced_peer(&opts, ip, snap(up)).unwrap())
                .collect()
        };
        let (home, one_down, two_down) = (on(0b1111), on(0b1110), on(0b1010));
        assert_eq!(one_down, on(0b1110));
        let mut moved = [0usize; 4];
        for i in 0..clients.len() {
            if home[i] != 0 {
                assert_eq!(one_down[i], home[i]);
            } else {
                assert_ne!(one_down[i], 0);
                moved[one_down[i]] += 1;
            }
            // a second peer dies: only its clients move again
            if one_down[i] != 2 {
                assert_eq!(two_down[i], one_down[i]);
            } else {
                assert!(two_down[i] == 1 || two_down[i] == 3);
            }
        }
        let total: usize = moved.iter().sum();
        assert!(total > 400, "peer 0 had {} of 4000 clients", total);
        for n in &moved[1..] {
            assert!(n * 5 > total, "clients of the dead peer went to {:?}", moved);
        }

        // ip hash: weights keep their meaning too, among all peers and among
        // those that are left
        opts.balancer = Balancer::new(Strategy::IpHash, &[1, 2, 3, 2]);
        let clients: Vec<IpAddr> = (0..40_000u32)
            .map(|i| Ipv4Addr::from(0x0a00_0000 + i * 7919).into())
            .collect();
        let shares = |opts: &ConnectOpts, up: u8, want: [f64; 4]| {
            let mut n = [0usize; 4];
            for ip in &clients {
                n[fo.balanced_peer(opts, ip, snap(up)).unwrap()] += 1;
            }
            for (n, want) in n.iter().zip(want) {
                let got = *n as f64 / clients.len() as f64;
                assert!(
                    (got - want).abs() < 0.01,
                    "up {:#06b}: {:?} of {}",
                    up,
                    n,
                    clients.len()
                );
            }
        };
        shares(&opts, 0b1111, [1.0 / 8.0, 2.0 / 8.0, 3.0 / 8.0, 2.0 / 8.0]);
        shares(&opts, 0b0111, [1.0 / 6.0, 2.0 / 6.0, 3.0 / 6.0, 0.0]);
        shares(&opts, 0b1011, [1.0 / 5.0, 2.0 / 5.0, 0.0, 2.0 / 5.0]);
        shares(&opts, 0b1001, [1.0 / 3.0, 0.0, 0.0, 2.0 / 3.0]);
        // peers that are alive but failed their latest probe only take over when
        // no ready peer is left
        let ready = |up: u8, ready: u8| -> Vec<usize> {
            let snap = Snapshot::pack(0, up, ready);
            let mut n = vec![0usize; 4];
            for ip in &clients {
                n[fo.balanced_peer(&opts, ip, snap).unwrap()] += 1;
            }
            n
        };
        let n = ready(0b0111, 0b0011);
        assert!(n[0] > 0 && n[1] > 0 && n[3] == 0);
        assert!(n[2] > clients.len() / 4, "clients of an alive peer stay: {:?}", n);
        let n = ready(0b0110, 0b0000);
        assert!(n[0] == 0 && n[1] > 0 && n[2] > 0 && n[3] == 0);

        // however large the share of the dead peer was
        opts.balancer = Balancer::new(Strategy::IpHash, &[80, 1, 1, 2]);
        shares(&opts, 0b1110, [0.0, 0.25, 0.25, 0.5]);
    }

    #[test]
    fn snapshot_layout() {
        let s = Snapshot::pack(5, 0b1010_0001, 0b0000_0001);
        assert_eq!(s.active(), 5);
        assert!(s.is_up(0) && !s.is_up(1) && s.is_up(5) && s.is_up(7));
        assert!(s.is_ready(0) && !s.is_ready(5));
    }

    #[test]
    fn start_optimistic_but_not_ready() {
        let fsm = Fsm::new(2, &opts());
        let s = fsm.snapshot();
        assert_eq!(s.active(), 0);
        assert!(s.is_up(0) && s.is_up(1));
        assert!(!s.is_ready(0) && !s.is_ready(1));
    }

    #[test]
    fn isolated_failures_never_switch() {
        let mut fsm = Fsm::new(2, &opts());
        let mut t = Instant::now();
        run(
            &mut fsm,
            &mut t,
            &[[F, O], [O, O], [O, O], [F, O], [O, O], [F, O], [O, O], [O, O]],
        );
        assert!(fsm.snapshot().is_up(0));
        assert_eq!(fsm.snapshot().active(), 0);
    }

    #[test]
    fn successes_in_between_do_not_forgive() {
        // a flapping peer: never 3 failures in a row, still 3 of the last 5
        let mut fsm = Fsm::new(2, &opts());
        let mut t = Instant::now();
        run(&mut fsm, &mut t, &[[F, O], [F, O], [O, O], [O, O]]);
        assert!(fsm.snapshot().is_up(0));
        run(&mut fsm, &mut t, &[[F, O]]);
        assert!(!fsm.snapshot().is_up(0));
        assert_eq!(fsm.snapshot().active(), 1);
    }

    #[test]
    fn failures_age_out_of_the_window() {
        let mut fsm = Fsm::new(2, &opts());
        let mut t = Instant::now();
        // F F O O O: the next failure sees only F O O O F
        run(
            &mut fsm,
            &mut t,
            &[[F, O], [F, O], [O, O], [O, O], [O, O], [F, O], [O, O]],
        );
        assert!(fsm.snapshot().is_up(0));
        assert_eq!(fsm.snapshot().active(), 0);
    }

    #[test]
    fn rise_starts_with_a_clean_record() {
        let mut fsm = Fsm::new(2, &FailoverOpts { rise: 2, ..opts() });
        let mut t = Instant::now();
        run(&mut fsm, &mut t, &[[F, O], [F, O], [F, O]]);
        assert!(!fsm.snapshot().is_up(0));
        // back after 2 successes; the 3 old failures are still inside the window
        run(&mut fsm, &mut t, &[[O, O], [O, O]]);
        assert!(fsm.snapshot().is_up(0));
        run(&mut fsm, &mut t, &[[F, O], [F, O]]);
        assert!(fsm.snapshot().is_up(0));
        run(&mut fsm, &mut t, &[[F, O]]);
        assert!(!fsm.snapshot().is_up(0));
    }

    #[test]
    fn switch_needs_fresh_backup() {
        let mut fsm = Fsm::new(2, &opts());
        let mut t = Instant::now();
        // backup never answered: primary goes down but traffic stays
        run(&mut fsm, &mut t, &[[F, F], [F, U], [F, F]]);
        let s = fsm.snapshot();
        assert!(!s.is_up(0));
        assert_eq!(s.active(), 0);
        // backup answers once: move now
        run(&mut fsm, &mut t, &[[F, O]]);
        assert_eq!(fsm.snapshot().active(), 1);
    }

    #[test]
    fn unknown_changes_nothing() {
        let mut fsm = Fsm::new(2, &opts());
        let mut t = Instant::now();
        run(&mut fsm, &mut t, &[[O, O], [F, O], [U, O], [U, O], [U, O], [F, O]]);
        assert!(fsm.snapshot().is_up(0));
        run(&mut fsm, &mut t, &[[U, O], [F, O]]);
        assert!(!fsm.snapshot().is_up(0));
    }

    #[test]
    fn failback_needs_rise_and_hold() {
        let mut fsm = Fsm::new(2, &opts());
        let mut t = Instant::now();
        run(&mut fsm, &mut t, &[[O, O], [F, O], [F, O], [F, O]]);
        assert_eq!(fsm.snapshot().active(), 1);

        // 4 successes: still down
        run(&mut fsm, &mut t, &[[O, O]; 4]);
        assert!(!fsm.snapshot().is_up(0));
        // 5th success: up, but hold (30s) not elapsed
        run(&mut fsm, &mut t, &[[O, O]]);
        assert!(fsm.snapshot().is_up(0));
        assert_eq!(fsm.snapshot().active(), 1);

        run(&mut fsm, &mut t, &[[O, O]; 24]);
        assert_eq!(fsm.snapshot().active(), 1);
        run(&mut fsm, &mut t, &[[O, O]]);
        assert_eq!(fsm.snapshot().active(), 0);
    }

    #[test]
    fn current_down_ignores_hold() {
        let mut fsm = Fsm::new(2, &opts());
        let mut t = Instant::now();
        run(&mut fsm, &mut t, &[[O, O], [F, O], [F, O], [F, O]]);
        assert_eq!(fsm.snapshot().active(), 1);
        // primary back, then backup dies right away: go back without waiting for hold
        run(&mut fsm, &mut t, &[[O, F]; 5]);
        assert_eq!(fsm.snapshot().active(), 0);
    }

    #[test]
    fn all_down_keeps_current() {
        let mut fsm = Fsm::new(2, &opts());
        let mut t = Instant::now();
        run(&mut fsm, &mut t, &[[O, O], [F, F], [F, F], [F, F], [F, F]]);
        let s = fsm.snapshot();
        assert!(!s.is_up(0) && !s.is_up(1));
        assert_eq!(s.active(), 0);
        // the first peer to come back (rise) takes traffic
        run(&mut fsm, &mut t, &[[F, O]; 5]);
        assert_eq!(fsm.snapshot().active(), 1);
    }

    fn probe_opts(timeout: u64) -> FailoverOpts {
        FailoverOpts {
            tcp_send: b"ping".to_vec(),
            tcp_expect: b"OK".to_vec(),
            timeout: Duration::from_millis(timeout),
            ..Default::default()
        }
    }

    /// Hold the first `silent` connections without answering, answer the rest.
    async fn probe_server(silent: u32) -> (SocketAddr, Arc<AtomicU32>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let lis = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = lis.local_addr().unwrap();
        let accepted = Arc::new(AtomicU32::new(0));
        let count = accepted.clone();
        tokio::spawn(async move {
            let mut held = Vec::new();
            loop {
                let (mut s, _) = lis.accept().await.unwrap();
                if count.fetch_add(1, Relaxed) < silent {
                    held.push(s);
                    continue;
                }
                tokio::spawn(async move {
                    let mut buf = [0u8; 4];
                    if s.read_exact(&mut buf).await.is_ok() {
                        let _ = s.write_all(b"OK").await;
                    }
                });
            }
        });
        (addr, accepted)
    }

    #[tokio::test]
    async fn tcp_probe_retries_only_while_silent() {
        let conn_opts = ConnectOpts::default();

        // answered at once: one connection
        let (addr, accepted) = probe_server(0).await;
        assert_eq!(probe_tcp(addr, &probe_opts(900), &conn_opts).await, O);
        assert_eq!(accepted.load(Relaxed), 1);

        // the first connection stays silent: the second one, started at a third of the timeout, answers
        let (addr, accepted) = probe_server(1).await;
        let t = Instant::now();
        assert_eq!(probe_tcp(addr, &probe_opts(900), &conn_opts).await, O);
        assert!(t.elapsed() >= Duration::from_millis(290));
        assert_eq!(accepted.load(Relaxed), 2);

        // nobody answers: all tries are used and the round fails at the timeout
        let (addr, accepted) = probe_server(u32::MAX).await;
        let t = Instant::now();
        assert_eq!(probe_tcp(addr, &probe_opts(900), &conn_opts).await, F);
        assert!(t.elapsed() >= Duration::from_millis(890));
        assert_eq!(accepted.load(Relaxed), TRIES);
    }

    #[tokio::test]
    async fn tcp_probe_refusal_is_an_answer() {
        let lis = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = lis.local_addr().unwrap();
        drop(lis);
        let t = Instant::now();
        assert_eq!(probe_tcp(addr, &probe_opts(6000), &ConnectOpts::default()).await, F);
        // no waiting for further tries (windows itself retries a refused connect for ~2s)
        assert!(t.elapsed() < Duration::from_millis(4000));
    }

    #[test]
    fn repick_and_stale() {
        let fo = Failover {
            peers: vec!["127.0.0.1:1".parse().unwrap(), "127.0.0.1:2".parse().unwrap()].into(),
            state: Arc::new(AtomicU32::new(0)),
            #[cfg(feature = "balance")]
            balanced: false,
        };
        let both_up = Snapshot::pack(0, 0b11, 0b11);
        let primary_down = Snapshot::pack(1, 0b10, 0b10);
        // sticky while up, move only off a down peer
        assert_eq!(Failover::repick(both_up, 1), 1);
        assert_eq!(Failover::repick(primary_down, 0), 1);
        // every peer down: an association on the non-active peer does not move
        let all_down = Snapshot::pack(0, 0b00, 0b00);
        assert_eq!(Failover::repick(all_down, 1), 1);
        assert_eq!(Failover::repick(all_down, 0), 0);
        assert_eq!(fo.retry_peer(both_up, &[0], false), Some(1));
        assert_eq!(fo.retry_peer(primary_down, &[1], false), None);

        let a: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let b: SocketAddr = "127.0.0.1:2".parse().unwrap();
        let other: SocketAddr = "127.0.0.1:3".parse().unwrap();
        assert!(fo.is_stale(&a, 1));
        assert!(!fo.is_stale(&b, 1));
        assert!(!fo.is_stale(&other, 1));
    }
}
