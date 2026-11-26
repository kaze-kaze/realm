# Realm IO

[![crates.io](https://img.shields.io/crates/v/realm_io.svg)](https://crates.io/crates/realm_io)
[![Released API docs](https://docs.rs/realm_io/badge.svg)](https://docs.rs/realm_io)

Realm's high performance IO collections.

## Example

```rust
use tokio::net::TcpStream;
use realm_io::{bidi_copy, bidi_zero_copy, bidi_copy_buf};
use realm_io::{Pipe, CopyBuffer};

let mut left = TcpStream::connect("abc").await.unwrap();
let mut right = TcpStream::connect("def").await.unwrap();

// direct copy     
bidi_copy(&mut left, &mut right).await;

// zero copy
bidi_zero_copy(&mut left, &mut right).await;

// use custom buffer(vector)
let buf1 = CopyBuffer::new(vec![0; 0x2000]);
let buf2 = CopyBuffer::new(vec![0; 0x2000]);
bidi_copy_buf(&mut left, &mut right, buf1, buf2).await;

// use custom buffer(pipe)
let buf1 = CopyBuffer::new(Pipe::new().unwrap());
let buf2 = CopyBuffer::new(Pipe::new().unwrap());
bidi_copy_buf(&mut left, &mut right, buf1, buf2).await;
```

## About Brutal Shutdown

By default, `bidi_copy_buf` and other IO functions perform a **graceful shutdown**.

With the feature `brutal-shutdown` enabled, these IO functions will decide to
perform a **brutal shutdown** once a `FIN` packet reaches, which will forcefully
close two connections on both sides without waiting for a reply packet.

This is helpful when handling connections from a poorly implemented client or server,
which may never shutdown its write side nor close the underlying socket.

The `bidi_copy_with_timeout`, `bidi_zero_copy_with_timeout` and `bidi_copy_buf_with_timeout`
variants take this choice as an argument and ignore the feature. `None` waits for both
directions to finish. `Some(Duration::ZERO)` closes both sides as soon as either direction
finishes. Any other duration lets the remaining direction run until it has made no write
progress for that long.
