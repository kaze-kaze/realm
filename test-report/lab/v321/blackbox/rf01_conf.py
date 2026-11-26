#!/usr/bin/env python3
"""RF-01: start v3.2.0 and the rc2 builds on the same configs, through the three
entries (-c file, -c dir, REALM_CONF), and compare what takes effect.

usage: python3 -I rf01_conf.py [name-filter]
writes results/rf01.json and results/rf01.md
"""
import json
import os
import re
import shutil
import socket
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import common as c

L = "127.0.0.1:39600"
L2 = "127.0.0.1:39601"
R = "127.0.0.1:39690"
R2 = "127.0.0.1:39691"

J = json.dumps

# old positional order of `network` (v3.2.0 src/conf/net.rs):
# no_tcp, use_udp, ipv6_only, send_mptcp, accept_mptcp, send_proxy, accept_proxy,
# send_proxy_version, accept_proxy_timeout, tcp_keepalive, tcp_keepalive_probe,
# tcp_timeout, udp_timeout, udp_no_fragment
NET14 = [False, True, True, None, None, False, True, 1, 7, 20, 4, 6, 17, True]
NET14_TOML = "[false, true, true, false, false, false, true, 1, 7, 20, 4, 6, 17, true]"
EP11 = [L, R, [R2], "roundrobin: 2, 1", None, "127.0.0.1", None, None, None, None, {"use_udp": True, "udp_timeout": 9}]

SAMPLES = [
    # ---- mapping forms -------------------------------------------------
    ("map_toml_basic", "toml", f"""
[log]
level = "warn"
output = "stderr"

[dns]
mode = "ipv4_only"
min_ttl = 1
max_ttl = 60
cache_size = 16
protocol = "udp"
nameservers = ["127.0.0.1:53"]

[network]
no_tcp = false
use_udp = true
ipv6_only = true
send_proxy = false
accept_proxy = true
send_proxy_version = 1
accept_proxy_timeout = 7
tcp_keepalive = 20
tcp_keepalive_probe = 4
tcp_timeout = 6
udp_timeout = 17
udp_no_fragment = true

[[endpoints]]
listen = "{L}"
remote = "{R}"

[[endpoints]]
listen = "{L2}"
remote = "{R}"
extra_remotes = ["{R2}"]
balance = "roundrobin: 2, 1"
through = "127.0.0.1"

[endpoints.network]
udp_timeout = 9
use_udp = false
"""),
    ("map_json_basic", "json", J({
        "log": {"level": "warn", "output": "stderr"},
        "dns": {"mode": "ipv4_only", "min_ttl": 1, "max_ttl": 60, "cache_size": 16, "protocol": "udp",
                "nameservers": ["127.0.0.1:53"]},
        "network": {"no_tcp": False, "use_udp": True, "ipv6_only": True, "send_proxy": False, "accept_proxy": True,
                    "send_proxy_version": 1, "accept_proxy_timeout": 7, "tcp_keepalive": 20,
                    "tcp_keepalive_probe": 4, "tcp_timeout": 6, "udp_timeout": 17, "udp_no_fragment": True},
        "endpoints": [
            {"listen": L, "remote": R},
            {"listen": L2, "remote": R, "extra_remotes": [R2], "balance": "roundrobin: 2, 1",
             "through": "127.0.0.1", "network": {"udp_timeout": 9, "use_udp": False}},
        ],
    })),
    ("map_json_minimal", "json", J({"endpoints": [{"listen": L, "remote": R}]})),
    ("map_toml_minimal", "toml", f'[[endpoints]]\nlisten = "{L}"\nremote = "{R}"\n'),
    ("map_toml_inline", "toml", f'endpoints = [{{ listen = "{L}", remote = "{R}", network = {{ use_udp = true, udp_timeout = 3 }} }}]\n'),
    ("map_json_unknown_keys", "json", J({
        "comment": "top", "log": {"level": "warn", "colour": True},
        "dns": {"mode": "ipv4_only", "resolver": {"deep": [1, {"x": 2}]}},
        "network": {"use_udp": True, "use_udpp": True},
        "endpoints": [{"listen": L, "remote": R, "remark": "x", "network": {"udp_timeout": 9, "udp_timeoutt": 1}}],
    })),
    ("map_toml_unknown_keys", "toml", f"""
comment = "top"
[log]
level = "warn"
colour = true
[dns]
mode = "ipv4_only"
[dns.resolver]
deep = [1, {{ x = 2 }}]
[network]
use_udp = true
use_udpp = true
[[endpoints]]
listen = "{L}"
remote = "{R}"
remark = "x"
[endpoints.network]
udp_timeout = 9
udp_timeoutt = 1
"""),
    ("map_json_new_keys", "json", J({
        "network": {"half_close_timeout": 3, "tcp_user_timeout": 11},
        "endpoints": [{"listen": L, "remote": R}],
    })),
    ("map_json_dup_known_endpoint", "json",
     '{"endpoints":[{"listen":"%s","listen":"%s","remote":"%s"}]}' % (L, L2, R)),
    ("map_json_dup_known_top", "json",
     '{"network":{"use_udp":true},"network":{"use_udp":false},"endpoints":[{"listen":"%s","remote":"%s"}]}' % (L, R)),
    ("map_json_dup_known_network", "json",
     '{"network":{"use_udp":true,"use_udp":false},"endpoints":[{"listen":"%s","remote":"%s"}]}' % (L, R)),
    ("map_json_dup_known_log", "json",
     '{"log":{"level":"warn","level":"info"},"endpoints":[{"listen":"%s","remote":"%s"}]}' % (L, R)),
    ("map_json_dup_known_dns", "json",
     '{"dns":{"mode":"ipv4_only","mode":"ipv6_only"},"endpoints":[{"listen":"%s","remote":"%s"}]}' % (L, R)),
    ("map_json_dup_endpoints", "json",
     '{"endpoints":[{"listen":"%s","remote":"%s"}],"endpoints":[{"listen":"%s","remote":"%s"}]}' % (L, R, L2, R)),
    ("map_json_dup_unknown", "json",
     '{"x":1,"x":{"y":2},"network":{"zz":1,"zz":2},"endpoints":[{"listen":"%s","remote":"%s","k":1,"k":2}]}' % (L, R)),
    ("map_toml_dup_known", "toml", f'[network]\nuse_udp = true\nuse_udp = false\n[[endpoints]]\nlisten = "{L}"\nremote = "{R}"\n'),
    ("map_toml_dup_unknown", "toml", f'x = 1\nx = 2\n[[endpoints]]\nlisten = "{L}"\nremote = "{R}"\n'),
    ("map_json_missing_remote", "json", J({"endpoints": [{"listen": L}]})),
    ("map_json_missing_endpoints", "json", J({"network": {"use_udp": True}})),
    ("map_json_null_tables", "json", J({"log": None, "endpoints": [{"listen": L, "remote": R}]})),
    ("map_json_null_network", "json", J({"endpoints": [{"listen": L, "remote": R, "network": None}]})),
    ("map_json_null_options", "json", J({"endpoints": [{"listen": L, "remote": R, "through": None, "balance": None,
                                                         "failover": None, "extra_remotes": []}]})),
    ("map_json_wrong_type", "json", J({"network": {"udp_timeout": "17"}, "endpoints": [{"listen": L, "remote": R}]})),
    ("map_json_failover", "json", J({"endpoints": [{"listen": L, "remote": R, "extra_remotes": [R2],
                                                    "failover": {"interval": 1.0, "fall": 2}}]})),
    ("map_json_failover_unknown_key", "json", J({"endpoints": [{"listen": L, "remote": R, "extra_remotes": [R2],
                                                                "failover": {"interval": 1.0, "falll": 2}}]})),
    ("legacy_json", "json", J({"listening_addresses": ["127.0.0.1"], "listening_ports": ["39600-39601"],
                               "remote_addresses": ["127.0.0.1"], "remote_ports": ["39690"]})),
    ("legacy_json_positional", "json", J([["127.0.0.1"], ["39600"], ["127.0.0.1"], ["39690"]])),

    # ---- endpoints written as positional arrays ---------------------------
    ("pos_ep_json_2", "json", J({"endpoints": [[L, R]]})),
    ("pos_ep_toml_2", "toml", f'endpoints = [["{L}", "{R}"]]\n'),
    ("pos_ep_json_3_extra", "json", J({"endpoints": [[L, R, [R2]]]})),
    ("pos_ep_toml_4_balance", "toml", f'endpoints = [["{L}", "{R}", ["{R2}"], "roundrobin: 2, 1"]]\n'),
    ("pos_ep_json_11_full", "json", J({"endpoints": [EP11]})),
    ("pos_ep_json_11_network_positional", "json", J({"endpoints": [EP11[:10] + [NET14]]})),
    ("pos_ep_json_failover_map", "json", J({"endpoints": [[L, R, [R2], None, {"interval": 1.0, "fall": 2}]]})),
    ("pos_ep_json_failover_positional", "json",
     J({"endpoints": [[L, R, [R2], None, [[], None, None, None, None, 1.0, 0.5, 2]]]})),
    ("pos_ep_json_failover_unknown_key", "json", J({"endpoints": [[L, R, [R2], None, {"interval": 1.0, "falll": 2}]]})),
    ("pos_ep_json_mixed_with_map", "json", J({"network": {"use_udp": True},
                                               "endpoints": [[L, R], {"listen": L2, "remote": R}]})),
    ("pos_ep_json_nested_map_unknown", "json",
     J({"endpoints": [EP11[:10] + [{"udp_timeout": 23, "use_udpp": True, "meta": {"a": {"b": [1, 2]}}}]]})),
    ("pos_ep_toml_nested_map_unknown", "toml",
     f'endpoints = [["{L}", "{R}", ["{R2}"], "roundrobin: 2, 1", {{}}, "127.0.0.1", "lo", "lo", "", "", {{ udp_timeout = 23, use_udpp = true }}]]\n'),
    ("pos_ep_json_1_too_few", "json", J({"endpoints": [[L]]})),
    ("pos_ep_json_0_too_few", "json", J({"endpoints": [[]]})),
    ("pos_ep_toml_1_too_few", "toml", f'endpoints = [["{L}"]]\n'),
    ("pos_ep_json_12_too_many", "json", J({"endpoints": [EP11 + [None]]})),
    ("pos_ep_json_12_too_many_b", "json", J({"endpoints": [EP11 + [{"x": 1}]]})),
    ("pos_ep_toml_12_too_many", "toml",
     f'endpoints = [["{L}", "{R}", ["{R2}"], "roundrobin: 2, 1", {{}}, "127.0.0.1", "lo", "lo", "", "", {{ udp_timeout = 23 }}, 1]]\n'),
    ("pos_ep_json_null_required", "json", J({"endpoints": [[None, R]]})),
    ("pos_ep_json_wrong_type", "json", J({"endpoints": [[L, R, "not-a-list"]]})),
    ("pos_ep_json_string", "json", J({"endpoints": [L]})),

    # ---- network written as a positional array -------------------------
    ("pos_net_json_14", "json", J({"network": NET14, "endpoints": [{"listen": L, "remote": R}]})),
    ("pos_net_toml_14", "toml", f'network = {NET14_TOML}\n[[endpoints]]\nlisten = "{L}"\nremote = "{R}"\n'),
    ("pos_net_json_13_udp_timeout_last", "json", J({"network": NET14[:13], "endpoints": [{"listen": L, "remote": R}]})),
    ("pos_net_json_2", "json", J({"network": [False, True], "endpoints": [{"listen": L, "remote": R}]})),
    ("pos_net_json_0", "json", J({"network": [], "endpoints": [{"listen": L, "remote": R}]})),
    ("pos_net_toml_2", "toml", f'network = [false, true]\n[[endpoints]]\nlisten = "{L}"\nremote = "{R}"\n'),
    ("pos_net_json_15_one_more", "json", J({"network": NET14 + [3], "endpoints": [{"listen": L, "remote": R}]})),
    ("pos_net_json_16_two_more", "json", J({"network": NET14 + [3, 11], "endpoints": [{"listen": L, "remote": R}]})),
    ("pos_net_json_17_too_many", "json", J({"network": NET14 + [3, 11, 1], "endpoints": [{"listen": L, "remote": R}]})),
    ("pos_net_toml_15_one_more", "toml",
     f'network = {NET14_TOML[:-1]}, 3]\n[[endpoints]]\nlisten = "{L}"\nremote = "{R}"\n'),
    ("pos_net_toml_17_too_many", "toml",
     f'network = {NET14_TOML[:-1]}, 3, 11, 1]\n[[endpoints]]\nlisten = "{L}"\nremote = "{R}"\n'),
    ("pos_net_json_wrong_type_slot12", "json",
     J({"network": NET14[:12] + [True, True], "endpoints": [{"listen": L, "remote": R}]})),
    ("pos_net_json_wrong_type_slot13", "json",
     J({"network": NET14[:13] + [5], "endpoints": [{"listen": L, "remote": R}]})),
    ("pos_net_json_nested_map", "json",
     J({"network": [False, True, {"x": 1}], "endpoints": [{"listen": L, "remote": R}]})),
    ("pos_net_in_map_endpoint", "json", J({"endpoints": [{"listen": L, "remote": R, "network": NET14}]})),
    ("pos_net_global_and_endpoint", "json",
     J({"network": NET14, "endpoints": [{"listen": L, "remote": R, "network": [None, None, False] + [None] * 9 + [5]}]})),

    # ---- log / dns written as positional arrays --------------------------
    ("pos_log_json", "json", J({"log": ["warn", "stderr"], "endpoints": [{"listen": L, "remote": R}]})),
    ("pos_log_toml", "toml", f'log = ["warn", "stderr"]\n[[endpoints]]\nlisten = "{L}"\nremote = "{R}"\n'),
    ("pos_log_json_3_too_many", "json", J({"log": ["warn", "stderr", 1], "endpoints": [{"listen": L, "remote": R}]})),
    ("pos_log_json_bad_level", "json", J({"log": ["loud"], "endpoints": [{"listen": L, "remote": R}]})),
    ("pos_dns_json_6", "json", J({"dns": ["ipv4_only", 1, 60, 16, "udp", ["127.0.0.1:53"]],
                                  "endpoints": [{"listen": L, "remote": R}]})),
    ("pos_dns_toml_6", "toml",
     f'dns = ["ipv6_then_ipv4", 1, 60, 16, "tcp", ["127.0.0.1:53"]]\n[[endpoints]]\nlisten = "{L}"\nremote = "{R}"\n'),
    ("pos_dns_json_2", "json", J({"dns": ["ipv6_only", 2], "endpoints": [{"listen": L, "remote": R}]})),
    ("pos_dns_json_7_too_many", "json", J({"dns": ["ipv4_only", 1, 60, 16, "udp", ["127.0.0.1:53"], 1],
                                           "endpoints": [{"listen": L, "remote": R}]})),

    # ---- the top level written as a positional array ----------------------
    ("pos_top_json_all_positional", "json",
     J([["warn", "stderr"], ["ipv4_only", 1, 60], NET14,
        [[L, R], [L2, R, [], None, None, None, None, None, None, None, [None, False]]]])),
    ("pos_top_json_maps_inside", "json",
     J([{"level": "warn", "colour": 1}, {"mode": "ipv4_only", "zzz": [1, {"a": 2}]}, {"use_udp": True, "use_udpp": True},
        [[L, R, [], None, None, None, None, None, None, None, {"udp_timeout": 23, "nested": {"x": {"y": 1}}}],
         {"listen": L2, "remote": R, "comment": "x"}]])),
    ("pos_top_json_empty_tables", "json", J([[], [], [], [[L, R]]])),
    ("pos_top_json_empty_maps", "json", J([{}, {}, {}, [{"listen": L, "remote": R}]])),
    ("pos_top_json_unit_test_sample", "json",
     J([["warn", "stderr"], ["ipv6_then_ipv4", 1],
        [False, True, None, None, None, None, None, None, None, None, None, 5, 17, True, 2],
        [[L, R, [], None, None, None, None, None, None, None, {"udp_timeout": 23, "use_udpp": True}]]])),
    ("pos_top_json_unit_test_sample_old_len", "json",
     J([["warn", "stderr"], ["ipv6_then_ipv4", 1],
        [False, True, None, None, None, None, None, None, None, None, None, 5, 17, True],
        [[L, R, [], None, None, None, None, None, None, None, {"udp_timeout": 23, "use_udpp": True}]]])),
    ("pos_top_json_3_too_few", "json", J([[], [], []])),
    ("pos_top_json_0_too_few", "json", "[]"),
    ("pos_top_json_5_too_many", "json", J([[], [], [], [[L, R]], None])),
    ("pos_top_json_5_too_many_b", "json", J([[], [], [], [[L, R]], {"x": 1}])),
    ("pos_top_json_null_tables", "json", J([None, None, None, [[L, R]]])),
    ("pos_top_json_endpoints_first", "json", J([[[L, R]]])),

    # ---- second pass: trailing elements that TOML never rejected -------------
    # (toml does not complain about unread array elements, so v3.2.0 ignored them)
    ("x_net_toml_15th_is_string", "toml",
     f'network = {NET14_TOML[:-1]}, "note"]\n[[endpoints]]\nlisten = "{L}"\nremote = "{R}"\n'),
    ("x_net_toml_15th_is_bool", "toml",
     f'network = {NET14_TOML[:-1]}, true]\n[[endpoints]]\nlisten = "{L}"\nremote = "{R}"\n'),
    ("x_net_toml_15th_is_negative", "toml",
     f'network = {NET14_TOML[:-1]}, -1]\n[[endpoints]]\nlisten = "{L}"\nremote = "{R}"\n'),
    ("x_net_toml_16th_is_string", "toml",
     f'network = {NET14_TOML[:-1]}, 3, "note"]\n[[endpoints]]\nlisten = "{L}"\nremote = "{R}"\n'),
    ("x_net_toml_17th_is_string", "toml",
     f'network = {NET14_TOML[:-1]}, 3, 0, "note"]\n[[endpoints]]\nlisten = "{L}"\nremote = "{R}"\n'),
    ("x_net_toml_15th_is_zero", "toml",
     f'network = {NET14_TOML[:-1]}, 0]\n[[endpoints]]\nlisten = "{L}"\nremote = "{R}"\n'),
    ("x_ep_net_toml_15th_is_string", "toml",
     f'endpoints = [{{ listen = "{L}", remote = "{R}", network = {NET14_TOML[:-1]}, "note"] }}]\n'),
    ("x_log_toml_3rd_is_string", "toml", f'log = ["warn", "stderr", "note"]\n[[endpoints]]\nlisten = "{L}"\nremote = "{R}"\n'),
    ("x_dns_toml_7th_is_string", "toml",
     f'dns = ["ipv4_only", 1, 60, 16, "udp", ["127.0.0.1:53"], "note"]\n[[endpoints]]\nlisten = "{L}"\nremote = "{R}"\n'),
    ("x_ep_toml_12th_is_string", "toml", f'endpoints = [["{L}", "{R}", [], "off", {{}}, "127.0.0.1", "lo", "lo", "", "", {{}}, "note"]]\n'),
    ("x_ep_json_null_network_slot", "json", J({"endpoints": [[L, R, [], None, None, None, None, None, None, None, None]]})),
    ("x_ep_json_null_extra_remotes_slot", "json", J({"endpoints": [[L, R, None]]})),
    ("x_ep_json_10_elements", "json", J({"endpoints": [[L, R, [], None, None, None, None, None, None, None]]})),
    ("x_failover_toml_12th_is_string", "toml",
     f'endpoints = [{{ listen = "{L}", remote = "{R}", extra_remotes = ["{R2}"], failover = [[], "", "", "", "echo", 1.0, 0.5, 2, 5, 2, 30.0, "note"] }}]\n'),
    ("x_legacy_positional_5", "json", J([["127.0.0.1"], ["39600"], ["127.0.0.1"], ["39690"], 1])),
]

# several files in one directory (only run through `-c dir`)
DIR_SAMPLES = [
    ("dir_positional_global_plus_map_endpoint", {
        "00-global.json": J([["warn", "stderr"], ["ipv4_only", 1, 60], NET14, []]),
        "10-ep.toml": f'[[endpoints]]\nlisten = "{L}"\nremote = "{R}"\n',
        "20-ep.json": J({"endpoints": [[L2, R, [], None, None, None, None, None, None, None, [None, False]]]}),
    }),
    ("dir_two_positional_networks", {
        "a.json": J({"network": [False, True], "endpoints": [[L, R]]}),
        "b.json": J({"network": NET14, "endpoints": [[L2, R]]}),
    }),
    ("dir_one_bad_positional", {
        "a.json": J({"endpoints": [[L, R]]}),
        "b.json": J({"endpoints": [[L2]]}),
    }),
]


def normalize_inited(line):
    return re.sub(r"half-close-timeout=[^,;]*, tcp-user-timeout=[^;]*; ", "", line)


def summarize(out, err):
    lines = out.splitlines()
    return {
        "log": [l for l in lines if l.startswith("log:")],
        "dns": [l for l in lines if l.startswith("dns:")],
        "inited": [l for l in lines if l.startswith("inited:")],
        "other_stdout": [l for l in lines if not l.startswith(("log:", "dns:", "inited:", "fd:"))][:6],
        "stderr": [l for l in err.splitlines() if l.strip()][:12],
    }


def run_one(tag, binary, args, env, expect_ports):
    # 39600-39699 lies inside the ephemeral port range, so another process on this
    # host can hold a listen port for a moment: such a run says nothing, repeat it
    for attempt in range(8):
        res = run_once(tag, binary, args, env, expect_ports)
        if not any("already in use" in l for l in res["stderr"]):
            break
        time.sleep(3)
    res["attempts"] = attempt + 1
    return res


def run_once(tag, binary, args, env, expect_ports):
    out_p = os.path.join(c.TMP, "rf01.out")
    err_p = os.path.join(c.TMP, "rf01.err")
    proc = c.start(binary, args, out_p, err_p, env=env)
    end = time.monotonic() + 6.0
    seen_inited = None
    lis = {"tcp": [], "udp": []}
    state = "timeout"
    while time.monotonic() < end:
        rc = proc.poll()
        if rc is not None:
            state = "exit"
            break
        if seen_inited is None and "inited:" in c.read(out_p):
            seen_inited = time.monotonic()
        if seen_inited is not None:
            lis = c.listeners([proc.pid])
            # give the listeners a moment: the old binary binds after printing
            if time.monotonic() - seen_inited > 0.6:
                state = "running"
                break
        time.sleep(0.05)
    if state == "running":
        lis = c.listeners([proc.pid])
    rc = c.stop(proc)
    res = summarize(c.read(out_p), c.read(err_p))
    res["state"] = state
    res["rc"] = rc if state == "exit" else None
    res["listen"] = lis
    return res


def classify(old, new):
    o_ok = old["state"] == "running"
    n_ok = new["state"] == "running"
    if o_ok and n_ok:
        same = (
            [normalize_inited(x) for x in old["inited"]] == [normalize_inited(x) for x in new["inited"]]
            and old["log"] == new["log"]
            and [re.sub(r"cache-size=32\b", "cache-size=8192", x) for x in old["dns"]] == new["dns"]
            and old["listen"] == new["listen"]
        )
        return "both-start-same" if same else "BOTH-START-DIFFERENT"
    if o_ok and not n_ok:
        return "OLD-ACCEPTS-NEW-REJECTS"
    if not o_ok and n_ok:
        return "old-rejects-new-accepts"
    return "both-reject"


def main():
    flt = sys.argv[1] if len(sys.argv) > 1 else ""
    os.makedirs(c.TMP, exist_ok=True)
    os.makedirs(os.path.join(c.WORK, "results"), exist_ok=True)
    results = []
    jobs = []
    for name, ext, text in SAMPLES:
        if flt and flt not in name:
            continue
        jobs.append((name, "file", {"conf." + ext: text}))
        jobs.append((name, "dir", {"conf." + ext: text}))
        jobs.append((name, "env", {"conf." + ext: text}))
    for name, files in DIR_SAMPLES:
        if flt and flt not in name:
            continue
        jobs.append((name, "dir", files))

    for name, mode, files in jobs:
        d = os.path.join(c.TMP, "rf01-conf")
        shutil.rmtree(d, ignore_errors=True)
        os.makedirs(d)
        for fn, text in files.items():
            with open(os.path.join(d, fn), "w") as f:
                f.write(text)
        first = sorted(files)[0]
        if mode == "file":
            args, env = ["-c", os.path.join(d, first)], None
        elif mode == "dir":
            args, env = ["-c", d], None
        else:
            args, env = [], {"REALM_CONF": files[first]}
        row = {"name": name, "mode": mode, "files": files}
        for tag in ("old", "new", "dbg"):
            row[tag] = run_one(tag, c.BINS[tag], args, env, None)
        row["verdict"] = classify(row["old"], row["new"])
        row["verdict_dbg"] = classify(row["old"], row["dbg"])
        results.append(row)
        print("%-44s %-4s %-26s old=%s/%s new=%s/%s dbg=%s/%s" % (
            name, mode, row["verdict"], row["old"]["state"], row["old"]["rc"], row["new"]["state"], row["new"]["rc"],
            row["dbg"]["state"], row["dbg"]["rc"]), flush=True)
        shutil.rmtree(d, ignore_errors=True)

    suffix = ("-" + flt) if flt else ""
    with open(os.path.join(c.WORK, "results", "rf01%s.json" % suffix), "w") as f:
        json.dump(results, f, indent=1, ensure_ascii=False)
    c.stop_all()


if __name__ == "__main__":
    try:
        main()
    finally:
        c.stop_all()
