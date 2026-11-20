# Realm Load Balance

[![crates.io](https://img.shields.io/crates/v/realm_lb.svg)](https://crates.io/crates/realm_lb)
[![Released API docs](https://docs.rs/realm_lb/badge.svg)](https://docs.rs/realm_lb)

Realm's load balance strategies.

Current implemented:

- IP Hash: weighted rendezvous hashing. A client address always gets the same peer, peers get addresses in the proportions of their weights, and adding a peer or changing a weight only moves clients to or from that peer.
- Round Robin: smooth weighted round robin.
