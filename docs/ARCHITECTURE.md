# OpenFlex 架构

## 目标与净室原则

OpenFlex 的目标是：用 Rust 实现一个与 FlexSim 核心建模语义一致的离散事件仿真系统，模型可读、
结果可复现、可在三大桌面平台安装、可完全由 CLI 驱动。

净室（clean-room）在这里有严格含义：

1. **规格来源**：只使用公开文档中访客可见的内容（官方帮助、公开教材、公开示例截图），逐条记录在
   [research/evidence.md](research/evidence.md)。
2. **没有代码接触**：没有下载、反编译、反汇编 FlexSim 的任何二进制或脚本。
3. **独立实现**：数据模型、事件调度、对象状态机、统计公式均由本项目重新设计；命名与 FlexSim 的对应
   关系只用于让熟悉 FlexSim 的用户快速上手。
4. **可验证**：每个语义都对应一个测试或可数值对照的案例（见 [spec/golden-cases.md](spec/golden-cases.md)）。

## 分层

```
┌─────────────────────────────────────────────┐
│ openflex-cli        clap 命令、参数覆盖、退出码 │
├─────────────────────────────────────────────┤
│ openflex-report     表格 / JSON / CSV / Markdown │
├─────────────────────────────────────────────┤
│ openflex-core       事件日历、对象运行时、统计、实验 │
├─────────────────────────────────────────────┤
│ openflex-model      JSON schema、分布、表达式、校验 │
└─────────────────────────────────────────────┘
```

- **openflex-model** 只拥有数据与静态检查：反序列化、参数校验、表达式解析、场景覆盖。它不推进时间。
- **openflex-core** 拥有全部运行时语义：事件、对象状态、路由、背压、统计累计、复现聚合。
- **openflex-report** 是纯函数式的渲染层，输入 `SimReport`，不接触引擎。
- **openflex-cli** 负责 I/O、参数、退出码与人类可读错误。

## 引擎

### 时间与事件

- 模型时间是一个 `f64`，单位由模型声明（秒/分/小时/天），引擎内部不换算。
- 事件日历是按 `(time, seq)` 排序的最小堆。`seq` 是插入序号，保证同一时刻的事件按调度顺序执行，
  从而使给定 seed 的运行完全确定。
- 主循环弹出事件直到 `stop_time`；结束后把所有对象的时间加权统计补齐到 `stop_time`。
- 防止零时间死循环：事件数上限 5000 万，超出报 `EventLimit`。

### 对象运行时

每个对象维护：

- `content: Vec<FlowItem>`：排队/在处理的项；
- `out_pending: Vec<FlowItem>`：已完成但下游拒收的项（背压）；
- 状态机：`Idle / Busy / Blocked / Down / OffShift`；
- 累计器：输入/输出计数、内容积分、停留时间和、状态时间、峰值内容。

状态时间与内容积分都在状态变化时结算，保证长时间运行不累积误差。

### 对象语义（v1 实现）

| 对象 | 实现要点 |
|---|---|
| 任意对象（on_exit） | 项在**路由选择之后、接收方 OnEntry 之前**触发 from 对象的 `on_exit`，因此 on_exit 改的标签对下游路由可见；失败重试不重复触发 |
| Source | 首件在 `first_arrival`（默认 0 = t=0 创建）；后续间隔按“上一批**离开** Source → 下一件到达”计时，下游阻塞时暂停到达节奏；也支持 `arrival_schedule`（表驱动、`repeat` 首行只首轮）与 `arrival_sequence`（上一批全部离开后推进） |
| Queue / Rack | 收到即释放；攒批（`batch_size`）时整批一次性提交（离开时刻 = 批次释放时刻），`batch_max_wait` 到期强制放行 |
| Processor | setup + process；故障立即打断加工、修复后补足剩余时间；操作员未到位时 `Blocked`；`operator_preempt` 可抢占低优先级操作员占用（被抢占方暂停并补足剩余时间） |
| Conveyor | 一维直线带：位置、速度、间距、`accumulating` 累积/非累积、容量；末端被阻塞时累积带继续压缩、非累积带整体冻结；`photo_eyes`（精确位置触发 `on_photo_eye`）、`stations`（停留加工、后车排队）与 `join_inputs`（两上游带公平轮转汇入） |
| Combiner | 端口 1（第一条连接）为容器；pack 等待 `components_per_container` 个组件后只释放容器（组件不单独计内容）；join 每端口一件、合并为一件；batch 收集一批后按 setup+process 加工并逐件释放（带 `batch` 标签） |
| Separator | `separate_mode`：`unpack`（默认）释放 `qty` 个零件 **加容器本身**（qty+1）；`split` 只释放 `qty` 个零件；0 = 拆出容器内全部组件；多输出端口按零件序号轮转分发 |
| Merge | 每个输入端口暂存一件；`wait_for_all` 时全部到齐才合成一件，否则每来一件即输出 |
| MultiProcessor | `slots` 个并行加工槽，每槽独立 setup/process；利用率按槽位秒/(时长×slots)；操作员按忙碌期整批申请 |
| Split | 按 `by_item_type` 选端口，或轮询输出端口 |
| Transporter / Dispatcher | Processor 通过 `process_operator` 向 Dispatcher 请求操作员；任务拆两段：home→取货（`network` 双向最短路或欧氏距离）→ load → 送达 → unload → 加工 → 空车回 home；`network` 缺失时回退欧氏；容量 = 可并发的任务数 |
| Person / AGV | `person`（capacity 固定 1，平面行走，network 或欧氏）、`agv`（transporter alias）；与 Dispatcher 的优先级/抢占/容量机制共用 |
| Elevator | 只做垂直运输：距离 = |Δz|，到站采样 `stop_time`（取货/送达各一次），`capacity` 表示并发任务数；对象新增 `z`（楼层/高度） |
| Resource | 计数型资源池（`capacity`、`downtimes`）；占用期间 Processor 处于 `Blocked`，停机窗口内不发放 |
| 全部对象 | 可选 `shifts`（`start`/`duration`/`repeat_every`）与 `timetable`（窗口列表）：窗口外进入 `OffShift`、暂停加工并拒绝进件，恢复后补足剩余加工时间 |
| Pull 模式 | 对象可配置 `pull:{enabled,strategy,requirement}`，由下游主动从上游取件；`requirement` 为假的件被跳过 |
| Hold | `PortStrategy::Hold` 扣住已完成项，直到 `release_item` 动作单次放行 |

### 传送带的简化

- 直线带，无转弯、合流、电机/光电门。
- 项到达末端即瞬时移交下游，没有独立的 transfer 延迟；因此“离开带面”与“移交完成”同一时刻。

### 任务执行器的简化

- 无路网、加速度与碰撞；行程 = 直线距离 / 固定速度。
- 不搬运 flow item，`capacity` 解释为可并发的操作员任务数。
- 只实现“Processor 请求操作员”这一种任务序列（travel → load → 让 Processor 加工 → unload → 回 home），不实现抢占、优先级与多段任务序列。
- 操作员在加工期间不释放；故障时随 Processor 一起暂停（见下）。

### 路由与背压

连接（`Connection`）决定拓扑；每条连接可带 `PortStrategy`：

| 策略 | 语义 |
|---|---|
| `first_available`（默认） | 按端口顺序选择第一个可接收的下游 |
| `round_robin` | 轮询可用输出 |
| `random` | 在可用输出中均匀随机 |
| `by_percentage` | 按权重抽取（不检查可用性，和 FlexSim 一样会造成阻塞） |
| `by_condition` | 依序求值条件表达式，命中即选 |
| `by_item_type` | `item.type` 作为端口索引 |
| `by_label` | 数值标签作为端口索引 |

下游 `can_accept` 为 false 时，上游把项留在原位并把状态置为 `Blocked`；下游一旦腾出容量就向所有上游
调度一次 `Flow` 事件，上游重试。这就是 FlexSim 中“阻塞—解除阻塞”的行为。

### 触发器与表达式

触发器是结构化动作列表，而不是任意脚本：

```json
"triggers": {
  "on_entry": [
    { "action": "set_label", "name": "arrived", "value": "time" },
    { "action": "increment_label", "name": "visits", "by": "1" }
  ],
  "on_exit": [{ "action": "send_message", "target": "Queue2", "delay": "0" }]
}
```

表达式语言支持字面量、`time`、`item.type`、`item.label.<name>`、`Object.content`、`Object.state`、
`Object.busy` 等变量，四则运算、比较、`&&`/`||`/`!`，以及 `if/min/max/abs/round/floor/ceil/sqrt/len`
函数。表达式在加载时解析，语法错误会出现在 `openflex validate` 的输出里。

### 故障、班次与实验

- 故障：Processor 可配置 `failure: { mtbf, mttr }`。Up time 从上次修复完成开始计时；
  故障**立即打断**当前 setup/process（记录剩余时间），修复后补足剩余时间继续；`on_breakdown` /
  `on_repair` 触发器用探测项执行动作。
- 班次：对象可配置 `shifts: [{ start, duration, repeat_every }]`（`repeat_every = 0` 表示只出现一次）。
  窗口外对象进入 `OffShift`：暂停 setup/process 并记录剩余时间、拒绝新进件、Conveyor 停止推进；
  下一个窗口开始时恢复。统计累计 `off_shift` 时间。
- 实验：模型内 `experiments[]` 声明“场景（覆盖参数）× 复现次数”，`openflex experiment` 逐场景运行并
  输出聚合与 95% 置信区间。

### 随机性与确定性

- 两条独立的 `StdRng`（ChaCha12）流：一条用于到达间隔，一条用于其余全部采样（加工、故障、装载等）。
  因此 `--set Proc.process_time=…` 不会改变到达序列，`openflex compare` 的 A/B 对比是可比的
  （common random numbers）。
- 复现 `i` 的 seed 由 SplitMix64 从基础 seed 派生，因此复现之间独立，且整组结果可由基础 seed 重放。
- 同一模型、同一 seed、同一版本 → 逐位相同的结果。

## 语义决策（FlexSim 文档未定义、由 OpenFlex 规定）

FlexSim 的公开帮助没有定义下面几项行为，但任何离散事件引擎都必须给出确定答案。OpenFlex 的规定如下，
并在此公开，便于模型作者预期结果：

1. **同一时刻的事件顺序**：事件按 `(time, seq)` 排序，`seq` 是调度顺序。因此同一时刻的事件按“谁先被调度”
   执行；同一模型、同一版本、同一 seed 的结果完全确定。
2. **故障是否打断在制品**：是。故障立即打断正在进行的 setup/process，修复后**补足剩余加工时间**，
   而不是重新开始；加工计时与 MTBF 计时分离。
3. **Separator 的 `split_count`**：默认 `separate_mode = "unpack"`，`split_count = qty` 表示释放
   `qty` 个零件 **加容器本身**（共 qty+1）；`split` 模式释放 qty 个零件（复制/拆分，不含容器）。
   0 在 unpack 模式下表示拆出容器内全部组件。
4. **Source 的首次到达**：`first_arrival`（默认 0）是首件的模型时间；后续间隔从“上一批**全部离开** Source”
   开始计时（与 FlexSim 的 inter-arrivaltime 定义一致），下游阻塞会推迟后续到达。
5. **停止边界**：`stop_time` 是开区间上界——`time >= stop_time` 的事件不会被处理；`Source.stop_time`
   同样是 `now >= stop` 时不再创建。
6. **班次与停机**：班次窗口外对象进入 `off_shift`，暂停加工并拒绝进件；窗口恢复后补足剩余加工时间，
   不重置已完成的 setup。
7. **统计窗口**：`warmup` 之前的计数与时间加权量在 warmup 时刻清零；`created/completed` 只统计
   warmup 之后创建的项。

## 检查点与续跑

`openflex run --save-checkpoint cp.json` 把运行到停止时刻的状态写成 JSON；`--resume cp.json --until T`
从快照继续，结果与一次性运行逐位一致（core 集成测试与 CLI 端到端测试均覆盖）。

快照包含：时钟、事件堆（含 stop 之后已排定的事件）、全部对象运行态（content/out_pending/状态/相位/计时器/
传送带位置与工位/操作员/在途/班次暂停/统计累加器）、两条可序列化的 `ChaCha12Rng` 随机流、计数器与运行配置。
`simulate_with_checkpoint` / `simulate_with_checkpoint_traced` 是底层 API。

限制：快照不内嵌模型（要求同名、同对象列表、同 seed/warmup）；仅单复现；trace 不随续跑恢复。

## Process Flow token 运行时

模型可声明 `process_flow`（模型级）：`blocks`（create/delay/assign/decision/sink；transfer 暂未实现，validate 拒绝）与带端口的 `connections`。
token 是带 id/标签/创建时间的轻量实体，沿块流转；引擎新增 `FlowCreate/FlowAdvance/FlowDelayDone` 事件，
`SimReport.process_flow` 给出每块 input/output/content_avg/avg_stay_time 与 token 创建/完成数，trace 记录
`token_create/token_delay/token_assign/token_decision/token_sink`，检查点覆盖 token 队列与块统计。
约束：流程图必须无环；decision 未命中条件走第一个出边；flow-only 模型允许空 `objects`；与对象模型可共存。

对象级 `params.process_flow` 与模型级流程合并为一张全局 block 图：`create.on_entry` 在该对象每次收货时产 token；
`transfer(to_object)` 把 token 转成 flow item 注入目标对象（继承标签/类型，目标满时挂起并在容量释放时重试），
对象级 block 的报告 id 形如 `Queue1.Create1`；`from_object` 暂未实现。

## FlexScript 脚本层（模型层已就绪）

`rhai` 沙箱脚本：触发器动作 `{"action":"script","source":"..."}`、条件路由/流程决策的 `script_condition`。
脚本可读 `time/item_type/item_id/label(name)`，调用 `set_label/set_item_type/send_message/release_item/log`，
无文件/网络/时钟/随机 API，操作数与字符串长度受限。模型层已完成语法校验与 `core/src/script.rs` 运行时封装；
**引擎执行集成尚未接线**（`apply_actions` 遇到 script 返回明确的未实现错误），下一批完成。

## Process Flow 等价模式

同样的语义也可以用现有对象与触发器表达（无 token 运行时的场景）：

| Process Flow 块 | OpenFlex 等价物 |
|---|---|
| Create | `source`（到达表或间隔分布） |
| Delay | `processor`（process_time） |
| Assign | `source.on_create` / 对象触发器里的 `set_label`、`set_item_type` |
| Decision | 输出连接的 `strategy: by_condition`（表达式路由） |
| Sink | `sink` |

示例：`examples/objects/process-flow-equivalent.ofm`。若要实现真正的 token 运行时，前置工作是补 Processor 的 `on_entry/on_exit` 触发点与零延迟消息动作。

## Fluid 子系统

模型含 fluid 对象（`fluid_generator` / `fluid_tank` / `fluid_valve` / `fluid_sink`）时，引擎启用固定步长事件
`FluidStep`（`Model.fluid_step`，默认 0.1）；纯离散模型不产生任何额外事件。

每步按 Kahn 拓扑序结算：generator 产流（可设 `stop_time`）；节点出流平均分配到各出边并受
`Connection.flow_rate` 限幅；tank 同一步先收后放（超容量部分计入 overflow，`max_outflow` 限制出流）；
valve 按 `open`/`ratio` 通过（无缓冲，超出丢弃）；sink 全收。`SimReport.fluid` 给出每对象
level_avg/level_max/throughput/overflow 与系统 total_inflow/total_throughput/total_overflow。

fluid 对象永不接收 flow item，fluid 与离散对象之间的连接在校验阶段被拒绝；二者可在同一模型中并行。
简化：不可压缩、无温度/压力/管道延迟；固定步长积分；fluid 不触发对象触发器、不支持 shifts/timetable。

## 统计口径

| 指标 | 定义 |
|---|---|
| `input_count` | 被对象接收的项数（不含被拒的 offer） |
| `output_count` | 成功交给下游/被 Sink 消耗的项数 |
| `avg_content` | `∫ content dt / (stop_time − warmup)` |
| `avg_stay_time` | `Σ(离开时刻 − 进入时刻) / 离开数` |
| `utilization` | 忙碌时间 / 统计时长 |
| `throughput` | `output_count / 统计时长` |
| `idle/busy/blocked/down/off_shift` | 各状态时间占统计时长的比例 |

口径细节：

- `created` / `completed` 都只统计 `warmup` 之后创建的项：统计窗外的在制项不计入 `completed`，
  因此 `created − completed` 就是统计窗内仍在系统中的项数。
- 容器件只算 1 件内容；容器内的组件不单独计入 Combiner/Separator 的 `content`。
- Queue 攒批的项在**批次释放时刻**记 `output` 与停留时间（即使下游随后逐件接收）。
- `avg_content` 的积分在每个内容变化**之前**结算旧内容，再以新内容重新起算，避免把整段间隔记到新件上。

多复现聚合给出均值、标准差、最小值、最大值与 95% 置信区间半宽（`1.96 × se`）。

## 扩展点

- **新对象**：在 `openflex-model` 加参数结构与 `ObjectParams` 变体，在 `validate.rs` 加校验，在
  `openflex-core/src/engine.rs`（或拆分出的对象模块）实现状态与事件处理，补一个示例模型与测试。
- **新分布**：在 `Distribution` 加变体、`validate`/`mean` 与采样分支。
- **新动作**：在 `Action` 加变体并在 `apply_actions` 实现。
- **新路由策略**：在 `PortStrategy` 加变体并在 `route` 实现。

## 已知限制

- 没有 3D 图形界面；布局坐标（`pos`）只用于运输距离与可视化导出。
- 表达式语言不是完整的 FlexScript；复杂逻辑需要用多个对象/触发器组合表达。
- 运输与任务执行器是简化模型（见 [COMPATIBILITY.md](COMPATIBILITY.md)）。
- 未实现完整 3D 界面、FlexScript 脚本语言、OptQuest 优化；People 仅平面直线/network（无 A* 网格与人群）、AGV 无电池、电梯无门控/召唤策略。
- Conveyor 仍是直线带（无曲线/Join 合流/入口公平轮转）；Rack 的 push_back/drive_in 为近似（无货位/巷道）；Transporter 无抢占/自定义多段任务/加速度/交通冲突。
