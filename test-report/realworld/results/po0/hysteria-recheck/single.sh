#!/bin/sh
# single.sh "<binary> <port>" ...   one plain udp endpoint per pair towards the chi hysteria port
pkill -f 'tmp/one-' 2>/dev/null; sleep 1
for v in "$@"; do
  set -- $v
  printf '[log]\nlevel = "warn"\n[network]\nuse_udp = true\nno_tcp = true\n[[endpoints]]\nlisten = "0.0.0.0:%s"\nremote = "198.51.100.11:31020"\n' "$2" > /tmp/one-$2.toml
  setsid nohup /rbt/bin/$1 -c /tmp/one-$2.toml >/tmp/one-$2.log 2>&1 &
done
sleep 1
ss -ulnpH | grep -E ':2602[0-9] ' | sed -E 's/.*:(2602[0-9]) .*users:\(\("([a-z0-9]+)".*/\1 \2/'
