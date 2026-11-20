#!/bin/bash
# fmt + clippy + tests + release build of the working tree into /work/bin/realm-fix
set -u
export CARGO_TERM_COLOR=never
cd /src
echo "== fmt"; cargo fmt --all -- --check && echo fmt-ok
echo "== clippy"; cargo clippy 2>&1 | tail -3
echo "== test realm_core (default features of the root crate)"
cargo test -p realm_core --features "balance,proxy,batched-udp,multi-thread" 2>&1 | grep -E "^test |test result|error|panicked|FAILED" | grep -v "\.\.\. ok$" 
echo "== test root"
cargo test 2>&1 | grep -E "test result|error\[|panicked|FAILED"
echo "== release"
cargo build --release 2>&1 | tail -1
cp target/release/realm /work/bin/realm-fix.tmp && mv /work/bin/realm-fix.tmp /work/bin/realm-fix && ls -la /work/bin/
echo CHECK-DONE
