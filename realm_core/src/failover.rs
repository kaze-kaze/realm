//! Active-standby failover.
//!
//! `remote` is the primary peer and `extra_remotes` are backups in priority order.
//!
//! The control plane is one probe task per endpoint and protocol. It publishes a
//! packed [`Snapshot`] into a single atomic. The data plane only loads that atomic
//! when a tcp connection is dialed or a udp batch is dispatched, so relaying itself
//! (splice, batched udp) is untouched.
//!
//! Decisions are made on probe results only, never on user traffic:
//! - a peer goes down after `fall` failures in the last `window` probes;
//!   two consecutive successes forgive earlier failures;
//! - a down peer comes back after `rise` consecutive successes;
//! - traffic only moves to a peer whose latest probe succeeded;
//! - failing back to a higher priority peer waits `hold` after the last switch,
//!   unless the current peer is down;
//! - when no peer is usable, the current choice is kept.

use std::fmt::{Display, Formatter};
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::future::join_all;
use tokio::time::{Instant, MissedTickBehavior, interval, sleep_until, timeout_at};

use crate::endpoint::{ConnectOpts, RemoteAddr};

/// Max peers, `remote` included. Bounded by the packed snapshot.
pub const MAX_PEERS: usize = 8;

/// Consecutive successes that forgive earlier failures.
const CLEAR_AFTER: u8 = 2;

/// Udp probe sends at most this many datagrams per round.
const UDP_TRIES: u32 = 3;

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
        )
    }
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
}

/// Runtime failover state of one endpoint and protocol.
pub struct Failover {
    peers: Box<[SocketAddr]>,
    state: Arc<AtomicU32>,
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
                conn_opts: conn_opts.clone(),
                state: state.clone(),
            },
            fsm,
        ));

        Self { peers, state }
    }

    #[inline]
    pub fn snapshot(&self) -> Snapshot {
        Snapshot(self.state.load(Relaxed))
    }

    #[inline]
    pub fn peer(&self, i: usize) -> SocketAddr {
        self.peers[i]
    }

    /// First ready peer other than `skip`, by priority.
    #[inline]
    pub fn fallback(&self, snap: Snapshot, skip: usize) -> Option<usize> {
        (0..self.peers.len()).find(|&i| i != skip && snap.is_ready(i))
    }

    /// Peer an existing udp association should use: stay unless its peer is down.
    #[inline]
    pub const fn repick(snap: Snapshot, cur: usize) -> usize {
        if snap.is_up(cur) {
            cur
        } else {
            snap.active()
        }
    }

    /// `src` is a peer other than `cur`, i.e. a late reply after the association moved.
    #[inline]
    pub fn is_stale(&self, src: &SocketAddr, cur: usize) -> bool {
        self.peers.iter().enumerate().any(|(i, p)| i != cur && p == src)
    }
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
                    if p.streak >= CLEAR_AFTER {
                        p.fails = 0;
                    }
                    if !p.up && p.streak >= self.rise {
                        p.up = true;
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
            if before.active() != after.active() {
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
async fn probe_tcp(target: SocketAddr, opts: &FailoverOpts, conn_opts: &ConnectOpts) -> Probe {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let deadline = Instant::now() + opts.timeout;
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
            match stream.read(&mut buf[n..]).await? {
                0 => break,
                k => n += k,
            }
        }
        Ok::<_, Error>(&buf[..n] == want)
    };

    match timeout_at(deadline, run).await {
        Ok(Ok(true)) => Probe::Ok,
        Ok(Ok(false)) => {
            log::debug!("[failover][tcp]probe {}: unexpected reply", target);
            Probe::Fail
        }
        Ok(Err(e)) => {
            log::debug!("[failover][tcp]probe {}: {}", target, e);
            Probe::Fail
        }
        Err(_) => {
            log::debug!("[failover][tcp]probe {}: timeout", target);
            Probe::Fail
        }
    }
}

/// Send `udp_send + nonce` and require it echoed back. Resend only while unanswered,
/// at most [`UDP_TRIES`] datagrams within `timeout`.
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
        return Probe::Fail;
    }

    let mut msg = Vec::with_capacity(opts.udp_send.len() + 16);
    msg.extend_from_slice(&opts.udp_send);
    msg.extend_from_slice(&nonce());
    // one extra byte so that a longer datagram never matches
    let mut buf = vec![0u8; msg.len() + 1];

    let start = Instant::now();
    for k in 1..=UDP_TRIES {
        let _ = sock.send(&msg).await;
        let until = start + opts.timeout * k / UDP_TRIES;
        loop {
            match timeout_at(until, sock.recv(&mut buf)).await {
                Ok(Ok(n)) if buf[..n] == msg[..] => return Probe::Ok,
                // stale or foreign datagram
                Ok(Ok(_)) => continue,
                // e.g. icmp unreachable, wait for the next try
                Ok(Err(e)) => {
                    log::debug!("[failover][udp]probe {}: {}", target, e);
                    sleep_until(until).await;
                    break;
                }
                Err(_) => break,
            }
        }
    }
    log::debug!("[failover][udp]probe {}: no echo", target);
    Probe::Fail
}

/// Unique per probe round within this process.
fn nonce() -> [u8; 16] {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    let mut b = [0u8; 16];
    b[..8].copy_from_slice(&now.to_le_bytes());
    b[8..].copy_from_slice(&seq.to_le_bytes());
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
    fn two_successes_forgive_earlier_failures() {
        // pure 3-of-5 would trip on F F O O F
        let mut fsm = Fsm::new(2, &opts());
        let mut t = Instant::now();
        run(&mut fsm, &mut t, &[[F, O], [F, O], [O, O], [O, O], [F, O]]);
        assert_eq!(fsm.snapshot().active(), 0);
        // without a double success in between it trips
        run(&mut fsm, &mut t, &[[O, O], [F, O], [F, O]]);
        assert!(!fsm.snapshot().is_up(0));
        assert_eq!(fsm.snapshot().active(), 1);
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

    #[test]
    fn repick_and_stale() {
        let fo = Failover {
            peers: vec!["127.0.0.1:1".parse().unwrap(), "127.0.0.1:2".parse().unwrap()].into(),
            state: Arc::new(AtomicU32::new(0)),
        };
        let both_up = Snapshot::pack(0, 0b11, 0b11);
        let primary_down = Snapshot::pack(1, 0b10, 0b10);
        // sticky while up, move only off a down peer
        assert_eq!(Failover::repick(both_up, 1), 1);
        assert_eq!(Failover::repick(primary_down, 0), 1);
        assert_eq!(fo.fallback(both_up, 0), Some(1));
        assert_eq!(fo.fallback(primary_down, 1), None);

        let a: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let b: SocketAddr = "127.0.0.1:2".parse().unwrap();
        let other: SocketAddr = "127.0.0.1:3".parse().unwrap();
        assert!(fo.is_stale(&a, 1));
        assert!(!fo.is_stale(&b, 1));
        assert!(!fo.is_stale(&other, 1));
    }
}
