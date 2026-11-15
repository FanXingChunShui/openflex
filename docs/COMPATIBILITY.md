# FlexSim 兼容性矩阵

本表说明 OpenFlex 对 FlexSim 25.x 建模语义的覆盖程度。状态含义：

- ✅ **一致**：参数与行为按公开文档实现，并有测试或数值案例。
- 🟡 **部分**：核心行为一致，复杂选项/边缘行为简化（下方注明）。
- ⛔ **未实现**：当前版本没有对应能力。

> 规格与证据来源见 [research/evidence.md](research/evidence.md) 与 [spec/](spec/)。
> 所有结论来自公开帮助文档（`help.autodesk.com/view/FLEXSIMIN/2025/ENU/`）与公开教材，未接触 FlexSim 源代码或二进制。

## 对象库

| FlexSim 对象 | OpenFlex | 状态 | 说明 / 简化点 |
|---|---|---|---|
| Source | `source` | ✅ | 首件默认 t=0；间隔按“上一批离开 → 下一件到达”计时；每批数量、最大到达数、停止时间、item type、初始标签 |
| Queue | `queue` | ✅ | 最大容量、FIFO/LIFO/优先级/按标签排序；攒批（`batch_size`）与超时放行（`batch_max_wait`）；批次在释放时刻整体提交 |
| Processor | `processor` | ✅ | setup + process、容量、故障 MTBF/MTTR（打断加工并补足剩余时间）、操作员请求（Dispatcher 或 Resource 池） |
| Sink | `sink` | ✅ | 吞吐与停留时间统计 |
| Combiner | `combiner` | ✅ | pack / join / batch（setup+process）；容器先到、等待组件数 |
| Separator | `separator` | ✅ | `separate_mode: unpack`（qty+1 件）/ `split`（qty 件）；0 = 容器内全部组件 |
| Conveyor | `conveyor` | ✅ | 速度、长度、间距、累积/非累积、容量、`curve{radius,angle}` 弧长；`photo_eyes`/`stations`/`join_inputs`；无多段曲线 |
| Merge | `merge` | ✅ | 多输入合流；可选等待全部输入 |
| Split | `split` | ✅ | 轮询或按 item type 分流 |
| Transporter / TaskExecuter | `transporter` | 🟡 | 速度、装载/卸载时间、`network` 最短路、`home` 取货、两段任务；`capacity`=并发任务数（不搬运 flow item）；无多段自定义任务序列 |
| Dispatcher | `dispatcher` | 🟡 | 派发操作员/运输任务，支持优先级与抢占、电梯召唤（优先级→同层→同向→最近） |
| Operator / Resource | `resource` | ✅ | 资源池容量与停机窗口 |
| Rack | `rack` | 🟡 | `rack_type`：floor(FIFO)/gravity(LIFO)/push_back(LIFO，levels×channels)/drive_in(FIFO，channels×depth)；无货位精细放置 |
| 班次 / 计划停机 | `params.shifts` | ✅ | `[{start, duration, repeat_every}]`，停机进入 off_shift、暂停处理、恢复后继续 |
| Time Table | `params.timetable` | ✅ | `{windows:[{start,end}]}`，与 shifts 取交集 |
| BasicFR / BasicTE | `basic_fr` / `basic_te` | ✅ | 通用固定资源与执行器 |
| MultiProcessor | `multiprocessor` | ✅ | 多槽并行加工，每槽 setup/process，操作员按忙碌期申请 |
| Fluid（流体） | `fluid_generator` / `fluid_tank` / `fluid_valve` / `fluid_pipe` / `fluid_sink` | 🟡 | 固定步长、液位/溢出/吞吐、pipe 延迟与背压、valve 压力阈值（入口流量近似，含闭锁）；无真实压力传播/温度/密度 |
| People / A* 行走 | `person` | 🟡 | `grid` 4 邻接 A* + 拥堵速度折减；无对角移动/个体避让 |
| AGV / 交通控制 | `agv` | 🟡 | transporter alias + 电池（容量/耗电/充电）；无交通控制/途中耗尽 |
| Elevator / 电梯 | `elevator` | 🟡 | 垂直运输、停靠、召唤/方向优先；无门控/超载/井道冲突 |
| Emptier | — | 🟡 | 可用 Sink 代替 |

## 逻辑与路由

| FlexSim | OpenFlex | 状态 | 说明 |
|---|---|---|---|
| OnEntry / OnExit / OnReceive | `params.triggers.*` | ✅ | 结构化动作：设置标签、改类型、累加、发消息 |
| OnMessage | `params.triggers.on_message` | ✅ | `send_message` 支持延迟 |
| OnBreakdown / OnRepair | `params.triggers.*` | ✅ | 故障/修复时触发 |
| OnPhotoEye | `params.triggers.on_photo_eye` | ✅ | 传送带光电门在精确位置触发 |
| OnCreate | `params.triggers.on_create` | ✅ | Source 创建项时触发 |
| Send To Port：First Available | `strategy: first_available` | ✅ | 默认 |
| Send To Port：Round Robin | `round_robin` | ✅ | |
| Send To Port：Random | `random` | ✅ | |
| Send To Port：By Percentage | `by_percentage` | ✅ | 权重不检查可用性（与 FlexSim 一样会阻塞） |
| Send To Port：Condition | `by_condition` | ✅ | 表达式求值，按顺序匹配 |
| Send To Port：By Item Type | `by_item_type` | ✅ | |
| Send To Port：By Label | `by_label` | ✅ | |
| FlexScript / C++ 逻辑 | 表达式 + rhai 脚本 | ✅ | `action: script` / `script_condition`（条件用 `result` 标签约定）；沙箱无文件/网络/随机 |
| 全局表（Global Table） | — | ⛔ | 可用标签/参数覆盖模拟 |
| Process Flow（流程图块） | `model.process_flow` / `params.process_flow` | 🟡 | 模型级+对象级 create/delay/assign/decision/sink/transfer(to/from)；块无容量/触发器；Studio 可编辑块与连接 |
| FlexScript 脚本 | `action: script` / `script_condition` | ✅ | rhai 沙箱，触发器/路由/流程决策；条件可直接返回布尔（兼容 `result` 标签）；查询 `object_content/state/utilization`；动作 `set_state/create_item/move_item` |

## 运行与统计

| FlexSim | OpenFlex | 状态 | 说明 |
|---|---|---|---|
| 模型时间单位 | `time_unit` | ✅ | 秒/分/小时/天，仅影响显示 |
| 停止时间 | `run.stop_time` | ✅ | |
| 预热（warm-up） | `run.warmup` | ✅ | 统计窗从 warm-up 起 |
| 随机种子 / 复现 | `run.seed` / `run.replications` | ✅ | SplitMix64 派生，可复现 |
| 运行/暂停/步进 | studio Run | 🟡 | CLI 批量运行 + Studio 运行/动画播放；无逐事件步进/断点 |
| 对象统计（状态占比、吞吐、WIP） | 报告 | ✅ | idle/busy/blocked/down/off_shift；时间加权 content；停留时间 |
| 班次 / 计划可用性 | `shifts` | ✅ | 见上表“班次” |
| 停留时间 | 报告 | ✅ | 平均停留时间 |
| Experimenter（场景 × 复现） | `experiments` + `openflex experiment` | ✅ | 95% 置信区间 |
| 置信区间 | 报告 | ✅ | 正态近似 `1.96·se` |
| OptQuest 优化 | — | ⛔ | 未实现 |
| Dashboard / 图表 | 报告 + Studio 柱状图 | 🟡 | Studio 有吞吐柱状图；无完整 Dashboard/模板 |
| 3D 视图 / 模型树 / GUI | `openflex studio` | 🟡 | 本地 Web Studio：Canvas 3D、字段级动态表单、增删对象、Process Flow 块编辑、撤销/重做、运行/动画；无 GPU 渲染 |
| 导出 Excel / 数据库 | CSV / JSON / xlsx | ✅ | `run --xlsx` 输出 Summary/Objects/Replications 三个 sheet |

## 文件与集成

| FlexSim | OpenFlex | 状态 | 说明 |
|---|---|---|---|
| .fsm 模型文件 | `.ofm` JSON | ✅ | 纯文本、可 diff、可版本控制 |
| 命令行参数 | `openflex` CLI | ✅ | validate/run/experiment/report/compare |
| 脚本接口 | CLI + JSON 报告 + rhai | ✅ | 模型内嵌 rhai 脚本（触发器/条件） |
| 与 Excel/数据库交换 | CSV/JSON/xlsx | ✅ | xlsx 导出；数据库无原生驱动（可用 CSV 中转） |
| 跨平台 | macOS/Windows/Linux | ✅ | 安装包与脚本 |

## 已知语义差异（有意为之）

1. **不使用任意脚本**：FlexSim 的 FlexScript 可读写任意对象与全局状态；OpenFlex 用受限表达式与动作，
   以保证模型可校验、可确定性重放。
2. **没有 3D 渲染**：`pos` 是二维坐标，仅用于计算运输距离与生成布局图。
3. **运输模型简化**：TaskExecuter 只实现“Processor 请求操作员”这一条任务序列（travel → load → 加工 →
   unload → 回 home），不实现抢占、优先级与多段任务序列；`capacity` 表示可并发任务数而非载货量。
4. **统计口径**：`utilization` 定义为忙碌时间/统计时长；与 FlexSim 的“利用率”定义一致，但对象自身
   没有“状态”的对象（如 Sink）不计算利用率。
5. **随机数**：使用 ChaCha12（`StdRng`）而非 FlexSim 的随机数生成器，因此同一模型的数值不会与
   FlexSim 逐位相同；分布语义与统计性质一致（用解析解与教材数值验证，见 golden 测试）。到达间隔使用
   独立随机流，保证只改加工参数时 A/B 对比仍然可比。
6. **Queue 攒批**：批次在“攒够”或“超时”时刻整体提交，批次内项的停留时间在此刻结算；下游可能随后
   逐件接收，但 Queue 的统计不再变化（FlexSim 的逐件 release 在时间加权口径上等价）。
