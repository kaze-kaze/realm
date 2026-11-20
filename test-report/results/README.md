# 原始结果

| 目录 | 被测二进制 | 内容 |
| --- | --- | --- |
| `merged/` | 已合并版（master 2729766） | 第一遍：故障转移、负载均衡、极端场景、误判统计、吞吐 |
| `fix-round1/` | 修复到一半的版本（问题 1~4、6、7 已修） | 第二遍全量回归、误判统计（含 200 端点低丢包长时统计、fall=2 对照）、第一次长稳 |
| `final/` | 修复版，去掉「连续 2 次成功清零」之前 | 全量回归、吞吐（`perf.json`、`perf-tcp.json`、`perf-ab.json`）、长稳及其分项和对照、卡死连接前后对比（`stuck-*.json`、`zero-window.json`）、随机相位抖动对照（`flap2.json`） |
| `experiment-noclear/` | 上一行的修复版去掉「连续 2 次成功清零」的实验构建（决定之前做的对照） | 误判统计、抖动场景 |
| `final2/` | 最终版：修复版正式去掉「连续 2 次成功清零」之后 | 全量回归（`fo.json`、`lb.json`、`ex.json`）、误判统计（`fp.json` 是 30 端点 × 10 分钟全部档位，`fp200.json` 是 200 端点 × 25 分钟的 2% / 5% / 10% 三档）、长稳（`soak.json`）、去掉前后的随机相位抖动对照（`flap3.json`）、两个追查（`flap03-diag*.json`：快速抖动时主节点偶尔真的不可达；`ex03-orig-diag.json`：原版对照的偶发异常复跑）、吞吐复核（`perf-tcp-combined.json` 是两次共 6 个有效轮次，两次的原始输出是 `perf-tcp-run1.json`、`perf-tcp-run2.json`，`perf.json` 是 1 轮，`perf-xfer.json` 是 1 MiB 往返的 8 轮顺序平衡对照） |
| `multi-peer/` | v3.1.0 发布版代码（`multi-v3.1.0.json`）和多节点修复版（其余文件） | 3 台以上的补测 `scen_multi.py`（MN-01 ~ MN-06）修复前后各一遍；修复版上的全量回归（`fo-fixed.json`、`lb-fixed.json`、`ex-fixed-ex.json`） |
| `release-check/` | 从 GitHub Release 下载的 aarch64 musl 发布包（校验和与发布页一致） | `v3.0.0-failover.json`：v3.0.0 发布包上的 7 个关键故障转移场景；`v3.1.1-multi-peer.json`：v3.1.1 发布包上的 3 台以上场景 MN-01 ~ MN-06 |

`perf-ab.json`、`stuck-*.json`、`zero-window.json`、`flap2.json`、`soak-tcp.json`、`soak-udp.json` 是在加入 UDP 串包修复（问题 8）之前的那次构建上测的。那次修复只改了 UDP 上游套接字的一个选项，这几项测量涉及的代码没有再变。`final/` 里的全量回归、吞吐表、`perf-tcp.json`、`soak.json` 和 `soak-ctl-*.json` 则是在含问题 8 修复的构建上测的。

报告正文和附录 A 用的是 `final2/` 的数字。3.1 的两张主表和长稳的内存对照（`soak-ctl-*.json`）仍来自 `final/`，3.1 末尾的复核表来自 `final2/`。
