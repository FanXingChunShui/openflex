# OpenFlex 未实现功能清单

> 基线：v0.10.0（2026-11-15）。本文只列**当前代码确实没有**的能力；已在
> [COMPATIBILITY.md](COMPATIBILITY.md) 标注“🟡 简化”的行为差异见该文档。
> 工作量口径：S ≤ 1 天，M ≤ 3 天，L > 3 天，XL > 2 周。

## 1. 对象与运行时

| # | 功能 | 现状 | 缺口 | 工作量 |
|---|---|---|---|---|
| 1 | **Fluid 真实压力/温度/密度** | ✅ 压差驱动 pipe 流量（`flow = k·Δp`，`k` 由 `flow_rate` 标定）、tank `p=ρ·g·h`（density/gravity 参数）、valve 压差滞回 `pressure_open/close` 自动重开、pipe `temperature` 黏度修正、背压与 `pressure` 报告；示例 fluid-pressure/fluid-backpressure | 无多相流/泵与阀特性曲线、无管满/可压缩效应 | L |
| 2 | **Person 人群模型** | ✅ `grid` 8 邻接 Dijkstra（对角 √2、禁切角）、局部密度（邻格 -15%、同格 -50%，下限 40%）、全局速度折减；示例 person-avoidance/person-grid | 无个体路径重规划/排队避让、无动态障碍、无楼梯/电梯换乘、无社会力模型 | L |
| 3 | **AGV 交通控制** | transporter alias + 电池；✅ 取货段途中耗尽就地停车充电并 `TransportResume` 续跑、同节点占用等待（0.5 步重试）；示例 agv-traffic/agv-battery | 无单向路网边/路网节点图、无充电桩容量/排队、无完整死锁避免、不搬运 flow item | L |
| 4 | **电梯门控** | 门开/关时间、超载折算、井道互斥、召唤/方向 | 无独立门态统计/对向调度/多轿厢群控 | S |
| 5 | **Conveyor 曲线与完整合流** | 直线带、PhotoEye/Station/Join（入口轮转）；✅ 可选 `curve: {radius, angle}`，按弧长计入行程时间（速度不变） | 无 3D 转弯几何/多段曲线/斜坡、无 Join 的 slug/gap 精细策略、无 Merge Controller、无电机/转盘/Transfers、PhotoEye 无遮挡持续状态/计数 | L |
| 6 | **Rack 货位精细放置** | (level,aisle,bay) 分配 + 行走时间 + pick_policy | 无空位查表/并发取货、无 SKU/批次策略 | S |
| 7 | **MultiProcessor 完整语义** | 多槽并行、每槽 setup/process、整批 crew 申请 | 无每槽独立操作员/故障、无槽位选择策略（优先级/最短队列） | M |
| 8 | **缺失对象** | Emptier + Robot/Crane/ASRS/Escalator 最小族 | Conveyor 电机/转盘、Fluid 泵/混合器、独立人员密度/碰撞模型 | M |
| 9 | **全局表（Global Table）** | schema/校验/访问器 + 表达式 `table(...)` 只读访问 | 写入/表驱动参数更新未支持 | S |
| 10 | **完整任务序列** | `task_sequence` 条件分支/自定义时长/优先级/抢占/恢复延迟/逐段事件 | 额外段在收尾阶段调度（未插入 load/unload 之间原始时间线）、无物理载货量 | S |
| 11 | **完整触发器集合** | OnCreate/Entry/Exit/Receive/Message/Breakdown/Repair/PhotoEye | 无 OnContentChange/OnStateChange/OnDraw/OnReset/OnStop/OnResume 等 | M |
| 12 | **A/S 连接与 Pull 细节** | 基础 Pull（strategy/requirement） | 无 A/S 连接端口语义、无 Pull 的完整策略（按端口条件等） | S–M |
| 13 | **Processor 故障细节** | 立即打断+补足剩余时间、MTBF/MTTR、first_failure_time、状态计时 | 无故障期间操作员释放策略、无 Down Behavior 动作复用 | S |

## 2. Process Flow 与脚本

| # | 功能 | 现状 | 缺口 | 工作量 |
|---|---|---|---|---|
| 14 | **Process Flow 块容量/触发器** | create/delay/assign/decision/sink/transfer(to/from) | 无块级排队容量、无块级触发器、无可视化编辑（studio 仅 JSON 文本） | M |
| 15 | **FlexScript 条件布尔返回** | ✅ 条件脚本可直接返回布尔表达式（如 `time < 10.0`）；仍兼容 `result` 标签为真的旧写法 | — | S |
| 16 | **FlexScript API 范围** | ✅ 新增 `object_content/object_state/object_utilization`、`set_state/create_item/move_item`；加原有 `time/item_type/item_id/label/set_label/set_item_type/send_message/release_item/log` | 无全局变量/表、无完整函数库、无 `send_delayed_message` 等其余动作 | M |
| 17 | **脚本动作扩展** | ✅ set_label/set_item_type/increment_label/send_message/destroy_item/release_item/script + `set_state/create_item/move_item` | 无完整 FlexSim 动作集（如 `send_delayed_message`/全局表写入） | S–M |

## 3. 运行、统计与优化

| # | 功能 | 现状 | 缺口 | 工作量 |
|---|---|---|---|---|
| 18 | **交互式运行控制** | 原生句柄 step/pause/resume/state/snapshot + 逐事件 step_event/next_event + Studio Step Event/事件列表 | 无 trace 回放/实时时钟速度 | S |
| 19 | **检查点多复现/续跑 trace** | 多复现检查点 + manifest（SHA-256/history/keep）+ Studio 恢复面板 | 续跑段 trace、校验和验证入口 | S |
| 20 | **统计高级方法** | t 区间/Little、`--batch-means`（系统级批均值 CI）、`--timeseries` | 无方差缩减/自相关处理/每对象批均值；时间序列为区间平均 | M |
| 21 | **Experimenter UI** | CLI `experiment` + Studio Experimenter（场景×复现对比表） | 无场景对比图表/参数选择器 | S |
| 22 | **OptQuest 等价优化** | grid/random/遗传、多目标加权、Pareto 前沿 | 无自定义权重/贝叶斯优化/约束优化/自动停止规则 | M |
| 23 | **Dashboard/图表** | Studio 三联 Dashboard（吞吐/利用率/WIP）+ PNG 导出 | 无时间序列/模板/图表库 | S |

## 4. 文件与互操作

| # | 功能 | 现状 | 缺口 | 工作量 |
|---|---|---|---|---|
| 24 | **`.fsm` 互操作** | `fsm_concept_json` 概念导出 | 不生成/不读取 FlexSim `.fsm` 二进制（净室不逆向，已声明） | — |
| 25 | **Excel 导入** | `run --xlsx` 导出；`import --xlsx` 读取 objects/connections/arrivals | 不计算公式/日期格式/合并单元格；无数据库驱动 | S |
| 26 | **3D/CAD 资产导入** | 无 | USD Stage、CAD 平面图、3D 模型导入 | L |
| 27 | **模型版本迁移** | 无 | schema 版本升级器/迁移工具（当前 v1 无升级路径） | S |

## 5. 图形界面

| # | 功能 | 现状 | 缺口 | 工作量 |
|---|---|---|---|---|
| 28 | **完整 3D 渲染** | Studio 是 Canvas 2D 等距/透视近似投影 | 无 WebGL/GPU 渲染、光照/材质、3D 动画对象（当前用圆点近似流动） | L |
| 29 | **模型树/属性面板** | 对象列表 + 字段级动态表单（按 kind）+ Advanced JSON | 无层级模型树 | S |
| 30 | **流程图可视化编辑** | Studio 拖拽画布（块拖动/端口连线）+ 块表单 + JSON | 无缩放/平移/画布内新建块 | S |
| 31 | **撤销/重做/协作** | 无 | 编辑历史、多用户协作 | L |

## 6. 发布与验证

| # | 功能 | 现状 | 缺口 | 工作量 |
|---|---|---|---|---|
| 32 | **三平台真机安装** | macOS 实机通过；Windows/Linux 仅 CI 构建与格式检查 | Windows MSI/zip、Linux deb/rpm/tar 未在真机安装验证 | S（需环境） |
| 33 | **签名/公证** | 脚本与 CI 可选步骤就绪（secrets 缺失自动跳过） | 无 macOS Developer ID/公证、无 Windows Authenticode、无 GPG 签名的实际密钥 | M（需证书） |
| 34 | **包管理器/容器发布** | Homebrew/winget/scoop 模板、GHCR workflow 就绪 | 未发布 tap/镜像/清单；Docker 未实际 build | S–M |
| 35 | **真实 FlexSim 对照** | `compare-flexsim.sh` 双报告模式 + 模板 | 无真实 FlexSim 运行数据（净室边界；需外部提供） | — |
| 36 | **性能规模** | M/M/1 约 1.75M events/s；100k 对象 validate 0.75s / run 2.61s；CI bench 门槛（10k） | 仍可优化 serde_ignored 与 notify；并行 worker 数自适应 | S |
| 37 | **CI 质量门槛** | deny/audit/coverage 60%/fuzz 180s/arm64/bench（10k 门槛） | dependabot Actions PR 未合并 | S |

## 7. 文档与本地化

| # | 功能 | 现状 | 缺口 | 工作量 |
|---|---|---|---|---|
| 38 | **文档站/教程** | README、docs/（中文为主） | 无 mdBook 站点、无分步教程/视频脚本 | M |
| 39 | **完整 i18n** | report/help 中英（`OPENFLEX_LANG`） | 文档无多语言、CLI 部分文案仍英文 | M |

## 8. 已知行为差异（不是“未实现”，但需知晓）

- 随机数使用 ChaCha12，与 FlexSim 不同，因此数值统计一致但不会逐位相同。
- 同刻事件排序为 `(time, seq)`，由 OpenFlex 自定义并已在 ARCHITECTURE 公开。
- 脚本条件优先使用脚本的布尔返回值；未返回布尔时回退到 `result` 标签约定（`> 0` 为真）。
- 3D 布局 `pos` 是 2D + 可选 `z`，不是完整 3D 坐标。
- 故障立即打断并补足剩余加工时间（文档已声明）。
