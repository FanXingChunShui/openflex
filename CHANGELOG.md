# Changelog

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
