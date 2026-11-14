# OpenFlex 未完成功能清单（Backlog）

> 基线：v0.1.0（2026-11-14，commit `e22dfff`）。本文档按“与 FlexSim 语义一致性 → 功能广度 → 工程质量”排序，
> 每项给出**现状、缺口、验收标准、工作量**（S ≤ 1 天，M ≤ 3 天，L > 3 天）。
> 证据编号（E01…）见 [research/evidence.md](research/evidence.md)，规格见 [spec/](spec/)。

## 状态总览（v0.2.0 基线）

| 组 | 状态 |
|---|---|
| P0 A1–A13 | ✅ 全部完成（A10 的 `operator_preempt` 抢占留 TODO；A4 用 `PortStrategy::Hold` 表达 −1） |
| P1 | B1 MultiProcessor、B10 分布扩展、B11 Time Table、B12 trace（core+CLI）✅；B2 Rack 类型（floor/gravity）✅、B3 Conveyor（PhotoEye/Station/Join）✅、B4 路网+两段任务 ✅、检查点/续跑（core+CLI）✅；B2 push_back/drive_in 细化、B5 Fluid、B6 People/AGV、B7 Process Flow、B8 3D/GUI、B9 脚本/.fsm 未完成 |
| P2 | C4 CLI 完整性、C5 并行复现、C6 JSON Schema、C7 质量工程、C8 i18n 基础 ✅；C1/C2/C3 的脚本与 CI 配置完成（Windows/Linux 真机安装与签名证书待环境） |
| P3 | D1 对照工具与模板 ✅（无真实 FlexSim 数据）；D2 `.fsm`、D3 wasm/云端、D4 优化未做 |

## 摘要

- **核心一致性**：6 个黄金案例已精确/在容差内通过；但仍有 **13 项 P0 语义缺口**（下表 A 组），其中
  “未知参数静默忽略”“Pull 模式”“Separator split 模式”“在途量计入容量”会直接影响模型可信度。
- **功能广度**：对象库覆盖 15 种；FlexSim 还有 Fluid、People/A*、AGV、电梯、机器人、MultiProcessor、
  仓储类型、曲线/合流输送机等 **20+ 个对象**未实现。
- **运行时/统计**：无交互式运行控制、无检查点续跑、无时间序列导出、无 t 分布置信区间、状态未细分。
- **发布**：三平台安装包已发布并校验；但 Windows/Linux **未在真机安装验证**，无签名/公证，无 Homebrew tap / Docker 发布。
- **验证边界**：未与真实 FlexSim 做数值对照（无许可证、净室限制），仅有公开文档语义 + 解析解/确定性推导。

---

## A 组：P0 — 语义一致性与正确性（建议下个迭代完成）

### A1. `validate` 静默忽略未知参数
- **现状**：`Source.params.arrival_style: "schedule"` 这样的未知字段被 serde 忽略，`openflex validate` 通过。
- **缺口**：违反 spec 的“不实现即声明”原则（[flexsim-to-openflex.md](spec/flexsim-to-openflex.md) §1.4）；用户以为参数生效。
- **验收**：所有 `*Params` 加 `#[serde(deny_unknown_fields)]` 或校验时收集未知键并报错；新增 `--allow-unknown` 逃生阀；测试覆盖。
- **工作量**：S

### A2. Source 的 Arrival Schedule / Arrival Sequence 模式
- **现状**：只实现 inter-arrival 模式。
- **缺口**：表驱动到达（ArrivalTime/Quantity/标签列）、`Repeat Table`、序列模式（上一批全部离开才推进）。
- **验收**：`arrival_schedule: {rows, repeat}` 与 `arrival_sequence: [...]`；黄金案例（重复表、首行仅首轮）；`time_zero_arrival` 对齐。
- **工作量**：M

### A3. Pull 模式（Pull / PullStrategy / PullRequirement）
- **现状**：纯 push + 背压。
- **缺口**：下游主动拉取、按条件跳过项、按策略选输入端口（[logic.md](spec/logic.md) §5）。
- **验收**：`params.pull: {enabled, strategy, requirement}`；Pull 与 SendToPort 同时成立；测试覆盖“多上游按标签选择”。
- **工作量**：M

### A4. SendToPort 返回 `-1`（扣住不发）
- **现状**：只有“有可用下游就发”，无“暂不释放”。
- **缺口**：`send_to_port: -1` + 显式 `release_item` 动作。
- **验收**：新动作 `release_item`；对象可长期持有已完成项；统计与阻塞状态正确。
- **工作量**：M

### A5. 在途量计入容量（nroftransportsIn / nroftransportsOut）
- **现状**：`can_accept` 只看 `content + processing`。
- **缺口**：运输中的项未计入有效含量，容量判断偏乐观（[objects.md](spec/objects.md) §1）。
- **验收**：对象维护在途计数；容量阈值 = content + in-transit；测试“容量 10、5 在途时关闭输入”。
- **工作量**：S

### A6. Separator 的 `split`（复制）模式
- **现状**：只实现 unpack（qty+1 件）。
- **缺口**：FlexSim 的 `split` 产出 qty 件（复制而非拆包）；模型无 `separate_mode` 字段。
- **验收**：`separate_mode: "unpack" | "split"`；两种语义各一个黄金案例；文档更新。
- **工作量**：S

### A7. Combiner 的 `batch` 模式与 setup/process
- **现状**：pack/join；无批处理模式，无 setup/process 时间，无操作员。
- **缺口**：FlexSim 的 Combiner 是 Processor 子类（[objects.md](spec/objects.md) §Combiner）。
- **验收**：`mode: "batch"`；`setup_time`/`process_time`/操作员参数；测试覆盖。
- **工作量**：M

### A8. 故障语义补全
- **现状**：MTBF/MTTR 循环，故障立即打断并补足剩余加工时间。
- **缺口**：`first_failure_time`（负值=忽略）、`Up Time` 仅在指定状态计时（如 processing）、故障期间是否释放操作员、OnBreakdown/OnRepair 的参数。
- **验收**：三个参数落地；`breakdown.ofm` 扩展案例验证“只在 processing 计时”；行为写进文档。
- **工作量**：M

### A9. 停机请求栈（多次停机叠加）
- **现状**：单一 down 状态。
- **缺口**：多个停机源（故障、计划维护、班次）叠加与恢复顺序（[logic.md](spec/logic.md) §8.3）。
- **验收**：停机请求栈；恢复后状态回退正确；测试覆盖两个停机源重叠。
- **工作量**：M

### A10. Processor 操作员完整语义
- **现状**：`process_operator` 向 Dispatcher 请求；无 setup 操作员数量、SameAsSetup、优先级、抢占。
- **缺口**：`NumSetupOperators`/`NumProcessOperators`、`UseSameNumOperators`、`OperatorPriority`、`OperatorPreempt`。
- **验收**：参数落地；抢占/排队测试；与 Dispatcher 优先级配合。
- **工作量**：L

### A11. 公开声明两个文档缺口
- **现状**：同刻事件排序 `(time, seq)`、故障立即打断并补足剩余时间，已实现但**未在用户文档明确声明**。
- **缺口**：研究指出这两个是官方文档未定义、必须自行规定并公开的语义（[golden-cases.md](spec/golden-cases.md) §9）。
- **验收**：写入 ARCHITECTURE/COMPATIBILITY 的“语义决策”章节；CHANGELOG 记录。
- **工作量**：S

### A12. 统计状态细分与区间估计
- **现状**：`idle/busy/blocked/down/off_shift`；95% CI 用正态近似。
- **缺口**：Setup / Processing / WaitingForOperator / WaitingForTransport 细分；t 分布临界值（小样本复现）；批均值法（batch means）；Little 定律自检输出。
- **验收**：`ObjectStats.states` 增加细分字段（向后兼容）；`Aggregate.ci95` 支持 t 分布（n<30）；`--check little` 开关。
- **工作量**：M

### A13. 实验场景的 Stop / Warmup 覆盖
- **现状**：`Scenario` 只有参数覆盖。
- **缺口**：FlexSim Experimenter 的 Scenario = 参数向量 + Stop Time + Warmup（[statistics.md](spec/statistics.md)）。
- **验收**：`Scenario.stop_time`、`Scenario.warmup`；实验报告每场景显示；测试覆盖。
- **工作量**：S

---

## B 组：P1 — 重要功能缺失（按对象/领域）

### B1. MultiProcessor
- 现状：未实现；`basic_fr` 仅空壳。
- 缺口：多槽并行加工、每槽独立 setup/process、共享操作员。
- 验收：`kind: "multiprocessor"`，槽位参数，黄金案例。
- 工作量：M

### B2. 仓储对象族
- 现状：`rack` 只有容量/层数（简化队列）。
- 缺口：Floor Storage、Gravity Flow Rack、Push Back Rack、Drive In Rack、Storage Object（最小滞留时间）、层级/通道/取货策略、放置动画。
- 验收：`rack_type` + 策略参数；仓储黄金案例（先进后出/重力流）。
- 工作量：L

### B3. 输送机族
- 现状：直线带（累积/非累积）。
- 缺口：曲线输送机、Join Conveyor（合流）、Merge Controller、Station、Photo Eye、Motor、Transfers、Mass Flow。
- 验收：至少 Join + Station + Photo Eye；合流黄金案例。
- 工作量：L

### B4. 任务执行器与路网
- 现状：直线距离/固定速度、容量=并发任务数、FIFO 调度。
- 缺口：Network Nodes 路网与最短路径、A/S 连接、加速度/减速度、优先级/抢占、多段任务序列（travel/load/unload 拆分）、载货量（capacity vs cargo）、回 home 策略、任务可见性。
- 验收：路网 + 多段任务序列；`transporter` 黄金案例（取货-运送-返回）。
- 工作量：L

### B5. 流体对象（12 个）
- 现状：未实现。
- 缺口：Fluid Generator/Tank/Mixer/…（连续流与离散事件混合）。
- 验收：至少 Generator/Tank/Valve/Pipe；混合仿真案例。
- 工作量：L

### B6. 人员与交通
- 现状：未实现。
- 缺口：People/A* 行走、电梯、AGV/交通控制、Robot/Crane/ASRSvehicle。
- 验收：人员行走 + 电梯各一个案例。
- 工作量：L

### B7. Process Flow 与全局表
- 现状：无。
- 缺口：流程图块（Token/活动/延迟/决策）、Global Table、表驱动参数。
- 验收：至少 Delay/Assign/Decision 块；表驱动 Source。
- 工作量：L

### B8. 可视化与实验界面
- 现状：CLI 报告；无 GUI。
- 缺口：3D 视图/模型树/属性面板、Dashboard/图表模板、Experimenter UI、OptQuest/灵敏度分析、USD/动画/OnDraw。
- 验收：MVP 3D 预览（egui/wgpu）+ 图表导出（SVG/PNG）；交互式运行控制（play/pause/step/speed）。
- 工作量：XL

### B9. 脚本与数据交换
- 现状：受限表达式 + 结构化动作。
- 缺口：FlexScript 兼容子集（字符串、对象引用、循环、函数、全局变量）、`.fsm` 导入导出、Excel（xlsx）导出、CSV 模型导入。
- 验收：明确“兼容子集”文档；导出 xlsx；`.ofm` ↔ `.fsm` 至少单向导出（净室：格式未公开则声明不支持）。
- 工作量：L

### B10. 分布扩展
- 现状：12 种。
- 缺口：FlexSim 公开文档涉及的约 30 种中的 Pareto、LogLogistic、Rayleigh、Johnson SB/SU、Pearson、Binomial、Poisson、Geometric、NegativeBinomial、Bernoulli、Hypergeometric、DiscreteUniform、3 参数 Weibull 等。
- 验收：新增 8~12 种 + 参数校验/均值 + 抽样测试（KS 检验）。
- 工作量：M

### B11. 时间与班次
- 现状：单窗口班次。
- 缺口：Time Table（按小时/星期的可用性表）、节假日、多班次重叠、班次对运输任务的影响。
- 验收：`timetable` 参数；跨班次黄金案例。
- 工作量：M

### B12. 事件追踪与调试
- 现状：无 trace。
- 缺口：事件日志（`--trace`）、单步执行、断点（对象/时间/条件）、检查点/续跑。
- 验收：`--trace out.jsonl`；`openflex run --until` + `--resume`。
- 工作量：M

---

## C 组：P2 — 工程质量、发布与文档

### C1. 真机安装验证
- 现状：macOS 实机通过；Windows/Linux 仅 CI 构建 + 格式检查。
- 缺口：Windows 实机 `msiexec /i`、Linux 实机 `dpkg -i` / `rpm -i` / tar.gz 安装。
- 验收：三平台各一次安装-运行-卸载记录，写入 [VERIFICATION.md](VERIFICATION.md)。
- 工作量：S（需环境）

### C2. 签名与公证
- 现状：无。
- 缺口：macOS Developer ID 签名 + 公证；Windows Authenticode；SHA256SUMS 的 GPG 签名。
- 验收：`spctl --assess` / `signtool verify` 通过；发布流程接入密钥。
- 工作量：M（需证书）

### C3. 包管理器与容器发布
- 现状：Homebrew 公式模板、deb/rpm/MSI、Dockerfile 已就绪；未发布到 tap/镜像仓库。
- 缺口：`brew tap` 发布、Docker Hub/GHCR 镜像、winget/scoop/chocolatey 清单。
- 验收：`brew install openflex`（tap）、`docker run ghcr.io/.../openflex --version`。
- 工作量：M

### C4. CLI 完整性
- 现状：validate/run/experiment/report/compare/new/objects/distributions。
- 缺口：shell completions（bash/zsh/fish/pwsh）、`--progress`、JSON 结构化错误、`--dry-run`、`openflex serve`（HTTP/REST）、`openflex layout`（导出 SVG/PNG）。
- 验收：`openflex completions zsh`；`--format json` 错误对象；serve 提供 `/run` 与 `/health`。
- 工作量：M

### C5. 性能与规模
- 现状：约 21 万事件/秒（release）；单线程。
- 缺口：并行复现（rayon）、事件堆优化、10 万对象/1 亿事件基准、内存峰值报告、事件上限可配。
- 验收：`benches/` + CI 性能门槛；`--parallel` 与确定性平衡方案。
- 工作量：M

### C6. 模型契约工具
- 现状：无 JSON Schema 发布。
- 缺口：`schema.json` 生成与版本化、编辑器补全、模型迁移工具（v1→v2）、未知字段策略（见 A1）。
- 验收：`openflex schema > openflex-model.schema.json`；CI 校验示例模型符合 schema。
- 工作量：S

### C7. 质量工程
- 缺口：fuzz（表达式/模型解析）、`cargo-deny`/`cargo-audit`、覆盖率门槛、aarch64 真机 CI、依赖更新机器人、CHANGELOG、贡献指南、行为准则、mdBook 文档站。
- 验收：CI 增加 audit/fuzz 短跑；覆盖率报告；CONTRIBUTING/CHANGELOG 落地。
- 工作量：M

### C8. 国际化
- 现状：README 中文为主、代码注释英文。
- 缺口：CLI 文案 i18n、英文/日文文档、错误消息本地化。
- 验收：`OPENFLEX_LANG` 环境变量；至少中英文。
- 工作量：M

---

## D 组：P3 — 长期与研究性

### D1. 与真实 FlexSim 的数值对照
- 现状：只有公开文档语义 + 解析解/确定性推导；未运行 FlexSim。
- 缺口：需要许可证或合作环境，对同一模型比对吞吐/WIP/停留时间（分布层面，非逐位）。
- 验收：对照报告（差异与解释），写入 [VERIFICATION.md](VERIFICATION.md)。
- 工作量：M（依赖外部条件）

### D2. `.fsm` 互操作
- 现状：仅 `.ofm`（JSON）。
- 缺口：净室不逆向 `.fsm` 二进制；可做“概念导出”（对象/连接/参数）并声明限制。
- 验收：导出文档 + 工具；明确不支持导入。
- 工作量：L

### D3. WebAssembly / 云端
- 缺口：wasm 构建、浏览器运行、REST 服务、多用户。
- 验收：`wasm32` 构建产物 + 简单 Web 前端。
- 工作量：L

### D4. 优化与实验设计
- 缺口：OptQuest 等价（遗传/贝叶斯优化）、灵敏度分析、场景自动搜索、置信区间驱动的停止规则。
- 验收：`openflex optimize` 命令；示例（加工线最优缓冲区）。
- 工作量：L

---

## 未验证项（不是“未实现”，但尚未证明）

| 项 | 说明 |
|---|---|
| 真实 FlexSim 对照 | 无许可证/安装，未做数值级对比（净室边界） |
| Windows 真机运行 | 仅 CI 构建与格式检查，无实机执行 |
| Linux 真机安装 | 仅 CI 构建与 `file` 检查，无 `dpkg -i`/`rpm -i` |
| MSI 安装/卸载 | 仅生成与格式识别，未在 Windows 上执行 |
| Docker 镜像 | Dockerfile 未实际构建 |
| 大规模性能 | 最大实测约 20 万事件，未做 10 万对象/1 亿事件 |
| 并发/多用户 | 单进程 CLI，无并发场景 |

---

## 建议迭代顺序

1. **v0.1.1（正确性）**：A1、A5、A6、A11、A13（都是 S，风险低，直接提升可信度）。
2. **v0.2（语义完整）**：A2、A3、A4、A8、A10、A12、B1、B10。
3. **v0.3（功能广度）**：B2、B3、B4、B7、B11、B12。
4. **v0.4（工程与发布）**：C1、C2、C3、C4、C5、C6、C7。
5. **长期**：B5、B6、B8、B9、D1–D4。
