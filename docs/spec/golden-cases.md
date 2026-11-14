# openflex 黄金案例（golden cases）

本文档给出**可由公开材料数值验证**的最小模型，配套模型文件在 `examples/golden/*.ofm`。
每个案例都写明：模型参数 → 期望指标 → 推导 → 允许误差 → 依据。

- **判据来源**只用两类：
  1. **FlexSim 公开文档给出的语义/默认行为**（见 [evidence.md](../research/evidence.md)）；
  2. **公开教科书结论**（排队论闭式解、确定性算术）。**不使用**任何 FlexSim 运行结果截图或私有实现。
- 因此这些案例验证的是"**语义一致性 + 数值正确性**"，而不是"与 FlexSim 逐位相同"。前者是净室可验证的目标；后者需要运行 FlexSim，本轮**未做**（也不在允许范围内）。
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
| `Proc1.content_avg` | 0.80 | 服务台"含服务中"的平均件数 = ρ |
| `Queue1.content_avg` | 3.20 | 队列平均等待件数 = Lq |
| `Queue1.staytime_avg` | 16.0 | 件在 Queue 的等待时长 = Wq |
| `Proc1.staytime_avg` | 4.0 | 服务时长均值 = 1/μ |
| `Proc1.utilization_busy` | 0.80 | (settle+process 时间)/统计时长 |
| `Source1.output` ≈ `Sink1.input` | ≈ 0.2 × 47000 = 9400 | 统计区间 = stop − warmup = 47000 |
| `wip_check`（Little） | `content_avg ≈ throughput_rate × staytime_avg` | 自动自检应通过 |

### 允许误差

| 口径 | 容差 | 理由 |
|---|---|---|
| 单次复现 | **±12%** | M/M/1 在 ρ=0.8 时 W 的重尾性使单次抽样波动较大；47000s 内约 9400 个样本，但自相关强 |
| **5 次复现的均值** | **±6%** | 平均值方差降至 1/√5；且 `warmup=1000` 只覆盖 200 个到达，残余瞬态仍存在 |
| 队列/服务台含量之比 `Lq/L = ρ` | ±8% | 结构性关系，比绝对量更稳 |
| 利用率 | ±0.02（绝对值） | 利用率样本量大、收敛快 |

**失败诊断**：若 `Queue1.content_avg` 偏小但 `staytime_avg` 正常 ⇒ 含量统计漏了最后一段（未在停止时结算，见 statistics.md §6.3）。若 `staytime_avg` 恰好等于 `process_time` 均值 ⇒ 停留时间测的是 Processor 而非 Queue 的进出点。

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
4. 全部 48 件在 478 前完成 ⇒ `Sink1.input = 48`。

### 期望指标（精确，容差 0 或浮点 1e-9）

| 指标 | 期望 |
|---|---|
| `Source1.output` | 48 |
| `Sink1.input` | 48 |
| `Proc1.input` / `output` | 48 / 48 |
| `Proc1.content_avg` | 384/480 = **0.8** |
| `Queue1.content_avg` | **0.0** |
| `Queue1.staytime_avg` | `null`（无样本） |
| `Proc1.staytime_avg` | **8.0** |
| `Proc1.utilization_busy` | **0.8** |
| `Proc1.states.idle.share` | **0.2** |
| `Proc1.states.processing.share` | **0.8** |

### 允许误差

**0**（确定性模型；仅允许浮点 1e-9）。若出现 47 或 49，说明到达序列或停止边界处理有 bug。

**失败诊断**：若 `Queue1.content_avg = 0` 但 `Queue1.states.releasing.share > 0` ⇒ Queue 的"收到即释放"语义被实现成了"停留一拍"，需要按 E05/E02 修正（Queue 收到即释放）。

---

## GC-3 `batching.ofm` — 队列攒批（批量语义 + 精确时间加权）

**目的**：验证 Queue 的 `Perform Batching` 语义（E06 高）：攒够 `batch_size` 才**逐件**放行；同时精确校验"时间加权平均含量"与"批次导致的批间空闲"。

### 模型参数

| 项 | 值 |
|---|---|
| `run.stop_time` | 440.0；`warmup` 0 |
| Source1 | `interarrival = constant(10.0)`，首件 t=0 |
| Queue1 | `max_content=1000`，fifo，**`batching = {enabled:true, batch_size:5, max_wait_time:0}`**（扩展键） |
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
| `Source1.output` / `Sink1.input` | **40** / **40** |
| `Queue1.content_avg` | **1.8182**（440 内 800/440） |
| `Queue1.content_max` | 5 |
| `Queue1.staytime_avg` / `min` / `max` | **20.0** / **0.0** / **40.0** |
| `Queue1.states.collecting.share` | 390/440 = **0.8864**（攒批期间；不含 release 瞬时） |
| `Proc1.utilization_busy` | 160/440 = **0.36364** |
| `Proc1.states.idle.share` | **0.63636** |
| `Proc1.content_avg` | **0.36364** |
| `Proc1.staytime_avg` | **4.0** |

### 允许误差

| 项 | 容差 |
|---|---|
| 计数类（input/output/max） | **0** |
| `content_avg` / `utilization` / `share` | **±0.002**（浮点累积） |
| `staytime_avg` | **±0.01** |

**失败诊断**：若 `Sink1.input = 40` 但 `Queue1.staytime_avg = 0` ⇒ 实现了"攒够后清空内容但不记录停留"。若 `Proc1.utilization = 0.909`（=40×10/440）⇒ 忽略了批间空闲，把批释放当成了流水释放。

---

## GC-4 `breakdown.ofm` — 故障维修（MTBF/MTTR 可用率）

**目的**：验证停机模型（E21 高）与"只有特定状态才累计 Up Time"的语义（E21 高：可指定只让 processing 状态计入 Up Time）。

### 模型参数

| 项 | 值 |
|---|---|
| `run.stop_time` | 48000.0；`warmup` 0 |
| Source1 | `interarrival = constant(6.0)` |
| Queue1 | `max_content=100000` |
| Proc1 | `process = constant(8.0)`，`max_content=1`，**`breakdown = {mtbf:constant(100), mttr:constant(10), first_failure_time:constant(100), count_time_in_states:["processing"]}`**（扩展键） |
| Sink1 | — |

### 为何这个案例不自相矛盾

- 到达率 `1/6 ≈ 0.1667/s` > 标称产能 `1/8 = 0.125/s` ⇒ **队列永远有货**，Processor 从不空闲；
- 因此"Up Time 只在 processing 计时"与"按墙钟计时"**给出相同结果**——这消除了两种解释的歧义，使案例可以精确判定。

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
| `Proc1.states.processing.share` | 100/110 = **0.9091** | ±0.002 |
| `Proc1.states.down.share` | 10/110 = **0.0909** | ±0.002 |
| `Proc1.states.idle.share` | ≈ **0** | ≤0.001 |
| 产出率 | `5455/48000 = 0.11365` 件/s | ±1% |

> **注意**：`processing.share` 与 `down.share` 之和 ≈ 1（无 idle），这是本案例的自检式断言。若实现把"故障计时"错误地挂在墙钟上且在 idle 时也累积，虽然总数不变，但一旦把 `Queue1` 改小（使 Proc 出现 idle）就会失配——因此 §8 建议后续加一个变体案例。

### 允许误差的说明

±1% 而非 0 的原因：**停机打断在制品**的处理方式在文档中未明确（是"完成当前件后停机"还是"立即打断、恢复后补足剩余加工时间"）。两种实现对 48000s 的总产出影响 < 1 件，但会影响 `processing.share` 的第 4 位小数。openflex 采用**立即打断 + 恢复后续加工剩余时间**（与"processing 状态才累计 up time"的语义自洽），并在 `report` 中输出 `preemption_events` 计数以便诊断。

---

## GC-5 `combiner-pack.ofm` — 装箱（pack 语义）

**目的**：验证 Combiner 的三条硬语义（E09 高）：①**必须先收到端口 1 的容器件**；②`components` 第 1 行对应**端口 2**；③`pack` 模式把零件装入容器件后只把**容器件**释放下游。

### 模型参数

| 项 | 值 |
|---|---|
| `run.stop_time` | 500.0 |
| TraySource | `interarrival = constant(50.0)` ⇒ 托盘 `0,50,…,450`（10 个） |
| BoxSource | `interarrival = constant(10.0)` ⇒ 箱 `0,10,…,490`（50 个） |
| Combiner1 | `combine_mode="pack"`，**`components=[4]`**（端口 2 每批收 4 个箱），`setup=process=0`，`max_content=1` |
| Sink1 | — |
| 连接顺序 | `TraySource→Combiner1` 为**第一条**连接（= 输入端口 1），`BoxSource→Combiner1` 为第二条（= 输入端口 2） |

### 推导

- 每 50s 供 1 个托盘、每 10s 供 1 个箱 ⇒ 每托盘需求 4 箱 = 40s < 50s，**箱供给充足**；
- 500s 内托盘供给 10 个 ⇒ 产出 **10 个托盘件**；
- 每个托盘消耗 4 箱 ⇒ 消耗 40 箱（50 个箱中 10 个剩余，停在 Queue/Source 侧）。

### 期望指标

| 指标 | 期望 | 容差 |
|---|---|---|
| `Sink1.input`（= 托盘数） | **10** | **±1** |
| `Combiner1.output` | 10 | ±1 |
| `Combiner1.content` 峰值 | 1 个托盘容器（+ 装入其中的箱不单独计内容） | 见 §8 |
| `BoxSource.output` | 40~50（受端口 2 阻塞影响） | 不判定 |

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
| Sep1 | `separate_mode="unpack"`，**`separate_qty=3`**，`setup=process=0`，`max_content=1` |
| Sink1 | — |

### 推导

- **unpack 语义**：解包 `qty=3` 件，**容器件本身仍会被释放** ⇒ 每 1 个容器件产出 `3 + 1 = 4` 件（E10 高）；
- 10 个容器件 ⇒ `10 × 4 = 40` 件进 Sink；
- 每 50s 一个容器件，解包顺序为"从后往前"（E10 高），但总量与顺序无关。

### 期望指标

| 指标 | 期望 | 容差 |
|---|---|---|
| `Sink1.input` | **40** | **0** |
| `Sep1.input` | 10 | 0 |
| `Sep1.output` | 40 | 0 |

### 对照检查（split 模式应有的结果）

若把 `separate_mode` 改成 `"split"`，则 `separate_qty=3` 表示"总产出 3 件"（复制 qty−1 次），期望 `Sink1.input = 30`。
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

运行方式（建议的 CLI，见 [runtime.md](runtime.md) §7）：

```
openflex check  examples/golden/mm1.ofm
openflex run    examples/golden/mm1.ofm --out mm1.results.json
openflex report mm1.results.json
```

自动化断言建议放在 `scripts/golden-check.sh`（不在本次交付范围），逐个模型运行并比对本文表格。

---

## §8 提案扩展字段（需与实现者确认后固化为 spec v1.1+）

本批 `.ofm` 中出现了**约定之外**的参数键，全部属于"FlexSim 有明确语义、但任务书的 ofm 约定尚未定义"的必要字段。**这些键不影响顶层结构**（`params` 是自由字典），但会决定实现者的解析器是否接受这些模型。

| 扩展键 | 位置 | 语义（依据） | 建议正式形态 |
|---|---|---|---|
| `batching.enabled` | queue.params | 是否攒批（E06 高） | 等同 FlexSim `DoBatching` |
| `batching.batch_size` | queue.params | 目标批大小，批内**逐件**放行（E06 高） | 等同 `BatchSize` |
| `batching.max_wait_time` | queue.params | 0 = 无限等待（E06 高） | 等同 `MaxBatchTime` |
| `breakdown.mtbf` | 任意对象 params | Up Time（上次恢复→下次停机），可为分布（E21 高） | 建议提升为顶层 `breakdowns[]` + 对象 `members[]`，与 FlexSim 工具对象同构 |
| `breakdown.mttr` | 同上 | Down Time，同组成员**同时恢复**（E21 高） | 同上 |
| `breakdown.first_failure_time` | 同上 | 首次故障时刻；负值=忽略首次（E21 高） | 同上 |
| `breakdown.count_time_in_states` | 同上 | 只有这些状态的时间计入 Up Time（E21 高） | 建议用状态名数组 |
| `combine_mode` | combiner.params | `pack`/`join`/`batch`（E09 高） | 等同 `CombineMode` |
| `components` | combiner.params | 第 i 个元素对应**端口 i+2** 的件数（E09 高） | 等同 `ComponentsList` |
| `separate_mode` | separator.params | `unpack`/`split`（E10 高，产出差 1） | 等同 `SeparateMode` |
| `separate_qty` | separator.params | unpack=解包数（产出 qty+1）；split=总产出数（E10 高） | 等同 `SeparateQty` |
| `time_zero_arrival` | source.params | 是否在 t=0 创建首件；**默认 true**（E04 高：FlexSim 的 `TimeZeroArrival`，文档称"勾选则 0 时刻创建 1 件"） | 建议默认 true 并在 check 时打印 |
| `items_per_arrival` | source.params | 每次到达创建的件数（约定内已有） | — |

**默认值建议**（openflex 决策，非 FlexSim 原文）：`batching.enabled=false`、`batch_size=1`、`max_wait_time=0`、`combine_mode="pack"`、`separate_mode="unpack"`、`separate_qty=1`、`time_zero_arrival=true`、`mtbf/mttr` 缺省即无故障。

---

## §9 这些案例**不能**验证什么（诚实边界）

1. **数值级与 FlexSim 对齐**：随机数生成器不同（净室自选），指数/正态抽样序列必然不同；只能比"分布层面的统计量"，不能比"第 37 件的到达时刻"。
2. **同刻事件顺序**：文档未定义（见 GC-5），只能各自定义并公开。
3. **3D 动画/渲染**：无渲染，无法验证 `AnimateItems`、`OnDraw` 等。
4. **Process Flow / Dashboard / Experimenter UI**：只验证数据语义，不验证界面。
5. **输送机与仓储**：v1 不实现，无对应案例。
6. **操作员/任务执行器的行程时间**：v1 用"固定速度+直线距离"，与 FlexSim 的路网/加速度模型不同，**不可**当作一致；相关案例待 v1.1 有路网后补。
