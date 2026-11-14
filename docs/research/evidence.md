# 证据台账（FlexSim 公开文档 → openflex 规格）

- 抓取日期：**2026-11-14**（本轮全部页面同一批次抓取，时间 15:02–15:20 +08:00）
- 抓取对象：FlexSim **2025** 官方帮助（Autodesk 托管），书签 `FLEXSIMIN / 2025 / ENU`，手册 `FlexSim-User-Manual`
- 抓取方式：先确认站点为 JS 渲染 SPA（`?guid=` 路由），再用 **Playwright MCP** 渲染取 `document.body.innerText` 验证正文；随后发现该 SPA 通过 `data/toctree.json` 暴露完整目录、通过静态 `cloudhelp/.../files/**.html` 暴露每页正文，于是对 **154 个页面**做 `curl` 批量抓取并转纯文本（`scripts` 未入库，产物在会话临时目录）。
  - 结果：目录树 1051 个主题页 → 选取 154 页 → 154/154 抓取成功（HTTP 200），无 403、无登录墙。
  - 备选路径 `web_fetch` 对该站基本无效（拿到 SPA 空壳），已放弃。
- URL 形式：站内可见地址 `https://help.autodesk.com/view/FLEXSIMIN/2025/ENU/?guid=<GUID>`；同页正文原始地址 `https://help.autodesk.com/cloudhelp/2025/ENU/FlexSim-User-Manual/files/<目录>/<GUID>.html`。台账只登记前者（访客可点开的规范地址），需要复核正文时把 `<GUID>` 拼进后者即可。

> **净室声明**：本台账全部证据来自公开可访问的厂商帮助文档（Creative Commons BY-NC-SA 3.0，见页面页脚）。**未**下载、解包或反编译任何 FlexSim 安装包，**未**使用私有源码、破解版或反编译产物。本文档只记录"访客可见"的行为描述，不复制实现代码。

- 置信度口径：
  - **高** = 官方参考页明确写出的行为/参数/事件/状态/统计定义，或明确给出的数值公式。
  - **中** = 官方教程/概念页的叙述性描述，或从多个页面交叉推出但无单一权威句。
  - **低** = 官方页面只给出名称/未见定义，或由术语推断（会在正文标注"待确认"）。

---

## A. 3D 对象库与对象参数

### E01 — Overview of 3D Library Objects
- URL：`https://help.autodesk.com/view/FLEXSIMIN/2025/ENU/?guid=FlexSim_User_Manual_using3dobjects_overview3dobjects_html`
- 摘要：3D 库按 **Fixed Resources**（Source/Queue/Processor/Sink/Combiner/Separator/MultiProcessor/BasicFR/Storage Object）、**Task Executers**（Dispatcher/TaskExecuter/Operator/Transporter/Elevator/Robot/Crane/ASRSvehicle/BasicTE）、**Conveyors**、**Warehousing** 等分类列出，逐个给出一句话职责说明（"Source 创建并释放流项目""Queue 存储直到下游就绪""Processor 模拟 setup+process 时间延迟""Sink 移除流项目"等）。
- 支撑：objects.md 全篇的对象清单与一句话语义；flexsim-to-openflex.md 的 `kind` 取值。
- 置信度：高

### E02 — Fixed Resources Concepts
- URL：`https://help.autodesk.com/view/FLEXSIMIN/2025/ENU/?guid=FlexSim_User_Manual_reference_3dobjects_fixedresources_fixedresourcesconcepts_html`
- 摘要：给出固定资源的**完整收发流水线**：①打开输入端口/找可接收项（Pull 模式下先调 Pull Strategy 得端口号，0=全开；再对该已释放项调 Pull Requirement，1=接收、0=跳过并等下一件）②处理项 ③释放并经 Send To Port 决定输出端口（>0 指定端口、0=全开、-1=不释放，需后接 `releaseitem()`）④移交（未勾 Use Transport 立即移交；勾选则调 Request Transport From，返回 TaskExecuter/Dispatcher 引用后自动生成默认任务序列；返回 0 表示用户自建任务序列）。含 `nroftransportsin/nroftransportsout` 语义、frload/frunload 任务、Pull 与 Send To Port 需同时成立（v6 起）。**并逐条定义事件参数**（On Entry/On Exit/On Message/Pull Requirement/Pull Strategy/Send To Port/Transport Reference/Pick Offset/Place Offset/On Stop/On Resume/On Content Change/On Input/Output Change/On Staytime Change/On State Change/On Reset/On Draw）与**统计定义**（Throughput=输入/输出各 +1；Content=件数，容器件只算 1；Staytime=exit time − entry time）。
- 支撑：logic.md 全部触发器/端口/拉取章节；statistics.md 的 Content/Throughput/Staytime 定义。
- 置信度：高

### E03 — Source（对象参考）
- URL：`...?guid=FlexSim_User_Manual_reference_3dobjects_fixedresources_source_html`
- 摘要：三种到达模式：**Inter-Arrivaltime**（调用 inter-arrivaltime 函数，返回值=**上一件离开到下一件到达**的等待时间；可设首次到达是否在 0 时刻）、**Arrival Schedule**（表驱动：ArrivalTime/ItemName/Quantity/标签列，Repeat Table 时首行只用于第一轮，可作为预热）、**Arrival Sequence**（无时间，逐行按"上批全部离开"推进）。事件：On Creation（Item, Row Number）、Inter Arrival Time；状态：**Generating**（无项、等下次创建）、**Blocked**（已创建但未离开）；统计：Output（累计离开数）、Staytime（被阻塞时滞留在 source 内的时间）。属性表：ArrivalStyle/FlowItemClass/InterArrivalTime/RepeatTable/TimeZeroArrival/ArrivalSchedule/ArrivalSequence。
- 支撑：objects.md §Source；golden-cases 的"0 时刻首发"设定；runtime.md 到达语义。
- 置信度：高

### E04 — The Source Panel
- URL：`...?guid=FlexSim_User_Manual_reference_propertiespanels_fixedresourcepanels_source_html`
- 摘要：面板字段与语义：FlowItem Class（创建哪种流项目类）、Arrival Style（三模式）、Inter-Arrival Time 模式下有 **Arrival at time 0** 复选（勾选则 0 时刻创建 1 件，下一次在第一个间隔后）与 Inter-Arrivaltime 函数；Arrival Schedule/Sequence 模式下有 **Repeat Table**、Edit Table；Arrivals 窗口列：ArrivalTime（模型时间单位）、ItemName、Quantity、Label 列（标签名+初值）。Repeat Table 语义：最后一行的到达时刻之后 **0 秒**衔接第一行，且第一行的到达时刻只在第一轮生效（可当预热用）；若要在末行与首行之间加间隔，就在末尾追加一行 Quantity=0。
- 支撑：objects.md §Source 参数表；golden-cases 的到达计数推导。
- 置信度：高

### E05 — Queue（对象参考）
- URL：`...?guid=FlexSim_User_Manual_reference_3dobjects_fixedresources_queue_html`
- 摘要：队列状态：**Empty / Collecting（攒批中）/ Releasing（释放中；不攒批且有件时也是此状态）/ Waiting for Transport**。统计沿用固定资源标准统计。属性表：BatchSize/DoBatching/FlushContent/ItemPlacement/LIFO/MaxBatchTime/StackBaseZ。
- 支撑：objects.md §Queue；statistics.md 状态占比表。
- 置信度：高

### E06 — The Queue Panel
- URL：`...?guid=FlexSim_User_Manual_reference_propertiespanels_fixedresourcepanels_queue_html`
- 摘要：Maximum Content（可容纳件数上限）、Item Placement（Stack Vertically / Horizontal Line / Stack inside Queue / Do Nothing，纯视觉但影响"哪件最久"的呈现）、Stack Base Z、**LIFO**（勾选=后进先出，否则 FIFO）、**Perform Batching**（攒够 Target Batch Size 或到 Max Wait Time 才放行；未勾选则下游可用即走）、**Target Batch Size**（批大小，但**逐件**送下游）、**Max Wait Time**（0=无限等待）、**Flush contents between batches**（整批离开前不许新件进入）。
- 支撑：objects.md §Queue 参数语义；golden-cases 的批量案例。
- 置信度：高

### E07 — Processor（对象参考）+ The Processor Panel
- URL：`...?guid=FlexSim_User_Manual_reference_3dobjects_fixedresources_processor_html`；`...?guid=FlexSim_User_Manual_reference_propertiespanels_fixedresourcepanels_processor_html`
- 摘要：处理逻辑=**Setup Time 后接 Process Time**；两者均可要求 1..n 个操作员（Use Operator(s) + 数量），**Same as Setup** 决定 process 是否复用 setup 的操作员，Operator 字段返回 operator/dispatcher 引用，另有 Priority（任务序列优先级，同优先级 FIFO）与 Preemption（抢占）。参数表：AnimateItems/NumProcessOperators/NumSetupOperators/OperatorPreempt/OperatorPriority/OperatorRef/ProcessTime/SetupTime/UseProcessOperators/UseSameNumOperators/UseSetupOperators。状态：**Idle / Setup / Processing / Blocked / Waiting for Operator / Waiting for Transport**。
- 支撑：objects.md §Processor；statistics.md 状态定义；logic.md 操作员语义。
- 置信度：高

### E08 — Sink（对象参考）
- URL：`...?guid=FlexSim_User_Manual_reference_3dobjects_fixedresources_sink_html`
- 摘要：Sink 恒处于 collecting 状态（除非被停止），不做状态统计，只看 **Input**（累计进入件数）。无自有面板，仅 Statistics/Visuals/Labels/Input/Ports/Triggers。
- 支撑：statistics.md"端到端吞吐"；golden-cases 用 Sink.input 作为产出计数。
- 置信度：高

### E09 — Combiner（对象参考）
- URL：`...?guid=FlexSim_User_Manual_reference_3dobjects_fixedresources_combiner_html`
- 摘要：**先从输入端口 1 收恰好 1 件**，之后才允许其它端口进件；Components List 指定端口 2、3…各收多少件（第 1 行对应端口 2），由连接自动生成。攒齐后才开始 setup/process。三种模式：**pack**（把端口 2+ 的件移入端口 1 的容器件，再释放容器件）、**join**（除端口 1 的件外全部销毁）、**batch**（攒齐并完成 setup/process 后把全部件释放）。Combiner **不使用** Pull Strategy。运输场景下，容器件在途期间只允许 1 件在途。手动移出容器件时必须给非 0 端口号（端口 0 被解释为"零件装入容器"）。属性表：CombineMode/ComponentsList。
- 支撑：objects.md §Combiner；golden-cases 的装箱案例。
- 置信度：高

### E10 — Separator（对象参考）
- URL：`...?guid=FlexSim_User_Manual_reference_3dobjects_fixedresources_separator_html`
- 摘要：收到 1 件后执行 setup/process；**unpack 模式**：把容器内 `qty` 件移入自身并全部释放，最后释放容器件 → 总产出 = qty + 1；**split 模式**：复制 `qty − 1` 次 → 总产出 = qty（语义差异必须保留）。unpack 顺序为**从后往前**（若需指定顺序，在 entry 触发器里设 rank）。附加事件 Split Unpack Quantity（在 on process finish 之后触发，决定拆分/解包数量）。属性表：SeparateMode/SeparateQty。
- 支撑：objects.md §Separator（含 qty 边界差异）。
- 置信度：高

### E11 — MultiProcessor / BasicFR（对象参考）
- URL：`...?guid=FlexSim_User_Manual_reference_3dobjects_fixedresources_multiprocessor_html`；`...?guid=FlexSim_User_Manual_reference_3dobjects_fixedresources_basicfr_html`
- 摘要：MultiProcessor 维护**多条 process 列表**，每条有 Process Time/操作员数/Operator/Priority/Preemption；状态含 Idle、**每道工序一个用户自定义状态**（需在 State Table 里建自定义状态表引用该 state profile）、Blocked、Waiting for Operator、Waiting for Transport。BasicFR 是"空壳固定资源"，只提供固定资源标准事件/状态（Idle/Setup/Processing/Blocked/Waiting for Operator/Waiting for Transport）与 Input/Output/Ports/Triggers 面板，逻辑完全由用户触发器实现。
- 支撑：objects.md §MultiProcessor、§BasicFR。
- 置信度：高

### E12 — The Input Panel / The Output Panel
- URL：`...?guid=FlexSim_User_Manual_reference_propertiespanels_fixedresourcepanels_input_html`；`...?guid=FlexSim_User_Manual_reference_propertiespanels_fixedresourcepanels_output_html`
- 摘要：Input 面板：**Pull**（是否主动拉取）、**Pull Strategy**（返回输入端口号；仅在该对象"准备接收下一件"时求值——容量 1 的 Processor 即每件离开后求值一次）、**Pull Requirement**（对 **ready 状态**（FRSTATE_READY）的项求值，返 1/0）。Output 面板：**Send To Port**（返回输出端口号；0=打开全部并按第一个可接收者发送）、**Use Transport**（勾选后多出运输引用字段，默认取 `current.centerObjects[1]`）、**Priority**、**Preempt**（含"可让运输器超载"的说明）。
- 支撑：logic.md §Send To Port / §Pull；flexsim-to-openflex.md 的 `connections` + 策略映射。
- 置信度：高

### E13 — Task Executer Concepts
- URL：`...?guid=FlexSim_User_Manual_reference_3dobjects_taskexecuters_taskexecuterconcepts_html`
- 摘要：任务执行器=可移动资源超类，能执行**任务序列**与**偏移行程**；TaskExecuter 本身又是 Dispatcher 的子类（可当队长）。收到任务序列时：无活动序列→立即执行；新序列抢占且优先级更高→抢占；否则走 Dispatcher 排队逻辑（按 Queue Strategy 优先级插队）。用户可定义 **Capacity**（最大内容；仅在 BREAK 任务判定"是否已满"时真正起作用——自建任务序列可超载）、**Max Speed / Acceleration / Deceleration**（长度/时间、长度/时间²）、load/unload 的 Travel Offsets。状态：Idle/Travel Empty/Travel Loaded/Offset Travel Empty/Offset Travel Loaded/Loading/Unloading/Utilize/Blocked。统计：Throughput=装载 +1、卸载 +1；Staytime=unload−load；**Travel Distance** 累计里程。事件含 On Receive Task Sequence / On Start Task / On Finish Task / On Preempt / Break To / On Resource Available / Pass To / Queue Strategy 等。属性表：Acceleration/AvailableOnStart/BreakTo/Deceleration/DoAnimations/ExtensionSpeed/FlipThreshold/LiftSpeed/LoadTime/MaxSpeed/Navigator/OffsetTravel/PassTo/QueueStrategy/RotateTraveling/UnloadTime。
- 支撑：logic.md §任务执行器；objects.md §Operator/§Transporter/§Dispatcher。
- 置信度：高

### E14 — Operator / Transporter / Dispatcher（对象参考）
- URL：`...?guid=FlexSim_User_Manual_reference_3dobjects_taskexecuters_operator_html`、`..._transporter_html`、`..._dispatcher_html`
- 摘要：Operator 被对象调用以在 setup/process/repair 期间"被占用"，释放后可去别处；可搬运件；偏移行程在"有涉及件"时只走 x/y 平面且把总距离减去 `xsize(operator)/2 + xsize(item)/2`。Transporter 主要搬运，叉车可升降到货架位，可一次多件。Dispatcher 用**输出端口**把任务序列分派给"团队"成员：收到序列先调 **Pass To**（返回端口号；0=按 Queue Strategy 入队；−1=不作为）；Queue Strategy 返回排序优先级，高优先级插到队首。
- 支撑：objects.md §Operator/§Transporter/§Dispatcher 参数与语义。
- 置信度：高

### E15 — Storage Object / Rack（仓储）
- URL：`...?guid=FlexSim_User_Manual_reference_3dobjects_warehousing_storageobject_html`；`..._warehousing_rack_html`
- 摘要：Storage Object 是抽象父类，把件存放在**按 bay（X）→ level（Z）→ slot** 组织的二维货位网格；默认**关闭**固定资源功能，可用 Flow 面板或连一条 A 连接启用；启用后按 max content 收件，每件执行 **Minimum Dwell Time** 定时器，超时后释放。Rack 特化：X 向为 bay、Z 向为 level，**拣选面在其 Y− 侧**；Rack 自身不实现状态，用 content 图取统计；附加属性 ExtendColumns/PickPlaceYOffset/ShelfTilt。
- 支撑：objects.md §Rack/§Storage；映射表中 rack 类对象标注"v1 不实现"。
- 置信度：高

### E16 — Conveyor 系列（Straight and Curved / Merge Controller）
- URL：`...?guid=FlexSim_User_Manual_reference_3dobjects_conveyors_straightcurvedconveyor_html`；`..._conveyors_mergecontroller_html`
- 摘要：直线/曲线输送机可模拟皮带或辊道；事件 **On Item Bump**（前后 slug 相撞合并）；属性含 Speed/Acceleration/Deceleration/StartLocation/EndLocation/Width/**UseVirtualLength/VirtualLength**/**IsAccumulating**/**IsSlugBuilder**/CatchCondition/EntrySpace… 等。Merge Controller 用于定义并流车道放行策略，一般只配合 slug-building 输送机；不实现状态、不统计，仅 Center/In/Out 关系。
- 支撑：objects.md §Conveyor（标注 v1 简化/不实现范围）。
- 置信度：高

---

## B. 逻辑：触发器 / 端口 / 标签 / 消息 / 故障班次

### E17 — The Triggers Panel
- URL：`...?guid=FlexSim_User_Manual_reference_propertiespanels_generalpanels_triggers_html`
- 摘要：Triggers 面板初始为空，按 Add 弹出**该对象可用的触发器菜单**（各对象集合不同，定义在该类对象的 Concepts 页 Events 节）；每个触发器可配"预置逻辑选项"或直接写 FlexScript。
- 支撑：logic.md §触发器总表"按对象分类"的组织方式。
- 置信度：高

### E18 — Key Concepts About Events（事件/状态/触发器）
- URL：`...?guid=FlexSim_User_Manual_modellogic_eventsstatestriggers_keyconceptsevents_html`
- 摘要：FlexSim 是**离散事件仿真**：把业务系统表示为按时间推进的一系列离散事件（到达、运输、加工、故障…）；对象与流项目交互时**事件顺序固定**（Processor 的三段：取件→加工→送件）；状态因事件而变。
- 支撑：logic.md 开头的仿真范式说明；runtime.md 的事件调度模型。
- 置信度：高

### E19 — Key Concepts About Labels
- URL：`...?guid=FlexSim_User_Manual_modellogic_labels_keyconceptslabels_html`
- 摘要：标签三要素 **Owner（流项目/token/3D 对象）+ Name（创建时定名、运行中不变）+ Value（可为文本/数字/引用/数组，运行中可变）**；典型用途：排序与条件路由（按标签值分流/从列表筛选）、表示对象状态变化、给资源打标记（如"是否在休息"）。
- 支撑：logic.md §标签；flexsim-to-openflex.md 的 `labels` 字段设计。
- 置信度：高

### E20 — Key Concepts About Ports
- URL：`...?guid=FlexSim_User_Manual_connectingflows_ports_keyconceptsports_html`
- 摘要：端口=对象间传递流项目的接触点。两类最常用：**输入/输出端口（A-connect，快捷键 A）**——上游 output → 下游 input，图形为红/绿三角；**中心端口（S-connect，快捷键 S）**——抽象引用关系，用于：运输（固定资源使用连在其中心端口的任务执行器）、setup/process 需要操作员、一般引用通信。另有 extended connection（仅网络节点/Traffic Control 使用）。复杂多对多建议改用 List 或 Process Flow。
- 支撑：flexsim-to-openflex.md 的 `connections`（A 连接）与 `resource_refs`（S 连接）区分。
- 置信度：高

### E21 — MTBF/MTTR
- URL：`...?guid=FlexSim_User_Manual_reference_tools_mtbfmttr_html`
- 摘要：对一组对象设定随机故障/修复。函数页签四个 picklist：**First Failure Time**（首次故障时刻；返回负数=忽略首次）、**Down Time**（=MTTR，决定停机多久，所有成员**同时**恢复）、**Up Time**（=MTBF，定义为"上次恢复→下次开始停机"的跨度，可只统计特定状态，例如仅在 Processing 时计时）、**Down Behavior**（可选，替代下列函数）以及 **Down Function / Resume Function**（对每个成员各执行一次）。可设 **Break down members individually**：勾选则 First Failure/Up/Down 对每个对象**各自**求值，否则整组共用一个时间。Enabled 可被 Experimenter 按场景开关。
- 支撑：logic.md §故障；golden-cases 的可用率推导；runtime.md 的"停机请求"语义。
- 置信度：高

### E22 — Time Tables
- URL：`...?guid=FlexSim_User_Manual_reference_tools_timetables_html`
- 摘要：按**计划**制造停机（班次/休息/换班）：每行一个 down time + duration（模型时间单位）；命中时先对每个成员执行 **Down Function**，再执行一次 **On Down**（传入成员表与该行）；时段结束时先逐个 **Resume Function**，再执行一次 **On Resume**。可为每行指定 **down state 与 state profile**（配合 `stopobject()/resumeobject()`）；跨日/周边界的相接时段若状态或 Down Behavior 相同会**自动合并**。可关联 **Down Behavior** 共享停机动作。
- 支撑：logic.md §班次；flexsim-to-openflex.md 的 `timetable`/`shift` 设计。
- 置信度：高

### E23 — Down Behavior
- URL：`...?guid=FlexSim_User_Manual_reference_tools_downbehavior_html`
- 摘要：可复用的"停机事件"定义（Down Function / Resume Function / On Down / On Resume + 标签），可被 Time Table 与 MTBF/MTTR 引用，用来表达"午休""班次结束"等语义。
- 支撑：logic.md §故障与班次的可复用动作模型。
- 置信度：高

### E24 — The Statistics Panel / The State Table / Tracked Variables
- URL：`...?guid=FlexSim_User_Manual_reference_propertiespanels_generalpanels_statistics_html`；`..._tools_statetable_html`；`..._tools_trackedvariables_html`
- 摘要：对象状态历史由**分类跟踪变量（categorical tracked variable）**记录；State Table 定义状态编号→名称/颜色的映射，支持自定义 state profile（如 MultiProcessor 的用户自定义状态）；Tracked Variables 支持 Level/State/Staytime 等类型并可在 Dashboard 出图。
- 支撑：statistics.md 的状态占比与状态表映射设计。
- 置信度：中（页面偏工具操作，未给公式）

### E25 — Model Triggers
- URL：`...?guid=FlexSim_User_Manual_reference_tools_modeltriggers_html`
- 摘要：提供模型级触发器（如模型重置、运行开始/结束等）挂载点，用于全局初始化/收尾逻辑。
- 支撑：flexsim-to-openflex.md 的 `run.on_reset`/`on_start` 钩子建议。
- 置信度：中

---

## C. 运行、时间与随机性

### E26 — Model Settings
- URL：`...?guid=FlexSim_User_Manual_reference_generalmodelsettings_modelgeneralproperties_html`
- 摘要：模型级设置含 **Decimal Precision**、**Random Number Streams**（重置时初始化的随机数流数量，范围 0..N）、**Length Units / Time Units / Fluid Units**（模型的时间/长度/流体单位；页面只写"displays the time units of the model"）、**Time Format / Date Format**（格式符表，二者不可混用）、**Embed media with model**（把 3D/图片嵌入模型文件，正文提到 `.fsm`）、**Disable AutoSave**、**Use Legacy SQL Nulls**、**Encrypt Model**（密码）。
- 支撑：runtime.md §时间单位与 §随机数流；objects.md 的单位字段说明。
- 置信度：高（设置项清单）/ 中（时间单位可选取值未列出，见"未获取"节）

### E27 — Running Simulations
- URL：`...?guid=FlexSim_User_Manual_flexsimui_runningsimulations_html`
- 摘要：运行是"推进事件"的过程。要点：**达到结束时间只是暂停而非终止**（再按 Run 会继续）；**同样的模型每次运行结果完全相同**（随机变量使用相同的随机数流，便于对照实验）；**Step** 推进到下一个事件，**Skip** 跳过同一时刻的全部事件，**Fast Forward** 用最大速度跑到下一个 stop time 后恢复原速度与刷新；Stop 建议理解为暂停；**Stop Times** 可多个且各自可启用；**Warmup Time** 到达后**清空已收集的统计**（因为预热期数据会失真）；**Run Speed 默认 4 模型秒/真实秒**，可 1.00 / Maximum / Custom，Display Mode 可改为"模拟秒每真实秒"。
- 支撑：runtime.md 全篇（运行控制、预热语义、速度）；statistics.md 预热对统计的影响。
- 置信度：高

### E28 — Simulation Control Bar
- URL：`...?guid=FlexSim_User_Manual_reference_tools_simulationcontrolbar_html`
- 摘要：按钮与快捷键明细：Reset（Ctrl+←，时钟归零/回到起始日期，清空件并复位）、Run（Ctrl+Space）、Stop（Ctrl+Space，等同暂停）、Fast Forward（Ctrl+Shift+↑）、Skip（Ctrl+Shift+→，跑到下一事件并跳过同一时刻其余事件）、Step（Ctrl+→，单事件）。Run Time 区：模拟时钟（从 0 开始）、Display Mode、**Start Time**（把真实日期时间映射到 0 时刻）、**Warmup Time**、**Stop Times**（可多个，未启用则无限运行）。Run Speed 区：滑块、速度表、1.00/Maximum/Custom。
- 支撑：runtime.md §运行控制命令与 CLI 映射（`reset/run/stop/step/skip/ff`）。
- 置信度：高

### E29 — Model Repeatability
- URL：`...?guid=FlexSim_User_Manual_bestpractices_modelrepeatability_html`
- 摘要：可复现的前提：①勾选 **Repeat Random Streams**（Statistics 菜单）→ 每次reset/run 随机数序列相同；②任务执行器需设 reset position；③Custom Draw 触发器不得用随机函数；④查询件位置前必须 `updatelocations()`（否则结果随运行速度/硬件变化）；⑤变量要初始化；⑥持久数据（Global Table、对象标签）不会自动复位，需在 OnReset 里恢复（标签可勾 Automatically Reset Labels）；⑦不能用内存地址/真实时钟等不可复现数据。
- 支撑：runtime.md §随机性与复现；statistics.md 的复现实验设计。
- 置信度：高

### E30 — Using the Distribution Chooser
- URL：`...?guid=FlexSim_User_Manual_modellogic_additionalconcepts_usingdistributionchooser_html`
- 摘要：**共 30 种**统计分布可选；常用五种：**Exponential（默认）**、Duniform（整数均匀）、Normal、Triangular、Uniform；**每种分布都带 Stream 参数**（选用哪条随机数流）。参数：Exponential=Location/Scale（Scale 为均值）；Duniform=Minimum/Maximum；Normal=Mean/Std Dev；Triangular=Minimum/Mode/Maximum；Uniform=Minimum/Maximum。
- 支撑：flexsim-to-openflex.md §分布映射（含 Exponential 用 Location/Scale 而非 mean 的差异；Duniform 未纳入版本 1 ofm 分布集）；runtime.md §随机数流。
- 置信度：高

### E31 — Empirical Distribution
- URL：`...?guid=FlexSim_User_Manual_reference_tools_empiricaldistribution_html`
- 摘要：用一张历史数据表当作分布：支持 Number 与 String（字符串自动设为 **Discrete**）；**Weighted** 复选可给每个值配权重（未勾选则等权）；内部会**排序并合并重复值及其权重**；数据可手工或经 Excel 导入，顺序无关。
- 支撑：ofm `empirical{values, weights?}` 的语义依据。
- 置信度：高

### E32 — Event List / Commands by Category（运行控制命令）
- URL：`...?guid=FlexSim_User_Manual_reference_tools_eventlist_html`；`...?guid=FlexSim_User_Manual_reference_codinginflexsim_commandreference_commandgroups_html`
- 摘要：Event List 列出**全部待处理事件**（Time/Object/Event Type/Involved/Event Data，可按列过滤），证明存在显式事件表调度。命令语义（公开命令参考）：`go([fast])` 启动（不清空、不重置；无事件则停）、`stop([fast])` 停止但保留待处理事件、`step()` 处理恰好一个事件、`skip()` 处理一个事件后把同一模型时刻的其余事件全部处理、`fastforward()` 存速度→最大速度跑到下一 stop time→恢复、`runspeed(num[,precisionMode])`=模型时间单位/真实秒、`stoptime(num[,stopNum])`（`stoptime(-1)` 返回当前值；`time=0,stopNum=-1` 重置为单个 disabled 停时；实际停止发生在**首个 ≥ 该时刻的系统检查点**）、`getrunstate()`（1=运行中）、`updatestates()`（把对象状态与 content 统计结算到当前时刻，Stop 按钮内部会先调用）、`endreplication(1)`、`commandlineparam("nrofops")`（配套 `flexsim.exe myfile.fsm -nrofops 8`，参数用 `-` 或 `/` 引导）。
- 支撑：runtime.md §运行控制命令语义、§模型文件与命令行；flexsim-to-openflex.md 的 CLI 子命令映射。
- 置信度：高

---

## D. 统计与实验

### E33 — Key Concepts About Getting Data
- URL：`...?guid=FlexSim_User_Manual_gettingdata_keyconceptsdata_html`
- 摘要：标准统计的五大类：**Content**（当前/最小/最大/平均含量）、**State**（当前状态与状态历史）、**Staytime**（最小/最大/平均停留）、**Throughput**（input/output 计数）、**Travel**（任务执行器累计行程）。另有高级工具表：Statistics Collector（采集原始数据入表）、Calculated Table（对采集表做过滤/比较/计算）、Experimenter、Optimizer。
- 支撑：statistics.md 的指标分类表。
- 置信度：高

### E34 — Performance Measures / Performance Measure Tables
- URL：`...?guid=FlexSim_User_Manual_gettingdata_performancemeasures_html`；`..._tools_performancemeasuretables_html`
- 摘要：性能指标=模型的**输出标量**（常见：总吞吐、平均利用率、平均等待时间、最大队长）；与 Model Parameter 相反（参数影响行为，指标只观测）。定义在 **Performance Measure Table**（新模型默认一张 `PerformanceMeasures` 表）；每行的 Value 是表达式，**在被取值时求值**（如每个复现结束或表格重绘时）；Reference 字段可指向模型节点（采样对象+统计项）；指标不得引用自身或自身历史（要用 Tracked Variable 存历史）。指标自动纳入实验/优化。
- 支撑：statistics.md §指标定义与求值时机；docs/spec 的 metrics 配置设计。
- 置信度：高

### E35 — Key Concepts About Experiments
- URL：`...?guid=FlexSim_User_Manual_gettingdata_advanceddatagathering_keyconceptsexperiments_html`
- 摘要：Job（Experiment / Range-Based / Optimization；后者需 OptQuest 授权）→ 生成 **Scenario** 与 **Replication** 组合的 **Task** → 结果入 **Results Database File**。**Scenario = 全部 Model Parameter 值 + 模型 Stop Time + 模型 Warmup Time** 构成的向量，按内容哈希唯一标识（界面显示后 8 位；自定名不进哈希）。**Replication 的随机数依赖复现编号**（第 1 次与第 2 次得到不同随机序列）。Task 并行跑（默认=CPU 核数），结果文件在**每个 Task 完成时**写入并跨 Job 持久，重跑前会查重跳过已完成的 Task；**不同模型的结果绝不能混用**。
- 支撑：statistics.md §实验器语义；runtime.md §replications 与 seed。
- 置信度：高

### E36 — Running Jobs / The Performance Measure Results Window
- URL：`...?guid=FlexSim_User_Manual_gettingdata_advanceddatagathering_runningjobs_html`；`..._tools_performancemeasureresults_performancemeasuresresults_html`
- 摘要：结果窗口可按指标出图：**Mean**（各场景均值）+ **Confidence**（选择置信水平，围绕均值画区间）；**Data Summary** 表给出每个 Scenario 的 **Mean / Sample Standard Deviation / Minimum / Maximum**（"Mean Based on" 选置信区间）；**Scenario Comparison** 以某场景为基线，按复现次数判断两场景均值是否**统计显著不同**并给出差值。
- 支撑：statistics.md §置信区间与场景比较（公式按样本均值±t 区间实现）。
- 置信度：中（给出量名与用法，未给闭式公式）

### E37 — Legacy Reports and Statistics
- URL：`...?guid=FlexSim_User_Manual_reference_tools_legacy_legacyreportsstatistics_html`
- 摘要：旧版"报告与统计"工具的存在与用途（旧版 Dashboard 图表的对应物），说明历史上存在面向对象的汇总报告视图（状态占比、吞吐、含量、停留时间等）。
- 支撑：statistics.md §报告内容（v1 的 `report` 输出对齐旧版报告字段）。
- 置信度：中

---

### E38 — User Commands
- URL：`...?guid=FlexSim_User_Manual_reference_tools_usercommands_html`
- 摘要：User Commands 让用户**在模型中新增自定义命令**并像内置命令一样调用（代码补全里显示为蓝色、可在 Command Summary 查到、悬停按 F1 出 Command Helper）。参数类型标注：Object/obj/treenode/node、Array/array、string/str、num/int/double/var；可用 `str/node` 形式并集；`[ ]` 包裹尾部可选参数；返回类型可为空或 num/str/node/var。
- 支撑：flexsim-to-openflex.md 的"用户命令 → openflex 表达式/自定义函数"映射与 `user_commands` 设计。
- 置信度：高

### E39 — Commands by Category（命令参考规模）
- URL：`...?guid=FlexSim_User_Manual_reference_codinginflexsim_commandreference_commandgroups_html`
- 摘要：公开命令参考按类别列出全部命令（含 3D 对象、conveyor、taskexecuter、statistics、model 等分组），每条含参数签名与语义说明；本台账只取与规格相关的少量命令语义（见 E32），**不复制代码实现**。
- 支撑：flexsim-to-openflex.md 的"脚本函数 → CLI/DSL 对应"表；CLI 命令命名。
- 置信度：高

### E40 — Key Concepts About Task Sequence Logic / How Task Executers Handle Tasks
- URL：`...?guid=FlexSim_User_Manual_workingwithtasks_keyconceptsabouttasklogic_html`；`..._howtaskexecutershandle_html`
- 摘要：任务序列是发给任务执行器的**任务列表**（travel/load/unload/delay/utilize/break 等），执行器逐条执行；序列可被更高优先级序列抢占；`nroftransportsin/out` 与 frload/frunload 保证"已承诺运输"的件不被重复计入容量判断。
- 支撑：logic.md §任务序列；objects.md §Operator。
- 置信度：高


### E41 — Task Priorities and Preemption
- URL：`https://help.autodesk.com/view/FLEXSIMIN/2025/ENU/?guid=FlexSim_User_Manual_workingwithtasks_taskprioritiespreemption_html`
- 摘要：任务执行器**默认按 FIFO** 处理任务；可用**优先级**（数值或标签）让更重要的任务先做。任务序列**默认优先级为 0**，**数值越大越先完成**（"priority 10 先于 priority 1"）。**抢占（preemption）** 决定是否中断当前任务，并决定被中断的任务是"重头再做"还是"直接删除"。典型场景：找活系统（从 List 拉任务的优先级）、机器故障（维修任务中断正常任务）、紧急插单。
- 支撑：logic.md §9.2 优先级与抢占；objects.md §10.1 `QueueStrategy` 语义。
- 置信度：高

### E42 — Label Matching
- URL：`...?guid=FlexSim_User_Manual_modellogic_labels_labelmatching_html`
- 摘要：标签匹配表用于**事件监听**（Process Flow 活动 / Statistics Collector）：把**事件参数**（最常用的是 Entering/Exiting Item、Station/Destination）写入标签，以便后续活动或采集表引用。可留空不关心的参数；行=该事件的可用参数。
- 支撑：logic.md §6 标签匹配；flexsim-to-openflex §5 的"标签继承规则"待定项。
- 置信度：中（概念页，无字段级定义）

### E43 — Key Concepts About Flow Items
- URL：`...?guid=FlexSim_User_Manual_using3dobjects_workingwithflowitems_keyconceptsflowitems_html`
- 摘要：流项目（flow item）是模型中流动的实体（物料、顾客、货物、订单…），默认外观是棕色方块；可在 Flow Item Bin 中改外观、**设置默认标签或 itemtype**、做外观帧。流项目分三类：**basic / container / TaskExecuter** 型，行为与属性不同。件可由 Source 创建，也可由 Process Flow 的 Create Object 活动创建。
- 支撑：术语表"Flow Item"；objects.md 中"容器件只计 1 个 content"的依据。
- 置信度：高

---

## 未获取 / 无法验证的内容（明确记录）

| 主题 | 状态 | 说明与后续建议 |
|---|---|---|
| 模型时间单位的**可选取值枚举**（秒/分/小时…） | 部分 | Model Settings 只写"displays the time units"，未见枚举页。旁证：命令参考提供 `hourtomodelunit()`/`minutetomodelunit()`/`getmodelunit()` 等换算函数，说明单位可配置但取值集未在帮助中列出。openflex 先固定 `time_unit ∈ {seconds, minutes, hours}`。置信度：低 |
| **Run Speed 与"1.00"的精确定义** | 已获取（高） | 默认 4 模型单位/真实秒；1.00=1 单位/真实秒 — 见 E27/E28 |
| Stop Time 的精确停止判定 | 已获取（高） | 停在**首个 ≥ 设定时刻的系统检查点**（可能因无更多事件而提前停）— 见 E32 |
| 置信区间的**闭式公式**（t 分布？z？） | 中 | 结果窗口只给量名（Mean/Sample StdDev/Confidence level）。openflex 采用教科书式 `mean ± t_{α/2,n−1}·s/√n`，并在 docs/spec/statistics.md 标注"公式为本项目定义，非文档原文" |
| 随机数生成器算法/流划分细节 | 低 | 文档只说"Random Number Streams 数量"与"每重复不同序列"。openflex 自选可复现 PRNG（如 PCG64）+ 流编号，不做数值级对齐（净室允许，因为未观察内部实现） |
| 30 种分布的完整清单与参数 | 部分 | 帮助只详述 5 种常用分布（E30）。其余仅"共 30 种"一句。记录在 flexsim-to-openflex.md 的"分布覆盖矩阵" |
| Combiner 的 ComponentsList 表结构细节 | 中 | 语义已明确（第 1 行=端口 2 收几件），表结构上限未见 |
| Conveyor 完整行为（slug 构建、合流、光电） | 部分 | 参数名与事件已取（E16），但行为细节分散在多页；v1 只做"段容量+速度+合流"简化 |
| FlexSim 的 `.fsm` 文件**内部格式** | 低 | 只有"模型文件扩展名 .fsm"、可嵌入媒体、可加密等外部描述。净室要求下**不做**格式逆向；openflex 自定义 JSON（见 spec） |
| Experimenter 结果数据库文件格式 | 低 | 只见"独立文件、随 Task 增长、可查重、可追加"等外部行为 |

---

## 统计

- 台账条目：**43 条**（要求 ≥20）
- 覆盖页面：154 页已下载（其中 1051 页目录可检索），本台账引用 44 个不同主题页
- 高风险主题（低置信度）已单独列出，不与高置信度结论混写
