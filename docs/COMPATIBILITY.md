# FlexSim 兼容性矩阵

本表说明 OpenFlex 对 FlexSim 建模语义的覆盖程度。权威核对基准是
`FlexSimChinese7.5`（2023-05-10 中文发行版）的 326 个帮助页，逐项结果见
[FEATURE_PARITY_75.md](FEATURE_PARITY_75.md)。状态含义：

- ✅ **一致**：参数与行为按公开文档实现，并有测试或数值案例。
- 🟡 **部分**：核心行为一致，复杂选项/边缘行为简化（下方注明）。
- ⛔ **未实现**：当前版本没有对应能力。

> 规格与证据来源见 [research/evidence.md](research/evidence.md) 与 [spec/](spec/)。
> 所有结论来自公开帮助文档（`help.autodesk.com/view/FLEXSIMIN/2025/ENU/`）与公开教材，未接触 FlexSim 源代码或二进制。

## FlexSim 7.5 中文版对象库逐项对照

以 `/Users/ckwang/Downloads/FlexSimChinese7.5/FlexSimChinese7.5/help/FlexSimObjectLibrary`
的 39 个帮助页为权威清单（2023-05-10 发行版）逐项核对。

| FlexSim 7.5 对象 | OpenFlex 对应 | 状态 |
|---|---|---|
| Source | `source` | ✅ |
| Queue | `queue` | ✅ |
| Processor | `processor` | ✅ |
| Sink | `sink` | ✅ |
| Combiner | `combiner` | ✅ |
| Separator | `separator` | ✅ |
| MultiProcessor | `multiprocessor` | ✅ |
| Rack | `rack` | 🟡 无空位查表/并发取货 |
| BasicFR | `basic_fr` | ✅ |
| Dispatcher | `dispatcher` | 🟡 无多段中途事件 |
| TaskExecuter / Transporter | `transporter` | 🟡 无自定义多段任务序列 |
| Operator | `person` | 🟡 无真正避让 |
| Elevator | `elevator` | 🟡 无门控/超载 |
| Robot | `robot` | 🟡 网格 A*，无抓取运动学 |
| Crane | `crane` | 🟡 无运动学曲线 |
| ASRSvehicle | `asrs_vehicle` | 🟡 共用 TransporterParams |
| BasicTE | `basic_te` | ✅ |
| Ticker | `ticker` | ✅ |
| FluidTank | `fluid_tank` | ✅ |
| FluidGenerator | `fluid_generator` | ✅ |
| FluidTerminator | `fluid_sink` | ✅ |
| FluidMixer | `fluid_mixer` | ✅ |
| FluidBlender | `fluid_blender` | ✅ |
| FluidSplitter | `fluid_splitter` | ✅ |
| FluidPipe | `fluid_pipe` | ✅ |
| FluidProcessor | `fluid_processor` | ✅ |
| ItemToFluid | `item_to_fluid` | ✅ |
| FluidToItem | `fluid_to_item` | ✅ |
| VisualTool | `visual_tool` | ✅ |
| NetworkNode | `network_node` | ✅ 节点可摆放、节点间用 A 连接成边、最短路绕行 |
| TrafficControl | `traffic_control` | ✅ 准入互斥生效（见下方说明） |

> `network_node` 已是一等对象：节点之间用普通 A 连接连成路网，边长默认取两节点直线距离，
> 可用 `params.edges` 覆盖（FlexSim 的 distance table）；运输腿取「起点→最近节点 → 最短路 →
> 最近节点→终点」，模型里没有节点时回退到 `transporter.network` 边表或直线距离。
>
> `traffic_control` 的**准入互斥**（一次只允许 `capacity` 辆车进入控制区）已在引擎中强制：
> 控制区成员 = 连到该对象的 `network_node` 集合；运输腿两端最近节点同属一个控制区时申请名额，
> 名额满则以 `TrafficWait` 事件等待，**释放时定向唤醒**排队者（只保留一次 0.1 s 兜底重试，
> 不再轮询）；`mode: un_timed` 不限名额、只按 `speed_factor` 缩放该段旅行时间；
> 占用记录在 `traffic_hold` 里随快照保存、恢复时重建计数。
> 端到端实测（两车 / 10 米腿 / 100 秒）：`capacity=1` 产生 **7** 次等待事件（定向唤醒前为 964），
> `capacity=2` 与 `un_timed` 均为 0 次，两车完成时刻不变。

## 对象库

| FlexSim 对象 | OpenFlex | 状态 | 说明 / 简化点 |
|---|---|---|---|
| Source | `source` | ✅ | 首件默认 t=0；间隔按“上一批离开 → 下一件到达”计时；每批数量、最大到达数、停止时间、item type、初始标签 |
| Queue | `queue` | ✅ | 最大容量、FIFO/LIFO/优先级/按标签排序；攒批（`batch_size`）与超时放行（`batch_max_wait`）；批次在释放时刻整体提交 |
| Processor | `processor` | ✅ | setup + process、容量、故障 MTBF/MTTR（打断加工并补足剩余时间）、操作员请求（Dispatcher 或 Resource 池）；`setup_finish` / `process_finish` 两个时点钩子（在 setup 结束与加工时钟结束触发，早于 `on_exit`） |
| Sink | `sink` | ✅ | 吞吐与停留时间统计 |
| Combiner | `combiner` | ✅ | pack / join / batch（setup+process）；容器先到、等待组件数 |
| Separator | `separator` | ✅ | `separate_mode: unpack`（qty+1 件）/ `split`（qty 件）；0 = 容器内全部组件 |
| Conveyor | `conveyor` | ✅ | 速度/长度/间距/容量、`curve` 弧长、`segments` 逐段积分、精确回退、严格最小间距（推挤压缩）；`photo_eyes`/`stations`/`join_inputs` |
| Merge | `merge` | ✅ | 多输入合流；可选等待全部输入 |
| Split | `split` | ✅ | 轮询或按 item type 分流 |
| Transporter / TaskExecuter | `transporter` | 🟡 | 速度、装载/卸载时间、`network` 最短路、`home` 取货、两段任务；`capacity`=并发任务数（不搬运 flow item）；自定义任务序列为**腿级**扩展（见 Dispatcher 行） |
| Dispatcher | `dispatcher` | 🟡 | 派发/优先级/抢占（`preempt_retry_delay`/`preempt_max_retries`）、电梯召唤；`task_sequence` 可在标准两段之间插入额外的 `travel`/`load`/`unload` 腿（可带 `duration` 与 `condition`），但腿类型仅此三种，无 FlexSim 的 UTILIZE/BREAK/协同任务 |
| Operator / Resource | `resource` | ✅ | 资源池容量与停机窗口 |
| Rack | `rack` | 🟡 | 四类 + `min_dwell` + `aisles`/`bays`/`pick_policy` 货位分配与取货行走时间；无空位查表/并发取货 |
| 班次 / 计划停机 | `params.shifts` | ✅ | `[{start, duration, repeat_every}]`，停机进入 off_shift、暂停处理、恢复后继续 |
| Time Table | `params.timetable` | ✅ | `{windows:[{start,end}]}`，与 shifts 取交集 |
| BasicFR / BasicTE | `basic_fr` / `basic_te` | ✅ | 通用固定资源与执行器；`basic_fr.emptier` 周期清空上游（等价 FlexSim Emptier） |
| Robot / Crane / ASRS | `robot` / `crane` / `asrs_vehicle` | 🟡 | 执行器变体：robot grid 8 邻接绕障、crane max(垂直,水平)、ASRS/AGV network 最短路；共用 TransporterParams |
| Escalator | `escalator` | 🟡 | 复用 conveyor 皮带语义（人员输送）；无独立密度/碰撞 |
| MultiProcessor | `multiprocessor` | ✅ | 多槽并行加工，每槽 setup/process，操作员按忙碌期申请 |
| Fluid（流体） | `fluid_generator` / `fluid_tank` / `fluid_valve` / `fluid_pipe` / `fluid_sink` / `ticker` / `fluid_processor` / `fluid_blender` / `fluid_mixer` / `fluid_splitter` / `item_to_fluid` / `fluid_to_item` | ✅ | 固定步长、液位/溢出、pipe 延迟/背压/压力场、valve 滞回；**Darcy 参数已接线**（pressure-mode pipe `Q=√(Δp/R)` + ρ(T)），缺参数回退线性标定；processor 速率/容量/损耗、splitter 百分比（按比例缩放）、blender 速率/配比/损耗、mixer 批步、两个桥接方向的离散↔流体换算均已接线；仍无多相/可压缩 |
| People / A* 行走 | `person` | 🟡 | `grid` 8 邻接 A* + 密度折减；无真正避让/重规划 |
| AGV / 交通控制 | `agv` | 🟡 | transporter alias + 电池（容量/耗电/充电/多充电桩候选）；运输腿受 `traffic_control` 准入互斥与 `speed_factor` 约束；无途中耗尽 |
| Elevator / 电梯 | `elevator` | 🟡 | 垂直运输、停靠、召唤/方向优先；无门控/超载/井道冲突 |
| Emptier | `basic_fr.emptier` | ✅ | 作为固定资源的周期性清空行为存在（`EmptierParams`），不是独立对象 |

## 逻辑与路由

| FlexSim | OpenFlex | 状态 | 说明 |
|---|---|---|---|
| OnEntry / OnExit / OnReceive | `params.triggers.*` | ✅ | 结构化动作：设置标签、改类型、累加、发消息 |
| OnMessage | `params.triggers.on_message` | ✅ | `send_message` 支持延迟 |
| OnBreakdown / OnRepair | `params.triggers.*` | ✅ | 故障/修复时触发 |
| OnPhotoEye | `params.triggers.on_photo_eye` | ✅ | 传送带光电门在精确位置触发 |
| OnContentChange / OnStateChange / OnReset / OnStop / OnResume | `params.triggers.*` | ✅ | OnStop/OnResume 经 `simulate_until`/`resume_from_checkpoint` 触发 |
| OnSetupFinish / OnProcessFinish | `params.triggers.setup_finish` / `process_finish` | ✅ | 在 setup 结束、加工时钟结束两个边界触发，早于 `on_exit`；`setup_time = 0` 时无独立 setup 阶段故不触发 |
| OnCreate | `params.triggers.on_create` | ✅ | Source 创建项时触发 |
| Send To Port：First Available | `strategy: first_available` | ✅ | 默认 |
| Send To Port：Round Robin | `round_robin` | ✅ | |
| Send To Port：Random | `random` | ✅ | |
| Send To Port：By Percentage | `by_percentage` | ✅ | 权重不检查可用性（与 FlexSim 一样会阻塞） |
| Send To Port：Condition | `by_condition` | ✅ | 表达式求值，按顺序匹配 |
| Send To Port：By Item Type | `by_item_type` | ✅ | |
| Send To Port：By Label | `by_label` | ✅ | |
| FlexScript / C++ 逻辑 | 表达式 + rhai 脚本 | ✅ | `action: script` / `script_condition`（条件用 `result` 标签约定）；沙箱无文件/网络/随机 |
| 全局表（Global Table） | `model.global_tables` | ✅ | schema/校验/访问器 + 表达式只读访问 + 可写 API（set_cell/insert_row/remove_row）+ 表驱动 Source 到达 |
| 全局变量（Global Variables） | `model.global_variables` | ✅ | `{name: {kind: number\|string, value}}`；schema + 校验（标识符、kind 与值类型一致）；表达式 `$Name` 读取（按 kind 返回数字或字符串），裸名仅在引擎无同名值时回退 |
| Pick Lists | `model.picklists` | ✅ | `{name: {entries: [{name, value}]}}`；schema + 校验（条目名非空不重名、至少一项）；表达式 `picklist("<list>", "<entry>")` 取条目值；未定义列表/条目是明确求值错误 |
| Model Triggers（模型级触发器） | `model.model_triggers` | ✅ | `on_start`（首个事件前）/ `on_reset`（预热边界）/ `on_stop`（到达 stop_time）；执行失败作为 `SimError::Script` 上报而非静默 |
| Performance Measure（自定义指标） | `model.performance_measures` | ✅ | `[{name, expression, unit?}]`；每复现结束时对 `time`/`created`/`completed`/全局变量求值，多复现取均值与 CI；未定义变量是明确 `SimError`；无指标时报告不出现该节 |
| Process Flow（流程图块） | `model.process_flow` / `params.process_flow` | 🟡 | 模型级+对象级 create/delay/assign/decision/sink/transfer(to/from)；块无容量/触发器；Studio 可编辑块与连接 |
| FlexScript 脚本 | `action: script` / `script_condition` | ✅ | rhai 沙箱，触发器/路由/流程决策；条件可直接返回布尔（兼容 `result` 标签）；查询 `object_content/state/utilization`；动作 `set_state/create_item/move_item` |

## 运行与统计

| FlexSim | OpenFlex | 状态 | 说明 |
|---|---|---|---|
| 模型时间单位 | `time_unit` | ✅ | 秒/分/小时/天，仅影响显示 |
| 停止时间 | `run.stop_time` | ✅ | |
| 预热（warm-up） | `run.warmup` | ✅ | 统计窗从 warm-up 起 |
| 随机种子 / 复现 | `run.seed` / `run.replications` | ✅ | SplitMix64 派生，可复现 |
| 运行/暂停/步进 | `SimulationHandle` + studio | ✅ | step_until/pause/resume/state/snapshot + 逐事件 step_event/next_event（EventPreview 含 detail）；Studio Step Event/事件列表 |
| 对象统计（状态占比、吞吐、WIP） | 报告 | ✅ | idle/busy/blocked/down/off_shift；时间加权 content；停留时间 |
| 班次 / 计划可用性 | `shifts` | ✅ | 见上表“班次” |
| 停留时间 | 报告 | ✅ | 平均停留时间 |
| Experimenter（场景 × 复现） | `experiments` + `openflex experiment` | ✅ | 95% 置信区间 |
| 置信区间 | 报告 | ✅ | 正态近似 `1.96·se` |
| OptQuest 优化 | `openflex optimize` | 🟡 | grid/random/genetic、多目标加权/自定义权重、Pareto 前沿；无贝叶斯/约束优化 |
| Dashboard / 图表 | 报告 + Studio 柱状图 | 🟡 | Studio 有吞吐柱状图；无完整 Dashboard/模板 |
| 3D 视图 / 模型树 / GUI | `openflex studio` / `openflex-gui` | 🟡 | Web Studio：3D 透视（遮挡/标签避让/拖拽/布局）+ 2D 俯视（网格吸附/对齐/分布）+ **多选/框选/批量操作**、PNG/SVG/model-JSON 导出、字段表单、模型树、Process Flow 拖拽画布、撤销、pause/step/resume、Experimenter、Dashboard、Global Table 编辑器；无 GPU/WebGL |
| 导出 Excel / 数据库 | CSV / JSON / xlsx | ✅ | `run --xlsx` 输出 Summary/Objects/Replications 三个 sheet |

## 文件与集成

| FlexSim | OpenFlex | 状态 | 说明 |
|---|---|---|---|
| .fsm 模型文件 | `.ofm` JSON | ✅ | 纯文本、可 diff、可版本控制 |
| 命令行参数 | `openflex` CLI | ✅ | validate/run/experiment/report/compare |
| 脚本接口 | CLI + JSON 报告 + rhai | ✅ | 模型内嵌 rhai 脚本（触发器/条件） |
| 与 Excel/数据库交换 | CSV/JSON/xlsx | ✅ | xlsx 导出（run --xlsx）与导入（import --xlsx objects/connections/arrivals）；无数据库驱动 |
| 跨平台 | macOS/Windows/Linux | ✅ | 安装包与脚本 |

## 已知语义差异（有意为之）

1. **不用 FlexScript/C++**：FlexSim 的 FlexScript 是完整的 C 风格脚本；OpenFlex 提供受限表达式与动作，
   以及 rhai 沙箱里的 `action: script`，以保证模型可校验、可确定性重放。
2. **无原生 3D 渲染管线**：`pos` 是二维坐标，用于运输距离与布局图；桌面 GUI 提供投影式透视/俯视视图，
   没有 FlexSim 的实体模型、光照与动画系统。
3. **运输模型部分实现**：TaskExecuter 支持“Processor 请求操作员”这条序列（travel → load → 加工 →
   unload → 回 home）；抢占与优先级**已部分接线**（`dispatcher.preempt_retry_delay` /
   `preempt_max_retries`、`processor.operator_priority` / `operator_preempt`）；
   任务序列可通过 `dispatcher.task_sequence` 在标准腿之间**插入额外的 travel/load/unload 腿**
   （每腿可带 `duration` 分布与 `condition` 表达式，逐腿独立调度），但**腿类型只有这三种**，
   没有 FlexSim 的 UTILIZE / BREAK / 协同任务 / 自定义任务序列构造 API；
   `capacity` 表示可并发任务数而非载货量。
4. **统计口径**：`utilization` 定义为忙碌时间/统计时长；与 FlexSim 的“利用率”定义一致，但对象自身
   没有“状态”的对象（如 Sink）不计算利用率。
5. **随机数**：使用 ChaCha12（`StdRng`）而非 FlexSim 的随机数生成器，因此同一模型的数值不会与
   FlexSim 逐位相同；分布语义与统计性质一致（用解析解与教材数值验证，见 golden 测试）。到达间隔使用
   独立随机流，保证只改加工参数时 A/B 对比仍然可比。
6. **Queue 攒批**：批次在“攒够”或“超时”时刻整体提交，批次内项的停留时间在此刻结算；下游可能随后
   逐件接收，但 Queue 的统计不再变化（FlexSim 的逐件 release 在时间加权口径上等价）。
