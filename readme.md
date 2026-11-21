# Realm

A simple, high performance relay server written in rust.

<p><img src="https://raw.githubusercontent.com/zhboner/realm/master/assets/realm.png"/></p>

[![ci](https://github.com/kaze-kaze/realm/actions/workflows/ci.yml/badge.svg)](https://github.com/kaze-kaze/realm/actions/workflows/ci.yml)
[![build](https://github.com/kaze-kaze/realm/actions/workflows/build.yml/badge.svg)](https://github.com/kaze-kaze/realm/actions/workflows/build.yml)
[![downloads](https://img.shields.io/github/downloads/kaze-kaze/realm/total?color=green)](https://github.com/kaze-kaze/realm/releases)
<!-- [中文说明](https://zhb.me/realm) -->

## About this fork

This is a fork of [zhboner/realm](https://github.com/zhboner/realm) (v2.9.6). Its own versions start at 3.0.0, and `realm -v` tells the two apart. A config written for upstream runs unchanged, and the new options are off until they are configured. One thing moves on the upgrade: an `iphash` endpoint maps its clients to peers anew, once.

What it adds:

- [failover](#endpointfailover-table): health checked primary and backups, for TCP and UDP.
- [failover with balance](#failover-with-balance): load balancing that leaves dead peers out, and balances UDP as well.
- [udp_probe = "quic"](#endpointfailover-table): probe a QUIC service (Hysteria 2, TUIC) on its own port.
- [udp_no_fragment](#networkudp_no_fragment-bool): do not fragment the datagrams of QUIC based services.
- [iphash](#endpointbalance-string) that follows the weights.
- Fixes to what upstream does: UDP datagrams above 1500 bytes are relayed whole instead of cut off, UDP replies no longer reach the wrong client when many associations are open, the listener survives running out of file descriptors, `--tcp-keepalive-probe` is read.

Upstream's binaries and container images ignore the `failover` table without a word: the relay works, and never fails over. Check the version before relying on it.

How it was tested, and what was not: [test-report](test-report/) (in Chinese).

## Install

Download a package from the [releases](https://github.com/kaze-kaze/realm/releases) and unpack it. For containers see [below](#container).

Which Linux package:

| package | for |
| --- | --- |
| `realm-<arch>-unknown-linux-gnu` | glibc 2.38 and later: Debian 13, Ubuntu 24.04. On an older system it stops with `GLIBC_2.38 not found`. |
| `realm-<arch>-unknown-linux-gnu-glibc2.28` | glibc 2.28 and later: Debian 10 to 12, Ubuntu 20.04 and 22.04. |
| `realm-<arch>-unknown-linux-musl` | static, for Alpine and anything else. On a glibc system prefer a gnu package: it relayed 12-23% more UDP in [our comparison](test-report/REALWORLD.md) (section 8.2), TCP was the same. |
| `realm-slim-...` | the same three, built without `balance`, `proxy` and `transport`. Failover is included; `balance` is ignored with a warning at startup. |

## Libs

The crates below are upstream's. This fork's changes to `realm_core` and `realm_lb` are not published to crates.io.

| lib | doc |
| ----- | ----- |
| realm-core | [![crates.io](https://img.shields.io/crates/v/realm_core.svg)](https://crates.io/crates/realm_core) [![Released API docs](https://docs.rs/realm_core/badge.svg)](https://docs.rs/realm_core) |
| realm-io | [![crates.io](https://img.shields.io/crates/v/realm_io.svg)](https://crates.io/crates/realm_io) [![Released API docs](https://docs.rs/realm_io/badge.svg)](https://docs.rs/realm_io) |
| realm-lb | [![crates.io](https://img.shields.io/crates/v/realm_lb.svg)](https://crates.io/crates/realm_lb) [![Released API docs](https://docs.rs/realm_lb/badge.svg)](https://docs.rs/realm_lb) |
| realm-hook | [![crates.io](https://img.shields.io/crates/v/realm_hook.svg)](https://crates.io/crates/realm_hook) [![Released API docs](https://docs.rs/realm_hook/badge.svg)](https://docs.rs/realm_hook)|
| realm-syscall | [![crates.io](https://img.shields.io/crates/v/realm_syscall.svg)](https://crates.io/crates/realm_syscall) [![Released API docs](https://docs.rs/realm_syscall/badge.svg)](https://docs.rs/realm_syscall) |

## Features

- Zero configuration. Setup and run in one command.
- Concurrency. Bidirectional concurrent traffic leads to high performance.
- Low resources cost.

## Container

Realm can be run in a container with OCI (like Docker, Podman, Kubernetes, etc), see guides [here](readme.container.md).

## Build

Install rust toolchain with [rustup](https://rustup.rs/):

```shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Clone this repository:

```shell
git clone https://github.com/kaze-kaze/realm && cd realm
```

Build:

```shell
cargo build --release
```

You can also pass `target_cpu=native` to allow more possible optimizations:

```shell
RUSTFLAGS='-C target_cpu=native' cargo build --release
```

The `realm` binary will be available in `target/release`.

### Build Options

- ~~udp: enable udp relay~~ builtin.
- ~~tfo: enable tcp-fast-open~~ deprecated.
- ~~trust-dns: enable trust-dns's async dns resolver~~ builtin.
- ~~zero-copy: enable zero-copy on linux~~ builtin.
- brutal-shutdown: see [realm_io/brutal-shutdown](realm_io/README.md#about-brutal-shutdown).
- hook: see [realm_hook](realm_hook/README.md).
- proxy: enable proxy-protocol.
- balance: enable load balance.
- transport: enable ws/tls/wss.
- transport-tls-ring: use [ring](https://github.com/briansmith/ring) as rustls backend.
- transport-tls-awslc: use [aws-lc](https://github.com/aws/aws-lc-rs) as rustls backend.
- batched-udp: enable more efficient udp on linux.
- multi-thread: enable tokio's multi-threaded IO scheduler.
- mi-malloc: custom memory allocator.
- jemalloc: custom memory allocator.
- page-alloc: custom memory allocator.

Default: proxy + balance + transport + transport-tls-awslc + batched-udp + brutal-shutdown + multi-thread.

Deafult-Slim: batched-udp + brutal-shutdown + multi-thread.

See also: [Cargo.toml](Cargo.toml).

Examples:

```shell
# simple tcp
cargo build --release --no-default-features

# enable other options
cargo build --release --features 'jemalloc'

# fully customized
cargo build --release \
    --no-default-features \
    --features 'transport, multi-thread, jemalloc'

# (since v2.9) use ring as rustls backend
cargo build --release \
    --no-default-features \
    --features 'multi-thread, brutal-shutdown' \
    --features 'proxy, balance, batched-udp' \
    --features 'transport, transport-tls-ring'
# equals
cargo build --release --no-default-features --features default-ring
```

### Cross Compile

Please refer to [https://rust-lang.github.io/rustup/cross-compilation.html](https://rust-lang.github.io/rustup/cross-compilation.html). You may need to install cross-compilers or other SDKs, and specify them when building the project.

Or have a look at [Cross](https://github.com/cross-rs/cross), it makes things easier.

## Usage

```shell
A high efficiency relay tool

Usage: realm [FLAGS] [OPTIONS]

Commands:
  convert  convert your legacy configuration into an advanced one

FLAGS:
  -h, --help     show help
  -v, --version  show version
  -d, --daemon   run as a unix daemon
  -u, --udp      force enable udp forward
  -m, --mtcp     force enable mptcp protocol
  -t, --ntcp     force disable tcp forward
  -6, --ipv6     force disable ipv6 mapped ipv4
  -f, --tfo      force enable tcp fast open -- deprecated
  -z, --splice   force enable tcp zero copy -- deprecated

OPTIONS:
  -c, --config <path>               use config file
  -l, --listen <address>            listen address
  -r, --remote <address>            remote address
  -x, --through <address>           send through ip or address
  -i, --interface <device>          send through interface
  -e, --listen-interface <device>   listen interface
  -a, --listen-transport <options>  listen transport
  -b, --remote-transport <options>  remote transport

SYS OPTIONS:
  -n, --nofile <limit>        set nofile limit
  -p, --pipe-page <number>    set pipe capacity
  -j, --pre-conn-hook <path>  set pre-connect hook

LOG OPTIONS:
      --log-level <level>  override log level
      --log-output <path>  override log output

DNS OPTIONS:
      --dns-mode <mode>          override dns mode
      --dns-min-ttl <second>     override dns min ttl
      --dns-max-ttl <second>     override dns max ttl
      --dns-cache-size <number>  override dns cache size
      --dns-protocol <protocol>  override dns protocol
      --dns-servers <servers>    override dns servers

PROXY OPTIONS:
      --send-proxy <send_proxy>        send proxy protocol header
      --send-proxy-version <version>   send proxy protocol version
      --accept-proxy <accept_proxy>    accept proxy protocol header
      --accept-proxy-timeout <second>  accept proxy protocol timeout

TIMEOUT OPTIONS:
      --tcp-timeout <second>         override tcp timeout(5s)
      --udp-timeout <second>         override udp timeout(30s)
      --tcp-keepalive <second>       override default tcp keepalive interval(15s)
      --tcp-keepalive-probe <count>  override default tcp keepalive count(3)
```

Start from command line arguments:

```shell
realm -l 0.0.0.0:5000 -r 1.1.1.1:443
```

Start with config file:

```shell
# use toml
realm -c config.toml

# use json
realm -c config.json

# use configs in folder (since v2.6.2)
# all toml and json files are recursively included (e.g.: config/log.toml, config/node/n1.toml)
# hidden ones are recursively excluded (e.g.: config/.hidden_file, config/.hidden_dir/)
realm -c config/
```

Start with environment variables:

```shell
REALM_CONF='{"endpoints":[{"listen":"127.0.0.1:5000","remote":"1.1.1.1:443"}]}' realm

# or
export REALM_CONF=`cat config.json | jq -c `
realm
```

`REALM_CONF` holds a whole config, in any of the file formats. If it cannot be parsed, realm says why and goes by the command line instead.

Convert a legacy config file:

```shell
realm convert old.json
```

## Configuration

TOML Example

```toml
[log]
level = "warn"
output = "realm.log"

[network]
no_tcp = false
use_udp = true

[[endpoints]]
listen = "0.0.0.0:5000"
remote = "1.1.1.1:443"

[[endpoints]]
listen = "0.0.0.0:10000"
remote = "www.google.com:443"

```

<details>
<summary>JSON Example</summary>
<p>

```json
{
  "log": {
    "level": "warn",
    "output": "realm.log"
  },
  "network": {
    "no_tcp": false,
    "use_udp": true
  },
  "endpoints": [
    {
      "listen": "0.0.0.0:5000",
      "remote": "1.1.1.1:443"
    },
    {
      "listen": "0.0.0.0:10000",
      "remote": "www.google.com:443"
    }
  ]
}

```

</p>
</details>

[See more examples here](./examples).

## Overview

```shell
├── log
│   ├── level
│   └── output
├── dns
│   ├── mode
│   ├── protocol
│   ├── nameservers
│   ├── min_ttl
│   ├── max_ttl
│   └── cache_size
├── network
│   ├── no_tcp
│   ├── use_udp
│   ├── ipv6_only
│   ├── tcp_timeout
│   ├── udp_timeout
│   ├── udp_no_fragment
│   ├── tcp_keepalive
│   ├── tcp_keepalive_probe
│   ├── send_mptcp
│   ├── accept_mptcp
│   ├── send_proxy
│   ├── send_proxy_version
│   ├── accept_proxy
│   └── accept_proxy_timeout
└── endpoints
    ├── listen
    ├── remote
    ├── extra_remotes
    ├── balance
    ├── failover->
    ├── through
    ├── interface
    ├── listen_interface
    ├── listen_transport
    ├── remote_transport
    └── network->
```

You should provide at least [endpoint.listen](#endpointlisten-string) and [endpoint.remote](#endpointremote-string), the left fields will take their default values.

Option priority: cmd override > endpoint config > global config.

### endpoint

#### endpoint.listen: string

Local address, supported formats:

- ipv4:port
- ipv6:port

#### endpoint.remote: string

Remote address, supported formats:

- ipv4:port
- ipv6:port
- example.com:port

#### endpoint.extra_remotes: string array

Extra remote address, same as endpoint.remote above.

#### endpoint.balance: string

Require `balance` feature.

Load balance strategy and weights of remote peers.

Format:

```bash
$strategy: $weight1, $weight2, ...
```

Where `remote` is used as default backend server, and `extra_remotes` are used as backups.

Available algorithms (provided by [realm_lb](./realm_lb/)):

- iphash: a client address always gets the same peer, and the peers get client addresses in the proportions of their weights. An IPv4 client counts as the same client on a dual-stack listener (`::ffff:a.b.c.d`). Adding a peer at the end of the list, or changing the weight of one, only moves clients to or from that peer.

- roundrobin: connections take turns, in the proportions of the weights.

Builds without the `balance` feature (the slim ones) ignore this option and say so in a warning at startup.

`iphash` was reworked after v3.1.1: before, weights were not followed (`2, 1, 1` gave about 72% / 20% / 8%) and equal weights were off by several points. Upgrading from v3.1.1 or earlier maps the clients of an `iphash` endpoint to peers anew, once. Established connections are not touched.

Example:

```toml
[[endpoints]]
remote = "a:443"
extra_remotes = ["b:443", "c:443"]
balance = "roundrobin: 4, 2, 1"
```

The weight of [a, b, c] is [4, 2, 1] in turn.

#### endpoint.failover: table

Active-standby failover for both TCP and UDP. `remote` is the primary, `extra_remotes` are backups in priority order (at most 7). Peers must be `ip:port` literals; for UDP they must share one address family. Together with `balance`, the peers share the load instead, see [below](#failover-with-balance).

A background task probes every peer per protocol. Traffic only moves on probe results, never on user traffic:

- a peer goes down after `fall` failed probes in the last `window` rounds, consecutive or not;
- a down peer comes back after `rise` consecutive successes;
- traffic only moves to a peer whose latest probe succeeded; if no peer is usable, the current one is kept;
- failing back to a higher priority peer waits `hold` seconds after the last switch, unless the current peer is down.

Behavior:

- TCP: new connections dial the active peer. If that dial fails, the same client connection retries on other ready peers before any data is relayed: up to three dials in all, so two peers failing at the same moment cost no connection while a third is ready. All dials share `tcp_timeout`, and one that can be retried elsewhere gets half of what is left of it. Established connections are never moved. On Linux, a connection whose peer stops acknowledging (or stops reading while data is pending) is closed after `tcp_keepalive` x (`tcp_keepalive_probe` + 1) seconds (60 by default), idle or not, so its client can reconnect to the active peer. Without this, a connection with data in flight to a dead peer hangs for about 15 minutes. `tcp_keepalive = 0` turns both off.
- UDP: new associations use the active peer. An existing association keeps its peer until that peer is down, then its next datagram goes to the active peer through the same upstream socket (client and relay ports stay the same). After such a move, late replies from the old peer are dropped.

Probes (from this host, with the same `interface` and `through` ip; a port in `through` is never used by probes):

- TCP: connect, send `tcp_send` (optional), require a reply starting with `tcp_expect`. Without `tcp_expect` a completed handshake counts as success. While nothing has answered, another connection is started at each third of `timeout`, at most 3 per round, so one lost SYN does not fail the round. A refusal or an unexpected reply fails the round at once.
- UDP, `udp_probe = "echo"` (default): send `udp_send` plus a per-round nonce, require the exact datagram echoed back. Resend only while unanswered, at most 3 datagrams within `timeout`. The target must be an echo responder: against a service that does not echo, every peer is marked down and UDP simply stays on its current peer.
- UDP, `udp_probe = "quic"`: send a QUIC packet of a version no server speaks and require the Version Negotiation packet that every QUIC server answers with. This probes a QUIC service (Hysteria 2, TUIC, HTTP/3) on its own port, without an echo responder next to it. It does not work for services that hide from unauthenticated packets, such as WireGuard or Hysteria with obfuscation: those need an echo responder.

To detect failures behind the next hop, point `probe` at a health port on the far end that is relayed along the same path (e.g. a tiny HTTP 204 responder and a UDP echo).

Probes are plain TCP and UDP. They send no PROXY header ([send_proxy](#networksend_proxy-bool)) and do not go through [remote_transport](#endpointremote_transport-string). When the data port expects either, probe it by handshake only (no `tcp_send` and `tcp_expect`), or point `probe` at a health port. A probe with `tcp_expect` against such a port fails every round: all peers are marked down, and nothing ever moves.

A key in the `failover` table that is not one of these is an error at startup, so that a misspelt option does not fall back to its default unnoticed.

| field | default | meaning |
| --- | --- | --- |
| probe | data addresses | health targets, one per peer, same order as `[remote, extra_remotes...]` |
| tcp_send | none | bytes sent by the TCP probe |
| tcp_expect | none | expected prefix of the TCP probe reply |
| udp_send | empty | prefix of the UDP probe datagram (`echo` only) |
| udp_probe | echo | `echo` or `quic` |
| interval | 3 | seconds between probe rounds |
| timeout | 1.5 | seconds per probe |
| fall / window | 3 / 5 | failures within the last rounds to mark a peer down (window <= 8) |
| rise | 5 | consecutive successes to mark a peer up again |
| hold | 30 | seconds before failing back |

With the defaults, a silently failing primary is detected in about 7.5-10.5s. Keep `tcp_timeout` at 3-5s on failover endpoints.

Watching it:

The log is the only place a failover shows, and it is off by default. Set [log.level](#loglevel-string) to `warn` on a host that uses failover. These lines are written at `warn` (shown here without the time, source and level in front):

```
[failover][tcp]0.0.0.0:5000 peer#0 10.0.0.4:3000 is down
[failover][tcp]0.0.0.0:5000 switch peer#0 10.0.0.4:3000 => peer#1 10.0.0.5:3000
[failover][tcp]0.0.0.0:5000 peer#0 10.0.0.4:3000 is up
[tcp]connect to peer#0 10.0.0.4:3000: connect timeout, retry peer#1 10.0.0.5:3000
```

- TCP and UDP are probed and reported separately (`[tcp]`, `[udp]`), and `peer#0` is `remote`. With `balance` there is no `switch` line, only `is down` and `is up`.
- The `retry` line is written once for every dial that is retried. Between a peer dying and the probes taking it out, a relay with many short connections writes one for each of them: tens of thousands of lines within seconds were measured at 30000 connections a second.
- A log file is never rotated by realm. Rotate it yourself (`copytruncate` with logrotate), or log to stdout and leave it to the service manager.
- At startup, the `inited:` line of an endpoint shows the failover options in effect.
- `info` adds a line for every connection and every UDP association with the peer it went to.

Limits:

- An endpoint has 2 to 8 peers: `remote` plus 1 to 7 `extra_remotes`.
- A new TCP connection dials at most three peers, and only retries on peers whose latest probe succeeded. A peer that has just died still counts as ready until a probe sees it fail, which takes up to `interval` + `timeout` (4.5s with the defaults; about `interval` if it refuses connections). So when three or more peers fail at the same moment, new connections can fail in that window. For example, with four peers and three of them dead, a connection whose three dials all land on dead peers fails without trying the fourth. This mostly happens when the dead peers fail fast (connection refused or unreachable). If a dial times out instead, the probes usually catch up before the next retry is chosen.
- Right after realm starts, no peer counts as ready until the first probe round finishes. That round starts at once and takes at most `timeout` (1.5s by default). Until then, a failed dial is not retried, so a new connection sent to a dead peer fails.

Example:

```toml
[[endpoints]]
listen = "0.0.0.0:5000"
remote = "10.0.0.4:3000"
extra_remotes = ["10.0.0.5:3000"]
network = { use_udp = true, tcp_timeout = 4 }

[endpoints.failover]
probe = ["10.0.0.4:3001", "10.0.0.5:3001"]
tcp_send = "GET / HTTP/1.0\r\n\r\n"
tcp_expect = "HTTP/1.1 204"
```

##### failover with balance

With both `balance` and `failover` on an endpoint, the peers are not primary and backups: the balancer spreads new TCP connections and new UDP associations over them, and the probes take dead ones out.

- A new session goes to the peer the balancer picks. If that peer is down, the others share what it would get, in the proportions of their weights: `roundrobin` moves on to its next peer, and with `iphash` each client of the dead peer has its own fixed substitute, so it still stays on one peer and the clients of the other peers are not moved. One failed probe does not move anything.
- TCP: if the dial fails, the same client connection retries on the next ready peers, as above.
- UDP: associations are balanced too (with `balance` alone, UDP only uses `remote`). An association keeps its peer until that peer is down, then moves to another one that is alive.
- A peer that comes back takes new sessions again after `rise` successes. `hold` is not used.
- `balance` needs one weight per peer.

Prefer `iphash` when a client's session spans several connections or carries state kept by one server (XHTTP, 0-RTT resumption): `roundrobin` sends those connections to different servers.

```toml
[[endpoints]]
listen = "0.0.0.0:5000"
remote = "10.0.0.4:3000"
extra_remotes = ["10.0.0.5:3000", "10.0.0.6:3000"]
balance = "iphash: 1, 1, 1"
network = { use_udp = true, tcp_timeout = 4 }

[endpoints.failover]
probe = ["10.0.0.4:3001", "10.0.0.5:3001", "10.0.0.6:3001"]
```

#### endpoint.through: string

TCP: Bind a specific `ip` before opening a connection.

UDP: Bind a specific `ip` or `address` before sending packet.

Supported formats:

- ipv4/ipv6 (tcp/udp)
- ipv4/ipv6:port (udp)

#### endpoint.interface: string

Bind to a specific interface for outgoing traffics.

#### endpoint.listen_interface: string

Bind to a specific interface for incoming traffics.

#### endpoint.listen_transport: string

Require `transport` feature.

See [Kaminari Options](https://github.com/zephyrchien/kaminari#options).

#### endpoint.remote_transport: string

Require `transport` feature.

See [Kaminari Options](https://github.com/zephyrchien/kaminari#options).

#### endpoint.network

The same as [network](#network), override global options.

### log

#### log.level: string

values:

- off
- error
- warn
- info
- debug
- trace

default: off

#### log.output: string

values:

- stdout
- stderr
- path (e.g. `/var/log/realm.log`)

default: stdout

### dns

Require `trust-dns` feature.

#### dns.mode: string

Dns resolve strategy.

values:

- ipv4_only
- ipv6_only
- ipv4_then_ipv6
- ipv6_then_ipv4
- ipv4_and_ipv6

default: ipv4_and_ipv6

#### dns.protocol: string

Dns transport protocol.

values:

- tcp
- udp
- tcp_and_udp

default: tcp_and_udp

#### dns.nameservers: string array

Custom upstream servers.

format: ["server1", "server2" ...]

default:

If on **unix/windows**, read from the default location.(e.g. `/etc/resolv.conf`).

Otherwise, use google's public dns(`8.8.8.8:53`, `8.8.4.4:53` and `2001:4860:4860::8888:53`, `2001:4860:4860::8844:53`).

#### dns.min_ttl: unsigned int

The minimum lifetime of a positive dns cache.

default: 0

#### dns.max_ttl: unsigned int

The maximum lifetime of a positive dns cache.

default: 86400 (1 day)

#### dns.cache_size: unsigned int

The maximum count of dns cache.

default: 32

### network

#### network.no_tcp: bool

Do not start a tcp relay.

default: false

#### network.use_udp: bool

~~Require `udp` feature~~

Start listening on a udp endpoint and forward packets to the remote peer.

It will dynamically allocate local endpoints and establish udp associations. Once timeout, the endpoints will be deallocated and the association will be terminated. See also: [network.udp_timeout](#networkudp_timeout-unsigned-int).

Due to the receiver side not limiting access to the association, the relay works like a full-cone NAT.

Datagrams of any size are relayed whole, including those that exceed one ethernet frame and travel as IP fragments (up to 65507 bytes of payload over IPv4).

default: false

#### network.ipv6_only: bool

Disable ipv4-mapped-ipv6 when binding to an ipv6 address.

E.g.:
`[::0]:port` with (ipv6_only=false) binds to `*:port`

`[::0]:port` with (ipv6_only=true) binds to `[::]:port`

default: false

#### ~~network.zero_copy: bool~~ deprecated

~~Require `zero-copy` feature.~~

~~Use `splice` instead of `send/recv` while handing tcp connection. This will save a lot of memory copies and context switches.~~

~~default: false~~

#### ~~network.fast_open: bool~~ deprecated

~~Require `fast-open` feature.~~

~~It is not recommended to enable this option, see [The Sad Story of TCP Fast Open](https://squeeze.isobar.com/2019/04/11/the-sad-story-of-tcp-fast-open/).~~

~~default: false~~

#### network.tcp_timeout: unsigned int

This is **connect** timeout. An attempt to connect to a remote peer fails after waiting for a period of time.

To disable timeout, you need to explicitly set timeout value to 0.

default: 5

#### network.udp_timeout: unsigned int

Terminate udp association after `timeout`.

The timeout value must be properly configured in case of memory leak. Do not use a large `timeout`!

default: 30

#### network.udp_no_fragment: bool

Linux only. Forward UDP the way a router does: a datagram that fits one frame leaves as don't-fragment and is never fragmented on this host.

Without it, once the kernel learns that the path to a destination is narrower than the interface, it fragments large datagrams itself. A QUIC sender (Hysteria, TUIC, HTTP/3) that probes the path MTU through the relay is then told its large packets arrive, keeps using them, and every one of them reaches the client in two fragments. Many networks carry fragments poorly or not at all, so transfers towards clients behind a smaller MTU (mobile networks, tunnels) slow down or stall. With this on, a datagram too large for the path is lost like behind any router, and the sender settles on a size that fits.

Turn it on for QUIC based services. Leave it off for WireGuard endpoints: WireGuard does not look for the path MTU and depends on fragmentation when its MTU is larger than the path allows (better: lower its MTU).

Datagrams that need more than one frame arrived in fragments and are still fragmented.

default: false

#### network.tcp_keepalive: unsigned int

TCP Keepalive interval.

On Linux, this is equivalent to setting both `net.ipv4.tcp_keepalive_time` and `net.ipv4.tcp_keepalive_intvl`.

To use system's tcp keepalive interval, you need to explicitly set this option to 0.

default: 15

#### network.tcp_keepalive_probe: unsigned int

TCP Keepalive retries.

On Linux, this is equivalent to `ipv4.tcp_keepalive_probes`.

At least 1: with keepalive on, 0 is taken as 1 and reported in a warning at startup (Linux refuses a count of 0, and with it every connection). To turn keepalive off, set `tcp_keepalive = 0`.

default: 3

#### network.send_mptcp: bool

Enable MPTCP outbound connections on Linux.

Requires a higher kernel version(>5.6) with `net.mptcp.enabled=1`.

See also [Path Manager](https://www.mptcp.dev/pm.html) guidelines.

default: false

#### network.accept_mptcp: bool

Enable MPTCP inbound connections on Linux.

default: false

#### network.send_proxy: bool

Require `proxy` feature.

Send haproxy PROXY header once the connection established. Both `v1` and `v2` are supported, see [send_proxy_version](#networksend_proxy_version-unsigned-int).

You should make sure the remote peer also speaks proxy-protocol.

default: false

#### network.send_proxy_version: unsigned int

Require `proxy` feature.

This option has no effect unless [send_proxy](#networksend_proxy-bool) is enabled.

value:

- 1
- 2

default: 2

#### network.accept_proxy: bool

Require `proxy` feature.

Wait for a PROXY header once the connection established.

If the remote sender does not send a `v1` or `v2` header before other contents, the connection will be closed.

default: false

#### network.accept_timeout: unsigned int

Require `proxy` feature.

Wait for a PROXY header within a period of time, otherwise close the connection.

default: 5.
