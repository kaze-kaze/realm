#!/bin/bash
# RF-02 runs, one after another (they share realm listen port 39620).
cd "$(dirname "$0")" || exit 1
P="both:3,v6:8,v4:8,v6:8,both:6"
run() { timeout 150 python3 -I rf02_dns.py "$@" || echo "FAILED: $*"; }
run new new-default        phases=$P,both:34
run new new-timeout0       udp_timeout=0 phases=$P
run new new-timeout2       udp_timeout=2 phases=$P
run dbg dbg-default        phases=both:3,v6:6,v4:6,both:4
run dbg dbg-timeout0       udp_timeout=0 phases=both:3,v6:6,v4:6,both:4
run old old-default        phases=$P
run old old-timeout0       udp_timeout=0 phases=$P
run old old-v4first-default  phases=v4:3,v6:40,v4:6,both:6
run old old-v4first-timeout0 udp_timeout=0 phases=v4:3,v6:12,v4:6,both:6
run new new-v4first-timeout0 udp_timeout=0 phases=v4:3,v6:12,v4:6,both:6
run new new-flap-timeout0  udp_timeout=0 phases=v4:2,v6:2,v4:2,v6:2,v4:2,v6:2,v4:2,v6:2,v4:2,v6:2,both:4
run new new-flap-timeout2  udp_timeout=2 phases=v4:2,v6:2,v4:2,v6:2,v4:2,v6:2,v4:2,v6:2,v4:2,v6:2,both:8
run new new-mode-6then4    mode=ipv6_then_ipv4 phases=both:3,v4:6,v6:6
run new new-through-v4     through=127.0.0.1 tcp=1 phases=both:3,v6:5,v4:4
run new new-through-v6     through=::1 tcp=1 phases=both:3,v4:5,v6:4
run old old-through-v4     through=127.0.0.1 tcp=1 phases=both:3,v6:5,v4:4
run old old-through-v6     through=::1 tcp=1 phases=both:3,v4:5,v6:4
run new new-failover       failover=1 tcp=1 phases=both:4,v6:3
run dbg dbg-failover       failover=1 tcp=1 phases=both:4,v6:3
run old old-failover       failover=1 tcp=1 phases=both:4,v6:3
