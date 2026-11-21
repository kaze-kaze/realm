use std::net::{IpAddr, Ipv4Addr};

use super::{Balance, Token};

/// Iphash balancer.
///
/// A client address always gets the same peer, and peers get clients in
/// proportion to their weights.
///
/// This is weighted rendezvous hashing. Every peer draws a number from the
/// address and its own index, and the best draw wins. The draw of a peer of
/// weight `w` is exponentially distributed with rate `w`, so the peer wins with
/// probability `w / sum of weights`: the shares are exact, not an approximation
/// that depends on how many points a ring has.
///
/// The draw of a peer depends on nothing but the address, its index and its
/// weight. So appending a peer, or changing the weight of one, only moves
/// clients to or from that peer.
///
/// The mapping from addresses to peers is part of the behaviour: changing how
/// it is computed moves the clients of every user on upgrade. See the
/// `ih_mapping_is_pinned` test.
#[derive(Debug)]
pub struct IpHash {
    /// `1 / weight` of each peer, infinite for a weight of zero.
    scale: Vec<f64>,
    total: u8,
}

impl Balance for IpHash {
    type State = IpAddr;

    fn total(&self) -> u8 {
        self.total
    }

    fn new(weights: &[u8]) -> Self {
        assert!(weights.len() <= u8::MAX as usize);

        Self {
            scale: weights.iter().map(|w| 1.0 / *w as f64).collect(),
            total: weights.len() as u8,
        }
    }

    fn next(&self, state: &Self::State) -> Option<Token> {
        if self.total <= 1 {
            return Some(Token(0));
        }
        self.next_among(state, |_| true)
    }
}

impl IpHash {
    /// The peer of `ip` among those that `usable` accepts, `None` if there is none.
    ///
    /// It is what [`Balance::next`] gives while that peer is usable. Otherwise it
    /// is the peer that comes next for this address, always the same one. So with
    /// some peers out of use, their clients are shared by the others in the
    /// proportions of their weights, and the clients of the others do not move.
    pub fn next_among(&self, ip: &IpAddr, usable: impl Fn(usize) -> bool) -> Option<Token> {
        let seed = seed(ip);

        // A peer of weight zero draws infinity: it only wins when the other usable
        // peers have no weight either, and then the first of them does.
        let mut best: Option<(usize, f64)> = None;
        for (i, scale) in self.scale.iter().enumerate().filter(|(i, _)| usable(*i)) {
            let draw = exponential(stream(seed, i as u64)) * scale;
            if best.is_none_or(|(_, best)| draw < best) {
                best = Some((i, draw));
            }
        }

        best.map(|(i, _)| Token(i as u8))
    }
}

/// One number per address. An ipv4 client is the same client whether it shows
/// up as `a.b.c.d` or, on a dual-stack listener, as `::ffff:a.b.c.d`.
fn seed(ip: &IpAddr) -> u64 {
    let v4 = |x: Ipv4Addr| mix(u32::from(x) as u64);
    match ip {
        IpAddr::V4(x) => v4(*x),
        IpAddr::V6(x) => match x.to_ipv4_mapped() {
            Some(x) => v4(x),
            None => {
                let x = u128::from(*x);
                mix(mix((x >> 64) as u64) ^ x as u64)
            }
        },
    }
}

/// The `i`-th number of the splitmix64 sequence that starts at `seed`: unrelated
/// numbers for the peers of one address, and for one peer across addresses.
#[inline]
fn stream(seed: u64, i: u64) -> u64 {
    const GOLDEN: u64 = 0x9e37_79b9_7f4a_7c15;
    mix(seed.wrapping_add((i + 1).wrapping_mul(GOLDEN)))
}

/// Finalizer of splitmix64 (Stafford's mix13): every input bit flips every
/// output bit about half of the time, so addresses that differ in one bit, as
/// neighbours in a subnet do, end up unrelated.
#[inline]
fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// Turn 64 random bits into an exponentially distributed number of rate 1.
#[inline]
fn exponential(bits: u64) -> f64 {
    // the middle of one of 2^52 equal steps of (0, 1): never 0 or 1, and exact
    // in a f64, so the logarithm is finite and not zero
    let uniform = ((bits >> 12) as f64 + 0.5) * (1.0 / (1u64 << 52) as f64);
    -uniform.ln()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;

    fn v4(x: u32) -> IpAddr {
        Ipv4Addr::from(x).into()
    }

    /// Addresses all over the ipv4 space, about 430 thousand of them.
    fn scattered() -> impl Iterator<Item = IpAddr> {
        (0..=u32::MAX).step_by(9973).map(v4)
    }

    fn pick(iphash: &IpHash, ip: &IpAddr) -> usize {
        iphash.next(ip).unwrap().0 as usize
    }

    /// Share of each peer in percent, and the number of addresses.
    fn shares(weights: &[u8], ips: impl Iterator<Item = IpAddr>) -> (Vec<f64>, usize) {
        let iphash = IpHash::new(weights);
        let mut count = vec![0usize; weights.len()];
        let mut total = 0;
        for ip in ips {
            count[pick(&iphash, &ip)] += 1;
            total += 1;
        }
        let shares = count.iter().map(|n| *n as f64 * 100.0 / total as f64).collect();
        (shares, total)
    }

    /// Every peer gets its weight's share of the addresses, give or take what
    /// that many random picks would be off by (five standard deviations).
    fn assert_shares(weights: &[u8], ips: impl Iterator<Item = IpAddr>) {
        let (shares, total) = shares(weights, ips);
        let sum: f64 = weights.iter().map(|w| *w as f64).sum();
        println!("{:?} over {} addresses: {:.2?}", weights, total, shares);
        for (i, (share, weight)) in shares.iter().zip(weights).enumerate() {
            let p = *weight as f64 / sum;
            let sigma = (p * (1.0 - p) / total as f64).sqrt() * 100.0;
            let off = (share - p * 100.0).abs();
            assert!(
                off <= 5.0 * sigma,
                "weights {:?}: peer {} got {:.2}% of {} addresses, expected {:.2}% within {:.2}",
                weights,
                i,
                share,
                total,
                p * 100.0,
                5.0 * sigma
            );
        }
    }

    #[test]
    fn ih_shares_follow_weights() {
        assert_shares(&[1, 1], scattered());
        assert_shares(&[1, 1, 1], scattered());
        assert_shares(&[1, 1, 1, 1], scattered());
        assert_shares(&[2, 1, 1], scattered());
        assert_shares(&[4, 2, 1], scattered());
        assert_shares(&[1, 3, 1, 1], scattered());
        assert_shares(&[80, 1, 1], scattered());
        assert_shares(&[255, 1], scattered());
        assert_shares(&[1; 8], scattered());
        assert_shares(&(1..=16).collect::<Vec<u8>>(), scattered());
        assert_shares(&[1; 255], scattered());
    }

    /// Clients are not spread all over the address space: they sit next to each
    /// other in a few networks.
    #[test]
    fn ih_neighbours_are_spread_too() {
        let subnet = |base: [u8; 4], bits: u32| {
            let base = u32::from(Ipv4Addr::from(base));
            (0..1u32 << (32 - bits)).map(move |i| v4(base + i))
        };
        for weights in [&[1, 1, 1][..], &[2, 1, 1], &[4, 2, 1]] {
            assert_shares(weights, subnet([10, 0, 0, 0], 14));
            assert_shares(weights, subnet([100, 64, 0, 0], 16));
            assert_shares(weights, subnet([192, 168, 0, 0], 16));
            assert_shares(weights, subnet([203, 0, 112, 0], 20));
            // the same host number in every /24 of a /8
            assert_shares(weights, (0..1u32 << 16).map(|i| v4(0x0b00_0001 + (i << 8))));
        }
    }

    #[test]
    fn ih_ipv6_shares_follow_weights() {
        let net = u128::from("2001:db8::".parse::<Ipv6Addr>().unwrap());
        // hosts of one /64, then the first host of many /64
        let hosts = || (1..=200_000u128).map(move |i| IpAddr::from(Ipv6Addr::from(net + i)));
        let nets = || (1..=200_000u128).map(move |i| IpAddr::from(Ipv6Addr::from(net + (i << 64) + 1)));
        for weights in [&[1, 1, 1][..], &[2, 1, 1], &[4, 2, 1]] {
            assert_shares(weights, hosts());
            assert_shares(weights, nets());
        }
    }

    #[test]
    fn ih_same_ip_same_peer() {
        let iphash = IpHash::new(&[1, 2, 3, 4]);
        assert_eq!(iphash.total(), 4);
        for ip in ["1.1.1.1", "8.8.8.8", "114.51.4.19", "2001:4860:4860::8888"] {
            let ip = ip.parse::<IpAddr>().unwrap();
            let peer = iphash.next(&ip);
            for _ in 0..16 {
                assert_eq!(iphash.next(&ip), peer);
            }
        }
        // a balancer built again from the same weights agrees
        let again = IpHash::new(&[1, 2, 3, 4]);
        for ip in scattered().take(10_000) {
            assert_eq!(iphash.next(&ip), again.next(&ip));
        }
    }

    #[test]
    fn ih_ipv4_mapped_is_ipv4() {
        let iphash = IpHash::new(&[1, 1, 1, 1, 1]);
        for ip in scattered().take(10_000) {
            let IpAddr::V4(x) = ip else { unreachable!() };
            let mapped = IpAddr::V6(x.to_ipv6_mapped());
            assert_eq!(iphash.next(&ip), iphash.next(&mapped));
        }
    }

    #[test]
    fn ih_single_or_no_peer() {
        let ip = v4(0x0102_0304);
        assert_eq!(IpHash::new(&[]).next(&ip), Some(Token(0)));
        assert_eq!(IpHash::new(&[7]).next(&ip), Some(Token(0)));
    }

    #[test]
    fn ih_zero_weight_gets_nothing() {
        let (shares, _) = shares(&[0, 1, 1], scattered());
        assert_eq!(shares[0], 0.0);
        assert_shares(&[0, 2, 1], scattered());
        assert_shares(&[3, 0, 1, 0], scattered());
        // nothing to choose from: the first peer
        let (shares, _) = shares_of_all_zero();
        assert_eq!(shares, vec![100.0, 0.0, 0.0]);
    }

    fn shares_of_all_zero() -> (Vec<f64>, usize) {
        shares(&[0, 0, 0], scattered().take(10_000))
    }

    /// One more peer takes its share from the others and moves nobody else.
    #[test]
    fn ih_new_peer_moves_only_its_clients() {
        let before = IpHash::new(&[2, 1, 1]);
        let after = IpHash::new(&[2, 1, 1, 2]);
        let mut moved = 0;
        for ip in scattered() {
            let (old, new) = (pick(&before, &ip), pick(&after, &ip));
            if new != old {
                assert_eq!(new, 3);
                moved += 1;
            }
        }
        // a third of the clients: its weight out of the new total
        let total = scattered().count();
        let share = moved as f64 / total as f64;
        assert!((share - 1.0 / 3.0).abs() < 0.005, "moved {} of {}", moved, total);
    }

    /// A different weight for one peer only moves clients to or from that peer.
    #[test]
    fn ih_new_weight_moves_only_its_clients() {
        let before = IpHash::new(&[1, 1, 1]);
        let more = IpHash::new(&[1, 3, 1]);
        let less = IpHash::new(&[3, 1, 3]);
        for ip in scattered() {
            let old = pick(&before, &ip);
            let new = pick(&more, &ip);
            assert!(new == old || new == 1);
            // `3, 1, 3` is `1, 1/3, 1`: peer 1 loses clients and gains none
            let new = pick(&less, &ip);
            assert!(new == old || old == 1);
        }
    }

    /// With a peer out of use, its clients go to the others by weight, however
    /// large its own share was, and nobody else moves.
    #[test]
    fn ih_next_among_shares_follow_weights() {
        let among = |weights: &[u8], out: &[usize], want: &[f64]| {
            let iphash = IpHash::new(weights);
            let mut count = vec![0usize; weights.len()];
            let mut total = 0;
            for ip in scattered() {
                let home = pick(&iphash, &ip);
                let now = iphash.next_among(&ip, |i| !out.contains(&i)).unwrap().0 as usize;
                assert!(!out.contains(&now));
                assert!(now == home || out.contains(&home));
                count[now] += 1;
                total += 1;
            }
            for (i, (n, want)) in count.iter().zip(want).enumerate() {
                let got = *n as f64 / total as f64;
                let sigma = (want * (1.0 - want) / total as f64).sqrt();
                assert!(
                    (got - want).abs() <= 5.0 * sigma,
                    "weights {:?} without {:?}: peer {} got {:.4}, expected {:.4}",
                    weights,
                    out,
                    i,
                    got,
                    want
                );
            }
        };
        among(&[1, 1, 1], &[], &[1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]);
        among(&[1, 1, 1], &[1], &[0.5, 0.0, 0.5]);
        among(&[2, 1, 1], &[0], &[0.0, 0.5, 0.5]);
        among(&[4, 2, 1], &[1], &[0.8, 0.0, 0.2]);
        among(&[80, 1, 1], &[0], &[0.0, 0.5, 0.5]);
        among(&[1, 2, 3, 2], &[2, 3], &[1.0 / 3.0, 2.0 / 3.0, 0.0, 0.0]);

        let iphash = IpHash::new(&[1, 2, 3]);
        let ip = v4(0x0102_0304);
        assert_eq!(iphash.next_among(&ip, |_| false), None);
        assert_eq!(iphash.next_among(&ip, |i| i == 2), Some(Token(2)));
        // weightless peers are the last resort, in order
        let iphash = IpHash::new(&[0, 0, 1]);
        assert_eq!(iphash.next_among(&ip, |_| true), Some(Token(2)));
        assert_eq!(iphash.next_among(&ip, |i| i != 2), Some(Token(0)));
    }

    /// The mapping must not change by accident: a change moves the clients of
    /// every `iphash` endpoint on upgrade, and that belongs in the release notes.
    #[test]
    fn ih_mapping_is_pinned() {
        let peers = |weights: &[u8]| -> Vec<usize> {
            let iphash = IpHash::new(weights);
            [
                "1.1.1.1",
                "8.8.8.8",
                "10.0.0.1",
                "10.0.0.2",
                "100.64.7.9",
                "192.168.1.1",
                "198.51.100.23",
                "203.0.113.77",
                "::1",
                "2001:db8::1",
                "2001:db8::2",
                "2001:db8:1::1",
                "fd9d:bb35:94bf:c38a:ee1:c75d:8df3:c909",
                "::ffff:10.0.0.1",
            ]
            .iter()
            .map(|ip| pick(&iphash, &ip.parse().unwrap()))
            .collect()
        };
        assert_eq!(peers(&[1, 1, 1]), PINNED_EQUAL);
        assert_eq!(peers(&[4, 2, 1]), PINNED_WEIGHTED);
        assert_eq!(peers(&[1, 1, 1, 1, 1, 1, 1, 1]), PINNED_EIGHT);
    }

    const PINNED_EQUAL: [usize; 14] = [1, 1, 1, 2, 0, 0, 0, 0, 0, 2, 2, 1, 2, 1];
    const PINNED_WEIGHTED: [usize; 14] = [0, 1, 0, 2, 0, 0, 0, 0, 0, 0, 0, 1, 2, 0];
    const PINNED_EIGHT: [usize; 14] = [7, 1, 6, 2, 7, 0, 4, 0, 3, 6, 5, 1, 2, 6];
}
