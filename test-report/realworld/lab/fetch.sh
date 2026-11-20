#!/bin/sh
# Runs inside the rbt image: download and verify the binaries under test into /rbt/bin.
set -e
mkdir -p /rbt/bin /rbt/dl
cd /rbt/dl
XRAY_VER=v26.11.9
case $(uname -m) in
x86_64)  XA=64;        HA=amd64; RA=x86_64;  V3SUM=695ccef3a1021248be8892544f8ee690f8fd9fc6cd43bf04395cb2e59bf48f92 ;;
aarch64) XA=arm64-v8a; HA=arm64; RA=aarch64; V3SUM=712a9eb8e418caff198fdb5028049cbd6d4ecbfaada10c3fcf69e11904207d38 ;;
esac
get() { curl -fsSL --retry 3 --max-time 180 -o "$1" "$2"; }
sum() { sha256sum "$1" | cut -d' ' -f1; }

get xray.zip https://github.com/XTLS/Xray-core/releases/download/$XRAY_VER/Xray-linux-$XA.zip
get xray.dgst https://github.com/XTLS/Xray-core/releases/download/$XRAY_VER/Xray-linux-$XA.zip.dgst
want=$(grep -iE '^SHA(2-)?256' xray.dgst | head -1 | awk '{print $NF}')
[ "$(sum xray.zip)" = "$want" ] || { echo "xray checksum mismatch"; exit 1; }
unzip -oq xray.zip xray -d /rbt/bin

get hysteria https://github.com/apernet/hysteria/releases/latest/download/hysteria-linux-$HA
if get hy.hashes https://github.com/apernet/hysteria/releases/latest/download/hashes.txt 2>/dev/null; then
  want=$(grep "hysteria-linux-$HA\$" hy.hashes | head -1 | awk '{print $1}')
  [ -z "$want" ] || [ "$(sum hysteria)" = "$want" ] || { echo "hysteria checksum mismatch"; exit 1; }
  [ -n "$want" ] && echo "hysteria checksum ok" || echo "hysteria: no published hash found"
fi
install -m 755 hysteria /rbt/bin/hysteria

get v3.tgz https://github.com/kaze-kaze/realm/releases/download/v3.0.0/realm-$RA-unknown-linux-musl.tar.gz
[ "$(sum v3.tgz)" = "$V3SUM" ] || { echo "realm v3.0.0 checksum mismatch"; exit 1; }
tar xzf v3.tgz && install -m 755 realm /rbt/bin/realm3 && rm -f realm

get og.tgz https://github.com/zhboner/realm/releases/download/v2.9.6/realm-$RA-unknown-linux-musl.tar.gz
tar xzf og.tgz && install -m 755 realm /rbt/bin/realmog && rm -f realm

cd / && rm -rf /rbt/dl
/rbt/bin/xray version | head -1
/rbt/bin/hysteria version 2>/dev/null | grep -iE '^version' || true
/rbt/bin/realm3 -v
/rbt/bin/realmog -v
