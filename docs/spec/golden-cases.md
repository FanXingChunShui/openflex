# openflex 黄金案例（golden cases）

本文档给出**可由公开材料数值验证**的最小模型，配套模型文件在 `examples/golden/*.ofm`。
每个案例都写明：模型参数 → 期望指标 → 推导 → 允许误差 → 依据。

- **判据来源**只用两类：
  1. **FlexSim 公开文档给出的语义/默认行为**（见 [evidence.md](../research/evidence.md)）；
  2. **公开教科书结论**（排队论闭式解、确定性算术）。**不使用**任何 FlexSim 运行结果截图或私有实现。
- 因此这些案例验证的是"**语义一致性 + 数值正确性**"，而不是"与 FlexSim 逐位相同"。前者是净室可验证的目标；后者需要运行 FlexSim，本轮**未做**（也不在允许范围内）。
### 指标名对照（阅读本文前请先看）

| 本文（FlexSim 语义） | 实现输出字段（`crates/openflex-report`） | 含义 |
|---|---|---|
| `output` / `input` | `output_count` / `input_count` | 单调累加计数 |
| `content_avg`（时间加权） | `avg_content` | `∫content dt / 统计时长` |
| `content_max` | `max_content` | 峰值含量 |
| `staytime_avg` | `avg_stay_time` | 样本平均停留 |
| `utilization_busy` | `utilization` | 忙碌时间占比 |
| `states.processing` | `state_times.busy` | 加工/忙碌状态 |
| `states.setup` | *无独立字段* | 当前实现把 setup 计入 `busy` |
| `states.idle` / `blocked` / `down` / `off_shift` | `state_times.{idle,blocked,down,off_shift}` | 同名 |
| `states.waiting_for_transport` / `waiting_for_operator` | *无* | 未建模（见 flexsim-to-openflex §13） |

- 所有 `.ofm` 的顶层结构严格遵循约定：`format / version / name / time_unit / run{stop_time,warmup,seed,replications} / objects[] / connections[]`。
  少数参数键（`batching`、`breakdown`、`combine_mode`、`components`、`separate_mode`、`separate_qty`）属于 **openflex 提案扩展**，在 §8 统一说明；标为"约定内"的键与任务书示例完全一致。

---

## GC-1 `mm1.ofm` — M/M/1 队列（统计一致性）

**目的**：验证"指数到达 + 指数服务 + 单服务台"在长时间运行下的统计量收敛到教科书 M/M/1 解。这是最能暴露事件/统计实现错误（时间加权写错、停留时间样本错、利用率口径错）的案例。

### 模型参数（全部属于约定字段）

| 项 | 值 |
|---|---|
| `time_unit` | `seconds` |
| `run.stop_time` | 48000.0（13h20m 模型时间） |
| `run.warmup` | 1000.0 |
| `run.seed` / `replications` | 1 / 5 |
| Source1 | `interarrival = exponential(mean=5.0)`，`items_per_arrival=1` |
| Queue1 | `max_content=1000000`（保证 Source 永不被阻塞），`order=fifo` |
| Proc1 | `setup_time=constant(0)`，`process_time=exponential(mean=4.0)`，`max_content=1` |
| Sink1 | — |

### 关键语义对齐（必读）

FlexSim 的 inter-arrivaltime 定义为"**上一件离开 Source → 下一件到达**"的时间，而 M/M/1 的 λ 定义是"到达→到达"（E03 高）。
文档给出的标准做法是：**在下游放一个大容量 Queue，使 Source 一创建就能立刻送出** —— 本例正是 `Queue1.max_content = 1e6`（E03 高）。
⇒ 满足该条件时，两种定义等价，模型才是真正的 M/M/1。

### 理论值（教科书）

λ = 1/5 = 0.2，μ = 1/4 = 0.25，ρ = λ/μ = **0.8**

| 指标 | 公式 | 值 |
|---|---|---|
| 平均队长（含服务中） | `L = ρ/(1−ρ)` | **4.0** |
| 平均排队长度 | `Lq = ρ²/(1−ρ)` | **3.2** |
| 平均逗留时间（系统） | `W = 1/(μ−λ)` | **20.0** |
| 平均等待时间（队列） | `Wq = ρ/(μ−λ)` | **16.0** |
| 服务台利用率 | `ρ` | **0.8** |

### 期望的 openflex 指标（映射见 statistics.md）

| openflex 指标 | 期望值 | 说明 |
|---|---|---|
| `Proc1.avg_content` | 0.80 | 服务台"含服务中"的平均件数 = ρ |
| `Queue1.avg_content` | 3.20 | 队列平均等待件数 = Lq |
| `Queue1.avg_stay_time` | 16.0 | 件在 Queue 的等待时长 = Wq |
| `Proc1.avg_stay_time` | 4.0 | 服务时长均值 = 1/μ |
| `Proc1.utilization` | 0.80 | (settle+process 时间)/统计时长 |
| `Source1.output_count` ≈ `Sink1.input_count` | ≈ 0.2 × 47000 = 9400 | 统计区间 = stop − warmup = 47000 |
| `wip_check`（Little） | `avg_content ≈ throughput × avg_stay_time` | 自动自检应通过 |

### 允许误差

| 口径 | 容差 | 理由 |
|---|---|---|
| 单次复现 | **±12%** | M/M/1 在 ρ=0.8 时 W 的重尾性使单次抽样波动较大；47000s 内约 9400 个样本，但自相关强 |
| **5 次复现的均值** | **±6%** | 平均值方差降至 1/√5；且 `warmup=1000` 只覆盖 200 个到达，残余瞬态仍存在 |
| 队列/服务台含量之比 `Lq/L = ρ` | ±8% | 结构性关系，比绝对量更稳 |
| 利用率 | ±0.02（绝对值） | 利用率样本量大、收敛快 |

**失败诊断**：若 `Queue1.avg_content` 偏小但 `avg_stay_time` 正常 ⇒ 含量统计漏了最后一段（未在停止时结算，见 statistics.md §6 第 3 条）。若 `avg_stay_time` 恰好等于 `process_time` 均值 ⇒ 停留时间测的是 Processor 而非 Queue 的进出点。

---

## GC-2 `det-line.ofm` — 确定性单机线（精确算术）

**目的**：验证事件调度与吞吐计数的**精确性**（无随机，答案唯一）。

### 模型参数

| 项 | 值 |
|---|---|
| `run.stop_time` | 480.0；`warmup` 0；`seed` 1；`replications` 1 |
| Source1 | `interarrival = constant(10.0)`，首件在 **t=0**（见 §8 `time_zero_arrival` 默认 true） |
| Queue1 | `max_content=1000`，fifo |
| Proc1 | `setup=0`，`process = constant(8.0)`，`max_content=1` |
| Sink1 | — |

### 推导（逐步）

1. 到达时刻：`0, 10, 20, …, 470` ⇒ 共 **48 件**（第 49 件在 t=480，恰好等于 stop_time，按"停止点为 ≥ stop_time 的首个检查点"语义不处理；即便处理也来不及完成，见 E32 高）。
2. 每件到达时 Processor 都空闲（前一间在到达前 2 秒已完成）⇒ **Queue 内容恒为 0**，无等待。
3. Processor 忙碌区间：`[0,8), [10,18), …, [470,478)` ⇒ 总忙碌 **48 × 8 = 384**。
4. 全部 48 件在 478 前完成 ⇒ `Sink1.input_count = 48`。

### 期望指标（精确，容差 0 或浮点 1e-9）

| 指标 | 期望 |
|---|---|
| `Source1.output_count` | 48 |
| `Sink1.input_count` | 48 |
| `Proc1.input_count` / `output_count` | 48 / 48 |
| `Proc1.avg_content` | 384/480 = **0.8** |
| `Queue1.avg_content` | **0.0** |
| `Queue1.avg_stay_time` | `null`（无样本） |
| `Proc1.avg_stay_time` | **8.0** |
| `Proc1.utilization` | **0.8** |
| `Proc1.state_times.idle` 占比 | **0.2** |
| `Proc1.state_times.busy` 占比 | **0.8** |

### 允许误差

**0**（确定性模型；仅允许浮点 1e-9）。若出现 47 或 49，说明到达序列或停止边界处理有 bug。

**失败诊断**：若 `Queue1.avg_content = 0` 但 `Queue1.state_times.busy > 0` ⇒ Queue 的"收到即释放"语义被实现成了"停留一拍"，需要按 E05/E02 修正（Queue 收到即释放）。

---

## GC-3 `batching.ofm` — 队列攒批（批量语义 + 精确时间加权）

**目的**：验证 Queue 的 `Perform Batching` 语义（E06 高）：攒够 `batch_size` 才**逐件**放行；同时精确校验"时间加权平均含量"与"批次导致的批间空闲"。

### 模型参数

| 项 | 值 |
|---|---|
| `run.stop_time` | 440.0；`warmup` 0 |
| Source1 | `interarrival = constant(10.0)`，首件 t=0 |
| Queue1 | `max_content=1000`，fifo，**`batch_size=5` + `batch_max_wait=0.0`**（0=无限等待，E06 高） |
| Proc1 | `process = constant(4.0)`，`max_content=1` |
| Sink1 | — |

### 推导

1. 到达：`0,10,…,390` ⇒ **40 件**；每 5 件成一批 ⇒ **8 批**。
2. 第 k 批在第 `5k` 件到达时攒齐，释放时刻 `r_k = 40 + 50(k−1)`，即 `40, 90, 140, 190, 240, 290, 340, 390`。
3. 释放是**逐件**送下游（E06 高），Processor 串行加工 4s/件 ⇒ 每批占 Processor **20s**，批间空闲 30s。
4. 批 8 于 390 释放，加工到 `390+20=410 < 440` ⇒ 全部 40 件完成。
5. **Queue 时间加权含量**：批 k 的件在 `r_k−40, r_k−30, r_k−20, r_k−10, r_k` 到达，故区间 `[r_k−40, r_k)` 内 content 依次为 `1,2,3,4`（每段 10s），在 `r_k` 瞬时跳到 5 并立即释放。
   - 每批积分 = `10×(1+2+3+4) = 100` 秒·件；批 1 从 t=0 起始，之后每批前有 10s 的 0；
   - 8 批合计 `800` ⇒ `800/440 = 1.8182`。
6. **Queue 停留时间**：批内 5 件的等待分别为 `40,30,20,10,0` ⇒ 均值 **20.0**，最小 0，最大 40。

### 期望指标

| 指标 | 期望 |
|---|---|
| `Source1.output_count` / `Sink1.input_count` | **40** / **40** |
| `Queue1.avg_content` | **1.8182**（440 内 800/440） |
| `Queue1.max_content` | 5 |
| `Queue1.avg_stay_time` | **20.0**（min 0 / max 40） |
| `Queue1.state_times.busy` 占比 | 390/440 = **0.8864**（攒批期间；不含瞬时释放） |
| `Proc1.utilization` | 160/440 = **0.36364** |
| `Proc1.state_times.idle` 占比 | **0.63636** |
| `Proc1.avg_content` | **0.36364** |
| `Proc1.avg_stay_time` | **4.0** |

### 允许误差

| 项 | 容差 |
|---|---|
| 计数类（input/output/max） | **0** |
| `avg_content` / `utilization` / 状态占比 | **±0.002**（浮点累积） |
| `avg_stay_time` | **±0.01** |

**失败诊断**：若 `Sink1.input_count = 40` 但 `Queue1.avg_stay_time = 0` ⇒ 实现了"攒够后清空内容但不记录停留"。若 `Proc1.utilization = 0.909`（=40×10/440）⇒ 忽略了批间空闲，把批释放当成了流水释放。

---

## GC-4 `breakdown.ofm` — 故障维修（MTBF/MTTR 可用率）

**目的**：验证停机模型（E21 高）与"只有特定状态才累计 Up Time"的语义（E21 高：可指定只让 processing 状态计入 Up Time）。

### 模型参数

| 项 | 值 |
|---|---|
| `run.stop_time` | 48000.0；`warmup` 0 |
| Source1 | `interarrival = constant(6.0)` |
| Queue1 | `max_content=100000` |
| Proc1 | `process = constant(8.0)`，`max_content=1`，**`failure = {mtbf: constant(100), mttr: constant(10)}`** |
| Sink1 | — |

### 为何这个案例不自相矛盾

- 到达率 `1/6 ≈ 0.1667/s` > 标称产能 `1/8 = 0.125/s` ⇒ **队列永远有货**，Processor 从不空闲；
- 因此 FlexSim 的"Up Time 只在 processing 计时"（E21 高）与"按墙钟计时"**给出相同结果**——这消除了两种解释的歧义，使案例可以精确判定（当前 openflex 只实现墙钟计时，见 flexsim-to-openflex §7.1）。

### 推导

1. 可用率 `A = MTBF/(MTBF+MTTR) = 100/110 = 0.90909`。
2. 每个 up-down 周期长 `110s`，其中可加工 `100s` ⇒ 每周期产出 `100/8 = 12.5` 件。
3. `48000 / 110 = 436.36` ⇒ 436 个完整周期（`436 × 110 = 47960s`），产出 `436 × 12.5 = 5450` 件。
4. 余下 `40s` 处于新周期 up 段 ⇒ 再产出 `40/8 = 5` 件。
5. **总产出 = 5455**。

### 期望指标

| 指标 | 期望 | 容差 |
|---|---|---|
| `Sink1.input` | **5455** | ±1%（≈±55） |
| `Proc1.state_times.busy` 占比 | 100/110 = **0.9091** | ±0.002 |
| `Proc1.state_times.down` 占比 | 10/110 = **0.0909** | ±0.002 |
| `Proc1.state_times.idle` 占比 | ≈ **0** | ≤0.001 |
| 产出率 | `5455/48000 = 0.11365` 件/s | ±1% |

> **注意**：`busy` 与 `down` 占比之和 ≈ 1（无 idle），这是本案例的自检式断言。若实现把"故障计时"错误地挂在墙钟上且在 idle 时也累积，虽然总数不变，但一旦把 `Queue1` 改小（使 Proc 出现 idle）就会失配——因此 §8 建议后续加一个变体案例。

### 允许误差的说明

±1% 而非 0 的原因：**停机打断在制品**的处理方式在文档中未明确（是"完成当前件后停机"还是"立即打断、恢复后补足剩余加工时间"）。两种实现对 48000s 的总产出影响 < 1 件，但会影响 `busy` 占比的第 4 位小数。**建议**实现采用"立即打断 + 恢复后续加工剩余时间"（与"processing 状态才累计 up time"的语义自洽），并在报告中输出 `preemption_events` 计数以便诊断。

---

## GC-5 `combiner-pack.ofm` — 装箱（pack 语义）

**目的**：验证 Combiner 的三条硬语义（E09 高）：①**必须先收到端口 1 的容器件**；②`components` 第 1 行对应**端口 2**；③`pack` 模式把零件装入容器件后只把**容器件**释放下游。

### 模型参数

| 项 | 值 |
|---|---|
| `run.stop_time` | 500.0 |
| TraySource | `interarrival = constant(50.0)` ⇒ 托盘 `0,50,…,450`（10 个） |
| BoxSource | `interarrival = constant(10.0)` ⇒ 箱 `0,10,…,490`（50 个） |
| Combiner1 | `mode="pack"`，**`components_per_container=4`**（每个托盘装 4 个箱） |
| Sink1 | — |
| 连接顺序 | `TraySource→Combiner1` 为**第一条**连接（= 输入端口 1），`BoxSource→Combiner1` 为第二条（= 输入端口 2） |

### 推导

- 每 50s 供 1 个托盘、每 10s 供 1 个箱 ⇒ 每托盘需求 4 箱 = 40s < 50s，**箱供给充足**；
- 500s 内托盘供给 10 个 ⇒ 产出 **10 个托盘件**；
- 每个托盘消耗 4 箱 ⇒ 消耗 40 箱（50 个箱中 10 个剩余，停在 Queue/Source 侧）。

### 期望指标

| 指标 | 期望 | 容差 |
|---|---|---|
| `Sink1.input_count`（= 托盘数） | **10** | **±1** |
| `Combiner1.output_count` | 10 | ±1 |
| `Combiner1` 内容峰值 | 1 个托盘容器（装入其中的箱不单独计内容，E02 高） | 见 §8 |
| `BoxSource.output_count` | 40~50（受端口 2 阻塞影响） | 不判定 |

### 为什么这里必须给 ±1 而不是 0

`t = 50k` 时刻**托盘与箱可能同时到达**（50、100、…），此时"先收容器件还是先收零件"由**同刻事件排序**决定，而 **FlexSim 文档未定义同刻事件的排序规则**。因此：
- 「50s 内产出 10 或 9 个托盘」都算通过；
- 但**必须**满足结构性断言：产出是**托盘件数**而非箱数（若得到 40~50，说明 pack 被实现成了 batch 或 join）。

> **给实现者的要求**：openflex 必须**自行定义并公开**同刻事件排序规则（建议：`(time, priority, 创建序号)`，对象创建顺序参与比较），并在 `check` 子命令里提示"该模型对同刻顺序敏感"。这条是净室重建里最容易被忽略的**行为定义缺口**。

---

## GC-6 `separator-unpack.ofm` — 拆包（unpack 的 "+1" 语义）

**目的**：验证 Separator unpack 与 split 的**数量差异**（E10 高），这是最容易差 1 件的语义。

### 模型参数

| 项 | 值 |
|---|---|
| `run.stop_time` | 500.0 |
| Source1 | `interarrival = constant(50.0)` ⇒ 容器件 `0,50,…,450`（10 个） |
| Sep1 | **`split_count=3`**（当前实现的唯一拆分参数；`unpack`/`split` 语义待明确，见 §9.3） |
| Sink1 | — |

### 推导

- **unpack 语义**：解包 `qty=3` 件，**容器件本身仍会被释放** ⇒ 每 1 个容器件产出 `3 + 1 = 4` 件（E10 高）；
- 10 个容器件 ⇒ `10 × 4 = 40` 件进 Sink；
- 每 50s 一个容器件，解包顺序为"从后往前"（E10 高），但总量与顺序无关。

### 期望指标

| 指标 | 期望 | 容差 |
|---|---|---|
| `Sink1.input_count` | **40** | **0** |
| `Sep1.input_count` | 10 | 0 |
| `Sep1.output_count` | 40 | 0 |

### 对照检查（split 模式应有的结果）

若把 `separate_mode` 改成 `"split"`，则 `separate_qty=3` 表示"总产出 3 件"（复制 qty−1 次），期望 `Sink1.input_count = 30`。
**这个对照必须写进单元测试**：同一 `qty=3` 在两种模式下差 10 件（本例），是本项目最容易错的 1 件偏差。

---

## GC 汇总表

| 案例 | 文件 | 分布 | 判定方式 | 关键断言 |
|---|---|---|---|---|
| GC-1 M/M/1 | `mm1.ofm` | exponential | 5 复现均值 vs 理论 ±6% | L=4, Lq=3.2, Wq=16, ρ=0.8 |
| GC-2 确定性线 | `det-line.ofm` | constant | 精确 | output=48, util=0.8, Queue 内容恒 0 |
| GC-3 攒批 | `batching.ofm` | constant | 精确 ±0.002 | output=40, Queue 均值 1.8182, 等待均值 20 |
| GC-4 故障 | `breakdown.ofm` | constant | ±1% | output≈5455, processing 占比 0.9091 |
| GC-5 装箱 | `combiner-pack.ofm` | constant | ±1 件 | output≈10 个托盘（不是 40 个箱） |
| GC-6 拆包 | `separator-unpack.ofm` | constant | 精确 | output=40（≠30，后者是 split） |

> 目录中还包含 `diag-empty-transit.ofm`：**它不是黄金案例**，而是缺陷 B1/B2/B3 的最小复现夹具（10 件、零停留、应全输出）。实现修复后用同一文件回归。

运行方式（建议的 CLI，见 [runtime.md](runtime.md) §7）：

```
openflex check  examples/golden/mm1.ofm
openflex run    examples/golden/mm1.ofm --out mm1.results.json
openflex report mm1.results.json
```

自动化断言建议放在 `scripts/golden-check.sh`（不在本次交付范围），逐个模型运行并比对本文表格。

---

## §8 字段说明（已按当前实现 schema 对齐）

任务书给出的约定字段（`format/version/name/time_unit/run{stop_time,warmup,seed,replications}/objects{name,kind,pos,params}/connections{from,to}`）**完全支持**，无需任何扩展。
下表列出本批黄金案例额外用到、且**已被当前实现接受**的键（字段名与 `crates/openflex-model/src/model.rs` 一致）：

| 键 | 位置 | 语义（依据） | 首次使用 |
|---|---|---|---|
| `first_arrival` | source.params | 首件时刻（0.0 = 0 时刻首发；对应 FlexSim `TimeZeroArrival`，E04 高） | 全部案例 |
| `batch_size` | queue.params | 攒够多少件才释放；`null`=不攒批（E06 高） | GC-3 |
| `batch_max_wait` | queue.params | 攒批最长等待，0=无限（E06 高） | GC-3 |
| `failure.mtbf` / `failure.mttr` | processor.params | 故障间隔 / 修复时长（E21 高） | GC-4 |
| `mode` | combiner.params | `pack` / `join`（E09 高；**`batch` 未实现**） | GC-5 |
| `components_per_container` | combiner.params | 每个容器装几个组件（默认 2） | GC-5 |
| `split_count` | separator.params | 拆分数量，0=全拆（语义待明确，见 §9.3） | GC-6 |

**尚未进入实现、但规格需要的键**（建议 v1.1 加，见 [flexsim-to-openflex.md](flexsim-to-openflex.md) §3、§7）：

`combiner.setup_time/process_time`（FlexSim 中 Combiner 是 Processor 子类）、`separator.mode`（unpack/split，E10 高）、`failure.first_failure_time`、`failure.count_time_in_states`（只让特定状态计入 MTBF，E21 高）、`queue.flush_between_batches`、`processor.use_same_operators`。

---

## §8b 实测基线（2026-11-14，`openflex 0.x` 当前构建）

用 `./target/release/openflex run examples/golden/*.ofm --format json` 实测，与本文期望值对比。**期望值不改**，差异登记为待修复项（编号 B1–B5），供实现者定位。

| 案例 | 期望 | 实测（seed=1） | 判定 |
|---|---|---|---|
| GC-1 mm1（5 复现均值） | `Queue.avg_content` 3.2；`Queue.avg_stay_time` 16；`Proc.utilization` 0.8；`Proc.avg_stay_time` 4 | 3.264；14.82；0.7936；3.968 | ✅ content/util/stay(proc) 在容差内；⚠ `Queue` 停留偏低 7.4%（见 **B3**） |
| GC-1 `Proc.avg_content` | 0.80 | **1.000**（恒等于 `max_content`） | ❌ **B1** |
| GC-2 det-line | `Sink1.input` 48；`Proc.output` 48；`Queue.avg_content` 0；`Proc.avg_content` 0.8；`util` 0.8 | 47；47；0.983；1.000；0.7833 | ❌ **B1 + B2 + B3** |
| GC-3 batching | `Sink1.input` 40；`Queue.avg_stay_time` 20；`Queue.avg_content` 1.8182 | 39；40.0；4.7727 | ⚠ 输出少 1 件（**B2**）；停留/含量口径异常（**B3**） |
| GC-4 breakdown | output ≈ 5455；`down` 占比 0.0909 | **5999**；**0.0272** | ❌ **B4**（故障损失仅 ~0.02%，远低于 9.09%） |
| GC-5 combiner-pack | `Sink1.input` ≈ 10（托盘数） | **60**（= 10 托盘 + 50 箱，全部透传） | ❌ **B5**（combiner 无 pack 逻辑） |
| GC-6 separator-unpack | `Sink1.input` 40 | **10**（1 入 1 出，未拆分） | ❌ **B5**（separator 无拆分逻辑） |

### 缺陷登记

| 编号 | 现象 | 最小复现 | 期望依据 | 备注 |
|---|---|---|---|---|
| **B1** | **Processor 的 `avg_content` 恒等于 `max_content`**（GC-1 中 0.8→1.000；GC-2 中 0.8→1.000；极端例：每 100s 来一件、加工 1s，应 0.01，实测 1.000） | `examples/golden/diag-empty-transit.ofm`（到达 100s、加工 1s、停 1000s；实测 Q.avg_content=1.0/P.avg_content=1.0/P.util=0.009 正确但 out=9） | E02 高：content 是"对象内件数"的时间加权平均 | Queue 的 `avg_content` 正常；仅 Processor 有误 → 疑为件离开后未停算积分 |
| **B2** | **最后一件不出结果**：`output_count` 恒比 `input_count` 少 1（GC-2: 48→47；diag: 10→9；stop 改 2000 仍少 1） | `examples/golden/diag-empty-transit.ofm` | E32 高：停止只冻结事件表，已完成件应已移交 | 疑为主循环在 `stop_time` 处未结算"已完成待移交"的件 |
| **B3** | **"零停留"情形下 `Queue.avg_content` 不为 0**：diag 中 10 件瞬时进出，应 0，实测 1.000（=10 件×100s/1000s）；GC-2 应 0，实测 0.983（=47×10/478） | `examples/golden/diag-empty-transit.ofm` | E05 高：Queue 收到即释放 | 与 B1 同源或为惰性结算偏差；mm1 中因停留时间长而被平均掉，只在"停留≈0"时暴露 |
| **B4** | **故障停机比例远低于设定**：MTBF=100/MTTR=10 应 9.09%，实测 2.72%；产出 5999 vs 5455 | `examples/golden/breakdown.ofm` | E21 高：`Up Time` 是"上次恢复→下次停机"，可用率 = MTBF/(MTBF+MTTR) | 疑为 MTBF 计时起算点或"故障未打断加工" |
| **B5** | **Combiner/Separator 无行为逻辑**：combiner 把 60 件全透传；separator 10 进 10 出 | `combiner-pack.ofm`、`separator-unpack.ofm` | E09/E10 高 | `engine.rs` 只对 Source/Processor/Queue 有分支；`docs/COMPATIBILITY.md` 却标 ✅，建议同步更正 |

> **给实现者的提醒**：B1/B3 是"统计口径"错误（最容易让用户不信任报告），B2 是"结果缺一件"（会让吞吐系统性偏低 1 件/对象），B4/B5 是"功能未实现但文档已宣称支持"。建议优先修 B2（影响所有模型）与 B1/B3（影响所有报告的可信度）。

## §9 这些案例**不能**验证什么（诚实边界）

1. **数值级与 FlexSim 对齐**：随机数生成器不同（净室自选），指数/正态抽样序列必然不同；只能比"分布层面的统计量"，不能比"第 37 件的到达时刻"。
2. **同刻事件顺序**：文档未定义（见 GC-5），只能各自定义并公开。
3. **3D 动画/渲染**：无渲染，无法验证 `AnimateItems`、`OnDraw` 等。
4. **Process Flow / Dashboard / Experimenter UI**：只验证数据语义，不验证界面。
5. **输送机与仓储**：v1 不实现，无对应案例。
6. **Separator 的 `split_count` 语义**：FlexSim 明确区分 `unpack`（产出 qty+1）与 `split`（产出 qty）（E10 高），而当前实现只有 `split_count` 一个数字，两种语义无法区分。**必须先定义**（建议：`split_count` = 总产出数，即 split 语义；另加 `mode: unpack` 支持 +1 语义），否则 GC-6 无法判定。
7. **操作员/任务执行器的行程时间**：v1 用"固定速度+直线距离"，与 FlexSim 的路网/加速度模型不同，**不可**当作一致；相关案例待 v1.1 有路网后补。
