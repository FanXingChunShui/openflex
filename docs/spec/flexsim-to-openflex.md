# FlexSim → openflex 映射规格（供实现者使用）

> 目标：把 FlexSim 的"对象 + 属性面板 + 触发器 + 工具"映射到 openflex 的模型 JSON 字段与 CLI 命令。
> 每条映射给出：FlexSim 概念（证据）→ openflex 形态 → 实现注意。
> 证据编号见 [evidence.md](../research/evidence.md)。凡标 **【约定】** 的字段与任务书给出的 ofm 约定完全一致；标 **【提案】** 的是本规格建议新增的字段（需实现者确认）。

---

## 1. 总原则

1. **语义对齐，数值不对齐**：行为语义（何时收件、何时释放、如何计数）必须与公开文档一致；随机数、渲染、文件格式**故意不同**（净室纪律 + 跨平台目标）。
2. **保序**：`connections[]` 的顺序**就是端口号**（端口 1 起）。任何去重/排序都会改变 `SendToPort` 的语义（E02 高、E20 高）。
3. **显式优于隐式**：FlexSim 靠 UI 面板隐含的默认值，openflex 要写进 JSON 或用文档化的默认值表。
4. **不实现即声明**：不做的对象/特性必须在 `check` 时明确报错或警告，不做"静默忽略"。

---

## 2. 对象映射

| FlexSim 对象 | openflex `kind` | v1 状态 | 证据 |
|---|---|---|---|
| Source | `source` | 实现 | E03 高 |
| Queue | `queue` | 实现 | E05 高 |
| Processor | `processor` | 实现 | E07 高 |
| Sink | `sink` | 实现 | E08 高 |
| Combiner | `combiner` | 实现 | E09 高 |
| Separator | `separator` | 实现 | E10 高 |
| MultiProcessor | `multiprocessor` | v1.1 | E11 高 |
| BasicFR | `basic_fr` | v1.1 | E11 高 |
| Storage Object / Rack / Floor Storage / Gravity Flow / Push Back / Drive In | `rack`（+`rack_type`） | 不实现（报错） | E15 高 |
| Dispatcher | `dispatcher` | 简化实现 | E14 高 |
| Operator | `operator` | 简化实现 | E14 高 |
| Transporter | `transporter` | 简化实现 | E14 高 |
| BasicTE | `basic_te` | 简化实现 | E13 高 |
| Elevator / Robot / Crane / ASRSvehicle | — | 不实现（报错） | E13 高 |
| Straight/Curved/Join Conveyor, Merge Controller, Station, Photo Eye, Motor, Transfers | — | 不实现（报错） | E16 高 |
| Fluid 系列（12 个） | — | 不实现（报错） | — |

对象公共字段【约定】：`name`（唯一，模型内引用键）、`kind`、`pos: [x, y]`（FlexSim 为 3D 坐标，v1 用 2D）、`params`（自由字典）。

---

## 3. 参数映射（逐对象）

> ⚠ **字段名以 `crates/openflex-model/src/model.rs` 的实现为准**（2026-11-14 核对）。
> 本节给出的键名均可被当前 `openflex validate` 接受；任务书约定的核心键（`interarrival`/`items_per_arrival`/`max_content`/`order`/`setup_time`/`process_time`/`pos`/`connections`）与示例完全一致。
> 标 **【约定】** = 与任务书 ofm 约定一致；**【已实现】** = 实现已存在但约定文档中未列出。

### 3.1 Source（`SourceParams`）

| FlexSim 属性 | openflex 键 | 类型/默认 | 说明 |
|---|---|---|---|
| `InterArrivalTime` | `interarrival`【约定】 | Distribution | 间隔定义是"**上一件离开 Source → 下一件到达**"（E03 高） |
| Arrivals 表 Quantity | `items_per_arrival`【约定】 | u32 / 1 | 每次到达创建件数 |
| `TimeZeroArrival` | `first_arrival`【约定】 | f64 / **0.0** | 首件时刻；0.0 即"0 时刻首发"（等价勾选 TimeZeroArrival，E04 高） |
| — | `max_arrivals`【已实现】 | Option<u32> | 到达批数上限 |
| — | `stop_time`【已实现】 | Option<f64> | Source 自身停止创建的时刻 |
| FlowItemClass / itemtype | `item_type`【已实现】 | u32 / 0 | 创建件的 itemtype |
| 件初始标签 | `labels`【已实现】 | map<string,f64> | 创建时附加的标签 |
| `ArrivalStyle`=Schedule/Sequence | *不支持* | — | 应报错而非静默忽略 |
| Arrival at time 0 | 由 `first_arrival=0` 表达 | — | 注意实现注释与实际默认值不一致（见 §13.4） |

### 3.2 Queue（`QueueParams`）

| FlexSim 属性 | openflex 键 | 类型/默认 | 说明 |
|---|---|---|---|
| `MaxContent` | `max_content`【约定】 | u32 | — |
| `LIFO`（含排序变体） | `order`【约定】 | `fifo`(默认) / `lifo` / `priority` / `by_label` | 见 `QueueOrder` |
| `DoBatching` | `batch_size`【已实现】 | `Option<u32>`（None=不攒批） | 攒够才释放（E06 高） |
| `MaxBatchTime` | `batch_max_wait`【已实现】 | f64 / 0.0（0=无限等） | E06 高 |
| `FlushContent` | *未实现* | — | 建议 v1.1 加 `flush_between_batches` |
| `ItemPlacement`/`StackBaseZ` | 忽略（纯视觉） | — | E06 高 |

### 3.3 Processor（`ProcessorParams`）

| FlexSim 属性 | openflex 键 | 类型/默认 | 说明 |
|---|---|---|---|
| `SetupTime` | `setup_time`【约定】 | Distribution / 0 | — |
| `ProcessTime` | `process_time`【约定】 | Distribution / 1 | — |
| `MaxContent` | `max_content`【约定】 | u32 / 1 | >1 = 并行处理 |
| `UseSetupOperators`+`OperatorRef` | `setup_operator`【已实现】 | Option<string> | 指向 dispatcher 名（简化：按数量而非优先级） |
| `UseProcessOperators`+`OperatorRef` | `process_operator`【已实现】 | Option<string> | 同上 |
| `UseSameNumOperators` | *未实现* | — | 需实现"process 复用 setup 的操作员"（E07 高） |
| `OperatorPriority`/`OperatorPreempt` | *未实现* | — | E07 高 |
| MTBF/MTTR | `failure: {mtbf, mttr, repair_operator?}`【已实现】 | 见 §7 | — |
| — | `changeover_time`【已实现】 | Distribution / 0 | 换型时间（FlexSim 无独立字段，用 setup 表达） |
| `AnimateItems` | 忽略（无渲染） | — | — |

### 3.4 Sink（`SinkParams`）

无参数；只有 `triggers` 与 `shifts`。`params: {}` 合法（E08 高）。

### 3.5 Combiner（`CombinerParams`）

| FlexSim 属性 | openflex 键 | 说明 |
|---|---|---|
| `CombineMode` | `mode`【已实现】= `pack`(默认) / `join` | `batch` 模式**未实现**（E09 高） |
| `ComponentsList`（逐端口一张表） | `components_per_container`【已实现】= u32 / 2 | **简化**：单一数字，非逐端口数组（E09 高 的"第 i 行↔端口 i+2"未保留） |
| setup/process/操作员 | *不支持* | FlexSim 中 Combiner 是 Processor 子类，有 setup/process（E09 高）；openflex 未实现 |
| `PullStrategy` | 不适用 | E09 高 |

### 3.6 Separator（`SeparatorParams`）

| FlexSim 属性 | openflex 键 | 说明 |
|---|---|---|
| `SeparateMode` | *未实现* | FlexSim 的 `unpack`/`split` 二元语义丢失（E10 高） |
| `SeparateQty` | `split_count`【已实现】= u32 / 0 | 0 = 全拆；**语义需明确**：是"分出 N 个"还是"总共 N 个"？两者差 1 件（E10 高） |

### 3.7 固定资源公共参数

| FlexSim 属性 | openflex 键 | 状态 |
|---|---|---|
| `Pull` / `PullStrategy` / `PullRequirement` | — | **未实现**（当前为纯 push + 背压模型，见 ARCHITECTURE "路由与背压"） |
| `SendToPort` | `connections[].strategy`【已实现】 | 见 §4.2 |
| `UseTransport` / `TransportRef` / `TransportPriority` / `TransportPreempt` | *未实现* | 运输由 transporter 对象 + 任务序列显式建模 |

### 3.8 任务执行器族

| FlexSim 属性 | openflex 键 | 说明 |
|---|---|---|
| `MaxContent`（Capacity） | `capacity`【已实现】 | — |
| `MaxSpeed` | `speed` | 见 `TransporterParams` |
| `LoadTime`/`UnloadTime` | `load_time`/`unload_time` | — |
| `Acceleration`/`Deceleration`/`OffsetTravel` | *未实现* | 匀速直线行驶（差异需声明） |
| `QueueStrategy`/`PassTo`/`BreakTo` | *简化* | 见 COMPATIBILITY 的 dispatcher 说明 |

## 4. 连接与端口

### 4.1 A 连接（输入/输出端口）→ `connections[]`【约定】

```json
"connections": [{"from": "Source1", "to": "Queue1"}, {"from": "Queue1", "to": "Proc1"}]
```

- 数组下标 i（从 0 起）对应**输入端口 i+1** 与上游的**输出端口 i+1**。
- 同一 `from` 出现多次 ⇒ 该对象有多个输出端口（按出现顺序编号）。
- `check` 必须验证：`from`/`to` 都存在；目标对象存在对应输入端口；不存在自环；不存在重复边（完全相同的 from/to 对，且非有意多端口时应提示）。

### 4.2 `SendToPort` → `connections[].strategy`【已实现】

FlexSim 的 `SendToPort` 是**对象级**表达式（0=全开、>0=指定端口、−1=扣住不发，E02 高）。当前实现把它放在**连接级**（每条连接可带 `strategy`，`type` 标签区分），语义上等价于"按策略挑一个出口"：

```json
"connections": [
  {"from": "Inbound", "to": "Main",
   "strategy": {"type": "by_percentage", "percentages": [70.0, 30.0]}},
  {"from": "Inbound", "to": "Express"}
]
```

| FlexSim `SendToPort` 写法 | openflex `strategy.type` | 语义差异 |
|---|---|---|
| 返回 0（发给第一个可接收） | `first_available`（默认） | ✅ 等价 |
| round-robin | `round_robin` | ✅ |
| random | `random` | ✅（在**可用**输出中随机） |
| by itemtype | `by_item_type` | ✅ itemtype n → 第 n 个可用输出 |
| by label 值 | `by_label` | ✅ 数值标签作端口索引 |
| by percentage（FlexSim 用 `bernoulli`/概率表达式） | `by_percentage` | 🟡 FlexSim 的百分比路由**不检查可用性**，会阻塞；实现与此一致 |
| 条件表达式（`if(itemtype==1,1,2)`） | `by_condition` | 🟡 依序求值，命中即选；无"全部表达式"能力 |
| **返回 −1（扣住不发）** | **未实现** | ⚠ FlexSim 可用 −1 + `releaseitem()` 实现"暂不释放"（E02 高）；openflex 用背压（`Blocked`）表达，语义不同 |
| **端口级开口/闭口（open/closed）** | 未建模 | ⚠ FlexSim 端口有开闭状态（E20 高），openflex 用 `can_accept` 动态判定 |

> **实现建议**：`by_percentage`/`by_condition` 的权重与条件都定义在**单条连接**上，而 FlexSim 的 `SendToPort` 一次求值决定整个端口集合。当一条 from 有 3 条出边且只有第 1 条带 `by_percentage` 时，语义会不明确 —— 建议 `validate` 强制"同一 from 的所有出边要么都无 strategy，要么只在第一条给 strategy 并覆盖全部出边"，或报 `ambiguous_strategy`。

### 4.3 S 连接（中心端口）→ 显式引用字段

FlexSim 的 S 连接表示"引用关系"（任务执行器↔固定资源、操作员↔处理器）（E20 高）。当前实现用**具名字段**表达，避免"连一根线就有语义"：

| S 连接用途 | openflex 字段 |
|---|---|
| 处理器 setup 时请求资源 | `processor.setup_operator`（dispatcher 名） |
| 处理器 process 时请求资源 | `processor.process_operator` |
| 故障时请求维修资源 | `failure.repair_operator` |
| 运输（固定资源要求运输器来取件） | 由 `transporter`/`dispatcher` 对象 + 任务序列建模（v1 简化） |

对应关系：FlexSim 的 `current.centerObjects[1]`（默认运输对象，E12 高）在 openflex 中**没有隐式默认**——必须显式写出引用名。这是有意的规范化（避免"删掉一根线就静默换承运方"）。

---

## 5. 触发器映射

FlexSim 触发器 = 对象上的命名事件钩子，可挂脚本（E17 高）。openflex v1 用**表达式 DSL**（不引入通用脚本引擎，避免沙箱与跨平台风险）：

```json
"triggers": {
  "on_entry":     {"expr": "item.label('size') = 2"},
  "on_exit":      {"expr": "stat('custom_exit') += 1"},
  "send_to_port": {"expr": "item.label('type')"},
  "on_breakdown": {"expr": "log('down at ' + time())"}
}
```

| FlexSim 触发器 | openflex 键 | 可返回值 | 证据 |
|---|---|---|---|
| OnEntry | `on_entry` | 否 | E02 高 |
| OnExit | `on_exit` | 否 | E02 高 |
| OnCreation（Source 专属） | `on_creation` | 否 | E03 高 |
| InterArrivalTime | `inter_arrival_time` | **是**（时长） | E03 高 |
| OnMessage | `on_message` | 否 | E02 高 |
| SendToPort | `send_to_port` | **是**（端口号） | E02 高 |
| PullStrategy | `pull_strategy` | **是**（端口号） | E02 高 |
| PullRequirement | `pull_requirement` | **是**（bool） | E02 高 |
| TransportReference | `transport_reference` | **是**（对象引用或 0） | E02 高 |
| OnStop / OnResume | `on_stop` / `on_resume` | 否 | E02 高 |
| OnContentChange / OnInputChange / OnOutputChange / OnStaytimeChange | `on_content_change` 等 | 否 | E02 高 |
| OnStateChange / OnStateValueChange | `on_state_change` / `on_state_value_change` | 否 | E02 高 |
| OnReset | `on_reset` | 否 | E02 高 |
| OnDraw | **不提供**（无渲染） | — | 差异声明 |
| OnReceiveTaskSequence / OnStartTask / OnFinishTask / OnPreempt / BreakTo / PassTo / QueueStrategy | `on_receive_task_sequence` 等 | 部分有返回值 | E13/E14 高 |
| SplitUnpackQuantity（Separator） | `split_unpack_quantity` | **是**（数量） | E10 高 |

**DSL 最小集合建议**（够覆盖黄金案例与常见场景）：

- 变量：`item`（当前件）、`time()`、`object`（当前对象）、`content()`
- 件的属性：`item.item_type`、`item.label('k')`（读）、`item.set_label('k', v)`
- 统计：`stat('name')`、`stat('name') += x`
- 消息：`send_message(target, delay, p1, p2, p3)`
- 随机：`random(stream)`、`sample('dist')`（**只能在允许的事件中调用**，见 logic.md §11）
- 控制：三元、比较、`&&`/`||`、算术

**时序禁令**：`on_entry`/`on_exit` 内不得调用 `stop_object`/`open_ports`/`create_task_sequence`（E02 高）。实现方式：这些 API 在 entry/exit 阶段**自动转为 0 延迟消息**并打 warning（与文档建议的 `senddelayedmessage(…,0)` 等价）。

---

## 6. 分布映射

### 6.1 覆盖矩阵

| FlexSim 分布 | 参数（E30 高） | ofm `distribution`【约定】 | 状态 |
|---|---|---|---|
| Exponential | Location, **Scale(=均值)**, Stream | `exponential{mean}` | 支持（见 §6.2 警示） |
| Constant（常量表达式） | 值 | `constant{value}` | 支持 |
| Uniform | Minimum, Maximum, Stream | `uniform{min,max}` | 支持 |
| Normal | Mean, Std Dev, Stream | `normal{mean,stddev}` | 支持 |
| Triangular | Minimum, Mode, Maximum, Stream | `triangular{min,mode,max}` | 支持 |
| **Duniform**（整数均匀） | Minimum, Maximum, Stream | **无** | **缺**：建议加 `duniform{min,max}`（E30 高） |
| Gamma | 帮助未详述 | `gamma{shape,scale}` | 支持（参数口径按标准定义） |
| Weibull | 未详述 | `weibull{shape,scale}` | 支持 |
| Lognormal | 未详述 | `lognormal{meanlog,sdlog}` | 支持 |
| Empirical（经验分布工具） | 表 + Weighted（E31 高） | `empirical{values,weights?}` | 支持 |
| 其余（文档称共 **30 种**） | 未逐一定义 | — | **缺**：Beta/Erlang/Poisson/Binomial/… 建议按 v1.2 逐个加，先记录在案 |

### 6.2 Exponential 的参数口径差异（重要）

- FlexSim：`Exponential(Location, Scale, Stream)`，**Scale 是均值**，Location 是下界偏移（E30 高）。
- openflex：`exponential{mean}`，**无 location**。
- 映射：`mean = Scale`，`Location` 非 0 时 `check` 报 `unsupported: exponential.location != 0`（v1），或实现为 `Location + Exponential(Scale)` 并写入 spec v1.1。

### 6.3 Stream（随机数流）

FlexSim 每个分布都能指定 `Stream`（E30 高）。openflex 建议加可选字段：

```json
{"distribution": "exponential", "mean": 5.0, "stream": 1}
```

`stream` 缺省 0。派生规则见 [runtime.md](runtime.md) §5（`stream_rng = derive(seed, replication, stream_index)`）。

---

## 7. 故障与班次映射

### 7.1 故障（MTBF/MTTR）

FlexSim 用**独立工具对象 + 成员列表**，同一对象可被多个 MTBF/MTTR 控制（E21 高）。当前实现把它放在对象级（`ProcessorParams.failure`）：

```json
"failure": {"mtbf": {"distribution": "constant", "value": 100.0},
            "mttr": {"distribution": "constant", "value": 10.0}}
```

| FlexSim | openflex | 差距 |
|---|---|---|
| Members 列表（多对象共享） | 每对象各自配置 | 🟡 无法表达"一组对象共用一个故障时钟" |
| First Failure Time（负值=忽略） | *未实现* | 首故障时刻不可控 |
| Up Time（MTBF）只在指定状态计时 | *未实现* | **重要**：FlexSim 可只在 `processing` 计时（E21 高），当前实现只能按墙钟 |
| Down Time（MTTR），组内同时恢复 | `mttr` | 🟡 单对象语义 |
| Break down members individually | *不适用* | 单对象配置天然独立 |
| Down/Resume Function | `triggers` 里的故障/修复动作 | 🟡 |
| Down Behavior（可复用） | `shifts` + 触发器近似 | 🟡 |
| 停机请求栈（多次停止叠加） | *未实现* | 见 logic.md §8.3 |

### 7.2 班次（Time Table）

实现用"窗口"而非"停机时段表"表达（`Shift { start, duration, repeat_every }`）：

```json
"shifts": [{"start": 28800.0, "duration": 28800.0, "repeat_every": 86400.0}]
```

| FlexSim（Time Table） | openflex | 差距 |
|---|---|---|
| 每行 `downTime + duration` | `start + duration` | ✅ 语义等价（正向表达"上班窗口"） |
| 重复模式 | `repeat_every`（0=只一次） | ✅ |
| **相接时段自动合并** | 不适用（无相接概念） | ⚠ 影响"停机次数"统计（E22 高） |
| Down Function / Resume Function（逐成员）+ On Down/On Resume（一次） | 无钩子，仅置 `OffShift` | 🟡 需 v1.1 补 |
| down state + state profile | 固定 `off_shift` | 🟡 |
| Down Behavior（复用动作） | *未实现* | 🟡 |

## 8. 统计与实验映射

| FlexSim | openflex | 说明 |
|---|---|---|
| 标准统计（Content/State/Staytime/Throughput/Travel） | `results.json` 的 `objects[]`（见 statistics.md §3） | 自动收集，无需配置 |
| Performance Measure（表 + Value 表达式）（E34 高） | `metrics[]`【提案】：`{"name":"Throughput1","expr":"sum(objects.output)","unit":"items"}` | 在**每个复现结束时**求值；不得引用自身 |
| Model Parameter（可被实验器扫参数） | `parameters{}` + `openflex param k=v`【提案】 | 对应 Scenario 的输入维度 |
| Experimenter Job/Scenario/Replication/Task（E35 高） | CLI：`openflex run --replications N --param k=v1,v2 …` | Task = (场景, 复现) |
| Scenario 含 Stop Time + Warmup | `--until`、`--warmup` 属于场景维度 | **必须**支持按场景覆盖（E35 高） |
| Results Database（独立文件、可查重） | `<model>.results.json`（每复现一条） | v1 不做增量查重（可作为 v1.1） |
| 置信区间/场景比较（E36 中） | `openflex report` 输出 mean/stddev/min/max + CI | 公式见 statistics.md §4.3 |
| Dashboard / Chart Templates | 不实现 UI；提供 CSV/JSON 数据出口 | 差异声明 |

---

## 9. 脚本与用户命令映射

| FlexSim | openflex | 说明 |
|---|---|---|
| User Commands（自定义命令，参数类型标注）（E38 高） | `user_commands[]`【提案】：`{"name":"myCmd","params":["num"],"return":"num","body":"expr"}` | 表达式复用 §5 DSL；参数类型保留 `num/str/node/var` |
| 内置命令（`go/stop/step/skip/runspeed/stoptime/getrunstate/updatestates`）（E32 高） | CLI 子命令/开关（见 §10） | 语义逐条对齐 |
| `commandlineparam("x")` + `-x v`（E32 高） | `openflex run model.ofm --param x=v` 或 `-Dx=v` | 供批处理脚本注入 |
| `senddelayedmessage()`（E02 高） | DSL `send_message(target, delay, p1,p2,p3)` | `delay=0` 也走事件表（不同步调用） |
| `releaseitem()` / `moveobject()` | DSL `release_item(item)` / `move_item(item, target)` | 配合 `send_to_port = -1`（E02 高） |
| `updatelocations()` | 无需（无渲染位置） | 差异声明 |

**不复制**：FlexSim 的 1000+ 命令实现细节。openflex 只实现"规格需要"的最小子集，并在 `explain` 输出中列出已实现命令。

---

## 10. CLI 命令映射（完整）

| FlexSim 操作 | openflex CLI |
|---|---|
| 打开模型并运行 | `openflex run model.ofm` |
| Reset + Run | `openflex run`（内部先 reset） |
| 设 stop time | `--until T`（覆盖 `run.stop_time`） |
| 设 warmup | `--warmup W` |
| Stop（暂停） | 交互模式 `s`；批处理下无意义 |
| Step 一个事件 | `--steps 1` |
| Skip 同时刻其余事件 | `--skip-to-next-time` |
| Fast Forward | `--fast-forward`（批处理默认即最大速度） |
| 设速度 | `--speed N`（仅交互/实时；批处理忽略并 warning） |
| 设随机种子 | `--seed S` |
| 设复现次数 | `--replications N` |
| 参数化（Scenario） | `--param k=v1,v2,...`（笛卡尔积生成场景） |
| 导出指标 | `--out r.json --format json\|table\|csv` |
| 校验模型 | `openflex check model.ofm`（**必做**） |
| 打印结构摘要 | `openflex explain model.ofm` |
| 渲染报告 | `openflex report r.json` |
| `flexsim.exe m.fsm -nrofops 8`（命令行传参） | `openflex run m.ofm -D nrofops=8` |

---

## 11. 术语表（FlexSim 官方术语 → 中文 → openflex 字段）

| English | 中文 | openflex 对应 | 证据 |
|---|---|---|---|
| Flow Item | 流项目 / 流动实体 | `item`（隐式，由 source 创建） | E43 高 |
| Fixed Resource | 固定资源 | `kind ∈ {source,queue,processor,sink,combiner,separator,...}` | E02 高 |
| Task Executer | 任务执行器 | `kind ∈ {dispatcher,operator,transporter,basic_te}` | E13 高 |
| Source | 发生器 | `source` | E03 高 |
| Queue | 队列 | `queue` | E05 高 |
| Processor | 处理器 | `processor` | E07 高 |
| Sink | 吸收器 | `sink` | E08 高 |
| Combiner | 合流器 / 装箱器 | `combiner` | E09 高 |
| Separator | 拆包器 / 分离器 | `separator` | E10 高 |
| MultiProcessor | 多工序处理器 | `multiprocessor` | E11 高 |
| BasicFR | 基础固定资源 | `basic_fr` | E11 高 |
| BasicTE | 基础任务执行器 | `basic_te` | E13 高 |
| Dispatcher | 调度器 | `dispatcher` | E14 高 |
| Operator | 操作员 | `operator` | E14 高 |
| Transporter | 搬运器 | `transporter` | E14 高 |
| Rack / Storage Object | 货架 / 存储对象 | `rack`（v1 不实现） | E15 高 |
| Port (input/output) | 端口（输入/输出） | `connections[]` | E20 高 |
| A-connect | A 连接（输入/输出端口连接） | `connections[]` | E20 高 |
| S-connect / center port | S 连接 / 中心端口 | `resource_refs[]` | E20 高 |
| Send To Port | 发送到端口（路由规则） | `send_to_port` | E02 高 |
| Pull / Pull Strategy / Pull Requirement | 拉取 / 拉取策略 / 拉取条件 | `pull` / `pull_strategy` / `pull_requirement` | E12 高 |
| Use Transport | 使用运输 | `use_transport` | E12 高 |
| Trigger | 触发器 | `triggers{}` | E17 高 |
| Event | 事件 | 事件表条目 | E18 高 |
| State | 状态 | `states{}`（统计输出） | E02 高 |
| Label | 标签 | `item.label(k)` | E19 高 |
| Item Type | 件类型 | `item.item_type` | E02 高 |
| Message | 消息 | `on_message` / `send_message()` | E02 高 |
| Task Sequence | 任务序列 | 内部结构（v1 简化） | E13 高 |
| Offset Travel | 偏移行程 | v1 不做 | E13 高 |
| MTBF / MTTR | 平均故障间隔 / 平均修复时间 | `breakdowns[].up_time` / `.down_time` | E21 高 |
| Down Behavior | 停机行为 | `down_behaviors` | E23 高 |
| Time Table | 时间表（班次/计划停机） | `shifts[]` | E22 高 |
| Warmup Time | 预热时间 | `run.warmup` | E27 高 |
| Stop Time | 停止时间 | `run.stop_time` | E28 高 |
| Run Speed | 运行速度 | `--speed` | E27 高 |
| Replication | 复现 | `run.replications` / `--replications` | E35 高 |
| Scenario | 场景 | (参数向量 + stop + warmup) | E35 高 |
| Task | 任务（场景×复现） | 一次模型运行 | E35 高 |
| Job | 作业 | 一次 CLI 批量运行 | E35 高 |
| Performance Measure | 性能指标 | `metrics[]` | E34 高 |
| Model Parameter | 模型参数 | `parameters{}` / `--param` | E34 高 |
| Statistics Collector | 统计采集器 | v1 不做（用标准统计） | E33 高 |
| Dashboard / Chart | 仪表板 / 图表 | v1 不做（CSV/JSON 出口） | E33 高 |
| Experimenter | 实验器 | `openflex run --param` 批量 | E35 高 |
| Model Triggers | 模型触发器 | `run.on_reset` / `run.on_start`【提案】 | E25 中 |
| User Command | 用户命令 | `user_commands[]` | E38 高 |
| Empirical Distribution | 经验分布 | `empirical{values,weights}` | E31 高 |
| Distribution Chooser | 分布选择器 | `distribution{}` | E30 高 |
| Random Number Stream | 随机数流 | `distribution.stream`【提案】 | E26/E30 高 |
| Process Flow | 流程流（可视化逻辑） | v1 不做 | — |

---

## 12. 实现优先级建议（按"能跑通黄金案例"排序）

1. **事件内核**：事件表（稳定全序）、`reset/run/stop`、模型时间、`stop_time` 边界、replications。
2. **固定资源四步流水线**：Source/Queue/Processor/Sink（含 `send_to_port=0/p/-1`、pull、Queue 收到即释放）。
3. **统计内核**：content 时间加权、throughput、staytime 样本、state 累计、warmup 清零、停止结算。
4. **分布**：constant / exponential / uniform / normal / triangular（够 GC-1~GC-4）。
5. **批量语义**：Queue batching（GC-3）、Combiner pack（GC-5）、Separator unpack/split（GC-6）。
6. **停机**：MTBF/MTTR + 状态栈（GC-4）；Time Table 班次（v1.1）。
7. **任务执行器**：Dispatcher/Operator/Transporter 的最简任务序列（无路网）。
8. **CLI**：`run/check/report/explain/param` + 结果 JSON。
9. **表达式 DSL**（触发器/策略/metrics 用）。
10. v1.1+：MultiProcessor、BasicFR/TE、Conveyor、Rack、Process Flow、Dashboard。

---

## 13. 必须由 openflex 自行定义并公开的 3 个"文档缺口"

这三点 FlexSim 文档**没有定义**，实现者不能"猜"，必须自行规定并写进用户文档（否则不可复现）：

1. **同刻事件排序**：建议 `(time, priority, 创建序号)` 全序，并在 `check` 中对同刻敏感模型提示。
2. **停机打断在制品的处理**：建议"立即打断 + 恢复后继续剩余加工时间"，并统计 `preemption_events`。
3. **随机数生成器与流派生**：建议 PCG64 + `derive(seed, replication, stream_index)`，并声明**不与 FlexSim 数值对齐**。
