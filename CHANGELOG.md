# Changelog

## v0.8.0 — 2026-11-15

### 任务执行器扩展（B6 最小族）

- 新增 `person`（capacity=1、平面/network 行走）、`agv`（transporter alias）、`elevator`（垂直运输、停靠时间、并发任务容量）。
- 对象新增 `z`（楼层/高度，默认 0，旧 JSON 不变）；Dispatcher 任务执行器族扩展为 Transporter/Person/Agv/Elevator，
  优先级、抢占、容量与等待统计全部沿用。
- 示例：`elevator.ofm`（5 件精确时序）、`person.ofm`、`agv.ofm`；`tests/b6.rs` 5 个测试。
- 简化：电梯无门控/召唤/井道冲突、Person 无 A*/拥挤、AGV 无电池。

### 报告与预览

- `run --fluid` 在 table/markdown/csv 增加 Fluid 段（每对象液位/吞吐/溢出 + 系统总量）。
- `report` / `experiment` 支持 `--flow` 与 `--fluid` 重渲染（含归档 csv/md）。
- 交互式 HTML 预览（v0.7.0 已落地）。

### 验证

- 276 个测试全绿（core 125：golden 10、objects 45、p0 16、checkpoint 6、process_flow 6、fluid 5、b6 5）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.8.0 三平台产物发布并 SHA256 校验。

## v0.7.0 — 2026-11-15

### Fluid 对象族（B5 最小）

- 新增 `fluid_generator` / `fluid_tank` / `fluid_valve` / `fluid_sink`，参数 `flow_rate`、`capacity`、
  `initial_level`、`max_outflow`、`open`/`ratio`；`Model.fluid_step`（默认 0.1）与 `Connection.flow_rate` 限幅。
- 固定步长拓扑结算：tank 同一步收放、溢出统计、valve 无缓冲、sink 全收；`SimReport.fluid` 给出
  每对象 level_avg/level_max/throughput/overflow 与系统总量；检查点覆盖液位与累计量。
- fluid 与离散对象禁止互连，二者可同模型并行；纯离散模型零额外事件。
- 示例 `fluid-line.ofm`：10/s → tank(100, max_outflow 5) → valve 0.5 → sink；level_max 100、avg 82.92、overflow 200、sink 2.50/s。

### 预览与报告

- **交互式 HTML 预览**：`layout --html out.html [--dimension 3d] [--animate N]`，自包含（内联 CSS/JS/SVG），
  拖拽旋转/缩放、Play/Pause 帧播放；无 CDN、无新依赖。
- **Process Flow 报告**：`run --flow` 在 table/markdown/csv 中展示 tokens created/completed 与每块统计。

### 验证

- 267 个测试全绿（core 120：golden 10、objects 45、p0 16、checkpoint 6、process_flow 6、fluid 5）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.7.0 三平台产物发布并 SHA256 校验。

## v0.6.0 — 2026-11-15

### Process Flow token 运行时（B7 最小子集）

- 模型级 `process_flow`：create / delay / assign / decision / sink 块与带端口的连接；校验无环、端点、必填块。
- token 沿块流转，`SimReport.process_flow` 给出每块 input/output/content_avg/avg_stay_time 与 token 计数；
  trace 记录 token 五类事件；检查点覆盖 token 队列与块统计（续跑与直接运行逐位一致）。
- 前置补齐：from 对象的 `on_exit` 在路由之后、接收方 OnEntry 之前触发。
- 限制：对象级绑定、transfer、块容量/触发器未实现。

### 预览、优化与脚本

- **等距 3D 预览**：`layout --dimension 3d --svg`（盒体/连接/箭头；`--animate N` 帧动画）。
- **优化**：`openflex optimize --param Obj.param=2..6 --objective throughput|wip|utilization`（网格/确定性抽样、`--parallel`、多格式输出）。
- **表达式字符串函数**：contains/starts_with/ends_with/upper/lower/concat、`+` 拼接、`item.name`。
- **`.fsm` 概念导出**：`fsm_concept_json`（明确净室边界，仅概念映射）。

### 验证

- 257 个测试全绿（core 115：golden 10、objects 45、p0 16、checkpoint 6、process_flow 6）。
- 黄金案例零回归；clippy 0；fmt 干净；228→257 测试。
- v0.6.0 三平台产物发布并 SHA256 校验。

## v0.5.0 — 2026-11-14

### 对象与运行时

- **操作员抢占**：`operator_preempt` 可打断低优先级、已持有操作员的加工任务；被抢占方暂停并补足剩余时间。
- **Rack 全部类型**：push_back（LIFO，`levels×channels`）、drive_in（FIFO，`channels×depth`）、floor、gravity；新增 `channels`/`depth`。
- **Process Flow**：未实现独立 token 运行时（评估为 2–3 周工程）；新增对象+触发器等价模式示例与文档。
- **experiment trace**：`--report-dir` + `--trace` 为每个场景/复现写 trace。
- **`--resume` 与 `--set` 互斥**（快照语义保护）。

### 工具与工程

- **结构化 CSV 导入**：`import --objects/--connections`（name,kind,pos；from,to,port,strategy），可与到达表组合。
- **基准扩展**：`bench_mm1 --model <path>`，输出 events/s 与 items/s。
- **WebAssembly**：core 可 `cargo check/build --target wasm32-unknown-unknown`，CI 新增阻塞型 wasm job；CLI 可编译但不可运行（文档声明）。
- **FlexSim 对照**：`compare-flexsim.sh` 新增双报告模式（对象级逐指标差值）。
- 安装/签名流程复核（secrets 缺失时跳过），真机安装记录更新到 v0.4.0。

### 验证

- 228 个测试全绿（core 108：golden 10、objects 44、p0 16、checkpoint 6）。
- 6 个黄金案例零回归；`acceptance.sh` 全 PASS；clippy 0 警告、fmt 干净。
- `verify-packaging.sh` 88 项 PASS；`verify-install.sh` 对 v0.4.0 真实 release PASS（20 项）。

## v0.4.0 — 2026-11-14

### 对象与运行时

- **Join Conveyor**：`conveyor.join_inputs: 2`，两条上游带按入口公平轮转汇入一条带，遵守 `item_spacing`；
  无 slug/gap 精细策略（文档注明简化）。
- **检查点/续跑**：`openflex run --save-checkpoint cp.json` / `--resume cp.json --until T`。
  快照包含时钟、事件堆（含 stop 后事件）、对象运行态、双 RNG 流、统计累加器；
  续跑结果与一次性运行逐位一致（core 6 个集成测试 + CLI 5 个端到端测试）。
- 停止边界统一为严格 `time < stop_time`（与黄金案例 GC-2 的语义一致），事件保留在堆中以支持续跑。

### 验证

- 214 个测试全绿（core 103：golden 10、objects 39、p0_semantics 16、checkpoint 6）。
- 6 个黄金案例零回归；`conveyor-join.ofm` 新增示例（10+10 → 20，公平轮转测试）。
- clippy 0 警告、fmt 干净、三平台构建与发布流水线保持通过。

## v0.3.0 — 2026-11-14

### 对象与运行时（P1 剩余）

- **Conveyor 光电门**：`photo_eyes: [{position, name}]`，项在精确位置 `position/speed` 触发 `on_photo_eye`，可改标签/类型/发消息。
- **Conveyor 工位**：`stations: [{position, process_time}]`，项在带面停留加工，后车按间距排队。
- **Rack 类型**：`rack_type: floor | gravity | push_back | drive_in`；floor FIFO，gravity/push_back LIFO，drive_in 近似。
- **Transporter 路网**：`network: [{from,to,distance}]` 双向最短路（无路网回退欧氏）；任务拆两段：取货 → 送达 → 空车回 home。
- **MultiProcessor** 与 **Time Table** 已在 v0.2.0 落地，本版补 schema/校验与文档。

### CLI 与可观测性

- `openflex run --trace events.jsonl`：逐事件 JSONL（`time,seq,event,object,data`），多复现写 `.repN.jsonl`。
- `--until` 作为 `--stop-time` 别名。
- i18n 扩展到 experiment 表格与 Markdown 报告（`OPENFLEX_LANG=zh`）。
- 性能基准示例 `cargo run --release -p openflex-cli --example bench_mm1`：约 1.8M events/s。
- `objects describe` 补齐新参数（photo_eyes/stations/rack_type/network 等）。

### 验证

- 198 个测试全绿（core 92，含 golden 10、objects 36、p0_semantics 16）。
- 6 个黄金案例零回归；新增 4 个对象示例（photo-eye/station/rack-gravity/transporter-network）。

## v0.2.0 — 2026-11-14

### 语义一致性（P0）

- **严格模型校验**：所有参数结构、连接、实验、触发器都拒绝未知键（`unknown key 'x'`），不再静默忽略。
- **Source 到达模式**：新增 `arrival_schedule`（表驱动、可 `repeat`，首行只首轮生效）与 `arrival_sequence`（上一批全部离开后推进）。
- **Pull 模式**：下游主动拉取（`pull:{enabled,strategy,requirement}`），与 SendToPort 同时成立。
- **SendToPort −1**：`PortStrategy::Hold` 扣住已完成项，`release_item` 动作单次放行。
- **在途量计入容量**：运输中的项计入有效含量，容量判断更严格。
- **故障参数**：`first_failure_time`、`count_time_in_states`（如只在 processing 计时）。
- **停机请求栈**：故障/班次/维护等停机源叠加，全部清除才恢复。
- **操作员语义**：setup/process 操作员数量、`use_same_num_operators`、优先级排队（抢占留 TODO）。
- **统计**：`StateTimes` 细分 setup/processing/waiting_for_operator/waiting_for_transport；置信区间改为 t 分布（n<30）；新增 Little 定律自检。
- **场景覆盖**：`Scenario.stop_time` / `warmup`。
- **挂死防护**：未实现的到达模式、零间隔且无上限的 Source 会明确报错而不是无限循环。

### 功能广度（P1）

- 新增分布：Pareto、LogLogistic、Rayleigh、Binomial、Poisson、Geometric、NegativeBinomial、Bernoulli（共 20 种）。
- 新增对象：MultiProcessor（多槽并行）。
- Combiner 增加 `batch` 模式（setup/process、操作员）。
- Separator 增加 `split` 模式（与 unpack 区分）。
- 全部对象支持 `timetable`（与 shifts 取交集）。
- 事件 trace API（`simulate_traced`，普通运行零开销）。
- 新增示例：`examples/golden/arrival-*.ofm`、`examples/objects/{multiprocessor,timetable,separator-split,combiner-batch,scenario-stop-warmup}.ofm`。

### CLI 与工程（P2）

- `completions`、`--progress`、`--dry-run`、结构化 JSON 错误、`--error-format`。
- `layout`（SVG 2D 布局）、`serve`（/health、/objects）、`schema`（JSON Schema）、`import --csv`。
- `run --xlsx`（Summary/Objects/Replications）、`--parallel N`（与串行逐位一致）。
- i18n 基础（`OPENFLEX_LANG=zh|en`）。
- 发布：签名/公证脚本与 CI 可选步骤、GHCR 容器 workflow、Homebrew/winget/scoop 模板、真机安装验证脚本。
- 质量：`deny.toml`、cargo-audit/coverage/fuzz/arm64 CI job、dependabot、FlexSim 对照工具。

### 验证

- 184 个测试全绿（含 10 个黄金案例、29 个对象测试、16 个 P0 语义测试）。
- 6 个黄金案例保持精确/容差内；M/M/1 解析解、确定性、故障可用率、批量、装箱、拆包无回归。
- 三平台构建与打包在 CI 中通过（Windows/Linux 真机安装与代码签名仍待环境）。
