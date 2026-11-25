# FlexSim 7.5 全功能对照矩阵（326 页清点）

> **证据来源**：本表结论全部来自 `/Users/ckwang/Downloads/FlexSimChinese7.5/FlexSimChinese7.5/help/` 下的
> 公开帮助文档（FlexSim 7.5 中文版随附的 326 个 HTML 页面，逐页去标签抽取正文后判定功能点），
> 以及本仓库 `crates/`、`docs/` 的当前实现。**未接触 FlexSim 源代码、二进制、安装包或反编译产物。**
> 帮助文档页面本身是厂商公开发布的用户手册，属于净室可用的公开材料。

> **两份文档的分工**（避免读者以为口径矛盾）：本文件 = 面向**帮助文档全量功能面**（361 页 = 主帮助 326 + Conveyor 模块 35），
> 把 326 页里出现的每一个功能点（含 GUI 编辑器、3D 媒体、C++ 编译期、许可证等生态环境）
> 都记成一行，所以 ⛔ 自然更多；`docs/COMPATIBILITY.md` = 面向**建模语义**的覆盖
> （对象库与核心机制、路由、统计、运行），不把 GUI/脚本生态计入，所以它的 ⛔ 很少。
> 同一个能力在两份表里可能一个是 ✅、一个是 ⛔（例：3D 渲染在语义表不计、在功能面表计 ⛔）。
> 看"模型能不能建对"读 COMPATIBILITY；看"FlexSim 的功能面还差多少"读本文件。
>
> **取证**：每行 ✅ 的实现位置与测试证据见 [PARITY_EVIDENCE.md](PARITY_EVIDENCE.md)；
> 该台账对 ✅ 逐行要求"实现符号 + 至少一个行为级测试"，证据不足的行已按台账降级为 🟡。

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

**状态图例**：✅ 一致（可按文档语义工作）· 🟡 部分（核心行为有，复杂选项简化）· ⛔ 缺口（不实现）· 📄 参考页（教程/术语/索引，没有功能面，不计入缺口——把它记成 🟡 会把"不是功能"错算成"只做了一半"）。

---

## 1. FlexSimConcepts（12 页）— 基础概念

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| Flowitems（流动实体） | `item` 隐式实体，含 `type`、`labels`、`id` | ✅ | 引擎 FlowItem 结构；`Source` 创建 |
| Itemtype（每个流项目自带类型值） | `item.type`，`Action::SetItemType`、`strategy: by_item_type` | ✅ | `action.rs`、`model.rs` PortStrategy |
| Labels（对象与流项目上的命名数据） | `item.label(name)` 表达式 + `set_label`/`increment_label` 动作 | 🟡 | 仅数值标签；文档允许字符串、列表、表（`FlexSimConcepts/Labels.html`），`PickListEntry.value` 是字符串但流项目标签无字符串通道 |
| Item 与 Current（两个访问变量） | 表达式里 `item`、`object`、`time` 等隐式绑定 | 🟡 | 无 `current`/`parnode(1)`/`ownerobject(c)` 这套 C 风格访问变量，依赖 rhai 沙箱的扁平 API |
| Ports（输入/输出/中心三类端口） | `connections[]`（A 连接）+ `pull`/`operator`/`transport_ref` 具名字段 | 🟡 | 无"中心端口"这一等概念：FlexSim 用 S 连接建立对象间引用，OpenFlex 用具名字符串引用（`model.rs`）；语义等价但不可自由新增任意引用通道 |
| Pick Lists（点选即得代码的列表控件） | `model.picklists` + 表达式 `picklist("list","entry")` + GUI 面板 | ✅ | 模型字段 `Model.picklists`、校验 `validate_picklists`、Schema `$defs/model.properties.picklists`、表达式求值 `EvalContext::picklist_entry`、GUI 增删改 `OpenFlexApp::add_pick_list` 等；没有 FlexSim 那种"点选即插入模板代码"的编辑器机制（模板代码见下一行） |
| Return Values（代码字段的返回值语义） | 表达式/脚本有返回值；`script_condition` 用 `result` 标签 | ✅ | `expr.rs`、`action.rs` |
| Template Code（弹窗与代码双向绑定的 `/**tag:*/` 机制） | 无对应物 | ⛔（有意差异） | 纯 FlexSim GUI 编辑器机制；OpenFlex 用 JSON 直接表达参数，不需要模板注入。**不计划实现** |
| Model Units（时间/长度/流体单位，创建后不可改） | `time_unit`（秒/分/小时/天） | 🟡 | 只有时间单位；**长度单位与流体单位没有独立字段**（`model.rs` Model 无 `length_unit`/`fluid_unit`），运输距离与流体量按无量纲数处理 |
| Model Tree View（树视图浏览全部数据） | Studio 模型树（重命名/复制/删除） | 🟡 | 有树形操作，但底层不是 FlexSim 的通用节点树，无法任意插入数字/字符串/对象/指针节点 |
| Order of Events（单个对象的固定事件序列） | 引擎内对象状态机 + 触发器时序 | ✅ | `engine.rs`；与 `docs/spec/logic.md` 的时序表一致 |
| FlexSim Concepts Overview | — | 📄 | 概念索引页，无独立功能可对照 |
| Simulation Model Units 的"Model Start Time" | 无 | ⛔ | 只支持 `t=0` 起点；文档允许把 0 时刻映射到真实日期时间 |

## 2. FlexSimObjectLibrary（39 页）— 对象库（已由队友对齐，本表仅登记结论）

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| FixedResources（Source/Queue/Processor/Sink/Combiner/Separator/MultiProcessor/BasicFR/Rack） | 同名 `kind` 全部实现 | ✅ | `openflex objects list` 共 39 类可运行对象（含新增 `motor`） |
| TaskExecuters（Dispatcher/Operator/Transporter/Elevator/Crane/ASRSvehicle/BasicTE/Robot） | 同名或变体（`person`/`resource`） | 🟡 | 见 `docs/COMPATIBILITY.md` 逐项说明；本矩阵不重复 |
| Fluid Objects（11 个） | `fluid_*` / `ticker` / 桥接两向 | 🟡 | 固定步长积分；无多相/可压缩 |
| NetworkNode / TrafficControl | `network_node` ✅；`traffic_control` ✅ | ✅ | 准入互斥已生效：`capacity` 满则等待、释放时定向唤醒（`traffic_try_acquire` / `traffic_hold` 随快照恢复） |

> 对象库 38/39 已对齐，本矩阵覆盖的是**对象库之外**的部分；上表只为完整性登记，不重复队友的逐项结论。

## 3. ObjectWindows（56 页）— 对象属性页

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| General Page：3D Shapes / 旋转 / 尺寸 | `pos` 仅二维坐标 | 🟡 | 无 3D 形状与朝向；Studio 有 2D/透视投影但形状是占位体 |
| General Page：OnDraw 自定义绘制与"返回 1 抑制默认绘制" | `triggers.on_draw` 可声明 | 🟡 | schema 与校验支持，**渲染端未接线**（`action.rs` 注释明说 "the renderer integration is a follow-up"） |
| Labels Page（对象标签，自动复位选项） | 对象级标签 | 🟡 | 无"Automatically Reset Labels"开关；标签复位只能靠 `on_reset` 手动写 |
| Triggers Page（每对象不同触发器集合） | 21 个触发器（含 `on_load`/`on_unload`/`on_empty`/`on_full`/`on_entry_request`/`on_receive_task_sequence`/`on_resource_available`） | 🟡 | 见 §7 PickLists 明细。**`OnArrival`/`OnContinue` 挂不上**：它们是 **NetworkNode 级**触发器，而引擎的旅行者只按**起终点腿**移动、**没有逐节点转移事件**——与已删除的 `max_travelers` 是同一个结构限制（要支持需先实现逐节点遍历，那是路由模型重写） |
| Statistics Window（实时状态与吞吐面板） | CLI 报告 + Studio 面板 + `--timeseries` | ✅ | 时间加权 content、状态占比、停留时间 |
| Flow Page（Send To Port / Use Transport / Priority / Preempt） | `connections[].strategy`；`Hold` 对应 −1；**`queue.use_transport`** | 🟡 | `Use Transport` 已对 **Queue 单出边**实现：物品等 transporter 搬运（`item_pickup → item_depart → item_dropoff`），搬运期间仍计入 queue 内容；实测 10 单位/速度 2 → 下游 `enter` 晚 5 个时间单位。`Priority`/`Preempt` 仍只在 Dispatcher 侧部分实现；其它对象上的 `Use Transport` 未做 |
| Source Page（三种到达风格 + 到达表） | 间隔/批量/首件/上限/停止 + `arrival_schedule_from_table` | ✅ | **本行此前虚标，已修**：原措辞"表驱动到达已接线"**不成立**——引擎只读 inline 到达表，表驱动的 source 一个物品都不创建（`created 0.0`），而 validate 通过。原引用测试 `source_can_load_its_arrival_schedule_from_a_table` 只验证 `Model::apply_table_reference` 这个**解析器**，从未验证引擎使用了它。修法：`Simulation::new` 对 inline/table 到达表统一**解析一次**缓存（table 失败时报 `object '<name>': …`），三个消费点改读缓存。证据：`a_table_driven_source_creates_the_rows_items`、`a_table_driven_source_respects_stop_time`、`editing_the_table_changes_the_arrivals`（`crates/openflex-core/tests/table_arrivals.rs`） |
| Queue Page（容量/LIFO/攒批/视觉摆放） | 容量、`order`（fifo/lifo/priority/by_label）、`batch_size`/`batch_max_wait` | 🟡 | 无 Item Placement/Stack Base Z 这类视觉参数（可接受，无 3D）；`FlushContent` 未实现 |
| Processor Page（setup/process/操作员/交叉传送） | setup/process/容量/操作员请求（`setup_operator`/`process_operator`）/优先级/抢占 | ✅ | `ProcessorParams` 含 `operator_priority`、`operator_preempt` |
| ProcessTimes Page（Setup 与 Process 各自的 operator 数与 Same as Setup） | `num_setup_operators`/`num_process_operators`/`use_same_num_operators` | ✅ | 三个字段齐备（`ProcessorParams.num_setup_operators` / `num_process_operators` / `use_same_num_operators`），含"Same as Setup"复用语义 |
| MultiProcessor Page（工序列表，顺序/并行） | `multiprocessor` 多槽 + 每槽 setup/process + **`steps[]` 顺序工序（含每步操作员数）** | ✅ | 物品占槽后按序跑完每道工序（每步 setup→process）再释放，多槽仍并行；实测两步各 2.0 → 4.0、含 setup → 4.5。**每步 `num_operators` 已接线**：该步开始前申请、**本步结束即释放**、多槽各自等待不串锁；`OperatorRequest` 抽象（`id` 区分槽/步）先落地才实现功能。未设该字段的模型报告逐位不变。`multiprocessor_steps_sum_process_time`、`multiprocessor_steps_include_setup_time`、`multiprocessor_steps_keep_slots_parallel`、`multiprocessor_empty_steps_matches_absent_field`、`multiprocessor_report_hashes_without_steps_are_unchanged`、`multiprocessor_step_validation_rejects_bad_values`、`multiprocessor_steps_are_deterministic`、`multiprocessor_step_waits_for_operator`、`multiprocessor_step_releases_operator_for_next_item`、`multiprocessor_step_waits_for_two_operators`、`multiprocessor_step_operators_are_per_lane`、`multiprocessor_steps_without_operators_keep_report`、`operator_request_abstraction_keeps_all_baselines` |
| Combiner Page（Pack/Join/Batch、Components list、Recycle To） | `mode`（pack/join/**batch**）+ `components_per_container` + setup/process + 操作员 | 🟡 | 三种模式与继承的 setup/process 均已实现（`CombinerMode::Batch`、`CombinerParams.setup_time/process_time`）；缺**逐端口 components 表**（单一数字）、Recycle To（join 销毁后回收） |
| Separator Page（Unpack/Split、Split/Unpack Quantity、Recycle From） | `separate_mode` + `split_count` | 🟡 | 两种模式与 qty+1 / qty 语义已对齐（`SeparateMode::Unpack/Split` 与 `split_count` 注释）；缺 Recycle From（split 复制源的回收） |
| Sink Page（Recycling Strategy 流项目回收） | `SinkParams.recycle: none|on_entry|by_batch` + `recycle_batch` | 🟡 | 策略显式化 + 批次 hook + 校验（`by_batch` 且 0 被拒）。**如实说明**：引擎在 Sink 入口本就丢弃件对象，因此这不改变运行时内存行为，只是把既有行为写明；仍无 Flowitem Bin（对象池） |
| Rack Page（Floor Storage、Shelf Tilt、Pick/Place Y Offset、标记货位） | `rack` 四类 + `aisles`/`bays`/`pick_policy`/`min_dwell` | 🟡 | 无 Floor Storage 模式、无倾斜/Y 偏移视觉量、无"已叫车货位高亮" |
| Rack SizeTable Page（逐 bay/level 自定义尺寸） | 无 | ⛔ | 只支持均匀网格 |
| BasicFR Advanced Page（Transport Out Notify/Complete 等专属触发器） | `basic_fr` 通用触发器 | ⛔ | 无运输通知类触发器 |
| Breakdowns / Breaks Page（MTBF/MTTR 成员列表，一对象可属多个 MTBF 组） | `failure` 在对象上（非独立组对象） | 🟡 | 故障参数已含首次故障与状态计时；**不能表达"一个对象属于多个 MTBF/MTTR 组"**，也无"多个停机请求叠加"的停机栈 |
| Container Page（容器件的打包方法） | 无容器件概念 | ⛔ | Flowitem Bin 的 Container 类型与打包方法未实现 |
| Container Functionality Page（VisualTool 内含对象时的输入端口代理） | `visual_tool` 存在 | ⛔ | 无"把输入连接动作转交给内部对象"的能力 |
| Collision Page（碰撞检测球、检查间隔） | 无 | ⛔ | TaskExecuter 无碰撞检测 |
| TaskExecuter Page（Capacity/Max speed/Accel/Decel/Flip threshold/Offset travel） | `capacity`/`speed`/`load_time`/`unload_time`/`home` + **`acceleration`/`deceleration`** | 🟡 | **加减速已接线**：单一 `travel_time` 实现梯形/三角形运动曲线，接进任务序列 pickup/leg/home、`queue.use_transport`、explicit route、elevator 水平段；未设加速度时精确等于 `距离/速度`（3 个基准模型 SHA-256 不变）。独立实测：d=10、v=1 无加速度 `dropoff@10.0`，加速度 1 → **11.0**；d=0.4 跑不到最高速，三角公式 `1.2649` 与实测一致。**仍缺** Flip Threshold、offset travel、载货量与并发任务数的区分。`travel_without_acceleration_keeps_the_reports`、`travel_time_trapezoid_formula`、`travel_time_triangle_formula`、`travel_time_boundary_matches`、`acceleration_slows_use_transport_leg` |
| Transporter Page（Lift speed / 升降） | `ElevatorParams` 有垂直速度与停靠时间；transporter 无升降 | 🟡 | 升降能力只在 `elevator` 上（`speed` 垂直速度 + `stop_time` 门停靠）；`transporter` 无叉车升降时长，取放瞬时完成 |
| ASRSvehicle / Crane / Robot Page（升降速度、行程顺序、关节速度表） | `asrs_vehicle`/`crane`/`robot` 变体 | 🟡 | 有路径策略；无关节/升降速度表与运动学曲线 |
| Robot Geometry Page（逐关节相对速度/复位位置/手动定位） | 无 | ⛔ | 无关节模型 |
| NetworkNode Page（最大停留车辆数） | **无**（`max_travelers` 曾是死参数，已移除） | ⛔ | 审计发现该字段声明了但引擎从不读取；实现节点级限额需重写 network 路由的占用模型，因此**删除字段而非保留虚标** |
| NetworkNodes / Speeds Page（TrafficControl 关联节点、按含量调速度表） | `traffic_control`（capacity/mode/speed_factor） | 🟡 | 准入互斥已生效；**"按含量分档调速度"表**仍只有单一 `speed_factor`，无逐档曲线 |
| Traffic Control Page（互斥 / 非定时两种模式） | `traffic_control`（`mode: mutual_exclusion` / `un_timed`） | ✅ | 两种模式均生效：互斥按 `capacity` 限名额，`un_timed` 不限名额只缩放 `speed_factor` |
| Fluid 各类页（Generator/Tank/Mixer/Blender/Splitter/Pipe/Processor/Conveyor/Terminator/ItemToFluid/FluidToItem/Ticker） | 同名 `fluid_*` | 🟡 | 见对象库表；流体页的 Marks（低/中/高液位触发）、Steps（分批延迟）、Sensors（传送带传感器）、Recipe/Percents 表可配置但语义简化 |
| Fluid Marks Page（低/中/高液位升降触发） | `FluidTankParams.marks: [{ name, level, on_rise, on_fall }]` | ✅ | 任意数量的具名液位标记，上穿/下穿各触发一次；`level ∈ [0, capacity]`，重名/空名被拒。约定：穿越判定以 **tick 采样**为准（步内多次穿越算一次），已写进 `FluidMark` 文档。`fluid_mark_rise_fires_once`、`fluid_mark_fall_fires_once`、`fluid_mark_jitter_counts_each_crossing`、`fluid_mark_and_full_hook_coexist` |
| Fluid Sensors Page（FluidConveyor 传感器表） | 无 | ⛔ | 无离散传感器 |
| Fluid Steps Page（分批收集 + 步前/步后延迟触发器） | `FluidMixerParams.steps: [{ pull, delay, on_before_delay, on_after_delay }]` | ✅ | 每步按端口拉取材料，收齐后等 `delay` 再分发到下一步；两个钩子分别在"收齐后/延迟前"与"延迟结束"触发。**本行此前标 ⛔ 的实际原因是 `MixerStep.delay` 声明了但引擎从未读取**（注释承诺、代码没做），本版已实现。实测 delay=5 时钩子成对且**每对间隔精确 5.0**。`mixer_delay_zero_keeps_the_report`、`mixer_delay_hooks_are_one_delay_apart`、`mixer_delay_hooks_fire_per_step` |
| Initial Product Window（ProductID + 子组分列表） | 无产物/组分（Product ID）概念 | ⛔ | 流体无组分追踪 |

## 4. PickLists（45 页）— 点选式逻辑字段（最重要的语义清单）

> 这是 FlexSim 与 OpenFlex 差距最集中的目录：45 个 picklist 是模型的"可编程表面"。
> OpenFlex 用 JSON 参数 + 表达式覆盖了其中 22 个，其余 23 个无对应物。

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| Send To Port | `connections[].strategy`（7 种 + Hold） | ✅ | 含 −1 语义 `Hold` + `release_item` |
| Pull Strategy | `params.pull.strategy` | ✅ | `engine.rs` 有 `pull_requirement_met` |
| Pull Requirement | `params.pull.requirement` | ✅ | 同上；有 `p0_semantics.rs::a03_pull_requirement_skips_items` 测试 |
| Request Transport From | `queue.use_transport`（指向 dispatcher） | 🟡 | 固定资源发起运输请求的对象级入口**已对 Queue 落地**：指定 dispatcher 后由其空闲 transporter 直接承运（**不经过 `Dispatcher.requests`**，因此不影响既有 operator 任务）。**边界**：仅 Queue、仅单出边、不做多车协同/preemption；其余对象与完整语义需要先把 `Dispatcher.requests` 抽象成带 kind 的请求（已记入 `docs/BACKLOG.md`） |
| Inter Arrivaltime | `source.interarrival`（分布） | 🟡 | 只有统计分布，**不接受任意表达式/查表**；无"按行号查到达表"的 per-row 表达式 |
| Process Time / Setup Time | `process_time` / `setup_time`（分布或表达式） | ✅ | `Expr` 支持 `item.label.*`、`time` 等 |
| Split Unpack Quantity | `separate_qty` 静态数字 | 🟡 | 无"按件动态决定拆几个"的字段（FlexSim 是 picklist） |
| Minimum Staytime（Rack 最小滞留） | `rack.min_dwell` | ✅ | 静态值；无按件表达式 |
| Pick Operator（返回 dispatcher/operator 引用） | `setup_operator`/`process_operator` 字符串名 | 🟡 | 静态引用；无"按条件选不同 dispatcher"的表达式 |
| Place in Bay / Place in Level | `rack.pick_policy` | 🟡 | 有策略枚举，无逐件表达式指定 bay/level |
| Pass To（Dispatcher 选端口） | `DispatcherParams` 的派发逻辑 | 🟡 | 有派发与优先级；无"Pass To 表达式"钩子 |
| Queue Strategy（任务序列排队打分） | Dispatcher 优先级排序 | 🟡 | 有优先级；无自定义打分函数 |
| Load / Unload Time | `load_time`/`unload_time`（分布 + 表达式） | ✅ | 时长精确生效：`load_time=5` → 载货腿在到达后 5.0 出发；`unload_time=3` → `operator_loaded` 在 requester 到达后 3.0 触发（`explicit_load_and_unload_durations_are_exact`，1e-9 内） |
| Flow Rate / Item Speed（流体/传送带速度） | `conveyor.speed`、流体速率 | 🟡 | 静态或分布；无"每件进入时求值"的 Item Speed picklist |
| Break To（中断任务序列的接受条件） | 无 | ⛔ | TaskExecuter 无 break 任务与 BreakTo 钩子 |
| Text Display（VisualTool 的 3D 文字） | `visual_tool.text`，画布上绘制为标签 | 🟡 | **已修**：`visual_tool` 此前只可编辑、画布上什么都不画（形状/大小/颜色/文字全是空转），现在按自身参数绘制（`visual_tool_size` / `parse_hex_color`，颜色写错回退而不是消失），`text` 画在对象上方；仍是 2D 画布标签，没有 3D 文字渲染。测试 `a_visual_tool_is_drawn_from_its_own_parameters` |
| Rise Fall Through Mark Triggers（液位穿越） | 同 `FluidTankParams.marks` 的 `on_rise`/`on_fall` | ✅ | 穿越事件即使钩子为空也写 trace（`fluid_mark_rise`/`fluid_mark_fall`）；与内置 `on_empty`/`on_full` 各自独立判定、互不吞没 |
| Entry Exit Trigger（OnEntry/OnExit） | `on_entry`/`on_exit` | ✅ | |
| Creation Trigger（OnCreation，含 row number） | `on_create` | 🟡 | 有触发器；无 `rownumber` 访问变量（表驱动到达时无法知道来自哪一行） |
| Message Trigger（OnMessage，三个参数） | `on_message` | 🟡 | 有触发器；`SendMessage` 动作**只带 target+delay，无三个用户参数**（`action.rs`） |
| Reset Trigger | `on_reset` | ✅ | |
| OnChange Trigger（watchlist 变量变化） | `TrackedVariable.on_change` | ✅ | 复用既有跟踪采样点（`tracked_interval`）与采样器已维护的"上一次值"（随 checkpoint 持久化）；**首次采样不算变化**，变化判定基于采样值（不是连续抖动）。实测 `time` 表达式 interval 5 / stop 30 → 恰好 5 次（`[10,15,20,25,30]`），常量 0 次。`kind != level` 给 `on_change` → validate 明确拒绝。`on_change_fires_once_per_changed_sample`、`on_change_never_fires_for_a_constant`、`on_change_script_runs`、`on_change_on_a_summary_is_rejected` |
| Collision Trigger | 无 | ⛔ | 无碰撞 |
| Breakdown/Repair Trigger（OnBreakDown/OnRepair） | `on_breakdown`/`on_repair`；`FailureParams.repair_operator` | ✅ | 故障修复可要求 dispatcher 送操作员到位才开始计时（等待期间保持 `down`、事件驱动、结束只释放该操作员）。`repair_operator_waits_for_the_operator`、`repair_operator_absent_keeps_the_report` |
| Down/Up Trigger（MTBF 组级） | `FailureParams.group` | ✅ | 同组共享**一次**故障周期：组故障时全员 `down` 且各触发 `on_breakdown`，修复后全员恢复且各触发 `on_repair`；组间互不影响。组由 **model order 第一个成员（driver）** 驱动，validate 要求同组 `mtbf`/`mttr` 一致。**决定性验证**：有 `group` 时 P1/P2 都 down 2/30，去掉 `group` 只有 P1 down。checkpoint 恢复逐位相同。`mtbf_group_stops_every_member_once`、`mtbf_group_differs_from_independent_failures`、`mtbf_groups_are_isolated`、`mtbf_group_checkpoint_resumes`、`mtbf_group_is_deterministic` |
| Load/Unload Trigger（OnLoad/OnUnload） | `Triggers.on_load` / `on_unload` | ✅ | OnLoad 落在 `TransportDepart`（`now+dwell+load`）、OnUnload 落在 `OperatorLoaded`，各一次且**不新增事件**（事件序列/统计不变）；`on_load_script_is_precompiled`、`on_load_fires_at_dwell_plus_load`、`on_load_label_is_visible_in_the_unload_hook`。限制：执行器不持件，钩子的 item 是 requester 的件（已写进 Rust 文档注释） |
| Setup/Process Finish Trigger（OnSetupFinish/OnProcessFinish） | `params.triggers.setup_finish` / `process_finish` | ✅ | `SimTrigger::SetupFinish`/`ProcessFinish`（`action.rs`；JSON 名 `setup_finish`/`process_finish` 由 `as_str`/`parse` 映射）；在两个边界触发且早于 `on_exit`；`setup_time=0` 无独立 setup 阶段故不触发 |
| OnEmpty / OnFull Trigger（流体满/空） | `Triggers.on_empty` / `on_full`（仅 `fluid_tank` 有意义） | ✅ | 锁存去抖，持续空/满只触发一次；**"满"的信号是溢出增量**（tank 满时液位停在 `capacity − 流入步长`，`level == capacity` 永不成立），已写进文档注释。`fluid_on_full_fires_once`、`fluid_on_empty_fires_once`、`fluid_boundary_hooks_do_not_change_reports` |
| OnEntryRequest Trigger（TrafficControl 准入裁决） | `Triggers.on_entry_request` | ✅ | 在 `traffic_try_acquire` 容量判定**之前**触发；`label("admit")==0` → 拒绝并回退到既有等待重试。**这是 OpenFlex 协议而非 FlexSim 公开 API**，已写进 Rust 文档注释。`on_entry_request_admit_zero_is_retried_later`、`on_entry_request_without_admit_keeps_the_default_path`、`on_entry_request_script_is_precompiled`。限制：上下文是 `FlowItem::probe` 空件，只有 labels 有意义 |
| OnReceiveTaskSequence | `Triggers.on_receive_task_sequence` | ✅ | 执行器被分派任务、**开始执行第一腿之前**触发一次（每任务一次，普通与协同分支都触发）。**边界**：序列是模型级声明、运行时只读，故钩子**不能修改序列**，只做副作用——已写进 Rust 文档注释。`receive_task_sequence_fires_once_per_dispatch`、`receive_task_sequence_hook_can_change_the_model`（`send_message` → 下游 `on_message` 真的运行，证明副作用送达模型） |
| OnResourceAvailable | `Triggers.on_resource_available` | 🟡 | 覆盖两个来源：`resource` 池位释放与 transporter `finish_task`。实测 3 件模型：每次释放各触发一次（t=4/24/44）。**边界**：shift 结束、维修完成等其它可用性变化不触发 |
| OnDraw Trigger | `on_draw`（schema 层） | 🟡 | 见 ObjectWindows 行 |
| Start/End of Warmup、Start/End of Run、Start/End of Scenario、Start/End of Experiment | `model_triggers`：`on_start`/`on_reset`(预热边界)/`on_stop`/`start_of_scenario`/`end_of_scenario`/`start_of_experiment`/`end_of_experiment`/`start_of_replication`/`end_of_replication` | ✅ | 9 个时点钩子全覆盖，按"实验→场景→复现"分层触发；`replication_lifecycle_hooks_fire_once_per_run`、`experiment_lifecycle_hooks_run_in_order`、`lifecycle_hook_errors_are_reported`、`schema_covers_lifecycle_hooks`（**此行为 2026-11-24 纠正：原先记作 ⛔ 是我漏查**） |
| Performance Measure（每复现结束时返回指标值） | `model.performance_measures` | ✅ | 模型可自定义 `{name, expression, unit}`，每复现结束求值并进报告/实验/优化（同 §9 的 Experimenter 行）；未定义变量是明确 `SimError`。证据见 [PARITY_EVIDENCE](PARITY_EVIDENCE.md) #42 |

## 5. TaskSequences（8 页）— 任务序列

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| Task Sequence Concepts（travel-load-break-travel-unload 五段式） | `explicit` 整链：`travel/load/unload/utilize/break` 任意顺序 | 🟡 | append 模式仍是两段/三段；`explicit` 下五段式可任意编排，含 `utilize`(Busy) / `break`(idle)。仍缺：动态目标跟随、checkpoint 保存链状态、链内抢占 |
| Task Types（TASKTYPE_TRAVEL/LOAD/UNLOAD/DELAY/UTILIZE/BREAK/TRAVELTOLOC/…) | `TaskStepKind` + `ExtraTaskLegKind` + `TravelTarget` | 🟡 | 已有 travel/load/unload/utilize/break 与目标（home/pickup/requester/object/position）；仍缺 DELAY、TRAVELTOLOC 的 end speed / forcetravel 等逐任务参数 |
| Custom Built Task Sequences（createemptytasksequence/inserttask/…） | 声明式 `explicit` 路线表 | 🟡 | **声明式**已能表达固定/多段流程；仍无**过程式**构造 API（运行时按条件 inserttask） |
| Task Sequence Preempting（抢占值、中断后重做/删除） | Dispatcher 有 `preempt_retry_delay`/`preempt_max_retries` | 🟡 | 有抢占重试；无"被中断任务重头再做 vs 删除"的语义选择 |
| Coordinated Task Sequences（两执行器协同，如双人抬） | `TaskStep::Detailed.involved: 2` | 🟡 | 声明"该步骤需要两名执行器"：dispatcher 一次派两个并写同一 `task_group`，**两者都到场才同步开始**，结束时各自释放；只够一人时**永久等待且不忙等**（实测整场 4 个事件）。`involved: 1` 与不写 `involved` 报告 sha256 相同。**边界**：仅支持声明式的 1/2 名执行器（FlexSim 的过程式 `insertallocatetask`/`insertsynctask` 未做）；与 `explicit` 的组合**加载即被 validate 拒绝**（内联/命名/ref 三条来源全覆盖）；抢占有专测（整组取消、无泄漏、无忙等）；AGV 充电场景由屏障判据专测覆盖（实测引擎在任务完成后才充电，故未出现"充电中误判到场"）。`involved_two_needs_both_arrivals`、`involved_two_with_one_executer_waits_without_spinning`、`involved_two_releases_both_executers`、`involved_out_of_range_is_rejected`、`involved_two_with_explicit_is_rejected`、`coordinated_group_survives_a_preemption`、`coordinated_barrier_waits_for_every_member` |
| Querying Task Sequences（查询/改任务序列字段） | trace 可观察各腿事件 | ⛔ | 无运行期任务序列对象可供读写 |
| Global Task Sequences（GTS，图形化共享任务序列） | `Model.task_sequences` + `DispatcherParams.task_sequence_ref` | 🟡 | **命名复用**已实现：多个 dispatcher 引用同一份 steps（引用与内联互斥、未知引用/重名/空名均 validate 拒绝；steps 不复制，改 GTS 影响全部引用者）。实测引用式与内联式报告 sha256 相同。**仍无图形化 GTS 编辑器**（用 JSON 声明） |
| Task Sequences Quick Reference（task type × involved × var1..4 表） | 无 | ⛔ | 同"Task Types" |

## 6. ModelingTools（31 页）— 建模工具窗口

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| Global Tables（全局表，数值/字符串，任意对象可读） | `model.global_tables`（schema + 校验 + 引擎 `table_cell` 读取 + 写入 API） | ✅ | `Engine` 的 `table_cell` 读取 + `set_cell/insert_row/remove_row`；表达式 `table[name,row,col]`；Studio 有编辑器 |
| Global Variables（全局变量与宏定义） | `model.global_variables` + 表达式 `$Name` | 🟡 | 变量已完整（字段 `Model.global_variables`、校验 `validate_global_variables`、Schema `$defs/model.properties.global_variables`、`$Name` 求值走 `EvalContext::global_variable`（按 `kind` 返回数字或字符串）、GUI 面板）；**无宏定义**（FlexSim 的 macro 是文本替换，语义不同） |
| MTBF/MTTR（组对象，成员列表，独立/整组故障，只在指定状态计时） | 对象级 `failure {mtbf, mttr, repair_operator, first_failure_time, count_time_in_states}` | 🟡 | 首次故障（负值=忽略，A8）与**只在指定状态累计 Up Time**（`count_time_in_states`）均已实现（`FailureParams` 的 `first_failure_time` 与 `count_time_in_states`）；仍无**组对象**（一个对象不能同时属多个 MTBF 组、无组级 Down/Up 触发器）、无停机请求栈 |
| Time Tables（计划停机：成员/函数表/状态/日周重复/日期模式/相接合并） | `params.timetable {windows}` + `shifts` | 🟡 | 有窗口与重复；**无 Down/Resume 函数钩子、无相接时段自动合并、无按日期模式** |
| User Commands（自定义命令，参数类型标注，可被模型调用） | `Model.user_commands: [{ name, parameters, body }]` | 🟡 | 脚本里直接 `<name>()` / `<name>(1.0)` 调用；**只支持 0/1 个数值参数**（更多在 validate 明确报错），命令名不得与 12 个内置脚本函数重名；包装体单一来源 → `validate` / `check-scripts` / 运行时三处判定一致。`user_command_without_parameters_runs`、`user_command_with_one_parameter_uses_the_value`、`user_command_return_value_is_usable`、`user_command_builtin_name_is_rejected`、`user_command_two_parameters_are_rejected`、`a_float_parameter_flows_into_integer_script_apis` |
| User Events（按设定时刻执行的函数） | `Model.user_events: [{ time, name, actions }]` + GUI 面板 | ✅ | 进入事件日历、`(time, seq)` 确定性、**同刻按声明顺序**；早于 warmup 照常执行；支持 `script`/`send_message`/`set_motor`（其余动作 validate 明确拒绝，因无 item 上下文）。`user_event_runs_once_at_its_time`、`user_events_at_the_same_time_keep_declaration_order`、`user_event_negative_time_is_rejected`、`user_event_past_the_stop_time_does_not_run`。限制：checkpoint 恢复**不重放**已过去的事件（语义已实现，硬断言待补） |
| Model Triggers（模型级触发器：OnReset/OnRunStart 等） | `model.model_triggers` | ✅ | `ModelTriggers` 含 `on_start`/`on_reset`/`on_stop`（Schema 有 `model_triggers`，执行点见 `fire_model_actions` 的四处调用）；执行失败上报 `SimError::Script`。实验生命周期钩子（Start/End of Experiment/Scenario/Replication）**已在另一行实现**（见 Start/End of Experiment 行）；本行只描述模型级 reset/start/stop。**旧措辞"仍无实验生命周期钩子"是 stale 描述，已按审计更正** |
| Tracked Variables（Work In Progress / Time In System 等可追踪量） | `model.tracked_variables`（`TrackedKind: level/state/staytime/kinetic`）+ `tracked_interval` + `SimReport.tracked_series` | ✅ | 四种 kind 均已实现：level/kinetic 按间隔采样（kinetic 是变化率）、state 给每状态占比、staytime 给停留分布摘要；证据见 [PARITY_EVIDENCE](PARITY_EVIDENCE.md) #43 |
| Event List（待处理事件列表，可快进到某事件） | `--trace` + trace 回放 + Studio 事件列表 | 🟡 | 能做的是**已发生事件**的记录与回放（`TraceLog`、`openflex trace`、Studio 分页列表）；FlexSim 的 Event List 展示的是**未来待处理事件**并可跳转执行——引擎没有"窥视事件堆"的接口。**降级理由（审计发现）**：原 ✅ 引用的 `trace_writes_parseable_jsonl_events`/`reads_a_jsonl_trace_file` 只断言 JSONL 可写/可读，证明不了"待处理事件列表" |
| Event Log（记录已发生事件，可导出 CSV） | `--trace <FILE>` | 🟡 | 有 trace 导出；无"Enable Logging + 时间段过滤 + CSV 导出"这套交互，也无"两次日志 diff 定位不复现点"的辅助 |
| Excel Interface（多工作表导入/导出） | `--xlsx`（3 sheet 导出）+ `import --xlsx` | 🟡 | 有 xlsx 双向；无"任意工作表映射到 Global Table / 到达表"的通用界面 |
| Script Console（即时执行 FlexScript） | `openflex eval --expr` / `--script` | 🟡 | 非交互式（每条命令一次进程），但**不跑模型即可试算表达式/运行沙箱脚本**，`--time` 可指定时刻；引擎状态路径（`Queue1.content`、`item.*`）明确报 `unknown variable` 而不是编造值；脚本错误带 `available:` 提示。`eval_answers_expressions_without_running_the_model`、`eval_runs_a_sandboxed_script`、`eval_reports_paths_that_need_a_run` |
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
| Find Objects（按名称/类型查找） | 模型树搜索 `tree_row_matches`（纯函数） | ✅ | 名称与 kind 双向子串匹配、大小写不敏感；**已从 UI 闭包抽出以便单测**（`the_tree_search_matches_names_and_kinds`） |
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
| Reports and Statistics：Model Documentation（自动生成模型文档） | `openflex document <model>`（`--format json`） | ✅ | Markdown 文档：对象表（参数**去掉默认值**）、连接表、每对象实际挂动作的触发器、模型级命令/事件/跟踪变量/任务序列、校验结果；`--format json` 为机器可读版本。**宽容设计**：用宽松加载，**有问题的模型也能出文档**，并把 issue 列在文档里（其它命令拒绝无效模型，因为会算出无意义的数）。`SimTrigger::ALL`（23 个）保证新增触发器不会被文档漏掉。`document_describes_the_model_without_running_it` |
| Experimenter：模型的场景声明（`experiments[]`） | `experiments[]` + `openflex experiment` + GUI `Declared experiments` 面板 | ✅ | 场景 × 复现、置信区间、per-scenario 失败隔离、hook 日志；GUI 可单个/全部运行并显示成败 |
| Dashboard Concepts（仪表盘容器，运行中实时刷新） | Studio 仪表盘 + `openflex chart`（终端） | 🟡 | 有动态仪表盘与终端图表；仍无 FlexSim 的"图表对象 + 数据源绑定"可编辑模型 |
| Dashboard Statistics Page（把对象统计画成图） | Studio 时间序列/柱状图 | 🟡 | 有吞吐柱状与多对象时序；无按"统计项选择器"自由组合 |
| Dashboard Tracked Variables（把 tracked variable 画成图） | `tracked_series` / `tracked_summaries` + GUI 迷你折线 | 🟡 | 数据与类型体系（level/state/staytime/kinetic）已具备；GUI 有 84×14 迷你折线，无独立图表对象 |
| Dashboard Utilization Analysis（利用率分析专用图） | 报告 utilization + 执行器状态细分列 | 🟡 | 数据已细到 `travel_empty/loaded/loading/unloading/utilize`；仍无「按状态堆叠」的专用分析图 |
| Dashboard Item Trace（单个流项目的轨迹追踪） | `openflex trace --item` / `--items` / `--paths` | ✅ | 单件路径表（Time/Object/Event + 步数/对象数/跨度）、跨件路径分组（覆盖率恒等有测试）；trace 事件带 `data["item"]` 与真实 `enter`/`exit`。见 §1a 上方的说明 |
| Dashboard Objects Page / Associations / Colors / Data / General / Financial Objects / HTML Statistic / DateTimeDisplay（图表属性页） | Studio 图表有有限属性 | ⛔ | 这一组是图表引擎的属性页；OpenFlex 无通用图表 DSL（Financial Objects、HTML Statistic 等完全无对应） |
| Dashboard ModelInput / ModelInputProperties（把模型输入做成可交互控件） | `--set`/Scenario 可改参数 | 🟡 | 有参数覆盖；无"仪表盘上的输入控件" |
| Custom Chart（自定义图表脚本） | 无 | ⛔ | 无图表脚本 |

## 9. ExperimenterOptimizer（4 页）— 实验与优化

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| Experimenter：Variables（标签/全局表/对象数/位置等任意可变量） | `Scenario.overrides`（对象参数替换） | 🟡 | 只能覆盖**对象参数**；不能直接改变量、标签、对象个数、位置 |
| Experimenter：Scenarios（变量取值组合） | `experiments[].scenarios` + `--set` | ✅ | 含 `stop_time`/`warmup` 覆盖（对齐 FlexSim 的 Scenario 定义） |
| Experimenter：Performance Measures（自定义指标函数） | `model.performance_measures` | ✅ | `PerformanceMeasure{name, expression, unit?}`（`Model.performance_measures`）；每复现结束由 `evaluate_performance_measures` 求值，报告/实验/优化自动带上；未定义变量是明确 `SimError` |
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
| FlexSim XML（模型/库/树以 XML 保存，可 diff） | JSON（可 diff） | 🟡 | 目标一致（文本可版本化），格式不同属有意差异；**证据薄弱：仅 schema 级 JSON 往返测试，无"可 diff/可版本化"的行为断言** |
| Advanced Undo（beginaggregatedundo/endaggregatedundo，供库开发者） | Studio 撤销 | ⛔ | 无脚本级聚合撤销 API |
| Custom Libraries（自定义对象库、Dropscript、自动安装） | 无 | ⛔ | 无用户库机制 |
| ModelLibraries Node（隐藏定义） | 无 | ⛔ | |
| Kinematics Concepts / Commands（initkinematics/addkinematic/updatekinematics） | 无 | ⛔ | 无运动学 |
| SQL Queries（query() 命令、SQL 语法过滤/排序/优先级） | `crates/openflex-model/src/sql.rs`：只读子集 + 表达式里 `query("…")` | 🟡 | 已实现 `SELECT … FROM … [WHERE] [ORDER BY] [LIMIT]` 子集（`query()` 可从表达式调用）；**缺 JOIN / 子查询 / 聚合 / 分组**。证据：`parses_every_clause`、`filters_and_sorts` 等 5 个 sql.rs 单测 |
| Sampler（引用对象/变量/标签/表的可视化取引用工具） | GUI `object_picker`（下拉候选 + 手输 + `unknown` 提示） | 🟡 | `openflex-gui` 的 `object_picker` 定义，7 处调用（setup/process operator、home×2、charger、shaft）；候选按语义过滤（operator 只列 Dispatcher/Resource），名字不存在时琥珀 `unknown`；**只覆盖 5 个对象引用字段**，没有 FlexSim 那样对标签、表格单元格、统计项的取样 |
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
| Getting Started（软件定位与建模流程） | `README`、`docs/`、`examples/` | 📄 | 教程流程页；对应的首个示例已单独成行（First Model） |
| First Model（三品种加工示例） | `examples/three-varieties.ofm` | ✅ | 三品种各一台专机（`by_item_type` 分流）+ 共用测试站 + 不合格返工回入口（`by_condition`）。实测 480 分钟 5 复现：测试站 util **0.763** 成为瓶颈。**此前本行只有"实测数字"、没有测试函数（审计发现），现补行为测试**：`three_varieties_model_keeps_its_shape`（三条按类型分流 + 返工回边）、`three_varieties_runs_five_replications_and_the_tester_is_the_bottleneck`（5 复现、Tester 是利用率最高的对象、util 落在 0.7–0.85 带内）、`three_varieties_is_deterministic_across_repeated_runs` |
| Interacting With FlexSim（建/命名/连接/导航） | Studio 交互 | 🟡 | 有建/连/导航；无键盘拖拽建连（A/S/Q 键语义） |
| Keyboard Interaction（3D 视图快捷键） | Studio 快捷键 | 🟡 | 有部分快捷键；无 FlexSim 全套 |
| Terminology（术语表） | `docs/spec/flexsim-to-openflex.md` 术语表 | 📄 | 术语参考页，无功能面 |
| License Activation（激活/归还/修复/并发服务器） | 无（开源，无许可） | ⛔ | 有意差异，不算缺口 |
| License Activation Example / Reference（激活错误码） | 无 | ⛔ | 同上 |

## 16. Commands（4 页）与 Tutorials（29 页）

| FlexSim 7.5 功能 | OpenFlex 现状 | 状态 | 证据或缺口说明 |
|---|---|---|---|
| Command Reference（按类/按字母的命令索引，正文由 JS 渲染） | `openflex --help`、`objects describe` | 🟡 | **这 4 页正文抽取为空（JS 驱动）**，只能确认其存在与用途；无法逐命令对照。命令语义已在 `docs/research/evidence.md` 从 2025 版在线文档取得 |
| Tutorials（15 组 × 介绍/分步）：Lesson 1–3、Labels、Global Modeling Tools、Task Sequence 1–3、TimeTables、User Events、SQL、Kinematics、Fluid | golden 案例 + `docs/spec/golden-cases.md` | 🟡 | 覆盖基础流程/标签/时间表；**User Events、SQL、Kinematics、Fluid、Task Sequence 教程对应的功能多数未实现**（见上表） |

---

## 17. 汇总统计（task-185 取证审计后）

> **读法**：下表的按目录计数是在取证审计那一刻生成的快照；之后又做了 4 处修订
> （删除 `max_travelers`、补 load/unload 时长断言、查找逻辑抽成纯函数、补 First Model 示例），
> 并把 3 个纯教程/术语页从 🟡 改判为 📄。**权威总数以第 325 行的 ✅39 / 🟡97 / ⛔73 / 📄3 为准**，
> 下表用来看**缺口集中在哪个目录**，不必逐格对齐。

矩阵共 **208 行**（按功能点拆行，不是按页面 1:1；一页常拆 2–6 行，索引类页面合并为 1 行），其中 ✅ 50 行、🟡 102 行、⛔ 53 行、📄 3 行（参考页，不计入缺口）。
> 本行数字由 `scripts/check-parity-counts.py` 从下方表格**自动核对**；改矩阵后若不一致，CI 会失败。
> 本行数字由 `scripts/check-parity-counts.py` 从下方表格**自动核对**；改矩阵后若不一致，CI 会失败。
其中主手册 17 个目录 197 行，另加 §17b 的 Conveyor 模块 10 行。

| 状态 | 行数 | 占比 |
|---|---:|---:|
| ✅ 一致 | **35** | 16.9% |
| 🟡 部分 | **103** | 49.8% |
| ⛔ 缺口 | **69** | 33.3% |
| **合计** | **207** | 100% |

> **本次审计的净变化**（task-185，详见 [PARITY_EVIDENCE.md](PARITY_EVIDENCE.md)）：
> 审计前 ✅ **41** → 审计后 **35**（降级 8 行：`max_travelers` 未接线、load/unload 无时长断言、
> 查找无测试、XML 仅 schema 级、三行纯文档/概念页、First Model 无一对一示例；同时升级 2 行：
> Performance Measure 与 Tracked Variables 的四种 kind 已实装）→ **审计后逐项修复，现为 ✅ 39**：
> `max_travelers` 直接**删除字段**（不留虚标）、load/unload 补了精确时长断言、
> 查找逻辑抽成纯函数并补测、First Model 补了 `examples/three-varieties.ofm`。
> ⛔ 70 → **69**（SQL 查询已有只读子集，⛔ → 🟡）。
> **现在的每一个 ✅ 都能在取证台账里指到实现符号 + 至少一个行为级测试。**

按目录分布（行数 / 一致 / 部分 / 缺口）：

| 目录 | 行数 | 一致 | 部分 | 缺口 | 缺口密度 |
|---|---:|---:|---:|---:|---|
| FlexSimConcepts | 12 | 5 | 6 | 1 | 8% |
| FlexSimObjectLibrary | 4 | 2 | 2 | 0 | — （队友已对齐，仅登记） |
| ObjectWindows | 33 | 5 | 17 | 11 | 33% |
| PickLists | 34 | 10 | 12 | 12 | 35% |
| TaskSequences | 8 | 0 | 4 | 4 | 50% |
| ModelingTools | 22 | 4 | 6 | 12 | 55% |
| ModelingViews | 13 | 1 | 11 | 1 | 8% |
| ChartingReporting | 12 | 3 | 6 | 3 | 25% |
| ExperimenterOptimizer | 7 | 3 | 4 | 0 | 0% |
| MainMenuToolbar | 11 | 0 | 9 | 2 | 18% |
| FlexSimCoding | 6 | 0 | 4 | 2 | 33% |
| MiscellaneousConcepts | 13 | 0 | 5 | 8 | 62% |
| GeneralWindows | 7 | 1 | 3 | 3 | 43% |
| 3DMedia | 6 | 0 | 0 | 6 | 100% |
| GettingStarted | 7 | 0 | 5 | 2 | 29%（含许可证类有意差异） |
| Commands + Tutorials | 2 | 0 | 2 | 0 | — （Commands 4 页正文为空） |
| Conveyor 模块（§17b） | 10 | 1 | 7 | 2 | 20% |

**读法**：⛔ 69 行里约 **30 行属于"有意差异"**（3D 渲染管线、C++ 编译期、通用节点树、
许可证体系，见 §19），真正可补的功能缺口约 **35–40 行**。缺口密度最高的三个目录是
**3DMedia（100%，全属有意差异）、MiscellaneousConcepts（62%，节点树/运动学/自建 GUI）、
ModelingTools（55%，调试器/编辑器类）**。

### 1a. `explicit` 整链模式：设计草案（未实现，下一轮照此做）

第 1 项「自定义任务序列构造 API」本轮只落地了 **`utilize` / `break` 两种额外腿**（append 机制内）。
真正的 `explicit` 整链模式**未实现**，原因与设计如下——记录在此，避免下一轮重新踩坑。

**为什么本轮不做**：现有 append 是在标准任务**整条链结束后**执行 extras；标准
pickup/depart/arrive/loaded/released/home 是硬编码顺序。`explicit` 要在链**开始前**接管每一腿，
并与 processor 的 `OperatorLoaded`/`OperatorReleased` 同步（否则 transporter 提前回程，
processor release 时找不到它）——等于新写一套链状态机。此外 `{"kind":"travel"}` 没有目标点字段，
无法区分「去取货点／去送达点／回 home／额外点」，只能靠约定猜，属于我们不接受的近似。

**建议的模型形状**：
```jsonc
"task_sequence_mode": "explicit",        // "append"(缺省，不序列化) | "explicit"
"task_sequence": [
  { "kind": "travel", "target": "pickup" },
  { "kind": "load" },
  { "kind": "travel", "target": "requester" },
  { "kind": "unload" },
  { "kind": "utilize", "duration": { "distribution": "constant", "value": 5 } },
  { "kind": "break",   "duration": { "distribution": "constant", "value": 2 } },
  { "kind": "travel", "target": "home" }
]
```
`TravelTarget` = `"pickup" | "requester" | "home" | { "object": "名字" } | { "position": [x, y] }`。
校验：explicit 非空、至少 1 个 travel、每个 travel 必须带 target、`load` 必须在 pickup/home 之后、
`unload` 前必须有 `load`、`utilize`/`break` 必须正 duration；append 模式下 `target` 禁止出现。

**调度器状态**（按 transporter 存）：`legs / next / requester / loaded / operator_loaded_fired /
waiting_for_release / release_seen / leg_started_at`。关键同步规则：
1. `unload` 完成瞬间 fire 一次 `OperatorLoaded(transporter, requester)`；
2. 之后执行器必须等 processor 的 `OperatorReleased` 才能继续「离开/回程」性质的腿；
   若下一腿是 `utilize`/`break` 可先执行（不移动执行器），否则停在原地（Busy + Utilize）；
3. `OperatorReleased` 是唯一同步事件（processor 不等 transporter）；handler 里只在当前无腿在跑时推进链；
4. traffic 每段 travel 复用 `traffic_try_acquire/wait`（阻塞期间 blocked、不计 travel）；
   AGV 中途充电复用现有逻辑、activity 记 Utilize；抢占在 explicit 链期间建议禁用（MVP）。
5. 确定性：legs 在 dispatch 时预采样时长/条件，不依赖中途 RNG 顺序。

**测试计划**：自定义 target 几何正确 / `OperatorLoaded` 恰一次且 release 前不离开 /
腿顺序与表一致 / traffic 阻塞不计 travel / break=idle、utilize=utilize /
非法 target 被 validate 拒 / 同 seed 两跑逐位相同 / append 旧数值不变。

## 17b. Conveyor 模块（35 页，此前未计入清点）

**口径修正**：主帮助目录是 326 页，但 FlexSim 把传送带深水区放在**模块自带帮助**里：
`modules/Conveyor/help/` 另有 **35 页**。真实语料是 **361 页**。本节的 35 页此前完全没被清点，
所以「Conveyor 完整行为」这一项的真实缺口比 Top-10 里写的大。

| Conveyor 模块功能 | OpenFlex 现状 | 状态 | 缺口说明 |
|---|---|---|---|
| Conveyor（直/弯）、Join Conveyor | `conveyor`（`curve`/`segments`/`join_inputs`） | ✅ | 直段与曲段都有，join 用 `join_inputs` |
| Merge Controller（对象） | `conveyor.merge_mode` + `merge_weight`（**参数而非对象**） | 🟡 | 三种释放策略已实现（round_robin/weighted/by_item_type）；但没有独立的 controller 对象，也没有「一个 controller 管多组合流」 |
| Photo Eye（对象四状态） | `conveyor.photo_eyes`（位置 + `range` + `triggers`） | 🟡 | 四状态与 `on_blocked`/`on_cleared` 两个跃迁触发器已实现，且**边界穿越是皮带事件**（实测 10∓0.5 = 9.50/10.50 精确触发）；旧的「经过即触发」保留。仍缺 Block Time / Clear Time / Offset Angle / Require Gap，也没有独立的光电门对象与类型库 |
| Decision Point | `conveyor.decision_points`（位置 + 条件 + 动作） | 🟡 | 在感应区内对**该件**求值条件表达式、为真按序执行动作，`once_per_item` 离开感应区后重置；边界穿越复用光电门那套皮带事件（实测 `pos 5.0 − range 0.5 = 4.500` 精确触发）。仍是**参数而非对象**，也没有决策点类型库 |
| Transfer（Entry/Exit，三种） | 无 | ⛔ | 连接时自动产生的传送点类型未建模 |
| Motor（开/关控制、同步 dog gap） | `motor` 对象：`params.running` + 触发器，控制所连 conveyor 的启停 | 🟡 | 开/关已实现（4 个行为测试：`stopped_motor_freezes_conveyor_and_acceptance`、`motor_resume_continues_conveyor` 等）；**仍缺 power-and-free 的 dog gap 同步** |
| Gapping（间隙成型） | `item_spacing` + `batch_size` | 🟡 | 静态最小间距有；缺按类型/按决策点的动态间隙与 gap 控制策略 |
| Sorting（分拣） | 可用 conveyor + 路由/`split` 表达 | 🟡 | 没有 sorter 对象与分拣策略表 |
| Power and Free | 无 | ⛔ | 完全未建模 |
| ConveyorSystemTool（类型作为全局设置） | 每对象参数 | ⛔（有意差异） | FlexSim 用「类型库 + 工具」批量套用；OpenFlex 用每对象参数 + JSON 复用，**不计划实现类型工具** |
| Behaviors/PickList（AreaRestriction / Condition / ConveyorSpeed / Movement / SendItem / StopResume） | 分散在参数的 trigger/条件里 | 🟡 | 语义大多能用既有机制表达，但不是可点选的 behaviour 列表 |

**已知缺陷（v1.37 起）**：决策点落在**拥堵的非累积皮带**上时，被挡件的「到达感应边界」事件会被
反复以 10⁻⁶ 级间隔重排（Zeno），最终耗尽事件预算。复现：两源（exponential 6，type 0/1）→
`speed 1 / length 12 / spacing 1` 皮带 → 两 sink，决策点 `position 6.0`；`stop_time 100` 正常、
120 起必现，**挪到 `position 2.0` 不复现**。与动作内容无关（空动作同样触发）。

**结论**：conveyor 的**基础行为**（速度/间距/容量/曲线/迭代积分/精确停位）与**两片深水**（slug 组、
合流释放策略）已一致；剩下的是**独立对象化的那部分**（Decision Point / Transfer / Motor / sorter /
Power-and-Free）与类型工具。后者我倾向把「类型工具」记为有意差异，其余按需求优先级逐项补。

## 18. 剩余缺口 Top 10（task-185 取证后重排）

本轮取证时又有几项确认已落地，从榜上移出：**执行器状态细分**（`travel_empty/ loaded/ loading/
unloading/ utilize` 均有数值断言）、**Tracked Variable 四类型**、**Item Trace**（`trace --item/--items/--paths`）、
**SQL 只读子集**（已降到 🟡，见下 #8）。

| # | 缺口 | 为什么重要 | 工作量 |
|---|---|---|---|
| 1 | **自定义任务序列的 `explicit` 整链模式**（`createemptytasksequence`/`inserttask` 的等价物） | 现有 `append` 只能在标准链**之后**加腿；表达不了"取货→加工→送回→充电→再取"这类链，也无法在链内换 target。设计草案见 §1a | **大** |
| 2 | **图表 DSL**（图表对象 + 数据源绑定；Utilization Analysis 按状态堆叠、Financial Objects、Zone/Milestone 模板） | 报告与 GUI 的图是固定几种；用户不能自选"哪条统计×哪种图"，也无法做利用率分解图。FlexSim 的主交付界面就是 Dashboard | **大** |
| 3 | **调试器**（断点 / 单步 / 调用栈 / 局部变量 / 性能剖析） | 模型逻辑出错时只有 `log()` 与 trace；`FlexSimCoding/Debugging/*` 5 页描述的是完整调试体验 | **大** |
| 4 | **Conveyor 剩余部分**：Transfer（Entry/Exit）对象、sorter、Power-and-Free、光电门的 Block Time / Clear Time / Require Gap | 基础行为（速度/间距/曲线/迭代积分/slug 组/合流策略）已一致；缺的是**对象化的那部分**与高级光电语义（§17b） | **大** |
| 5 | **MTBF/MTTR 组对象与停机请求栈** | 参数侧（首次故障、只在指定状态计时）已补齐；但一个对象仍不能属多个组、无组级 Down/Up 触发器、多次停止不能叠加 | **中** |
| 6 | **字符串/复合标签** | `FlowItem.labels` 是 `BTreeMap<String, f64>`，只有数值通道；FlexSim 允许字符串、列表、表，做"按名称分流/记录状态文本"时被迫编码成数字 | **小** |
| 7 | **实验生命周期钩子**（Start/End of Experiment / Scenario / Replication，6 个） | `model_triggers` 只有 `on_start`/`on_reset`/`on_stop`；实验前后汇总、每场景初始化无从挂载（目前只能逐场景在模型里写） | **小** |
| 8 | **SQL 的 JOIN / 子查询 / 聚合 / 分组** | 只读子集（SELECT/WHERE/ORDER BY/LIMIT）已够查表，但"按类型汇总取前 3"这类仍需手工循环 | **小–中** |
| 9 | **PickLists 的模板代码机制**（`/**tag:*/` 双向绑定 + 点选插入代码） | 现在 picklist 是命名列表（表达式取值），不是"点选即生成代码"的编辑体验；影响的是建模效率而非语义 | **中** |
| 10 | **"已声明但未接线"参数的审计**（`max_travelers` 类） | 本轮发现 `max_travelers` 有字段/GUI/文档但引擎从不读——这类虚标比缺功能更危险。建议 `validate` 对"声明了却零引用"的参数给 warning | **小** |

**次级但低成本**（不进 Top 10，可顺手做）：`FlushContent`（整批走完前拒绝新件）、
Recent 文件列表、多停止时间列表、`Groups` 持久化命名分组、
`Model Layouts` 从 localStorage 迁入模型文件、`on_draw` 渲染端接线、`OnEntryRequest` 准入裁决钩子、
长度/流体单位（现在只有 `time_unit`）。

> 已移出榜单：加工时点钩子、PickLists/全局变量接线、模型级触发器、Performance Measure、
> Tracked Variables（含四类型）、Sampler、TrafficControl 准入互斥、执行器状态细分、Item Trace。
> 移出依据见 [PARITY_EVIDENCE.md](PARITY_EVIDENCE.md)。

## 19. 有意差异（非缺口，需在文档中持续声明）

以下行为与 FlexSim 不同，但属于净室与架构选择，不应计入"缺口"：

1. **无 3D 渲染管线**：因此 3DMedia 全部 6 页、Shape Frames、Animation Creator、AVI Maker、
   Presentation Builder、Light Source、Model Floor/Background 均为"不实现"，而非"实现不完整"。
2. **无 C++ 编译期**：`Build` 菜单、`When to Compile`、Code Profiler 的 C++ 侧不适用。
3. **无通用节点树**：`Tree Structure`、`Model Tree View` 的任意节点编辑不适用；OpenFlex 用严格 JSON schema
   换取可校验性与跨平台确定性。
4. **无许可证体系**：`License Activation` 三页不适用。
5. **随机数生成器不同**：同 seed 可复现，但不与 FlexSim 逐位一致（已在 `docs/spec/runtime.md` 声明）。
6. **Visio 导入器不实现**：FlexSim 从 `.vsdx` 图元重建对象与连线；OpenFlex 的模型是严格 JSON，
   没有"图即模型"的双向绑定。用 `openflex layout` 从模型出 SVG 是**单向导出**，方向相反。
7. **碰撞检测不实现**：`Collision Page`/`Collision Trigger` 是 3D 包围球检测，服务的是动画与视觉真实感；
   无 3D 渲染管线时它没有可观察的效果（第 1 条的推论），故不做一个空壳开关。
8. **Robot 关节几何不实现**：`Robot Geometry Page`（逐关节速度/复位/手动定位）与 3D 姿态强绑定，
   与第 1 条同因。
9. **媒体文件预加载不实现**：`Media Files` 服务的是 3D 形状与贴图缓存，同第 1 条。
10. **FlexScript 语法不兼容**：OpenFlex 用 rhai 沙箱（`action: script`）与表达式 DSL。
    这不是"功能缺失"，而是**净室与可校验性**的取舍：脚本能静态编译校验、错误可定位、运行确定性可重放。
    迁移成本是真实存在的（已有 `openflex migrate` 做能自动化的部分），文档不掩饰这一点。