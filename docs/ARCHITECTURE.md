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

- 故障：Processor 可配置 `failure: { mtbf, mttr }`，按分布循环进入 `Down`，修复后恢复并执行
  `on_repair`。
- 班次：对象可配置 `shifts`，停机时进入 `OffShift`，不接收新项，恢复后继续（见对象参数说明）。
- 实验：模型内 `experiments[]` 声明“场景（覆盖参数）× 复现次数”，`openflex experiment` 逐场景运行并
  输出聚合与 95% 置信区间。

### 随机性与确定性

- 单一 `StdRng`（ChaCha12），按 seed 初始化。
- 复现 `i` 的 seed 由 SplitMix64 从基础 seed 派生，因此复现之间独立，且整组结果可由基础 seed 重放。
- 同一模型、同一 seed、同一版本 → 逐位相同的结果。

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
- 未实现 Fluid、人员行走（A*）、AGV 交通控制、OptQuest 优化。
