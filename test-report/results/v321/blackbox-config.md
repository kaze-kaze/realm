| sample | entry | v3.2.0 | rc2 | rc2-debug | verdict | listeners old / new | rc2 warnings | reason (old ; new) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| map_toml_basic | file | started | started | started | both-start-same | tcp:39600,39601 udp:39600 / tcp:39600,39601 udp:39600 | 0 | - ; - |
| map_toml_basic | dir | started | started | started | both-start-same | tcp:39600,39601 udp:39600 / tcp:39600,39601 udp:39600 | 0 | - ; - |
| map_toml_basic | env | started | started | started | both-start-same | tcp:39600,39601 udp:39600 / tcp:39600,39601 udp:39600 | 0 | - ; - |
| map_json_basic | file | started | started | started | both-start-same | tcp:39600,39601 udp:39600 / tcp:39600,39601 udp:39600 | 0 | - ; - |
| map_json_basic | dir | started | started | started | both-start-same | tcp:39600,39601 udp:39600 / tcp:39600,39601 udp:39600 | 0 | - ; - |
| map_json_basic | env | started | started | started | both-start-same | tcp:39600,39601 udp:39600 / tcp:39600,39601 udp:39600 | 0 | - ; - |
| map_json_minimal | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| map_json_minimal | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| map_json_minimal | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| map_toml_minimal | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| map_toml_minimal | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| map_toml_minimal | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| map_toml_inline | file | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| map_toml_inline | dir | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| map_toml_inline | env | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| map_json_unknown_keys | file | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 6 | - ; - |
| map_json_unknown_keys | dir | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 6 | - ; - |
| map_json_unknown_keys | env | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 6 | - ; - |
| map_toml_unknown_keys | file | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 6 | - ; - |
| map_toml_unknown_keys | dir | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 6 | - ; - |
| map_toml_unknown_keys | env | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 6 | - ; - |
| map_json_new_keys | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 1 | - ; - |
| map_json_new_keys | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 1 | - ; - |
| map_json_new_keys | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 1 | - ; - |
| map_json_dup_known_endpoint | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | duplicate field `listen` at line 1 column 50 ; duplicate field `listen` at line 1 column 50 |
| map_json_dup_known_endpoint | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | duplicate field `listen` at line 1 column 50 ; duplicate field `listen` at line 1 column 50 |
| map_json_dup_known_endpoint | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | duplicate field `listen` at line 1 column 50 ; duplicate field `listen` at line 1 column 50 |
| map_json_dup_known_top | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | duplicate field `network` at line 1 column 37 ; duplicate field `network` at line 1 column 37 |
| map_json_dup_known_top | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | duplicate field `network` at line 1 column 37 ; duplicate field `network` at line 1 column 37 |
| map_json_dup_known_top | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | duplicate field `network` at line 1 column 37 ; duplicate field `network` at line 1 column 37 |
| map_json_dup_known_network | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | duplicate field `use_udp` at line 1 column 36 ; duplicate field `use_udp` at line 1 column 36 |
| map_json_dup_known_network | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | duplicate field `use_udp` at line 1 column 36 ; duplicate field `use_udp` at line 1 column 36 |
| map_json_dup_known_network | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | duplicate field `use_udp` at line 1 column 36 ; duplicate field `use_udp` at line 1 column 36 |
| map_json_dup_known_log | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | duplicate field `level` at line 1 column 30 ; duplicate field `level` at line 1 column 30 |
| map_json_dup_known_log | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | duplicate field `level` at line 1 column 30 ; duplicate field `level` at line 1 column 30 |
| map_json_dup_known_log | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | duplicate field `level` at line 1 column 30 ; duplicate field `level` at line 1 column 30 |
| map_json_dup_known_dns | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | duplicate field `mode` at line 1 column 33 ; duplicate field `mode` at line 1 column 33 |
| map_json_dup_known_dns | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | duplicate field `mode` at line 1 column 33 ; duplicate field `mode` at line 1 column 33 |
| map_json_dup_known_dns | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | duplicate field `mode` at line 1 column 33 ; duplicate field `mode` at line 1 column 33 |
| map_json_dup_endpoints | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | duplicate field `endpoints` at line 1 column 82 ; duplicate field `endpoints` at line 1 column 82 |
| map_json_dup_endpoints | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | duplicate field `endpoints` at line 1 column 82 ; duplicate field `endpoints` at line 1 column 82 |
| map_json_dup_endpoints | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | duplicate field `endpoints` at line 1 column 82 ; duplicate field `endpoints` at line 1 column 82 |
| map_json_dup_unknown | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 3 | - ; - |
| map_json_dup_unknown | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 3 | - ; - |
| map_json_dup_unknown | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 3 | - ; - |
| map_toml_dup_known | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | TOML parse error at line 3, column 1 duplicate key ; TOML parse error at line 3, column 1 duplicate key |
| map_toml_dup_known | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | TOML parse error at line 3, column 1 duplicate key ; TOML parse error at line 3, column 1 duplicate key |
| map_toml_dup_known | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | TOML parse error at line 3, column 1 duplicate key ; TOML parse error at line 3, column 1 duplicate key |
| map_toml_dup_unknown | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | TOML parse error at line 2, column 1 duplicate key ; TOML parse error at line 2, column 1 duplicate key |
| map_toml_dup_unknown | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | TOML parse error at line 2, column 1 duplicate key ; TOML parse error at line 2, column 1 duplicate key |
| map_toml_dup_unknown | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | TOML parse error at line 2, column 1 duplicate key ; TOML parse error at line 2, column 1 duplicate key |
| map_json_missing_remote | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | missing field `remote` at line 1 column 44 ; missing field `remote` at line 1 column 44 |
| map_json_missing_remote | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | missing field `remote` at line 1 column 44 ; missing field `remote` at line 1 column 44 |
| map_json_missing_remote | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | missing field `remote` at line 1 column 44 ; missing field `remote` at line 1 column 44 |
| map_json_missing_endpoints | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | missing field `endpoints` at line 1 column 30 ; missing field `endpoints` at line 1 column 30 |
| map_json_missing_endpoints | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | missing field `endpoints` at line 1 column 30 ; missing field `endpoints` at line 1 column 30 |
| map_json_missing_endpoints | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | missing field `endpoints` at line 1 column 30 ; missing field `endpoints` at line 1 column 30 |
| map_json_null_tables | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: null, expected struct LogConf at line 1 column 12 ; invalid type: null, expected a configuration table at line 1 column 12 |
| map_json_null_tables | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: null, expected struct LogConf at line 1 column 12 ; invalid type: null, expected a configuration table at line 1 column 12 |
| map_json_null_tables | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | invalid type: null, expected struct LogConf at line 1 column 12 ; invalid type: null, expected a configuration table at line 1 column 12 |
| map_json_null_network | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: null, expected struct NetConf at line 1 column 89 ; invalid type: null, expected a configuration table at line 1 column 89 |
| map_json_null_network | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: null, expected struct NetConf at line 1 column 89 ; invalid type: null, expected a configuration table at line 1 column 89 |
| map_json_null_network | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | invalid type: null, expected struct NetConf at line 1 column 89 ; invalid type: null, expected a configuration table at line 1 column 89 |
| map_json_null_options | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| map_json_null_options | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| map_json_null_options | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| map_json_wrong_type | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: string "17", expected usize at line 1 column 32 ; invalid type: string "17", expected usize at line 1 column 32 |
| map_json_wrong_type | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: string "17", expected usize at line 1 column 32 ; invalid type: string "17", expected usize at line 1 column 32 |
| map_json_wrong_type | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | invalid type: string "17", expected usize at line 1 column 32 ; invalid type: string "17", expected usize at line 1 column 32 |
| map_json_failover | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| map_json_failover | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| map_json_failover | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| map_json_failover_unknown_key | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | unknown field `falll`, expected one of `probe`, `tcp_send`, `tcp_expect`, `udp_send`, `udp_probe`, `interval`, `timeout`, `fall`, `window`,  ; unknown field `falll`, expected one of `probe`, `tcp_send`, `tcp_expect`, `udp_send`, `udp_probe`, `interval`, `timeout`, `fall`, `window`,  |
| map_json_failover_unknown_key | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | unknown field `falll`, expected one of `probe`, `tcp_send`, `tcp_expect`, `udp_send`, `udp_probe`, `interval`, `timeout`, `fall`, `window`,  ; unknown field `falll`, expected one of `probe`, `tcp_send`, `tcp_expect`, `udp_send`, `udp_probe`, `interval`, `timeout`, `fall`, `window`,  |
| map_json_failover_unknown_key | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | unknown field `falll`, expected one of `probe`, `tcp_send`, `tcp_expect`, `udp_send`, `udp_probe`, `interval`, `timeout`, `fall`, `window`,  ; unknown field `falll`, expected one of `probe`, `tcp_send`, `tcp_expect`, `udp_send`, `udp_probe`, `interval`, `timeout`, `fall`, `window`,  |
| legacy_json | file | started | started | started | both-start-same | tcp:39600,39601 udp:- / tcp:39600,39601 udp:- | 1 | attention: you are using a legacy config file! ; attention: you are using a legacy config file! |
| legacy_json | dir | started | started | started | both-start-same | tcp:39600,39601 udp:- / tcp:39600,39601 udp:- | 1 | attention: you are using a legacy config file! ; attention: you are using a legacy config file! |
| legacy_json | env | started | started | started | both-start-same | tcp:39600,39601 udp:- / tcp:39600,39601 udp:- | 1 | attention: you are using a legacy config file! ; attention: you are using a legacy config file! |
| legacy_json_positional | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 1 | attention: you are using a legacy config file! ; attention: you are using a legacy config file! |
| legacy_json_positional | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 1 | attention: you are using a legacy config file! ; attention: you are using a legacy config file! |
| legacy_json_positional | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 1 | attention: you are using a legacy config file! ; attention: you are using a legacy config file! |
| pos_ep_json_2 | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_json_2 | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_json_2 | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_toml_2 | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_toml_2 | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_toml_2 | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_json_3_extra | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_json_3_extra | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_json_3_extra | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_toml_4_balance | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_toml_4_balance | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_toml_4_balance | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_json_11_full | file | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_ep_json_11_full | dir | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_ep_json_11_full | env | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_ep_json_11_network_positional | file | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_ep_json_11_network_positional | dir | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_ep_json_11_network_positional | env | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_ep_json_failover_map | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_json_failover_map | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_json_failover_map | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_json_failover_positional | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_json_failover_positional | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_json_failover_positional | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_json_failover_unknown_key | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | unknown field `falll`, expected one of `probe`, `tcp_send`, `tcp_expect`, `udp_send`, `udp_probe`, `interval`, `timeout`, `fall`, `window`,  ; unknown field `falll`, expected one of `probe`, `tcp_send`, `tcp_expect`, `udp_send`, `udp_probe`, `interval`, `timeout`, `fall`, `window`,  |
| pos_ep_json_failover_unknown_key | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | unknown field `falll`, expected one of `probe`, `tcp_send`, `tcp_expect`, `udp_send`, `udp_probe`, `interval`, `timeout`, `fall`, `window`,  ; unknown field `falll`, expected one of `probe`, `tcp_send`, `tcp_expect`, `udp_send`, `udp_probe`, `interval`, `timeout`, `fall`, `window`,  |
| pos_ep_json_failover_unknown_key | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | unknown field `falll`, expected one of `probe`, `tcp_send`, `tcp_expect`, `udp_send`, `udp_probe`, `interval`, `timeout`, `fall`, `window`,  ; unknown field `falll`, expected one of `probe`, `tcp_send`, `tcp_expect`, `udp_send`, `udp_probe`, `interval`, `timeout`, `fall`, `window`,  |
| pos_ep_json_mixed_with_map | file | started | started | started | both-start-same | tcp:39600,39601 udp:39600,39601 / tcp:39600,39601 udp:39600,39601 | 0 | - ; - |
| pos_ep_json_mixed_with_map | dir | started | started | started | both-start-same | tcp:39600,39601 udp:39600,39601 / tcp:39600,39601 udp:39600,39601 | 0 | - ; - |
| pos_ep_json_mixed_with_map | env | started | started | started | both-start-same | tcp:39600,39601 udp:39600,39601 / tcp:39600,39601 udp:39600,39601 | 0 | - ; - |
| pos_ep_json_nested_map_unknown | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 2 | - ; - |
| pos_ep_json_nested_map_unknown | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 2 | - ; - |
| pos_ep_json_nested_map_unknown | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 2 | - ; - |
| pos_ep_toml_nested_map_unknown | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 1 | - ; - |
| pos_ep_toml_nested_map_unknown | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 1 | - ; - |
| pos_ep_toml_nested_map_unknown | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 1 | - ; - |
| pos_ep_json_1_too_few | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid length 1, expected struct EndpointConf with 11 elements at line 1 column 34 ; missing field `remote` at line 1 column 34 |
| pos_ep_json_1_too_few | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid length 1, expected struct EndpointConf with 11 elements at line 1 column 34 ; missing field `remote` at line 1 column 34 |
| pos_ep_json_1_too_few | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | invalid length 1, expected struct EndpointConf with 11 elements at line 1 column 34 ; missing field `remote` at line 1 column 34 |
| pos_ep_json_0_too_few | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid length 0, expected struct EndpointConf with 11 elements at line 1 column 17 ; missing field `listen` at line 1 column 17 |
| pos_ep_json_0_too_few | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid length 0, expected struct EndpointConf with 11 elements at line 1 column 17 ; missing field `listen` at line 1 column 17 |
| pos_ep_json_0_too_few | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | invalid length 0, expected struct EndpointConf with 11 elements at line 1 column 17 ; missing field `listen` at line 1 column 17 |
| pos_ep_toml_1_too_few | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | TOML parse error at line 1, column 14 invalid length 1, expected struct EndpointConf with 11 elements ; TOML parse error at line 1, column 14 missing field `remote` |
| pos_ep_toml_1_too_few | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | TOML parse error at line 1, column 14 invalid length 1, expected struct EndpointConf with 11 elements ; TOML parse error at line 1, column 14 missing field `remote` |
| pos_ep_toml_1_too_few | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | TOML parse error at line 1, column 14 invalid length 1, expected struct EndpointConf with 11 elements ; TOML parse error at line 1, column 14 missing field `remote` |
| pos_ep_json_12_too_many | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | trailing characters at line 1 column 176 ; trailing characters at line 1 column 176 |
| pos_ep_json_12_too_many | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | trailing characters at line 1 column 176 ; trailing characters at line 1 column 176 |
| pos_ep_json_12_too_many | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | trailing characters at line 1 column 176 ; trailing characters at line 1 column 176 |
| pos_ep_json_12_too_many_b | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | trailing characters at line 1 column 176 ; trailing characters at line 1 column 176 |
| pos_ep_json_12_too_many_b | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | trailing characters at line 1 column 176 ; trailing characters at line 1 column 176 |
| pos_ep_json_12_too_many_b | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | trailing characters at line 1 column 176 ; trailing characters at line 1 column 176 |
| pos_ep_toml_12_too_many | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_toml_12_too_many | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_toml_12_too_many | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_ep_json_null_required | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: null, expected a string at line 1 column 20 ; invalid type: null, expected a string at line 1 column 20 |
| pos_ep_json_null_required | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: null, expected a string at line 1 column 20 ; invalid type: null, expected a string at line 1 column 20 |
| pos_ep_json_null_required | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | invalid type: null, expected a string at line 1 column 20 ; invalid type: null, expected a string at line 1 column 20 |
| pos_ep_json_wrong_type | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: string "not-a-list", expected a sequence at line 1 column 66 ; invalid type: string "not-a-list", expected a sequence at line 1 column 66 |
| pos_ep_json_wrong_type | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: string "not-a-list", expected a sequence at line 1 column 66 ; invalid type: string "not-a-list", expected a sequence at line 1 column 66 |
| pos_ep_json_wrong_type | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | invalid type: string "not-a-list", expected a sequence at line 1 column 66 ; invalid type: string "not-a-list", expected a sequence at line 1 column 66 |
| pos_ep_json_string | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: string "127.0.0.1:39600", expected struct EndpointConf at line 1 column 32 ; invalid type: string "127.0.0.1:39600", expected a configuration table at line 1 column 32 |
| pos_ep_json_string | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: string "127.0.0.1:39600", expected struct EndpointConf at line 1 column 32 ; invalid type: string "127.0.0.1:39600", expected a configuration table at line 1 column 32 |
| pos_ep_json_string | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | invalid type: string "127.0.0.1:39600", expected struct EndpointConf at line 1 column 32 ; invalid type: string "127.0.0.1:39600", expected a configuration table at line 1 column 32 |
| pos_net_json_14 | file | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_json_14 | dir | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_json_14 | env | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_toml_14 | file | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_toml_14 | dir | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_toml_14 | env | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_json_13_udp_timeout_last | file | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_json_13_udp_timeout_last | dir | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_json_13_udp_timeout_last | env | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_json_2 | file | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_json_2 | dir | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_json_2 | env | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_json_0 | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_net_json_0 | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_net_json_0 | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_net_toml_2 | file | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_toml_2 | dir | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_toml_2 | env | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_json_15_one_more | file | exit 101 | started | started | old-rejects-new-accepts | - / tcp:39600 udp:39600 | 0 | trailing characters at line 1 column 84 ; - |
| pos_net_json_15_one_more | dir | exit 101 | started | started | old-rejects-new-accepts | - / tcp:39600 udp:39600 | 0 | trailing characters at line 1 column 84 ; - |
| pos_net_json_15_one_more | env | exit 2 | started | started | old-rejects-new-accepts | - / tcp:39600 udp:39600 | 0 | trailing characters at line 1 column 84 ; - |
| pos_net_json_16_two_more | file | exit 101 | started | started | old-rejects-new-accepts | - / tcp:39600 udp:39600 | 1 | trailing characters at line 1 column 84 ; - |
| pos_net_json_16_two_more | dir | exit 101 | started | started | old-rejects-new-accepts | - / tcp:39600 udp:39600 | 1 | trailing characters at line 1 column 84 ; - |
| pos_net_json_16_two_more | env | exit 2 | started | started | old-rejects-new-accepts | - / tcp:39600 udp:39600 | 1 | trailing characters at line 1 column 84 ; - |
| pos_net_json_17_too_many | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | trailing characters at line 1 column 84 ; trailing characters at line 1 column 91 |
| pos_net_json_17_too_many | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | trailing characters at line 1 column 84 ; trailing characters at line 1 column 91 |
| pos_net_json_17_too_many | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | trailing characters at line 1 column 84 ; trailing characters at line 1 column 91 |
| pos_net_toml_15_one_more | file | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_toml_15_one_more | dir | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_toml_15_one_more | env | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_toml_17_too_many | file | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 1 | - ; - |
| pos_net_toml_17_too_many | dir | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 1 | - ; - |
| pos_net_toml_17_too_many | env | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 1 | - ; - |
| pos_net_json_wrong_type_slot12 | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: boolean `true`, expected usize at line 1 column 77 ; invalid type: boolean `true`, expected usize at line 1 column 77 |
| pos_net_json_wrong_type_slot12 | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: boolean `true`, expected usize at line 1 column 77 ; invalid type: boolean `true`, expected usize at line 1 column 77 |
| pos_net_json_wrong_type_slot12 | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | invalid type: boolean `true`, expected usize at line 1 column 77 ; invalid type: boolean `true`, expected usize at line 1 column 77 |
| pos_net_json_wrong_type_slot13 | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: integer `5`, expected a boolean at line 1 column 78 ; invalid type: integer `5`, expected a boolean at line 1 column 78 |
| pos_net_json_wrong_type_slot13 | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: integer `5`, expected a boolean at line 1 column 78 ; invalid type: integer `5`, expected a boolean at line 1 column 78 |
| pos_net_json_wrong_type_slot13 | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | invalid type: integer `5`, expected a boolean at line 1 column 78 ; invalid type: integer `5`, expected a boolean at line 1 column 78 |
| pos_net_json_nested_map | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: map, expected a boolean at line 1 column 26 ; invalid type: map, expected a boolean at line 1 column 26 |
| pos_net_json_nested_map | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: map, expected a boolean at line 1 column 26 ; invalid type: map, expected a boolean at line 1 column 26 |
| pos_net_json_nested_map | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | invalid type: map, expected a boolean at line 1 column 26 ; invalid type: map, expected a boolean at line 1 column 26 |
| pos_net_in_map_endpoint | file | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_in_map_endpoint | dir | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_in_map_endpoint | env | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_global_and_endpoint | file | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_global_and_endpoint | dir | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_net_global_and_endpoint | env | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| pos_log_json | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_log_json | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_log_json | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_log_toml | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_log_toml | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_log_toml | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_log_json_3_too_many | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | trailing characters at line 1 column 28 ; trailing characters at line 1 column 28 |
| pos_log_json_3_too_many | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | trailing characters at line 1 column 28 ; trailing characters at line 1 column 28 |
| pos_log_json_3_too_many | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | trailing characters at line 1 column 28 ; trailing characters at line 1 column 28 |
| pos_log_json_bad_level | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | unknown variant `loud`, expected one of `off`, `error`, `warn`, `info`, `debug`, `trace` at line 1 column 15 ; unknown variant `loud`, expected one of `off`, `error`, `warn`, `info`, `debug`, `trace` at line 1 column 15 |
| pos_log_json_bad_level | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | unknown variant `loud`, expected one of `off`, `error`, `warn`, `info`, `debug`, `trace` at line 1 column 15 ; unknown variant `loud`, expected one of `off`, `error`, `warn`, `info`, `debug`, `trace` at line 1 column 15 |
| pos_log_json_bad_level | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | unknown variant `loud`, expected one of `off`, `error`, `warn`, `info`, `debug`, `trace` at line 1 column 15 ; unknown variant `loud`, expected one of `off`, `error`, `warn`, `info`, `debug`, `trace` at line 1 column 15 |
| pos_dns_json_6 | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_dns_json_6 | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_dns_json_6 | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_dns_toml_6 | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_dns_toml_6 | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_dns_toml_6 | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_dns_json_2 | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_dns_json_2 | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_dns_json_2 | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_dns_json_7_too_many | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | trailing characters at line 1 column 59 ; trailing characters at line 1 column 59 |
| pos_dns_json_7_too_many | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | trailing characters at line 1 column 59 ; trailing characters at line 1 column 59 |
| pos_dns_json_7_too_many | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | trailing characters at line 1 column 59 ; trailing characters at line 1 column 59 |
| pos_top_json_all_positional | file | started | started | started | both-start-same | tcp:39600,39601 udp:39600 / tcp:39600,39601 udp:39600 | 0 | - ; - |
| pos_top_json_all_positional | dir | started | started | started | both-start-same | tcp:39600,39601 udp:39600 / tcp:39600,39601 udp:39600 | 0 | - ; - |
| pos_top_json_all_positional | env | started | started | started | both-start-same | tcp:39600,39601 udp:39600 / tcp:39600,39601 udp:39600 | 0 | - ; - |
| pos_top_json_maps_inside | file | started | started | started | both-start-same | tcp:39600,39601 udp:39600,39601 / tcp:39600,39601 udp:39600,39601 | 5 | - ; - |
| pos_top_json_maps_inside | dir | started | started | started | both-start-same | tcp:39600,39601 udp:39600,39601 / tcp:39600,39601 udp:39600,39601 | 5 | - ; - |
| pos_top_json_maps_inside | env | started | started | started | both-start-same | tcp:39600,39601 udp:39600,39601 / tcp:39600,39601 udp:39600,39601 | 5 | - ; - |
| pos_top_json_empty_tables | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_top_json_empty_tables | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_top_json_empty_tables | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_top_json_empty_maps | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_top_json_empty_maps | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_top_json_empty_maps | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| pos_top_json_unit_test_sample | file | exit 101 | started | started | old-rejects-new-accepts | - / tcp:39600 udp:39600 | 1 | trailing characters at line 1 column 126 ; - |
| pos_top_json_unit_test_sample | dir | exit 101 | started | started | old-rejects-new-accepts | - / tcp:39600 udp:39600 | 1 | trailing characters at line 1 column 126 ; - |
| pos_top_json_unit_test_sample | env | exit 2 | started | started | old-rejects-new-accepts | - / tcp:39600 udp:39600 | 1 | trailing characters at line 1 column 126 ; - |
| pos_top_json_unit_test_sample_old_len | file | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 1 | - ; - |
| pos_top_json_unit_test_sample_old_len | dir | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 1 | - ; - |
| pos_top_json_unit_test_sample_old_len | env | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 1 | - ; - |
| pos_top_json_3_too_few | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid length 3, expected struct FullConf with 4 elements at line 1 column 12 ; missing field `endpoints` at line 1 column 12 |
| pos_top_json_3_too_few | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid length 3, expected struct FullConf with 4 elements at line 1 column 12 ; missing field `endpoints` at line 1 column 12 |
| pos_top_json_3_too_few | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | invalid length 3, expected struct FullConf with 4 elements at line 1 column 12 ; missing field `endpoints` at line 1 column 12 |
| pos_top_json_0_too_few | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid length 3, expected struct FullConf with 4 elements at line 1 column 2 ; missing field `endpoints` at line 1 column 2 |
| pos_top_json_0_too_few | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid length 3, expected struct FullConf with 4 elements at line 1 column 2 ; missing field `endpoints` at line 1 column 2 |
| pos_top_json_0_too_few | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | invalid length 3, expected struct FullConf with 4 elements at line 1 column 2 ; missing field `endpoints` at line 1 column 2 |
| pos_top_json_5_too_many | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | trailing characters at line 1 column 56 ; trailing characters at line 1 column 56 |
| pos_top_json_5_too_many | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | trailing characters at line 1 column 56 ; trailing characters at line 1 column 56 |
| pos_top_json_5_too_many | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | trailing characters at line 1 column 56 ; trailing characters at line 1 column 56 |
| pos_top_json_5_too_many_b | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | trailing characters at line 1 column 56 ; trailing characters at line 1 column 56 |
| pos_top_json_5_too_many_b | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | trailing characters at line 1 column 56 ; trailing characters at line 1 column 56 |
| pos_top_json_5_too_many_b | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | trailing characters at line 1 column 56 ; trailing characters at line 1 column 56 |
| pos_top_json_null_tables | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: null, expected struct LogConf at line 1 column 5 ; invalid type: null, expected a configuration table at line 1 column 5 |
| pos_top_json_null_tables | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: null, expected struct LogConf at line 1 column 5 ; invalid type: null, expected a configuration table at line 1 column 5 |
| pos_top_json_null_tables | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | invalid type: null, expected struct LogConf at line 1 column 5 ; invalid type: null, expected a configuration table at line 1 column 5 |
| pos_top_json_endpoints_first | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | expected value at line 1 column 3 ; expected value at line 1 column 3 |
| pos_top_json_endpoints_first | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | expected value at line 1 column 3 ; expected value at line 1 column 3 |
| pos_top_json_endpoints_first | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | expected value at line 1 column 3 ; expected value at line 1 column 3 |
| dir_positional_global_plus_map_endpoint | dir | started | started | started | both-start-same | tcp:39600,39601 udp:39600 / tcp:39600,39601 udp:39600 | 0 | - ; - |
| dir_two_positional_networks | dir | started | started | started | both-start-same | tcp:39600,39601 udp:39600,39601 / tcp:39600,39601 udp:39600,39601 | 0 | - ; - |
| dir_one_bad_positional | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid length 1, expected struct EndpointConf with 11 elements at line 1 column 34 ; missing field `remote` at line 1 column 34 |
| x_net_toml_15th_is_string | file | started | exit 101 | exit 101 | OLD-ACCEPTS-NEW-REJECTS | tcp:39600 udp:39600 / - | 0 | - ; TOML parse error at line 1, column 84 invalid type: string "note", expected usize |
| x_net_toml_15th_is_string | dir | started | exit 101 | exit 101 | OLD-ACCEPTS-NEW-REJECTS | tcp:39600 udp:39600 / - | 0 | - ; TOML parse error at line 1, column 84 invalid type: string "note", expected usize |
| x_net_toml_15th_is_string | env | started | exit 2 | exit 2 | OLD-ACCEPTS-NEW-REJECTS | tcp:39600 udp:39600 / - | 1 | - ; TOML parse error at line 1, column 84 invalid type: string "note", expected usize |
| x_net_toml_15th_is_bool | file | started | exit 101 | exit 101 | OLD-ACCEPTS-NEW-REJECTS | tcp:39600 udp:39600 / - | 0 | - ; TOML parse error at line 1, column 84 invalid type: boolean `true`, expected usize |
| x_net_toml_15th_is_bool | dir | started | exit 101 | exit 101 | OLD-ACCEPTS-NEW-REJECTS | tcp:39600 udp:39600 / - | 0 | - ; TOML parse error at line 1, column 84 invalid type: boolean `true`, expected usize |
| x_net_toml_15th_is_bool | env | started | exit 2 | exit 2 | OLD-ACCEPTS-NEW-REJECTS | tcp:39600 udp:39600 / - | 1 | - ; TOML parse error at line 1, column 84 invalid type: boolean `true`, expected usize |
| x_net_toml_15th_is_negative | file | started | exit 101 | exit 101 | OLD-ACCEPTS-NEW-REJECTS | tcp:39600 udp:39600 / - | 0 | - ; TOML parse error at line 1, column 84 invalid value: integer `-1`, expected usize |
| x_net_toml_15th_is_negative | dir | started | exit 101 | exit 101 | OLD-ACCEPTS-NEW-REJECTS | tcp:39600 udp:39600 / - | 0 | - ; TOML parse error at line 1, column 84 invalid value: integer `-1`, expected usize |
| x_net_toml_15th_is_negative | env | started | exit 2 | exit 2 | OLD-ACCEPTS-NEW-REJECTS | tcp:39600 udp:39600 / - | 1 | - ; TOML parse error at line 1, column 84 invalid value: integer `-1`, expected usize |
| x_net_toml_16th_is_string | file | started | exit 101 | exit 101 | OLD-ACCEPTS-NEW-REJECTS | tcp:39600 udp:39600 / - | 0 | - ; TOML parse error at line 1, column 87 invalid type: string "note", expected usize |
| x_net_toml_16th_is_string | dir | started | exit 101 | exit 101 | OLD-ACCEPTS-NEW-REJECTS | tcp:39600 udp:39600 / - | 0 | - ; TOML parse error at line 1, column 87 invalid type: string "note", expected usize |
| x_net_toml_16th_is_string | env | started | exit 2 | exit 2 | OLD-ACCEPTS-NEW-REJECTS | tcp:39600 udp:39600 / - | 1 | - ; TOML parse error at line 1, column 87 invalid type: string "note", expected usize |
| x_net_toml_17th_is_string | file | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| x_net_toml_17th_is_string | dir | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| x_net_toml_17th_is_string | env | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| x_net_toml_15th_is_zero | file | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| x_net_toml_15th_is_zero | dir | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| x_net_toml_15th_is_zero | env | started | started | started | both-start-same | tcp:39600 udp:39600 / tcp:39600 udp:39600 | 0 | - ; - |
| x_ep_net_toml_15th_is_string | file | started | exit 101 | exit 101 | OLD-ACCEPTS-NEW-REJECTS | tcp:39600 udp:39600 / - | 0 | - ; TOML parse error at line 1, column 155 invalid type: string "note", expected usize |
| x_ep_net_toml_15th_is_string | dir | started | exit 101 | exit 101 | OLD-ACCEPTS-NEW-REJECTS | tcp:39600 udp:39600 / - | 0 | - ; TOML parse error at line 1, column 155 invalid type: string "note", expected usize |
| x_ep_net_toml_15th_is_string | env | started | exit 2 | exit 2 | OLD-ACCEPTS-NEW-REJECTS | tcp:39600 udp:39600 / - | 1 | - ; TOML parse error at line 1, column 155 invalid type: string "note", expected usize |
| x_log_toml_3rd_is_string | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| x_log_toml_3rd_is_string | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| x_log_toml_3rd_is_string | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| x_dns_toml_7th_is_string | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| x_dns_toml_7th_is_string | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| x_dns_toml_7th_is_string | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| x_ep_toml_12th_is_string | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | panic realm_lb/src/balancer.rs:94:53: called `Option::unwrap()` on a `None` value ; panic src/conf/endpoint.rs:101:13: endpoint 127.0.0.1:39600: balance requires a strategy and weights separated by ':' |
| x_ep_toml_12th_is_string | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | panic realm_lb/src/balancer.rs:94:53: called `Option::unwrap()` on a `None` value ; panic src/conf/endpoint.rs:101:13: endpoint 127.0.0.1:39600: balance requires a strategy and weights separated by ':' |
| x_ep_toml_12th_is_string | env | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | panic realm_lb/src/balancer.rs:94:53: called `Option::unwrap()` on a `None` value ; panic src/conf/endpoint.rs:101:13: endpoint 127.0.0.1:39600: balance requires a strategy and weights separated by ':' |
| x_ep_json_null_network_slot | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: null, expected struct NetConf at line 1 column 104 ; invalid type: null, expected a configuration table at line 1 column 104 |
| x_ep_json_null_network_slot | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: null, expected struct NetConf at line 1 column 104 ; invalid type: null, expected a configuration table at line 1 column 104 |
| x_ep_json_null_network_slot | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | invalid type: null, expected struct NetConf at line 1 column 104 ; invalid type: null, expected a configuration table at line 1 column 104 |
| x_ep_json_null_extra_remotes_slot | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: null, expected a sequence at line 1 column 58 ; invalid type: null, expected a sequence at line 1 column 58 |
| x_ep_json_null_extra_remotes_slot | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | invalid type: null, expected a sequence at line 1 column 58 ; invalid type: null, expected a sequence at line 1 column 58 |
| x_ep_json_null_extra_remotes_slot | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | invalid type: null, expected a sequence at line 1 column 58 ; invalid type: null, expected a sequence at line 1 column 58 |
| x_ep_json_10_elements | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| x_ep_json_10_elements | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| x_ep_json_10_elements | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| x_failover_toml_12th_is_string | file | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| x_failover_toml_12th_is_string | dir | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| x_failover_toml_12th_is_string | env | started | started | started | both-start-same | tcp:39600 udp:- / tcp:39600 udp:- | 0 | - ; - |
| x_legacy_positional_5 | file | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | unknown variant `127.0.0.1`, expected one of `off`, `error`, `warn`, `info`, `debug`, `trace` at line 1 column 13 ; unknown variant `127.0.0.1`, expected one of `off`, `error`, `warn`, `info`, `debug`, `trace` at line 1 column 13 |
| x_legacy_positional_5 | dir | exit 101 | exit 101 | exit 101 | both-reject | - / - | 0 | unknown variant `127.0.0.1`, expected one of `off`, `error`, `warn`, `info`, `debug`, `trace` at line 1 column 13 ; unknown variant `127.0.0.1`, expected one of `off`, `error`, `warn`, `info`, `debug`, `trace` at line 1 column 13 |
| x_legacy_positional_5 | env | exit 2 | exit 2 | exit 2 | both-reject | - / - | 1 | unknown variant `127.0.0.1`, expected one of `off`, `error`, `warn`, `info`, `debug`, `trace` at line 1 column 13 ; unknown variant `127.0.0.1`, expected one of `off`, `error`, `warn`, `info`, `debug`, `trace` at line 1 column 13 |

tally: {"both-start-same": 155, "both-reject": 121, "old-rejects-new-accepts": 9, "OLD-ACCEPTS-NEW-REJECTS": 15}
