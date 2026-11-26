### new-default (binary new, options {"phases": "both:3,v6:8,v4:8,v6:8,both:6,both:34"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| both | 3 | 0 | 30 | 30 | 0 | 0 | 0 | v4 0.02 | 60403 | 3..3 (max 3) |
| both | 3 | 1 | 30 | 30 | 0 | 0 | 0 | v4 0.02 | 40926 | 3..3 (max 3) |
| v6 | 8 | 0 | 80 | 1 | 79 | 0 | 0 | v4 0.00, v6 0.10 | 60403 59669 | 3..5 (max 5) |
| v6 | 8 | 1 | 80 | 1 | 79 | 0 | 0 | v4 0.00, v6 0.10 | 40926 39153 | 3..5 (max 5) |
| v4 | 8 | 0 | 77 | 75 | 2 | 0 | 0 | v4 0.20, v6 0.00 | 59669 39869 | 5..7 (max 7) |
| v4 | 8 | 1 | 77 | 75 | 2 | 0 | 0 | v4 0.20, v6 0.00 | 39153 57498 | 5..7 (max 7) |
| v6 | 8 | 0 | 80 | 3 | 77 | 0 | 0 | v4 0.00, v6 0.30 | 39869 52764 | 7..9 (max 9) |
| v6 | 8 | 1 | 80 | 3 | 77 | 0 | 0 | v4 0.00, v6 0.30 | 57498 57239 | 7..9 (max 9) |
| both | 6 | 0 | 60 | 57 | 3 | 0 | 0 | v4 0.30, v6 0.00 | 52764 34540 | 9..11 (max 11) |
| both | 6 | 1 | 60 | 57 | 3 | 0 | 0 | v4 0.30, v6 0.00 | 57239 59500 | 9..11 (max 11) |
| both | 34 | 0 | 337 | 337 | 0 | 0 | 0 | v4 0.00 | 34540 | 9..3 (max 9) |
| both | 34 | 1 | 337 | 337 | 0 | 0 | 0 | v4 0.00 | 59500 | 9..3 (max 9) |

log: 10 `new association` lines, 8 `remove association` lines; resolution changes: 127.0.0.1:35224 -> [::1]:35224 -> 127.0.0.1:35224 -> [::1]:35224 -> 127.0.0.1:35224
alive at end: True

### new-timeout0 (binary new, options {"udp_timeout": "0", "phases": "both:3,v6:8,v4:8,v6:8,both:6"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| both | 3 | 0 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 49634 | 3..3 (max 3) |
| both | 3 | 1 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 39767 | 3..3 (max 3) |
| v6 | 8 | 0 | 80 | 0 | 80 | 0 | 0 | v6 0.00 | 40005 | 5..5 (max 5) |
| v6 | 8 | 1 | 80 | 0 | 80 | 0 | 0 | v6 0.00 | 49958 | 5..5 (max 5) |
| v4 | 8 | 0 | 80 | 80 | 0 | 0 | 0 | v4 0.00 | 37781 | 7..7 (max 7) |
| v4 | 8 | 1 | 80 | 80 | 0 | 0 | 0 | v4 0.00 | 44464 | 7..7 (max 7) |
| v6 | 8 | 0 | 80 | 1 | 79 | 0 | 0 | v4 0.00, v6 0.10 | 37781 37619 | 7..9 (max 9) |
| v6 | 8 | 1 | 80 | 1 | 79 | 0 | 0 | v4 0.00, v6 0.10 | 44464 48633 | 7..9 (max 9) |
| both | 6 | 0 | 60 | 59 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 37619 50671 | 9..11 (max 11) |
| both | 6 | 1 | 60 | 59 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 48633 54677 | 9..11 (max 11) |

log: 10 `new association` lines, 0 `remove association` lines; resolution changes: 127.0.0.1:60127 -> [::1]:60127 -> 127.0.0.1:60127 -> [::1]:60127 -> 127.0.0.1:60127
alive at end: True

### new-timeout2 (binary new, options {"udp_timeout": "2", "phases": "both:3,v6:8,v4:8,v6:8,both:6"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| both | 3 | 0 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 52756 | 3..3 (max 3) |
| both | 3 | 1 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 49294 | 3..3 (max 3) |
| v6 | 8 | 0 | 80 | 0 | 80 | 0 | 0 | v6 0.00 | 59905 | 5..3 (max 5) |
| v6 | 8 | 1 | 80 | 0 | 80 | 0 | 0 | v6 0.01 | 46643 | 5..3 (max 5) |
| v4 | 8 | 0 | 80 | 79 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 59905 55736 | 3..3 (max 5) |
| v4 | 8 | 1 | 80 | 79 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 46643 51663 | 3..3 (max 5) |
| v6 | 8 | 0 | 80 | 2 | 77 | 1 | 1 | v4 0.00, v6 0.20 | 55736 45106 | 3..3 (max 5) |
| v6 | 8 | 1 | 80 | 2 | 78 | 0 | 0 | v4 0.00, v6 0.20 | 51663 47879 | 3..3 (max 5) |
| both | 6 | 0 | 60 | 58 | 2 | 0 | 0 | v4 0.20, v6 0.00 | 45106 33238 | 3..3 (max 5) |
| both | 6 | 1 | 60 | 58 | 2 | 0 | 0 | v4 0.20, v6 0.00 | 47879 40668 | 3..3 (max 5) |

log: 10 `new association` lines, 8 `remove association` lines; resolution changes: 127.0.0.1:59927 -> [::1]:59927 -> 127.0.0.1:59927 -> [::1]:59927 -> 127.0.0.1:59927
alive at end: True

### new-v4first-timeout0 (binary new, options {"udp_timeout": "0", "phases": "v4:3,v6:12,v4:6,both:6"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| v4 | 3 | 0 | 30 | 30 | 0 | 0 | 0 | v4 0.01 | 32978 | 3..3 (max 3) |
| v4 | 3 | 1 | 30 | 30 | 0 | 0 | 0 | v4 0.01 | 49184 | 3..3 (max 3) |
| v6 | 12 | 0 | 120 | 0 | 120 | 0 | 0 | v6 0.01 | 56285 | 5..5 (max 5) |
| v6 | 12 | 1 | 120 | 0 | 120 | 0 | 0 | v6 0.01 | 44965 | 5..5 (max 5) |
| v4 | 6 | 0 | 60 | 60 | 0 | 0 | 0 | v4 0.00 | 56267 | 7..7 (max 7) |
| v4 | 6 | 1 | 60 | 60 | 0 | 0 | 0 | v4 0.00 | 51896 | 7..7 (max 7) |
| both | 6 | 0 | 60 | 60 | 0 | 0 | 0 | v4 0.00 | 56267 | 7..7 (max 7) |
| both | 6 | 1 | 60 | 60 | 0 | 0 | 0 | v4 0.00 | 51896 | 7..7 (max 7) |

log: 6 `new association` lines, 0 `remove association` lines; resolution changes: 127.0.0.1:42831 -> [::1]:42831 -> 127.0.0.1:42831
alive at end: True

### new-flap-timeout0 (binary new, options {"udp_timeout": "0", "phases": "v4:2,v6:2,v4:2,v6:2,v4:2,v6:2,v4:2,v6:2,v4:2,v6:2,both:4"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| v4 | 2 | 0 | 20 | 20 | 0 | 0 | 0 | v4 0.00 | 52590 | 3..3 (max 3) |
| v4 | 2 | 1 | 20 | 20 | 0 | 0 | 0 | v4 0.00 | 43404 | 3..3 (max 3) |
| v6 | 2 | 0 | 20 | 0 | 20 | 0 | 0 | v6 0.00 | 48217 | 5..5 (max 5) |
| v6 | 2 | 1 | 20 | 0 | 20 | 0 | 0 | v6 0.00 | 38820 | 5..5 (max 5) |
| v4 | 2 | 0 | 20 | 20 | 0 | 0 | 0 | v4 0.00 | 40965 | 7..7 (max 7) |
| v4 | 2 | 1 | 20 | 20 | 0 | 0 | 0 | v4 0.00 | 38712 | 7..7 (max 7) |
| v6 | 2 | 0 | 20 | 0 | 20 | 0 | 0 | v6 0.00 | 53749 | 9..9 (max 9) |
| v6 | 2 | 1 | 20 | 0 | 20 | 0 | 0 | v6 0.00 | 56665 | 9..9 (max 9) |
| v4 | 2 | 0 | 20 | 19 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 53749 42969 | 9..11 (max 11) |
| v4 | 2 | 1 | 20 | 19 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 56665 50148 | 9..11 (max 11) |
| v6 | 2 | 0 | 20 | 1 | 19 | 0 | 0 | v4 0.00, v6 0.10 | 42969 38398 | 11..13 (max 13) |
| v6 | 2 | 1 | 20 | 1 | 19 | 0 | 0 | v4 0.00, v6 0.10 | 50148 60958 | 11..13 (max 13) |
| v4 | 2 | 0 | 20 | 19 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 38398 54499 | 13..15 (max 15) |
| v4 | 2 | 1 | 20 | 19 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 60958 55698 | 13..15 (max 15) |
| v6 | 2 | 0 | 20 | 1 | 19 | 0 | 0 | v4 0.00, v6 0.10 | 54499 59403 | 15..17 (max 17) |
| v6 | 2 | 1 | 20 | 1 | 19 | 0 | 0 | v4 0.00, v6 0.10 | 55698 51432 | 15..17 (max 17) |
| v4 | 2 | 0 | 20 | 19 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 59403 36909 | 17..19 (max 19) |
| v4 | 2 | 1 | 20 | 19 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 51432 49347 | 17..19 (max 19) |
| v6 | 2 | 0 | 20 | 1 | 19 | 0 | 0 | v4 0.00, v6 0.10 | 36909 55064 | 19..21 (max 21) |
| v6 | 2 | 1 | 20 | 1 | 19 | 0 | 0 | v4 0.00, v6 0.10 | 49347 46769 | 19..21 (max 21) |
| both | 4 | 0 | 40 | 39 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 55064 33793 | 21..23 (max 23) |
| both | 4 | 1 | 40 | 39 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 46769 49064 | 21..23 (max 23) |

log: 22 `new association` lines, 0 `remove association` lines; resolution changes: 127.0.0.1:43515 -> [::1]:43515 -> 127.0.0.1:43515 -> [::1]:43515 -> 127.0.0.1:43515 -> [::1]:43515 -> 127.0.0.1:43515 -> [::1]:43515 -> 127.0.0.1:43515 -> [::1]:43515 -> 127.0.0.1:43515
alive at end: True

### new-flap-timeout2 (binary new, options {"udp_timeout": "2", "phases": "v4:2,v6:2,v4:2,v6:2,v4:2,v6:2,v4:2,v6:2,v4:2,v6:2,both:8"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| v4 | 2 | 0 | 20 | 20 | 0 | 0 | 0 | v4 0.00 | 37333 | 3..3 (max 3) |
| v4 | 2 | 1 | 20 | 20 | 0 | 0 | 0 | v4 0.00 | 59273 | 3..3 (max 3) |
| v6 | 2 | 0 | 20 | 0 | 20 | 0 | 0 | v6 0.00 | 41906 | 5..5 (max 5) |
| v6 | 2 | 1 | 20 | 0 | 20 | 0 | 0 | v6 0.01 | 34314 | 5..5 (max 5) |
| v4 | 2 | 0 | 20 | 19 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 41906 47374 | 3..5 (max 5) |
| v4 | 2 | 1 | 20 | 19 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 34314 36819 | 3..5 (max 5) |
| v6 | 2 | 0 | 20 | 1 | 19 | 0 | 0 | v4 0.00, v6 0.10 | 47374 54424 | 5..5 (max 5) |
| v6 | 2 | 1 | 20 | 1 | 19 | 0 | 0 | v4 0.00, v6 0.10 | 36819 38453 | 5..5 (max 5) |
| v4 | 2 | 0 | 20 | 19 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 54424 46036 | 5..5 (max 5) |
| v4 | 2 | 1 | 20 | 19 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 38453 48042 | 5..5 (max 5) |
| v6 | 2 | 0 | 20 | 1 | 19 | 0 | 0 | v4 0.00, v6 0.10 | 46036 45197 | 5..5 (max 5) |
| v6 | 2 | 1 | 20 | 1 | 19 | 0 | 0 | v4 0.00, v6 0.10 | 48042 50411 | 5..5 (max 5) |
| v4 | 2 | 0 | 20 | 19 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 45197 44271 | 5..5 (max 5) |
| v4 | 2 | 1 | 20 | 19 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 50411 43432 | 5..5 (max 5) |
| v6 | 2 | 0 | 20 | 1 | 19 | 0 | 0 | v4 0.00, v6 0.10 | 44271 50539 | 5..5 (max 5) |
| v6 | 2 | 1 | 20 | 1 | 19 | 0 | 0 | v4 0.00, v6 0.10 | 43432 60781 | 5..5 (max 5) |
| v4 | 2 | 0 | 20 | 19 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 50539 54798 | 5..5 (max 5) |
| v4 | 2 | 1 | 20 | 19 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 60781 46236 | 5..5 (max 5) |
| v6 | 2 | 0 | 20 | 1 | 19 | 0 | 0 | v4 0.00, v6 0.10 | 54798 56381 | 5..5 (max 5) |
| v6 | 2 | 1 | 20 | 1 | 19 | 0 | 0 | v4 0.00, v6 0.10 | 46236 39410 | 5..5 (max 5) |
| both | 8 | 0 | 80 | 79 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 56381 33144 | 5..3 (max 5) |
| both | 8 | 1 | 80 | 79 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 39410 37325 | 5..3 (max 5) |

log: 22 `new association` lines, 20 `remove association` lines; resolution changes: 127.0.0.1:49001 -> [::1]:49001 -> 127.0.0.1:49001 -> [::1]:49001 -> 127.0.0.1:49001 -> [::1]:49001 -> 127.0.0.1:49001 -> [::1]:49001 -> 127.0.0.1:49001 -> [::1]:49001 -> 127.0.0.1:49001
alive at end: True

### new-mode-6then4 (binary new, options {"mode": "ipv6_then_ipv4", "phases": "both:3,v4:6,v6:6"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| both | 3 | 0 | 30 | 0 | 30 | 0 | 0 | v6 0.00 | 42211 | 3..3 (max 3) |
| both | 3 | 1 | 30 | 0 | 30 | 0 | 0 | v6 0.00 | 60838 | 3..3 (max 3) |
| v4 | 6 | 0 | 60 | 59 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 42211 58319 | 4..5 (max 5) |
| v4 | 6 | 1 | 60 | 60 | 0 | 0 | 0 | v4 0.00 | 40530 | 4..5 (max 5) |
| v6 | 6 | 0 | 60 | 0 | 60 | 0 | 0 | v6 0.00 | 40660 | 7..7 (max 7) |
| v6 | 6 | 1 | 60 | 0 | 60 | 0 | 0 | v6 0.00 | 49235 | 7..7 (max 7) |

log: 6 `new association` lines, 0 `remove association` lines; resolution changes: [::1]:39612 -> 127.0.0.1:39612 -> [::1]:39612
alive at end: True

### new-nodata (binary new, options {"phases": "both:3,none:5,v6:5,none:4,both:3"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| both | 3 | 0 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 57298 | 3..3 (max 3) |
| both | 3 | 1 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 57769 | 3..3 (max 3) |
| none | 5 | 0 | 50 | 50 | 0 | 0 | 0 | v4 0.00 | 57298 | 3..3 (max 3) |
| none | 5 | 1 | 50 | 50 | 0 | 0 | 0 | v4 0.00 | 57769 | 3..3 (max 3) |
| v6 | 5 | 0 | 50 | 0 | 50 | 0 | 0 | v6 0.00 | 33377 | 5..5 (max 5) |
| v6 | 5 | 1 | 50 | 0 | 50 | 0 | 0 | v6 0.00 | 49895 | 5..5 (max 5) |
| none | 4 | 0 | 40 | 0 | 40 | 0 | 0 | v6 0.00 | 33377 | 5..5 (max 5) |
| none | 4 | 1 | 40 | 0 | 40 | 0 | 0 | v6 0.01 | 49895 | 5..5 (max 5) |
| both | 3 | 0 | 30 | 30 | 0 | 0 | 0 | v4 0.01 | 42733 | 7..7 (max 7) |
| both | 3 | 1 | 30 | 30 | 0 | 0 | 0 | v4 0.01 | 41698 | 7..7 (max 7) |

log: 6 `new association` lines, 0 `remove association` lines; resolution changes: 127.0.0.1:46251 -> [::1]:46251 -> 127.0.0.1:46251

log warnings/errors (first 3): [WARN][udp]failed to resolve target.rf02.test:46251: DNS error: no records found for Query { name: Name("target.rf02.test."), query_type: A, query_class: IN } (last address: Some(127.0.0.1:46251), 0 suppressed) ;; [WARN][udp]failed to resolve target.rf02.test:46251: DNS error: no records found for Query { name: Name("target.rf02.test."), query_type: A, query_class: IN } (last address: Some(127.0.0.1:46251), 19 suppressed) ;; [WARN][udp]failed to resolve target.rf02.test:46251: DNS error: no records found for Query { name: Name("target.rf02.test."), query_type: A, query_class: IN } (last address: Some(127.0.0.1:46251), 19 suppressed)
alive at end: True

### dbg-default (binary dbg, options {"phases": "both:3,v6:6,v4:6,both:4"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| both | 3 | 0 | 30 | 30 | 0 | 0 | 0 | v4 0.01 | 53330 | 3..3 (max 3) |
| both | 3 | 1 | 30 | 30 | 0 | 0 | 0 | v4 0.01 | 60960 | 3..3 (max 3) |
| v6 | 6 | 0 | 60 | 0 | 60 | 0 | 0 | v6 0.01 | 34104 | 5..5 (max 5) |
| v6 | 6 | 1 | 60 | 0 | 60 | 0 | 0 | v6 0.01 | 54978 | 5..5 (max 5) |
| v4 | 6 | 0 | 60 | 58 | 2 | 0 | 0 | v4 0.20, v6 0.00 | 34104 37522 | 5..7 (max 7) |
| v4 | 6 | 1 | 60 | 58 | 2 | 0 | 0 | v4 0.21, v6 0.01 | 54978 41257 | 5..7 (max 7) |
| both | 4 | 0 | 40 | 40 | 0 | 0 | 0 | v4 0.00 | 37522 | 7..7 (max 7) |
| both | 4 | 1 | 40 | 40 | 0 | 0 | 0 | v4 0.01 | 41257 | 7..7 (max 7) |

log: 6 `new association` lines, 0 `remove association` lines; resolution changes: 127.0.0.1:44882 -> [::1]:44882 -> 127.0.0.1:44882
alive at end: True

### dbg-timeout0 (binary dbg, options {"udp_timeout": "0", "phases": "both:3,v6:6,v4:6,both:4"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| both | 3 | 0 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 59409 | 3..3 (max 3) |
| both | 3 | 1 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 39397 | 3..3 (max 3) |
| v6 | 6 | 0 | 60 | 1 | 59 | 0 | 0 | v4 0.00, v6 0.10 | 59409 60591 | 3..5 (max 5) |
| v6 | 6 | 1 | 60 | 1 | 59 | 0 | 0 | v4 0.00, v6 0.10 | 39397 33847 | 3..5 (max 5) |
| v4 | 6 | 0 | 59 | 58 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 60591 35539 | 5..7 (max 7) |
| v4 | 6 | 1 | 59 | 58 | 1 | 0 | 0 | v4 0.10, v6 0.00 | 33847 48629 | 5..7 (max 7) |
| both | 4 | 0 | 40 | 40 | 0 | 0 | 0 | v4 0.00 | 35539 | 7..7 (max 7) |
| both | 4 | 1 | 40 | 40 | 0 | 0 | 0 | v4 0.01 | 48629 | 7..7 (max 7) |

log: 6 `new association` lines, 0 `remove association` lines; resolution changes: 127.0.0.1:50911 -> [::1]:50911 -> 127.0.0.1:50911
alive at end: True

### old-default (binary old, options {"phases": "both:3,v6:8,v4:8,v6:8,both:6"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| both | 3 | 0 | 30 | 0 | 30 | 0 | 0 | v6 0.00 | 55920 | 3..3 (max 3) |
| both | 3 | 1 | 30 | 0 | 30 | 0 | 0 | v6 0.00 | 46907 | 3..3 (max 3) |
| v6 | 8 | 0 | 80 | 0 | 80 | 0 | 0 | v6 0.00 | 55920 | 3..3 (max 3) |
| v6 | 8 | 1 | 80 | 0 | 80 | 0 | 0 | v6 0.00 | 46907 | 3..3 (max 3) |
| v4 | 8 | 0 | 80 | 80 | 0 | 0 | 0 | v4 0.00 | 55920 | 3..3 (max 3) |
| v4 | 8 | 1 | 80 | 80 | 0 | 0 | 0 | v4 0.00 | 46907 | 3..3 (max 3) |
| v6 | 8 | 0 | 80 | 0 | 80 | 0 | 0 | v6 0.00 | 55920 | 3..3 (max 3) |
| v6 | 8 | 1 | 80 | 0 | 80 | 0 | 0 | v6 0.00 | 46907 | 3..3 (max 3) |
| both | 6 | 0 | 60 | 0 | 60 | 0 | 0 | v6 0.00 | 55920 | 3..3 (max 3) |
| both | 6 | 1 | 60 | 0 | 60 | 0 | 0 | v6 0.00 | 46907 | 3..3 (max 3) |

log: 2 `new association` lines, 0 `remove association` lines; resolution changes: [::1]:42375 -> 127.0.0.1:42375 -> [::1]:42375
alive at end: True

### old-timeout0 (binary old, options {"udp_timeout": "0", "phases": "both:3,v6:8,v4:8,v6:8,both:6"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| both | 3 | 0 | 30 | 0 | 30 | 0 | 0 | v6 0.00 | 42008 | 3..3 (max 3) |
| both | 3 | 1 | 30 | 0 | 30 | 0 | 0 | v6 0.00 | 43477 | 3..3 (max 3) |
| v6 | 8 | 0 | 80 | 0 | 80 | 0 | 0 | v6 0.00 | 42008 | 3..3 (max 3) |
| v6 | 8 | 1 | 80 | 0 | 80 | 0 | 0 | v6 0.00 | 43477 | 3..3 (max 3) |
| v4 | 8 | 0 | 80 | 80 | 0 | 0 | 0 | v4 0.00 | 42008 | 3..3 (max 3) |
| v4 | 8 | 1 | 80 | 80 | 0 | 0 | 0 | v4 0.00 | 43477 | 3..3 (max 3) |
| v6 | 8 | 0 | 80 | 1 | 79 | 0 | 0 | v4 0.00, v6 0.10 | 42008 | 3..3 (max 3) |
| v6 | 8 | 1 | 80 | 1 | 79 | 0 | 0 | v4 0.00, v6 0.10 | 43477 | 3..3 (max 3) |
| both | 6 | 0 | 60 | 0 | 60 | 0 | 0 | v6 0.00 | 42008 | 3..3 (max 3) |
| both | 6 | 1 | 60 | 0 | 60 | 0 | 0 | v6 0.00 | 43477 | 3..3 (max 3) |

log: 2 `new association` lines, 0 `remove association` lines; resolution changes: [::1]:53255 -> 127.0.0.1:53255 -> [::1]:53255
alive at end: True

### old-v4first-default (binary old, options {"phases": "v4:3,v6:40,v4:6,both:6"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| v4 | 3 | 0 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 52985 | 3..3 (max 3) |
| v4 | 3 | 1 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 43069 | 3..3 (max 3) |
| v6 | 40 | 0 | 224 | 0 | 101 | 123 | 0 | v6 29.93 | 54091 | 3..3 (max 3) |
| v6 | 40 | 1 | 224 | 0 | 102 | 122 | 0 | v6 29.93 | 50830 | 3..3 (max 3) |
| v4 | 6 | 0 | 60 | 53 | 7 | 0 | 0 | v4 0.71, v6 0.00 | 54091 | 3..3 (max 3) |
| v4 | 6 | 1 | 60 | 53 | 7 | 0 | 0 | v4 0.71, v6 0.00 | 50830 | 3..3 (max 3) |
| both | 6 | 0 | 60 | 7 | 53 | 0 | 0 | v4 0.00, v6 0.70 | 54091 | 3..3 (max 3) |
| both | 6 | 1 | 60 | 7 | 53 | 0 | 0 | v4 0.00, v6 0.70 | 50830 | 3..3 (max 3) |

log: 2 `new association` lines, 0 `remove association` lines; resolution changes: 127.0.0.1:55333 -> [::1]:55333 -> 127.0.0.1:55333 -> [::1]:55333

log warnings/errors (first 3): [ERROR][udp]failed to sendto [::1]:55333 for 127.0.0.1:34093: Address family not supported by protocol (os error 97) ;; [ERROR][udp]failed to sendto [::1]:55333 for 127.0.0.1:51001: Address family not supported by protocol (os error 97) ;; [ERROR][udp]failed to sendto [::1]:55333 for 127.0.0.1:34093: Address family not supported by protocol (os error 97)
alive at end: True

### old-v4first-timeout0 (binary old, options {"udp_timeout": "0", "phases": "v4:3,v6:12,v4:6,both:6"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| v4 | 3 | 0 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 46370 | 3..3 (max 3) |
| v4 | 3 | 1 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 38540 | 3..3 (max 3) |
| v6 | 12 | 0 | 50 | 0 | 0 | 50 | 0 | - | - | 3..3 (max 3) |
| v6 | 12 | 1 | 50 | 0 | 0 | 50 | 0 | - | - | 3..3 (max 3) |
| v4 | 6 | 0 | 55 | 51 | 0 | 4 | 0 | v4 0.97 | 46370 | 3..3 (max 3) |
| v4 | 6 | 1 | 55 | 51 | 0 | 4 | 0 | v4 0.97 | 38540 | 3..3 (max 3) |
| both | 6 | 0 | 30 | 9 | 0 | 21 | 0 | v4 0.00 | 46370 | 3..3 (max 3) |
| both | 6 | 1 | 30 | 9 | 0 | 21 | 0 | v4 0.00 | 38540 | 3..3 (max 3) |

log: 2 `new association` lines, 0 `remove association` lines; resolution changes: 127.0.0.1:50851 -> [::1]:50851 -> 127.0.0.1:50851 -> [::1]:50851

log warnings/errors (first 3): [ERROR][udp]failed to sendto [::1]:50851 for 127.0.0.1:50502: Address family not supported by protocol (os error 97) ;; [ERROR][udp]failed to sendto [::1]:50851 for 127.0.0.1:53679: Address family not supported by protocol (os error 97) ;; [ERROR][udp]failed to sendto [::1]:50851 for 127.0.0.1:50502: Address family not supported by protocol (os error 97)
alive at end: True

### old-nodata (binary old, options {"phases": "both:3,none:5,v6:5,none:4,both:3"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| both | 3 | 0 | 30 | 0 | 30 | 0 | 0 | v6 0.00 | 35424 | 3..3 (max 3) |
| both | 3 | 1 | 30 | 0 | 30 | 0 | 0 | v6 0.00 | 32868 | 3..3 (max 3) |
| none | 5 | 0 | 21 | 0 | 0 | 21 | 0 | - | - | 3..3 (max 3) |
| none | 5 | 1 | 21 | 0 | 0 | 21 | 0 | - | - | 3..3 (max 3) |
| v6 | 5 | 0 | 49 | 0 | 47 | 2 | 0 | v6 0.37 | 35424 | 3..3 (max 3) |
| v6 | 5 | 1 | 49 | 0 | 48 | 1 | 0 | v6 0.37 | 32868 | 3..3 (max 3) |
| none | 4 | 0 | 19 | 0 | 3 | 16 | 0 | v6 0.00 | 35424 | 3..3 (max 3) |
| none | 4 | 1 | 19 | 0 | 3 | 16 | 0 | v6 0.00 | 32868 | 3..3 (max 3) |
| both | 3 | 0 | 28 | 0 | 26 | 2 | 0 | v6 0.49 | 35424 | 3..3 (max 3) |
| both | 3 | 1 | 28 | 0 | 26 | 2 | 0 | v6 0.49 | 32868 | 3..3 (max 3) |

log: 2 `new association` lines, 0 `remove association` lines; resolution changes: [::1]:47504

log warnings/errors (first 3): [ERROR][udp]error: DNS error: no records found for Query { name: Name("target.rf02.test."), query_type: AAAA, query_class: IN } ;; [ERROR][udp]error: DNS error: no records found for Query { name: Name("target.rf02.test."), query_type: AAAA, query_class: IN } ;; [ERROR][udp]error: DNS error: no records found for Query { name: Name("target.rf02.test."), query_type: AAAA, query_class: IN }
alive at end: True

### new-through-v4 (binary new, options {"through": "127.0.0.1", "tcp": "1", "phases": "both:3,v6:5,v4:4"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| both | 3 | 0 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 38826 | 3..3 (max 3) |
| both | 3 | 1 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 35237 | 3..3 (max 3) |
| | | tcp | | | | | | v4|tcp-ping | | |
| v6 | 5 | 0 | 50 | 50 | 0 | 0 | 0 | v4 0.00 | 38826 | 3..3 (max 3) |
| v6 | 5 | 1 | 50 | 50 | 0 | 0 | 0 | v4 0.00 | 35237 | 3..3 (max 3) |
| | | tcp | | | | | | error: ConnectionResetError(104, 'Connection reset by peer') | | |
| v4 | 4 | 0 | 40 | 40 | 0 | 0 | 0 | v4 0.00 | 38826 | 3..3 (max 3) |
| v4 | 4 | 1 | 40 | 40 | 0 | 0 | 0 | v4 0.00 | 35237 | 3..3 (max 3) |
| | | tcp | | | | | | v4|tcp-ping | | |

log: 2 `new association` lines, 0 `remove association` lines; resolution changes: 127.0.0.1:36676 -> [::1]:36676 -> 127.0.0.1:36676

log warnings/errors (first 3): [WARN][udp]failed to resolve target.rf02.test:36676: entity not found (last address: Some(127.0.0.1:36676), 0 suppressed) ;; [WARN][udp]failed to resolve target.rf02.test:36676: entity not found (last address: Some(127.0.0.1:36676), 19 suppressed) ;; [WARN][udp]failed to resolve target.rf02.test:36676: entity not found (last address: Some(127.0.0.1:36676), 19 suppressed)
alive at end: True

### new-through-v6 (binary new, options {"through": "::1", "tcp": "1", "phases": "both:3,v4:5,v6:4"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| both | 3 | 0 | 30 | 0 | 30 | 0 | 0 | v6 0.00 | 59057 | 3..3 (max 3) |
| both | 3 | 1 | 30 | 0 | 30 | 0 | 0 | v6 0.00 | 43799 | 3..3 (max 3) |
| | | tcp | | | | | | v6|tcp-ping | | |
| v4 | 5 | 0 | 50 | 0 | 50 | 0 | 0 | v6 0.00 | 59057 | 3..3 (max 3) |
| v4 | 5 | 1 | 50 | 0 | 50 | 0 | 0 | v6 0.00 | 43799 | 3..3 (max 3) |
| | | tcp | | | | | | error: ConnectionResetError(104, 'Connection reset by peer') | | |
| v6 | 4 | 0 | 40 | 0 | 40 | 0 | 0 | v6 0.00 | 59057 | 3..3 (max 3) |
| v6 | 4 | 1 | 40 | 0 | 40 | 0 | 0 | v6 0.00 | 43799 | 3..3 (max 3) |
| | | tcp | | | | | | v6|tcp-ping | | |

log: 2 `new association` lines, 0 `remove association` lines; resolution changes: [::1]:36834 -> 127.0.0.1:36834 -> [::1]:36834 -> 127.0.0.1:36834 -> [::1]:36834

log warnings/errors (first 3): [WARN][tcp]connect to target.rf02.test:36834 as 127.0.0.1:36834: Address family not supported by protocol (os error 97), try next ip (0 suppressed) ;; [WARN][udp]failed to resolve target.rf02.test:36834: entity not found (last address: Some([::1]:36834), 0 suppressed) ;; [WARN][udp]failed to resolve target.rf02.test:36834: entity not found (last address: Some([::1]:36834), 19 suppressed)
alive at end: True

### old-through-v4 (binary old, options {"through": "127.0.0.1", "tcp": "1", "phases": "both:3,v6:5,v4:4"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| both | 3 | 0 | 13 | 0 | 0 | 13 | 0 | - | - | 1..1 (max 1) |
| both | 3 | 1 | 13 | 0 | 0 | 13 | 0 | - | - | 1..1 (max 1) |
| | | tcp | | | | | | error: ConnectionResetError(104, 'Connection reset by peer') | | |
| v6 | 5 | 0 | 21 | 0 | 0 | 21 | 0 | - | - | 1..1 (max 1) |
| v6 | 5 | 1 | 21 | 0 | 0 | 21 | 0 | - | - | 1..1 (max 1) |
| | | tcp | | | | | | error: ConnectionResetError(104, 'Connection reset by peer') | | |
| v4 | 4 | 0 | 39 | 37 | 0 | 2 | 0 | v4 0.37 | 34396 | 1..3 (max 3) |
| v4 | 4 | 1 | 39 | 38 | 0 | 1 | 0 | v4 0.37 | 42920 | 1..3 (max 3) |
| | | tcp | | | | | | v4|tcp-ping | | |

log: 2 `new association` lines, 0 `remove association` lines; resolution changes: [::1]:35867 -> 127.0.0.1:35867

log warnings/errors (first 3): [ERROR][udp]failed to associate for 127.0.0.1:33376: Invalid argument (os error 22) ;; [ERROR][udp]failed to associate for 127.0.0.1:59433: Invalid argument (os error 22) ;; [ERROR][udp]failed to associate for 127.0.0.1:33376: Invalid argument (os error 22)
alive at end: True

### old-through-v6 (binary old, options {"through": "::1", "tcp": "1", "phases": "both:3,v4:5,v6:4"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| both | 3 | 0 | 30 | 0 | 30 | 0 | 0 | v6 0.00 | 39100 | 3..3 (max 3) |
| both | 3 | 1 | 30 | 0 | 30 | 0 | 0 | v6 0.00 | 55831 | 3..3 (max 3) |
| | | tcp | | | | | | v6|tcp-ping | | |
| v4 | 5 | 0 | 27 | 0 | 10 | 17 | 0 | v6 0.00 | 39100 | 3..3 (max 3) |
| v4 | 5 | 1 | 27 | 0 | 10 | 17 | 0 | v6 0.00 | 55831 | 3..3 (max 3) |
| | | tcp | | | | | | error: ConnectionResetError(104, 'Connection reset by peer') | | |
| v6 | 4 | 0 | 39 | 0 | 38 | 1 | 0 | v6 0.25 | 39100 | 3..3 (max 3) |
| v6 | 4 | 1 | 39 | 0 | 38 | 1 | 0 | v6 0.25 | 55831 | 3..3 (max 3) |
| | | tcp | | | | | | v6|tcp-ping | | |

log: 2 `new association` lines, 0 `remove association` lines; resolution changes: [::1]:35539 -> 127.0.0.1:35539 -> [::1]:35539

log warnings/errors (first 3): [ERROR][udp]failed to sendto 127.0.0.1:35539 for 127.0.0.1:48520: Network is unreachable (os error 101) ;; [ERROR][udp]failed to sendto 127.0.0.1:35539 for 127.0.0.1:36998: Network is unreachable (os error 101) ;; [ERROR][udp]failed to sendto 127.0.0.1:35539 for 127.0.0.1:48520: Network is unreachable (os error 101)
alive at end: True

### new-failover (binary new, options {"failover": "1", "tcp": "1", "phases": "both:4,v6:3"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| both | 4 | 0 | 40 | 40 | 0 | 0 | 0 | v4 0.00 | 54015 | 3..3 (max 3) |
| both | 4 | 1 | 40 | 40 | 0 | 0 | 0 | v4 0.00 | 55719 | 3..3 (max 3) |
| | | tcp | | | | | | v4|tcp-ping | | |
| v6 | 3 | 0 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 54015 | 3..3 (max 3) |
| v6 | 3 | 1 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 55719 | 3..3 (max 3) |
| | | tcp | | | | | | v4|tcp-ping | | |

log: 2 `new association` lines, 0 `remove association` lines; resolution changes: -

log warnings/errors (first 3): [WARN][failover][udp]127.0.0.1:39620 peer#0 127.0.0.1:56738 is down ;; [WARN][failover][udp]127.0.0.1:39620 peer#1 127.0.0.1:44652 is down
alive at end: True

### dbg-failover (binary dbg, options {"failover": "1", "tcp": "1", "phases": "both:4,v6:3"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| both | 4 | 0 | 40 | 40 | 0 | 0 | 0 | v4 0.00 | 35015 | 3..3 (max 3) |
| both | 4 | 1 | 40 | 40 | 0 | 0 | 0 | v4 0.00 | 56882 | 3..3 (max 3) |
| | | tcp | | | | | | v4|tcp-ping | | |
| v6 | 3 | 0 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 35015 | 3..3 (max 3) |
| v6 | 3 | 1 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 56882 | 3..3 (max 3) |
| | | tcp | | | | | | v4|tcp-ping | | |

log: 2 `new association` lines, 0 `remove association` lines; resolution changes: -

log warnings/errors (first 3): [WARN][failover][udp]127.0.0.1:39620 peer#0 127.0.0.1:45162 is down ;; [WARN][failover][udp]127.0.0.1:39620 peer#1 127.0.0.1:46972 is down
alive at end: True

### old-failover (binary old, options {"failover": "1", "tcp": "1", "phases": "both:4,v6:3"})

| DNS answers | seconds | client | sent | from v4 | from v6 | no reply in 120ms | late replies | first reply per family (s after switch) | relay source ports seen by upstream | realm UDP sockets (first..last) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| both | 4 | 0 | 40 | 40 | 0 | 0 | 0 | v4 0.00 | 56485 | 3..3 (max 3) |
| both | 4 | 1 | 40 | 40 | 0 | 0 | 0 | v4 0.00 | 60959 | 3..3 (max 3) |
| | | tcp | | | | | | v4|tcp-ping | | |
| v6 | 3 | 0 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 56485 | 3..3 (max 3) |
| v6 | 3 | 1 | 30 | 30 | 0 | 0 | 0 | v4 0.00 | 60959 | 3..3 (max 3) |
| | | tcp | | | | | | v4|tcp-ping | | |

log: 2 `new association` lines, 0 `remove association` lines; resolution changes: -

log warnings/errors (first 3): [WARN][failover][udp]127.0.0.1:39620 peer#0 127.0.0.1:49039 is down ;; [WARN][failover][udp]127.0.0.1:39620 peer#1 127.0.0.1:60982 is down
alive at end: True

