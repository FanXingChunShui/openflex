# 性能与规模基线

本页记录 OpenFlex 在**大模型**上的实测性能与 profiling 结论：模型解析/校验、仿真运行、
内存峰值，以及与 `bench_mm1`（小模型吞吐）的对比。对应 `docs/BACKLOG.md` C5「性能与规模」。

> **结论先行（v0.25.0，2026-11-15）**
> - 10 万对象：`validate` **0.79 s**、`run` **2.31 s**（64,805 events/s、10,801 items/s），
>   峰值内存 158 / 393 MB。相比 v0.11.0（85.12 s / 271.56 s / 552 events/s）**快约 115×**，
>   扩展性接近线性（10× 对象 → ~10× 时间，此前是 242×/316×）。
> - 10k 对象（CI 基准规模）：validate **0.09 s** / run **0.21 s**，CI 预算 2.0 s / 5.0 s 有 **22×** 余量。
> - 小模型（M/M/1）：**1.52M events/s**（v0.16 基线，v0.20 未重测）。
> - **CI 的 `bench` job 对 10k 模型设 validate < 2 s、run < 5 s 的硬门槛**（基线的 12–14× 余量）。
> - profiling 结论（v0.14.0 采样，v0.15/0.16 未改这两条热路径）：`validate` 的 **70.8%** 在
>   serde/serde_json 反序列化，其中 **50.5% 是 `serde_ignored` 的未知字段跟踪**；
>   `run` 的 **72%** 在引擎事件推进（`flow`/`flow_queue`/`notify_*`），参数 `clone` 约 6%，
>   `report`/CSV 写出 < 1%。

## 1. 测量方法

工具：[`scripts/bench-large-model.sh`](../../scripts/bench-large-model.sh)（基准）、
[`scripts/profile-large-model.sh`](../../scripts/profile-large-model.sh)（macOS `sample` + `atos` 火焰图/热点）。

```sh
# 快速基准（10k 对象，约 0.3 秒）
scripts/bench-large-model.sh

# 10 万对象基线（v0.14.0 约 4 秒；输出 JSON + 可直接贴表的 markdown 行）
scripts/bench-large-model.sh --objects 100000 --json /tmp/bench-100k.json --markdown /tmp/row.md

# 只生成模型（给 profiler 用）
scripts/bench-large-model.sh --objects 100000 --generate-only /tmp/bench-100k
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

## 3. 实测结果（macOS 15.8.1 x86_64，release 构建 v0.25.0）

| 场景 | 对象数 | 事件数 | validate | run | 峰值 RSS（validate / run） | events/s | items/s |
|---|---|---|---|---|---|---|---|
| `bench_mm1`（内置 M/M/1） | 4 | 120,318 | — | 0.079 s | —（未测） | **1,516,812** | 252,802 |
| 大模型 10k | 10,000 | 15,000 | 0.09 s | 0.21 s | 19.1 / 45.2 MB | 71,688 | 11,948 |
| 大模型 100k | 100,000 | 150,000 | **0.79 s** | **2.31 s** | 158.5 / 393.1 MB | **64,805** | 10,801 |

> `bench_mm1` 与 1k 的数据是 v0.16.0 基线（v0.20.0 未重测小模型；大模型三档以 v0.20.0 为准）。

> 单次测量有波动：同一台机器上 10k 的 validate/run 在 0.08–0.14 s / 0.20–0.40 s 之间浮动
> （文件缓存、CPU 频率、后台负载）。表中取较保守的一次；CI 门槛按波动上限留余量布置。

`bench_mm1` 的命令与输出（对照用）：

```sh
cargo run --release -p openflex-cli --example bench_mm1 -- --stop 100000
# items completed: 20053
# events processed: 120318
# untraced: 0.069 s -> 1747867 events/s, 291311 items/s
# traced:   0.099 s -> 1212762 events/s, 202127 items/s (tracing overhead included)
```

### 历史对比（同一模型）

| 指标 | v0.11.0 | v0.21.0 | v0.23.0 | v0.24.0 | v0.25.0 | v0.25 vs v0.11 |
|---|---|---|---|---|---|---|
| 100k `validate` | 85.12 s | 0.75 s | 0.76 s | 0.78 s | **0.79 s** | **~108×** |
| 100k `run` | 271.56 s | 2.21 s | 2.46 s | 2.36 s | **2.31 s** | **~117×** |
| 100k events/s | 552 | 67,993 | 60,922 | 63,634 | **64,805** | **~117×** |
| 100k 峰值 RSS（run） | 396.7 MB | 392.0 MB | 393.6 MB | 386.7 MB | 393.1 MB | ≈ 持平 |
| `bench_mm1` events/s | 1,709,048 | 1,516,812 | （未重测） | （未重测） | — |

> v0.11.0 的数据来自本页旧版（同机同模型）；从 v0.12 起引入的对象索引/全局表等改动
> 把超线性查找消掉了，所以这不再是"规模不可用"的模型。

## 4. 扩展性分析（v0.14.0）

| 规模变化 | validate 时间 | run 时间 |
|---|---|---|
| 10k → 100k（对象 ×10） | 0.09 → 0.79 s（**×8.8**） | 0.21 → 2.31 s（**×11**） |

> 10× 对象 → 9.1×/12× 时间，整体是**线性到略超线性**（run 的额外 2× 来自每事件的对象访问
> 与内存分配，见 §5.3），没有 `n²` 迹象。

- 10× 对象带来 9.4×/13× 时间，**基本线性**；run 略高于线性（×13）主要来自事件数固定
  但每事件的对象访问变多，以及内存分配（见 profiling）。
- 事件数只与链数线性相关（25k 链 → 150k 事件）；100k 对象 57.5k events/s 与 10k 对象
  74.7k events/s 同一量级，说明规模已不再是数量级问题。
- 内存线性：158 MB（validate）/ 393 MB（run，含 14 MB 模型与 8.4 MB 报告）。

### 4.1 CI 性能门槛（`bench` job）

`.github/workflows/ci.yml` 的 `bench` job 在每个 PR 上跑 10k 对象基准，把耗时写进 job summary，
并**阻塞**超过预算的构建：

| 阶段 | 预算 | v0.25.0 本机基线 | 余量 |
|---|---|---|---|
| `validate` | **2.0 s** | 0.09 s | **22×** |
| `run` | **5.0 s** | 0.21 s | **24×** |

为什么这么宽：CI runner 是共享的，本机基线本身在 0.08–0.40 s 间波动；门槛要抓的是
**数量级回归**（例如 O(n²) 查找重新出现在 validate 或事件循环里），而不是 20% 抖动。
如果确实观测到 runner 噪声导致的失败，先把预算上调并在这里记录原因，不要删除 job。

实现：`scripts/bench-large-model.sh --objects 10000 --json bench.json --markdown bench-row.md`
→ CI 的 python 断言读取 `bench.json` 的 `validate.seconds` / `run.seconds`；
失败时打印 `::error::`，并把这次结果与 `bench-row.md` 作为 artifact 保留（30 天）。

## 5. Profiling 结论（macOS `sample` + `atos`）

### 5.1 方法

release profile 里 `strip = true`，`sample` 只能看到 `???`。不修改 `Cargo.toml` 的
做法是构建一个**带符号的临时二进制**，并用 `atos` 把采样地址还原成函数名：

```sh
# 1) 带符号构建（输出到独立 target 目录，不影响发布产物）
cargo build --release --locked -p openflex-cli \
  --target-dir /tmp/prof-target \
  --config 'profile.release.strip=false'

# 2) 采样（脚本会自动用 atos 解析 openflex 的地址）
scripts/profile-large-model.sh --binary /tmp/prof-target/release/openflex \
  --objects 1000000 --skip-run --warmup 1 --sample-seconds 8 --out-dir /tmp/prof-validate --keep

# 3) run 需要更多事件：对象数不变、items/source 提高
scripts/profile-large-model.sh --binary /tmp/prof-target/release/openflex \
  --objects 100000 --items 40 --skip-validate --warmup 2 --sample-seconds 8 --out-dir /tmp/prof-run --keep
```

`profile-large-model.sh` 会把 `sample` 的原始文件、每阶段耗时与 `profile.md` 摘要写进
`--out-dir`；`--keep` 保留现场（默认删除）。`sample` 输出里 `???` 的地址会用
`atos -o <binary> -l <load address> <addr>` 还原符号（Rust 符号保留 mangled 名，能看出
`openflex_core::engine::Simulation::flow` 这类路径）。

### 5.2 `validate` 热点（100 万对象，24,755 样本，总耗时 10 s）

| 函数 | 样本 | 占比 |
|---|---|---|
| `serde_ignored::TrackedSeed<…openflex_model::model::Model::parse_json…>` | 4297 | 50.5% |
| `szone_malloc_should_clear` | 541 | 6.4% |
| `free_tiny` | 358 | 4.2% |
| `serde_ignored::MapAccess<…Model::parse_json…>::next_value` | 290 | 3.4% |
| `serde_ignored::MapAccess<…>::next_entry` | 290 | 3.4% |
| `serde_json::value::de::MapDeserializer::next_entry_seed` | 287 | 3.4% |
| `_platform_memcmp` | 264 | 3.1% |
| `BTreeMap<String, Value>::insert`（未知字段收集） | 193 | 2.3% |
| `tiny_malloc_should_clear` | 160 | 1.9% |
| `serde_ignored::Wrap<Value>::deserialize`（visit_borrowed_str） | 146 | 1.7% |
| `hashbrown::HashMap<…>::get` | 137 | 1.6% |
| `openflex_model::validate::validate_object` | 130 | 1.5% |

分类占比：**JSON/serde 反序列化 70.8%**、内存分配/释放 17.7%、字符串/UTF-8 5.0%、
哈希/查找 3.0%、语义校验 **2.0%**、其他 1.4%。

**解读**：`validate` 的慢不是"逐对象校验逻辑重"（`validate_object` 只占 1.5%），
而是**反序列化**：`Model::parse_json` 走 `serde_ignored` 逐字段跟踪未知键，仅这一层就
占 50.5%；再加上 `serde_json::Value` 中转、`BTreeMap` 插入与大量 `String`/`Vec` 分配，
解析 14 MB JSON 的代价远高于校验本身。

### 5.3 `run` 热点（100k 对象 × 40 items ≈ 600 万事件，5,531 样本，总耗时 11 s）

| 函数 | 样本 | 占比 |
|---|---|---|
| `openflex_core::engine::Simulation::flow` | 1866 | 33.7% |
| `Simulation::flow_queue` | 1194 | 21.6% |
| `Simulation::notify_upstreams` | 527 | 9.5% |
| `Simulation::notify_pullers` | 423 | 7.6% |
| `Simulation::flow_processor` | 269 | 4.9% |
| `Simulation::drain_out_pending` | 218 | 3.9% |
| `SourceParams::clone` | 137 | 2.5% |
| `free_tiny` | 118 | 2.1% |
| `ProcessorParams::clone` | 105 | 1.9% |
| `tiny_malloc_should_clear` | 74 | 1.3% |
| `Triggers::clone` | 57 | 1.0% |
| `Simulation::set_phase` | 54 | 1.0% |
| `Action::clone` | 20 | 0.4% |

（采样中最栈顶的叶子帧分类：引擎事件推进 ≈ 72%，参数克隆 ≈ 6%，内存分配 ≈ 6.6%，
`validate`/`report`/CSV 相关 < 1%。）

**解读**：

- `run` 的开销集中在**事件推进与上下游通知**（`flow` + `flow_queue` + `notify_*` ≈ 72%）。
  `notify_upstreams` / `notify_pullers` 会在每次状态变化时广播给邻居，即使邻居没有状态变化。
- **参数 `clone` 约 6%**：`SourceParams` / `ProcessorParams` / `Triggers` / `Action` 在热路径上
  被克隆（每次取参数或建对象时）。这是最容易拿的一段收益（改成引用/`Arc`）。
- **`report` 聚合与 CSV 写出 < 1%**：8.4 MB 报告不是瓶颈，之前"run 慢可能因为报告"的猜测不成立。
- `validate` 在 run 里只占 0.1%，说明 run **没有重复做完整校验**（此前怀疑不成立）。

### 5.4 具体优化建议（按收益/风险排序）

| # | 位置 | 建议 | 预期收益 | 风险 |
|---|---|---|---|---|
| 1 | `openflex_model::model::Model::parse_json` | 去掉 `serde_ignored` 热路径：默认用 `serde_json::from_slice::<Model>`（或 `deny_unknown_fields`），只在 `--strict`/`validate --unknown` 时启用未知键收集；或解析后对原始 JSON 做一次"已知键集合"比对（不建 `BTreeMap`） | validate 最高可省 ~50% 的解析时间 | 需要保持 A1 的"未知字段报错"语义与退出码 |
| 2 | 同上 | 避免 `serde_json::Value` 中转与 `BTreeMap<String, Value>`：未知键用 `Vec<(String, Value)>`（本题只有少量），或直接跳过存储只计数 | 省 2–3% + 分配 | 低 |
| 3 | `Simulation::flow*` / `notify_*` | 把"通知上游/下游"改成 dirty 标记 + 批量处理（每步只处理变化过的对象），或缓存"该端口是否可推进" | run 10–20% | 中（需保持语义/确定性） |
| 4 | 引擎热路径 | 参数 `clone` 改引用或 `Arc<Params>`；`process_time` 的分布采样可预计算/缓存 | run ~5% | 低 |
| 5 | 引擎/报告 | `Vec` 预分配（`with_capacity(objects)`）、字符串 intern（对象名在报告中重复出现） | 内存 -10~20% | 低 |
| 6 | 解析层 | 若 `serde_json` 仍是瓶颈，评估 `simd-json`/`yyjson`（需要 `&mut [u8]` 与 unsafe 审查） | 解析再 -20~30% | 中（依赖与新 unsafe） |
| 7 | 命令行 | `--strict`（未知字段检查）与默认解析拆成两条路径，并在文档标出性能差异 | — | 低 |

> 本任务只做 profiling 与建议，未修改 `crates/`。落地 1/3/4 中的任意一项，都建议用
> `scripts/profile-large-model.sh` 前后各采一次样对比（同一模型、同一采样时长）。

## 6. 这组数据能说明什么、不能说明什么

**能说明**：

- 10 万对象**已经可用**：validate 0.75 s、run 2.61 s；不再是"规模不可用"。
- 小模型吞吐健康（**1.75M events/s**），大规模（57.5k events/s）与对象数基本线性。
- 瓶颈位置明确：validate 在反序列化（`serde_ignored` 占一半），run 在事件推进与通知。

**不能说明**：

- 不能外推到 1 亿事件（未测；需要小对象数 + 长 stop_time）。
- 不同机器/OS 之间不要直接比绝对值（本页全部为 macOS 15.8.1 x86_64）。
- `sample` 只采最栈顶叶子帧，行号/内联信息缺失；百分比是"时间占比"而非精确指令数。

## 7. 复现与记录

```sh
# 1) 构建 release 二进制
cargo build --release --locked -p openflex-cli

# 2) 10 万对象基线（v0.14.0 约 4 秒）
scripts/bench-large-model.sh --objects 100000 \
  --json /tmp/bench-100k.json --markdown /tmp/bench-row.md

# 3) 小模型对照
cargo run --release -p openflex-cli --example bench_mm1 -- --stop 100000

# 4) profiling（带符号二进制 + sample + atos）
cargo build --release --locked -p openflex-cli --target-dir /tmp/prof-target \
  --config 'profile.release.strip=false'
scripts/profile-large-model.sh --binary /tmp/prof-target/release/openflex \
  --objects 1000000 --skip-run --warmup 1 --sample-seconds 8 --out-dir /tmp/prof-validate --keep
scripts/profile-large-model.sh --binary /tmp/prof-target/release/openflex \
  --objects 100000 --items 40 --skip-validate --warmup 2 --sample-seconds 8 --out-dir /tmp/prof-run --keep
```

每次记录写一行（`--markdown` 的格式）：

| 日期 | 主机 | 对象数 | 模型大小 | validate s | validate MB | run s | run MB | 事件数 | events/s | items/s |
|---|---|---|---|---|---|---|---|---|---|---|
| 2026-11-15 | Darwin 24.6.0 (x86_64) | 100000 (25000 chains) | 14.2 MB | 0.79 | 158 | 2.31 | 393 | 150000 | 64805 | 10801 |
| 2026-11-15 | Darwin 24.6.0 (x86_64) | 10000 (2500 chains) | 1.4 MB | 0.09 | 19 | 0.21 | 45 | 15000 | 71688 | 11948 |
| 2026-11-15 | Darwin 24.6.0 (x86_64) | 1000 (250 chains) | 0.1 MB | 0.02 | 4 | 0.02 | 7 | 1500 | 60799 | 10133 |
| 2026-11-15（历史） | Darwin 24.6.0 (x86_64) | 100000 | 14.2 MB | 85.12 | 134 | 271.56 | 397 | 150000 | 552 | 92 |

> 基准数据只用于同机对比；macOS 与 Linux、不同 CPU 之间不要直接比绝对值。
