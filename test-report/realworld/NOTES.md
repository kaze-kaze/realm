# 真实业务测试工作笔记（2026-11-19 至 11-20，已完成）

写给后续整理报告用，数字均来自 `results/` 或测试机 `/opt/rbt/run/res/`。

入库前，文中和结果文件里测试机、本机的公网地址已替换为文档保留地址（chi 198.51.100.11、ams 198.51.100.12、po0 198.51.100.13、本机 203.0.113.10）。

## 环境

- 三台测试机，互为中转 / 落地 / 客户端，另加本机 Mac Docker 客户端（住宅线路）。
  - chi：Greencloud Chicago，Debian 13，内核 6.12，4 核 7.5G，198.51.100.11
  - ams：HostHatch AMS，Debian 12，内核 6.1，3 核 2.9G，198.51.100.12，**网卡 MTU 1450**
  - po0：PO0 US，Debian 12，内核 6.1，2 核 2G，198.51.100.13（NAT）；机上原有 realm 2.9.6（50000 端口）和 wg-core，未触碰
  - RTT：chi↔po0 65 ms，chi↔ams 146 ms，ams↔po0 148 ms
- 容器：`rbt-land`（落地，桥接网络，自己的网络命名空间，故障只注入在容器内）、`rbt-relay`（中转，主机网络）、`rbt-cli`（客户端，桥接）、`rbt-rust`（仅 chi，编译用）。镜像 `rbt:1`（debian:trixie-slim + 工具）。
- 防火墙临时放行：ams 的 nft `inet host_input` 加了一条 comment "rbt" 的规则；chi 的 iptables 加了 `RBT` 链。清理时要删。
- 软件：Xray 26.11.9，Hysteria 2.14.0，WireGuard（内核），realm v3.0.0 发布版（realm3）、上游 2.9.6（realmog）、测试构建 realm3x / realm3y / realm3z（最终，sha256 73aa6a87…）。
- 中转规则：relay chi → 主 po0 / 备 ams；relay po0 → 主 chi / 备 ams；relay ams → 主 chi / 备 po0。
- 端口：落地 31000+off；中转 fo 21000、pl 22000、p2 22500、og 23000、rr 24000、ih 24500、fd 21500、rh 24200、hh 24700。
- 16 个"协议"：vmess-tcp、vmess-ws、vless-enc（VLESS Encryption）、vless-reality、vless-ws-tls、vless-xhttp-tls、trojan-tls、ss-aead、ss-2022、vmess-kcp、vless-grpc-tls、vless-hu-tls、hy2、hy2-obfs、wg、tgt（裸转发）。

## 搭环境时碰到的、与 realm 无关但值得写进报告的

- Xray 26.11 拒绝"无 TLS / 无加密的 VLESS 连公网地址"，改用 VLESS Encryption。gRPC、HTTPUpgrade、WebSocket、VMess 都打印弃用警告但仍可用。
- Xray 26.11 的 freedom 出站默认拦截回环 / 内网目标（finalRules），测试目标在落地端本机，需要显式放行。
- Go 拒绝 LibreSSL 生成的带显式曲线参数的 EC 私钥（Hysteria 启动失败），用 OpenSSL 3 重新生成。

## 冒烟（v3.0.0 发布版，全部协议 × 各种中转方式）

- 直连 16/16。
- AMS 客户端 → relay chi：6 组 × 16 = 96，95 通过。
- chi 客户端 → relay po0：96，94 通过。
- po0 客户端 → relay ams：96，约 90 通过。
- Mac → 三台中转的 fo 端点：48/48 通过。
- 失败只有两类，原版 2.9.6 同样失败：
  1. rr（轮询）下的 vless-enc：0-RTT 票据只在签发的落地机有效，连接轮到另一台就失败（约 25%）。
  2. 经过 ams 的 WireGuard 上传：MTU 问题，见下。

## MTU / 大包

- ams↔po0 之间超过约 1450 字节的 IP 包直连也不通（DF 包被丢且 ICMP 回不到 po0）。chi↔ams、chi↔po0 任意大小都通。Mac 住宅线路上分片包（>1472）全丢，直连也一样。
- WireGuard 默认 MTU 1420 → 外层 1480 字节，过不了 ams 的 1450。属于线路问题，建议经中转的 WG 把 MTU 设到 1380 以下（待实测确认）。
- **realm 缺陷（上游同样有）**：UDP 接收缓冲每包 1500 字节，更长的数据报被截断后照样转发。
  - 裸转发：1501–9000 字节 0/50 且全部 DAMAGED；直连正常。
  - ss-2022 / ss-aead UDP：应用数据报 ≥1452 字节全部丢失（封装后超过 1500），直连正常。1452 正好是 WG 满包。
  - 修复后（realm3z）：1501–60000 字节 250/250 完整；ss-2022 内 1452–4000 字节全部通过。

## 故障演练 D1：主落地机黑洞 60 s，默认参数（AMS 客户端 → relay chi，fo）

realm 日志：故障后 8 s 判下线并切换，恢复后 13 s 切回，23 个端点同一秒完成。

| 协议 | TCP 新建请求失败 | 长连接 | UDP 中断 |
| --- | --- | --- | --- |
| vmess-tcp / vmess-ws / vless-reality / vless-ws-tls / trojan / ss / tgt | 0/738 | 3 s 后断开，3 s 内重连到备机 | 6 s（走 TCP 的）/ 9 s（原生 UDP） |
| vless-enc | 11（0-RTT 票据） | 同上 | 6 s |
| vless-xhttp-tls | 155，约 39 s | 10 s | 多次 3 s |
| vless-grpc-tls | 248，62 s | 60 s | 63 s |
| hy2 / hy2-obfs | 121，30 s | 28 s | 31 s |
| wg | 58，14.6 s | 14 s | 16 s |
| vmess-kcp | 25，6 s | 6 s | 9 s |

## D2：同样的故障，relay 上 `tcp_keepalive=5, tcp_keepalive_probe=2`，Hysteria 客户端 idle 8s；加对照线路 p2-chi

- 对照线路（不经过故障机）：16 协议 190 s 内 0 失败、UDP 0 丢包、长连接不断。
- gRPC：62 s → 15.9 s。xhttp：最后一次失败 15.9 s（原 39 s）。
- Hysteria 仍是 30 s：客户端侧 `quic.maxIdleTimeout` 无效。单独实验：服务器侧设 `quic.maxIdleTimeout: 10s` 后，客户端 10.0 s 发现失效（默认 30.0 s）。
- xhttp 在切回主机前后有几次 3 s 的 UDP 中断：会话的上行 POST 和下行 GET 落到了不同落地机。

## 负载均衡 lb1 / lb2（AMS 客户端 → relay chi，一台落地机黑洞）

稳态（lb2，黑洞 120 s，每轮 40 次新建 TCP + 8 条 UDP 流）：

| 端点 | TCP 失败 | UDP 流 |
| --- | --- | --- |
| rr（轮询，无健康检查） | 20/40（50%） | ss/tgt 8/8 全死（UDP 只走第一个节点） |
| ih（IP 哈希，无健康检查） | 40/40 | 8/8 全死 |
| rh（轮询 + 健康检查，新功能） | 0/40 | 8/8 正常，全部在存活机 |
| hh（IP 哈希 + 健康检查，新功能） | 0/40 | 8/8 正常 |

恢复后 rh 回到 20/20、UDP 4/4；hh 回到原落地机。正常时 rh 的 UDP 流 4/4 均分，rr 的 UDP 全在第一台。

## QUIC 探测

- 对 Hysteria 2 端口发 1200 字节、版本号 0x1a2a3a4a 的长包头，chi / ams 两台都回 Version Negotiation 并带回连接 ID。小于 1200 字节不回。hy2-obfs、wg、ss、kcp 都不回。
- fd 端点（探测业务端口本身）冒烟：TCP 协议和 hy2 全通过，relay 日志无误判下线。

## 代码

- 分支 `fix/real-world-findings`，PR kaze-kaze/realm#3。
  - 1cf917b fix(udp): relay datagrams larger than 1500 bytes whole
  - 6678f3c feat(failover): probe QUIC services directly, balance over live peers
  - 664c47b fix(udp): bound the memory large datagrams can take（自动安全审查指出首版大包缓冲可被放大，改为每线程共享溢出区）
- 没有 workflow 权限，CI 配置没改；`balance` 相关集成测试和批量收发路径的测试只在本地带 `--features proxy,balance,batched-udp` 跑过。

## 吞吐（芝加哥机器回环，perf_udp.py，6 轮中位数，Mbit/s）

- 发布版对比（og / v3.0.0 发布 musl / 新版本机 glibc）：UDP 上行 1400：885 / 892 / 1073；下行 1400：916 / 913 / 1060；上行 200：141 / 138 / 167；TCP 约 18.7–20.1 Gbit/s 无差别。**不是同条件**（musl 对 glibc）。
- 同工具链（v3.0.0 源码本机编译 realm300g 对新版 realm3z）：UDP 上行 1400：1099（1016–1217）对 1006（984–1222）；下行 1400：990 对 999；上行 200：157 对 166；TCP 19089 对 19154、18960 对 19654。结论持平。
- 待做：官方 gnu 产物（bin/realm3gnu，已下载校验）对 musl 产物（bin/realm3）。

## 连续演练（campaign.py，base 1795130918 = 23:28:38 UTC，AMS 客户端 → relay chi，中转 keepalive 5/2，落地 Hysteria idle 10s）

结果文件 `c-chi-<名字>.json`，每项 170 秒一轮，故障在 +30 s 开始。详见 REALWORLD.md 第 4.3 节的表。要点：

- drain：TCP 8.3 s 转备、0 失败、长连接不断；ss/tgt UDP 无缝；hy2 断 12 s、wg 断 16 s。
- refuse：TCP 0–1 失败；原生 UDP 断 8 s；wg 23 s。
- loss30：中转日志里 TCP 端点没有判下线，UDP 端点 21020、21009 各切走一次；TCP 请求 4–21% 超时。
- delay800：fo（带应答校验）9.6 s 切；fd（只握手）没切，reality 233/599 超时。
- flap：TCP 2/599 失败，约 10–14 s 后稳定在备机。
- alldown（40 s）：中断等于故障时长，恢复后立即可用，UDP 会话没有被挪。
- loss10：中转日志 23:46:08–23:47:08 无任何 failover 事件（23 个端点 0 误判）。
- svcstop：fo 的 hy2 断 61 s、ss UDP 断 60 s；fd 的 hy2 10.1 s 恢复。
- 对照线路只有 vless-reality 出现 3 次约 1.75 s 的握手停顿（7+14 次超时）。
- **作废数据**：loss10 / loss30 里 hy2 的结果。fault.sh 原先用 iptables OUTPUT 丢包，发送方收到 EPERM，quic-go 直接断连并在之后发 stateless reset。已改为 netem 丢出站包。bh 也同样改了。

## Hysteria 空闲超时实验（po0 客户端直连 chi 落地，只丢入站包）

- 客户端 quic 8s/2s、4s/2s、默认：都是 30.0 s 后断开。
- 落地端 `quic.maxIdleTimeout: 10s`：10.0 s 后断开（客户端默认或 8s 都一样）。

## WireGuard MTU（po0 客户端 → relay ams → chi 落地）

1420：上传超时；1392 / 1390 / 1380 / 1280：上传下载都正常。

## 第二阶段（进行中）

- A/B：chi 客户端，apps 套件，vias pl-po0（新版）对 og-po0（原版），16 协议，tag apps-ab-chi。
- 长稳：AMS 客户端，apps 套件 loops=2，fo-chi，wg_mtu 1380，tag soak-ams；po0 落地故障表 base 1795132380：270 bh 35s、570 loss10 120s、930 bh 40s、1230 health bad 60s、1530 stop xray+hy2 30s、1830 loss20 60s、2130 bh 30s。
- 本机：apps，fo-ams，7 协议，tag apps-mac。

## 收尾（2026-11-20 02:00–03:00 UTC）

- 第四个提交 `8f658de`（原 `1bb21a4`，内容相同，为触发 CI 重推过一次）：`udp_no_fragment`。数据在 `results/chi/cap-new.out`、`cap-og.out`、`results/po0/big4-*.json`。
- 官方 gnu 对 musl 产物：`results/chi/perf3.out`，UDP 上 gnu 快 12–23%。
- UDP 丢包核对：`results/chi/udpcmp*.out`（真实线路）、`udploop.out`（回环，四个构建各 292,800 包 0 丢）。
- 内存：`results/chi/rss.out`。
- Hysteria 经 po0 中转大文件传输复查：`results/chi/hysteria-recheck/`、`results/po0/hysteria-recheck/`。
  - hyab2（第 1 轮）：v300 单端点下载 0/8。hyab3（互换端口）：v300 8/8，新版 3/8。hyab4（原版 × 3、新版 × 3 交替）：原版在 26020 端口 1/6，其余 6/6。hyab5（中转缓冲 4 MiB，再次互换）：失败散落在各端口。失败跟着某一条连接走，不跟版本，也不固定在某个端口。
  - po0 主机 UDP 统计：默认缓冲下 hyab4 期间 InDatagrams +4,125,707、RcvbufErrors +52,713（1.28%）；4 MiB 缓冲下 hyab5 期间 +4,392,450、+383（0.009%）。
  - 直连对照（hydirect.py，po0 客户端 → chi 落地）：下载 17/18、上传 18/18。
  - Hysteria 客户端在失败时的日志：`timeout: no recent network activity`。根因未查到。
- GitHub Actions：`8f658de` 没有任何流水线。`gh workflow run` 返回 `HTTP 422: Actions has been disabled for this repository`，而 `actions/permissions` 接口显示 enabled=true。最后一次成功触发是 2026-11-19 22:58 UTC（`664c47b`）。关闭再打开 PR、重推提交都不触发。约 03:05 UTC 自行恢复（手动触发成功），原因不明。
- 本地补做的验证：`realm_core` 对 x86_64-pc-windows-gnu、x86_64-apple-darwin、aarch64-linux-android、x86_64-unknown-linux-musl 的 `cargo check`（两组 feature）0 错误；整个二进制的交叉检查因为 aws-lc-sys 需要交叉 C 工具链没法在本地做。
- 清理：三台测试机和本机的 rbt 容器、镜像、网络、目录、防火墙规则全部删除并核对；po0 的 rmem/wmem_default 已改回 212992。
- 密钥：自动安全检查提示 `lab/secrets.json` 里有密码和私钥。该文件从未入库，对应服务已全部拆除；已删除 `secrets.json` 和 `pki/`，新增 `lab/mksecrets.py`（现场生成，容器内实测通过）和 `lab/.gitignore`。全目录扫描无残留密钥。
- 用户 2026-11-20 的决定：合并 PR #3 并发 v3.1.0；UDP 会话保持一律挪走；`test-report/` 入库；本机旧测试台暂时保留。
