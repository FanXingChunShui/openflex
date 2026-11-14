# 与 FlexSim 的数值对照（方法与模板）

本页定义 OpenFlex 如何接受**外部提供的**仿真结果并与自身结果做数值对照。
它回答一个问题：*别人用 FlexSim 跑出来的数，和我们跑出来的数差多少，差在哪。*

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

| 输入 | 来源 |
|---|---|
| `--report report.json` | `openflex run model.ofm --report-dir <dir>` 生成 |
| `--csv flexsim.csv` | 外部（真实 FlexSim 运行 / 公开教材 / 解析解） |

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
```

输出列：external 值、OpenFlex 值、绝对差、相对差（以外部值为分母）、是否在容差内。
逐行标记 `**NO**` 的行必须在报告的「差异解释」里给出原因。

退出码：`0` 正常（差异只报告）；`1` 使用了 `--fail-on-mismatch` 且存在超容差行，或 CSV 里的
指标在报告中找不到（可用 `--allow-missing` 放宽）；`2` 参数/CSV 格式错误。

### 判定标准

| 项 | 规则 |
|---|---|
| 容差 | 默认相对容差 ±5%（`--tolerance 0.05`）。解析解可收紧到 ±1%，随机模型不建议小于 ±3% |
| 分母 | 相对差 = `(openflex - external) / external`；external 为 0 时退化为绝对差 ≤ 容差 |
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

## 3. 真实 FlexSim 对照的采集步骤（由持证方执行）

1. **模型对齐**：在 FlexSim 里重建同一拓扑（Source → Queue → Processor → Sink），参数逐项
   对照 `.ofm`：到达间隔分布、加工时间分布、容量、路由。记录 FlexSim 版本与建模时间。
2. **实验设置对齐**：Stop Time = 48000、Warmup = 1000、复现次数 = 5、
   随机数种子策略（FlexSim 用自身 RNG；对照时只比较均值与 CI，不比较逐事件序列）。
3. **导出数值**：每个对象导出利用率、平均含量、平均停留时间、输入/输出计数；
   整次运行导出完成数。列名遵循上面的三列 CSV 格式，**对象名与 `.ofm` 一致**。
4. **交给 OpenFlex 侧**：把 CSV 与 FlexSim 版本、模型截图/描述一起提交到 issue 或 PR，
   放进 `docs/verification/data/`（数值数据可提交；`.fsm` 文件不要提交）。
5. **运行对照**：
   ```sh
   scripts/compare-flexsim.sh --report /tmp/of-report/report.json \
     --csv docs/verification/data/flexsim-mm1-<date>.csv --tolerance 0.05 --format markdown
   ```
6. **写记录**：把输出表贴进本页「对照记录」区，逐行解释超容差项。

### 常见差异来源（按可能性排序）

1. **warmup 与停止时间不同**：最容易被忽略，影响最大。
2. **统计噪声**：单次复现的差异可达 5% 以上；必须先看 CI95。
3. **对象语义不同**：例如 FlexSim 的在途量计入容量、Separator 的 split 模式、
   Processor 的 setup 时间（见 `docs/BACKLOG.md` 的 A 组），这些是**已知**的实现差异。
4. **统计量定义不同**：FlexSim 的 throughput 可能按「每小时」而非「每时间单位」，
   平均含量可能含/不含在途项。对照前必须确认定义。
5. **路由/触发器差异**：条件路由、标签、`SendToPort` 的 -1（未实现，见 A4）。

## 4. 对照记录模板

每完成一次真实对照，在下面追加一行，并把差异表贴在后面的小节里。

| 日期 | FlexSim 版本 | 模型 | 复现次数 | 容差 | 指标数 | 在容差内 | 超容差项 | 数据文件 | 执行人 |
|---|---|---|---|---|---|---|---|---|---|
| 2026-11-14 | —（解析解） | `mm1` | 5 | ±5% | 5 | 5 | 0 | [flexsim-mm1-example.csv](flexsim-mm1-example.csv) | OpenFlex 维护者 |
| 待定 | FlexSim 2024/2025 |  |  |  |  |  |  |  | 外部持证方 |

### 记录 A：M/M/1 解析值对照（示例，非 FlexSim）

- 日期：2026-11-14
- 外部数据：M/M/1 解析解（λ=0.2、μ=0.25），非 FlexSim
- 命令：`scripts/compare-flexsim.sh --report /tmp/of-report/report.json --csv docs/verification/flexsim-mm1-example.csv --tolerance 0.05`
- 结果：5/5 在 ±5% 内（见第 2 节表格）
- 解释：差异来自有限停止时间与样本量，方向与理论一致；无模型语义差异。

### 记录 B：真实 FlexSim 对照（待填）

```text
- 日期：
- FlexSim 版本 / 运行环境：
- 模型（.ofm 路径 + 对象映射表）：
- 实验设置：stop_time / warmup / replications / seed 策略
- 数据文件：docs/verification/data/flexsim-<model>-<date>.csv（SHA-256：）
- 命令与输出：
- 结论：在容差内 n/m；超容差项与解释：
- 是否触发 BACKLOG 条目（例如 A5/A6/A7）：
```
