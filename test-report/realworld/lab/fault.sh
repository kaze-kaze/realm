#!/bin/sh
# Fault injection inside a landing container (its own network namespace, the host is not touched).
#   fault.sh <action> [args]
#   fault.sh sched <base-epoch> "<offset-seconds> <action> [args]" ...     (background, logged)
# actions: bh on|off            drop everything in and out (silent host or path failure)
#          bhsrc <ip> on|off    drop one source only (path failure seen by one relay)
#          rej on|off           refuse: TCP reset, UDP port unreachable (host up, service ports closed)
#          loss <pct>|off       random loss both ways (iptables in, netem out)
#          netem <args>|off     tc netem on egress (delay, jitter, ...)
#          health bad|good      health port answers 503 / stops echoing
#          stop <svc> | start <svc>   svc: xray hy2 hy2o tgt health wg
#          freeze <proc> | thaw <proc>  SIGSTOP / SIGCONT (process hangs, ports still accept)
RUN=/rbt/run
act() {
  a=$1; shift
  case $a in
  bh)
    # Outgoing packets are dropped in the qdisc, not by an OUTPUT rule: that one makes
    # send() fail with EPERM, which no real outage does (quic servers close on it).
    if [ "$1" = off ]; then iptables -D INPUT -i eth0 -j DROP; tc qdisc del dev eth0 root 2>/dev/null
    else iptables -I INPUT -i eth0 -j DROP; tc qdisc replace dev eth0 root netem loss 100%; fi ;;
  bhsrc)
    if [ "$2" = off ]; then iptables -D INPUT -i eth0 -s "$1" -j DROP; iptables -D OUTPUT -o eth0 -d "$1" -j DROP
    else iptables -I INPUT -i eth0 -s "$1" -j DROP; iptables -I OUTPUT -o eth0 -d "$1" -j DROP; fi ;;
  rej)
    if [ "$1" = off ]; then
      iptables -D INPUT -i eth0 -p tcp -j REJECT --reject-with tcp-reset; iptables -D INPUT -i eth0 -p udp -j REJECT --reject-with icmp-port-unreachable
    else
      iptables -I INPUT -i eth0 -p tcp -j REJECT --reject-with tcp-reset; iptables -I INPUT -i eth0 -p udp -j REJECT --reject-with icmp-port-unreachable
    fi ;;
  loss)
    # incoming: iptables; outgoing: netem (see bh for why not an OUTPUT rule)
    iptables -S INPUT | grep -- '--mode random' | sed 's/^-A/-D/' | while read -r r; do iptables $r; done
    tc qdisc del dev eth0 root 2>/dev/null
    if [ "$1" != off ]; then
      p=$(python3 -c "print($1/100.0)")
      iptables -I INPUT -i eth0 -m statistic --mode random --probability "$p" -j DROP
      tc qdisc replace dev eth0 root netem loss "$1%"
    fi ;;
  netem)
    if [ "$1" = off ]; then tc qdisc del dev eth0 root 2>/dev/null; else tc qdisc replace dev eth0 root netem "$@"; fi ;;
  health)
    if [ "$1" = bad ]; then touch $RUN/land/unhealthy; else rm -f $RUN/land/unhealthy; fi ;;
  stop)  sh /rbt/lab/svc.sh land-stop "$1" ;;
  start) sh /rbt/lab/svc.sh land-start "$1" ;;
  freeze) pkill -STOP -x "$1" ;;
  thaw)   pkill -CONT -x "$1" ;;
  clear)
    iptables -F INPUT; iptables -F OUTPUT; tc qdisc del dev eth0 root 2>/dev/null; rm -f $RUN/land/unhealthy
    pkill -CONT -x xray; pkill -CONT -x hysteria ;;
  *) echo "unknown action $a"; return 1 ;;
  esac
  echo "$(date +%s.%N | cut -c1-14) $a $*" >>$RUN/land/fault.log
}

if [ "$1" = sched ]; then
  base=$2; shift 2
  for e in "$@"; do
    off=${e%% *}; rest=${e#* }
    ( python3 -c "import time,sys; time.sleep(max(0, float(sys.argv[1]) + float(sys.argv[2]) - time.time()))" "$base" "$off"
      # shellcheck disable=SC2086
      act $rest ) </dev/null >/dev/null 2>&1 &
  done
  exit 0
fi
act "$@"
