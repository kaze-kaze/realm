#!/bin/bash
# build original v2.9.6 and the working tree, default features, release
set -eu
export CARGO_TERM_COLOR=never
git config --global --add safe.directory /src
mkdir -p /work/bin
if [ "${1:-all}" = all ]; then
  rm -rf /orig && mkdir /orig && git -C /src archive 7e9f44c | tar -x -C /orig
  (cd /orig && cargo build --release --target-dir /src/target/orig 2>&1 | tail -2)
  cp /src/target/orig/release/realm /work/bin/realm-orig
fi
(cd /src && cargo build --release 2>&1 | tail -2)
cp /src/target/release/realm /work/bin/realm-new.tmp && mv /work/bin/realm-new.tmp /work/bin/realm-new
ls -la /work/bin
echo BUILD-DONE
