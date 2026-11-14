# openflex 运行规格：时间、运行控制、随机性、模型文件与命令行

> 证据编号见 [evidence.md](../research/evidence.md)。

## 1. 时间与单位

| 项 | FlexSim 行为 | openflex 规定 | 证据 |
|---|---|---|---|
| 模型时间单位 | 模型有 `Time Units` 设置，`Model Settings` 只描述为"displays the time units of the model"，**未列出可选取值**；命令参考提供 `hourtomodelunit/minutetomodelunit/daytomodelunit/getmodelunit` 等换算，说明单位可配置 | `time_unit ∈ {seconds, minutes, hours}`，默认 `seconds`；**所有内部计算一律用该单位下的浮点模型时间**，不做隐式换算 | E26 高 / 枚举值 低 |
| 长度单位 | 独立设置（`Length Units`）；速度以"长度单位/时间单位"表示 | v1 只用于任务执行器行程；默认 `meters` | E26 高 |
| 时钟起点 | 模拟时钟从 **0** 开始；`Start Time` 可把真实日期时间映射到 0 时刻 | `run.start_time` 仅影响 `report` 的日期显示，不影响事件时间 | E28 高 |
| 时间格式 | `Time Format` / `Date Format` 各有格式符表，**两者不可混用** | 报表用 `HH:mm:ss` / `yyyy-MM-dd` | E26 高 |
| 数值精度 | `Decimal Precision` 控制界面可编辑值的精度 | 内部 double，输出按 `precision`（默认 6 位有效） | E26 高 |

> **停止判定规则（重要）**：模型"不一定会停在指定时刻"，而是停在**首个 ≥ 该时刻的系统检查点**；如果事件表提前空了则更早停（E32 高）。openflex 实现：主循环 `while let Some(ev) = pop() { if ev.time >= stop_time { 记录并停在 min(ev.time, ...) } }` —— 语义上采用"处理完 < stop_time 的全部事件后停在 stop_time 或更早（无事件时）"，并在报告中区分 `stopped_at` 与 `stop_time`。

## 2. 运行控制命令

FlexSim 控制栏（E28 高）与对应脚本命令（E32 高）的语义映射：

| 操作 | 快捷键 | 命令 | 语义（须精确复刻） | openflex CLI |
|---|---|---|---|---|
| Reset | Ctrl+← | `reset`（重载模型） | 时钟归零/回到起始日期；清空模型中的件；图标复位；**丢弃全部待处理事件** | `openflex reset <model>`（隐含在 run 前执行一次） |
| Run | Ctrl+Space | `go([fast])` | 从当前位置继续推进事件；**不清空、不重置**；事件表空了就停 | `openflex run <model>` |
| Stop / Pause | Ctrl+Space | `stop([fast])` | **暂停**语义：时钟停，**待处理事件保留**；再次 `go` 从原时刻续跑 | `openflex run --until <t>` / 交互模式 `s` |
| Step | Ctrl+→ | `step()` | 处理**恰好一个**事件后停 | `openflex run --steps 1` |
| Skip | Ctrl+Shift+→ | `skip()` | 处理一个事件，然后把**同一模型时刻**的其余事件全部处理完才停 | `openflex run --skip-to-next-time` |
| Fast Forward | Ctrl+Shift+↑ | `fastforward()` | 保存当前速度 → 设最大速度 → 跑到下一个启用的 stop time → 恢复速度与刷新 | `openflex run --fast-forward` |
| 结束复现 | — | `endreplication(1)` | 提前结束当前复现（后续复现照常） | `openflex run --stop-when <expr>` |
| 查询状态 | — | `getrunstate()` / `getrunstate(1)` | 返回 1=运行中；带参时返回"是否有事件正在执行" | `--json` 输出中 `state: running/paused/done` |
| 结算统计 | — | `updatestates()` | 把对象状态与 content 统计结算到当前时刻（**Stop 按钮内部先调用它**，否则最后一整段状态的时长不会计入报表） | 停止时自动执行 |

**多个停止时间**：可配置任意多个 stop time，每个可独立启用；未启用或已越过最后一个则无限运行直到手动停止（E28 高）。`stoptime(num, stopNum)` 按序号设置；`stoptime(-1)` 返回当前值；`stoptime(0, -1)` 重置为单个 disabled 的 0 时刻停时（E32 高）。

openflex 的 `run` 块：
```json
"run": {"stop_time": 480.0, "warmup": 0.0, "seed": 1, "replications": 1}
```
扩展（提案，见 flexsim-to-openflex §8）：`"stop_times": [{"time": 480, "enabled": true}, ...]`。

## 3. 预热（Warmup）

- 语义：**到达预热时刻后，把已收集的统计清零**，因为预热期数据会失真（E27 高）。
- 目的：让系统进入稳态（例：24×7 运行的医院模型需要先跑到正常负荷）。
- 实现要点（必须写进回归测试）：
  1. 预热边界**不影响**事件与实体本身（在制品的件继续存在，队列内容不清空）；
  2. 清零对象：`input`/`output` 计数、content 的 min/max/avg 累积、staytime 样本集、**每个状态的累计时长**、行程统计；
  3. 预热期间已完成的件的停留时间样本**不应**进入统计（因为样本集被清空）；
  4. 边界落在两个事件之间时，取"最后一个 ≤ warmup 的事件处理完之后"作为清零点（E27 高：预热是时间点语义）。
- 与实验器交互：**Warmup Time 是 Scenario 的一部分**（可以随场景变化）（E35 高）。

## 4. 运行速度

| 项 | 值 | 证据 |
|---|---|---|
| 默认速度 | **4 模型时间单位 / 真实秒**（默认 "4 simulation seconds per real-time second"） | E27 高 |
| `1.00` | 1 显示单位 / 真实秒（等价实时，若显示单位=秒） | E27/E28 高 |
| `Maximum` | 最快（不限制，通常关闭视图刷新） | E28 高 |
| `Custom` | 用户输入任意值 | E28 高 |
| `runspeed(num)` | 同上，程序方式设置；`precisionMode` 可选低/高精度（高精度用于仿真加速/仿真器对接；**每次 reset 会重置为低精度**） | E32 高 |
| Display Mode | 时钟可显示"模型时间单位"/"日期时间文本"/两者；速度表也可按"某单位每真实秒"显示 | E28 高 |

openflex v1：`--speed <n>` 只影响**交互式/实时**模式；批处理/CLI 运行始终以最大速度执行（因为无渲染）。这一差异必须写在 CLI 文档里（`--speed` 在 `--batch` 下被忽略并给出 warning）。

## 5. 随机性与可复现

### 5.1 FlexSim 的机制

- 随机数由**流（stream）**组织：`Model Settings → Random Number Streams` 设置"reset 时要初始化的流数量（0..N）"（E26 高）；每种分布都带 `Stream` 参数选择用哪条流（E30 高）。
- **`Repeat Random Streams`**（Statistics 菜单）勾选后，每次 reset+run 产生**相同**的随机序列 ⇒ 结果可复现（E29 高）。
- **Replication 编号决定随机序列**：第 1 次与第 2 次复现得到不同随机数（E35 高）。
- 官方承诺："若不改变模型，每次运行结果完全相同，即使设置了随机属性"（E27 高）。

### 5.2 openflex 规定

- **算法**：PCG64（或 SplitMix64 播种 + xoshiro256**），**净室自选**，不与 FlexSim 数值对齐（理由：文档未公开其生成器，禁止逆向）。
- **流的划分**：`stream = f(seed, replication, stream_index)`，其中 `stream_index` 由分布参数的可选 `stream` 字段给出（默认 0）。同一模型 + 同 seed + 同 replication ⇒ 逐事件一致。
- **复现**：`replications = n` 时，第 k 次复现（k 从 1 开始）使用 `seed_k = mix(seed, k)`；结果报告给出每次复现的指标与汇总（均值/标准差/区间，见 statistics.md）。
- **确定性要求**：事件表必须用稳定全序（time, priority, insertion_seq），HashMap 迭代顺序**不得**进入任何决策路径。这是 openflex 与 FlexSim"同模型必同结果"承诺对应的实现约束。

## 6. 模型文件概念

FlexSim 侧（E26/E35 高）：

| 概念 | 内容 |
|---|---|
| 模型文件 | 扩展名 `.fsm`（另见工程 `.fsp`、视图 `.fsv`、会话 `.fss`；这些来自 `filebrowse` 的过滤器字符串）；**内部格式未公开**，净室**不做**逆向 |
| 嵌入媒体 | `Embed media with model` 可把 3D/图片嵌进模型文件；`Export Embedded Media` 可导出。注：若 3D 形状引用外部文件，只有主文件与被引用纹理会被嵌入 |
| 加密 | `Encrypt Model` 可设密码；**密码不可恢复** |
| 自动保存 | `[modelname]_autosave.fsm` 与模型同目录；可按模型关闭 |
| 实验结果库 | Experimenter 首次跑 Job 时创建**独立结果文件**（默认与模型同名同目录）；每个 Task 完成即写入、跨 Job 持久、重跑前查重跳过；**不同模型的结果绝不能混用** |
| 命令行 | `flexsim.exe myfile.fsm -nrofops 8`；模型内用 `commandlineparam("nrofops")` 读取（`-xxx` 或 `/xxx` 均可） |

openflex 侧（**自定义 JSON，不做格式兼容**）：

```json
{
  "format": "openflex-model", "version": 1, "name": "mm1",
  "time_unit": "seconds",
  "run": {"stop_time": 480.0, "warmup": 0.0, "seed": 1, "replications": 1},
  "objects": [ ... ], "connections": [ ... ]
}
```

- `format` 固定 `"openflex-model"`，`version` 目前 `1`；读取时**未知字段必须报错**（`--strict` 默认开启），以免模型静默变形。
- 顶层只允许：`format/version/name/time_unit/run/objects/connections`（+ 提案扩展 `resource_refs/metrics/shifts/breakdowns/user_commands/expressions`）。
- 结果输出建议：`<model>.results.json`（每复现一条 + 汇总），不写入模型文件。

## 7. 命令行（openflex CLI 规格建议）

对齐 FlexSim 的"可命令行启动 + 模型内读参数"能力（E32 高），并满足跨平台安装（macOS/Windows/Linux 单二进制）：

```
openflex run   <model.ofm> [--until T] [--replications N] [--seed S] [--warmup W]
                           [--steps N|--skip-to-next-time|--fast-forward]
                           [--out results.json] [--format json|table|csv] [--strict|--no-strict]
openflex check <model.ofm>          # 只校验模型（schema/引用/可达性/参数区间），不运行
openflex report <results.json>      # 从结果文件渲染报告
openflex explain <model.ofm>        # 打印对象/连接/策略摘要（人类可读）
openflex param <model.ofm> k=v ...  # 命令行覆盖参数（等价 FlexSim 的 Scenario 注入）
```

- `openflex param` 对应 FlexSim 的 "model parameter + Experimenter 场景"（参数化 + 批量跑）；
- `--seed` 等价 `run.seed`，`--replications` 等价 `run.replications`；
- **必须有 `openflex check`**：净室重建最怕模型静默走偏（例：`connections` 顺序被改写、`SendToPort` 语义误实现），校验器要能发现"不可达对象""端口数与策略不匹配"等问题。
- 退出码约定：`0` 成功；`2` 模型校验失败；`3` 运行期错误（如容量非法）；`4` 内部错误。

## 8. 与 FlexSim 的**有意差异**（必须在文档中声明）

| 差异 | FlexSim | openflex | 理由 |
|---|---|---|---|
| 渲染 | 3D 实时视图，速度影响绘制 | 无渲染（v1） | 净室最小核心；`--speed` 在批处理下无意义 |
| 模型文件 | `.fsm` 私有二进制 | JSON 文本 | 可 diff、可校验；不逆向私有格式 |
| 随机数 | 私有生成器 + 流 | PCG64 + 派生流 | 文档未公开算法，禁止逆向；数值不与 FlexSim 逐位对齐 |
| 时间单位枚举 | 未在文档中枚举 | 固定三值 | 见 §1 |
| 图形化属性面板 | 有 | 无（用 JSON + CLI） | 核心功能优先 |
| Process Flow / Dashboard | 有 | v1 不做（`metrics` 表达式替代） | 范围控制 |
| 加速/仿真器对接 | `runspeed` 高精度模式 | 无 | 无 GUI 时钟 |
