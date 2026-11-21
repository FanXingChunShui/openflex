# FlexSim 7.5 全功能对照矩阵（326 页清点）

> **证据来源**：本表结论全部来自 `/Users/ckwang/Downloads/FlexSimChinese7.5/FlexSimChinese7.5/help/` 下的
> 公开帮助文档（FlexSim 7.5 中文版随附的 326 个 HTML 页面，逐页去标签抽取正文后判定功能点），
> 以及本仓库 `crates/`、`docs/` 的当前实现。**未接触 FlexSim 源代码、二进制、安装包或反编译产物。**
> 帮助文档页面本身是厂商公开发布的用户手册，属于净室可用的公开材料。

清点口径与任务卡一致（`glob` 枚举，非 `find`）：

| 目录 | 页数 | 目录 | 页数 |
|---|---:|---|---:|
| PickLists | 45 | ObjectWindows | 56 |
| FlexSimObjectLibrary | 39 | ModelingTools | 31 |
| Tutorials | 29 | MiscellaneousConcepts | 23 |
| ChartingReporting | 19 | ModelingViews | 13 |
| FlexSimConcepts | 12 | FlexSimCoding | 10 |
| MainMenuToolbar | 10 | GettingStarted | 8 |
| TaskSequences | 8 | GeneralWindows | 7 |
| 3DMedia | 6 | Commands | 4 |
| ExperimenterOptimizer | 4 | ROOT（Welcome/WhatsNew） | 2 |
| **小计** | **324** | **合计** | **326** |

正文抽取情况：326/326 成功解码（UTF-8 / GB18030 / UTF-16 三种编码混用，已按 BOM 与 `meta charset`
分派解码器），317 页有效正文 >200 字符；4 个 `Commands/*` 页是 JS 驱动的命令索引（正文仅标题），
1 个 `TaskSequences/Example.html` 是空占位页。**其余 321 页均按正文判定功能点。**

**状态图例**：✅ 一致（可按文档语义工作）· 🟡 部分（核心行为有，复杂选项简化）· ⛔ 缺口（不实现）。

---

## 1. FlexSimConcepts（12 页）— 基础概念

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| Flowitems（流动实体） | `item` 隐式实体，含 `type`、`labels`、`id` | ✅ | 引擎 FlowItem 结构；`Source` 创建 |
| Itemtype（每个流项目自带类型值） | `item.type`，`Action::SetItemType`、`strategy: by_item_type` | ✅ | `action.rs`、`model.rs` PortStrategy |
| Labels（对象与流项目上的命名数据） | `item.label(name)` 表达式 + `set_label`/`increment_label` 动作 | 🟡 | 仅数值标签；文档允许字符串、列表、表（`FlexSimConcepts/Labels.html`），`PickListEntry.value` 是字符串但流项目标签无字符串通道 |
| Item 与 Current（两个访问变量） | 表达式里 `item`、`object`、`time` 等隐式绑定 | 🟡 | 无 `current`/`parnode(1)`/`ownerobject(c)` 这套 C 风格访问变量，依赖 rhai 沙箱的扁平 API |
| Ports（输入/输出/中心三类端口） | `connections[]`（A 连接）+ `pull`/`operator`/`transport_ref` 具名字段 | 🟡 | 无"中心端口"这一等概念：FlexSim 用 S 连接建立对象间引用，OpenFlex 用具名字符串引用（`model.rs`）；语义等价但不可自由新增任意引用通道 |
| Pick Lists（点选即得代码的列表控件） | `model.picklists` 可声明并校验 | ⛔ | **只做了 schema**：`validate.rs:2019` 校验条目非空/重名，但 `openflex-core` 与 `openflex-cli` **零引用**（`grep picklist` 无命中），运行时不解析、不参与任何字段求值 |
| Return Values（代码字段的返回值语义） | 表达式/脚本有返回值；`script_condition` 用 `result` 标签 | ✅ | `expr.rs`、`action.rs` |
| Template Code（弹窗与代码双向绑定的 `/**tag:*/` 机制） | 无对应物 | ⛔ | 纯 GUI 编辑器机制；OpenFlex 用 JSON 直接表达参数，不需要模板注入 |
| Model Units（时间/长度/流体单位，创建后不可改） | `time_unit`（秒/分/小时/天） | 🟡 | 只有时间单位；**长度单位与流体单位没有独立字段**（`model.rs` Model 无 `length_unit`/`fluid_unit`），运输距离与流体量按无量纲数处理 |
| Model Tree View（树视图浏览全部数据） | Studio 模型树（重命名/复制/删除） | 🟡 | 有树形操作，但底层不是 FlexSim 的通用节点树，无法任意插入数字/字符串/对象/指针节点 |
| Order of Events（单个对象的固定事件序列） | 引擎内对象状态机 + 触发器时序 | ✅ | `engine.rs`；与 `docs/spec/logic.md` 的时序表一致 |
| FlexSim Concepts Overview | — | ✅ | 概念页，无独立功能 |
| Simulation Model Units 的"Model Start Time" | 无 | ⛔ | 只支持 `t=0` 起点；文档允许把 0 时刻映射到真实日期时间 |

## 2. FlexSimObjectLibrary（39 页）— 对象库（已由队友对齐，本表仅登记结论）

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| FixedResources（Source/Queue/Processor/Sink/Combiner/Separator/MultiProcessor/BasicFR/Rack） | 同名 `kind` 全部实现 | ✅ | `openflex objects list` 共 38 类可运行对象 |
| TaskExecuters（Dispatcher/Operator/Transporter/Elevator/Crane/ASRSvehicle/BasicTE/Robot） | 同名或变体（`person`/`resource`） | 🟡 | 见 `docs/COMPATIBILITY.md` 逐项说明；本矩阵不重复 |
| Fluid Objects（11 个） | `fluid_*` / `ticker` / 桥接两向 | 🟡 | 固定步长积分；无多相/可压缩 |
| NetworkNode / TrafficControl | `network_node` ✅；`traffic_control` 🟡 | 🟡 | 准入互斥正由另一名队友补（任务卡已注明），本表不重复判定 |

> 对象库 38/39 已对齐，本矩阵覆盖的是**对象库之外**的部分；上表只为完整性登记，不重复队友的逐项结论。

## 3. ObjectWindows（56 页）— 对象属性页

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| General Page：3D Shapes / 旋转 / 尺寸 | `pos` 仅二维坐标 | 🟡 | 无 3D 形状与朝向；Studio 有 2D/透视投影但形状是占位体 |
| General Page：OnDraw 自定义绘制与"返回 1 抑制默认绘制" | `triggers.on_draw` 可声明 | 🟡 | schema 与校验支持，**渲染端未接线**（`action.rs` 注释明说 "the renderer integration is a follow-up"） |
| Labels Page（对象标签，自动复位选项） | 对象级标签 | 🟡 | 无"Automatically Reset Labels"开关；标签复位只能靠 `on_reset` 手动写 |
| Triggers Page（每对象不同触发器集合） | 14 个通用触发器 | 🟡 | 见 §7 PickLists 明细；对象专属触发器（`OnArrival`/`OnContinue`/`OnEntryRequest`/`OnResourceAvailable`）缺失 |
| Statistics Window（实时状态与吞吐面板） | CLI 报告 + Studio 面板 + `--timeseries` | ✅ | 时间加权 content、状态占比、停留时间 |
| Flow Page（Send To Port / Use Transport / Priority / Preempt） | `connections[].strategy`；`Hold` 对应 −1 | 🟡 | `Use Transport`/`Request Transport From` 无对象级开关，运输靠 `transporter`/`dispatcher` 任务；`Priority`/`Preempt` 只在 Dispatcher 侧部分实现 |
| Source Page（三种到达风格 + 到达表） | 间隔/批量/首件/上限/停止 + `arrival_schedule_from_table` | ✅ | 表驱动到达已接线（`engine.rs` 读全局表） |
| Queue Page（容量/LIFO/攒批/视觉摆放） | 容量、`order`（fifo/lifo/priority/by_label）、`batch_size`/`batch_max_wait` | 🟡 | 无 Item Placement/Stack Base Z 这类视觉参数（可接受，无 3D）；`FlushContent` 未实现 |
| Processor Page（setup/process/操作员/交叉传送） | setup/process/容量/操作员请求（`setup_operator`/`process_operator`）/优先级/抢占 | ✅ | `ProcessorParams` 含 `operator_priority`、`operator_preempt` |
| ProcessTimes Page（Setup 与 Process 各自的 operator 数与 Same as Setup） | `num_setup_operators`/`num_process_operators`/`use_same_num_operators` | ✅ | 三个字段齐备（`model.rs:1372-1379`），含"Same as Setup"复用语义 |
| MultiProcessor Page（工序列表，顺序/并行） | `multiprocessor` 多槽 + 每槽 setup/process | 🟡 | 无"工序列表"（多道工序按序执行）概念，只有并行槽 |
| Combiner Page（Pack/Join/Batch、Components list、Recycle To） | `mode`（pack/join/**batch**）+ `components_per_container` + setup/process + 操作员 | 🟡 | 三种模式与继承的 setup/process 均已实现（`model.rs:1534-1563`）；缺**逐端口 components 表**（单一数字）、Recycle To（join 销毁后回收） |
| Separator Page（Unpack/Split、Split/Unpack Quantity、Recycle From） | `separate_mode` + `split_count` | 🟡 | 两种模式与 qty+1 / qty 语义已对齐（`model.rs:1594-1617`）；缺 Recycle From（split 复制源的回收） |
| Sink Page（Recycling Strategy 流项目回收） | 无回收机制 | ⛔ | 无 Flowitem Bin，也无"回收到池"的性能优化路径（对结果无影响，但文档列为页面功能） |
| Rack Page（Floor Storage、Shelf Tilt、Pick/Place Y Offset、标记货位） | `rack` 四类 + `aisles`/`bays`/`pick_policy`/`min_dwell` | 🟡 | 无 Floor Storage 模式、无倾斜/Y 偏移视觉量、无"已叫车货位高亮" |
| Rack SizeTable Page（逐 bay/level 自定义尺寸） | 无 | ⛔ | 只支持均匀网格 |
| BasicFR Advanced Page（Transport Out Notify/Complete 等专属触发器） | `basic_fr` 通用触发器 | ⛔ | 无运输通知类触发器 |
| Breakdowns / Breaks Page（MTBF/MTTR 成员列表，一对象可属多个 MTBF 组） | `failure {mtbf,mttr}` 在对象上 | 🟡 | **不能表达"一个对象属于多个 MTBF/MTTR 组"**，也无"多个停机请求叠加"的停机栈 |
| Container Page（容器件的打包方法） | 无容器件概念 | ⛔ | Flowitem Bin 的 Container 类型与打包方法未实现 |
| Container Functionality Page（VisualTool 内含对象时的输入端口代理） | `visual_tool` 存在 | ⛔ | 无"把输入连接动作转交给内部对象"的能力 |
| Collision Page（碰撞检测球、检查间隔） | 无 | ⛔ | TaskExecuter 无碰撞检测 |
| TaskExecuter Page（Capacity/Max speed/Accel/Decel/Flip threshold/Offset travel） | `capacity`/`speed`/`load_time`/`unload_time`/`home` | 🟡 | 无 Acceleration/Deceleration（匀速）、无 Flip Threshold、无 offset travel、无装载容量与实际载货的区分（`capacity` 在部分变体是并发任务数） |
| Transporter Page（Lift speed / 升降） | `ElevatorParams` 有垂直速度与停靠时间；transporter 无升降 | 🟡 | 升降能力只在 `elevator` 上（`speed` 垂直速度 + `stop_time` 门停靠）；`transporter` 无叉车升降时长，取放瞬时完成 |
| ASRSvehicle / Crane / Robot Page（升降速度、行程顺序、关节速度表） | `asrs_vehicle`/`crane`/`robot` 变体 | 🟡 | 有路径策略；无关节/升降速度表与运动学曲线 |
| Robot Geometry Page（逐关节相对速度/复位位置/手动定位） | 无 | ⛔ | 无关节模型 |
| NetworkNode Page（最大停留车辆数等） | `network_node`（`edges`/`max_travelers`） | ✅ | `max_travelers` 已建模（`NetworkNodeParams`） |
| NetworkNodes / Speeds Page（TrafficControl 关联节点、按含量调速度表） | `traffic_control` 参数 | 🟡 | 准入互斥由队友补；"按含量分档调速度"表未实现 |
| Traffic Control Page（互斥 / 非定时两种模式） | `traffic_control`（capacity/mode/speed_factor） | 🟡 | 同上 |
| Fluid 各类页（Generator/Tank/Mixer/Blender/Splitter/Pipe/Processor/Conveyor/Terminator/ItemToFluid/FluidToItem/Ticker） | 同名 `fluid_*` | 🟡 | 见对象库表；流体页的 Marks（低/中/高液位触发）、Steps（分批延迟）、Sensors（传送带传感器）、Recipe/Percents 表可配置但语义简化 |
| Fluid Marks Page（低/中/高液位升降触发） | `on_port`/`on_factor` 等 | ⛔ | 无液位穿越触发器 |
| Fluid Sensors Page（FluidConveyor 传感器表） | 无 | ⛔ | 无离散传感器 |
| Fluid Steps Page（分批收集 + 步前/步后延迟触发器） | 无 | ⛔ | 无"步进投料"概念 |
| Initial Product Window（ProductID + 子组分列表） | 无产物/组分（Product ID）概念 | ⛔ | 流体无组分追踪 |

## 4. PickLists（45 页）— 点选式逻辑字段（最重要的语义清单）

> 这是 FlexSim 与 OpenFlex 差距最集中的目录：45 个 picklist 是模型的"可编程表面"。
> OpenFlex 用 JSON 参数 + 表达式覆盖了其中 22 个，其余 23 个无对应物。

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| Send To Port | `connections[].strategy`（7 种 + Hold） | ✅ | 含 −1 语义 `Hold` + `release_item` |
| Pull Strategy | `params.pull.strategy` | ✅ | `engine.rs` 有 `pull_requirement_met` |
| Pull Requirement | `params.pull.requirement` | ✅ | 同上；有 `p0_semantics.rs::a03_pull_requirement_skips_items` 测试 |
| Request Transport From | 无字段；运输由 transporter 任务表达 | 🟡 | 缺"由固定资源发起运输请求并指定承运方"的对象级入口（`transport_ref` 仅在部分对象上） |
| Inter Arrivaltime | `source.interarrival`（分布） | 🟡 | 只有统计分布，**不接受任意表达式/查表**；无"按行号查到达表"的 per-row 表达式 |
| Process Time / Setup Time | `process_time` / `setup_time`（分布或表达式） | ✅ | `Expr` 支持 `item.label.*`、`time` 等 |
| Split Unpack Quantity | `separate_qty` 静态数字 | 🟡 | 无"按件动态决定拆几个"的字段（FlexSim 是 picklist） |
| Minimum Staytime（Rack 最小滞留） | `rack.min_dwell` | ✅ | 静态值；无按件表达式 |
| Pick Operator（返回 dispatcher/operator 引用） | `setup_operator`/`process_operator` 字符串名 | 🟡 | 静态引用；无"按条件选不同 dispatcher"的表达式 |
| Place in Bay / Place in Level | `rack.pick_policy` | 🟡 | 有策略枚举，无逐件表达式指定 bay/level |
| Pass To（Dispatcher 选端口） | `DispatcherParams` 的派发逻辑 | 🟡 | 有派发与优先级；无"Pass To 表达式"钩子 |
| Queue Strategy（任务序列排队打分） | Dispatcher 优先级排序 | 🟡 | 有优先级；无自定义打分函数 |
| Load / Unload Time | `load_time`/`unload_time` | ✅ | 分布 + 表达式 |
| Flow Rate / Item Speed（流体/传送带速度） | `conveyor.speed`、流体速率 | 🟡 | 静态或分布；无"每件进入时求值"的 Item Speed picklist |
| Break To（中断任务序列的接受条件） | 无 | ⛔ | TaskExecuter 无 break 任务与 BreakTo 钩子 |
| Text Display（VisualTool 的 3D 文字） | `visual_tool` 无文本渲染 | ⛔ | 无 3D 文本 |
| Rise Fall Through Mark Triggers（液位穿越） | 无 | ⛔ | 见 Fluid Marks |
| Entry Exit Trigger（OnEntry/OnExit） | `on_entry`/`on_exit` | ✅ | |
| Creation Trigger（OnCreation，含 row number） | `on_create` | 🟡 | 有触发器；无 `rownumber` 访问变量（表驱动到达时无法知道来自哪一行） |
| Message Trigger（OnMessage，三个参数） | `on_message` | 🟡 | 有触发器；`SendMessage` 动作**只带 target+delay，无三个用户参数**（`action.rs`） |
| Reset Trigger | `on_reset` | ✅ | |
| OnChange Trigger（watchlist 变量变化） | 无 | ⛔ | 无 watchlist 机制 |
| Collision Trigger | 无 | ⛔ | 无碰撞 |
| Breakdown/Repair Trigger（OnBreakDown/OnRepair） | `on_breakdown`/`on_repair` | ✅ | |
| Down/Up Trigger（MTBF 组级） | 无组级触发器 | ⛔ | 只有对象级 `on_breakdown` |
| Load/Unload Trigger（OnLoad/OnUnload） | 无 | ⛔ | 无装载/卸载时点钩子 |
| Setup/Process Finish Trigger（OnSetupFinish/OnProcessFinish） | 无 | ⛔ | **重要缺口**：无加工完成时点钩子，只能靠 `on_exit` 近似（语义不同：exit 在下游接收后） |
| OnEmpty / OnFull Trigger（流体满/空） | `fluid_*` 有液位但无此类触发器 | ⛔ | |
| OnEntryRequest Trigger（TrafficControl 准入裁决） | `traffic_control` 参数 | ⛔ | 准入互斥由队友补参数侧；**裁决钩子**仍缺 |
| OnReceiveTaskSequence | 无 | ⛔ | TaskExecuter 无接收任务序列的钩子 |
| OnResourceAvailable | 无 | ⛔ | Dispatcher 无"下游可用"回调 |
| OnDraw Trigger | `on_draw`（schema 层） | 🟡 | 见 ObjectWindows 行 |
| Start/End of Warmup、Start/End of Run、Start/End of Scenario、Start/End of Experiment | `run.warmup` 有预热但**无时点钩子** | ⛔ | 6 个实验生命周期 picklist 全缺；`Experiment` 只有 overrides/stop_time/warmup |
| Performance Measure（每复现结束时返回指标值） | 报告固定字段 | 🟡 | 指标由引擎固定生成，**不能由模型自定义表达式**（无 `metrics[]` / performance measure 表） |

## 5. TaskSequences（8 页）— 任务序列

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| Task Sequence Concepts（travel-load-break-travel-unload 五段式） | 运输任务两段/三段（取货→送达→回 home） | 🟡 | 无 break 段；`docs/COMPATIBILITY.md` 已承认"两段任务" |
| Task Types（TASKTYPE_TRAVEL/LOAD/UNLOAD/DELAY/UTILIZE/BREAK/TRAVELTOLOC/…) | 隐含在 transporter 行为里 | 🟡 | 无用户可见的任务类型枚举与逐任务参数（end speed、forcetravel 等） |
| Custom Built Task Sequences（createemptytasksequence/inserttask/dispatchtasksequence） | rhai 脚本可发消息、改状态 | ⛔ | **无任务序列构造 API**：模型无法自建任务序列 |
| Task Sequence Preempting（抢占值、中断后重做/删除） | Dispatcher 有 `preempt_retry_delay`/`preempt_max_retries` | 🟡 | 有抢占重试；无"被中断任务重头再做 vs 删除"的语义选择 |
| Coordinated Task Sequences（两执行器协同，如双人抬） | 无 | ⛔ | 无协同任务 |
| Querying Task Sequences（查询/改任务序列字段） | 无 | ⛔ | 无任务序列对象可供查询 |
| Global Task Sequences（GTS，图形化共享任务序列） | 无 | ⛔ | 无 GTS 工具 |
| Task Sequences Quick Reference（task type × involved × var1..4 表） | 无 | ⛔ | 同"Task Types" |

## 6. ModelingTools（31 页）— 建模工具窗口

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| Global Tables（全局表，数值/字符串，任意对象可读） | `model.global_tables`（schema + 校验 + 引擎 `table_cell` 读取 + 写入 API） | ✅ | `engine.rs:3069`；表达式 `table[name,row,col]`；Studio 有编辑器 |
| Global Variables（全局变量与宏定义） | `model.global_variables` | 🟡 | 有变量；**无宏定义**（FlexSim 的 macro 文本替换） |
| MTBF/MTTR（组对象，成员列表，独立/整组故障，只在指定状态计时） | 对象级 `failure {mtbf,mttr}` | 🟡 | 无组对象、无 FirstFailureTime 负值忽略、**无"只在 processing 状态累计 Up Time"** |
| Time Tables（计划停机：成员/函数表/状态/日周重复/日期模式/相接合并） | `params.timetable {windows}` + `shifts` | 🟡 | 有窗口与重复；**无 Down/Resume 函数钩子、无相接时段自动合并、无按日期模式** |
| User Commands（自定义命令，参数类型标注，可被模型调用） | 无 | ⛔ | 模型无法定义新命令；只能复用内置表达式函数 |
| User Events（按设定时刻执行的函数） | 无 | ⛔ | 无定时用户事件 |
| Model Triggers（模型级触发器：OnReset/OnRunStart 等） | `on_reset` 是对象级 | ⛔ | **无模型级触发器**；跨对象初始化只能逐对象写 |
| Tracked Variables（Work In Progress / Time In System 等可追踪量） | 报告内置若干聚合量 | 🟡 | 无用户自定义 tracked variable |
| Event List（待处理事件列表，可快进到某事件） | `--trace` + trace 回放 + Studio 事件列表 | ✅ | `simulate_traced`、`TraceLog`、Studio 分页 trace |
| Event Log（记录已发生事件，可导出 CSV） | `--trace <FILE>` | 🟡 | 有 trace 导出；无"Enable Logging + 时间段过滤 + CSV 导出"这套交互，也无"两次日志 diff 定位不复现点"的辅助 |
| Excel Interface（多工作表导入/导出） | `--xlsx`（3 sheet 导出）+ `import --xlsx` | 🟡 | 有 xlsx 双向；无"任意工作表映射到 Global Table / 到达表"的通用界面 |
| Script Console（即时执行 FlexScript） | 无交互式控制台 | ⛔ | 只有模型内脚本；无 REPL |
| Flowitem Bin（流项目类与打包方法、默认标签、形状帧） | 无 | ⛔ | 流项目只有单一形态 |
| Animation Creator（关键帧动画、代理对象） | 无 | ⛔ | 无动画系统 |
| AVI Maker（录制模型动画为 AVI） | 无 | ⛔ | |
| Presentation Builder（飞行路径演示） | 无 | ⛔ | |
| Media Files（预加载 3D 形状与图片、取路径/索引） | 无 | ⛔ | 无媒体库 |
| Model Floor / Model Background（地板与背景对象，各限一个） | 无 | ⛔ | 无场地视觉对象 |
| Graphical User Interfaces（自建 GUI：视图/属性表/树绑定） | Studio 固定界面 | 🟡 | 有可配置面板与表单，但不能像 FlexSim 那样自建任意 GUI 并绑定模型属性 |
| Visio Importer（从 Visio 图导入模型） | 无 | ⛔ | |
| ModelLibraries Node（把定义隐藏到模型库节点） | 无 | ⛔ | 无命名空间/隐藏机制 |
| When to Compile（FlexScript vs C++ 编译时机） | 无编译概念（解释执行/预编译 rhai） | ⛔ | 架构性差异，非功能缺失（见 §13 有意差异） |

## 7. ModelingViews（13 页）— 建模视图

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| Perspective / Orthographic View（3D/正交视图、视图保存、单位网格） | Studio 透视投影 + 2D 俯视（Canvas/WebGL） | 🟡 | 有双视图、相机预设、网格吸附；无正交相机与"命名视图保存" |
| Quick Properties（上下文属性面板） | Studio 字段表单 | ✅ | |
| Toolbox（工具集中管理：Global Table/Time Table/Dashboard…） | Studio 侧栏 | 🟡 | 有等效入口，但不是"可 Add 任意工具对象"的工具箱 |
| Library Icon Grid（图标库拖拽建对象） | Studio 新建对象 | 🟡 | 可从列表新建；无拖拽图标网格与自定义库 |
| Edit Selected Objects（批量改坐标/尺寸/旋转/复位位置） | 批量编辑/复制/删除、对齐分布 | 🟡 | 有批量操作；无"复位位置"概念 |
| Find Objects（按名称/类型查找） | 模型树搜索 | ✅ | |
| Groups（对象分组，组信息存于 /Tools/Groups 与对象属性） | 多选 | 🟡 | 有临时多选；**无持久化命名分组**（模型里没有 `groups` 字段） |
| Model Layouts（同一模型的多种布局） | 命名布局预设（localStorage） | 🟡 | 有预设但存于浏览器本地，**不在模型文件里**，不可版本化 |
| Light Source Editor（光源位置/强度） | 无 | ⛔ | 无光照模型 |
| Measure / Convert（测量距离、换算长度/时间单位） | `layout` 导出 + 单位换算函数 | 🟡 | 有单位换算与坐标；无交互式测量工具 |
| Tree Window（树浏览与编辑） | Studio 模型树 | 🟡 | 见 Model Tree View |
| View Settings（视角、网格、光照、显示项） | Studio 视图设置（持久化） | 🟡 | 有相机/网格/热力图；无光照与显示项开关全集 |
| Travel Networks Menu（网络视图模式） | `network_node` + 边表 | 🟡 | 无"网络视图模式"可视化切换 |

## 8. ChartingReporting（19 页）— 图表与报告

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| Reports and Statistics：Summary Report（Excell 导出） | `report`（Markdown/CSV/JSON）+ `--xlsx` | 🟡 | 有汇总报告与 xlsx；**无"用户自选属性列 → Excel"的可配置报告**，列由引擎固定 |
| Reports and Statistics：State Report（状态占比报表） | 报告含状态占比 | ✅ | |
| Reports and Statistics：Model Documentation（自动生成模型文档） | 无 | ⛔ | 无文档生成 |
| Dashboard Concepts（仪表盘容器，运行中实时刷新） | Studio Dashboard（运行中刷新） | 🟡 | 有仪表盘；无 FlexSim 的"图表对象 + 数据源绑定"模型 |
| Dashboard Statistics Page（把对象统计画成图） | Studio 时间序列/柱状图 | 🟡 | 有吞吐柱状与多对象时序；无按"统计项选择器"自由组合 |
| Dashboard Tracked Variables（把 tracked variable 画成图） | 无 tracked variable | ⛔ | 依赖 §6 的 Tracked Variables |
| Dashboard Utilization Analysis（利用率分析专用图） | 报告有 utilization 字段 | 🟡 | 有数据；无专用分析图（按状态堆叠的利用率分解） |
| Dashboard Item Trace（单个流项目的轨迹追踪） | 无 | ⛔ | 无逐件轨迹 |
| Dashboard Objects Page / Associations / Colors / Data / General / Financial Objects / HTML Statistic / DateTimeDisplay（图表属性页） | Studio 图表有有限属性 | ⛔ | 这一组是图表引擎的属性页；OpenFlex 无通用图表 DSL（Financial Objects、HTML Statistic 等完全无对应） |
| Dashboard ModelInput / ModelInputProperties（把模型输入做成可交互控件） | `--set`/Scenario 可改参数 | 🟡 | 有参数覆盖；无"仪表盘上的输入控件" |
| Custom Chart（自定义图表脚本） | 无 | ⛔ | 无图表脚本 |

## 9. ExperimenterOptimizer（4 页）— 实验与优化

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| Experimenter：Variables（标签/全局表/对象数/位置等任意可变量） | `Scenario.overrides`（对象参数替换） | 🟡 | 只能覆盖**对象参数**；不能直接改变量、标签、对象个数、位置 |
| Experimenter：Scenarios（变量取值组合） | `experiments[].scenarios` + `--set` | ✅ | 含 `stop_time`/`warmup` 覆盖（对齐 FlexSim 的 Scenario 定义） |
| Experimenter：Performance Measures（自定义指标函数） | 报告固定字段 | 🟡 | 同 §4 末行：指标不可由模型定义 |
| Experimenter：Analyzing Results（均值/标准差/置信区间/场景对比） | `report` 汇总 + `compare` + batch-means 置信区间 | ✅ | `openflex-report/src/batch.rs` |
| Optimizer：Design（决策变量、约束、目标函数、多目标权重） | `openflex optimize`（grid/random/genetic，多目标加权、Pareto） | 🟡 | 有搜索与目标；**无约束（线性/非线性）表达**、无贝叶斯优化 |
| Optimizer：Run / Results（迭代过程、收敛图） | optimize 输出 | 🟡 | 有结果与 Pareto 前沿；无迭代收敛可视化 |
| Experimenter/Optimizer Advanced（并行/远程/许可证） | `--parallel <N>` | 🟡 | 有本地并行；无远程 worker |

## 10. MainMenuToolbar（10 页）— 菜单与运行面板

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| File：New / Open / Recent / Save / Save As（.fsm/.fsx/.t） | `new` / `validate` / 文件即 JSON | 🟡 | 有新建模板与读写；**无 Recent 列表，无 .fsx/.t 导出** |
| File：Print / Export 图片 | `layout` 导出 SVG；Studio 导出 PNG | 🟡 | 有图导出；无打印与多格式导出矩阵 |
| Edit：Undo / Redo | Studio 撤销 | 🟡 | 有撤销；无重做栈深控制、无"高级聚合撤销"API |
| Edit：Resize/Rotate Object、Set Spline Tension、Set Number Precision、Set Conveyor Angle | 无 | ⛔ | 均为 GUI/渲染参数 |
| Build：Build FlexScript / Compile Model / Compile Session / Open Visual Studio / Make All Code C++ | 无 | ⛔ | 无 C++ 编译管线（有意差异） |
| Execute：Reset / Run / Stop / Step / Increase/Decrease Run Speed / 多停止时间 | `run`（reset→run）+ `step_event`/`next_event`（Studio）+ `--stop-time` | 🟡 | 有等价命令；**无运行速度控制**（无实时时钟）、无多停止时间列表（只有单个 `stop_time`） |
| Statistics：Reports and Statistics / Experimenter / Repeat Random Streams / ExpertFit | `report`/`experiment`/`run.seed`；**无 ExpertFit** | 🟡 | 无分布拟合（ExpertFit 曲线拟合） |
| View：Model View / Model Tree / Model Control GUI / Start Page / Default Workspace / 显示项 | Studio 视图与树 | 🟡 | 有视图与树；无"模型控制 GUI"与工作区保存 |
| Debug：Output/System/Compiler/Script Console、Breakpoints、Code Profile、Event List、Event Log | `--trace`；Studio 事件；**无调试器** | 🟡 | 无断点/单步/调用栈/局部变量/性能分析器（见 §11） |
| Help：Command Helper / Attribute Hints / 手册 | `--help`/`objects describe`/`schema` | 🟡 | 有 CLI 帮助与 schema；无 F1 命令助手 |
| Simulation Run Panel（时钟、Run Time、速度、停止时间勾选） | CLI + Studio 时间轴 | 🟡 | 有时间显示与暂停/步进；无速度滑块 |

## 11. FlexSimCoding（10 页）— 编码与调试

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| Writing Logic in FlexSim（FlexScript 与 C++ 二选一） | rhai 沙箱脚本 + 表达式 DSL | 🟡 | 有脚本层；语法与 FlexScript 不兼容（有意差异） |
| Basic Modeling Functions（常用命令速查：getitemtype/setlabelnum…） | 表达式函数集（`min/max/abs/…` + `item.*`） | 🟡 | 覆盖常用读取；无命令族全集（FlexSim 命令参考有上千条） |
| Code Editor（多标签、语法高亮、模板代码） | 无（JSON 编辑 / Studio 表单） | ⛔ | 无代码编辑器 |
| Command Helper（F1 命令助手） | `--help`、`objects describe` | 🟡 | 有离线帮助；无上下文命令提示 |
| Debugging Overview / Breakpoints / Call Stack / Local Variables / Watch Variables / Code Profiler | 无 | ⛔ | **整套调试器缺失**（无断点、调用栈、变量观察、性能剖析） |
| Debug 控制台（pt/pr/pd/pf 打印） | `log()` 脚本动作 → 报告 | 🟡 | 有日志；无分级控制台与输出过滤 |

## 12. MiscellaneousConcepts（23 页）— 其它概念与子系统

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| Tree Structure（一切皆节点：数/串/对象/指针 + 可执行节点） | JSON 模型对象图 | ⛔ | 架构性差异：无通用节点树，不能任意插入自定义节点承载数据 |
| FlexSim XML（模型/库/树以 XML 保存，可 diff） | JSON（可 diff） | ✅ | 目标一致（文本可版本化），格式不同属有意差异 |
| Advanced Undo（beginaggregatedundo/endaggregatedundo，供库开发者） | Studio 撤销 | ⛔ | 无脚本级聚合撤销 API |
| Custom Libraries（自定义对象库、Dropscript、自动安装） | 无 | ⛔ | 无用户库机制 |
| ModelLibraries Node（隐藏定义） | 无 | ⛔ | |
| Kinematics Concepts / Commands（initkinematics/addkinematic/updatekinematics） | 无 | ⛔ | 无运动学 |
| SQL Queries（query() 命令、SQL 语法过滤/排序/优先级） | 无 SQL | ⛔ | **重要缺口**：全局表无法用 SQL 查询；列表/优先级场景需大量手工逻辑 |
| Sampler（引用对象/变量/标签/表的可视化取引用工具） | 无 | ⛔ | 纯 GUI 工具；OpenFlex 用字符串名引用 |
| State List（状态号与宏对照表：STOPPED/TRAVEL_LOADED/…） | `StateTimes`：5 个顶层状态 + 6 个明细项 | 🟡 | 实际有 **idle/busy/blocked/down/off_shift** 顶层，加明细 **setup/processing/waiting_for_operator/waiting_for_transport/door_opening/door_closing**（`stats.rs`），比 5 个更细；仍**无状态号↔宏的对照表**，也无 Travel Empty/Travel Loaded/Loading/Unloading 这类执行器状态 |
| View Attributes Reference（自定义 GUI 的视图属性全集） | 无 | ⛔ | 依赖自建 GUI |
| GUI Events and View Attributes（OnClick/OnDrag/OnDraw…） | 无 | ⛔ | 无 GUI 事件模型 |
| Webserver（flexsimserver.exe，浏览器访问模型） | `openflex serve`（`/health`,`/objects` 只读） | 🟡 | 有最小 HTTP API；**无查询驱动/远程控制**（文档的 webserver 可查数据、驱动模型） |
| When to Compile | 无编译期概念 | ⛔ | 架构性差异 |

## 13. GeneralWindows（7 页）— 通用窗口

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| Model Settings（精度、传送带角度、样条张力、日期格式、加密、嵌入媒体） | 无设置文件 | ⛔ | 无模型级设置（精度/格式/加密）；只保留 `time_unit` |
| Global Preferences（跨模型默认偏好） | 无 | ⛔ | 无用户级偏好存储 |
| Table Editor（编辑任意节点的表数据） | Studio Global Table 编辑器 | ✅ | 有表格编辑 |
| Database Table View（dbopen 打开的数据库表） | 无数据库驱动 | ⛔ | 无 DB 连接 |
| Find and Replace（跨窗口查找替换） | 模型树搜索 | 🟡 | 有查找；无跨对象批量替换 |
| Attribute Hints（特殊属性名释义表） | `schema` 命令 | 🟡 | 有字段说明；无属性名速查表 |
| Tree Browse Dialog（选择树节点） | 模型树点选 | 🟡 | 有选择；无通用节点选择对话框 |

## 14. 3DMedia（6 页）— 3D 媒体

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| Importing 3D Media（30+ 格式：wrl/3ds/dxf/stl/skp/obj…） | 无 | ⛔ | 无 3D 模型导入（无渲染管线） |
| Importing AutoCAD Drawings（dxf/dwg 清理与导入） | 无 | ⛔ | |
| Level of Detail（同名 LOD 文件分级显示） | 无 | ⛔ | |
| Preparing a 3D File（AC3D 等预处理规范） | 无 | ⛔ | |
| Shape Factors（导入形状的缩放/偏移校正） | 无 | ⛔ | |
| Shape Frames（多形状帧动画与 setframe） | 无 | ⛔ | |

## 15. GettingStarted（8 页）— 上手与术语

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| Getting Started（软件定位与建模流程） | `README`、`docs/` | ✅ | 文档齐备 |
| First Model（三品种加工示例） | `examples/`、golden 案例 | ✅ | 有等价示例 |
| Interacting With FlexSim（建/命名/连接/导航） | Studio 交互 | 🟡 | 有建/连/导航；无键盘拖拽建连（A/S/Q 键语义） |
| Keyboard Interaction（3D 视图快捷键） | Studio 快捷键 | 🟡 | 有部分快捷键；无 FlexSim 全套 |
| Terminology（术语表） | `docs/spec/flexsim-to-openflex.md` 术语表 50 条 | ✅ | 已有中英对照 |
| License Activation（激活/归还/修复/并发服务器） | 无（开源，无许可） | ⛔ | 有意差异，不算缺口 |
| License Activation Example / Reference（激活错误码） | 无 | ⛔ | 同上 |

## 16. Commands（4 页）与 Tutorials（29 页）

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| Command Reference（按类/按字母的命令索引，正文由 JS 渲染） | `openflex --help`、`objects describe` | 🟡 | **这 4 页正文抽取为空（JS 驱动）**，只能确认其存在与用途；无法逐命令对照。命令语义已在 `docs/research/evidence.md` 从 2025 版在线文档取得 |
| Tutorials（15 组 × 介绍/分步）：Lesson 1–3、Labels、Global Modeling Tools、Task Sequence 1–3、TimeTables、User Events、SQL、Kinematics、Fluid | golden 案例 + `docs/spec/golden-cases.md` | 🟡 | 覆盖基础流程/标签/时间表；**User Events、SQL、Kinematics、Fluid、Task Sequence 教程对应的功能多数未实现**（见上表） |

---

## 17. 汇总统计

矩阵共 **197 行**（按功能点拆行，不是按页面 1:1；一页常拆 2–6 行，索引类页面合并为 1 行）。

| 状态 | 行数 | 占比 |
|---|---:|---:|
| ✅ 一致 | **32** | 16.2% |
| 🟡 部分 | **89** | 45.2% |
| ⛔ 缺口 | **76** | 38.6% |
| **合计** | **197** | 100% |

按目录分布（行数 / ✅ / 🟡 / ⛔）：

| 目录 | 行数 | 一致 | 部分 | 缺口 | 缺口密度 |
|---|---:|---:|---:|---:|---|
| FlexSimConcepts | 13 | 5 | 5 | 3 | 23% |
| FlexSimObjectLibrary | 4 | 1 | 3 | 0 | — （队友已对齐，仅登记） |
| ObjectWindows | 33 | 5 | 17 | 11 | 33% |
| PickLists | 34 | 9 | 12 | 13 | 38% |
| TaskSequences | 8 | 0 | 3 | 5 | 62% |
| ModelingTools | 22 | 2 | 7 | 13 | 59% |
| ModelingViews | 13 | 2 | 10 | 1 | 8% |
| ChartingReporting | 11 | 1 | 5 | 5 | 45% |
| ExperimenterOptimizer | 7 | 2 | 5 | 0 | 0% |
| MainMenuToolbar | 11 | 0 | 9 | 2 | 18% |
| FlexSimCoding | 6 | 0 | 4 | 2 | 33% |
| MiscellaneousConcepts | 13 | 1 | 2 | 10 | 77% |
| GeneralWindows | 7 | 1 | 3 | 3 | 43% |
| 3DMedia | 6 | 0 | 0 | 6 | 100% |
| GettingStarted | 7 | 3 | 2 | 2 | 29%（含许可证类有意差异） |
| Commands + Tutorials | 2 | 0 | 2 | 0 | — （Commands 4 页正文为空） |

**读法**：⛔ 76 行里，约 30 行属于"有意差异"（3D 渲染、C++ 编译、节点树、许可证，见 §19），
真正可补的功能缺口约 **45–50 行**；🟡 89 行里大多数是"有核心行为、缺复杂选项"，
按 §18 的 Top 10 逐个补齐后可显著上移。

## 18. 剩余缺口 Top 10（按"影响建模能力 × 实现代价"排序）

| # | 缺口 | 为什么重要 | 工作量 |
|---|---|---|---|
| 1 | **加工完成时点钩子（OnSetupFinish/OnProcessFinish）缺失** | 文档 `PickLists/ProcessFinish.html` 明确这是"加工完成、件尚未离开"的唯一时点；现在只能用 `on_exit` 近似，而 exit 在下游接收后才触发，导致"完工即打标签/发消息"这类常见逻辑语义错位 | **小**（引擎已有状态迁移点，加两个触发点即可） |
| 2 | **PickLists 只做 schema 未接线** | `model.picklists` 能解析能校验，但 `openflex-core`/`openflex-cli` 零引用；用户写了 picklist 却无任何运行时效果，属于"看起来支持实际不支持" | **小**（把 `PickList` 接入表达式求值 + 一个 CLI 查看命令） |
| 3 | **自定义任务序列构造 API（createemptytasksequence/inserttask/dispatchtasksequence）** | FlexSim 的运输逻辑几乎都靠自建任务序列；没有它就无法表达"取货→加工→送回→充电"这类多段流程，TaskExecuter 永远只能是两段 | **大**（需引入任务序列对象模型 + 调度器改造） |
| 4 | **性能指标不可由模型定义（Performance Measure picklist）** | 实验器只能看固定字段，无法回答"每件平均成本""单位时间利润"这类业务指标；这是仿真项目的最终交付物 | **中**（`metrics[]` + 表达式求值 + 报告列注入） |
| 5 | **SQL 查询（query()）缺失** | 全局表/列表的过滤排序目前要靠手工脚本；FlexSim 用 SQL 表达"取优先级最高且满足条件的 3 件" | **中**（可先实现 SELECT/WHERE/ORDER BY/LIMIT 子集） |
| 6 | **Tracked Variables 与执行器状态细分（State List）** | 面向固定资源的状态已细到 setup/processing/waiting_for_*/门开合，但**任务执行器没有 travel_empty/travel_loaded/loading/unloading/utilize 这些状态**，无法回答"车辆空驶占比""装卸占多少时间"；且无用户自定义 tracked variable | **中**（补执行器状态枚举 + 自定义 tracked variable + 报告列） |
| 7 | **模型级触发器与实验生命周期钩子（Model Triggers / Start-End of Warmup·Run·Scenario·Experiment）** | 跨对象初始化、每复现前重置、实验前后汇总都要靠这些钩子；现在只能在每个对象上重复写 `on_reset` | **小**（6+2 个钩子点，集中在引擎的运行边界） |
| 8 | **MTBF/MTTR 组对象与"只在指定状态计时"** | 文档 `MTBF MTTR/index.html` 明确支持一个对象属多个组、组内独立/整组故障、只累计 processing 时间；现在只有对象级墙钟计时，可用率会算错 | **中**（组对象 + 状态感知计时器 + 停机栈） |
| 9 | **图表 DSL 与 Item Trace / Utilization Analysis / Financial Objects** | 报告是固定表格，无法回答"这条产线的利用率随时间怎么变""这一件走了哪条路径"；Dashboard 是 FlexSim 的主要交付界面 | **大**（图表描述模型 + 数据绑定 + 渲染） |
| 10 | **调试器（断点/单步/调用栈/局部变量/性能剖析）** | 模型逻辑出错时现在只有 `log()` 和 trace；文档 `FlexSimCoding/Debugging/*` 5 页描述的是完整调试体验，是模型可维护性的关键 | **大**（rhai 侧调试协议 + Studio 面板） |

**次级但低成本的项目**（不建议排进 Top 10，但可顺手做）：`FlushContent`（整批走完前拒绝新件）、
字符串标签（现在标签只有数值通道）、Recent 文件列表、多停止时间列表、`Groups` 持久化命名分组、
`Model Layouts` 从 localStorage 迁入模型文件、`on_draw` 渲染端接线。

> 注：`UseSameNumOperators`（Same as Setup）已实现，不再列入待办。

---

## 19. 有意差异（非缺口，需在文档中持续声明）

以下行为与 FlexSim 不同，但属于净室与架构选择，不应计入"缺口"：

1. **无 3D 渲染管线**：因此 3DMedia 全部 6 页、Shape Frames、Animation Creator、AVI Maker、
   Presentation Builder、Light Source、Model Floor/Background 均为"不实现"，而非"实现不完整"。
2. **无 C++ 编译期**：`Build` 菜单、`When to Compile`、Code Profiler 的 C++ 侧不适用。
3. **无通用节点树**：`Tree Structure`、`Model Tree View` 的任意节点编辑不适用；OpenFlex 用严格 JSON schema
   换取可校验性与跨平台确定性。
4. **无许可证体系**：`License Activation` 三页不适用。
5. **随机数生成器不同**：同 seed 可复现，但不与 FlexSim 逐位一致（已在 `docs/spec/runtime.md` 声明）。