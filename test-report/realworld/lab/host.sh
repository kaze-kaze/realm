#!/bin/sh
# Runs on the docker host.  host.sh image | fetch | up | fw on|off | down
R=${RBT_ROOT:-/opt/rbt}
ME=$(cat "$R/me")
# Addresses allowed to reach the test ports: the test hosts and any outside client.
# The defaults are placeholders from the documentation range, the real ones are not kept
# in the repository: set RBT_PEERS, or write them to lab/peers.local (space separated).
PEERS=${RBT_PEERS:-$(cat "$R/lab/peers.local" 2>/dev/null)}
PEERS=${PEERS:-"198.51.100.11 198.51.100.12 198.51.100.13 203.0.113.10"}
PKGS="python3 curl ca-certificates iproute2 iptables iperf3 wireguard-tools socat procps unzip jq dnsutils tcpdump aria2 openssl psmisc netcat-openbsd iputils-ping conntrack kmod"

fw_on() {
  case $ME in
  ams)
    nft list chain inet host_input input | grep -q '"rbt"' || \
      nft insert rule inet host_input input ip saddr "{ $(echo $PEERS | sed 's/ /, /g') }" meta l4proto "{ tcp, udp }" th dport 21000-24999 accept comment '"rbt"'
    ;;
  chi)
    iptables -N RBT 2>/dev/null || iptables -F RBT
    for p in $PEERS; do
      iptables -A RBT -s "$p" -p tcp --dport 21000:24999 -j ACCEPT
      iptables -A RBT -s "$p" -p udp --dport 21000:24999 -j ACCEPT
    done
    iptables -C INPUT -j RBT 2>/dev/null || iptables -I INPUT 1 -j RBT
    ;;
  esac
}

fw_off() {
  case $ME in
  ams)
    for h in $(nft -a list chain inet host_input input | grep '"rbt"' | sed 's/.*handle //'); do
      nft delete rule inet host_input input handle "$h"
    done
    ;;
  chi)
    iptables -D INPUT -j RBT 2>/dev/null
    iptables -F RBT 2>/dev/null
    iptables -X RBT 2>/dev/null
    ;;
  esac
}

case $1 in
image)
  docker rm -f rbt-build >/dev/null 2>&1
  docker run --name rbt-build -e DEBIAN_FRONTEND=noninteractive debian:trixie-slim sh -c \
    "apt-get update -qq && apt-get install -y -qq --no-install-recommends $PKGS >/dev/null && rm -rf /var/lib/apt/lists/*" || exit 1
  docker commit rbt-build rbt:1 >/dev/null && docker rm rbt-build >/dev/null
  docker images rbt:1 --format '{{.Repository}}:{{.Tag}} {{.Size}}'
  ;;
fetch)
  docker run --rm -v "$R":/rbt rbt:1 sh /rbt/lab/fetch.sh
  ;;
up)
  mtu=$(cat /sys/class/net/eth0/mtu 2>/dev/null || echo 1500)
  docker network inspect rbtnet >/dev/null 2>&1 || docker network create -o com.docker.network.driver.mtu="$mtu" rbtnet >/dev/null
  docker rm -f rbt-land rbt-relay rbt-cli >/dev/null 2>&1
  common="--init -v $R:/rbt -w /rbt/lab --ulimit nofile=1048576:1048576 -e RBT_ME=$ME"
  if [ "$ME" != mac ]; then
    docker run -d --name rbt-land --hostname land-$ME --privileged --network rbtnet $common \
      -p 0.0.0.0:31001-31012:31001-31012/tcp -p 0.0.0.0:31040:31040/tcp -p 0.0.0.0:31050:31050/tcp -p 0.0.0.0:31060:31060/tcp \
      -p 0.0.0.0:31008-31010:31008-31010/udp -p 0.0.0.0:31020-31021:31020-31021/udp -p 0.0.0.0:31030:31030/udp \
      -p 0.0.0.0:31040:31040/udp -p 0.0.0.0:31050:31050/udp -p 0.0.0.0:31060:31060/udp \
      rbt:1 sleep infinity >/dev/null
    docker run -d --name rbt-relay --hostname relay-$ME --network host --cap-add NET_ADMIN --cap-add NET_RAW --cap-add SYS_PTRACE \
      $common rbt:1 sleep infinity >/dev/null
    fw_on
  fi
  docker run -d --name rbt-cli --hostname cli-$ME --privileged --network rbtnet $common rbt:1 sleep infinity >/dev/null
  docker ps --filter name=rbt- --format '{{.Names}} {{.Status}}'
  ;;
fw)
  [ "$2" = on ] && fw_on || fw_off
  ;;
bg)
  # host.sh bg <tag> <container> "<command>"   detached, output in run/out/<tag>.out
  mkdir -p "$R/run/out"; rm -f "$R/run/out/$2.out" "$R/run/out/$2.rc"
  docker exec -d "$3" sh -c "cd /rbt/lab && { $4 ; } >/rbt/run/out/$2.out 2>&1; echo \$? >/rbt/run/out/$2.rc"
  echo "started $2"
  ;;
wait)
  # host.sh wait <tag> [max seconds] [tail lines]
  n=0; max=${3:-240}
  while [ ! -f "$R/run/out/$2.rc" ] && [ $n -lt $max ]; do sleep 2; n=$((n+2)); done
  tail -n "${4:-40}" "$R/run/out/$2.out" 2>/dev/null
  if [ -f "$R/run/out/$2.rc" ]; then echo "[done rc=$(cat "$R/run/out/$2.rc")]"; else echo "[still running after ${n}s]"; fi
  ;;
down)
  docker rm -f rbt-land rbt-relay rbt-cli rbt-build rbt-rust >/dev/null 2>&1
  docker network rm rbtnet >/dev/null 2>&1
  fw_off
  docker rmi rbt:1 debian:trixie-slim rust:slim-trixie >/dev/null 2>&1
  rm -rf "$R"
  echo "down: containers $(docker ps -a --filter name=rbt- -q | wc -l), images $(docker images -q rbt | wc -l), dir $(ls -d "$R" 2>/dev/null | wc -l)"
  ;;
*)
  echo "usage: host.sh image|fetch|up|fw on|off|down"; exit 1
  ;;
esac
