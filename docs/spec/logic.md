# openflex 逻辑规格：事件、触发器、路由、标签、消息、故障与班次、任务执行器

> 证据编号见 [evidence.md](../research/evidence.md)。所有条目括注（证据号 + 置信度）。

## 1. 仿真范式：离散事件 + 状态机

FlexSim 是**离散事件仿真（DES）**：业务系统被表示为随时间推进的一系列离散事件（到达、运输、加工、故障…）；每个对象在事件驱动下切换**状态**，并在事件点上执行**触发器**（E18 高）。

实现含义（openflex 必须满足的不变式）：

1. **事件表**按 `(time, priority, seq)` 全序排列；同刻事件必须可确定性排序，否则复现失败。FlexSim 的事件表可视化工具 Event List 支持按 Time/Object/Event Type/Involved/Event Data 查看与过滤（E32 高）。
2. **时间只前进不倒退**；`step()` = 处理恰好一个事件；`skip()` = 处理一个事件后把**同一模型时刻**的其余事件一并处理（E32 高）。
3. 对象与件的交互**事件顺序固定**（例：Processor 的三段固定为 取件 → 加工 → 送件）（E18 高）。

## 2. 触发器（Trigger）机制

### 2.1 挂载与求值

- 触发器**不是**所有对象都有的固定集合：Triggers 面板初始为空，点 Add 弹出**该对象类别允许的触发器清单**，具体清单定义在该类对象的 Concepts 页 Events 节（E17 高）。
- 触发器逻辑可以是"预置选项拼装"或直接写脚本（openflex 用表达式/受限 DSL，见 §9 与 [flexsim-to-openflex.md](flexsim-to-openflex.md)）。
- 触发器分两类语义（E02 高）：
  - **通知型**（OnEntry、OnExit、OnContentChange、OnReset…）：无返回值，只产生副作用；
  - **求值型**（SendToPort、PullStrategy、PullRequirement、TransportRef、SplitUnpackQuantity…）：**必须返回**规定类型的值，返回值改变对象行为。

### 2.2 公共触发器清单

固定资源的 18 个公共触发器与参数表见 [objects.md](objects.md) §1.2；任务执行器另有 `OnReceiveTaskSequence`、`OnStartTask`、`OnFinishTask`、`OnPreempt`、`BreakTo`、`OnResourceAvailable`、`PassTo`、`QueueStrategy` 等（E13 高）。

### 2.3 时序禁令（必须复刻，否则行为不一致）

在 `OnEntry` / `OnExit` 中**不要**执行会再产生事件的动作（停止对象、开关输入/输出端口、创建或分派任务序列等）。原因：此刻对象自身的后续事件尚未建好。正确做法是 `sendDelayedMessage(obj, 0, ...)`（0 延迟消息），在消息触发器里做（E02 高）。

> openflex 实现建议：对象内部维护"事件构建阶段"标志；在 `entry/exit` 阶段对这些 API 返回可诊断错误（或按文档语义自动降级为 0 延迟任务），并在 CLI 校验模式下给出 warning。

### 2.4 状态与状态 profile

- 状态以**分类跟踪变量**记录历史；`OnStateChange` 给出 `(StateProfileNode, ToState, FromState)`，`OnStateValueChange` 需给定 state profile 号（0 = 默认 profile）（E02 高）。
- 自定义 profile 用于"同一对象有多套状态语义"（MultiProcessor 每道工序一个状态）（E11 高、E24 中）。
- 对象被**多次停止**时会把状态压栈，恢复时逐层弹出（E22 高）。

## 3. 路由：Send To Port

### 3.1 返回值语义（这是最容易实现错的地方，E02 高）

| 返回值 | 行为 |
|---|---|
| `p > 0` | 打开输出端口 `p`，把端口号**记在件上**；等"该输出端口 + 下游对应输入端口都开"时推送/拉取 |
| `0` | 打开**全部**输出端口，发给**第一个能接收**的下游（不指定端口） |
| `-1` | **不释放**该件；之后必须用 `releaseitem()` 显式释放（释放时会**再次调用** SendToPort），或用 `moveobject()` 直接搬走 |

### 3.2 求值时机

- 每件**恰好一次**，在该件"准备好发送"的时刻（Processor = 加工结束；Queue = 批次攒齐并释放时）（E02 高）。
- 若对象配置了"重新求值 SendToPort"，则**每当有下游对象变为可接收**时对同一件重新求值；不是持续求值。需要手动触发时用 `openoutput()`（E02 高）。
- 与 Pull 的关系：v6 起 Send To Port 与 Pull Strategy **同时生效**，两者都为真才发生拉取（E02 高）。

### 3.3 常用路由策略（文档列举的语义类别，E02 高）

一对一（单端口）、一对多（round-robin / first available / random / **by itemtype** / by label 值）、多对一（拉取侧的 round-robin / first available / **longest waiting** / random / by itemtype）。

> **按 itemtype 直接发到端口 p 的规则**：itemtype 为 1 的件走端口 1 —— 因此**端口顺序**就是语义的一部分（E02 高）。openflex 的 `connections[]` 必须**保序**，且不得去重排序。

## 4. 拉取：Pull

| 项 | 语义（E12 高） |
|---|---|
| 开关 | `Pull` 勾选才有 Pull Strategy / Pull Requirement |
| `PullStrategy` | 返回要拉取的**输入端口号**；`0` = 从第一个可用端口拉。仅在 reset 时与"对象准备接收下一件"时求值（容量 1 的 Processor ⇒ 每件离开后求值一次） |
| `PullRequirement` | 返回 1/0，只对**已处于 ready 状态**（FRSTATE_READY）的件求值；对同一件返回 0 后，只有该上游**新释放**件时才重新评估（不会对已拒绝的件反复求值），但每完成一次"收件→释放"循环后会重新评估所有件（E02 高） |
| 与端口 | 拉取方只能从连在自己输入端口上的对象里找，并按**端口顺序**（端口 1 → 端口 2 …）搜索（E02 高） |

## 5. 端口与连接

| 类型 | 快捷键 | 用途 | 语义（E20 高） |
|---|---|---|---|
| 输入/输出端口（A 连接） | `A` | 固定资源之间传件；上游 output → 下游 input | 定义"谁能把件送给谁"与"谁从谁那里取件"的**搜索图**；端口有**开/闭**状态表示可用性 |
| 中心端口（S 连接） | `S` | 任务执行器 ↔ 固定资源；任意两对象间的引用关系 | 不传件，只建立引用：运输承运方、setup/process 所需操作员、一般通信 |
| 扩展连接（extended） | — | 仅网络节点 / Traffic Control | v1 不实现 |

`connections[]` 在 openflex 里表达 **A 连接**；S 连接用 `resource_refs`（见 flexsim-to-openflex §4）。

## 6. 标签（Label）

三要素（E19 高）：

- **Owner**：标签属于某个流项目 / token / 3D 对象；
- **Name**：创建时定名，运行期**不可改名**；
- **Value**：可为文本、数字、对象引用、数组；运行期**可变**。

用途（E19 高）：排序与条件路由（按标签分流/从列表筛选）、表示对象或件的状态演变（例：`Status: empty → filled → capped`）、给资源打标记（例：操作员"是否在休息"）。

工程约束（E29 高）：

- 对象标签属于**持久数据**：模型 reset **不会**自动恢复初值，需在 `OnReset` 触发器里手动恢复，或勾选对象 Label 页的 "Automatically Reset Labels"。
- 件的标签随件生命周期消失（Sink 吸收后不存在），跨对象追踪只能靠统计采集。

标签匹配（E42 中）：事件监听（Process Flow / Statistics Collector）通过"标签匹配表"把**事件参数**（如 Entering/Exiting Item、Station/Destination）写进标签，从而把事件信息带给后续活动或采集表。openflex v1 不做事件监听框架，但**标签复制语义**要在 `combiner/separator` 上保留（pack 时容器件保留，unpack 时子件继承/清除哪些标签需定义，见 flexsim-to-openflex §5 "标签继承规则"）。

## 7. 消息机制

| 机制 | 语义（E02 高） |
|---|---|
| `OnMessage` 触发器 | 收到消息时触发；参数 `SendingObject`(Object) + `MessageParam1..3`(Variant，可为数/字符串/treenode/数组) |
| 立即消息 | 立刻在**当前事件内**同步触发接收方 OnMessage（会嵌套执行） |
| 延迟消息 | 在 `time + delay` 时刻作为**新事件**触发；`delay = 0` 用来"把动作推到当前事件之后"（§2.3 的标准做法） |
| 用途 | 解耦对象间通信；避免在 entry/exit 阶段破坏事件构建；实现"等一帧再处理" |

openflex 实现建议：消息 = 事件表里的一个事件类型 `(target, messageId, p1, p2, p3)`；`delay=0` 仍要进事件表（不能同步调用），以保证与 FlexSim 相同的时序。

## 8. 故障与班次（停机）

### 8.1 MTBF/MTTR（随机故障）

对**一组**对象设置随机故障与修复（E21 高）：

| 函数/参数 | 语义 |
|---|---|
| `FirstFailureTime` | 首次故障时刻函数；**返回负数 = 忽略首次故障** |
| `UpTime`（=MTBF） | 定义：**上次恢复 → 下次开始停机**之间的时长。可只让**特定状态**的时间计入 Up Time（例：Processor 只在 `processing` 状态计时，`idle` 时时间不累积） |
| `DownTime`（=MTTR） | 停机多久后恢复；**同组所有成员同时恢复** |
| `BreakDownMembersIndividually` | 勾选 ⇒ FirstFailure/Up/Down 对每个成员**各自**求值（各自独立的故障节奏）；不勾 ⇒ 整组共用一个时间 |
| `DownFunction` / `ResumeFunction` | 停机/恢复时**对每个成员各执行一次**（在这里停止/恢复对象） |
| `DownBehavior` | 可复用的停机动作包（Down/Resume/OnDown/OnResume + 标签）；选中它时上面两个函数不再触发（E23 高） |
| `Enabled` | 可被 Experimenter 按场景开关 |

**实现要点**：
- 故障计时器按"计时的状态集合"累积；对象进入不计时的状态时**暂停**累积（这是 MTBF 与 ProcessTime 交互的关键）。
- 停机是**停止请求栈**（见 §8.3），可与班次叠加。

### 8.2 Time Table（计划停机 / 班次）

按表制造停机（E22 高）：

- 每行 = `(downTime, duration)`，单位是模型时间单位；命中时：
  1. 对每个成员执行 `DownFunction`；
  2. 再执行一次 `OnDown`（参数：成员列表 + 该表行）。
- 时段结束时：
  1. 对每个成员执行 `ResumeFunction`；
  2. 再执行一次 `OnResume`。
- 每行可指定 **down state + state profile**，配合 `stopobject()/resumeobject()` 使用（E22 高）。
- **跨日/周边界相接的时段，若状态或 DownBehavior 相同会自动合并**（例：周一 22:00–24:00 与 周二 00:00–02:00 合成一段）。这条会影响"停机次数"这种统计，必须实现（E22 高）。
- 可关联 `DownBehavior` 以复用动作（E23 高）。

> 班次在 FlexSim 里就是"周期性 Time Table"（每班/每天/每周重复）。openflex 的 `shift` 语法见 flexsim-to-openflex §6。

### 8.3 停机状态栈（关键语义）

对象可被**多个** Time Table 与 MTBF/MTTR 同时停止；每次停止把"新状态"压栈，恢复时**逐层弹回**上一层的状态（E22 高、E02 高：OnStop 的 `#StopRequests` = 含本次的停机请求数，OnResume 的 `#StopRequests` = 剩余请求数）。

实现细则建议：
- `stop(id, state, priority, stateProfile)`：把请求压入栈（按 priority 决定当前生效状态，取最高优先级；同优先级取最后压入）；
- `resume(id)`：弹出该 id 的请求；若栈非空则回到栈顶状态；
- 触发 `OnStop`/`OnResume` 并计入对应 state 的时长统计。

## 9. 任务执行器与任务序列

### 9.1 任务序列结构

任务序列 = 发给任务执行器的**有序任务列表**；执行器逐条执行，可被更高优先级的任务序列**抢占**（E40 高、E13 高）。

自动运输（`UseTransport`）时，对象生成的任务序列形状固定（E02 高）：

```
1. Travel   → 去上游对象
2. FRLoad   → 装载件（会通知固定资源递减 nroftransportsOut）
3. Break    → 视情况中断到其它序列
4. Travel   → 去下游对象
5. FRUnload → 卸载件（会通知固定资源递减 nroftransportsIn）
```

`FRLoad/FRUnload` 与普通 `Load/Unload` 的唯一区别：**移动件之前先通知固定资源**，保证 `nroftransportsIn/Out` 递减正确（E02 高）。每个 `TransportRef` 调用**最终必须**对应恰好一个 FRLoad + 一个 FRUnload（E02 高）。

### 9.2 优先级与抢占

- 默认 FIFO；任务序列优先级默认 `0`，**数值越大越先做**（`10` 先于 `1`）（E41 高）。
- 抢占（preempt）决定：是否中断当前任务；被中断的任务是**重头再做**还是**直接删除**（E41 高）。
- Dispatcher 的排队：收到序列先 `PassTo`（>0 端口转发；0 入队；−1 不作为），入队时用 `QueueStrategy` 给每个待排队列算分（**分值大者靠前**），并按序逐个比较定位插入位置（E14 高）。
- TaskExecuter 自身的判定顺序：无活动序列→立即执行；新序列抢占且优先级更高→抢占；否则走 Dispatcher 排队逻辑（E13 高）。

### 9.3 容量（Capacity）的真实语义

`MaxContent`（Capacity）**只在 BREAK 任务判定"是否已满"时起作用**：若执行器遇到 break 任务且已达容量上限，则不执行 break、继续当前序列。用户自建任务序列可以超载（E13 高）。

### 9.4 任务类型（TASKTYPE_*）

文档中明确出现的任务类型（E13/E02 高）：`TRAVEL`、`TRAVELTOLOC`、`LOAD`、`UNLOAD`、`FRLOAD`、`FRUNLOAD`、`DELAY`、`UTILIZE`、`BREAK`、`TIMEDTRAVEL`、`AGVTRAVEL`、`CUSTOM`。
状态与任务的对应：`TRAVEL` → `travel_empty`/`travel_loaded`；`LOAD`/`UNLOAD` → `loading`/`unloading`；`UTILIZE` → `utilize`（E13 高）。

### 9.5 行程与偏移

- 常规行程：从当前位置到目标对象；速度/加速度受 `MaxSpeed`/`Acceleration`/`Deceleration` 约束（E13 高）。
- 偏移行程（offset travel）：用于"在对象内部走到件的位置"；Operator 在有涉及件时只走 x/y 平面并把总距离减去 `xsize(operator)/2 + xsize(item)/2`；Transporter 有涉及件时叉前端对准目标 x/y 并抬到目标 z（E14 高）。
- v1 简化：openflex 以"固定速度 + 直线距离"实现，暂不做加速度积分与叉车升降；**但接口字段保留**，见 flexsim-to-openflex §7。

## 10. 状态机总表

| 对象族 | 状态集合 | 证据 |
|---|---|---|
| Source | `generating`, `blocked` | E03 高 |
| Queue | `empty`, `collecting`, `releasing`, `waiting_for_transport` | E05 高 |
| Processor / Combiner / Separator / BasicFR | `idle`, `setup`, `processing`, `blocked`, `waiting_for_operator`, `waiting_for_transport` | E07/E09/E10/E11 高 |
| MultiProcessor | `idle`, `process_1..n`（用户自定义状态）, `blocked`, `waiting_for_operator`, `waiting_for_transport` | E11 高 |
| Sink | 恒 `collecting`（不做状态统计） | E08 高 |
| Task Executer 族 | `idle`, `travel_empty`, `travel_loaded`, `offset_travel_empty`, `offset_travel_loaded`, `loading`, `unloading`, `utilize`, `blocked` | E13 高 |
| Storage/Rack、Merge Controller | 不实现状态（用 content 图统计） | E15/E16 高 |

被停止的对象：进入"停机状态"（由 `stop()` 的 state 与 stateProfile 决定），并压栈（§8.3）。

## 11. 逻辑层的可复现性约束

同 seed 同参数必须**逐事件**复现。以下会破坏复现性，属于实现禁令（E29 高）：

1. 在 `OnDraw`（或任何渲染路径）中调用随机函数；
2. 用真实时钟/内存地址参与决策；
3. 未初始化的变量参与判断；
4. 查询件位置却未先更新位置（FlexSim 用 `updatelocations()`；openflex 在推进行程时同步更新，无独立渲染位置）；
5. 依赖持久数据（标签、全局表）却不复位就重跑；
6. 任务执行器未设复位位置（openflex 在 reset 时统一把位置/载货清零，**这是有意的规范化**，见 runtime.md §5）。
