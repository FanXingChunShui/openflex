# openflex 统计规格：指标定义、实验器语义、报告与图表

> 证据编号见 [evidence.md](../research/evidence.md)。
> **重要区分**：FlexSim 文档明确给出了指标的**类别与语义**（高置信度），但**没有给出闭式公式**。下文中标 `[文档语义]` 的部分来自官方；标 `[openflex 定义]` 的部分是本项目为避免歧义而选定的教科书式定义，不是 FlexSim 原文。

## 1. 统计的分类

官方把标准统计分为五类（E33 高）：

| 类别 | 官方说明 | 采集粒度 |
|---|---|---|
| Content | 跟踪 3D 对象中的件数；默认给 current / min / max / **average** | 随时间变化 |
| State | 对象的当前状态与状态历史（各状态可用性取决于对象类型） | 随时间分段 |
| Staytime | 件在固定资源中的停留时长；默认给 min / max / average | 每件一个样本 |
| Throughput | 通过对象的件数；默认给 input 与 output | 单调累加 |
| Travel | 任务执行器的累计行程（用模型的距离单位） | 单调累加 |

补充：部分对象有专属统计（例：Source 的 staytime 记录"被阻塞时滞留"；Dispatcher/TaskExecuter 有任务序列队列；Combiner/Separator 有批量相关统计）（E03/E05/E13 高）。

## 2. 指标定义

记号：观测区间 `[t0, t1]`（有预热时 `t0 = warmup`，`t1` = 运行结束或停机时刻）；`n` = 区间内离开的件数。

### 2.1 Content（含量）

- `content(t)`：t 时刻对象内的件数；**容器件只计 1**（E02 高）。
- `content_current = content(t1)`
- `content_max = max_t content(t)`，`content_min = min_t content(t)`
- `content_avg = (1/(t1−t0)) · ∫_{t0}^{t1} content(t) dt` —— **时间加权平均**。`[openflex 定义]`（官方只说"average … calculated from the entire model run"，E02 高；时间加权是 DES 领域的标准解释）
- 实现：在每次 content 变更事件上累加 `Σ content_before × Δt`，最后除以区间长度。区间长度为 0 时报 `NaN`/`null`（不得报 0）。

### 2.2 Throughput（吞吐）

- `input`：每有 1 件进入 +1；`output`：每有 1 件离开 +1；**均单调递增**，reset 才归零（E02 高）。
- 任务执行器的 input/output 分别对应"装载/卸载"（E13 高）。
- 派生：`throughput_rate = output / (t1 − t0)`（件/模型时间单位）`[openflex 定义]`。
- **不要**用 `input − output` 当作 content（有在途件与容器件，两者不等价）。

### 2.3 Staytime（停留时间）

- 每件离开时记录一个样本：`staytime = exit_time − entry_time`（E02 高）。
- `staytime_avg = (1/n) Σ x_i`；`staytime_min = min x_i`；`staytime_max = max x_i` —— **样本统计**，不是时间加权（与 Content 的 avg 口径不同！）。
- 任务执行器：`staytime = unload_time − load_time`（E13 高）。
- 建议同时输出分位数 `p50/p90/p95/p99`（`[openflex 定义]`，官方无此字段，但对排队分析必需）。

### 2.4 State（状态）

- 每个状态 s 的累计时长 `T_s`；状态序列在每次 `OnStateChange` 处切分（E02 高）。
- `state_share_s = T_s / Σ_s T_s`（= 该状态占比）（`[openflex 定义]`，官方称"categorical tracked variable … total time at each state"，E02/E24 高）
- **停机时间要单独可见**：被 `stop()` 的对象进入停机状态，占比统计里应能区分"生产性状态"与"停机状态"（E22 高）。
- 状态映射默认六态：`idle/setup/processing/blocked/waiting_for_operator/waiting_for_transport`（Processor 族）；Queue 四态；Source 两态。完整表见 [logic.md](logic.md) §10。

### 2.5 Utilization（利用率）

- `[openflex 定义，文档未给闭式公式]`：对加工类对象
  - `utilization_busy = (T_setup + T_processing) / (t1 − t0)`（"忙碌占比"）
  - `utilization_effective = output × avg(process_time) / (t1 − t0)`（"有效产能占比"）
  - 两者在无返工、无空转 setup 时相等；有 setup 时 `busy ≥ effective`。
- 对任务执行器：`utilization = (T_travel_empty + T_travel_loaded + T_loading + T_unloading + T_utilize) / (t1 − t0)`，另给 `T_idle` 占比（E13 高：状态集合由官方给出，公式为本项目定义）。
- 置信度说明：**状态定义高**，**公式组合中（本项目定义）**。

### 2.6 Travel（行程）

- `travel_distance`：所有 travel 任务的距离之和（E13 高）。v1 按直线距离累计。

### 2.7 文献可验证的对照值

对 M/M/1（到达率 λ、服务率 μ、ρ = λ/μ < 1）的稳态理论值（教科书，非 FlexSim 文档）：

| 量 | 公式 |
|---|---|
| 平均队长（系统中件数，含服务中） | `L = ρ/(1−ρ)` |
| 平均排队长度 | `Lq = ρ²/(1−ρ)` |
| 平均逗留时间 | `W = 1/(μ−λ)` |
| 平均等待时间 | `Wq = ρ/(μ−λ)` |
| 服务台利用率 | `ρ` |
| 等待时间分布 | `P(Wq > t) = ρ·e^{−(μ−λ)t}` |

这些是 golden-cases GC-1 的判据来源，见 [golden-cases.md](golden-cases.md)。

## 3. 报告内容

FlexSim 侧的报告能力（E37 中、E33 高）：

- **标准统计**：每个 3D 对象的 Content / State / Staytime / Throughput / Travel，实时显示在 Quick Properties 的 Statistics 组（E33 高）。
- **旧版报告（Legacy Reports and Statistics）**：面向对象的汇总视图（状态占比、吞吐、含量、停留时间等），是新版 Dashboard/图表的前身（E37 中）。
- **图表模板**：按统计类别分组 —— **Content / Output / Staytime / State / Zone / Milestone**（另有 Label、Tracked Variable、Travel Distance、Process Flow Activity Statistics 模板组）（E33/E34 高：页面明确列出分组）。
- **Statistics Collector**：采集原始事件数据入表，可用 Calculated Table 做过滤/比较/计算（E33 高）。
- **Milestone Collector**：跟踪"里程碑"（件到达某点/某事件发生）的时刻与时长（E33/E34 中）。

openflex v1 报告（`--format json|table|csv`）：

```
runtime:     model, time_unit, stop_time, warmup, base_seed, replications[],
             （每个复现：index, seed, final_time, objects{...}）
objects[]:   kind, input_count, output_count, avg_content, max_content,
             avg_stay_time, state_times{idle,busy,blocked,down,off_shift,
             setup,processing,waiting_for_operator,waiting_for_transport,
             door_opening,door_closing}, utilization, throughput,
             （其余按对象类型：battery_remaining、fluid 液位/流量等）
aggregate:   created/completed（Aggregate：mean, std_dev, min, max, ci95, n）
performance_measures: {name: Aggregate}（模型声明了才出现）
tracked_series:       {name: [(t, value), ...]}（模型声明了才出现）
```

> **字段名以 `crates/openflex-core/src/stats.rs` 的 `ObjectStats` / `StateTimes` / `Aggregate`
> 为准**（2026-11 核对）。早期草稿曾用 `content_current` / `utilization_busy` / `p95` 这类名字，
> 实现里并不存在——分位数与有效利用率目前**没有**独立字段。

**Little 定律自检**：`content_avg ≈ throughput_rate × staytime_avg`（相对误差 > 5% 时输出 warning）。这是最有价值的自动一致性检查，能立刻暴露"事件结算漏掉最后一段时长"之类的实现错误（`[openflex 定义]`）。

## 4. 实验器（Experimenter）语义

### 4.1 术语与关系（E35 高）

```
Job ──生成──> Task 列表 ──每个 Task = (Scenario, Replication 编号)──> 跑一次模型 ──> Results Database
```

| 术语 | 定义（官方） |
|---|---|
| **Job** | 生成 Scenario 并决定每个 Scenario 跑几次复现的策略。三类：**Experiment**（显式定义场景 + 每场景复现数）、**Range-Based**（给参数取值范围，组合出每个取值一个场景）、**Optimization**（给目标函数，自动搜索；需 OptQuest 授权，**v1 不做**） |
| **Scenario** | 输入向量 = **全部 Model Parameter 值 + 模型 Stop Time + 模型 Warmup Time**；按内容算哈希唯一标识（界面显示后 8 位；自定名称不参与哈希） |
| **Replication** | 同一 Scenario 的独立重复；**随机序列由复现编号决定**（第 1、2 次不同） |
| **Task** | `(Scenario, Replication)` 二元组，模型跑一次；3 场景 × 10 复现 = 30 个 Task |
| **Results Database File** | 每个 Task 完成即写入的独立结果文件；跨 Job 持久；重跑前查重以跳过已完成 Task；**不同模型结果不可混用** |

并行度：默认 = CPU 核数；每核内存 ≈ 一次模型运行的内存（E35 高）。

结果内容：全部 Performance Measure 值 + Dashboard 图表数据 + Statistics Collector/Calculated Table 的表 + System/Output Console 消息（E35 高）。

### 4.2 指标（Performance Measure）的求值时机（E34 高）

- 指标是**输出标量**，只观测不干预；定义在 Performance Measure Table（新模型默认表名 `PerformanceMeasures`）。
- `Value` 是表达式，**在被需要时求值**（典型：每个复现结束时）；`Reference` 可指向对象节点；可用采样器指向"对象 + 其某个统计"。
- 指标**不得引用自身或自身历史**（要历史就用 Tracked Variable 存，再让指标显示它）。
- 指标自动进入实验/优化。

### 4.3 结果汇总与置信区间

官方结果窗口提供（E36 中）：

| 视图 | 内容 |
|---|---|
| Mean + Confidence | 各 Scenario 的均值，并可选择置信水平，围绕均值画区间 |
| Data Summary | 每个 Scenario 的 **Mean / Sample Standard Deviation / Minimum / Maximum**（"Mean Based on" 选置信区间） |
| Scenario Comparison | 以某场景为基线，按复现次数判断两场景均值是否**统计显著不同**并给差值 |

`[openflex 定义]` 闭式实现（官方未给公式）：

```
n 个复现的样本均值  x̄ = (1/n) Σ x_i
样本标准差          s = sqrt( Σ(x_i − x̄)² / (n−1) )
均值标准误          SE = s / sqrt(n)
置信区间（双侧）    x̄ ± t_{1−α/2, n−1} · SE
```

- `n < 2` 时不报区间（报 `null` 并标注 `insufficient_replications`）；
- `t` 分位数的实现与文档同义（`crates/openflex-core/src/stats.rs` 的 `t_critical_95`）：
  **自由度 1..29 查固定表**（双侧 95%，`df=1 → 12.706`、`df=29 → 2.045`），
  **`df ≥ 30` 直接用正态分位 1.96**；**不做线性插值**（用表值直接命中，超出表格范围即切换 1.96）。
  这样小样本区间足够宽、大样本不额外放大；
- 另提供 **batch-means** 区间（`openflex-report/src/batch.rs`、`run --batch-means [N]`，默认 10 批）：
  把**连续的复现**分成 k 批、先算批内均值，再把 k 个批均值当作独立样本，
  用 `k−1` 自由度的 t 分位求 95% 区间。适用于复现级样本噪声较大、
  或一次长运行已被切成多段的情形（`--batch-means-objects` 同时对每对象的
  input/output/avg_wip/utilization 出批均值）；
- **两场景比较**：若两场景区间不重叠 ⇒ 判定显著不同（v1 的保守近似）；正式版建议用双样本 t 检验（Welch：`t = (x̄₁−x̄₂)/sqrt(s₁²/n₁+s₂²/n₂)`）。

## 5. Dashboard 与图表（v1 只输出数据，不渲染 UI）

官方图表分组（E33/E34 高）：基础图（柱/线/饼等通用图）、图表模板（Content / Output / Staytime / State / Zone / Milestone / Label / Tracked Variable / Travel Distance / Process Flow Activity Statistics）。

openflex v1 策略：
- **不实现** Dashboard UI；
- 提供与图表模板**同构**的数据出口：`results.json` 已包含 Content/State/Staytime/Output 所需的全部时序或汇总数据；
- 提供 `openflex report --format csv --series content:Proc1` 之类的导出，供外部绘图（gnuplot/Excel/Python）。
- 若后续要做终端图表，建议 TUI 折线/柱状即可，不追求 3D。

## 6. 统计实现检查表

1. Content 的 avg 是**时间加权**，Staytime 的 avg 是**样本平均** —— 两者口径不同（E02 高 + `[openflex 定义]`）。
2. 预热要清空**状态累计**，否则最后一个状态的时长会跨过预热边界（runtime.md §3）。
3. 停止时先"结算"（等价 `updatestates()`），把当前状态与 content 的时长补到停止时刻，否则报表少最后一段（E32 高）。
4. 容器件只把 content 加 1（E02 高）。
5. input/output 单调递增，**只在 reset 归零**（E02 高）。
6. 统计时长为 0 时不得输出 0 或崩溃，应输出 `null` + 原因。
7. 每复现独立累计，跨复现不可复用累加器（否则复现 2 的值会带上复现 1 的残留）。
8. Scenario 必须包含 stop_time 与 warmup（E35 高），否则实验比较不成立。
9. 复现编号必须进入随机流派生，否则 N 个复现会得到完全相同的样本（E35 高）。
10. Little 定律自检（§3）建议默认开启。
