# 性能与规模基线

本页记录 OpenFlex 在**大模型**上的实测性能：模型解析/校验、仿真运行、内存峰值，
以及与 `bench_mm1`（小模型吞吐）的对比。它对应 `docs/BACKLOG.md` C5「性能与规模」。

> **结论先行（v0.11.0，2026-11-15）**：小模型（4 个对象）约 **1.7M 事件/秒**；
> 10 万对象时降到 **552 事件/秒**（validate 85 s、run 272 s、峰值内存 397 MB）。
> 时间随对象数**超线性**增长（10× 对象 → 242×/316× 时间），说明瓶颈在对象规模而非事件数。
> 10 万对象/1 亿事件的目标**尚未达成**，优化方向见第 6 节。

## 1. 测量方法

工具：[`scripts/bench-large-model.sh`](../../scripts/bench-large-model.sh)

```sh
# 快速基准（10k 对象，约 2 秒）
scripts/bench-large-model.sh

# 完整 10 万对象基线（本页第 3 节的数据，约 10 分钟）
scripts/bench-large-model.sh --objects 100000 --json /tmp/bench-100k.json

# 把结果追加成 markdown 表格行
scripts/bench-large-model.sh --markdown /tmp/row.md
```

脚本做三件事：

1. **生成模型**：`Source → Queue → Processor → Sink` 链 × N 组；每个 Source 按恒定间隔
   产出 `--items` 个 item，Processor 的加工时间接近 0，因此测的是引擎记账而不是排队动态。
   对象数 = 4 × 链数（向下取整到 4 的倍数）。
2. **测 validate**：`openflex validate model.ofm`（JSON 解析 + 语义校验），wall time +
   子进程峰值 RSS。
3. **测 run**：`openflex run model.ofm --format csv`（解析 + 仿真 + 生成报告），同样记录
   wall time 与峰值 RSS；再跑一次**不计时**的 `--trace` 统计精确事件数，得到 events/s。

峰值内存通过 `os.wait4` 的 `rusage.ru_maxrss` 取**该子进程**的最大常驻集
（macOS 单位字节，Linux KiB 换算成字节），不是父进程的累计值。
计时包含进程启动与报告格式化，也就是用户真实付出的成本。

## 2. 模型规格（10 万对象基线）

| 项 | 值 |
|---|---|
| 对象数 | 100,000（25,000 条链 × 4） |
| 链结构 | `Source → Queue → Processor → Sink` |
| Source 参数 | 恒定到达间隔 50（stop_time 100，first_arrival 50） |
| Processor 参数 | setup 0、process 0.0001、max_content 1 |
| 运行设置 | stop_time=100，warmup=0，seed=1，replications=1 |
| 模型文件 | 14.2 MB（紧凑 JSON） |
| 报告 CSV | 8.4 MB，100,000 行对象统计 |
| 事件总数 | 150,000（每条链 6 个事件） |

## 3. 实测结果（macOS 15.8.1 x86_64，release 构建 v0.11.0）

| 场景 | 对象数 | 事件数 | validate | run | 峰值 RSS（validate / run） | events/s | items/s |
|---|---|---|---|---|---|---|---|
| `bench_mm1`（内置 M/M/1） | 4 | 120,318 | — | 0.070 s | —（未测） | **1,709,048** | 284,841 |
| 大模型 1k | 1,000 | 1,500 | 0.04 s | 0.05 s | 3.3 / 7.2 MB | 29,560 | 4,927 |
| 大模型 10k | 10,000 | 15,000 | 0.35 s | 0.86 s | 18.9 / 44.5 MB | 17,390 | 2,898 |
| 大模型 100k | 100,000 | 150,000 | **85.12 s** | **271.56 s** | 134.3 / 396.7 MB | **552** | 92 |

`bench_mm1` 的命令与输出（对照用）：

```sh
cargo run --release -p openflex-cli --example bench_mm1 -- --stop 100000
# items completed: 20053
# events processed: 120318
# untraced: 0.070 s -> 1709048 events/s, 284841 items/s
# traced:   0.099 s -> 1220444 events/s, 203407 items/s (tracing overhead included)
```

## 4. 扩展性分析

| 规模变化 | validate 时间 | run 时间 |
|---|---|---|
| 1k → 10k（对象 ×10） | 0.04 → 0.35 s（×8.75） | 0.05 → 0.86 s（×17.2） |
| 10k → 100k（对象 ×10） | 0.35 → 85.12 s（**×242**） | 0.86 → 271.56 s（**×316**） |

- 如果时间与对象数线性，10× 对象应该只带来 10× 时间；实测是 242×/316×，
  拟合指数约 **n²·⁴ ~ n²·⁵**。
- 事件数只与链数线性相关（25k 链 → 150k 事件），但 events/s 从 1.7M 掉到 552，
  说明耗时不在事件处理循环，而在**与对象总数相关的查找/校验**上。
- 内存基本线性（100k 对象 → 397 MB 峰值，其中模型 14 MB、报告 8.4 MB）。

**推断（未做 profile，因为不能改 `crates/`）**：`validate` 的每条连接/对象检查看起来对
全对象集合做了二次扫描；`run` 中每个事件可能也要按名字/连接线性查找对象。两者叠加导致
10 万对象不可用。验证方法（在允许改 `crates/` 的任务里）：`cargo flamegraph` /
`perf record`，或对 `validate` 单独做对象数扫描基准。

## 5. 这组数据能说明什么、不能说明什么

**能说明**：

- 10 万对象规模**当前不可用**：一次 run 要 4.5 分钟，交互/CI 都不现实。
- 小模型（M/M/1、典型教学模型）的吞吐是健康的：**约 170 万事件/秒**。
- 内存不是主要障碍（100k 对象 397 MB，线性增长）。

**不能说明**：

- 不能把 1.7M events/s 外推到大规模模型；事件/秒强烈依赖对象数。
- 没有测 1 亿事件（需要小对象数 + 长 stop_time；本页只覆盖对象规模维度）。
- 没有与优化版本对比（v0.11.0 是当前唯一基线）。

## 6. 待优化项（对应 BACKLOG C5）

| 项 | 影响 | 建议 |
|---|---|---|
| validate 超线性 | 100k 对象 85 s | 为连接/对象建立索引（name → index 的 HashMap 一次构建），避免重复线性扫描 |
| run 中对象查找 | 552 events/s | 事件里用对象 id 而非名字查找；连接预解析成端口数组 |
| 报告生成 | 100k 行 CSV 8.4 MB | 已含在 run 时间里；若需要可单独测 `--format csv` vs `--format json` |
| 1 亿事件 | 未测 | 用 4 对象 + stop_time≈1e7 测长跑；确认事件堆没有退化 |
| 并行复现 | `--parallel` 未纳入本页 | 加一个 5 复现并行/串行对比 |

## 7. 复现与记录

```sh
# 1) 构建 release 二进制
cargo build --release --locked -p openflex-cli

# 2) 10 万对象基线（约 10 分钟；输出 JSON + 可直接贴进表格的行）
scripts/bench-large-model.sh --objects 100000 \
  --json /tmp/bench-100k.json --markdown /tmp/bench-row.md

# 3) 小模型对照
cargo run --release -p openflex-cli --example bench_mm1 -- --stop 100000
```

每次记录写一行（`--markdown` 的格式）：

| 日期 | 主机 | 对象数 | 模型大小 | validate s | validate MB | run s | run MB | 事件数 | events/s | items/s |
|---|---|---|---|---|---|---|---|---|---|---|
| 2026-11-15 | Darwin 24.6.0 (x86_64) | 100000 (25000 chains) | 14.2 MB | 85.12 | 134 | 271.56 | 397 | 150000 | 552 | 92 |
| 2026-11-15 | Darwin 24.6.0 (x86_64) | 10000 (2500 chains) | 1.4 MB | 0.35 | 19 | 0.86 | 45 | 15000 | 17390 | 2898 |
| 2026-11-15 | Darwin 24.6.0 (x86_64) | 1000 (250 chains) | 0.1 MB | 0.04 | 3 | 0.05 | 7 | 1500 | 29560 | 4927 |

> 基准数据只用于同机对比；macOS 与 Linux、不同 CPU 之间不要直接比绝对值。
