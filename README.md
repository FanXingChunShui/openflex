# OpenFlex

**净室重建的离散事件仿真引擎 —— 用 Rust 实现 FlexSim 的核心建模语义，跨平台、CLI 优先。**

OpenFlex 是一个独立的、从零实现的离散事件仿真（DES）系统。它用公开文档描述的 FlexSim 行为作为规格，
用 Rust 重新实现建模对象、统计分布、对象逻辑、路由规则、故障维修和实验器。模型是纯文本 JSON，
运行结果可复现（给定 seed 完全确定），报告可导出为 JSON / CSV / Markdown，并可在 macOS、Windows、
Linux 上安装使用。

> **最新发布**：[v0.1.0](https://github.com/FanXingChunShui/openflex/releases/tag/v0.1.0) —— macOS universal、
> Windows x64/arm64、Linux x86_64/aarch64 压缩包，以及 Windows MSI、deb、rpm 安装包，附 `SHA256SUMS.txt`。

> **净室声明**：本项目没有使用、复制或反编译 FlexSim 的任何源代码或二进制。所有行为规格来自公开的
> 用户手册、帮助文档和公开教材中访客可见的内容，证据记录在 [docs/research/evidence.md](docs/research/evidence.md)。

## 特性一览

| 能力 | 状态 |
|---|---|
| 对象库：Source、Queue、Processor、Sink、Combiner、Separator、Conveyor、Merge、Split、Transporter、Dispatcher、Resource、Rack、BasicFR、BasicTE | ✅ |
| 20 种统计分布（常数/均匀/正态/指数/三角/Gamma/Weibull/对数正态/Erlang/Beta/整数均匀/经验/Pareto/LogLogistic/Rayleigh/二项/泊松/几何/负二项/Bernoulli） | ✅ |
| 对象触发器（OnCreate / OnEntry / OnExit / OnReceive / OnMessage / OnBreakdown / OnRepair） | ✅ |
| 路由规则（first available / round robin / random / 百分比 / 条件表达式 / 按 item type / 按标签） | ✅ |
| 故障与维修（MTBF / MTTR）、班次与计划停机 | ✅ |
| 多复现实验、场景覆盖（含 Stop/Warmup）、95% 置信区间（t 分布）、并行复现 | ✅ |
| 报告：终端表格、JSON、CSV、Markdown | ✅ |
| CLI：validate / run / experiment / report / compare / import / layout / serve / schema / completions | ✅ |
| macOS / Windows / Linux 安装包与安装脚本 | ✅ |
| 3D 图形界面（FlexSim 的 3D 视图） | ⛔ 规划中；当前提供无界面 CLI 与结构化输出 |

完整的兼容矩阵见 [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md)。

## 快速开始

### 从源码构建

```sh
git clone <this repo> && cd openflex
cargo build --release -p openflex-cli
./target/release/openflex --help
```

要求 Rust 1.97 或更新。

### 安装

```sh
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/openflex/openflex/main/packaging/install.sh | sh

# Windows (PowerShell)
irm https://raw.githubusercontent.com/openflex/openflex/main/packaging/install.ps1 | iex
```

其他方式（Homebrew、deb/rpm、MSI、Docker）见 [docs/install.md](docs/install.md)。

### 第一个模型

```sh
openflex new demo --template mm1
cd demo
openflex validate mm1.ofm
openflex run mm1.ofm --replications 10
```

输出示例：

```
OpenFlex report: mm1
stop time 48000.00 s, warm-up 1000.00 s, replications 5, base seed 1

Object                        In       Out    AvgWIP    MaxWIP   Stay (s) Util (%)
--------------------------------------------------------------------------------
Proc1                     9400.4    9400.6      1.00       1.0       3.97     79.4
Queue1                    9399.6    9400.4      3.26      28.6      14.81      0.0
Sink1                     9400.6    9400.6      0.00       0.0       0.00      0.0
Source1                      0.0    9399.6      0.00       0.0       0.00      0.0
--------------------------------------------------------------------------------
Items created             9399.6  (95% CI ±53.6)
Items completed           9400.6  (95% CI ±52.5)
Items in system              1.8  (95% CI ±1.6)
```

## 模型格式

模型是 JSON 文件（推荐扩展名 `.ofm`）：

```json
{
  "format": "openflex-model",
  "version": 1,
  "name": "mm1",
  "time_unit": "minutes",
  "run": { "stop_time": 10000.0, "warmup": 1000.0, "seed": 42, "replications": 10 },
  "objects": [
    { "name": "Arrivals", "kind": "source", "pos": [0, 0], "params": {
        "interarrival": { "distribution": "exponential", "mean": 5.0 } } },
    { "name": "Queue", "kind": "queue", "pos": [5, 0], "params": { "max_content": 10000 } },
    { "name": "Server", "kind": "processor", "pos": [10, 0], "params": {
        "process_time": { "distribution": "exponential", "mean": 4.0 } } },
    { "name": "Exit", "kind": "sink", "pos": [15, 0], "params": {} }
  ],
  "connections": [
    { "from": "Arrivals", "to": "Queue" },
    { "from": "Queue", "to": "Server" },
    { "from": "Server", "to": "Exit" }
  ]
}
```

- `kind` 决定对象类型；`params` 的字段见 `openflex objects describe <kind>`。
- `time_unit` 只影响显示；模型内所有时间数值使用同一单位。
- `run.replications` 是默认复现次数，`openflex run --replications N` 可覆盖。
- 实验（场景 × 复现）用 `experiments` 数组声明，见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)。

## CLI 速查

```sh
openflex validate model.ofm                  # 校验，退出码 2 表示失败
openflex run model.ofm --stop-time 480 --seed 7 --replications 10
openflex run model.ofm --set Server.process_time.mean=4.5 --format json
openflex run model.ofm --report-dir out/     # 写出 report.json / report.csv / report.md / replications.csv
openflex run model.ofm --xlsx out.xlsx       # 导出 Excel（Summary / Objects / Replications）
openflex run model.ofm --parallel 4          # 并行复现（结果与串行逐位一致）
openflex run model.ofm --progress            # stderr 显示复现进度
openflex run model.ofm --trace events.jsonl  # 逐事件 JSONL（多复现写 .repN.jsonl）
openflex run model.ofm --until 500           # --stop-time 的别名
openflex experiment model.ofm --format json  # 运行模型里声明的实验
openflex experiment model.ofm --report-dir exp --trace  # 每个场景/复现的 trace
openflex report out/report.json              # 读回报告并重新渲染
openflex compare a.json b.json               # A/B 对比两个报告
openflex import --csv arrivals.csv --out model.ofm   # 从到达表生成模型
openflex import --objects objects.csv --connections connections.csv --out model.ofm  # 从对象/连接表生成
openflex layout model.ofm --svg out.svg      # 导出 2D 布局图
openflex schema --out schema.json            # 导出模型 JSON Schema
openflex serve --port 8080                   # 本地 HTTP：/health、/objects
openflex completions zsh                     # shell 补全脚本
openflex objects list                        # 列出所有对象类型
openflex objects describe processor          # 查看对象参数
openflex distributions                       # 列出 20 种统计分布
openflex new mymodel --template line         # 生成模板模型

# 性能基准（release）
cargo run --release -p openflex-cli --example bench_mm1 -- --stop 100000
cargo run --release -p openflex-cli --example bench_mm1 -- --model examples/mm1.ofm
```

- 退出码：`0` 成功，`1` 运行错误，`2` 模型校验失败。
- `--format json` 时错误也以 JSON 输出（`{"error":{"code","message","path"}}`），可用 `--error-format json|text` 单独控制。
- `OPENFLEX_LANG=zh` 切换中文报告表头与帮助文案。

## FlexSim 对应关系

| FlexSim | OpenFlex | 说明 |
|---|---|---|
| Source | `kind: "source"` | 到达间隔、批量、最大到达数、停止时间 |
| Queue | `kind: "queue"` | FIFO/LIFO/优先级/按标签排序、批量释放 |
| Processor | `kind: "processor"` | setup + process、容量、故障维修、操作员 |
| Sink | `kind: "sink"` | 统计吞吐与停留时间 |
| Combiner / Separator | `kind: "combiner"` / `"separator"` | pack / join、拆分 |
| Conveyor | `kind: "conveyor"` | 速度、长度、累积/非累积 |
| Merge / Split | `kind: "merge"` / `"split"` | 多路合流 / 分流 |
| Transporter / Dispatcher | `kind: "transporter"` / `"dispatcher"` | 任务执行器（简化版） |
| Resource / Operator | `kind: "resource"` | 资源池与停机窗口 |
| Rack | `kind: "rack"` | 货架存储 |
| 对象触发器 | `params.triggers` | 结构化动作，不执行任意代码 |
| Send To Port | `connections[].strategy` | 与 FlexSim 的路由规则对应 |
| Experimenter | `experiments[]` + `openflex experiment` | 场景 × 复现、置信区间 |
| 3D 视图 / 树视图 / GUI | ⛔ 暂缺 | CLI 优先；见路线图 |

## 项目结构

```
crates/openflex-model     模型 schema、分布、表达式、校验
crates/openflex-core      事件调度引擎、对象运行时、统计与实验
crates/openflex-report    报告渲染（table/json/csv/markdown）
crates/openflex-cli       openflex 命令行
packaging/                安装脚本、包管理器配置、Dockerfile
docs/                     架构、兼容性、安装、规格与净室证据
examples/                 示例模型（按对象与场景分类）
```

## 开发

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all
```

未完成功能与迭代计划见 [docs/BACKLOG.md](docs/BACKLOG.md)。设计说明见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)，净室证据见
[docs/research/evidence.md](docs/research/evidence.md)，行为规格见 [docs/spec/](docs/spec/)。

## 许可证

MIT OR Apache-2.0，任你选择。
