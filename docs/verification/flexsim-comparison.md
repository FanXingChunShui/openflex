# 与 FlexSim 的数值对照（方法与模板）

本页定义 OpenFlex 如何做数值对照，有两种模式：

1. **外部数据对照**：别人用 FlexSim（或教材/解析解）跑出来的数，和我们的数差多少、差在哪；
2. **两份 OpenFlex 报告对照**：升级版本、调参或换 seed 之后，逐对象/逐指标看差值（不需要 CSV）。

两种模式共用同一个工具 `scripts/compare-flexsim.sh`，输出格式与容差判定一致。

> **净室边界（重要）**
>
> OpenFlex 是净室实现：**没有运行过 FlexSim，也不会运行**。本仓库、CI 和文档都不包含
> FlexSim 的安装、许可证、二进制、`.fsm` 解析或反编译。本页描述的对照必须由**持有
> FlexSim 许可证的人**在外部完成：他们运行模型、导出数值、把 CSV 交给我们；我们只用
> 工具比较数字，再解释差异。
>
> 因此：
> - 我们没有 FlexSim 的数值结果时，本页的状态就是「待外部输入」，不能写成「已对照」。
> - 只有公开材料（教材、论文、解析解、官方文档示例）的数值可以直接引用，但必须注明来源，
>   并且**不能**说成「与 FlexSim 实测一致」。
> - 不推测 FlexSim 的内部实现细节；差异只从公开文档语义与统计误差解释。

## 1. 工具：`scripts/compare-flexsim.sh`

输入两份数据，输出差异表：

| 模式 | 输入 A | 输入 B |
|---|---|---|
| CSV（外部数据） | `--report report.json`（`openflex run model.ofm --report-dir <dir>` 生成） | `--csv flexsim.csv`（真实 FlexSim 运行 / 公开教材 / 解析解） |
| 双报告（无需 CSV） | `--report-a a/report.json` | `--report-b b/report.json` |

两种模式都支持 `--tolerance`、`--format markdown|csv|json|table`、`--fail-on-mismatch`、
`--allow-missing`；双报告模式另有 `--label-a`/`--label-b`，CSV 模式有 `--label`。

CSV 只需要三列，顺序固定、表头可选：

```csv
object,metric,value
Proc1,utilization,0.80
Queue1,avg_content,3.2
Queue1,avg_stay_time,16.0
```

- `object`：对象名，与 `.ofm` 里的 `name` 一致；整次运行的计数器用 `@model`（或留空）。
- `metric`：报告字段名，可用点号路径（`input_count.mean`、
  `objects.Queue1.utilization`）。
- `value`：数值；带 `%` 后缀时会除以 100（`80%` → `0.8`）。

用法与判定：

```sh
# 生成 OpenFlex 报告
./target/release/openflex run examples/golden/mm1.ofm --report-dir /tmp/of-report

# 默认 ±5%，markdown 表格输出
scripts/compare-flexsim.sh \
  --report /tmp/of-report/report.json \
  --csv docs/verification/flexsim-mm1-example.csv \
  --tolerance 0.05

# 机器可读 / 严格失败
scripts/compare-flexsim.sh --report r.json --csv f.csv --format csv
scripts/compare-flexsim.sh --report r.json --csv f.csv --fail-on-mismatch

# 对象名对不上时用 --label 只改标题；重命名请先改 CSV 的 object 列
scripts/compare-flexsim.sh --report r.json --csv f.csv --label "FlexSim 2024 (run 3)"

# 双报告模式：不需要 CSV，直接比两份 report.json
scripts/compare-flexsim.sh --report-a before/report.json --report-b after/report.json
```

输出列：A 值、B 值、绝对差（B - A）、相对差（以 A 为分母；CSV 模式 A 是外部值）、
是否在容差内。逐行标记 `**NO**` 的行必须在报告的「差异解释」里给出原因。

退出码：`0` 正常（差异只报告）；`1` 使用了 `--fail-on-mismatch` 且存在超容差行，或 CSV 里的
指标在报告中找不到（可用 `--allow-missing` 放宽）；`2` 参数/CSV 格式错误。

### 判定标准

| 项 | 规则 |
|---|---|
| 容差 | 默认相对容差 ±5%（`--tolerance 0.05`）。解析解可收紧到 ±1%，随机模型不建议小于 ±3% |
| 分母 | 相对差 = `(B - A) / A`；CSV 模式的 A 是外部值，双报告模式的 A 是基线报告。A 为 0 时退化为绝对差 ≤ 容差 |
| 复现次数 | OpenFlex 用 `--replications 5` 起；外部结果至少 5 次复现取均值，否则差异无法区分「模型不同」和「随机波动」 |
| 置信区间 | 表里同时给出 OpenFlex 的 `ci95`（`--format json`）。若 `|diff| < ci95`，差异在统计噪声内 |
| 停止时间 | 两边必须一致（含 warmup）。停止时间不同直接不比 |
| 单位 | 模型 `time_unit` 必须一致（秒/分钟/小时） |

## 2. 示例：公开 M/M/1 解析值（**不是** FlexSim）

`examples/golden/mm1.ofm` 是 M/M/1：到达间隔均值 5、加工时间均值 4（λ=0.2、μ=0.25、ρ=0.8），
停止时间 48000、warmup 1000、5 次复现。下表用**公开的 M/M/1 解析解**演示工具本身，
数据文件是 [flexsim-mm1-example.csv](flexsim-mm1-example.csv)。

解析值来源（任何排队论教材的 M/M/1 公式）：

```text
ρ  = λ/μ        = 0.2/0.25 = 0.8
Lq = ρ²/(1-ρ)   = 3.2
Wq = Lq/λ       = 16
W  = 1/(μ-λ)    = 20
```

实测（2026-11-14，release 构建，`--tolerance 0.05`）：

| object | metric | M/M/1 解析值 | OpenFlex | 绝对差 | 相对差 | 在 ±5% 内 |
|---|---|---|---|---|---|---|
| Proc1 | utilization | 0.8 | 0.790475 | -0.00952 | -1.19% | 是 |
| Queue1 | avg_content (Lq) | 3.2 | 3.05731 | -0.14269 | -4.46% | 是 |
| Queue1 | avg_stay_time (Wq) | 16 | 15.4243 | -0.57567 | -3.60% | 是 |
| Proc1 | avg_stay_time (服务时间) | 4 | 3.98665 | -0.01335 | -0.33% | 是 |
| Proc1 | throughput | 0.2 | 0.198268 | -0.00173 | -0.87% | 是 |

结论：5 项全部在 ±5% 内，方向一致（有限样本 + 有限停止时间导致略低于解析解），
与 `docs/VERIFICATION.md` 的黄金案例结论一致。**这张表证明工具可用，不证明与 FlexSim 一致。**

## 3. 两个 OpenFlex 报告直接对比（无需 CSV）

对照不一定需要外部数据：升级版本、调参、换 seed 之后，直接比两份 `report.json`
就能看出哪些对象/指标动了、动了多少。该模式遍历两边的 `objects.<name>.<metric>` 以及
整次运行的 `completed`/`created`，取聚合均值（`mean`）比较。

```sh
# 基线 vs 新版本（或 seed 1 vs seed 7）
./target/release/openflex run model.ofm --report-dir /tmp/before
./target/release/openflex run model.ofm --report-dir /tmp/after
scripts/compare-flexsim.sh \
  --report-a /tmp/before/report.json \
  --report-b /tmp/after/report.json \
  --label-a before --label-b after --tolerance 0.05

# 机器可读 / 严格失败
scripts/compare-flexsim.sh --report-a a.json --report-b b.json --format json
scripts/compare-flexsim.sh --report-a a.json --report-b b.json --fail-on-mismatch
```

只在一边出现的指标记为 missing（默认让退出码为 1；`--allow-missing` 放宽），
适合「新版本加了/删了某个统计字段」的场景。

实测（2026-11-14，release 构建，`mm1` 模型 seed 1 vs seed 7，各 5 次复现，`--tolerance 0.05`）：

| object | metric | seed 1 | seed 7 | 绝对差 | 相对差 | 在 ±5% 内 |
|---|---|---|---|---|---|---|
| @model | completed | 9316.8 | 9397.6 | +80.8 | +0.87% | 是 |
| Proc1 | utilization | 0.790475 | 0.801742 | +0.01127 | +1.43% | 是 |
| Queue1 | avg_content | 3.05731 | 3.09811 | +0.04079 | +1.33% | 是 |
| Queue1 | avg_stay_time | 15.4243 | 15.4752 | +0.05091 | +0.33% | 是 |
| Proc1 | idle_fraction | 0.209525 | 0.198258 | -0.01127 | -5.38% | **NO** |
| Queue1 | max_content | 28 | 26.2 | -1.8 | -6.43% | **NO** |

46 个指标里 44 个在 ±5% 内。两个「NO」都不是模型差异：
`idle_fraction = 1 - utilization`，绝对值只差 0.011；
`max_content` 是整数型的队列峰值，单次复现波动本来就大。这正说明应当
**先看绝对差与 CI95（`--format json` 里有 `ci95_a`/`ci95_b`），再看相对差**。

与 CLI 的 `openflex compare a.json b.json` 相比：`compare` 给对象级概览，
本工具给「逐指标 + 容差判定 + 可直接贴进报告的表格」，两者互补。

## 4. 真实 FlexSim 对照的采集步骤（由持证方执行）

> 采集方需要：FlexSim 许可证，以及一台能运行 `openflex` 的机器（或直接用 CI 产物）。
> 维护者**不接触** FlexSim 的安装、许可证或 `.fsm` 文件；只接收第 4.5 步产出的 CSV
> 与元数据。下面 8 步都在外部完成。

### 4.1 步骤

1. **模型对齐**：在 FlexSim 里重建同一拓扑（Source → Queue → Processor → Sink），参数逐项
   对照 `.ofm`：到达间隔分布、加工时间分布、容量、路由。记录 FlexSim 版本、建模时间、
   以及任何“FlexSim 做不到 / 只能近似”的项。
2. **对象名映射**：先填 [4.3](#43-对象名映射表模板) 的表。CSV 的 `object` 列一律写
   **OpenFlex 的名字**（大小写敏感），不要写 FlexSim 的内部 id。
3. **实验设置对齐**：Stop Time、Warmup、复现次数（≥5）、随机数种子策略
   （FlexSim 用自身 RNG；只比较均值与 CI，不比较逐事件序列）。
4. **导出数值**：每个对象导出利用率、平均含量、平均停留时间、输入/输出计数；
   整次运行导出完成数（`@model,completed`）。按 [4.2](#42-csv-模板) 的三列格式写。
5. **自检 CSV**：过一遍 [4.4](#44-导出前自检清单) 的清单，避免格式问题被误判成模型差异。
6. **提交**：把 CSV、FlexSim 版本与设置说明、对象映射表、模型截图/描述一起提交到 issue 或 PR。
   数据放 `docs/verification/data/`（数值可提交；**不要**提交 `.fsm` 二进制）。
   附上 `shasum -a 256 <csv>` 的结果，便于核对。
7. **运行对照**：

   ```sh
   # 1) 用同一模型生成 OpenFlex 报告
   openflex run model.ofm --report-dir /tmp/of-report

   # 2) 与外部 CSV 比较（默认 ±5%）
   scripts/compare-flexsim.sh --report /tmp/of-report/report.json \
     --csv docs/verification/data/flexsim-mm1-<date>.csv \
     --tolerance 0.05 --format markdown

   # 3) 机器可读/严格失败
   scripts/compare-flexsim.sh --report /tmp/of-report/report.json \
     --csv docs/verification/data/flexsim-mm1-<date>.csv --format json
   ```

8. **写记录**：把输出表贴进本页「对照记录」区，逐行解释超容差项；如果差异指向某个已知
   语义缺口（A 组），在记录里写上对应的 BACKLOG 编号。

### 4.2 CSV 模板

模板文件：[`flexsim-result-template.csv`](flexsim-result-template.csv)（可直接复制修改）。
`csv` 模式的 `--csv` 接受任意路径，建议放到 `docs/verification/data/`。

```csv
object,metric,value
Proc1,utilization,0.80
Queue1,avg_content,3.2
Queue1,avg_stay_time,16.0
Proc1,avg_stay_time,4.0
Proc1,throughput,0.20
```

| 列 | 必填 | 说明 |
|---|---|---|
| `object` | 是 | 对象名，与 `.ofm` 的 `name` 一致；整次运行计数器写 `@model` |
| `metric` | 是 | 报告字段：`utilization`、`avg_content`、`avg_stay_time`、`input_count`、`output_count`、`throughput`、`max_content`、`down_fraction` 等；也接受点号路径（`input_count.mean`） |
| `value` | 是 | 数值；百分比可写 `80%`（自动 ÷100）；不接受千分位逗号 |

补充约定：

- 以 `#` 开头的行会被忽略（模板顶部有注释），空行也忽略。
- 表头可省略（按 `object,metric,value` 顺序解析）；建议保留。
- 一个 CSV 只放一个模型、一组实验设置；不同 stop_time / 复现次数请分开文件。
- 缺失指标：CSV 有而报告没有 → 工具报 missing 并返回 1；加 `--allow-missing` 可放宽。

### 4.3 对象名映射表模板

| FlexSim 对象 | FlexSim 类型 | OpenFlex 对象 | OpenFlex 类型 | 备注 |
|---|---|---|---|---|
| Source1 | Source | Source1 | source | 到达分布一致（exponential mean 5） |
| Queue1 | Queue | Queue1 | queue | 容量一致 |
| Processor1 | Processor | Proc1 | processor | **名字不同**，CSV 里写 `Proc1` |
| Sink1 | Sink | Sink1 | sink | |

> 名字不一致时，**改 CSV 的 object 列**（不要改 `.ofm`，否则报告里也对不上）。

### 4.4 导出前自检清单

- [ ] Stop Time / Warmup 与 `.ofm` 的 `run` 完全一致（含单位）
- [ ] 复现次数 ≥ 5，CSV 里是**均值**，不是某一次复现
- [ ] 对象名与 `.ofm` 的 `name` 完全一致（大小写敏感）
- [ ] 数值用小数点（`3.2`），不用千分位逗号（`3,200`）
- [ ] 百分比可以写 `80%`，也可以写 `0.8`（不要混用）
- [ ] 指标单位与 `time_unit` 一致（秒/分钟/小时）
- [ ] 文件是 UTF-8，表头是 `object,metric,value`
- [ ] 附上 `shasum -a 256 <csv>` 与 FlexSim 版本、复现次数、种子策略

### 4.5 如何从 FlexSim 导出对象指标 CSV

> **净室声明**：我们没有运行 FlexSim，也没有它的安装或文档副本。下面是最通用的
> “从仿真结果里取对象指标”流程，基于公开文档与常见 UI 惯例；**具体菜单名以你使用的
> FlexSim 版本为准**。如果你只能提供 Excel，也完全可以——按 4.2 的三列格式另存为 CSV 即可。

**方式 A：单次仿真的 Statistics 面板（适合小模型、快速核对）**

1. 用与 `.ofm` 相同的 Stop Time / Warmup 跑一次仿真。
2. 打开对象的统计视图（通常在 Statistics → 选中对象 → 查看 utilization / content /
   stay time / input / output 等字段）。
3. 逐对象记录需要的指标。字段名可能和 OpenFlex 不同，**以数值含义对应**：
   `utilization` ↔ 利用率、`avg_content` ↔ 平均在制品、`avg_stay_time` ↔ 平均停留时间、
   `input_count` / `output_count` ↔ 输入/输出计数。
4. 写进 4.2 的三列 CSV；对象名用 OpenFlex 的名字（见 4.3 映射表）。

**方式 B：Experimenter 导出（推荐，含多次复现的均值）**

1. 在 Experimenter 里建一个 Scenario（参数 + Stop Time + Warmup），
   Replications 设为 ≥ 5，与 OpenFlex 的 `run.replications` 对应。
2. 跑完后打开结果表（Performance Measures / 每个对象的统计）。
3. 用结果表的导出功能（通常是 Export → CSV/Excel，或右键 “Export Table”）导出。
4. 把宽表转成三列（见下面的转换脚本），只保留 `object,metric,value`。

**方式 C：自定义统计 / 脚本导出（高级）**

如果模型里用了自定义统计或脚本，直接把需要的量写成 CSV 三列即可；
不要在 CSV 里混入图表、合并单元格或标题行。

**宽表 → 三列（awk，示例）**

FlexSim 导出的宽表通常长这样（列名随版本变化）：

```csv
Object,Utilization,AvgContent,AvgStayTime,Input,Output
Processor1,0.81,0.02,4.02,9336,9335
Queue1,0.00,2.91,15.10,9336,9336
```

转成对照工具要的格式：

```sh
awk -F, 'NR==1 { next }
  { printf "%s,utilization,%s\n%s,avg_content,%s\n%s,avg_stay_time,%s\n%s,input_count,%s\n%s,output_count,%s\n",
      $1,$2, $1,$3, $1,$4, $1,$5, $1,$6 }' flexsim-export.csv > flexsim.csv
```

或者用 python（可读性更好）：

```python
import csv
with open("flexsim-export.csv", newline="") as source, open("flexsim.csv", "w", newline="") as target:
    reader = csv.DictReader(source)
    writer = csv.writer(target)
    writer.writerow(["object", "metric", "value"])
    for row in reader:
        for metric, column in [
            ("utilization", "Utilization"),
            ("avg_content", "AvgContent"),
            ("avg_stay_time", "AvgStayTime"),
            ("input_count", "Input"),
            ("output_count", "Output"),
        ]:
            if row.get(column):
                writer.writerow([row["Object"], metric, row[column]])
```

**导出后自检**（与 4.4 合并使用）

- 每个对象至少 1 行，指标名与 OpenFlex 报告字段一致（`utilization`、`avg_content` 等）。
- 数值是**复现均值**，不是某一次复现；百分比要么 `0.81` 要么 `81%`，不要混。
- 三列 CSV 里不要有合计行、空行（空行会被忽略，`#` 注释行也会被忽略）。

**示例数据**：`docs/verification/flexsim-export-example.csv` 是一份“可直接喂给
`compare-flexsim.sh`”的三列示例（数值来自公开 M/M/1 解析解，**不是** FlexSim 运行结果），
用来说明格式与对照流程：

```sh
scripts/compare-flexsim.sh --report /tmp/of-report/report.json \
  --csv docs/verification/flexsim-export-example.csv --tolerance 0.05
```

### 4.6 常见导出错误

| 现象 | 原因 | 处理 |
|---|---|---|
| `missing metric on one side` | 对象名不一致 / 指标名拼错 | 对照 4.3 的映射表改 CSV |
| 相对差 100% 以上 | 单位不同（每小时 vs 每时间单位） | 统一单位后再比 |
| 结果全为 0 | 导出的是“瞬时值”而非“均值” | 导出复现后的统计量 |
| `non-numeric value` | 千分位逗号、`%` 前后带空格、空单元格 | 改成纯数字或 `xx%` |
| 差异集中在 `avg_content` | FlexSim 可能含/不含在途量 | 明确统计口径，记录在差异解释里 |

### 常见差异来源（按可能性排序）

1. **warmup 与停止时间不同**：最容易被忽略，影响最大。
2. **统计噪声**：单次复现的差异可达 5% 以上；必须先看 CI95。
3. **对象语义不同**：例如 FlexSim 的在途量计入容量、Separator 的 split 模式、
   Processor 的 setup 时间（见 `docs/BACKLOG.md` 的 A 组），这些是**已知**的实现差异。
4. **统计量定义不同**：FlexSim 的 throughput 可能按「每小时」而非「每时间单位」，
   平均含量可能含/不含在途项。对照前必须确认定义。
5. **路由/触发器差异**：条件路由、标签、`SendToPort` 的 -1（未实现，见 A4）。

## 5. 对照记录模板

每完成一次真实对照，在下面追加一行，并把差异表贴在后面的小节里。

| 日期 | FlexSim 版本 | 模型 | 复现次数 | 容差 | 指标数 | 在容差内 | 超容差项 | 数据文件 | 执行人 |
|---|---|---|---|---|---|---|---|---|---|
| 2026-11-14 | —（解析解） | `mm1` | 5 | ±5% | 5 | 5 | 0 | [flexsim-mm1-example.csv](flexsim-mm1-example.csv) | OpenFlex 维护者 |
| 2026-11-14 | —（两个 OpenFlex 报告，seed 1 vs seed 7） | `mm1` | 5 | ±5% | 46 | 44 | 2（`idle_fraction`、`max_content`） | — | OpenFlex 维护者 |
| 待定 | FlexSim 2024/2025 |  |  |  |  |  |  |  | 外部持证方 |

### 记录 A：M/M/1 解析值对照（示例，非 FlexSim）

- 日期：2026-11-14
- 外部数据：M/M/1 解析解（λ=0.2、μ=0.25），非 FlexSim
- 命令：`scripts/compare-flexsim.sh --report /tmp/of-report/report.json --csv docs/verification/flexsim-mm1-example.csv --tolerance 0.05`
- 结果：5/5 在 ±5% 内（见第 2 节表格）
- 解释：差异来自有限停止时间与样本量，方向与理论一致；无模型语义差异。

### 记录 B：真实 FlexSim 对照（待填）

```text
- 日期 / 执行人：
- FlexSim 版本 / 运行环境（OS、CPU）：
- OpenFlex 版本 / 运行环境：
- 模型：.ofm 路径 + 对象映射表（4.3）
- 实验设置：stop_time / warmup / replications / seed 策略 / time_unit
- 数据文件：docs/verification/data/flexsim-<model>-<date>.csv
  SHA-256：
- 命令与输出（贴 diff 表）：
- 结论：在容差内 n/m；超容差项逐条解释（噪声 / 语义 / 单位）：
- 是否触发 BACKLOG 条目（例如 A5/A6/A7）：
- 后续动作（需要谁做什么）：
```
