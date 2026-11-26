# 测试台复现方法

全部在一个特权 Docker 容器里跑，不接触任何真实线路或服务器。容器内用网络命名空间搭出「客户端 c — 中继 r — 后端 a / b / x」的拓扑，用 `tc netem`、`iptables`、断网卡、杀进程来注入故障。

## 准备

```bash
docker run -d --name realm-test --privileged --ulimit nofile=1048576:1048576 \
  -v "$PWD":/src -v realm-cargo-registry:/usr/local/cargo/registry \
  -v realm-rustup:/usr/local/rustup -v realm-target:/src/target \
  -v "$PWD/test-report/lab":/work -w /src rust:bookworm sleep infinity
```

```bash
docker exec realm-test bash -c 'apt-get update -qq && apt-get install -y -qq cmake clang libclang-dev iperf3 jq iproute2 iptables procps psmisc iputils-ping'
```

```bash
docker exec realm-test bash /work/build.sh all
```

`build.sh all` 会把原版 v2.9.6（7e9f44c）构建成 `/work/bin/realm-orig`，把当前工作区构建成 `/work/bin/realm-new`。测试工具 `fot` 单独构建：

```bash
docker exec realm-test bash -c 'cd /work/fot && cargo +stable build --release --target-dir /src/target/fot && cp /src/target/fot/release/fot /work/bin/fot'
```

## 运行

| 脚本 | 内容 | 耗时 |
| --- | --- | --- |
| `scen_fo.py [前缀..]` | 故障转移场景 FO-01 ~ FO-23 | 约 13 分钟 |
| `scen_lb.py` | 负载均衡回归 LB-01 ~ LB-08（原版对新版） | 约 3 分钟 |
| `scen_multi.py [前缀..]` | 3 台以上：优先级链、备机先挂、两台同时挂、带健康检查的轮询 / IP 哈希、8 台满配（MN-01 ~ MN-06） | 约 4 分钟 |
| `extreme.py [前缀..]` | 极端场景 EX-01 ~ EX-10 | 约 10 分钟 |
| `scen_fp.py 秒数 每档端点数 ["额外配置"] [输出名]` | 劣化链路下的误判统计 | 自定 |
| `soak.py 秒数` | 混合负载 + 随机故障的长稳测试 | 自定 |
| `scen_stuck.py 秒数 [输出名]` | 主节点静默失效后，已有连接卡多久 | 约 5 分钟 |
| `perf.py 轮数` | 回环吞吐与资源对比 | 每轮约 4 分钟 |
| `perf_ab.py 轮数` | 顺序平衡的短连接 A/B | 每轮约 1 分钟 |
| `perf_xfer.py 轮数` | 顺序平衡的 1 MiB 往返 A/B | 每轮约 40 秒 |
| `flap3.py` | 随机相位抖动：去掉清零规则前后两个构建的对照（需要 `bin/realm-fix3` 和 `bin/realm-fix4`） | 约 12 分钟 |
| `flap03_diag2.py [轮数]` | 快速抖动（断、通各 0.3 秒）时用独立 ping 核对探测结果 | 每轮约 35 秒 |
| `ex03_orig_diag.py [轮数]` | 原版在 5000 个 UDP 会话下的套接字数、端口重叠和串包 | 每轮约 15 秒 |

```bash
docker exec realm-test bash -c 'cd /work && python3 scen_fo.py'
```

环境变量：`REALM_NEW` 指定被测二进制（默认 `/work/bin/realm-new`），`LAB_OUT` 指定结果目录（默认 `/work/out`）。每个脚本把结果写成 JSON，`mkmd.py`、`mktables.py` 把 JSON 转成报告里的表格。

两个注意事项：

- 环境变量用 `docker exec -e REALM_NEW=...` 传，不要写进 `bash -c '...'` 的命令行里。脚本清理现场时用 `pkill -f realm-`，命令行里带 `realm-` 字样的外层 shell 会被一起杀掉。
- 每个脚本开头都会重建网络命名空间并杀掉残留进程，同一个容器里一次只能跑一个脚本。要并行就多开几个容器。吞吐类脚本（`perf*.py`）要在机器空闲时单独跑。
- 新旧版的性能对比要用顺序平衡的脚本（`perf_tcp.py`、`perf_ab.py`、`perf_xfer.py`）。`perf.py` 按固定顺序测各个版本，同一轮里靠后的版本可能系统性地偏低 10%~15%，只适合看量级。
- 断、通各 0.3 秒这样快速地反复断网卡，偶尔会让后端一侧解析不出中继的 MAC 地址，后端变成真的不可达（约十几轮出现一次）。遇到「探测连续全失败」时先用 `flap03_diag2.py` 核对，它会同时记录一个独立的 ping。

## 没有本地 Docker 时：在 GitHub 托管 runner 上跑

v3.2.1 发版前的回归是这样跑的（结果在 `results/v321/lab/`，报告 10.3、10.4）：

- 推一个临时分支，分支上多加一个只对这个分支触发的 workflow，跑完把分支删掉。runner 用 `ubuntu-24.04`。
- runner 的内核默认不带 `sch_netem`，起容器之前先装上：`sudo apt-get install -y linux-modules-extra-$(uname -r) && sudo modprobe sch_netem`。
- 容器按上面「准备」起，不需要那三个缓存卷；装包那一行多加了 `python3`。
- 四个套件（`scen_fo.py`、`scen_lb.py`、`scen_multi.py`、`extreme.py`）各开一个 job：同一个容器里一次只能跑一个脚本。构建约 15 分钟，每个套件 3~13 分钟。
- runner 之间速度差得很多（EX-07 在一台上是 1700 连接/秒，在另一台上是 1 万），新旧对照只能比同一个 job 里交替跑出来的数。上一个版本用 `git archive <标签>` 取出源码，构建成 `/work/bin/realm-<标签>`，再用 `-e REALM_NEW=...` 指给单个场景（如 `python3 extreme.py ex07`）。

`extreme.py` 在 v3.2.1 改了两处：

- EX-07 的压测端原来固定钉在 4~9 号核，现在机器不足 10 核时改用 2 号核到最后一个核。中转仍然钉在 1 号核。
- EX-02（主节点被杀，5000 条长连接同时重连）的断连次数上限从 5100 放宽到 6000。被杀的后端在内核关完它的 5000 条连接之前（约 0.1 秒）仍然接受新连接，这段时间里重连上来的会被重置一次再连回来，撞上多少取决于中转接受重连有多及时。其余判定（重连 0 失败、全部落到备节点、句柄不泄漏）没有变。

## v3.2.1 的回环脚本

`v321/` 里是报告第十节用的故障复现和吞吐对照脚本。它们只用回环地址，不需要 root，也不需要容器，说明见 [v321/README.md](v321/README.md)。

## 测试工具 fot

`fot/src/main.rs`，一个二进制多个子命令：

- `srv`：可编程后端。TCP/UDP 数据端口 + 独立健康端口 + 本机控制端口，运行时可切换成「不回数据」「接受后立刻关闭」「延迟回复」等模式。应答里带后端标记（A/B/C），客户端据此知道每个请求落到了哪。
- `churn`：短连接压测，每个连接一次请求并逐字节校验回显，按 100ms 分桶记录成功（按后端）和失败（按原因）。
- `hold`：长连接，周期性请求并校验，记录断连和「同一条连接是否换了后端」。
- `udp`：多个 UDP 套接字按速率发包，记录送达、重复、乱序的旧后端回包，以及每个会话经历的后端序列。
- `abuse`：异常客户端（连上即 RST、半包后半关闭、传输中途 RST、连上不说话）。
