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
| Conveyor | `conveyor` | ✅ | 速度、长度、间距、累积/非累积、容量；`photo_eyes` 光电门、`stations` 停留加工、`join_inputs` 两带汇入（入口轮转简化）；无曲线 |
| Merge | `merge` | ✅ | 多输入合流；可选等待全部输入 |
| Split | `split` | ✅ | 轮询或按 item type 分流 |
| Transporter / TaskExecuter | `transporter` | 🟡 | 速度、装载/卸载时间、按坐标距离行驶、`home` 回程；`capacity` 解释为可并发操作员任务数（不搬运 flow item）；无任务序列/抢占/路网 |
| Dispatcher | `dispatcher` | 🟡 | 把 Processor 的操作员请求派给最近的空闲 transporter；FIFO 队列，无优先级/抢占 |
| Operator / Resource | `resource` | ✅ | 资源池容量与停机窗口 |
| Rack | `rack` | 🟡 | `rack_type`：floor(FIFO)/gravity/push_back(LIFO)/drive_in(FIFO 近似)；无货位/巷道精细放置 |
| 班次 / 计划停机 | `params.shifts` | ✅ | `[{start, duration, repeat_every}]`，停机进入 off_shift、暂停处理、恢复后继续 |
| Time Table | `params.timetable` | ✅ | `{windows:[{start,end}]}`，与 shifts 取交集 |
| BasicFR / BasicTE | `basic_fr` / `basic_te` | ✅ | 通用固定资源与执行器 |
| MultiProcessor | `multiprocessor` | ✅ | 多槽并行加工，每槽 setup/process，操作员按忙碌期申请 |
| Fluid（流体） | — | ⛔ | 未实现 |
| People / A* 行走 | — | ⛔ | 未实现 |
| AGV / 交通控制 | — | ⛔ | 未实现 |
| Elevator / 电梯 | — | ⛔ | 未实现 |
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
| FlexScript / C++ 逻辑 | 表达式语言 + 动作 | 🟡 | 无任意脚本；支持变量、算术、比较、条件与少量函数 |
| 全局表（Global Table） | — | ⛔ | 可用标签/参数覆盖模拟 |
| Process Flow（流程图块） | — | 🟡 | 用对象 + 触发器表达；无可视化流程块编辑器 |

## 运行与统计

| FlexSim | OpenFlex | 状态 | 说明 |
|---|---|---|---|
| 模型时间单位 | `time_unit` | ✅ | 秒/分/小时/天，仅影响显示 |
| 停止时间 | `run.stop_time` | ✅ | |
| 预热（warm-up） | `run.warmup` | ✅ | 统计窗从 warm-up 起 |
| 随机种子 / 复现 | `run.seed` / `run.replications` | ✅ | SplitMix64 派生，可复现 |
| 运行/暂停/步进 | — | 🟡 | CLI 批量运行；无交互式时钟控制 |
| 对象统计（状态占比、吞吐、WIP） | 报告 | ✅ | idle/busy/blocked/down/off_shift；时间加权 content；停留时间 |
| 班次 / 计划可用性 | `shifts` | ✅ | 见上表“班次” |
| 停留时间 | 报告 | ✅ | 平均停留时间 |
| Experimenter（场景 × 复现） | `experiments` + `openflex experiment` | ✅ | 95% 置信区间 |
| 置信区间 | 报告 | ✅ | 正态近似 `1.96·se` |
| OptQuest 优化 | — | ⛔ | 未实现 |
| Dashboard / 图表 | 报告（Markdown/CSV） | 🟡 | 无交互图表；可由报告生成 |
| 3D 视图 / 模型树 / GUI | — | ⛔ | CLI 优先；`pos` 仅用于运输距离与布局导出 |
| 导出 Excel / 数据库 | CSV / JSON | 🟡 | 可被 Excel 打开；无原生 xlsx 写入 |

## 文件与集成

| FlexSim | OpenFlex | 状态 | 说明 |
|---|---|---|---|
| .fsm 模型文件 | `.ofm` JSON | ✅ | 纯文本、可 diff、可版本控制 |
| 命令行参数 | `openflex` CLI | ✅ | validate/run/experiment/report/compare |
| 脚本接口 | CLI + JSON 报告 | 🟡 | 无嵌入式脚本引擎（表达式语言除外） |
| 与 Excel/数据库交换 | CSV/JSON | 🟡 | |
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
