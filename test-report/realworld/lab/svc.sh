#!/bin/sh
# Runs inside a container.  svc.sh land-up | land-down | relay-up [opts-json] | relay-down | land-start <name>
RUN=/rbt/run; BIN=/rbt/bin; LAB=/rbt/lab
ME=${RBT_ME:-$(cat /rbt/me)}
mkdir -p $RUN/log $RUN/pid
bg() { name=$1; shift; setsid nohup "$@" >>$RUN/log/$name.log 2>&1 & echo $! >$RUN/pid/$name.pid; }

land_start() {
  case $1 in
  xray)   bg xray $BIN/xray run -c $RUN/land/xray.json ;;
  hy2)    bg hy2 $BIN/hysteria server -c $RUN/land/hy2.yaml ;;
  hy2o)   bg hy2o $BIN/hysteria server -c $RUN/land/hy2-obfs.yaml ;;
  tgt)    bg tgt python3 $LAB/tgt.py 31060 8 ;;
  health) bg health python3 $LAB/health.py 31040 --gate $RUN/land/unhealthy ;;
  iperf)  for p in 31050 31051 31052 31053 31054; do iperf3 -s -p $p -D >/dev/null 2>&1; done ;;
  wg)
    ip link del wg0 2>/dev/null
    ip link add wg0 type wireguard && wg setconf wg0 $RUN/land/wg0.conf && ip addr add 10.99.0.1/16 dev wg0 && ip link set wg0 mtu ${RBT_WG_MTU:-1420} up
    sysctl -qw net.ipv4.ip_forward=1
    iptables -t nat -C POSTROUTING -s 10.99.0.0/16 ! -o wg0 -j MASQUERADE 2>/dev/null || iptables -t nat -A POSTROUTING -s 10.99.0.0/16 ! -o wg0 -j MASQUERADE
    ;;
  esac
}
land_stop() {
  case $1 in
  xray) pkill -x xray ;;
  hy2)  pkill -f 'hysteria server -c /rbt/run/land/hy2.yaml' ;;
  hy2o) pkill -f 'hysteria server -c /rbt/run/land/hy2-obfs.yaml' ;;
  tgt)  pkill -f tgt.py ;;
  health) pkill -f health.py ;;
  iperf) pkill -x iperf3 ;;
  wg)   ip link del wg0 2>/dev/null ;;
  esac
}

case $1 in
land-up)
  mkdir -p $RUN/land
  for s in xray hy2 hy2o tgt health iperf wg; do land_stop $s; done
  rm -f $RUN/land/unhealthy
  python3 $LAB/gen.py land $ME || exit 1
  $BIN/xray run -test -c $RUN/land/xray.json 2>&1 | tail -3
  sleep 0.5
  for s in xray hy2 hy2o tgt health iperf wg; do land_start $s; done
  sleep 2
  echo "procs: $(pgrep -x xray | wc -l) xray, $(pgrep -x hysteria | wc -l) hysteria, $(pgrep -f tgt.py | wc -l) tgt, $(pgrep -f health.py | wc -l) health, $(pgrep -x iperf3 | wc -l) iperf3; wg: $(wg show wg0 listen-port 2>&1)"
  ;;
land-down)
  for s in xray hy2 hy2o tgt health iperf wg; do land_stop $s; done
  ;;
land-start) land_start $2 ;;
land-stop)  land_stop $2 ;;
relay-up)
  mkdir -p $RUN/relay
  pkill -f '/rbt/bin/realm'; pkill -f mon.py
  R3=${RBT_REALM:-realm3}
  OPTS=$2; [ -n "$OPTS" ] || OPTS='{}'
  python3 $LAB/gen.py relay $ME "$OPTS" || exit 1
  sleep 0.5
  [ -n "$3" ] || : >$RUN/relay/v3.log
  : >$RUN/log/realm3.log
  bg realm3 $BIN/$R3 -c $RUN/relay/v3.toml
  bg realmog $BIN/realmog -c $RUN/relay/og.toml
  bg mon python3 $LAB/mon.py
  sleep 1.5
  echo "procs: $(pgrep -f '/rbt/bin/realm3' | wc -l) x $R3 ($($BIN/$R3 -v | cut -d" " -f2), $(sha256sum $BIN/$R3 | cut -c1-12)), $(pgrep -x realmog | wc -l) realmog; listening: $(ss -tlnH | grep -cE ':2[1-4][0-9]{3} ') tcp, $(ss -ulnH | grep -cE ':2[1-4][0-9]{3} ') udp"
  tail -3 $RUN/log/realm3.log
  ;;
relay-down)
  pkill -f '/rbt/bin/realm'; pkill -f mon.py
  ;;
*) echo "usage"; exit 1 ;;
esac
