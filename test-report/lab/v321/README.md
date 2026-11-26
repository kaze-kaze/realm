# v3.2.1 的回归与对照脚本

对应报告第十节。全部只用回环地址，不需要 root，也不需要容器；故障靠 `LD_PRELOAD` 的小垫片注入，垫片只加载进被测的 realm 进程。

## 准备

```bash
gcc -shared -fPIC -O2 -o noport.so shim/noport.c -ldl
gcc -shared -fPIC -O2 -o sndbuf.so shim/sndbuf.c -ldl
gcc -shared -fPIC -O2 -o bindfail.so shim/bindfail.c -ldl
```

被测二进制自己构建（`cargo build --release`，或加 `--no-default-features --features default-ring`）。`churn.py` 需要 `../fot`（构建方法见上一级的 README）。

## 脚本

| 脚本 | 内容 | 用法 |
| --- | --- | --- |
| `noport.py` | 新客户端拿不到本地端口时，这条 UDP 规则还转不转发。`noport.so` 在标记文件存在期间，让还没有本地端口的套接字发送时返回 `EAGAIN`（内核自动绑定失败时的返回值）。它复现的是 v3.2.0 的故障；这一版建关联时就绑定了端口，这个垫片对它不再生效 | `noport.py REALM noport.so 标记文件路径` |
| `bindfail.py` | 同一个问题在这一版上的验证：`bindfail.so` 让关联套接字的 `bind` 本身失败（内核端口用完时的返回），看新客户端、已建立的客户端和日志各是什么表现。v3.2.0 从不显式绑定，对它加参数 `old`，垫片改为让未绑定套接字上的发送失败，与 `noport.so` 相同 | `bindfail.py REALM bindfail.so 输出目录 [old]` |
| `lostwake.py` | 很多 UDP 客户端同时下行、监听套接字发送缓冲被打满之后，各客户端还收不收得到回包，关联的套接字会不会被回收。`sndbuf.so` 把监听套接字的发送缓冲压到最小，回环上才出得来 `EAGAIN` | `lostwake.py REALM sndbuf.so 输出目录 [客户端数=12] [sndbuf=1] [秒数=3]` |
| `halfclose.py` | TCP 半关闭的四种场景，各与直连对照：客户端先关写端再等应答、应答还在路上而对端仍在上传、慢读下载、慢读上传 | `halfclose.py REALM` |
| `startup.py` | 启动与配置的验收：端口被占、重复实例、未知键、非法取值、没有任何监听等 18 项，逐项打印 `PASS` / `FAIL` | `startup.py REALM 输出目录` |
| `ab.py` | 新旧两个二进制的回环吞吐对照：UDP 上行、下行（1 个与 4 个客户端）、TCP 大流量、短连接。按 ABBA 顺序轮换，报中位数与范围 | `ab.py OLD NEW [轮数] [fot]`，`AB_ONLY=udp` 只跑名字里带这个词的项，`AB_OUT=文件` 存原始结果 |
| `bulk.py` | TCP 大流量对照，发送端、接收端、中转各占不同的核。`ab.py` 里的同名项收发两端在一个 Python 进程里，抖动太大，报告用的是这个 | `bulk.py 轮数 每次GiB 名字=二进制 ...` |
| `churn.py` | 短连接对照，可以带参数比较多个变体（例如同一个二进制加 `--half-close-timeout 0`） | `churn.py fot 轮数 每次秒数 "名字=二进制 参数..." ...` |
| `exlocal.py` | 测试台两个极端场景的回环版，不需要网络命名空间：`cpu` 是 EX-07（中转钉在一个核上被 TCP 和 UDP 同时打满），`herd` 是 EX-02（主节点被杀，5000 条长连接同时重连）。回环上每个系统调用便宜得多，EX-07 的差别远没有测试台上明显，只能用来看方向 | `exlocal.py cpu\|herd fot "名字=二进制[,额外的 toml 行;...]" ...`，`EX_PPS`、`EX_SECS`、`EX_LOG` 调参数 |
| `teardown.py` | 后端进程被杀之后，内核还会继续接受新连接多久（监听套接字是最后才关的）。`close` 与 `keep` 两种模式分别对应对端收到 FIN 后立即关闭、保持不动 | `teardown.py fot close\|keep` |
| `blackbox/` | 发版前最后一轮修订的黑盒对照：配置兼容（`rf01_*`）、域名换地址族（`rf02_*`，自带一个回环 DNS 应答器）、PROXY v2 头（`rf03*`）、管道容量抽样（`nf02_strace.py`）。用环境变量 `REALM_OLD`、`REALM_NEW`、`REALM_DBG` 指定 v3.2.0、被测发布构建和它的调试构建 | 见各脚本开头的说明，`run_rf02.sh` 是域名换地址族的整套参数 |

## 注意

- 端口固定在 39020–39699 之间（`blackbox/` 用 39600 以上）。本机别的程序占了其中的端口时，v3.2.0 及更早的版本不会报错，只是那条规则不监听（这正是这一版修的问题之一），脚本会表现为连不上。
- 这些端口落在系统的临时端口范围里。紧跟在短连接压测（`churn.py`、`ab.py` 的短连接项、`exlocal.py`）之后跑别的脚本，端口可能被处于 TIME_WAIT 的旧连接占着：这一版会拒绝启动，脚本表现为全部连不上。等一分钟（`ss -tan | grep -c TIME-WAIT` 降下来）再跑。
- 吞吐数字只能用来比新旧，不代表真实线路上的能力：回环没有网卡、没有丢包、没有时延。
- 机器上有别的负载时，单次结果能差出 20% 以上。多跑几轮看中位数，并且新旧交替着跑。
