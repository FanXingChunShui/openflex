# Changelog

## v0.17.0 — 2026-11-15

### 运行控制与对象

- **原生 `SimulationHandle`**：`step_until/pause/resume/snapshot/restore/state/result`，增量推进不重建引擎；
  pause/resume 触发 OnStop/OnResume；多次 step 与一次性运行结果完全相等。
- **Rack 真实货位**：`bays` + `travel_time_per_bay`/`aisle_time`，入架分配 (level,aisle,bay)，
  取货计入行走时间；容量 levels×aisles×bays。
- **Studio**：Pause/Resume 接原生句柄（替换 checkpoint 分段近似）；Global Table 编辑器（单元格/行/新表，保存校验）。

### 工程

- CI 新增 **bench job**（10k 对象 validate <2s / run <5s，超限阻塞）。
- 清单同步 v0.16.0、真机矩阵 v0.16.0、GHCR 0.16.0 已发布（latest 指向 0.16.0）。
- FlexSim 导出 CSV 指引（Statistics/Experimenter/自定义）与示例数据（7/7 在 ±5%）。

### 验证

- 415 个测试全绿（core 189、model 109、CLI 77、report 16）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.17.0 三平台产物发布并 SHA256 校验。

## v0.16.0 — 2026-11-15

### 对象与运行时

- **Rack 按层取货**：`aisles` + `pick_policy`（level_order/nearest/fifo），入架打 level 标签，按层/最近层取货。
- **OnStop / OnResume**：`simulate_until`（暂停点触发 OnStop + checkpoint）与 `resume_from_checkpoint`（恢复触发 OnResume）；
  旧 API 不触发，行为不变。
- **任务序列**：`DispatcherParams.task_sequence`（travel/load/unload）追加段折算额外时间。
- **Global Table 可写**：`set_cell`/`insert_row`/`remove_row`/`Model::set_table_cell`；**表驱动 Source 到达**
  （`arrival_schedule_from_table` 从表解析 time/quantity/item_type/labels）；**OnDraw** 触发器 schema/校验。
- **Studio**：Pause/Resume/Stop 运行控制（checkpoint 分段）、模型树右键 Rename/Duplicate/Delete、Dashboard CSV 导出与时间序列折线图。

### 验证

- 406 个测试全绿（core 180、model 109、CLI 75、report 16）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.16.0 三平台产物发布并 SHA256 校验。

## v0.15.0 — 2026-11-15

### 引擎集成

- **S 连接**：不参与输出端口编号/SendToPort 路由，仅作资源链接（`Processor -S-> Dispatcher` 团队识别）；
  A 与缺省等价。
- **Global Table 表达式**：`table(name, row, column)` 与 `table.<Name>.<row>.<col>` / `table.<Name>.<column>` 只读访问。
- **新触发器**：OnContentChange（接收/取出）、OnStateChange（状态真变化）、OnReset（warmup 重置）已执行，
  `trigger_depth` 防递归；OnStop/OnResume 待运行控制。

### 对象

- **Rack `min_dwell`**：最小滞留（`DwellDone` 事件，push/pull 两条路径都尊重）。
- **电梯门控**：`door_open_time`/`door_close_time`、`max_load`+`load_per_task` 有效载客、`shaft` 井道互斥。
- **Emptier**：`basic_fr.emptier{interval,max_items,destroy}` 周期清空上游（等价 FlexSim Emptier，不新增 kind）。

### Studio 与工具

- Studio 模型树（按 kind 分组/折叠/搜索/3D 高亮联动）。
- `openflex migrate` 接线 `migrate_json`（v1/未知版本/原子写）。
- `optimize --weight throughput=0.7,wip=0.3` 自定义权重。
- 性能：100k 对象 validate 0.75s / run 2.61s（v0.11.0 的约 100×）；profiling 定位 validate 热点为
  `serde_ignored`（~50%）而非语义校验（~2%），run 热点在 flow/notify（~72%）。
- 真机安装命令清单（Windows/Linux）、清单同步 v0.14.0、GHCR 0.14.0 已发布。

### 验证

- 368+ 测试全绿（core 173、model 95、CLI 73、report 16）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.15.0 三平台产物发布并 SHA256 校验。

## v0.14.0 — 2026-11-15

### 模型能力

- **Global Table**：`model.global_tables`（columns/rows，number/bool/string 单元格）、校验与 `Model::table()`；
  空表不写回、重复表名显式拒绝。引擎/表达式访问待接线。
- **完整触发器枚举**：新增 OnContentChange/OnStateChange/OnReset/OnStop/OnResume（schema/校验就绪，执行待接线）。
- **A/S 连接**：`Connection.kind`（`a`/`s`），S 连接不占输出端口、禁端口/策略、禁 source-sink；引擎 S 路由待接线。
- **模型迁移**：`migrate_json` 版本校验 + v1→v2 钩子；CLI `openflex migrate` 规范化输出。

### 优化与统计

- **OptQuest 等价**：`optimize --algorithm grid|random|genetic`、多目标加权、`--pareto` 非支配前沿、确定性 seed。
- **时间序列**：`run --timeseries out.csv --interval N`（区间平均 content 序列）；Studio Dashboard 每复现散点图。
- **性能**：validate 连接/flow/fluid 索引化、report 聚合一次分组；100k 对象 validate 81s/run 259s（−5%），
  瓶颈已定位到反序列化/逐对象校验/重复 validate，后续继续。

### 发布

- GHCR 复核：0.12.0/0.13.0 均存在（此前 404 是 QEMU 多架构构建中），latest=0.13.0；container 加固（超时/诊断/registry 复核）。
- 清单同步 v0.13.0、真机矩阵 v0.13.0、mdBook 构建通过。

### 验证

- 363 个测试全绿（model 95、core 158、CLI 72、report 16）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.14.0 三平台产物发布并 SHA256 校验。

## v0.13.0 — 2026-11-15

### 对象与运行时

- **AGV 有向路网**：`NetworkEdge.directed`（默认无向）；**全段途中扣电**（取货/送达/回程分别按实际距离实时扣电，
  耗尽就地充电后从该段续跑）；示例 `agv-network.ofm`（去程 2s、回程 6s 精确断言）。

### Studio

- **Process Flow 拖拽画布**：块矩形/端口/连线绘制，拖动块调整布局（`metadata.flow_layout`），
  从输出端口拖到目标块创建连接，点击块编辑参数；与块表单/JSON/撤销同步。
- **`openflex import --xlsx`**：读取 xlsx（zip + 手写 XML）的 objects/connections/arrivals 工作表生成模型，
  可 validate/run；错误带 sheet 与 Excel 行号。

### 工程与发布

- **mdBook 文档站真实构建通过**（mdbook v0.5.4，修复 build-dir/重复章节/外部链接问题）。
- 包管理器清单同步到 v0.12.0（homebrew/winget/scoop，verify-packaging 98 项 PASS）。
- 真机安装矩阵更新 v0.12.0（macOS verify-install 20 项 PASS）。
- 已知：GHCR 0.12.0 镜像缺失（container workflow 未成功推送，需重跑）。

### 验证

- 334 个测试全绿（core 158、CLI 89、model 70、report 16）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.13.0 三平台产物发布并 SHA256 校验。

## v0.12.0 — 2026-11-15

### 对象与运行时

- **Fluid 压力场**：tank p=ρgh、pipe Δp 驱动流量（flow_rate 标定）、valve `pressure_open/close` 滞回自动重开、
  pipe 温度黏度修正；`pressure` 报告改为时间平均压力（Pa）。示例 `fluid-pressure.ofm`（19 次关/19 次重开）。
- **Person**：8 邻接 A*（对角 √2、禁切角）；同格 −50%、邻格每人 −15%、全局每人 −10%（下限 40%）。
- **AGV**：取货段途中实时扣电、耗尽就地充电后 `TransportResume` 续跑；目标节点占用等待；
  单向路网/全段扣电/充电桩排队留待下一批。

### Studio

- 时间步进（`POST /api/step`，复用检查点，结果与一次性运行一致）、对象断点、Reset；
- Experimenter（场景×复现对比表）；Dashboard 三联 Canvas（吞吐/利用率/WIP）与 PNG 导出。

### 工程与发布

- 10 万对象性能基准 `scripts/bench-large-model.sh`（M/M/1 1.7M events/s；100k 对象 272s，超线性已记录）；
- 文档站骨架（mdBook）、FlexSim 对照采集流程与 CSV 模板；
- 包管理器清单同步到 v0.11.0（homebrew/winget/scoop，修复 scoop URL 前缀 bug）。

### 验证

- 332 个测试全绿（core 157：golden 10、objects 46、p0 16、process_flow 12、fluid 11、b6 12、script 5）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.12.0 三平台产物发布并 SHA256 校验。

## v0.11.0 — 2026-11-15

### 对象与运行时

- **Fluid 背压/压力**：两阶段 acceptance 结算——tank 始终接收（溢流统计）但释放受下游限制；valve 关闭/低于阈值时
  acceptance=0，上游液位上升；pipe 保留下游收不下的体积；报告新增 `pressure`（平均入口流量）。
- **Conveyor 曲线**：`curve{radius, angle}` 弧长追加到带长（速度不变），直线段行为不变。
- **FlexScript 扩展**：条件可直接返回布尔（兼容 `result` 标签）；新增 `object_content/object_state/object_utilization`
  查询与 `set_state/create_item/move_item` 动作（容量检查、计数、日志）。

### Studio

- 字段级动态表单（按 kind，含 20 种分布子表单）；Process Flow 块列表/块表单/连接增删；撤销/重做（Ctrl+Z/Y）；
  保留 Advanced JSON。

### 工程与发布

- CI：覆盖率 **60% 行覆盖硬门槛**（基线 77.91%）、fuzz 60s→180s、arm64 必跑。
- Homebrew 公式升级为可发布状态（真实 v0.10.0 sha256）、winget/scoop 清单更新、GHCR 镜像已核实（amd64+arm64）；
  `verify-packaging.sh` 新增清单版本/sha 一致性检查（89 项 PASS）。

### 验证

- 326 个测试全绿（core 154：golden 10、objects 46、p0 16、process_flow 12、fluid 9、b6 10、script 5 等）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.11.0 三平台产物发布并 SHA256 校验。

## v0.10.0 — 2026-11-15

### FlexScript 脚本执行

- rhai 沙箱脚本在触发器动作、`ByCondition.script_condition`、Process Flow `Decision` 中执行；
  模型加载时按 source 预编译缓存；可设置标签/类型、发消息、`release_item`、`log`（写 `script_log` trace）；
  条件约定为把数值标签 `result` 设为正值；错误映射 `SimError::Script` 带对象/触发器位置。
- 示例 `script-trigger.ofm`（VIP 4 / 普通 5、9 条日志）。

### Person A* 与人群

- `Model.grid`（宽/高/格距/障碍）：Person 4 邻接 A* 寻路（取货/送达/回程），越界或不可达回退 network/欧氏；
  多个忙碌 person 之间速度折减（每多一人 -10%，最低 50%）。
- 示例 `person-grid.ofm`：绕墙 10 格 / 速度 2 = 5s（欧氏 2s）。

### Process Flow

- `transfer(from_object)`：从 Queue/Rack 取首个 item 转 token，无 item 时等待唤醒；示例 `process-flow-from-object.ofm`。

### Studio

- 增删对象（自动命名/放置，删除清理 connections 与 params 引用）、Process Flow JSON 编辑与保存、
  实时重投影（未保存即重绘）、动画回放（速度滑块 + Play/Pause）。

### 验证

- 321 个测试全绿（core 150：golden 10、objects 45、p0 16、process_flow 12、fluid 8、b6 10、script 3 等）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.10.0 三平台产物发布并 SHA256 校验。

## v0.9.0 — 2026-11-15

### 对象与运行时

- **对象级 Process Flow**：`params.process_flow` 与模型级流程合并；`create.on_entry` 收货产 token；`transfer(to_object)`
  把 token 转成 flow item 注入目标对象（继承标签/类型、目标满时挂起重试）；对象级 block id 形如 `Queue1.Create1`。
- **电梯召唤/方向优先**：优先级 → 同层 → 同向 → 最近楼层；方向持久。
- **AGV 电池**：`battery_capacity` / `consumption_per_distance` / `charge_time` / `charger`；耗尽自动充电，充电中不派单。
- **Fluid pipe / 压力阈值**：`fluid_pipe`（length/speed/delay/flow_rate 的纯延迟缓冲）；valve `pressure_threshold`（入口流量近似）。
- 示例：`process-flow-transfer.ofm`、`agv-battery.ofm`、`fluid-pipe.ofm`、电梯召唤测试。

### FlexScript 与 3D Studio

- **rhai 沙箱脚本层**：`action: script` 与 `script_condition`，模型层语法校验、`core/src/script.rs` 运行时封装
  （确定性 API、操作数/字符串上限）；引擎执行集成留到下一版。
- **`openflex studio`**：本地 127.0.0.1 Web 编辑器——Canvas 3D 视图（旋转/缩放/流动动画）、对象参数编辑与保存、
  Run 按钮（服务端运行 + 统计表/柱状图）；无 CDN、无新 Rust 依赖、2MB body 上限。

### 验证

- 310 个测试全绿（core 138、CLI 85、model 70、report 16）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.9.0 三平台产物发布并 SHA256 校验。

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
