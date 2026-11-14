# openflex 对象规格：FlexSim 3D 对象库对齐表

> 本文档从 FlexSim 2025 公开帮助推导"对象—参数—语义"规格。每条规格后括号内是证据（见 [evidence.md](../research/evidence.md) 的条目号 + 置信度）。
> 参数**类型**沿用 FlexSim 的说法：`Unit`（带时间/长度单位的表达式）、`Number`、`Boolean`、`Options`（枚举）、`Code`（用户可编辑逻辑表达式）、`Table`（表）、`Object`（对象引用）。
> **默认值**只在官方文档明确写出时才写"文档默认"；否则写 `openflex 默认`（本项目自定，属于净室设计决策，不是 FlexSim 行为复刻）。

## 0. 分类与实现优先级

| 分类 | 对象 | openflex v1 目标 | 依据 |
|---|---|---|---|
| 固定资源 Fixed Resources | Source, Queue, Processor, Sink, Combiner, Separator, MultiProcessor, BasicFR, Storage Object(抽象) | **实现**：Source/Queue/Processor/Sink/Combiner/Separator；MultiProcessor/BasicFR 为 v1.1 | E01 高 |
| 任务执行器 Task Executers | Dispatcher, TaskExecuter, Operator, Transporter, Elevator, Robot, Crane, ASRSvehicle, BasicTE | **v1 实现简化版**：Dispatcher/Operator/Transporter/BasicTE（无路网几何，行程=欧氏距离/固定速度） | E13 高 |
| 输送机 Conveyors | Straight/Curved Conveyor, Join Conveyor, Merge Controller, Station, Photo Eye, Motor, Transfers, Mass Flow 系列 | **v1 不做**；仅在 spec 中登记参数以便后续 | E16 高 |
| 仓储 Warehousing | Storage Object, Rack, Floor Storage, Gravity Flow Rack, Push Back Rack, Drive In Rack | **v1 不做**（Storage Object 的"最小滞留时间"语义登记备查） | E15 高 |
| 流体 Fluid | Fluid Generator/Tank/Mixer/…（12 个） | **不做**（超出核心离散事件范围） | E01 高 |
| 可视化/其它 | Container, Visual Tool, USD Stage, State List, General Property List | 不做（`Container` 仅作为模型分层容器概念） | E01 高 |

关于 `BasicFR`（空壳固定资源）与 `BasicTE`（空壳任务执行器）：它们是"只提供标准事件/状态、逻辑全靠用户触发器"的基类对象，常用于自定义对象开发（E11 高、E13 高）。openflex 保留同名 `kind` 以便用户从零搭逻辑。

---

## 1. 所有固定资源的共同语义（基类）

固定资源的收发流水线是**四步**（E02 高），所有固定资源都走这四步，差异只在"何时收/何时放"：

1. **打开输入端口并找一件可接收的件**
   - 若开启 Pull：先调 `PullStrategy` 得输入端口号（`0`=打开全部端口）；再对被上游释放的件逐件调 `PullRequirement`（`1`=收下、`0`=跳过并等下一件被释放）。
   - 未开启 Pull：跳过全部拉取逻辑，等第一件可用件。
2. **处理**（各类对象不同：Queue 立即放行、Processor 走 setup+process…）。
3. **释放并用 `SendToPort` 决定输出端口**：`>0` 指定端口；`0`=打开全部端口、发给第一个能接收的下游；`-1`=**不释放**（需后续 `releaseitem()` 或 `moveobject()`）。
4. **移交**：未勾 `UseTransport` → 立即移交；勾选 → 调 `TransportRef`（`Request Transport From`）得到任务执行器/调度器引用，自动生成默认任务序列（Travel→FRLoad→Break→Travel→FRUnload）；`TransportRef` 返回 `0` 表示"用户已自己建了任务序列，别再建"。

配套不变式（E02 高，必须实现，否则容量判断会错）：

- 每个固定资源维护 **`nroftransportsIn`**（在途进入本对象、尚未到的件数）与 **`nroftransportsOut`**（已承诺运出、尚未被取走的件数）。
- 当对象的"有效含量"= content + nroftransportsIn 时，容量判断要用有效含量。文档例子：容量 10 的 Queue 当前 5 件、还有 5 件在途 → 必须关闭输入端口。
- 件一旦进入 "Waiting For Transport" 状态，**目的地不可改**（Send To Port 与拉取筛选已完成）。

### 1.1 固定资源公共属性（E02 高）

| 参数名 | 类型 | 文档默认 | 语义 |
|---|---|---|---|
| `MaxContent` | Number | 未说明（openflex 默认：Processor 1，Queue 1000） | 同时可容纳的最大件数 |
| `Pull` | Boolean | 未说明（openflex 默认 false） | 是否主动拉取上游 |
| `PullRequirement` | Code | 未说明（openflex 默认 `1`） | 对每个 ready 件返 1/0 |
| `PullStrategy` | Code | 未说明（openflex 默认 `0`=第一个可用端口） | 返回输入端口号 |
| `SendToPort` | Code | 未说明（openflex 默认 `0`） | 返回输出端口号 |
| `UseTransport` | Boolean | 未说明（openflex 默认 false） | 是否用任务执行器运输 |
| `TransportPriority` | Number | 未说明 | 运输任务序列优先级 |
| `TransportPreempt` | Options | 未说明 | 运输抢占策略 |
| `TransportRef` | Code | 默认 `current.centerObjects[1]`（E12 高） | 返回承运的 TaskExecuter/Dispatcher |

### 1.2 固定资源公共事件（E02 高）

| 触发器 | 触发时机 | 参数（名/类型） |
|---|---|---|
| `OnEntry` | 件刚进入对象，**先于其它逻辑** | EnteringItem(Object), InputPort(int) |
| `OnExit` | 件即将离开（尚未离开；此时 content **仍含**该件） | ExitingItem(Object), OutputPort(int) |
| `OnMessage` | 收到消息 | SendingObject(Object), MessageParam1..3(Variant) |
| `PullRequirement` | 输入端口变 ready 时，对每个 ready 件 | Item(Object), Port(int) |
| `PullStrategy` | 输入端口变 ready 时（无参） | — |
| `SendToPort` | 每件准备发送时**恰好一次** | ReleasingItem(Object) |
| `TransportReference` | 勾 UseTransport 且下游可接收时，在 SendToPort 之后 | Item, Port, Destination, Priority(double), Preempt(int) |
| `PickOffset` | 运输器到达本对象、准备偏移行程去取件时 | Item, Loader |
| `PlaceOffset` | 运输器到达、准备偏移行程去放件时 | Item, Unloader |
| `OnStop` | 被 `Object.stop()` / Time Table / MTBF-MTTR 停止时 | #StopRequests(int), State(int), ID(int), Priority(double), StateProfile(int) |
| `OnResume` | 恢复运行时 | #StopRequests(int), ID(int), StateProfile(int) |
| `OnContentChange` | content 变化（进入或离开） | — |
| `OnInputChange` | 件进入 | — |
| `OnOutputChange` | 件离开 | — |
| `OnStaytimeChange` | 件离开后、紧跟 OnOutputChange | — |
| `OnStateChange` | 状态变化 | StateProfileNode(treenode), ToState(int), FromState(int) |
| `OnStateValueChange` | 与 OnStateChange 同时；需给 state profile 号 | state profile number |
| `OnReset` | 模型 reset 时对每个对象 | — |
| `OnDraw` | 3D 视图重绘时（**不得用随机函数**，否则破坏可复现性） | View(treenode) |

> **净室要点**：`OnEntry/OnExit` 里**不要**做"停止对象、开关端口、创建/分派任务序列"这类会再产生事件的操作——文档明确建议改用 `senddelayedmessage(…, 0)` 把动作推迟到消息触发器，因为此刻对象自身的事件尚未建完（E02 高）。这是必须复刻的时序约束。

### 1.3 固定资源公共统计（E02 高）

| 指标 | 定义 |
|---|---|
| `input` | 每有 1 件进入 +1，单调递增 |
| `output` | 每有 1 件离开 +1，单调递增 |
| `content` | 当前件数（**容器件只算 1**）；另给 min/max/avg（整轮运行统计） |
| `staytime` | 每件离开时记 `exitTime − entryTime`；另给 min/max/avg |
| `state` | 分类跟踪变量：各状态累计时长与占比 |

---

## 2. Source（发生器）

职责：创建一类流项目并释放；是固定资源但**没有输入端口**（E03 高）。

**三种到达模式**（`ArrivalStyle`）：

| 模式 | 语义 | 关键细节 |
|---|---|---|
| `Inter-Arrivaltime` | 调用 inter-arrivaltime 函数，返回"等到下一件到达"的时间，等待→创建→释放→再调用 | **间隔的定义是"上一件离开 → 下一件到达"**，不是"到达→到达"。想让间隔=真实到达间隔，就在下游放一个大容量 Queue 让 source 立刻送出（E03 高） |
| `Arrival Schedule` | 按表到达：每行 ArrivalTime/ItemName/Quantity/标签列 | 到达时刻须单调不减；勾 `RepeatTable` 时末行之后 **0 秒**接首行，且**首行的到达时刻只在第一轮生效**（可当预热）；想加间隔就在末尾加 Quantity=0 的行（E03/E04 高） |
| `Arrival Sequence` | 表驱动但**无时间**：上一行全部件离开后立刻进入下一行 | 可重复（E03 高） |

**参数**（E03/E04 高）：

| 参数名 | 类型 | 文档默认 | 语义 |
|---|---|---|---|
| `ArrivalStyle` | Options | `Inter-Arrivaltime`（文档称默认策略为指数分布，E30 高） | 三模式之一 |
| `FlowItemClass` | Options | 未说明 | 创建哪种流项目类 |
| `TimeZeroArrival` | Boolean | 文档称"可勾选"，语义=勾选则 **0 时刻创建 1 件**，下一件在第一个间隔后（E04 高） | 与 interarrival 函数配合 |
| `InterArrivalTime` | Unit/Code | 未说明 | 返回等待时长 |
| `RepeatTable` | Boolean | 未说明 | 表是否循环 |
| `ArrivalSchedule` / `ArrivalSequence` | Table | 空表 | 见上 |

**专属事件**：`OnCreation`（CreatedItem, RowNumber；时刻=创建时，**不触发 OnEntry**，此时表里的标签列尚未加上）、`InterArrivalTime`（无参，仅在 Inter-Arrivaltime 模式；在 OnReset 之后、每次 OnExit 之前触发，返回值=距下一件的模型时间）（E03 高）。

**状态**：`generating`（无件、等下次创建）、`blocked`（已创建、等离开）。Sink 之外的固定资源不会出现 `generating`（E03 高）。

**统计**：`output`（累计离开）、`staytime`（被阻塞时滞留时间）（E03 高）。

---

## 3. Queue（队列）

职责：暂存件直到下游可接收（E01 高）。收件后**立即**释放（E02 高），除非在攒批。

| 参数名 | 类型 | 文档默认 | 语义 |
|---|---|---|---|
| `MaxContent` | Number | 未说明（openflex 默认 1000） | 同时可容纳上限 |
| `LIFO` | Boolean | 文档称"未勾选则 FIFO"，即默认 false=FIFO（E06 高） | 后进先出 |
| `DoBatching` | Boolean | 未说明（openflex 默认 false） | 是否攒批 |
| `BatchSize` | Number | 未说明（openflex 默认 1） | 目标批大小；**批内件仍逐件送下游**（E06 高） |
| `MaxBatchTime` | Unit | 0=无限等（E06 高） | 攒批最长等待；超时则按现有量放行 |
| `FlushContent` | Boolean | 未说明（openflex 默认 false） | 整批走完前不许新件进入 |
| `ItemPlacement` | Options | 未说明 | `StackVertically` / `HorizontalLine` / `StackInsideQueue` / `DoNothing`（**纯视觉**，但影响"谁最久"的呈现） |
| `StackBaseZ` | Unit | 未说明 | 堆叠起始高度 |

**状态**：`empty` / `collecting` / `releasing` / `waiting_for_transport`（E05 高）。注意：**不攒批且有件时状态是 `releasing`**，不是"有货待发"这类自造状态（E05 高）。

**统计**：标准固定资源统计（E05 高）。

**语义要点**：Queue 是**唯一**"收到即释放"的固定资源，因此它的 `staytime` 实际等于"等待下游可用的时间"。做 M/M/1 时 Queue 承担等待区（见 golden-cases GC-1）。

---

## 4. Processor（处理器）

职责：对一个件产生 setup + process 的时间延迟，可占用操作员，可多件并行（E07 高）。

| 参数名 | 类型 | 文档默认 | 语义 |
|---|---|---|---|
| `MaxContent` | Number | 未说明（openflex 默认 1） | 同时容纳件数（>1 即可并行处理） |
| `SetupTime` | Unit/Code | 未说明（openflex 默认 0） | 收到件后、开始加工前的等待；期间可占用操作员 |
| `ProcessTime` | Unit/Code | 未说明 | 单件加工时长 |
| `UseSetupOperators` | Boolean | 未说明 | setup 期间是否要操作员 |
| `NumSetupOperators` | Number | 未说明 | setup 期间要几个 |
| `UseProcessOperators` | Boolean | 未说明 | process 期间是否要操作员 |
| `NumProcessOperators` | Number | 未说明 | process 期间要几个 |
| `UseSameNumOperators` | Boolean | 未说明 | 对应面板 "Same as Setup"：勾选则 process **复用** setup 的操作员；不勾则释放 setup 操作员并另叫新的（E07 高） |
| `OperatorRef` | Code | 未说明 | 返回 operator/dispatcher 引用（可用特殊选项分别为 setup/process 指定不同对象） |
| `OperatorPriority` | Number | 未说明 | 发给操作员的任务序列优先级（同优先级按收到顺序，E07 高） |
| `OperatorPreempt` | Options | 未说明 | 调用操作员时的抢占值；**可能让操作员做平时不允许的事**（E07 高） |
| `AnimateItems` | Boolean | 未说明 | 纯视觉：加工期间件是否平移 |

**状态**：`idle` / `setup` / `processing` / `blocked` / `waiting_for_operator` / `waiting_for_transport`（E07 高）。
`blocked` = 已释放件但下游不收；`waiting_for_operator` = 等操作员到达（含等待**维修**）；`waiting_for_transport` = 下游可收但运输器没来取（E07 高）。

**统计**：标准固定资源统计。利用率口径见 [statistics.md](statistics.md) §2。

---

## 5. Sink（吸收器）

职责：把件从模型移除（E01 高）。

- **无自有参数**；面板只有 Statistics/Visuals/Labels/Input/Ports/Triggers（E08 高）。
- **状态恒为 collecting**（除非被停止），因此 Sink **不做有意义的状态统计**，只看 `input`（E08 高）。
- 统计：`input` = 累计进入件数（E08 高）。做端到端吞吐时用 `Sink.input` 或上游 `output`。

---

## 6. Combiner（合流/装箱）

职责：把多件组合（pack）/合并（join）/成批（batch）（E09 高）。

**收件规则（必须精确复刻）**：
- **先从输入端口 1 收恰好 1 件**，收不到就不允许其它端口进件；
- 之后按 **ComponentsList** 从端口 2、3… 各收指定件数（**列表第 1 行 ↔ 端口 2**），表随连接自动生成；
- 全部到齐后才开始 setup/process（E09 高）。

| 参数名 | 类型 | 文档默认 | 语义 |
|---|---|---|---|
| `CombineMode` | Options | 未说明 | `pack`：把端口 2+ 的件移入端口 1 的容器件再释放容器件；`join`：**销毁**除端口 1 外的件；`batch`：攒齐并完成 setup/process 后把**全部**件一起释放（E09 高） |
| `ComponentsList` | Table | 随连接生成 | 每个端口每批收几件 |
| 继承 Processor 的全部参数 | — | — | setup/process/操作员（E09 高） |

**不使用 `PullStrategy`**（自己处理拉取逻辑，E09 高）。

**陷阱（要写进实现检查表）**：
- 运输场景下，容器件在途时**只允许 1 件在途**；容器件到达后才允许 components 的件同时在途（E09 高）。
- 手动移出容器件时必须给**非 0** 端口号——`OnExit` 里端口号 0 被解释为"零件被装入容器"，会导致对象不再接收下一个容器件（E09 高）。

**状态**：与 Processor 相同（E09 高）。

---

## 7. Separator（拆包/复制）

职责：把一个件拆成多件（E10 高）。

| 参数名 | 类型 | 文档默认 | 语义 |
|---|---|---|---|
| `SeparateMode` | Options | 未说明 | `unpack`（解包容器）/ `split`（复制） |
| `SeparateQty` | Number | 未说明 | **两种模式语义不同**（见下） |
| 继承 Processor 参数 | — | — | setup/process 后**才**拆分（E10 高） |

**qty 语义差异（必须实现正确，否则产出计数错 1）**（E10 高）：

- `unpack`：解包**恰好 qty 件** → 总产出 = **qty + 1**（qty 件 + 容器件本身，容器在最后释放）
- `split`：复制 **qty − 1** 次 → 总产出 = **qty**

**顺序**：unpack 从**后往前**（先取容器内最后一件）；要改顺序须在 entry 触发器里设 rank（E10 高）。

**专属事件**：`SplitUnpackQuantity`（在 `OnProcessFinish` 之后触发，返回实际拆分/解包数；参数 Item + 后续参数见页面表格）（E10 高）。

---

## 8. MultiProcessor（多工序处理器）— v1.1

一条 process 列表，每道工序独立配置（E11 高）：

| 字段 | 语义 |
|---|---|
| Process Time | 该道工序耗时（picklist/表达式） |
| Number of Operators | 该工序要几个操作员 |
| Operator | operator/dispatcher 引用 |
| Priority | 发操作员任务序列的优先级 |
| Preemption | 抢占值 |

**状态**：`idle` + **每道工序一个用户自定义状态**（需在 State Table 建自定义状态表并引用该对象的 state profile）+ `blocked` + `waiting_for_operator` + `waiting_for_transport`（E11 高，E24 中）。

---

## 9. BasicFR（空壳固定资源）— v1.1

只提供固定资源标准事件与状态（`idle/setup/processing/blocked/waiting_for_operator/waiting_for_transport`）与 Input/Output/Ports/Triggers 面板；**没有任何内建处理逻辑**，全部行为由用户触发器实现（E11 高）。参数上只继承 1.1 的公共属性。

---

## 10. 任务执行器（Operator / Transporter / Dispatcher / BasicTE）

任务执行器族的能力（E13 高）：

- 执行**任务序列**（task sequence）与**偏移行程**（offset travel）；
- TaskExecuter 自身是 Dispatcher 的子类 → 可当"队长"给队员派活；
- 收到任务序列时的判定顺序：无活动序列 → 立即执行；新序列**抢占且优先级更高** → 抢占；否则走调度器的排队逻辑（按 Queue Strategy 得分插队）。

### 10.1 任务执行器公共属性（E13 高）

| 参数名 | 类型 | 语义 |
|---|---|---|
| `MaxContent`（面板名 Capacity） | Number | 最大装载件数。**只在 BREAK 任务判定"是否已满"时生效**；用户自建任务序列可让它超载（E13 高） |
| `MaxSpeed` | Unit | 最大速度（长度/时间） |
| `Acceleration` / `Deceleration` | Unit | 加速度/减速度（长度/时间²） |
| `LoadTime` / `UnloadTime` | Unit/Code | 装载/卸载耗时 |
| `OffsetTravel` | Options | 偏移行程是否启用/方式 |
| `PassTo` | Code | 返回把任务序列转给哪个输出端口 |
| `QueueStrategy` | Code | 返回排序分值（**值越大越靠前**，E14 高） |
| `BreakTo` | Code | 中断到其它任务序列的判定 |
| `AvailableOnStart` | Boolean | 启动时是否可用 |
| `Navigator` | Object | 所属路网 |
| `TravelOffsets` | Table | load/unload 的行程偏移 |

### 10.2 状态（E13 高）

`idle` / `travel_empty` / `travel_loaded` / `offset_travel_empty` / `offset_travel_loaded` / `loading` / `unloading` / `utilize` / `blocked`。

- `travel_empty`/`travel_loaded` 专属于 `TASKTYPE_TRAVEL` 任务；`loading`/`unloading` 只覆盖"到达后、等待 modeler 定义的 load/unload 时间"这段；`utilize` 表示在工位被占用（setup/process/repair），可用 `TASKTYPE_DELAY` 等复用（E13 高）。
- `blocked` = 在路网上行进时被阻塞（E13 高）。

### 10.3 统计（E13 高）

| 指标 | 定义 |
|---|---|
| `input` | 每装载 1 件 +1 |
| `output` | 每卸载 1 件 +1 |
| `content` | 当前载件数（容器件只算 1） |
| `staytime` | **卸载时刻 − 装载时刻** |
| `travelDistance` | 累计行程 |

### 10.4 各对象差异

- **Operator**：被对象调用以在 setup/process/repair 期间被占用；释放后可去别处；也可搬运件。偏移行程：**有涉及件**时只在 x/y 平面走，且总距离减去 `xsize(operator)/2 + xsize(item)/2`；**无涉及件**时按 TaskExecuter 常规（x/y 中心与 z 底到达目标）（E14 高）。
- **Transporter**：主要搬运；叉车可升到货架高度；可一次多件。偏移行程：有涉及件时"叉前端对准目标 x/y 并抬到目标 z"，无涉及件时同常规（E14 高）。
- **Dispatcher**：用输出端口定义"团队"；收到任务序列先调 `PassTo`（端口号>0=立刻转给该端口；0=按 Queue Strategy 入队；−1=不作为/假定已处理）（E14 高）。
- **BasicTE**：空壳任务执行器，逻辑全由用户触发器实现（E13 高）。
- Elevator / Robot / Crane / ASRSvehicle：v1 不实现，参数登记在 §12 备查。

---

## 11. 与 openflex 模型 JSON 的对应

见 [flexsim-to-openflex.md](flexsim-to-openflex.md)。要点：

- FlexSim 的"对象 + 属性面板"→ openflex `objects[].kind` + `objects[].params`
- FlexSim 的"A 连接（输入/输出端口）"→ `connections[]`（有序，**顺序即端口号**，E20 高）
- FlexSim 的"S 连接（中心端口）"→ `resource_refs`（见 flexsim-to-openflex §4）
- 每个对象的 `pos` 在 FlexSim 里是 3D 坐标，openflex v1 用 2D `[x, y]`

---

## 12. 登记但 v1 不实现的对象参数（备查）

| 对象 | 关键参数（E16/E15 高） |
|---|---|
| Straight/Curved Conveyor | `Speed`, `Acceleration`, `Deceleration`, `StartLocation`, `EndLocation`, `Width`, `UseVirtualLength`, `VirtualLength`, `IsAccumulating`, `IsSlugBuilder`, `CatchCondition`, `Visualization`, `EntrySpacePadding`, `EntrySpaceSizeFactor`；事件 `OnItemBump`(FrontSlug, BackSlug) |
| Join Conveyor | 同上 + 合流相关 |
| Merge Controller | 无自有状态/统计；仅 Center/In/Out 关系 + 放行策略 |
| Storage Object / Rack | 货位网格（bay→level→slot）、`MinimumDwellTime` 事件、`UseAsFixedResource`、Rack 附加 `ExtendColumns`/`PickPlaceYOffset`/`ShelfTilt` |
| Elevator / Robot / Crane / ASRSvehicle | 继承任务执行器参数 + 各自运动学参数（v1 统一按"固定速度 + 欧氏距离"退化实现） |

---

## 13. 实现检查表（易错点汇总）

从证据中提炼的、**容易实现错**的语义（每条都是回归测试点）：

1. Source 的 inter-arrivaltime 是"**离开→到达**"间隔，不是"到达→到达"（E03 高）。
2. `SendToPort` 返回 `0` = 全开端口、发给第一个可接收者；`-1` = 扣住不发（E02 高）。
3. `-1` 之后必须能通过 `releaseitem()` 再释放，且**再次调用** `SendToPort`（E02 高）。
4. Pull 模式下 `PullStrategy` 只在"准备接收下一件"时求值；容量 1 的 Processor 即"每件离开后求值一次"（E12 高）。
5. Queue 不攒批且有件时状态是 `releasing`（E05 高）。
6. Separator 的 unpack/split 产出差 1 件（E10 高）。
7. Combiner 必须先收到端口 1 的容器件（E09 高）。
8. 容器件只把 content 加 1（E02 高）。
9. OnEntry/OnExit 中不得做"会产生新事件"的操作，需用 0 延迟消息推迟（E02 高）。
10. OnDraw 中不得调用随机函数，否则同 seed 也不可复现（E02/E29 高）。
11. 预热结束要**清空统计**（含状态累计、吞吐、含量极值、停留时间样本）（E27 高）。
12. Sink 没有有意义的状态统计（E08 高）。
