# Changelog

## v1.62.0 — 2026-11-27

这一版有一件功能、一件工具、和一件**覆盖面上的疏漏**。

**1. Process Flow 的块级排队容量（`FlowDelayParams.capacity`）。** 之前所有到达的 token 同时开始计时。
现在 token 先占槽位：有空位立即进入；满则按到达顺序在**块入口 FIFO 排队**（不采样、不计 delay 时间），
完成离开时释放槽位并在**同一 `now`** 调度队列首 token，**无额外延迟**。

只有 `capacity > 0` 才增加这次同刻调度，所以不限制时的事件日历与历史完全一致（基准报告 SHA-256 不变）。
`None` 与 `0` 都定义为不限制。检查点也覆盖排队状态。

实测 3 token + delay 5：

| 容量 | 离开时刻 |
|---|---|
| 不限制 | 5 / 5 / 5 |
| 1 | 5 / 10 / 15 |
| 2 | 5 / 5 / 10 |

**2. 工具：`scripts/check-stale-gaps.py`（已进 CI）。** 上一轮发现矩阵里"仍缺 checkpoint 保存链状态"其实早已修好。
**🟡 行的"缺"是一句断言，也会过期**；读到一个过期的"缺"，代价是一周白干。
脚本按"缺口短语提到某个 backticked 标识符 + 它确实是 `model.rs` 里的 `pub` 字段 + 存在以它开头的测试"判定可疑，
误报进白名单（现在 3 条，每条都写了理由；第一版把 `group` 误判成 `groups_steps_...`，已收紧为整词前缀匹配）。

**3. 矩阵里从来没有 Process Flow。** 这不是"没写全"，是**整块缺失**：7.5 中文帮助的 326 页里没有 Process Flow 章节
（该模块文档随模块单独提供，这套中文包未收录），所以主表一直没覆盖它——而它恰恰是 FlexSim 的核心建模工具，
我们也早已实现 token 运行时与六个块。

新增 §17c 登记 11 行：运行时、Create/Delay/Assign/Decision/Sink/Transfer、统计/trace/检查点（8 ✅），
可视化编辑器（🟡），块级触发器与其他块（⛔）。**证据等级明确标低**：块清单按已实现部分登记，
没能逐页对照 7.5 版模块帮助；⛔ 写的是"我们确实没有"，不是"7.5 确定有、只是我们没做"。

矩阵 **208 → 219 行**（✅ 61 / 🟡 101 / ⛔ 54 / 📄 3），计数脚本已核对。

发布节奏保持**至少 3 小时一版**。

### 工具与 CI

- **工具：`scripts/check-stale-gaps.py`（已进 CI）**。上一轮发现矩阵里"仍缺 checkpoint 保存链状态"其实早已修好——
  **🟡 行的"缺"是一句断言，也会过期**，而读到一个过期的"缺"，代价是一周白干。
  脚本按"缺口短语提到某个 backticked 标识符 + 它确实在 `model.rs` 里 + 存在以它开头的测试"判定可疑，
  误报进 `docs/stale-gap-allowlist.txt`（现在 2 条，都写了理由；第一版把 `group` 误判成 `groups_steps_...`，已收紧为整词前缀匹配）。

- **冒烟套件 26 → 27 项**：新增 `process flow capacity`（3 token + delay 5：不限 5/5/5、cap1 5/10/15、cap2 5/5/10）。


### 模型与引擎

- **Process Flow 的块级排队容量（`FlowDelayParams.capacity`）**：矩阵 🟡「Process Flow 块容量」此前所有到达的 token 同时计时。
  现在 token 到达 delay 块先占槽位：有空位立即进入并开始计时；满则按到达顺序在**块入口 FIFO 排队**（不采样、不计 delay 时间）。
  完成离开时释放槽位并在**同一 `now`** 调度队列首 token 进入，**无额外延迟**；只有 `capacity > 0` 才增加这次同刻调度，
  所以不限制时的事件日历与历史完全一致（基准报告 SHA-256 不变）。`None` 与 `0` 都定义为不限制（注释写死）。
  检查点也覆盖：`delay_capacity_queue_survives_a_checkpoint`（stop 6 时取检查点，续跑与不中断一致）。
  独立实测：3 个 token + delay 5 → capacity 1 离开 **5/10/15**、capacity 2 为 **5/5/10**、不限制为 **5/5/5**。

### 文档

- **矩阵补上 Process Flow 模块（§17c，11 行）**：7.5 中文帮助的 326 页里**没有 Process Flow 章节**（该模块文档随模块单独提供，
  这套中文包未收录），所以主表一直没覆盖它——但它是 FlexSim 的核心建模工具，而我们已实现 token 运行时与六个块。
  新增一节登记：运行时、Create/Delay/Assign/Decision/Sink/Transfer 六个块、统计/trace/检查点（8 ✅）、
  可视化编辑器（🟡，Studio 里是 JSON 文本）、块级触发器与其他块（⛔）。
  **证据等级明确标低**：块清单按已实现部分登记，未能逐页对照 7.5 版模块帮助；⛔ 写的是"我们确实没有"，不是"7.5 确定有"。
  矩阵 208 → **219 行**（✅ 61 / 🟡 101 / ⛔ 54 / 📄 3），计数脚本已核对。


### 验证

- `cargo test --workspace --locked`：**1146 passed / 0 failed**；clippy 0 警告、fmt 干净。
- smoke **27/27**（新增 `process flow capacity`）；示例 **66/66**；golden 全过；GUI **157 项**。
- 矩阵 **219 行**：✅ 61 / 🟡 101 / ⛔ 54 / 📄 3；`check-parity-counts.py`、`check-stale-gaps.py`、
  `audit-parity-evidence.py --strict`、`check-doc-baselines.sh` 全部通过。
- 独立实测：3 token + delay 5 → 不限 5/5/5、cap1 5/10/15、cap2 5/5/10。

## v1.61.0 — 2026-11-27

这一版补光电门的两个逐门参数，并且**发现矩阵里的一条"缺"其实早就修好了**。

**1. 光电门 `clear_time` 与 `require_gap`。**

- **`clear_time`（去抖）**：眼睛从"有件"变"无件"**不立即**发 Clear，进入 `Clearing` 并调度一个定时事件，
  到期仍无件才发 Clear；计时期间又来件 → 取消本次计时、回到 `Blocked`，**既不发现 Clear 也不发新的 Block**。
- **`require_gap`（连续遮挡）**：**先**判定连续遮挡——上一遮挡件离开门后，若后一件与它的同刻皮带中心距
  `< require_gap`，门保持 `Blocked`、不启动 clear 计时、第二件不产生 Block 跃迁；间距足够（或无后件）才让 `clear_time` 起算。

间距直接取 `Belt.positions`（front-first）同一时刻做差，**没有改皮带几何模型**。
缺省 0 走历史立即转换路径，**3 个基准模型的 report + trace SHA-256 逐字节不变**。

**2. 检查点连"已排定的定时器"也要存住。** 光电门的 Clear 定时是一个排定事件，续跑时最容易丢的就是它。
新增 `photo_eye_clear_time_survives_a_checkpoint`：在计时进行中（t=11，到期 12.5）取检查点、做一次 JSON roundtrip、
恢复后续跑，Clear 时刻仍是 **12.5**，与不中断运行一致。

**3. 矩阵里一条"缺"是过期的——如果不去核，我们会花一周实现一个已经存在的东西。**

「Task Sequence Concepts」写着"仍缺 checkpoint 保存链状态"。实测：`--stop-time 20 --save-checkpoint` 后 `--resume`，
报告与不中断运行**逐字节相同**（排除 final_time）。explicit 链在 task-183 就有测试，append 链的 `pending_steps`
也进检查点——**这条不是要做的功能，是要补的证据**。已新增 `append_chain_checkpoint_matches_uninterrupted_run`。

**教训**：矩阵里的 ✅ 需要证据，**🟡 里的"缺"同样需要证据**。我顺手扫了全部 🟡 行，没有再发现同类过期。

发布节奏保持**至少 3 小时一版**。

### 模型与引擎

- **补测：append 链的检查点续跑**（`append_chain_checkpoint_matches_uninterrupted_run`）。
  矩阵原文说「仍缺 checkpoint 保存链状态」，实测是**过期**的：`--stop-time 20 --save-checkpoint` 后 `--resume`，
  报告与不中断运行**逐字节相同**（排除 final_time）。explicit 链早在 task-183 有测试，而 append 链的 `pending_steps`
  此前**只有手工验证**——现在钉住了。这条也提醒：矩阵里的"缺"同样需要证据，否则会把已经修好的东西再记一遍。

- **光电门 `clear_time`（去抖）与 `require_gap`（连续遮挡）**：矩阵 🟡「Photo Eye」缺 FlexSim 的 Clear Time / Require Gap。
  `clear_time`：无遮挡后进入 `Clearing` 并调度 `PhotoEyeClear` 定时事件，到期仍无件才发 Clear；计时期间来件 → 取消、回 `Blocked`，
  **不发 Clear 也不发 Block**。`require_gap`：**先**判定连续遮挡——上一遮挡件离开门后，若后一件与它的同刻皮带中心距
  `< require_gap`，门保持 `Blocked`、不启动 clear 计时、第二件不产生 Block 跃迁；间距足够或无后件才让 `clear_time` 起算。
  间距直接取 `Belt.positions`（front-first）同一时刻做差，**未改皮带几何模型**。缺省 0 走历史立即转换路径，
  **3 个基准模型的 report + trace SHA-256 逐字节不变**。
  **检查点**：`photo_eye_clear_at` 进检查点（全 None 时省略，旧 checkpoint 兼容），
  新增 `photo_eye_clear_time_survives_a_checkpoint`（计时中 t=11 取检查点，续跑后 Clear 仍为 12.5）——
  光电门+检查点最容易丢的就是这种"已排定的定时器"。实测：`clear_time: 2` 的 Clear 由立即延到 7.5（该模型几何下的值）。


### 验证

- `cargo test --workspace --locked`：**1139 passed / 0 failed**；clippy 0 警告、fmt 干净。
- GUI **157 项**（本轮 GUI 只补 6 行机械字段）、`openflex-core --lib` 见上；smoke **26/26**；示例 **66/66**；golden 全过。
- 矩阵：✅ 53 / 🟡 **100** / ⛔ 52 / 📄 3；`audit-parity-evidence.py --strict` 通过。
- 独立实测：`clear_time: 2` 时 Clear 由立即变为 7.5（该模型几何），`clear_time: 0` 无延迟事件；
  append 链检查点续跑报告与不中断运行一致；`photo_eye_clear_time_survives_a_checkpoint` 续跑 Clear=12.5。

## v1.60.0 — 2026-11-27

这一版补的是**任务序列里最常用的两个逐任务参数**：`end_speed` 和 `force_travel`。

**1. `end_speed`：到达时只减速到某个速度，而不是停住。** 用于"边移动边作业"的场景（叉车不停车取货、
AGV 通过站点不刹停）。加减速开启时，这不是比例缩放——**按运动学重新推导**：

```text
D_trap(e) = v²/(2a) + (v²-e²)/(2b)      D_min(e) = e²/(2a)
梯形 d >= D_trap(e):  t = v/a + (v-e)/b + (d - D_trap(e))/v
三角 D_min(e) <= d < D_trap(e):  p = √(a(2bd+e²)/(a+b)),  t = p/a + (p-e)/b
```

手算与实现一致：`d=10, v=2, a=1, b=2, e=1` → **6.125**（`e=0` 时 6.5）；append 模式下 pickup `4.0 → 3.625`、delivery `6.5 → 6.125`。

**2. `force_travel`：距离为 0 也执行这段 travel**（否则任务序列会"塌陷"掉这一段）。时长仍然是 0——**不臆造时间**。

**3. 不可行区选择报错，而不是近似。** 当 `e > 0` 且 `d < D_min(e)`，物理上到不了这个速度（减速段会变成负时间）。
我们选择 `SimError::Config`，点名 `end_speed`、实际距离、**最短距离**。

备选方案是"把 `end_speed` 当成可达上限，短距离只加速到 `√(2ad)`"——那会**静默交付低于用户要求的到达速度**，
正是我们这几次最想避免的"看起来对、实际不是那个意思"。**报错比一个语义漂移的数字好。**

**边界都写死**：`e == 0` 直接委托原函数（保证逐位一致，不走通式以免浮点末位浮动）；无加减速时 `end_speed` 不影响时长；
`d == 0` 且无 `force_travel` 不报错；`force_travel && d == 0 && e > 0` 报错。append 模式下 duration-only 的 extra travel 设这两个字段会被 validate 拒绝。

**4. 一处流程说明。** 这个字段加在 `TaskStep::Detailed` 上会强制更新所有 struct-variant 字面量，GUI 里 12 行必须机械补齐。
规则的边界是：**"不要动 GUI"指的是不改行为与界面，不是不许改到编译不过的地方**。diff 里只有 12 行字段补齐，没有别的。

**5. DELAY 任务类型**（矩阵「Task Types」最后一项）。三者的差别是**统计口径**：`utilize` 是 busy 占用，
`delay` 是原地等待、**时间落进 `idle` 而不进 busy/utilize**，`break` 是下班。

实现前先读了 `ExecutorActivity` 才动手——新增 `ExecutorActivity::Delay` + `state_times.delay` + 报告 `delay` 字段
（非零才出现，缺省 JSON 逐字节不变），**没有改 busy 口径**。实测同 d=5 对照：

| 腿型 | idle | busy | delay | utilize | 完成时刻 |
|---|---:|---:|---:|---:|---:|
| 无额外腿 | 74 | 21 | 0 | 1 | 21 |
| delay | 79 | 21 | 5 | 1 | 26 |
| utilize | 74 | 26 | 0 | 6 | 26 |

delay 与 utilize 完成时刻相同、统计不同——这正是三者该有的样子。

发布节奏保持**至少 3 小时一版**。

### 模型与引擎

- **travel 逐任务参数：`end_speed` 与 `force_travel`**（矩阵 🟡「Task Types」）。`TaskStep::Detailed` 新增
  `end_speed: Option<f64>`（到达只减速到该速度）与 `force_travel: bool`（距离 0 也执行该腿）。
  加减速按**运动学重新推导**（不是比例缩放）：`D_trap(e)=v²/2a+(v²-e²)/2b`、`D_min(e)=e²/2a`；
  梯形 `t=v/a+(v-e)/b+(d-D_trap)/v`；三角 `p=√(a(2bd+e²)/(a+b))`、`t=p/a+(p-e)/b`。
  手算与实现一致：`d=10,v=2,a=1,b=2,e=1` → **6.125**（`e=0` 为 6.5）；append 实测 pickup 4.0→3.625、delivery 6.5→6.125。
  **不可行区 `d < D_min(e)` 选择报错**（`SimError::Config`，点名 `end_speed`、实际距离、最短距离）——
  B 方案（把 `end_speed` 当"可达上限"）会静默交付低于用户要求的到达速度，正是我们最想避免的语义漂移；C 更糟。
  边界写死：`e==0` 直接委托原函数（保证逐位一致）；无加减速时 `end_speed` 不影响时长；`d==0` 且无 `force_travel` 不报错；
  `force_travel` + `d==0` + `e>0` 报错，`e=0` 则时长 0 但腿仍然发生。append 的 duration-only extra travel 设这两字段被 validate 拒绝。
  上界校验分层：validate 静态拒绝负数/非有限、可解析 executor 时拒绝超速；dispatch 时再按实际 executor 校验。

- **DELAY 任务类型（`ExtraTaskLegKind::Delay`）**：矩阵 🟡「Task Types」最后一项。
  三者的差别是**统计口径**：`utilize` 是 **busy** 占用，`delay` 是原地等待、**时间落进 `idle` 而不进 busy/utilize**，
  `break` 是下班。实现时先读了 `ExecutorActivity` 才动手：新增 `ExecutorActivity::Delay` + `StateTimes.delay`
  与报告聚合 `ObjectAggregate.delay`（非零才出现，**缺省 JSON 逐字节不变**），**没有改 busy 口径**。
  实测同 d=5 对照：delay 腿 `idle=79 busy=21 delay=5`；utilize 腿 `idle=74 busy=26 delay=0`；
  **完成时刻都是 26**——同语义、不同统计，这正是三者该有的样子。
  `duration <= 0` validate 报错点名字段；append 与 explicit 两种任务序列都支持。


### 验证

- `cargo test --workspace --locked`：**1132 passed / 0 failed**；clippy 0 警告、fmt 干净。
- GUI **157 项**、`openflex-core --lib` **338 项**；smoke **26/26**；示例 **66/66**；golden 全过。
- 矩阵：✅ **53** / 🟡 **100** / ⛔ 52 / 📄 3（Task Types 升 ✅）；`audit-parity-evidence.py --strict` 通过
  （它当场抓到我手写的两个**不存在的测试名**，已改为实际名称）。
- `check-dead-params.sh` 0 known；`check-doc-baselines.sh`、`check-changelog.sh`、`check-parity-counts.py` 通过。
- 独立实测：`end_speed: 3`（executor 速度 1）→ validate 点名最快的 executor 速度；`-1` → `must be a finite, non-negative speed`；
  delay vs utilize 同 d=5：完成时刻都 26，`busy` 21 vs 26、`delay` 5 vs 0。

## v1.59.0 — 2026-11-27

这一版把一个 ⛔ 变成 ✅，而且**故意只做了一半的功能**——这是最重要的一点。

**1. `run.start_time`：报告可以从"早上八点"开始。** 矩阵原来把 Model Start Time 列为 ⛔：只支持 `t=0`，
而文档允许把 0 时刻映射到真实时间。

现在的实现是**纯显示偏移**：`final_time`、trace 的每条事件 `time`、`tracked_series` 时间点、
`--paths` CSV 的 `first_time`/`last_time` 一律加这个常量；而**内部时钟、事件调度、分布采样、
`stop_time`/`warmup` 的含义一个字没动**，`avg_stay_time`/`busy_fraction`/`throughput`/`completed` 逐位不变。

为什么不做"真把时钟推到 8 点"？因为那会连带改变 `first_arrival`、`warmup`、`stop_time` 的语义——
不是加一个字段，是重新定义模型时间轴。**我们宁可交付一个边界清楚的显示偏移，也不交付一个半懂的时间语义。**

实测：`start_time: 100` → `final_time` 20→120、trace 首事件 100.0，而 `avg_stay` 仍 2.0、`completed` 仍 3；
缺省 0 时输出逐字节不变（3 个基准模型 report JSON + CSV 的 SHA-256 证明）。

**2. 顺带确认了 CSV 里没有"时间/时长两义"的列**：`report.csv`/`replications.csv` 只有时长与比例列，
所以不用猜——这也正是先停手条款要防的情况。

**3. `docs/COMPATIBILITY.md` 补齐了最近三个版本的新能力**：多故障源、逐端口 components、多槽 batch、
按前车类型的皮带间距、按占用量分档调速。这份表是用户查"FlexSim 的对象对应我们什么"的第一站，
落后于实现就等于告诉用户"这个参数不存在"。

**4. 稳定性复核**：800 轮 fuzz 全过；10 万对象基准 run 通过（12.8 万事件/秒）。

发布节奏保持**至少 3 小时一版**。

### 文档

- **`docs/COMPATIBILITY.md` 补齐最近几个版本的新能力**：Processor 的**多个独立故障源**（`extra_failures`）、
  Combiner 的**逐端口 `components` 表**与 **`max_content` 多槽**（batch 逐槽并行、busy 取并集）、
  Conveyor 的**按前车类型的动态间距**（`gap_by_type`）、TrafficControl 的**按占用量分档调速**（`speed_by_content`）。
  这份表是用户查"FlexSim 的对象对应我们什么"的第一站，落后于实现就等于告诉用户"这个参数不存在"。

### 模型与引擎

- **Model Start Time（`run.start_time`，显示偏移）**：矩阵 ⛔「Simulation Model Units 的 Model Start Time」。
  **只做输出层偏移**：`final_time`、trace 每条事件的 `time`、`tracked_series` 时间点、
  `--paths` CSV 的 `first_time`/`last_time` 加该常量；**内部时钟、调度、采样、`stop_time`/`warmup` 的含义一律不动**，
  时长/比例/计数（`avg_stay_time`、`busy_fraction`、`throughput`、`state_times`、`completed`）逐位不变。
  原因写在 `RunSettings` 注释里：把偏移推进调度器，`first_arrival`/`warmup`/`stop_time` 的语义全要跟着改。
  `start_time` 缺省为 0 时输出逐字节不变（3 个基准模型 report JSON + CSV 的 SHA-256 证明）；负偏移也正确。
  独立实测：`start_time: 100` → `final_time` 20→120、trace 首事件 `100.0`，而 `avg_stay` 仍 2.0、`completed` 仍 3。
  CSV 侧确认没有"时间/时长两义"的列：report/replications CSV 只有时长与比例列，故不受影响。

### 桌面 GUI

- **GUI：运行设置里加 `START AT`（显示偏移）**，提示写明"只影响报告时间，时钟仍从 0 起"；测试 `start_time_is_display_only_in_the_file`（缺省 0 不写进文件）。GUI **157 项**。
- **冒烟套件 25 → 26 项**：新增 `start time offset`（`final_time` 20→120、首事件 100.0，而 `avg_stay_time`/`throughput`/`completed` 不变）。


### 验证

- `cargo test --workspace --locked`：**1123 passed / 0 failed**；clippy 0 警告、fmt 干净。
- GUI **157 项**、`openflex-core --lib` **338 项**；smoke **26/26**；示例 **66/66**；golden 全过。
- 矩阵：✅ **52** / 🟡 101 / ⛔ **52** / 📄 3（Model Start Time 由 ⛔ 升 ✅）；`audit-parity-evidence.py --strict` 通过。
- `check-dead-params.sh` 0 known；`check-doc-baselines.sh` 通过；`check-changelog.sh`、`check-parity-counts.py` 通过。
- 独立实测：`start_time: 100` → `final_time` 20→120、trace 首事件 100.0，`avg_stay_time`/`completed` 不变；负偏移 `-50` 正确。
- 稳定性：fuzz 800 轮 PASS；10 万对象基准 PASS（12.8 万事件/秒）。

## v1.58.0 — 2026-11-27

这一版继续补"模型作者天天会用"的东西，另外把两份最容易被误读的文档改了。

**1. `query()` 能聚合了。** 之前只有 `SELECT … FROM … [WHERE] [ORDER BY] [LIMIT]`，
而 `query()` 最常见的用法恰恰是"按列汇总"。现在支持 `COUNT(*) / SUM / AVG / MIN / MAX` 与 `GROUP BY`，
`ORDER BY` 可以用聚合（`count(*)` 形式）。JOIN / 子查询仍然明确不做。

**几处语义写死**，因为空值和坏数据最容易"看起来对、算错了"：

- **空输入**：`COUNT`=0、`SUM`=0、`AVG/MIN/MAX`=**空串**、`GROUP BY` 无匹配行 = 0 行；
- **`SUM` 遇到非数值单元格报错**，点名 1-based 行号 + 列名——不跳过、不当 0；
- 聚合混普通列而没有 `GROUP BY` → 报错；
- `check()`（validate 用）在**跑之前**就能拒绝坏聚合。

**2. 坏查询不会把整次跑挂掉。** 新增 `malformed_queries_never_panic`：24 条畸形/恶意语句
（空串、`SELECT COUNT(`、未闭合引号、`; DROP TABLE`、NUL 字符、负 LIMIT…）走 `parse`/`check`/`execute` 三条路径都不得 panic。
`query()` 是从模型表达式里调的，panic 会把整次运行带走，而不是报一条坏查询。

**3. README 的能力表补全了。** 原来只列 13 行，缺全局表/`query()`、任务序列、路网与交通、多故障源、
流体/人员/AGV/电梯、模型文档生成、检查点——这些都已经有了。现在按 ✅/🟡/⛔ 如实标注，并指向 208 行的逐行矩阵。
**README 是用户第一眼看到的功能列表，不能比矩阵旧。**

**4. `PARITY_AUDIT.md` 补了第二轮取证。** 第一轮的方法边界（"测试名存在 = 有证据"）正是
表驱动到达那个"标着 ✅ 却完全失效"的功能躲过审计的原因。文档现在写清三级证据、新工具的分级规则、
白名单机制，以及"历史数字只是快照、计数以 `check-parity-counts.py` 为准"。

**5. 皮带按 item type 的动态间距。** 之前只有静态 `item_spacing`；现在可按**前车**的 `item_type`
覆盖间距（`gap_by_type: [{ item_type, gap }]`），未命中回落 `item_spacing`，空表 = 原路径（3 个 conveyor 基准 SHA 不变）。

**slug 的交互写死**：slug 内部仍按 `slug_min_gap`，slug **之间**的下一组首件按前一组末件的类型查本表。
实现上给 `Belt` 增加了与 follower 对齐的 gaps，`limit`/`compress`/time-to-next-event 全按 follower gap——
**没有改推进模型**，那是 slug/accumulating 共用的核心路径。GUI 里皮带表单多了 `gap by item type`。

发布节奏保持**至少 3 小时一版**。

### 文档

- **`docs/PARITY_AUDIT.md` 补了第二轮取证**：第一轮的方法边界（"测试名存在 = 有证据"）正是表驱动到达那个
  "标着 ✅ 却完全失效"的功能躲过审计的原因。现在文档写清了三级证据（behaviour/model/report）、
  新工具 `audit-parity-evidence.py` 的分级规则、过期引用的白名单机制，以及剩余两条"表单往返级"证据的口径。
  并说明：历史数字只是快照，**计数以 `check-parity-counts.py` 为准**——第一轮那种"✅ 50 / 台账 43 / 卡 51"的漂移不会再发生。

- **README 的能力表补全**：原来只列了 13 行，缺全局表/`query()`、任务序列、路网与交通、多故障源、流体/人员/AGV/电梯、
  模型文档生成、检查点等实际已有能力。现在按 ✅/🟡/⛔ 如实标注（🟡 的行写明口径），并指向
  `FEATURE_PARITY_75.md` 的 208 行逐行对照——README 是用户第一眼看到的功能列表，不能比矩阵旧。


### 模型与引擎

- **SQL 聚合与 GROUP BY（`query()` 子集）**：矩阵 🟡「SQL Queries」此前只有 `SELECT … FROM … [WHERE] [ORDER BY] [LIMIT]`。
  现在支持 `COUNT(*) / SUM / AVG / MIN / MAX` 与 `GROUP BY`；**JOIN / 子查询仍不做**（注释里继续写明）。
  几处语义写死在模块注释里：**空输入** COUNT=0、SUM=0、AVG/MIN/MAX=**空串**、`GROUP BY` 无匹配行=0 行；
  **SUM 遇非数值单元格报错并点名 1-based 行号 + 列名**（不跳过、不当 0）；聚合混普通列而没 `GROUP BY` 直接报错；
  `ORDER BY` 用函数形式（`count(*)`）；`check()` 在 validate 阶段就能拒绝坏聚合。
  独立实测：`3|60|20|10|30`（count/sum/avg/min/max）、`A|2|40` 与 `B|1|20`（按 count 降序）、空 WHERE 得 `0`。

- **SQL 解析器加健壮性测试 `malformed_queries_never_panic`**：24 条畸形/恶意语句（空串、`SELECT COUNT(`、
  未闭合引号、`; DROP TABLE`、NUL 字符、负 LIMIT…）走 `parse`/`check`/`execute` 三条路径都不得 panic——
  `query()` 是从模型表达式里调的，panic 会把整次跑挂掉，而不是报一条坏查询。

- **皮带按 item type 的动态间距（`ConveyorParams.gap_by_type`）**：矩阵 🟡「Gapping」此前只有静态 `item_spacing`。
  现在可按**前车的 `item_type`** 覆盖间距（`[{ item_type, gap }]`），未命中回落 `item_spacing`；空表 = 原路径
  （**3 个 conveyor 基准 SHA 逐位不变**）。**slug 的交互写死**：slug 内部仍按 `slug_min_gap`，
  slug **之间**的下一组首件按"前一组最后一件的类型"查本表。实现上给 `Belt` 增加了与 follower 对齐的 gaps，
  `limit`/`compress`/time-to-next-event 全部按 follower gap，**没有改推进模型**（那是 slug/accumulating 共用的核心路径）。
  独立实测：重复 `item_type` 与负 `gap` 分别报错并点名 `gap_by_type[1].item_type` / `gap_by_type[0].gap`。


### 工具与 CI

- **冒烟套件 24 → 25 项**：新增 `sql aggregates`（`3|60|20|10|30`、`A|2|40` / `B|1|20` 按 count 降序、空 WHERE 得 `0`）。

### 桌面 GUI

- **GUI：皮带的 `gap by item type` 编辑**（勾选后逐行 `type / gap`，空 = 沿用 `item_spacing`）。
  测试 `typed_conveyor_gaps_round_trip`。GUI **156 项**。


### 验证

- `cargo test --workspace --locked`：**1115 passed / 0 failed**；clippy 0 警告、fmt 干净。
- GUI **156 项**、`openflex-core --lib` **338 项**；smoke **25/25**；示例 **66/66**；golden 全过。
- `check-parity-counts.py` ✅ 51 / 🟡 101 / ⛔ 53 / 📄 3；`audit-parity-evidence.py --strict` 通过；
  `check-doc-baselines.sh` 通过（UNIMPLEMENTED / BACKLOG 指向当前版本）。
- 独立实测：`3|60|20|10|30`、`A|2|40` / `B|1|20`（按 count 降序）、空 WHERE 得 `0`；
  重复 `gap_by_type.item_type`、负 `gap` 报错并点名字段。

## v1.57.0 — 2026-11-27

这一版把上一版**明确留白**的那块补上了，并且把一份过期文档变成了会自己报警的东西。

**1. Combiner batch 的逐槽并行。** 上一版多槽只能"同时攒、轮流加工"，吞吐与单槽相同；现在
`max_content > 1` 时每槽有自己的 `phase/token/操作员`，两组 setup/process 可以重叠。
`max_content == 1` 仍走原对象级路径，**4 个基准 SHA-256 逐位不变**。

**关键是它不是新设计的：照抄 MultiProcessor 的形状。** 对象级 busy 取"任一槽非 idle 的**并集**"，
与 MP 同口径。独立实测（stop 10、setup 1 + process 2）：完成数相同，但 **busy 0.6（单槽顺序）vs 0.3（双槽重叠）**——
并集不是累加。冒烟套件把这条钉住了。

**2. `docs/UNIMPLEMENTED.md` 停在"基线 v0.10.0"**，已经落后 46 个版本：它还在说"无每槽独立操作员"（早已实现）、
"表驱动参数更新未支持"（表驱动到达已修）。**过期的清单比没有更糟**——它告诉读者一个已经存在的能力不存在。
已刷新到当前版本，并新增 `scripts/check-doc-baselines.sh`（已进 CI）：文档里的基线版本与 `Cargo.toml` 不一致就失败。

发布节奏保持**至少 3 小时一版**。

**3. TrafficControl 按占用量分档调速（`speed_by_content`）。** 路网里车越多越慢，现在可以表达：
`[{ up_to, factor }]`，占用量 <= up_to 时用该档因子；空表回退单一 `speed_factor`（3 个交通基准 SHA 不变）。

这条路我把**两处语义写死**，因为它们最容易"看起来对、理解错"：

- **占用量 = 该 control 当前已准入的 traveler 数**，在计算该段行程速度时即时读取——不是全局对象数、也不是队列长度；
- **分档因子替换该 control 的 `speed_factor`，不相乘**；多个 control 重叠时仍按原有"各因子相乘"。
  `un_timed` 不做准入、占用量恒为 0，只命中第一档——已写明。

顺带补了一个 schema 漏登记：`traffic_control` kind 此前**没有在 schema 里登记**。

### 文档

- **`docs/UNIMPLEMENTED.md` 停在"基线 v0.10.0"** 已经很久：它还在说"无每槽独立操作员"（早已实现）、
  "表驱动参数更新未支持"（表驱动到达已修）。**过期的清单比没有更糟**——它告诉读者一个已经存在的能力不存在。
  已刷新到 v1.56.0，并新增 `scripts/check-doc-baselines.sh`（已进 CI）：文档里的基线版本与 `Cargo.toml` 不一致就失败。

- **`docs/BACKLOG.md` 也停在"基线 v0.1.0 / 状态总览 v0.10.0"**：和 `UNIMPLEMENTED.md` 同一个毛病，
  但这里更危险——它是"计划文档"，读者会把"状态总览"当现状。现在基线刷新到当前版本，
  "状态总览"明确标为**历史记录**，并在开头指向以矩阵为准。`check-doc-baselines.sh` 现在**同时检查这两份文档**。


### 模型与引擎

- **Combiner batch 逐槽并行**（task-211 里明确留白的那一块，现在补上）：`max_content > 1` 时每槽有自己的
  `phase/token/end/operator_*`，`flow_combiner_batch_parallel` 同时启动所有满料槽；**对象级 busy 取"任一槽非 idle 的并集"**
  （与 MultiProcessor 同一口径，不新增累加语义）。`max_content == 1` 仍走原对象级路径，**4 个基准 SHA 逐位不变**。
  逐槽 setup→process、逐槽操作员（setup/process 可用不同 dispatcher，一槽等操作员不阻塞另一槽的 process）。
  独立实测（stop 10、setup 1 + process 2）：完成数都为 4，但 **busy 0.6（单槽顺序）vs 0.3（双槽重叠）**——正是并集而非累加。

- **TrafficControl 按占用量分档调速（`speed_by_content`）**：矩阵 🟡「按含量分档调速度」。
  两处语义在注释里写死，不留模糊：
  1. **占用量 = 该 TrafficControl 当前已准入的 traveler 数**（`traffic_occupancy`），在计算该段行程速度时即时读取——
     不是全局对象数、也不是队列长度；
  2. **分档因子替换该 control 的单一 `speed_factor`，不相乘**；多个 control 重叠时仍按原有"各 control 因子相乘"。
     `un_timed` 不做准入、占用量恒为 0，只会命中第一档——已写明。
  命中规则：第一个 `occupancy <= up_to`；都不命中用最后一档；空表 = 原路径（**3 个交通基准 SHA 逐位不变**）。
  顺带补了一个 schema 漏登记：`traffic_control` kind 此前**没有在 schema 里登记**。
  独立实测：乱序 `up_to` 与 `factor: 0` 分别报错并点名 `speed_by_content[1].up_to` / `speed_by_content[0].factor`。


### 工具与 CI

- **冒烟套件 23 → 24 项**：新增 `combiner batch parallel`（单槽顺序 busy 0.6；双槽重叠 busy 0.3 —— 并集而非累加）。

### 桌面 GUI

- **GUI：TrafficControl 的 `speed by occupancy` 分档编辑**（勾选后逐档 `up to / factor`，空 = 沿用单一速度因子）。
  测试 `traffic_speed_bands_round_trip`。GUI **155 项**。


### 验证

- `cargo test --workspace --locked`：**1097 passed / 0 failed**；clippy 0 警告、fmt 干净。
- GUI **155 项**、`openflex-core --lib` **332 项**；smoke **24/24**；示例 **66/66**。
- `scripts/acceptance.sh` **all golden checks passed**；`check-parity-counts.py` ✅ 51 / 🟡 101 / ⛔ 53 / 📄 3。
- `check-dead-params.sh` 0 known；`audit-parity-evidence.py --strict` 通过；
  **`check-doc-baselines.sh` 通过（UNIMPLEMENTED / BACKLOG 都指向当前版本）**。
- 独立实测：双槽 batch busy **0.3** vs 单槽 **0.6**（并集非累加）；乱序 `speed_by_content` 与 `factor: 0` 报错点名字段。
- Windows 交叉编译 0 错误；fuzz 冒烟 PASS；1 万对象基准 run 0.24 s、10.4 万件/秒。

## v1.56.0 — 2026-11-26

这一版补的是**装配建模**里最常见的一种形状：一个容器要**不同数量的不同零件**。

**`CombinerParams.components`（逐端口表）**：Pack 模式下每个组件端口可以要自己的数量——
2 个瓶子 + 1 个盖子不再是"总数 3"。`None` 时沿用 `components_per_container`，
**3 个 combiner 基准模型（pack/join/batch）报告 SHA-256 逐位不变**。
实现上把 `ObjectRt.components` 从 `Vec<FlowItem>` 换成保留端口号的 `Vec<(u32, FlowItem)>`。

**一个明确的选择：`join`/`batch` 模式下设 `components` 直接报错**，而不是静默忽略。
错误信息写明"各自模式有独立端口语义"。留一个能被解析、却被引擎忽略的字段，
正是我们删过两次字段的原因。

**文档承认了一件容易坑人的事**：`split` 对象用自己的轮询/按类型分流，**会忽略连接上的 `strategy`**
（实测在连接上写 `by_condition` 不生效）。已在 `COMPATIBILITY.md` 与矩阵里写明，
并给出"把路由点放在下游 processor/queue"的用法——需要按条件路由时，别把策略挂在 split 的出口上。

GUI 里 Combiner 表单多了 `per-port components`（离开 pack 模式会自动收起该表，避免写出校验会拒绝的模型）；
冒烟套件扩到 22 项。

**`CombinerParams.max_content`（多槽，pack 模式）**：先做零行为重构——"单容器 + 扁平组件"换成 `Vec<CombinerSlot>`，
`max_content == 1` 时 **4 个基准模型 SHA-256 逐位不变**（3 个 combiner 基准 + 逐端口 components 模型）。
`> 1` 时 pack 可同时攒多组、组间不串。

**这里我选择了一个"只做一半并写清楚"的做法。** `batch` 模式下多槽只是"能同时攒多组"，
**setup/process 仍按对象顺序执行**，吞吐与 `max_content=1` 相同。
让两个批处理真正并行，必须把 `phase/token/phase_end` 下沉到每个槽，并**重新定义 busy/processing 的统计口径**
（对象级 busy 会从"累加"变成"并集"）——那是统计语义变更，不是功能卡该顺手做的事。
所以矩阵和变更日志里都写明"batch 仍顺序处理"，另开卡做。

发布节奏保持**至少 3 小时一版**。

### 文档

- **文档：`split` 对象忽略连接策略**（实测在连接上写 `by_condition` 不生效，仍按自己的轮询/按类型分流）。已在 `COMPATIBILITY.md` 与矩阵 Send To Port 行列明，并给出"把路由点放在下游 processor/queue"的用法。这类"模型作者以为生效、实际不生效"的差异，比缺功能更容易坑人。

### 模型与引擎

- **Combiner 逐端口 components 表（`CombinerParams.components`）**：Pack 模式下每个组件端口可以要不同数量（2 个瓶子 + 1 个盖子），`None` 时沿用 `components_per_container`，**3 个 combiner 基准模型（pack/join/batch）报告 SHA-256 逐位不变**。实现上把 `ObjectRt.components` 从 `Vec<FlowItem>` 换成保留端口号的 `Vec<(u32, FlowItem)>`。**`join`/`batch` 模式下设 `components` 会被 validate 明确拒绝**（而不是静默忽略），错误信息说明"各自模式有独立端口语义"。实测：[2,1] 时 combiner 收 4 件、完成 1 个容器；缺省时只消费 1 个组件。

- **Combiner 多槽（`CombinerParams.max_content`，pack 模式）**：先做零行为重构——`ObjectRt` 的
  "单容器 + 扁平组件"换成 `Vec<CombinerSlot>`，`max_content == 1` 时 **4 个基准模型 SHA-256 逐位不变**
  （3 个 combiner 基准 + 逐端口 components 模型）。`max_content > 1` 时 pack 可同时 staging 多组、
  组间不串；`max_content: 0` 与 `join + max_content > 1` 均被 validate 拒绝。
  **明确未做（本卡按 A 方案验收）**：`batch` 模式下多槽只是"可以同时攒多组"，**setup/process 仍按对象顺序执行**
  （`phase/token/phase_end` 还是对象级），所以吞吐与 `max_content=1` 相同。
  要让两个批处理真正并行，必须把 phase/token/结束事件下沉到每个槽，并**重新定义 busy/processing 的统计口径**
  （对象级 busy 会从"累加"变成"并集"）——这是统计语义变更，单独开卡，不在功能卡里顺手改。


### 桌面 GUI

- **GUI：Combiner 的逐端口 components 编辑**（`per-port components` 复选框 + 每端口数量；离开 pack 模式时自动收起该表，避免写出校验会拒绝的模型）。测试 `a_combiner_can_need_different_counts_per_port`。GUI **154 项**。

- **GUI：Combiner 加 `max content`（同时可攒几组）与逐端口 components 编辑**；测试断言 `max_content` 按仓库惯例始终序列化（与 `components_per_container` 一致），旧新文件可互换。GUI **154 项**。


### 工具与 CI

- **冒烟套件 21 → 22 项**：新增 `per-port components`（缺省收 2 件；`[2,1]` 收 4 件、完成 1 个容器）。

- **冒烟套件 22 → 23 项**：新增 `combiner multi slot`（`max_content: 2` 的 batch：4 件进、4 件出）。


### 验证

- `cargo test --workspace --locked`：**1084 passed / 0 failed**；clippy 0 警告、fmt 干净。
- GUI **154 项**、`openflex-core --lib` **320 项**；smoke **23/23**；示例 **66/66**；`acceptance.sh` golden 全过。
- `check-parity-counts.py` ✅ 51 / 🟡 101 / ⛔ 53 / 📄 3；`audit-parity-evidence.py --strict` 通过。
- 独立实测：`[2,1]` 逐端口表 → combiner 收 4 件完成 1 个容器；`join` + `components`、`max_content: 0`、
  `join` + `max_content > 1` 均被 validate 拒绝；`max_content: 2` 的 batch 4 件进 4 件出。

## v1.55.0 — 2026-11-26

这一版补的是**可靠性建模**：一个对象可以因为**多个互不相关的原因**停机。

**`ProcessorParams.extra_failures`**：在原有的 `failure` 之外再加独立故障源，每个源有自己的
mtbf / mttr / 首次故障时刻 / 状态计时 / 修理工。对象 down **当且仅当任一源未修复**；
一个源修完只清自己的停机请求，其它源还在修就继续 down，**重叠期间不重复计入 down**。

**又一次"先抽象、后加功能"。** 第一步是零行为重构：`failure: Option<FailureRt>` → `failures: Vec<FailureRt>`，
**4 个带故障的基准模型（plain / 状态计时 / 修理工 / MTBF 组）报告 SHA-256 逐位不变**，
既有 breakdown/repair 测试**一条未改**。在这个过程中还发现并修掉一处单源差异：重新计时会把 per-source 的修理工丢掉。

**明确不做的事**：`extra_failures` 里写 `group` 会被 validate 拒绝，错误信息点名
`extra_failures[0].group`——"多源共享组"是另一个数量级的问题，与其留一个被忽略的字段，不如明确拒绝。

实测：单源（mtbf 10 / mttr 2）down **10.0s**；再加一个 mtbf 25 / mttr 3 的源后 down **16.0s**。

GUI 里处理器表单多了 `+ failure source`（不提供 group 输入，因为校验会拒绝），冒烟套件扩到 21 项。

发布节奏保持**至少 3 小时一版**。

### 模型与引擎

- **多个独立故障源（`ProcessorParams.extra_failures`）**：矩阵 🟡「一个对象可属多个 MTBF/MTTR 组、多个停机请求可叠加」。
  按既定路径**先做零行为重构**：`ObjectRt.failure: Option<FailureRt>` → `failures: Vec<FailureRt>`，
  **4 个带故障的基准模型（plain / count_time_in_states / repair_operator / MTBF group）报告 SHA-256 逐位不变**，
  既有 breakdown/repair 测试**一条未改**。然后加 `extra_failures: Vec<FailureParams>`：每源独立计时，
  对象 down **当且仅当任一源未修复**，一源修完只清自己的停机请求，**重叠期间不重复计入 down**。
  每源可有自己的 `first_failure_time`、`count_time_in_states`、`repair_operator`（不同 dispatcher 也行）。
  **明确不做**：`extra_failures` 里的 `group` 被 validate 拒绝——"多源共享组"是另一个数量级的问题，错误信息会点名字段。
  独立实测：单源 down 10.0s；加一个 mtbf 25 / mttr 3 的源后 down 16.0s。

### 工具与 CI

- **冒烟套件 20 → 21 项**：新增 `extra failure source`（单源 down 10.0s；加 mtbf 25 / mttr 3 的源后 16.0s）。

### 桌面 GUI

- **GUI：处理器的额外故障源编辑器**（`+ failure source`，每源 mtbf/mttr；不提供 group——validate 会拒绝）。测试 `extra_failure_sources_round_trip_without_a_group`。GUI **153 项**。


### 验证

- `cargo test --workspace --locked`：**1069 passed / 0 failed**；clippy 0 警告、fmt 干净。
- GUI **153 项**、`openflex-core --lib` **307 项**；smoke **21/21**；示例 **66/66**。
- `scripts/acceptance.sh` **all golden checks passed**；`check-parity-counts.py` ✅ 51 / 🟡 101 / ⛔ 53 / 📄 3。
- `check-dead-params.sh` 0 known；`audit-parity-evidence.py --strict` 通过；`check-doc-evidence.sh` 47 名可解析。
- 独立实测：单源 down 10.0s → 双源 16.0s；`extra_failures[0].group` 被 validate 拒绝并点名。

## v1.54.0 — 2026-11-26

这一版的主题是**取证**：把"这个 ✅ 到底靠什么撑起来"变成机器可查。

**1. 新增 `scripts/audit-parity-evidence.py`（已进 CI，`--strict`）。**
它把 ✅ 行引用的测试**按所在 crate 分类**：`behaviour`（core/cli 测试，会跑引擎）、`model`（只解析/校验）、
`report`（输出/UI）。**只用模型层测试支撑的 ✅ 会被报出来**——因为"模型对了、引擎忽略该字段"
正是上一版表驱动到达失效的形态。它还会比对台账的结论等级、检查矩阵行内引用的名字是否真是测试函数。

**2. 抓到一个真正的过期引用。** 矩阵与审计文档都还引用 `mixer_delay_zero_keeps_the_report`，
而该测试早已改名为 `mixer_delay_zero_fires_the_hooks_and_keeps_the_report`。
`check-doc-evidence.sh` 只查台账、**从不查矩阵行**，所以改名后一直没人发现。
现在补了 `docs/evidence-non-test-names.txt` 白名单（字段名/事件名/枚举值），
白名单之外"像测试却查无此 fn"的名字会让 CI 失败。

**3. Pick Lists 补行为证据。** 原先只有模型层求值测试。新增 `crates/openflex-core/tests/picklists.rs`：
同一模型只改 picklist 条目值，路由就在两个 sink 之间翻面（4/0 ↔ 0/4）——**证明引擎真的消费了这个列表**。
过程中还确认：**`split` 对象按自己的语义分发输出、忽略连接策略**，所以行为测试必须把路由点放在 processor 上。

**4. 证据等级写明。** Quick Properties / Table Editor 原是"表单往返级"，现在矩阵里**明确标注等级**，
不再与行为级证据混同；Table Editor 另外补了 `editing_the_table_changes_the_arrivals`
（改单元格 → 到达数 2→4），因为它就是编辑器的数据路径。

发布节奏保持**至少 3 小时一版**。

### 工具与 CI

- **新增 `scripts/audit-parity-evidence.py`（已进 CI，`--strict`）**：把 ✅ 行的证据**按测试所在 crate 分类**——
  `behaviour`（core / cli 测试，会跑引擎）、`model`（只解析/校验）、`report`（输出/UI）。
  **只用模型层测试支撑的 ✅ 会被报出来**，因为"模型对了、引擎却忽略该字段"正是上一版表驱动到达失效的形态。
  它还会比对 `docs/PARITY_EVIDENCE.md` 台账的结论等级，并检查矩阵行内引用的名字是否真的是测试函数。
- **抓到一个真正的过期引用**：矩阵与审计文档都还引用 `mixer_delay_zero_keeps_the_report`，
  而该测试早已改名为 `mixer_delay_zero_fires_the_hooks_and_keeps_the_report`。
  `check-doc-evidence.sh` 只查台账、**从不查矩阵行**，所以改名后一直没人发现——现在补了
  `docs/evidence-non-test-names.txt` 白名单（字段名/事件名/枚举值），白名单外的"像测试却查无此 fn"的名字会让 CI 失败。

- **Pick Lists 补行为证据（此前只有模型层求值测试）**：新增 `crates/openflex-core/tests/picklists.rs`——
  同一模型只改 picklist 条目值，路由就在两个 sink 之间翻面（4/0 ↔ 0/4），**证明引擎真的消费了这个列表**。
  这来自新审计工具的分类：`model` 层测试（只验证解析/求值）不足以支撑一个 ✅ 行，
  因为"模型对了、引擎忽略字段"正是上一版表驱动到达的失效形态。
  过程中还确认了一件事：**`split` 对象按自己的语义分发输出、会忽略连接策略**，所以行为测试必须把路由点放在 processor 上。


### 验证

- `cargo test --workspace --locked`：**1061 passed / 0 failed**；clippy 0 警告、fmt 干净。
- smoke **20/20**；示例 **66/66**；`scripts/acceptance.sh` **all golden checks passed**。
- `check-parity-counts.py` ok（✅ 51 / 🟡 101 / ⛔ 53 / 📄 3 / 共 208 行）；
  **`audit-parity-evidence.py --strict` 通过**——51 行 ✅ 中不再有"仅模型层证据"或"查无此测试函数"的行。
- `check-dead-params.sh` 0 known；`check-doc-evidence.sh` 47 名可解析；`check-changelog.sh` 通过。
- 独立实测：`picklist` 条目值 0/1 切换 → KA/KB 的 4/0 与 0/4 对调。

## v1.53.0 — 2026-11-25

这一版的核心是**修掉一个"标着 ✅ 却完全失效"的功能**，以及它带出来的两个补全。

**1. 表驱动到达此前是死的。** 矩阵写着"表驱动到达已接线（`engine.rs` 读全局表）"，
实测却是 **`created 0.0`** —— 一个物品都不创建，而 `validate` 通过。
根因：两个到达表函数只读 `params.arrival_schedule()`，该访问器对表驱动返回 `None`，于是第一行就 `return`。

**这个缺陷上一轮躲过了审计**：原引用测试 `source_can_load_its_arrival_schedule_from_a_table`
**只验证了 `Model::apply_table_reference` 这个解析器**，从没验证引擎用了它。
审计当时只核对"测试名存在"，没核对"测试断言的是行为"——这正是审计方法的边界，现在写进了矩阵。

修法是把 inline/table 到达表在 `Simulation::new` 里**解析一次缓存**，三个消费点改读缓存。
行为测试：1+2+1=4 件、stop 25 时只有前两行、改 quantity 单元格 2→4（**这正是 GUI 表格编辑器的数据路径**）。

**2. `on_create` 的 `rownumber`。** 表驱动到达修好之后，"这一件来自表的哪一行"才有意义：
表驱动/inline 调度时给 **0 起**行号，其它创建路径给 **-1**（明确的"无行号"，不冒充第一行）。
选择"始终暴露、不可用时 -1"而不是"变量不存在"，是为了避免脚本在别的触发器里炸出 `unknown variable`。
实测：3 行表 1/2/1 件 → `row=0,1,1,2`；interarrival → `row=-1`。

**3. 运输器加减速。** `acceleration`/`deceleration` 用梯形/三角形运动曲线，
接进任务序列、`use_transport`、explicit route、elevator 水平段。**未设加速度时精确等于 `距离/速度`**。
实测 d=10、v=1：无加速度 `dropoff@10.0`，加速度 1 → **11.0**；d=0.4 跑不到最高速，三角公式 `1.2649` 与实测一致。

**4. 工具也会犯错。** `changelog-add.py` 把**描述它自己的那段文字里提到的 `## Unreleased`** 当成了区块标题，
于是把条目插进了已发布版本。已改为只认行首标题，并给 `check-changelog.sh` 加了"不允许空区块"规则。

发布节奏保持**至少 3 小时一版**。

### 模型与引擎

- **运输器加减速（`acceleration` / `deceleration`）**：`TransporterParams`/`AgvParams`/`ElevatorParams` 新增两个可选字段。
  单一 `travel_time(distance, speed, accel, decel)` 实现**梯形/三角形**两种运动曲线，接进任务序列 pickup/leg/home、
  `queue.use_transport` 的物品搬运、explicit route、elevator 的水平段。
  **未设加速度时精确返回 `distance / speed`**（3 个基准模型报告 SHA-256 与改动前常量一致）。
  独立实测：d=10、v=1 时无加速度 `item_dropoff@10.0`，加速度 1 → **11.0**（梯形）；d=0.4 时**跑不到最高速**，
  三角公式给出 `1.2649`，实测同值。

- **`on_create` 的 `rownumber`（表驱动到达的行号）**：脚本上下文新增 `rownumber`——表驱动/inline 调度时为**该件来自的表行号（0 起）**，
  其它创建路径为 **-1**。表驱动到达修好之后这个变量才有意义；两者合起来才是完整的"表驱动到达 + 可追溯行"。
  选择"始终暴露、不可用时 -1"而不是"变量不存在"，是为了避免脚本在别的触发器里炸出 `unknown variable`。
  独立实测（3 行表 1/2/1 件）：`on_create` 依次得到 `row=0, row=1, row=1, row=2`；interarrival 模式得到 `row=-1`。


### 工具与 CI

- **修复：表驱动到达此前**完全失效**（虚标的 ✅）。矩阵写着"表驱动到达已接线（`engine.rs` 读全局表）"，
  实测却是 **`created 0.0`** —— 一个物品都不创建，而 validate 通过。
  根因：`handle_schedule_arrival` / `schedule_next_schedule_row` 只读 `params.arrival_schedule()`，
  该访问器对表驱动返回 `None`，两个函数第一行就直接返回。
  原引用测试 `source_can_load_its_arrival_schedule_from_a_table` **只验证了 `Model::apply_table_reference` 这个解析器**，
  从没验证引擎用了它——**这正是"只测解析、不测行为"的典型虚标**，而且它躲过了上一轮审计（审计只看测试名）。
  修法：`Simulation::new` 对每个 source 的 inline/table 到达表**解析一次缓存**（table 失败时报 `object '<name>': …`），
  三个消费点改读缓存；inline 与 table 共用同一份运行时数据。
  行为测试（`crates/openflex-core/tests/table_arrivals.rs`）：`a_table_driven_source_creates_the_rows_items`（1+2+1=4 件）、
  `a_table_driven_source_respects_stop_time`（stop 25 时只有前两行）、
  `editing_the_table_changes_the_arrivals`（改 quantity 2→4、把行推到 500 后变 1——这正是 GUI 表格编辑器的数据路径）。

- **冒烟套件 17 → 19 项**：新增 `table driven arrivals`（全局表 1+2+1 行 → 恰好 4 件；**这条正是那个"标着 ✅ 却完全失效"的功能**，
  现在端到端盯住了）与 `transporter acceleration`（d=10/v=1：无加速度 `dropoff@10.0`，加速度 1 → `11.0`）。

- **冒烟套件 19 → 20 项**：新增 `row number`（表驱动 `on_create` 依次得到 `row=0,1,1,2`；interarrival 得到 `row=-1`）。


### 桌面 GUI

- **运输器/AGV 表单新增 `acceleration` / `deceleration`**（留空 = 匀速，保持旧行为）。
  测试 `a_transporter_records_its_motion_profile`（缺省不序列化、有值时往返）。GUI **152 项**。


### 验证

- `cargo test --workspace --locked`：**1058 passed / 0 failed**；clippy 0 警告、fmt 干净。
- GUI **152 项**、`openflex-core --lib` **299 项**；`scripts/acceptance.sh` **all golden checks passed**。
- smoke **20/20**；示例 **66/66**；`check-parity-counts.py` **✅ 51 / 🟡 101 / ⛔ 53 / 📄 3 / 共 208 行**。
- `check-dead-params.sh` 0 known；`check-doc-evidence.sh` 47 名可解析；`check-changelog.sh` 通过。
- Windows 交叉编译 0 错误；fuzz 冒烟 PASS；1 万对象基准 run **0.24 s**、14.9 万事件/秒。
- 独立实测：表驱动 1+2+1 → `created 4.0`、stop 25 → 2 件；`rownumber` = `0,1,1,2`（interarrival → `-1`）；
  加减速 d=10/v=1 → `dropoff` 从 10.0 变 **11.0**，d=0.4 的三角曲线 `1.2649` 与实测一致。

## v1.52.0 — 2026-11-25

本轮最值得记的不是新功能，而是**审计推翻了我自己的数字**。

**1. `openflex document`（模型文档）**。矩阵里"Model Documentation"一直是 ⛔。现在有命令了：
对象表（参数**去掉默认值**，只显示模型真正设了什么）、连接表、每对象实际挂动作的触发器、
模型级命令/事件/跟踪变量、以及**校验结果**。
一个刻意的选择：它用**宽松加载**，所以**有问题的模型也能出文档**，问题列在文档末尾。
其它命令拒绝无效模型是因为会算出无意义的数；而"文档"的价值恰恰在于让人看清模型哪里不对。
顺带新增 `SimTrigger::ALL`（23 个），保证以后新增触发器不会被文档/审计工具漏掉。

**2. 第二次取证审计（`docs/PARITY_AUDIT.md`）**。对每一行 ✅ 逐行核对"引用的测试是否存在、
断言的是行为还是仅解析/序列化、能否用 CLI 端到端复现"。结论：**48 行已验证 / 2 行证据偏弱 / 0 行需降级**。
审计过程抓出三件事，其中两件是我自己的：

- **First Model 行只有"实测 util 0.763"，没有测试函数** → 已补 3 个行为测试（三条按类型分流 + 返工回边、
  5 复现且 Tester 是利用率最高对象、util 落在 0.7–0.85 带内、同 seed 逐位相同）。**把散文变成断言**。
- **Event List 行虚标**：引用的两个测试只断言 trace JSONL 可写/可读，证明不了"待处理事件列表/快进"。
  我们做的是**已发生事件**的回放，FlexSim 的是**未来待处理事件**列表 → 已降级为 🟡 并写明理由。
- **我一直在用错的 ✅ 行数**：`grep` 把别的表里的 ✅ 也算进去了。实际 **208 行、✅ 50**，而我之前写的是 212 行、✅ 51。
  现在有 `scripts/check-parity-counts.py`（已进 CI）：从表格本身重算并核对汇总行，对不上就失败。

**3. 收回一处 stale 描述**：Model Triggers 行还写着"仍无实验生命周期钩子"，而生命周期钩子早就 ✅ 了。

发布节奏保持**至少 3 小时一版**。

### 报告与 CLI

- **新增 `openflex document <model>`（模型文档自动生成）**：输出 Markdown（对象表含**去掉默认值后的参数摘要**、
  连接表、每对象实际挂了动作的**触发器**、模型级命令/事件/跟踪变量/任务序列、以及**校验结果**），
  `--format json` 给同一份内容的机器可读版本。
  一个刻意的选择：它用**宽松加载**（`parse_json` 不做语义校验），所以**有问题的模型也能出文档**——
  文档里会列出 `**1 issue(s)**` 与逐条位置。其它命令拒绝无效模型是因为会算出无意义的数；而"文档"的价值恰恰在于
  让人看清模型哪里不对。
- **`SimTrigger::ALL`**：模型 crate 新增触发器全集常量（23 个），文档/审计工具枚举触发器时不会再漏掉新加的。

### 工具与 CI

- **矩阵汇总行自动核对（`scripts/check-parity-counts.py`，已进 CI）**：从表格本身重算 ✅/🟡/⛔/📄 与总行数，
  与"矩阵共 N 行"那句对不上就失败。起因是**审计发现我一直在用错的 ✅ 行数**（grep 把别的表里的 ✅ 也算进去了）：
  实际是 **197 行、✅ 49**，而汇总里写的是 212 行、✅ 51。
- **First Model 补行为测试**（审计发现该行只有"实测 util 0.763"、没有测试函数）：
  新增 `crates/openflex-core/tests/examples.rs`——三条按类型分流 + 返工回边、5 复现且 Tester 是利用率最高对象、
  util 落在 0.7–0.85 带内、同 seed 逐位相同。**把散文变成断言**。
- **Event List 降级为 🟡**（同一次审计）：我们做的是**已发生事件**的记录/回放，FlexSim 的是**未来待处理事件**列表；
  原先引用的两个测试只断言 JSONL 可写/可读，证明不了"待处理事件列表"。

### 验证

- `cargo test --workspace --locked`：**1042 passed / 0 failed**；clippy 0 警告、fmt 干净。
- GUI **151 项**、`openflex-core --lib` **287 项**；`scripts/acceptance.sh` **all golden checks passed**。
- smoke **17/17**；示例 **66/66**（validate + 运行 + 同 seed 逐字节相同 + `document` 可生成）。
- `check-dead-params.sh` 0 known；`check-doc-evidence.sh` 47 名可解析；`check-changelog.sh` 通过；
  **`check-parity-counts.py` 通过（✅ 50 / 🟡 102 / ⛔ 53 / 📄 3 / 共 208 行）**。
- 独立实测：`openflex document` 对 66 个示例全部生成成功；对**含重复对象名**的模型也成功，
  并在文档末尾列出 `**1 issue(s)**: objects.S: duplicate object name`。

## v1.51.0 — 2026-11-25

本轮的**主线是一次"先抽象、后加功能"的成功案例**，以及把"改了什么没改什么"变成可验证事实。

**1. `OperatorRequest` 抽象（零行为变化）。** 请求队列从 `VecDeque<usize>` 换成带 `{requester, id, phase}` 的结构。
验收标准就是它存在的理由：**6 个基准模型（3 MP + 3 operator-heavy）的报告 SHA-256 与改动前完全一致**，
现有 operator/transport 测试**一条未改**全绿。checkpoint 也处理干净了：`Serialize` 仍输出 `u64`（旧形状），
`Deserialize` 兼容旧整数与新结构——旧存档可读。

**2. 按步操作员（上一版**故意删掉**的字段）现在真的接线了。** 该步开始前申请、本步结束即释放、多槽各自等待不串锁。
上一版我们把 `ProcessStep.num_operators` 删掉了，因为引擎读不了它；抽象落地后才加回来。
实测差别：同一模型只给某步加上 `num_operators: 1`，MP 的 busy 分布就从 0 变成 0.6——字段不再是装饰。

**3. 两条新的回归网。**
- `scripts/check-examples.sh`（已进 CI）：**66 个示例模型**逐个 validate + 同 seed 跑两次，报告必须逐字节相同。
  示例是"能执行的文档"，引擎改动在那里出问题，单测未必覆盖得到。
- 冒烟套件已在上一版扩到 17 项。

**4. 把我自己犯过 4 次的错误做成机制。** CHANGELOG 条目总被插进**已发布**区块（还重复过标题），
现在有 `scripts/changelog-add.py`：先定位 `## Unreleased`，只在区块内插入，找不到就报错。

发布节奏保持**至少 3 小时一版**。

### 桌面 GUI

- **MultiProcessor 工序列表编辑器**：`steps (n)` + `+ step`，每步可勾选 `setup` / `process` 并编辑对应分布，
  也可删除。**未勾选 = 字段缺省**（继承对象级 setup/process），所以空列表与旧模型 JSON 逐字节一致。
  测试 `multiprocessor_steps_serialise_only_when_present`。GUI **151 项**。

- **MultiProcessor 每步的操作员数**：步骤行的 `operators` 复选框 + 数字（未勾选 = 字段缺省，继承对象级 crew）。
  测试扩展到断言 `num_operators: 2` 会被序列化。GUI **151 项**。


### 模型与引擎

- **`OperatorRequest` 前置抽象（零行为变化）**：`Dispatcher`/`Resource` 的请求队列从 `VecDeque<usize>`
  换成 `VecDeque<OperatorRequest { requester, id, phase }>`。`id`/`phase` 目前**只做簿记**，
  授予逻辑仍按 `requester`，因此**没有任何可观察变化**。
  验收方式就是这张卡的全部价值：**6 个基准模型（3 个 MP + 3 个 operator-heavy：`max_tasks`、resource 钩子、修理工）
  的报告 SHA-256 与改动前常量完全一致**，且现有 operator/transport 测试**一条未改**全绿。
  **checkpoint 兼容**处理得也干净：`ObjectRt.requests` 会被序列化，自定义 serde 让 `Serialize` **仍输出 `u64`**（与旧形状一致）、
  `Deserialize` 同时接受旧整数与新结构，`id`/`phase` 不落盘——旧 checkpoint 可读、新 checkpoint 形状不变。
  为后续两张卡铺路：MultiProcessor 按步操作员、Use Transport 的完整语义。

- **MultiProcessor 的按步操作员**：`ProcessStep.num_operators` 真正接线——该工序开始前申请 n 个操作员、
  **本步结束即释放**（不是整个 busy period），多槽各自等待/授予、不串全局锁。
  这是上一版**故意留空**的字段：当时引擎读不了它，所以字段被删掉了；`OperatorRequest` 抽象落地后才加回来。
  **未设 `num_operators` 的模型报告逐位不变**（含"有 steps 但都没设"这种中间态），
  `check-dead-params.sh` 0 known 证明字段被真实读取。
  实测差别：同一模型只把某步加上 `num_operators: 1`，MP 的 busy 分布就从 0 变成 0.6——字段不再是装饰。


### 工具与 CI

- **CHANGELOG 插入助手**：`scripts/changelog-add.py --section "模型与引擎"`。这四次同一个错误（条目落进**已发布**区块、还重复了标题）终于被"机制"解决——助手先定位 `## Unreleased` 区块，只在区块内插入，找不到区块就报错，绝不全局替换。

- **新增 `scripts/check-examples.sh`（已进 CI）**：**66 个示例模型**逐个 `validate` + 运行两次同 seed，
  两次报告必须**逐字节相同**。示例是"能执行的文档"，引擎改动在那里出问题，单测未必覆盖得到。
  当前结果：`all 66 example models validate, run, and are deterministic`。

### 验证

- `cargo test --workspace --locked`：**1038 passed / 0 failed**；clippy 0 警告、fmt 干净。
- GUI **151 项**、`openflex-core --lib` **287 项**；`scripts/acceptance.sh` **all golden checks passed**；
  smoke **17/17**；**`scripts/check-examples.sh` 66/66**（新）。
- `check-dead-params.sh` 0 known；`check-doc-evidence.sh` 47 名可解析；`check-changelog.sh` 通过。
- Windows 交叉编译 0 错误；fuzz 冒烟 PASS；1 万对象基准 run **0.24 s**、15.4 万事件/秒。
- 独立实测：只给某步加 `num_operators: 1` → MP 的 busy 分布从 0 变 0.6；`num_operators: 0` 被 validate 拒绝。


## v1.50.0 — 2026-11-25

本轮补上一块**核心建模语义**：物品不再只能"瞬移"到下游。

**`queue.use_transport`** —— 指定 dispatcher 后，物品要**等一台 transporter 来搬**，搬运期间仍计入 queue 内容。
实测（10 单位距离、速度 2）：下游 `enter` 在 **t=5**，对照的瞬时流模型是 **t=0**；3 件依次在 5/15/25 完成且守恒。
**运输途中存盘 → resume 与不中断直跑的报告逐位相同**。

实现上刻意**不碰** `Dispatcher.requests`（那会让既有 operator 任务回归），只从 dispatcher 的空闲车直接派；
因此明确不做多车协同/preemption/多出边，边界写在矩阵与 BACKLOG 里。

**两处"减法"比加法更值得记：**
- MultiProcessor 的顺序工序列表落地了（两步 2.0 → 4.0，含 setup → 4.5），但**每步的操作员数没有做成字段**：
  引擎只能按步累加时长，按步申请/释放 crew 需要把 operator 分配从"对象级"改成"对象+槽级"。
  **与其留一个 validate 接受、引擎忽略的字段，不如先不加**——`MixerStep.delay` 的教训直接应用。
- 队友两次在动手前停下交成本报告（一次给窄版拆分，一次判定"加个侧表就行"是幻觉），
  这比交付一个"看起来能用"的假功能有价值。

冒烟套件从 11 项扩到 **17 项**，现在覆盖 `use_transport` 与工序链。
发布节奏保持**至少 3 小时一版**。

### 模型与引擎

- **MultiProcessor 的顺序工序列表**：`MultiProcessorParams.steps`（空不序列化）——物品占一个槽位后
  **按顺序**跑完每道工序（每步 setup → process），全部完成后才释放；多槽并行不受影响。
  实测：两步各 2.0 → 下游 `enter` 在 **t=4.0**；setup 1 + process 2 + setup 0.5 + process 1 → **t=4.5**；
  不写 `steps` 仍是 1.0（原行为）。逐位不变仍用**改动前记录的报告 SHA-256 常量**钉住。
  **一个刻意的减法**：原计划给每步加 `num_operators`，但引擎本轮只实现到"按步累加时长"，
  按步申请/释放操作员需要动 MP 的 crew 状态。**与其留一个 validate 接受、引擎忽略的字段，不如先不加**——
  所以 `ProcessStep` 里干脆没有这个字段，注释写明原因。这正是 `MixerStep.delay` 教训的直接应用。



- **`queue.use_transport`（物品要等 transporter 来搬，而不是瞬时流走）**：FlexSim 里最常见的建模形态之一。
  窄切片：**单出边 Queue** + 指定 dispatcher；物品在搬运完成前**仍计入 queue 内容**，
  由 dispatcher 的空闲 transporter 直接取车（**不碰 `Dispatcher.requests`**，避免回归既有 operator 任务），
  事件链 `item_pickup → item_depart → item_dropoff`，距离/速度/装载时间全部复用既有实现。
  实测（10 单位距离、速度 2）：`item_dropoff` 在 **t=5**，下游 `enter` 同刻；对照的瞬时流模型是 t=0。
  3 件物品依次在 5 / 15 / 25 完成，`created 3 = completed 3`（守恒）。
  **逐位不变的钉住方式值得记**：不是"同 seed 跑两遍"，而是**改动前先记录 3 个随机生成 queue 模型的报告 SHA-256 常量**，
  字段缺省时报告必须与那些常量一致——把"95% 有把握"变成可验证。
  validate 拒绝：空串、指向非 dispatcher、无可用执行器、**非唯一出边**（`exactly one output connection`）。
  **第二刀也已完成**：运输途中（t=2、物品在车上）存盘 → resume 到 t=20，与不中断直跑的报告**逐位相同**；
  trace 链路断言 `item_pickup@0 → item_depart@0 → item_dropoff@5 → enter K@5`（装载段间隔 == 距离/速度，1e-6）；
  同 seed 两次运行逐位相同。**明确未做**：无车唤醒精细化、下游 blocked 更强语义、多车/多出边/preemption。
- **GUI**：Queue 表单新增 `use transport` 选择器（指向 dispatcher）。测试
  `a_queue_can_name_a_dispatcher_to_carry_its_items`（含"清空选择必须写 `None` 而不是空串"）。GUI **150 项**。


### 工具与 CI

- **冒烟套件从 11 项扩到 17 项**：新增 `OnChange`（`time` 变量 interval 5 → 恰好在 10/15/20/25/30 触发）、
  **Combiner pack**（端口 0 是容器、端口 1 是组件；10s 一个容器 → 恰好 10 件）、
  **Timetable/off-shift**（窗口外不加工）、**checkpoint 续跑**（t=15 存盘 → t=30 resume，与直跑的
  `completed`/`created` 完全相同）、**Queue use_transport**（瞬时流 vs 等车：t=0 vs t=5，且 `item_*` 链路有序）、
  **MultiProcessor 工序链**（两步 2.0 → 4.0，含 setup → 4.5）。
  两条新用例**第一次写都是错的**：combiner 端口顺序反了（恒 0 件），checkpoint 最初想比整份报告（噪声大）。
  修正的是**模型**，不是断言——这个习惯比套件本身更重要。



### 验证

- `cargo test --workspace --locked`：**1029 passed / 0 failed**；clippy 0 警告、fmt 干净。
- GUI **150 项**、`openflex-core --lib` **279 项**；`scripts/acceptance.sh` **all golden checks passed**；
  **`scripts/feature-smoke.py` 17/17**。
- `check-dead-params.sh` 0 known；`check-doc-evidence.sh` 47 名可解析；`check-changelog.sh` 通过。
- Windows 交叉编译 0 错误；fuzz 冒烟 PASS；1 万对象基准 validate **0.09 s / 29 MB**、run **0.24 s / 58 MB**。
- 独立实测：`item_pickup@0 → item_depart@0 → item_dropoff@5 → enter K@5`；运输途中 t=2 存盘 resume 报告
  sha256 与直跑相同（`991a3f5f…`）；工序链 4.0 / 4.5 两例。

## v1.49.0 — 2026-11-25

本轮做的是**完整性与最后一公里**，不是新对象。

**1. 11 项端到端冒烟（已接入 CI）。** 每个用例都建一个小模型、跑一次 CLI、断言**外部可观察的结果**。
它防的是「单测通过但功能从外面根本用不到」——死参数、被接受却忽略的动作、永不触发的钩子。
写脚本时第一条液位标记用例**自己就是错的**（罐子排空太快、液位不涨），修正模型后才通过；
这恰好说明这类冒烟的价值：**默认怀疑模型，而不是怀疑功能**。

**2. 日志一路可读到最后一公里。** 上一轮发现 CLI 自己那份 `write_trace` 丢掉了日志文本；
本轮把剩下两个缺口也补上：GUI 的 `Log (n)` 面板与 `openflex run --logs`（日志走 stderr，
所以 `--format json` 的 stdout 仍可解析——测试里显式断言了这一点）。
现在同一条链是：`log()` → `TraceEvent.message` → JSONL 第六键 → `trace --logs` / `run --logs` / GUI 面板。

发布节奏保持**至少 3 小时一版**。

### 桌面 GUI

- **运行日志面板**：`Log (n)` —— 上一次 **Trace run** 里所有 `log()` 消息，按「时间 / 对象 / 文本」列出
  （可滚动）。此前 GUI 完全不显示脚本与钩子的日志，用户只能去 grep 导出的 JSONL；现在
  `TraceEvent.message`（本轮上一版刚让它进 trace）直接上屏。测试 `script_logs_become_panel_rows`。GUI **149 项**。


### 报告与 CLI

- **`openflex run --logs`**：把脚本与钩子里 `log()` 的消息打到 **stderr**（`--format json` 的 stdout 保持可解析，
  测试里显式断言了这一点），没有日志时明确说 `no script log messages` 而不是静默。隐含开启 tracing。
  测试 `run_logs_prints_messages_without_corrupting_stdout`。
  与 GUI 的 `Log (n)` 面板、`trace --logs`、以及 JSONL 第六键 `message` 是同一条链：
  **日志一路可读到最后一公里**，而不是只存在某个内部结构里。

### 工具与 CI

- **新增 `scripts/feature-smoke.py`（11 项端到端冒烟，已接入 CI）**：每个用例都建一个小模型、跑一次 CLI、
  断言**外部可观察的结果**（数值 / trace 事件 / 复现的错误）。它防的是「单测通过但功能从外面根本用不到」
  这一类问题——死参数、被接受却忽略的动作、永不触发的钩子。
  覆盖：slug 组、合流控制器（3:1 权重比落在 2.2–4.0 带内）、光电门边界时刻（9.5/10.5）、决策点（4.5）、
  Motor 停/开（0 vs 5 件）、User Event（t=10 一次）、MTBF 组（有组时 P2 也 down、无组时 0）、
  全局任务序列（与内联逐字节相同）、协同步骤（两名执行器都 `transport_arrive`）、
  trace 保留 `log()` 文本、流体液位标记（一次上升）。
  写脚本时第一条液位标记用例**自己就是错的**（罐子排空太快、液位不涨），修正模型后才通过——
  这恰好说明这类冒烟的价值：默认怀疑模型，而不是怀疑功能。



### 验证

- `cargo test --workspace --locked`：**1014 passed / 0 failed**；clippy 0 警告、fmt 干净。
- GUI **149 项**；`scripts/acceptance.sh` **all golden checks passed**；
  **`scripts/feature-smoke.py` 11/11 通过**（新，已进 CI）。
- `check-dead-params.sh` 0 known；`check-doc-evidence.sh` 47 名可解析；`check-changelog.sh` 通过。
- 独立实测：`run --logs` 把 4 条钩子日志打到 stderr，同次 `--format json` 的 stdout 仍是合法 JSON（completed=3.0）。

## v1.48.0 — 2026-11-25

本轮的主题是「**共享**」与「**别丢数据**」。

- **MTBF 组**：同组对象共享一次故障/维修周期——组故障时全员 `down`、修复后全员恢复。
  决定性验证是"有组 vs 无组"的对照：同一模型有 `group` 时 P1 与 P2 **都** down 2/30，
  去掉 `group` 只有 P1 down（P2 从 mtbf=100 抽样，30 秒内不会坏）。
- **trace 不再丢日志文本**：CLI 自己有一份 `write_trace`，与 report 里的实现重复，
  它把 `TraceEvent.message` 丢掉了，于是保存下来的 trace 里所有 `script_log` 都看不到内容。
  已删掉 CLI 的副本、统一委托给 report（单一实现），并新增 `openflex trace --logs`。

顺带补上 GUI 里**完全缺失**的故障编辑器（`mtbf`/`mttr`/维修操作员/状态白名单/`failure group`）——
而这是核心建模能力，此前只能改 JSON。
发布节奏保持**至少 3 小时一版**。

### 模型与引擎

- **MTBF 组（共享故障/维修）**：`FailureParams.group` —— 同名对象共享**一次**故障周期：
  组故障时全员 `down` 且各自触发 `on_breakdown`，修复完成时全员恢复且各自 `on_repair`；组间互不影响。
  实现上组由 **model order 第一个成员（driver）** 持有并调度一次共享时钟，非 driver 不再各自 arm，
  所以"到底用谁的 mtbf/mttr"是可预测的（validate 还要求同组 `mtbf`/`mttr` 一致，避免隐式歧义）。
  **决定性验证**：同一模型下，有 `group` 时 **P1 与 P2 都 down 2/30**；去掉 `group`（P2 从 mtbf=100 抽样）时**只有 P1 down**——
  证明组真的改变了行为，而不是被忽略。
  checkpoint：组状态不另建易漂移的 map，driver 的 `FailureRt`、全员 `downtime`/`repair_waiting` 都在 `ObjectRt`，
  deadline 在事件日历；t=6 存盘 resume 与直跑逐位相同。无 `group` 的既有单对象路径逐位不变。

### 报告与 CLI

- **保存的 trace 现在带 `log()` 文本**：CLI 自己有一份 `write_trace`（与 report 里的 `write_trace_jsonl` 重复），
  它把 `TraceEvent.message` 丢掉了，于是 `run --trace` 存下的 trace 里所有 `script_log` 的 `data` 都是 `{}`、
  **日志内容根本看不到**。已把 CLI 的写入器**删掉、统一委托给 report 的那一份**（单一实现，避免再漂移），
  并给 JSONL 增加**可选第六键 `message`**（仅当日志非空时出现，旧文件与旧读者都不受影响）。
  新命令 `openflex trace <file> --logs`：按时间列出 `script_log` 的消息。测试 `trace_files_carry_script_log_messages`。


### 桌面 GUI

- **故障（breakdowns）编辑器**：GUI 此前**完全没有**故障参数编辑入口——`mtbf`/`mttr`/`repair_operator`/
  `first_failure_time`/`count_time_in_states` 全都只能改 JSON，而这是核心建模能力。
  processor 表单新增 `breakdowns` 勾选框（**不给套一个全零块**，要用户显式开启），勾上后出现
  mtbf/mttr 分布编辑器、维修操作员选择、`first failure`（-1 = 从 mtbf 抽样）与「状态白名单」文本框
  （逗号分隔、去空、空 = 墙钟时间）。测试 `a_breakdown_block_round_trips_and_state_scoping_parses`。GUI **148 项**。



### 验证

- `cargo test --workspace --locked`：**1012 passed / 0 failed**；clippy 0 警告、fmt 干净。
- GUI **148 项**、`openflex-core --lib` **265 项**；`scripts/acceptance.sh` **all golden checks passed**。
- `check-dead-params.sh` **0 known**；`check-doc-evidence.sh` 47 名可解析；`check-changelog.sh` 通过。
- 独立实测：有组 → P1/P2 都 down 2/30；无组 → 只有 P1 down；trace 文件里 `"message":"RTS"` 可读，
  `trace --logs` 按时间列出。

## v1.47.0 — 2026-11-24

本轮补了两组触发器（任务交接与资源释放），并把 GUI 里**完全无法编辑**的跟踪变量补上编辑面板。

两处值得记的处理方式：
- `on_receive_task_sequence` 在 FlexSim 里可以**修改**任务序列；我们的序列是模型级声明、运行时只读，
  所以这个钩子只承诺"触发 + 副作用"，并把这条边界**写进 Rust 文档注释**——不是含糊地做个半成品。
- `on_resource_available` 覆盖了 **resource 池位释放**与 **transporter `finish_task`** 两个来源；
  shift 结束、维修完成等其它可用性变化明确记为未覆盖。

另外把「改动 CHANGELOG 时条目插错区块」这个我犯了三次的错误变成了 CI 检查：
它当场在**已发布的历史区块**里找出 **7 处重复的 `###` 标题**（都是那几次错误替换留下的），已全部合并。
发布节奏保持**至少 3 小时一版**。

### 模型与引擎

- **`on_receive_task_sequence` 触发器**：执行器被分派到任务序列、**开始执行第一腿之前**触发一次（每任务恰好一次，
  普通与协同分支都触发）。**能力边界写进 Rust 文档注释**：我们的任务序列是模型级声明（`task_sequence`/GTS），
  运行时只读，所以**此钩子不能修改序列**，只能做副作用（`log`/`set_label`/`send_message`/`set_motor`）。
  时点说明也写清了：FlexSim 的"收到序列"事件在我们这里不存在，因此定义为"开始执行第一腿之前"。
- **`on_resource_available` 触发器**：资源重新可用时触发，**两个来源都覆盖**——
  `resource` 池位释放与 transporter 的 `finish_task`。
  实测（3 件、每件一次派发）：`on_receive_task_sequence` 在 t=0/20/40 各一次，
  `on_resource_available` 在 t=4/24/44 各一次，两个钩子的脚本各执行 3 次。
  边界：shift 结束、维修完成等其它"可用性变化"不触发（记入矩阵的未覆盖说明）。
- 两个钩子都进了 `collect_model_scripts` 的手写清单（第 7 次提，这次一开始就加）。
  验收里特意加了一条 **"钩子里 `send_message` 能改变下游行为"**——证明副作用真的送达模型，而不只是写了一条 trace。

### 桌面 GUI

- **跟踪变量面板**：GUI 此前**完全不能编辑** `tracked_variables`（只能改 JSON），而这些变量正是报告里
  series / state 占比 / staytime 分布的数据来源。新增 `Tracked variables (n)` 面板：JSON 缓冲 + Apply，
  外加 `sample every`（`tracked_interval`）数字框；提示写明四种 `kind` 与"`on_change` 只在 level 采样变化时执行"。
  测试 `tracked_variables_round_trip_through_the_panel_buffer`（含"summary 类变量带 `on_change` 被 validate 拒绝"）。
  GUI **147 项**。


### 验证

- `cargo test --workspace --locked`：**1001 passed / 0 failed**；clippy 0 警告、fmt 干净。
- GUI **147 项**、`openflex-core --lib` **253 项**；`scripts/acceptance.sh` **all golden checks passed**。
- `check-dead-params.sh` **0 known**；`check-doc-evidence.sh` 47 名可解析；**新增 `check-changelog.sh`** 已接入 CI。
- Windows 交叉编译 0 错误；fuzz 冒烟 PASS；大模型基准（1 万对象）validate **0.08 s / 24 MB**、run **0.23 s / 58 MB**。
- 独立实测：3 件模型下 `on_receive_task_sequence` 在 t=0/20/40 各一次、`on_resource_available` 在 t=4/24/44 各一次。

## v1.46.0 — 2026-11-24

本轮又抓到一个**"声明了但代码没做"**的参数：`MixerStep.delay`。注释写着 "waits out the step delay"，
代码却在材料收齐的**同一 tick 立即分发**——那个 delay 从来不存在。

这让一张原本很小的卡（"加两个步前/步后钩子"）差点变成在**不存在的时间点上**挂两个永不触发的钩子。
队友按先停手条款先交了现状报告，我们才改成"先让 delay 真的生效，再挂两个真时点的钩子"。

它也暴露了我的检查工具的盲区：`check-dead-params.sh` 靠"字段名在引擎里是否出现过"判断，
而 `delay` 是通用词——**绿色只代表没有明显死字段，不是证明**。我已把检查扩展到嵌套参数结构体，
并把这个盲区写进脚本注释。

另外新增 **OnChange 触发器**（跟踪变量变化时执行动作）与 **GUI 用户命令面板**。
发布节奏保持**至少 3 小时一版**。

### 模型与引擎

- **`OnChange` 触发器（跟踪变量值变化时执行动作）**：`TrackedVariable.on_change`（空不序列化，旧模型 JSON 与报告逐位不变）。
  复用**既有的跟踪采样点**（`tracked_interval`），不另造轮询；"上一次采样值"用的是采样器本来就在维护的
  `tracked_prev`（kinetic 用它，且**随 checkpoint 持久化**）——所以不存在"把变化定义成和 0 不同"的假语义。
  三条约定写进文档：变化判定基于**采样值**（不是连续抖动）、**首次采样不算变化**、高频变量（如 `time`）
  每次采样都会触发一次（事件量 = 采样次数，作者应自行控制 `tracked_interval`）。
  实测：`expression: "time"`、interval 5、stop 30 → 恰好 **5 次**，时刻 `[10, 15, 20, 25, 30]`；常量变量 **0 次**。
  `kind != level`（summary/kinetic）却给 `on_change` → validate 明确拒绝：
  "only `level` variables have a previous sample value (summaries have none)"——静默忽略比报错更糟。
  动作是**模型级**的（与 user events 共用同一个 `run_model_actions`），脚本错误带变量名与模型时间。

### 桌面 GUI

- **用户命令面板**：模型级 `User commands (n)` —— 与 user events / 模型触发器同款「JSON 缓冲 + Apply」，
  首次打开从模型灌入、之后保留用户输入，应用走 `commit_model_edit` 单一入口。提示写明"任脚本里可当
  `name()` / `name(3.0)` 调用"。测试 `user_commands_round_trip_through_the_panel_buffer`
  （含"包装后的函数体以 `fn setx(v)` 开头"的断言）。GUI **146 项**。

- **`MixerStep.delay` 真正生效**（此前**声明了但引擎从未读取**——注释写着 "waits out the step delay"，
  代码却在收齐的同一 tick 立即分发）。现在：材料收齐后设 `mixer_delay_until`（期间不输出）、
  调度新事件 `MixerDelayDone`，下一次 fluid step 才分发并进入下一步。
  `delay == 0` **不排任何等待事件**（既有模型逐位不变，钉住测试），但两个钩子**仍然触发**（见下条）。
  这又是一个"死参数"——而且它躲过了 `check-dead-params.sh`：因为 `delay` 是通用词，引擎里到处都有同名出现。
- **流体混合器步前/步后延迟触发器**：`MixerStep.on_before_delay`（收齐后、delay 开始前）与
  `on_after_delay`（delay 结束事件里）。实测 delay=5 时钩子成对出现且**每对间隔精确 5.0**
  （`[1.0, 6.0] / [6.0, 11.0] / [11.0, 16.0] …`），这是"两个时点真的不同"的硬证据。
  钩子是对象级（probe item，无真实流项目），脚本错误带 mixer 名与模型时间。
  **`delay == 0` 时两个钩子仍然触发**（同刻成对），而不是静默不触发——用户配了 `on_after_delay` 却一行日志都没有，
  等于一个没有信号的失败。实现上不排任何额外等待事件，所以**事件序列与数值仍完全不变**。
  实测零延迟：`[1.0, 1.0] [2.0, 2.0] [3.0, 3.0] …`（成对同刻）；delay=5：`[1.0, 6.0] [6.0, 11.0] …`（间隔 5）。
  顺带修了一个真实隐患：零延迟时钩子会按已 `+1` 的 `mixer_step` 取到**下一步**的动作，
  现在改为由调用方传入当前 step。
- **我的检查工具修了一个盲区**：`check-dead-params.sh` 原先只查顶层 `*Params`，**嵌套参数结构体**
  （`MixerStep`/`PhotoEye`/`DecisionPoint`/`FluidMark`/`TaskStep`… 共 19 个）从未被检查。现已覆盖。
  同时把它真正的盲区写进脚本注释：**通用词字段名（`delay`/`size`/`count`）靠文本匹配证明不了"未被使用"**，
  绿色运行只代表"没有明显死字段"，不是证明。


### 报告与 CLI

- **`openflex eval` 现在能看到模型里定义的用户命令**：它此前用 `ScriptEngine::new()`，而模型内的脚本用
  `with_commands(model)`——于是 `eval --script 'mycommand();'` 会报 "Function not found"，而同一段脚本在模型里却正常。
  已改为同一个引擎。另外：命令定义会前置到源码，导致 rhai 的**行号偏移**，`eval` 出错时会打一行说明
  （不打印我算不准的具体偏移量——宁可说"有偏移"也不给错数字）。测试 `eval_can_call_the_models_user_commands`。


### 验证

- `cargo test --workspace --locked`：**996 passed / 0 failed**；clippy 0 警告、fmt 干净。
- GUI **146 项**、`openflex-core --lib` **249 项**；`scripts/acceptance.sh` **all golden checks passed**。
- `check-dead-params.sh` **0 known**（现已覆盖 42 个顶层 + 19 个嵌套参数结构体）；`check-doc-evidence.sh` 47 名可解析。
- 独立实测：`time` 跟踪变量 interval 5 / stop 30 → **5 次**（`[10,15,20,25,30]`），常量 0 次；
  mixer delay=5 → 钩子成对且间隔精确 **5.0**（`[1.0,6.0] [6.0,11.0] …`）；delay=0 → 成对**同刻**（`[1.0,1.0] [2.0,2.0] …`）且报告不变。

## v1.45.0 — 2026-11-24

本轮的看点是**协同任务序列**（FlexSim 的"双人抬"）：一个任务步骤可以声明需要**两名执行器**，
两者都到场才同步开始、结束时各自释放。

这个功能差点被做成假的。第一轮队友只交了一份**成本报告**——它先证明了"同一 requester 可以有多个请求，
`serving_requester` 是每执行器的单值"，所以不需要重写请求结构，确认后才动手。
如果当时直接写，很可能做出一个"看起来协同、实际串行"的东西；我们上一轮刚把虚标参数清干净，不能同一天再造一个。

补漏也补齐了：不支持的组合**加载即被 validate 拒绝**（不是文档里写一句"不支持"）、抢占**整组取消不卡死**、
屏障判据有专测。顺带修了 4 处用户可见拼写。
发布节奏保持**至少 3 小时一版**。

### 模型与引擎

- **协同任务序列（`involved: 2`）**：`TaskStep::Detailed.involved`（默认 1、空不序列化，`Simple` 等价 1），
  声明"完成这一步需要两个执行器"。语义：dispatcher **一次派两个执行器**并写入同一 `task_group`；
  **两个都 `TransportArrive` 才开始**（屏障在 explicit 与 append 的共同入口，先到者只登记、不轮询）；
  结束时两个各自释放。实测：两名执行器同刻 `transport_arrive`；`involved: 1` 的报告与"完全不写 `involved`"
  **sha256 相同**；只有一名执行器时 `completed 0` 且**整场仅 4 个事件**（永久等待且不忙等，按设计）。
  **先停手条款起作用了**：这张卡第一轮只交了一份成本报告（"同一 requester 可以有多个请求，`serving_requester`
  是每执行器的单值，所以不需要重写请求结构"），确认后才实施，避免了一个假功能。
  **边界（如实记录）**：
  - `involved: 2` 与 `explicit` 模式不兼容，现在**加载即被 validate 拒绝**（覆盖内联序列、命名序列、以及 ref+explicit 三条来源），
    而不是只在文档里写一句"不支持"；
  - **抢占**有专测：高优先级抢走一个组员时**整组取消**、另一组员被唤醒按普通任务继续，
    断言 `task_groups` 为空（无泄漏）、事件数有界（无忙等）、任务仍能完成；
  - **AGV 途中充电**：实测引擎的充电发生在 `finish_task` 之后（AGV 到达之后才充电），所以没有"把充电中当已到场"的 bug；
    为覆盖真实风险，屏障判据有专测 `coordinated_barrier_waits_for_every_member`（只有一个成员到达时另一成员不计入、
    等待期间事件堆长度不变、后到者唤醒先到者）。仍未覆盖"AGV 真的在 pickup 段耗尽电量"的整链夹具。
- 顺带修了 4 处用户可见拼写（`executers` → `executors`）。


### 桌面 GUI

- **任务步骤的"×2 执行器"开关**：dispatcher 的步骤编辑器每行加一个 `×2` 复选框（悬停说明"需要两个执行器同时到场才开始，仅 append 模式"）。
  勾选会把 `Simple` 步骤自动升级成 `Detailed`（保留原 kind），取消勾选写回 `involved: 1`——后者**不序列化**，所以与旧模型逐字节一致。
  测试 `a_step_can_be_made_coordinated_from_the_editor`。GUI **145 项**。

### 验证

- `cargo test --workspace --locked`：**986 passed / 0 failed**；clippy 0 警告、fmt 干净。
- GUI **145 项**、`openflex-core --lib` **241 项**；`scripts/acceptance.sh` **all golden checks passed**。
- `check-dead-params.sh` **0 known**；`check-doc-evidence.sh` 47 名可解析；fuzz 冒烟 PASS。
- Windows 交叉编译 `cargo check --target x86_64-pc-windows-msvc --workspace --all-targets` **0 错误**。
- 独立实测：两名执行器同刻 `transport_arrive`；`involved: 1` 与不写 `involved` 的报告 **sha256 相同**；
  只有一名执行器时 `completed 0` 且**整场仅 4 个事件**（永久等待且不忙等）。

## v1.44.0 — 2026-11-24

本轮的共同点是「**把写在文档里的东西变成真的**」：

- **共享任务序列（GTS）**：任务序列可以命名后给多个 dispatcher 复用，引用式与内联式的报告 **sha256 完全相同**；
- **用户自定义命令**：脚本里能直接调 `<name>(1.0)`。实现上没去啃"改造 ScriptEngine"的硬骨头——
  发现 rhai 同一 AST 内函数可见，只加一个 `definitions` 字段就完成了，风险远小于原估；
- **流体液位标记**：任意具名液位的上穿/下穿触发，`on_empty`/`on_full` 成了它的特例；
- 三者都配了 GUI 编辑入口，并各自补了 `script` 动作的预编译回归测试。

**又一次"只有真跑才会发现"的缺陷**：命令参数是 `f64`，而 `set_item_type` 只注册了 `i64` 重载，
于是最自然的写法 `set_item_type(t)` 在运行时报 `Function not found`。已补 `f64` 重载并加回归测试。
发布节奏保持**至少 3 小时一版**。

### 模型与引擎

- **全局任务序列（GTS）**：`Model.task_sequences: [{ name, steps, mode }]` + `DispatcherParams.task_sequence_ref`，
  把任务序列命名后给多个 dispatcher 复用（此前只能各写一份、改漏一处就漂移）。
  引用与内联 `task_sequence` **同时给出会被 validate 拒绝**（"use exactly one"，不静默取其一）；
  未知引用报错含 dispatcher 名、缺失名字与可用列表；重名/空名同样拒绝。
  **steps 不复制进引用者**（每次派发查表），所以"改 GTS 影响所有引用者"的语义是可预测的。
  实测：引用式与内联式的报告 **sha256 完全相同**（`completed 3.0`）。
  边界：绕过 validate（`parse_json` 后直接跑）时引擎优先用 ref，行为确定但不推荐；未来 GTS 数量大时可加 HashMap 索引（仍不复制 steps）。


### 验证

- `cargo test --workspace --locked`：**977 passed / 0 failed**；clippy 0 警告、fmt 干净。
- GUI **144 项**、`openflex-core --lib` **233 项**。
- `scripts/acceptance.sh`：**all golden checks passed**；`check-dead-params.sh` **0 known**；`check-doc-evidence.sh` 47 名可解析。
- 独立实测：GTS 引用 vs 内联报告 sha256 相同（`62f1d5fa…`）；命令 `mark()` 改道 3 件到另一出口；
  流体 Buffer 下落穿过两个标记各触发一次 `fluid_mark_fall` 且脚本钩子执行。

## v1.43.0 — 2026-11-24

> **发布事故（已修正，记录在此以免重演）**：v1.43.0 首次上传时多出一个
> `openflex-1.42.0-universal-apple-darwin.tar.gz`——因为 `dist/`（构建产物目录）**一直被纳入版本库**，
> 我上一版为补 macOS 资产在本地生成的 1.42.0 包被后续 `git add -A` 一并提交，发布流水线打包 `dist/*` 时
> 就把它带进了新版本。已删除该资产、重建 `SHA256SUMS.txt`（15 → 14 行），并把 `dist/` 加入 `.gitignore`
> 与 `git rm --cached`，从根上消除这类"旧产物搭车"。最终 15 资产 / 14 项哈希全 OK。


这一版把「**模型声明的每个参数都真的能到达引擎**」这条线走完了：`dead-params-allowlist` 从 5 条降到 **0 条**。
过程中接了两个真参数（`tick_length`、`repair_operator`），删了三个虚标（`receive_port`/`destination_port`/`product_id`）。

功能侧：**User Events**（按时刻执行的模型级动作，GUI 也有面板）、**`openflex eval`**（不跑模型就能试算表达式/跑沙箱脚本，
即 Script Console 的可用形态）。
两次「只有跑起来才会发现」的坑都记在下面：`block(Operator)` 会把对象从 `down` 变成 `blocked` 从而破坏故障状态机；
`collect_model_scripts` 是手写清单，新触发器漏收会让 `script` 动作在运行时才失败。
发布节奏保持**至少 3 小时一版**。

### 模型与引擎

- **`TickerParams.tick_length` 接线**：它现在真的是流体积分步长（全局只有一个 FluidStep 循环，
  **多 ticker 取最小的正 tick_length**——这是 OpenFlex 约定、不是 FlexSim 公开语义，已写进函数注释）。
  没有 ticker 时仍用既有 `fluid_step` / 0.1，**逐位不变**。步长变化会改变离散化数值，这是预期：
  测试断言"1.0 与 0.5 的液位不同但都收敛"，而不是假装相等。
- **`FailureParams.repair_operator` 接线**：故障修复**必须等 dispatcher 把操作员送到**才开始计时。
  修复过程中踩到一个只有跑起来才会暴露的坑：等待路径里调 `block(Operator)` 会把对象从 `down` 变成 `blocked`，
  **破坏故障状态机，对象永远不恢复**。改为不 block、只发 operator 请求，操作员到位时在 `grant_operator`
  启动修复计时，结束时用新增的 `release_repair_operator()` 只释放 repair 那个操作员（抽出共享的
  `release_operator_source`）。全程事件驱动、无忙等。实测：无操作员时修复间隔 2.0（1e-9）；
  有操作员时 **> 7.0**（travel 10 + mttr 2，证明"到位才开始计时"）；`items_completed == 1`（**必须恢复**）；
  等待计入 `down`。
  顺带修了夹具本身的问题（`process_time 100` + `mtbf 1` 让对象几乎全在故障循环里，`items_completed` 恒为 0，
  是"看起来死锁"的第二来源）。
- **删除 `FluidProcessorParams.receive_port` / `destination_port`**：这两个字段**声明了但引擎从不读取**，
  而流体在本项目里不是端口模型。按先例（`max_travelers`）删除而不是留虚标——旧模型含这两个字段会被
  `deny_unknown_fields` 明确拒绝。GUI 里对应的两个输入框一并移除。
  allowlist 因此从 **5 条降到 1 条**（只剩 `FluidMixerParams.product_id`，需要产品设计决定）。


- **User Events（按时刻执行的模型级动作）**：`Model.user_events: [{ time, name, actions }]`（空不序列化，
  旧模型 JSON 逐位不变）。事件进入事件日历、按 `(time, seq)` 确定性排序、**同一时刻按声明顺序**执行；
  早于 warmup 的事件**照常执行**（它是模型逻辑，不是统计）。
  `user_events` 里的 `action: "script"` **一开始就加进了 `collect_model_scripts` 的手写清单**——
  这个坑已经踩过两次，这次没再踩。实测：t=10 的两条事件共 2 次、`data["event"]` 顺序 `[0, 1]`、时刻都是 10.0。
  只支持 `script` / `send_message` / `set_motor`（其余动作在 validate 明确拒绝——user event 没有 item 上下文）。
  checkpoint 约定：**恢复时不重放已过去的事件**（写在注释里；硬断言需要带 trace 的 resume 入口，暂缺）。
- **删除最后一个死参数 `FluidMixerParams.product_id`**：整套产品/配方模型不存在，mixer 的 steps 只用 pull 体积与 delay，
  接上它不会有任何消费者。按先例删除。
  **至此 `docs/dead-params-allowlist.txt` 清空**：`check-dead-params.sh` → **0 known / 0 stale**，
  即模型声明的每个参数字段现在都真的能到达引擎。


- **用户自定义命令（User Commands）**：`Model.user_commands: [{ name, parameters, body }]`（空不序列化，旧 JSON 不变），
  脚本里可以直接 `<name>()` / `<name>(1.0)` 调用。实现上**没有**去改造 `ScriptEngine`/`register_api`——
  rhai 会「同一 AST 内定义的函数对同 AST 代码可见」，所以只需把命令包装成 `fn <name>(<param>) { <body> }`
  并**前置**到脚本源码再编译（`ScriptEngine::with_commands(model)`）。风险因此比预想小得多。
  **包装只有一个来源**（`openflex_model::user_command_source`），`validate` / `check-scripts` / 运行时预编译三条路径逐字相同，
  所以同一个坏 body 在三处会给出一致的判定（有测试钉住）。
  `validate` 现在也报命令体的编译错误（`user_commands.<i>.body`），并拒绝空名/重名/参数 >1
  （文案明确："only 0 or 1 numeric parameter is supported"）与**与 12 个内置脚本函数重名**。
  实测：命令 `mark()` 设 `set_item_type(1)` → 3 件全部改道到另一出口（不调用时全部走默认出口）。
- **修掉一个只有真跑才会发现的缺陷**：命令参数是 `f64`（rhai 数字都是浮点），而 `set_item_type` 只注册了 `i64` 重载，
  于是最自然的写法 `set_item_type(t)` 会报 `Function not found`。已为 `set_item_type` 与 `create_item`
  补 `f64` 重载（截断取整、双向 clamp），并加回归测试 `a_float_parameter_flows_into_integer_script_apis`。


- **流体液位标记（Fluid Marks）**：`FluidTankParams.marks: [{ name, level, on_rise, on_fall }]`（空不序列化，旧 JSON 不变），
  液位**上穿/下穿**标记时各触发一次；`level` 必须落在 `[0, capacity]`，重名/空名被拒。
  `on_empty`/`on_full` 是它的特例，两者**各自独立判定、互不吞没**。
  **约定写进文档**：穿越判定以 **tick 采样**为准（相邻两步比较），步内多次穿越只算一次——这是可判定且诚实的选择。
  穿越本身即使没有钩子也写 trace（`fluid_mark_rise` / `fluid_mark_fall`），建模期就能观察。
  实测：Buffer 从初始液位下落，依次穿过 high(5.0) 与 low(1.0) 两个标记，各产生一次 `fluid_mark_fall` 且脚本钩子执行。
  范围：只有 `fluid_tank`（其它流体对象没有液位容量语义）。

### 桌面 GUI

- **流体罐液位标记表单**：`level marks (n)` 列表可增删，行内编辑名字与阈值（阈值输入框**按容量夹取**，
  越界也写不进去）；带 `on_rise`/`on_fall` 钩子的标记会显示 `hooks` 提示，动作列表仍在 JSON 编辑区。
  测试 `fluid_marks_round_trip_and_stay_within_capacity`（含"标记高于容量被 model 拒绝"）。GUI **144 项**。

- **User events 面板**：与模型触发器同样的「JSON 缓冲 + Apply」方式编辑 `model.user_events`
  （文档首次打开时从模型灌入，之后保留用户输入；应用走 `commit_model_edit` 单一入口，因此撤销/校验一致）。
  测试 `user_events_round_trip_through_the_panel_buffer`。GUI **142 项**。


- **dispatcher 表单支持全局任务序列引用**：新增 `task sequence name` 输入框，填了名字就灰化下面的内联步骤编辑区，
  并提示"内联步骤在被引用时会被忽略"——因为二者互斥（validate 会拒绝），界面上先说清楚，而不是等用户保存后才报错。
  测试 `a_dispatcher_reference_and_inline_steps_are_mutually_exclusive`（含"同时给两者被拒"与"清空输入即清除引用"）。GUI **143 项**。

### 报告与 CLI

- **新增 `openflex eval`（脚本控制台 / 表达式试算）**：不跑模型就能试算表达式或运行沙箱脚本。
  - `openflex eval model.ofm --expr "1 + 2 * 3"` → `7`；`--time 5` 让 `time` 取任意时刻；
  - `openflex eval model.ofm --script 'log("hi"); set_label("x", 7.0);'` → 打印日志与标签；
  - **引擎状态（`Queue1.content`、`item.*`）需要真实运行**，因此明确报 `unknown variable` 而不是编造一个值；
  - 脚本错误附带与引擎一致的 `available:` 变量提示；同时传 `--expr` 与 `--script` 是用法错误。
  这正是「Script Console」缺口的可用形态——调条件、试 picklist、验证 `query()` 都不用改模型。


### 验证

- `cargo test --workspace --locked`：**955 passed / 0 failed**；`clippy --workspace --all-targets` 0 警告、`fmt` 干净。
- `scripts/acceptance.sh`：**all golden checks passed**。
- `scripts/check-dead-params.sh`：**0 known / 0 stale**（此前 7 条）。
- 独立实测：两份同刻 user event → 2 次、顺序 `[0,1]`、时刻均 10.0；`eval --expr "1+2*3"` → 7、
  `--time 5` 时 `time*2` → 10；流体 Buffer `on_full`/`on_empty` 各一次。

## v1.42.0 — 2026-11-24

这一版把「**声明了但没人读**」当成一类正式缺陷来清：新增 3 个触发器、接通 2 个死参数，并让 GUI 的 141 个测试**第一次进入 CI**。

- **触发器**：`on_load`/`on_unload`（装卸时点，落在既有的 `TransportDepart`/`OperatorLoaded` 上，**不新增事件**）、流体 `on_empty`/`on_full`、交通准入 `on_entry_request`；
- **死参数**：`DispatcherParams.max_tasks` 与 `ProcessorParams.changeover_time` 从"改了没反应"变成真行为，allowlist 从 7 条降到 5 条；
- **GUI**：`visual_tool` 此前只可编辑、画布上什么都不画，现在按自身参数绘制；
- **CI**：`openflex-gui` 的测试此前被三处 `--exclude` 排除，现在有独立作业并且已通过。

过程中修掉一个只有跑起来才会暴露的缺口：`collect_model_scripts` 用**手写清单**做脚本预编译，新增的触发器漏收会导致 `action: "script"` 在运行时失败而 `validate` 放行。三次新钩子各配了脚本回归测试。
发布节奏保持**至少 3 小时一版**。

### 模型与引擎

- **把两个"死参数"接上真行为**（它们此前有字段、有界面、引擎从不读，用户改了毫无变化）：
  - **`DispatcherParams.max_tasks`**：dispatcher 同时挂起任务数上限；达上限时新请求**留在队列里不丢**，
    任务完成（`finish_task`）或被抢占中止时释放名额并重新派发队首——复用既有事件链，**无忙等**。`0` = 不限（逐位不变）。
    计数是**每个 dispatcher 各自**的（与 FlexSim 的 per-dispatcher 语义一致）。
  - **`ProcessorParams.changeover_time`**：**相邻两件 `item_type` 不同**才插入换型（首次加工与同类型连续加工都不插，
    这与"每次 setup 都延迟"是两回事）；时长计入 Setup 阶段因此自动进 busy/utilization，trace 记 `changeover` 事件。
    实测 6+6 件交替、换型 3：busy **0.12 → 0.285**（差值 0.165 恰好 = 11 次换型 × 3 / 200），平均停留 2.0 → 4.75。
    只为常量负值报错；非 Processor 对象忽略该参数。
  两条已从 `docs/dead-params-allowlist.txt` 删除（`check-dead-params.sh` 现在 **5 known / 0 stale**，剩下的见 `docs/BACKLOG.md`）。
  **边界**：换型只接单 Processor 路径（`MultiProcessor` 的槽位语义未动）；换型是"延长 setup"而非独立阶段。

- **流体 `OnEmpty` / `OnFull` 触发器**：`Triggers.on_empty` / `on_full`（空不序列化，旧模型 JSON 与报告逐位不变）。
  去抖用**锁存 + 相邻采样对比**，没有引入静默阈值：离开边界即重新武装，所以"持续空/持续满"只触发一次。
  触发点在每个固定 fluid step 末尾（`fluid_advance` 之后）统一扫描，**不改任何液位数值**。
  **一个只有跑起来才会发现的事实**：tank 满时液位停在 `capacity − 一个流入步长`（例如 99.5 而不是 100），
  余量走 `overflow` 计数——所以 `level == capacity` 永远不会触发。可靠的"刚满"信号是**本步有新溢出**，
  这条已写进 `SimTrigger::OnFull` 与 `Triggers.on_full` 的文档注释（我们也考虑过改引擎让液位 clamp 到容量，
  那会动数值、与 golden 钉住冲突，因此不做）。
- **交通准入 `OnEntryRequest` 触发器**：在 `traffic_try_acquire` 的**容量判定之前**触发，
  `traffic_try_acquire` 因此改为 `Result<bool, SimError>`（脚本错误可传播，5 个调用点已更新）。
  **裁决协议写进 Rust 文档注释并明确标注是 OpenFlex 协议、不是 FlexSim 公开 API**：
  钩子结束后 `label("admit") == 0` → 本次拒绝、回退到既有等待重试（`TRAFFIC_RETRY_DELAY`）；
  标签缺失或非 0 → 走既有放行。实测 `admit=0` 时首次装载明显延后、但**最终仍完成 2 件**（不会卡死）。
  限制：交通请求没有真实 flow item，上下文是 `FlowItem::probe` 空件，只有 labels 有意义。
- **两个新触发器都进了 `collect_model_scripts` 的手写清单**——这是 task-188 踩过的坑（`on_load`/`on_unload`
  漏收导致 `action: "script"` 运行时失败），这次三个钩子各有一个「脚本钩子能跑」的回归测试。


- **`OnLoad` / `OnUnload` 触发器**：运输链路上「装货完成」与「卸货完成」两个时点各触发一次。
  OnLoad 落在既有的 `TransportDepart`（`now + dwell + load`）、OnUnload 落在 `OperatorLoaded`——
  **不需要新增事件，事件序列与统计都不变**（这比我原计划的「新增 `TransportLoaded`」更保守）。
  实测（`transport_depart`/`operator_loaded` 同刻）：t=0 pickup/depart/OnLoad、t=10 OperatorLoaded/OnUnload。
  **两条限制已写进 Rust 文档注释**：① OpenFlex 的执行器/操作员**不持有** FlowItem，钩子拿到的是
  requester（processor）的件，**不是"搬运件"**；② 该钩子不改变事件流。
- **Sink 回收策略**：`recycle: none|on_entry|by_batch` + `recycle_batch`（默认 100），`by_batch` 且 0 被明确拒绝。
  **如实说明**：当前引擎在 Sink 入口本就丢弃件对象，所以回收**不改变任何运行时内存行为**——
  本项是把既有行为显式化 + 批次 hook + 校验，不是省内存的优化。
- **修掉一个只有跑起来才会发现的缺口**：`collect_model_scripts` 用**手写的触发器清单**做预编译，
  新增的 `on_load`/`on_unload` 没被收进去，于是这两个钩子里写 `action: "script"` 会在运行时失败
  （`script source was not precompiled`），而 `validate` 却放行。已补上并用 `on_load_script_is_precompiled` 钉住。

### 桌面 GUI

- **`visual_tool` 现在真的会画出来**：此前它只可编辑（形状/大小/颜色/文字都能填），但**画布上什么都不画**——
  一个纯粹“空转”的对象。现在按自身参数绘制：`size` 折算像素并做上下限（写个离谱值也不会糊满画布）、
  `color` 解析 `#rrggbb`（**写错回退到种类色而不是消失**）、`text` 画在对象上方；
  `plane` 用低透明度。测试 `a_visual_tool_is_drawn_from_its_own_parameters`。GUI **141 项**。

### 示例

- **`examples/conveyor-sorting.ofm`**：一条带按 `item_type` 分拣——决策点用 `set_destination` 把 type 1 送到
  第 1 个出口。实测 300 秒：`created 92.3 / completed 89.7`，`LaneA 51.7 + LaneB 38.0 = 89.7`（守恒），
  **0.06 s 跑完**（Zeno 修复前这个形状根本跑不完，所以上一版没敢放进来）。

### 文档

- **纠正一处我自己写错的矩阵行**：生命周期钩子（Start/End of Warmup/Run/Scenario/Experiment）
  原记作 ⛔「6 个 picklist 全缺」，实际 `model_triggers` 有 9 个时点钩子且已有 4 个测试覆盖。
  已改为 ✅ 并注明「原记 ⛔ 是我漏查」。**缺口数量因此 -1**（⛔ 72 → 71）。


### 验证

- `cargo test --workspace --locked`：**940 passed / 0 failed**。
- GUI **141 项**（现已在 CI 独立作业中运行）、`openflex-core --lib` **209 项**。
- `clippy --workspace --all-targets` 0 警告、`fmt` 干净；`scripts/acceptance.sh` **all golden checks passed**。
- CI 13 个作业全绿（含新增的 `tests (openflex-gui, Linux)`、Windows 二进制冒烟）。
- 独立实测：`changeover_time` 3 时 busy **0.12 → 0.285**（差值 0.165 = 11 次换型 × 3 / 200）；
  流体 `on_full`/`on_empty` 各一次；`admit=0` 时首次装载延后但最终完成 2 件。

## v1.41.0 — 2026-11-24

**这一版最重要的是修掉一个发布后才发现的崩溃**：Windows 给主线程只有 1 MB 栈，
而 debug 构建的 CLI 在任何子命令执行前就溢出了——**v1.39.0 与 v1.40.0 的 Windows 二进制很可能一启动就崩**。
根因不是某段代码递归，而是「整份工作都跑在执行环境给的默认栈上」；现在 CLI 与 GUI 都改到**显式 32 MB 栈的线程**，
并在 CI 里先跑一次 Windows 二进制，让这类问题显示成一条明确的失败，而不是 111 个测试断言差异。

功能侧新增 `Action::SetDestination`（分拣的最小积木）；同时**修掉了一条从 v1.37.0 就存在的引擎缺陷**——
拥堵皮带上的决策点会让仿真跑不完（Zeno 事件风暴），最小复现从「>5000 万事件、60 s 超时」降到 **395 个事件 / 0.16 s**。
发布节奏保持**至少 3 小时一版**。

### 修复

- **Windows 上 CLI 完全无法启动（栈溢出）**：v1.40.0 发布后 CI 的 Windows 作业报
  **111 个 CLI 测试全部失败、0 通过**，每个子进程的 stderr 都是
  `thread 'main' has overflowed its stack`。Windows 给主线程 **1 MB** 栈，而 debug 构建的这个二进制
  在**任何子命令执行之前**就已超出；macOS 同样的代码在 1 MB 限制下仍能跑。
  修法是把真正的工作放到**显式指定 32 MB 栈的线程**上（`main` 只做 spawn 与收集退出码），
  从根上不再依赖平台默认栈大小。**这是发布后才发现的缺陷，说明 v1.40.0 与 v1.39.0 的 Windows 二进制
  很可能一启动就崩**——下一版会带上修复。
- **CI 增加 Windows 二进制冒烟步骤**：先 `--version` / `distributions` / `validate` 跑一次，
  让「二进制起不来」直接显示成一条明确的失败，而不是 111 个测试断言差异。
  这个坑之所以到发布后才发现，正是因为之前没有任何一步**运行**过 Windows 二进制。

- **决策点的 Zeno 事件风暴（v1.37 起存在）**：拥堵的非累积皮带上，决策点会让仿真耗尽 5000 万事件预算。
  根因在 `Belt::time_to_next_event_with_stops`：**后车**用静态 `limit`（前车位置 − spacing）算"追上前车"的时间，
  而前车同速前移时该 limit 同步移动，拥堵下这个时间恒为 ~10⁻⁵ s → 调度器不停排 `BeltTick`，每个 tick 只推进 ~10⁻⁵ 秒。
  决策点边界事件只是把 tick 更早引入拥堵区，所以**动作内容无关**（空动作同样触发）。
  修法：`index > 0` 的件**只在前车确实被挡住时**才把"追上前车"当作事件；前车在移动时由前车 tick 驱动。
  实测最小复现（`stop_time 150`）：**修复前跑不完（>5000 万事件、60 s 超时）→ 修复后 395 个事件、0.16 s**。
  回归测试 `belt_decision_point_congestion_does_not_zeno` 与对照 `congested_belt_without_decision_points_still_flows`；
  golden 验收仍全过（`advance` 的推进与最终位置不变，改变的是事件**序列**而非统计）。

### 模型与引擎

- **`Action::SetDestination`（分拣的最小积木）**：决策点已经能按条件改类型/打标签/发消息，但**不能把件真的分到另一条线**——
  它缺一个"送往哪个出口"的动作。新增 `Action::SetDestination { port }`：
  引擎在 `route()` 开头优先读 per-item 覆盖值（不改路由签名），目标忙则原地重试、**不静默回退默认策略**；
  端口越界/无对应连接是明确 `SimError`（含对象名、端口号、对象输出端口数）；交付成功后立即清空，**不泄漏到下游**。
  实测：`on_entry` 里设 `port:1` → 30 件全部走第 1 口（默认策略会全走第 0 口）。
  **顺序语义已写进动作的文档注释**：覆盖值由**下一次**路由消费，所以 `on_entry` 有效、`on_exit` 太晚
  ——在 `on_exit` 里设置会变成**下一个对象**的覆盖值。这是我自己实测踩到的坑，不想让用户再踩一次。

### 验证

- `cargo test --workspace --locked`：**916 passed / 0 failed**；clippy 0 警告、fmt 干净。
- Windows CI：**111 个 CLI 测试全部通过、0 次栈溢出**（修复前 0 通过 / 111 失败），并新增二进制冒烟步骤。
- `cargo check --target x86_64-pc-windows-msvc`（本地交叉编译）：CLI 与 GUI 均 0 错误。
- 实测 `set_destination`：`on_entry` 设 `port:1` → 30 件全部走第 1 口（默认策略会全走第 0 口）。

## v1.40.0 — 2026-11-23

一版**只含安全修复**的发布。v1.39.0 发布后 CI 的 `cargo-deny` 变红，而代码与依赖都没动——
是新公告进了 RustSec 库。修的是 `url` 在嵌套 `blob:` URL 上 `Url::origin()` 栈溢出，
经 `webbrowser → egui-winit → eframe` 传进 GUI。
发布节奏保持**至少 3 小时一版**。

### 安全

- **修复 `RUSTSEC-2026-0592`**（`url` 在嵌套 `blob:` URL 上 `Url::origin()` 栈溢出）：
  `url` 2.5.8 → **2.5.9**（传递依赖链 `webbrowser → egui-winit → eframe`）。
  这条是 v1.39.0 发布后 CI 的 `cargo-deny` 变红暴露的——**代码没变、依赖没变，是新公告进了 RustSec 库**，
  我本地装 `cargo-deny` 复现拿到编号后才动手，没有靠猜。
- **清掉两条失效豁免**：`deny.toml` 里 `RUSTSEC-2026-0249`(smartstring) 与 `RUSTSEC-2026-0192`(ttf-parser)
  已被 cargo-deny 判为 `advisory-not-detected`（公告不再匹配我们的依赖图）。留着会**掩盖将来同编号的新问题**，
  故删除；现在 `advisories ok, bans ok, licenses ok, sources ok`。


### 验证

- `cargo deny check`：**advisories ok, bans ok, licenses ok, sources ok**。
- `cargo test --workspace --locked`：**910 passed / 0 failed**；clippy 0 警告、fmt 干净。

## v1.39.0 — 2026-11-23

这一轮的重点不是加功能，而是**让功能清单变得可核查**，并按核查结果修掉问题：

- 对矩阵里 41 行 ✅ 逐行取证（实现符号 + 行为级测试），**8 行证据不足被降级**，随后又修好了 3 行；
- 其中最重要的一条：`NetworkNodeParams.max_travelers` **有字段、有界面、有文档，引擎却从不读它**——
  用户设了值、模型毫无变化。实现节点限额需要重写路由模型，所以**删掉字段**而不是留个虚标；
- 新增两项 CI 检查，专门防止这类问题复发：**文档引用的测试名必须真实存在**、
  **新参数若引擎不读就让构建失败**。

功能侧：新增 conveyor **Motor**（开/停控制）、补齐 GUI 里**六类对象的属性表单**（此前只能改 JSON）、
补上 FlexSim「First Model」对应的三品种示例。
发布节奏保持**至少 3 小时一版**。

### 模型与引擎

- **移除 `max_travelers`（声明但未接线的参数）**：取证审计发现 `NetworkNodeParams.max_travelers`
  **有字段、有 GUI 控件、有 `objects describe` 文案，但 `openflex-core` 零引用**——用户设了值、界面接受、
  文档说支持，模型却毫无变化。实现节点级限额需要重写 network 路由的占用模型（逐节点转移事件 + 占用队列），
  **不做半成品**，因此把字段、GUI 控件与 CLI 文案一并删除。旧模型含该字段会被明确拒绝：
  `unknown field max_travelers, expected edges or triggers`。
  记录在此而不是让它悄悄消失：这是一个**曾经对外承诺过却从未生效**的参数。
- **模型树查找变成可测的纯函数**：审计指出「按名称/类型查找」的过滤逻辑内联在 UI 闭包里、
  无法单测（属**可测性缺陷**而非单纯缺测试）。抽出 `tree_row_matches(needle, name, kind)` 并补测试
  （空串全显示、名称与 kind 双向匹配、大小写不敏感、不误匹配别的 kind），矩阵该行从 🟡 升为 ✅。
- **装卸时长精确断言**：新增 `explicit_load_and_unload_durations_are_exact` —— `load_time=5` 时载货腿
  在到达后 5.0 才出发、`unload_time=3` 时 `operator_loaded` 在 requester 到达后 3.0 触发（1e-9 内）。
  此前只有"两次运行一致"，**没有断言时长真的生效**。

### 工具与 CI

- **死参数棘轮 `scripts/check-dead-params.sh`**：扫描模型里声明的每个 `*Params` 字段，
  找出**引擎里从未出现**的那些（含通过访问器间接消费的情况，避免误报），
  与 `docs/dead-params-allowlist.txt` 比对——**新增一个引擎不读的字段会让 CI 失败**。
  这条检查直接回应 `max_travelers` 那类问题：用户设了值、界面接受、模型毫无变化。
  目前冻结 7 个已知项（`max_tasks` / `repair_operator` / `product_id` / 流体收发端口 /
  `changeover_time` / `tick_length`），每项都待「接线或删除」的设计决定，而不是放着不管。
  已验证：临时加一个探针字段会让它失败（exit 1），移除后恢复。
- **`scripts/check-doc-evidence.sh` 接入 CI**：文档里引用的每个测试名必须能在源码里 `fn` 到，
  防止「取证台账」本身变成新的虚标来源（它当场抓到 2 个把 JSON 策略字符串当测试名的引用）。

### 示例

- **`examples/three-varieties.ofm`**：对应 FlexSim「First Model」——三种产品各一台专机、共用一台测试站、
  不合格返工回入口。`by_item_type` 分流到专机、`by_condition` 按测试站输入计数决定返工。
  实测 480 分钟 5 复现：测试站 util **0.763** 成为瓶颈，入口缓冲因 FIFO 队头被占机阻塞而堆积——
  这正好是「瓶颈在哪、测试站前的缓冲是否重要」这个教学问题的答案。

- **conveyor `Motor` 对象**：新增 `ObjectKind::Motor` + `MotorParams { running, triggers }`（`running` 缺省 true、
  空时不序列化，旧模型 JSON 与报告逐位不变）。Motor 通过 **A 连接**控制它所连的 conveyor：
  `running: false` 时皮带**停止推进**、已存在件位置冻结、`can_accept` 为假（上游不再往皮带上堆）、
  且**不产生新事件**（trace 事件数有界，不会零间隔自旋）；置起后从原位置继续、不重复计数。
  新增 `Action::SetMotor { target, running }` 供触发器/消息切换；`validate` 拒绝 Motor 连到非 conveyor，
  以及 Motor 上的 S 连接（措辞含 Motor 名）。实测：同一模型 stop → `completed 0`、running → `completed 5`。

### 桌面 GUI

- **补齐六类对象的属性表单**：`multiprocessor`（槽位/设置与加工时间/操作员）、
  `combiner`（pack/join/batch + 组件数 + 操作员）、`separator`（unpack/split + 数量 + 语义提示）、
  `merge`（等待全部输入）、`split`（按 item type）、`escalator`（复用 conveyor 表单）此前在 GUI 里
  只显示「no form yet — edit the JSON below」，现在都有真实编辑器。
- **Motor 也接入 GUI**：选择器、conveyor 家族配色、`running` 复选框。
- 删掉 `_ => "no form yet"` 兜底：**所有对象类型都有表单了**，以后新增 kind 会直接编译失败，
  用编译错误提醒补表单，而不是在界面上留个空壳。

### 验证

- `cargo test --workspace --locked`：**910 passed / 0 failed**。
- GUI **140 项**、`openflex-core --lib` **180 项**、`openflex-model` + report + CLI 其余。
- `clippy --workspace --all-targets` 0 警告、`fmt` 干净；**64 个示例模型全部 validate 通过**。
- `scripts/acceptance.sh`（golden 数值验收）：**all golden checks passed**。
- `scripts/check-doc-evidence.sh`：47 个被引用的测试名全部解析到真实 `fn`。
- `scripts/check-dead-params.sh`：7 个已知死参数被冻结；实测新增探针字段会让它退出 1。

## v1.38.0 — 2026-11-22

本轮把「模型自己声明的实验」打通到 GUI（此前执行逻辑埋在 CLI 里，桌面端完全看不到模型声明的场景），把 **`explicit` 路线状态纳入检查点**（中途存盘续跑与一次跑完逐位相同），并修掉 README 里一处**明显失实**的功能表（它仍写着「3D 视图 / GUI ⛔ 暂缺」，而三平台桌面安装包已经发布）。
发布节奏保持**至少 3 小时一版**。

### 桌面 GUI

- **已声明实验面板**：左侧新增 `Declared experiments (n)` —— 列出模型 JSON 里 `experiments`
  的每个实验（场景数 × 复现数），可 `run` 单个或 `Run all`；跑在后台线程（不冻结界面），
  结果表按 `实验/场景` 给出 `completed`、吞吐与成败，失败行用琥珀色并悬停显示原因。
  此前 GUI 只能跑自己的参数扫描，**看不到模型里声明的场景**（`grep experiments` 在 GUI 里是 0 命中）。
  依赖新增 GUI → `openflex-report`（显示报告本就该由它渲染）。
  结果表把每个场景与**首个跑成功的场景**对比，差值超过 0.05 时显示 `12.3 (+2.3)` 并按好坏变色
  ——这是 FlexSim Experimenter 摘要列的读法。GUI 测试 **139 项**。

### 模型与引擎

- **`explicit` 链状态进入检查点**：`Checkpoint.explicit_routes: BTreeMap<String, ExplicitState>`（键是**对象名**，非下标），
  保存剩余腿表、`requester`、`loaded` / `operator_loaded` / `waiting_release` / `release_seen` / `leg_in_progress`、
  当前腿 kind、当前 travel 目标与**剩余时间**；`restore` 按对象名重建并按 `checkpoint.now + remaining` 恢复当前腿
  （绝对 deadline 用 `#[serde(skip)]`，由事件日历保留触发时刻）。
  核心断言达成：t=5 中途存盘再 resume 到 t=20，与不中断跑完的 `ReplicationResult` **逐位相同**（含执行器状态细分）；
  在 `waiting_for_release` 时刻存盘也能正确续等。旧 checkpoint 缺该字段时按「链从头开始」，已在 `Checkpoint`
  文档注明为向后兼容策略，`format_version` 不变。

- **实验执行提取为可复用 API**：模型声明的 `experiments` 执行逻辑此前埋在 CLI 的 `main.rs` 里，GUI 完全用不上
  （GUI 只能跑自己的参数扫描，看不到模型里的场景）。现在移到 `openflex-report`：
  `run_declared_experiments(model, parallel)` / `run_declared_experiments_with_options` 返回
  `ScenarioOutcome { experiment, scenario, overrides, report: Option<SimReport>, error, error_kind, trace_files, logs }`
  ——**每条带完整 `SimReport`**，另有 `experiment_reports()` 分组成可直接渲染的 `ExperimentReport`。
  失败不中断：某场景失败不影响其它场景，`error_kind` 区分 `usage` / `runtime`。
  CLI 改为调用它，**三格式输出逐字节不变**（用改动前快照 + 逐字符 `assert_eq` 锁住，13 个 stdout 输出与 9 个归档文件 sha256 相同）。
  一处行为差异如实记录：`experiment --trace` 的 stderr 进度提示不再带事件数。


### 验证

- `cargo test --workspace --locked`：**898 passed / 0 failed**。
- GUI **139 项**、`openflex-core --lib` **169 项**、`openflex-report` + CLI 合计 187+。
- `clippy` 0 警告、`fmt` 干净；63 个示例模型全部 validate 通过。
- 独立实测：`openflex experiment examples/objects/scenario-stop-warmup.ofm` 两场景结果与提取前一致；
  `optimize --pareto` 多目标输出正常；检查点续跑与直跑 `ReplicationResult` 逐位相同。

## v1.37.0 — 2026-11-22

本轮把 FlexSim **Conveyor 模块**（主帮助之外的 35 页）当作正式规格来补：**光电门四状态**（并让边界穿越成为皮带事件，触发时刻精确到 `10∓0.5`）、**决策点**（按件求值条件后执行动作）、GUI 侧的光电门/决策点编辑器与**显式路线可视化**，并新增一个同时演示 slug / 加权合流 / 光电门 / 决策点的示例模型。
同时修掉两处「提示会骗人」的问题：脚本 `available:` 曾列出脚本根本解析不了的表达式路径；脚本诊断曾因字段名 `source` 被 thiserror 当作因果链而打印两遍。
发布节奏保持**至少 3 小时一版**。

### 桌面 GUI

- **决策点编辑器**：conveyor 表单新增 `decision points (n)` 列表，逐行编辑 `pos` / `±range` /
  `once per item` 勾选框与**条件表达式**（空 = 恒真，不写进 JSON）；动作列表仍在 JSON 编辑区。
  条件回显用 `Expr::Display` 的**规范形式**（二元表达式带括号），并已确认该渲染**幂等**——
  `((x))` 归一为 `(x)`，反复编辑不会叠加括号。GUI 测试 **137 项**。
- **光电门几何编辑器**：conveyor 表单此前**完全没有**光电门编辑入口，只能改 JSON。现在有
  `photo eyes (n)` 列表，逐行编辑 `pos` / `range`（勾选框控制「是否设置」，未设置时**不写进 JSON**）/ `name`，
  可 `+ photo eye` 添加、`x` 删除；`join inputs` 也改为可选的数字框。触发动作列表仍留在 JSON 编辑区。
  GUI 测试 **136 项**（新增 `photo_eye_geometry_round_trips_through_json`）。

- **画布绘制 `explicit` 路线**：dispatcher 声明了显式路线时，画布上从它就画出一条**琥珀色虚线**
  （每段末端一个小圆点），与蓝色实线的流连线在视觉上区分开——一眼能看出「计划路线」与「物料流向」是两件事。
  `home`/`pickup` 解析回 dispatcher 自身位置；`{object}` 与 `{position}` 按实际坐标；
  **`requester` 依赖运行时**，画到那里就停，不臆造位置；`append` 模式的模型不画任何线。
  GUI 测试 **135 项**（新增 `explicit_routes_become_canvas_waypoints`）。

### 模型与引擎

- **新示例 `examples/conveyor-flow-control.ofm`**：一条带同时演示
  slug 组、`merge_mode: weighted [3,1]` 的两路合流、光电门 `on_blocked/on_cleared` 脚本、
  决策点（`item.type == 1` 时打 `express` 标签、`once_per_item`）。实测 300 秒 3 复现：
  主带 `max WIP 25`、完成 66 件、35 次光电门跃迁脚本日志。
- **conveyor 决策点**：`ConveyorParams.decision_points: [{ position, range, condition, actions, once_per_item, name }]`。
  件经过感应区时对**该件**求值 `condition`（复用既有表达式语言，`item.*` 指向它），为真按序执行 `actions`；
  `once_per_item` 离开感应区后重置。**复用光电门那套「边界穿越是皮带事件」机制，没有新时间模型**。
  实测决策点在 `position 5.0 − range 0.5 = 4.500` 精确触发。条件求值失败是明确 `SimError`
  （含决策点名、件 id、模型时间），不静默当 false。明确不做：决策点的类型库（与 ConveyorSystemTool = 有意差异一致）、
  与 merge controller 的联动（用自身计数判断）。
- **脚本诊断的 `available:` 提示改用脚本变量表**：task-177 的提示原样用了**表达式语言**的 `BUILTIN_VARIABLES`，
  于是 rhai 脚本里报「变量找不到」时，提示会列出 `item.type` 这类**脚本根本解析不了**的路径。
  现在改用新增的 `SCRIPT_VARIABLES`（`time` / `item_type` / `item_id` / `label(name)` / `set_label(...)` …），
  并加回归测试断言提示里**不出现** `item.type`。

- **conveyor 光电门四状态**：`PhotoEye` 新增 `range`（感应半宽，缺省 0.5，**注释已说明这是本项目自选粒度而非 FlexSim 公开值**）
  与 `triggers { on_blocked, on_cleared }`（空时不序列化，旧 JSON 与行为逐位不变）。
  引擎为每个光电门维护 `Cleared / Blocking / Blocked / Clearing` 四状态，
  **只在进入 Blocked 与进入 Cleared 两个跃迁各触发一次**；旧的「经过即触发」`on_photo_eye` 保持不动（附加而非替换）。
  依据是 Conveyor 模块自带帮助（`modules/Conveyor/help/`）的两页，而非主帮助目录。
  实测一件经过：`on_blocked`、`on_cleared` 各一次且顺序正确。
  光电门的**边界穿越本身是皮带的一个事件**：`schedule_belt_tick` 把 `[pos−range, pos+range]`
  的穿越时刻与出口/站台/哨兵/ slug 事件一起取最小，所以 `blocked`/`cleared` 在元件真正进出感应区时就触发。
  实测单件（长 20 / speed 1 / 眼位 10 / range 0.5）：**`blocked = 9.50`、`cleared = 10.50`**，与 10∓0.5 完全一致
  （修复前 `cleared` 要到 t=20 的出口事件才发，晚 9.5 个单位——这是我实测发现后专门补的一刀）。
  防自旋：件停在站台上不产生零间隔事件；`range` 覆盖整条带时不加边界候选。


### 验证

- `cargo test --workspace --locked`：**885 passed / 0 failed**。
- GUI **137 项**、`openflex-core --lib` **165 项**、`openflex-model` **45 项**。
- `clippy` 0 警告、`fmt` 干净；示例模型全部 validate 通过。
- 独立实测：光电门 `blocked = 9.50 / cleared = 10.50`（修复前 cleared 要到 t=20）；
  决策点在 `pos 5.0 − range 0.5 = 4.500` 触发；`available:` 现列 `time, item_type, item_id, set_label(...)`
  而**不再**出现 `item.type`。

## v1.36.0 — 2026-11-22

本轮把 conveyor 补齐两片（**slug 组**与**合流控制器**），把**脚本错误**变成能定位的诊断（并修掉一处会重复打印的缺陷），GUI 补上 **state/staytime 跟踪汇总**面板。
期间有一次我自己的误判被测试纠正：想让 GUI 提示「脚本编译失败」，写测试时才发现模型校验已经覆盖，改动已整条撤回，没有留下重复逻辑。
发布节奏保持**至少 3 小时一版**。

### 桌面 GUI

- **报告展示跟踪汇总**：`state` / `staytime` 两类跟踪变量不产生时间序列，报告里在
  `tracked_summaries` 中。GUI 新增 `Tracked summaries (n)` 面板——`state` 显示各状态占比
  （零占比省略、并注明「覆盖整个模型而非单个对象」），`staytime` 显示
  `n / min / p50 / p90 / max / mean`。此前这两类数据只能去 JSON 里看。GUI 测试 **134 项**。

### 模型与引擎

- **conveyor 合流控制器**：`ConveyorParams` 新增 `merge_mode`
  （`round_robin` 缺省且**不序列化**，旧 JSON 与数值逐位不变 / `weighted` / `by_item_type`）与
  `merge_weight`。选择逻辑只改 `join_preferred_port`，与皮带推进解耦：
  - `weighted` 按「已服务数/权重」最小者取，权重和无需归一；
  - `by_item_type` 按上游首件 `item_type` 升序，同类型回落 cursor 轮询；
  - 主带入口被占时**不消费配额**（不更新 cursor/count），入口放行后再按规则竞争。
  校验三条精准文案：缺权重、长度与输入数不符、非 `weighted` 模式给权重。
  独立实测（两条上游各 2.0 间隔、按 item_type 分流到两个汇）：
  `round_robin` 上带比 **1.04 : 1**、`weighted [3,1]` **3.08 : 1**（偏差 2.7%，在 ±5% 内）。
  **已知语义**：`by_item_type` 是**严格优先**——高优先输入持续饱和时低优先流会被饿死
  （实测 `49 : 0`），这是严格优先的直接后果而非缺陷，已在此注明。

- **脚本错误诊断 + `check-scripts`**：脚本失败时错误文案含
  **脚本原文（>80 字符中间省略）+ 位置 + 模型时间 + rhai 原始信息**；
  引用未定义变量时追加 `available: …`，清单直接由 `BUILTIN_VARIABLES` 生成（不另列一套）。
  新命令 `openflex check-scripts <model.json>` **不跑仿真**，只编译所有 `Action::Script`，
  逐条报 `FAIL <location>: <error>` + 原文，末尾给「N checked / P passed / F failed」，退出码 0/2。
  实测：语法错脚本 `FAIL objects.S.triggers.on_create[0]: script compile error: Unexpected ';'`；
  运行时未定义变量则 `check-scripts` 判通过、`run` 给出完整诊断。
  **顺带修掉一处会重复打印的缺陷**：`SimError::Script` 的字段原名 `source`，
  thiserror 会自动把它当作 error 的 cause，而 `Display` 已内联同一段文字，于是 anyhow 的因果链
  把整段诊断打印两遍。字段改名 `detail`（类型层修复，对所有消费者生效），并加回归测试
  `script_diagnostic_is_not_duplicated_by_the_cause_chain` 锁住。

- **conveyor slug 累积**：`ConveyorParams` 新增 `slug: bool`（缺省 false，**不序列化**、旧模型与数值逐位不变）
  与 `slug_min_gap`（缺省 = `item_spacing`）。`slug: true` 时皮带相邻两件间隙小于该值即视为一组，
  整组以**组首**位移整体推进（组内间隙恒定、不压缩不拉开），组首到出口或被限位时**整组同时停**，
  出口件移除后整组同时恢复；不同 slug 组之间仍按普通 `item_spacing`。
  实现是一条独立的 `advance_slug_belt`，`slug` 缺省时仍走原 `Belt::advance`、一行未改。
  实测（单元，三件 `[8.0, 6.5, 5.0]`）：推进 2s → `[10.0, 8.5, 7.0]` 到位停住；移除组首后推进 1s →
  `[9.5, 8.0]`，间隙仍 1.5；间隙 4 > 2 的两件与 `slug:false` 的推进逐位一致。
  **说明**：slug 改变的是**运动耦合**而非容量，所以我用 CLI 端到端对比 `maxWIP`（两者都是 11）
  看不出差异——这符合设计，其行为由单元测试的位置序列锁定。
  缺口（未近似实现）：合流控制器、光电门阵列高级语义、Mass Flow、slug 可视化/报告列。

- **`explicit` 第二刀：load/unload 与 release 同步**：explicit 路线现支持 `travel/load/unload/utilize/break`
  五类腿，`pickup` / `requester` 目标**真正实现**（不再报未实现）。
  `unload` 完成瞬间触发**恰一次** `OperatorLoaded`；其后若下一腿不是 `utilize`/`break`，
  执行器**停在原地（Busy + Utilize）等 `OperatorReleased`**，且不再走标准 `TransportHome` 硬编码回程
  ——标准链的 handler 一行未改，新分支只在 explicit 链状态存在时生效。
  实测序列 `pickup → 去 D → load(2) → requester → unload(3) → home`：
  `operator_loaded` 恰一次（t=15.0）、`operator_released` 与回程 depart **同为 t=22.0**（证明等到释放才走）、
  执行器细分 `travel_empty 10 / travel_loaded 10 / loading 2 / unloading 3 / utilize 7`，
  其中 7 秒正是等待释放的时间。抢占在 explicit 链内禁用（MVP，注释已说明）。


- **`explicit` 路线编辑器**：任务序列编辑区新增 `append` / `explicit` 模式切换；
  explicit 下每个 travel 腿显示目标选择器（`home` / `object` / `position` / 无），
  `object` 给名字输入、`position` 给 x/y 两个 DragValue，可单独删除某腿。
  此前 `target` 字段只靠编译兜底被保留、在界面里完全不可编辑。GUI 测试 **133 项**。


### 验证

- `cargo test --workspace --locked`：**862 passed / 0 failed**。
- GUI **134 项**、`openflex-core --lib` **145 项**、`openflex-model` **45 项**。
- `clippy` 0 警告、`fmt` 干净；**62 个示例模型全部 validate 通过**。
- 独立实测：slug 组内间隙恒定（`[10.0, 8.5, 7.0]` 整组停）；合流 `round_robin` 1.04:1、
  `weighted [3,1]` **3.08:1**、`by_item_type` 严格优先（49:0，已注明可饿死低优先流）；
  脚本诊断单次打印（`SimError::Script` 字段改名 `detail` 消除因果链重复）；
  `check-scripts` 语法错退出 2、合法退出 0。

## v1.35.0 — 2026-11-21

这一轮有三条主线：**让模型能自己声明「量」与「路线」**（只读 SQL、Tracked Variable 四种类型、`explicit` 固定路线、`utilize`/`break` 任务腿），**让「这一件走了哪条路径」可回答**（trace 带 flow item id + 真实 `enter`/`exit`、`openflex trace` 的单件与跨件视图），以及**终端里能看的图表**（`openflex chart`）。
多轮里两次因为「无法正确实现」而主动缩小范围并把设计写进缺口清单，没有留下近似实现。
发布节奏保持**至少 3 小时一版**。

### 报告与 CLI

- **终端图表 `openflex chart`**：`--of utilization | measures | series:<名字>`，
  `--kind bar|line`、`--width N`（钳制 10..120）、`--format table|markdown`、`-o` 写文件。
  - 利用率：只为「有容量含义」的对象画条（复用与 GUI 同一判定，避免两个界面对同一模型给出不同结论），
    实测 `mm1` 的 `Server` 为 **79.8%**；
  - 指标：每个指标一根条，注记 `n` 与 95% CI（`n<2` 显示 `—`，不出 `±0.000`）；
  - 跟踪时序：8 级块字符 sparkline（按宽度分桶重采样，行尾给 min..max 与均值）。
  - 边界文案齐全：`(no data)`、全零、单点、常量；条长对负值用 `|value|` 并在图上注明，不静默误导；
    非零但不足一格的值强制 1 格（避免「非零看起来像 0」）；中文标签用东亚宽度对齐，
    未知序列退出码 2 并列出可用名。

- **Item Trace（单件路径追踪）**：新 `openflex trace <file.jsonl>` 子命令——
  无参数出摘要（事件数、带 item 的事件数、可追踪件数、时间范围），
  `--items` 列每件 id 与步数，`--item <id>` 打完整路径表（Time / Object / Event）并给摘要
  （步数、进入过的对象数、跨度），支持 `--format json|csv|markdown` 与 `--output`。
  提取器 `item_traces()` 只读 `data["item"]`、按 `(time, seq)` 排序，**不做任何 seq/时间推断**；
  缺 `item` 的关系事件（`operator_loaded` 等）自动跳过。未知 id 退出码 2 并列出可追踪 id。
  实测同一模型：`item 1` 13 步 / 4 个对象 / 跨度 42，`item 2` 7 步。
  顺带修掉一个**既有**缺陷：`report`/`trace` 读文件失败时 I/O 原因经 anyhow 因果链打印两遍。
- **跨件路径聚合 `--paths`**：把每件的对象序列**只压缩相邻重复**后分组（首尾同一对象保留，能看出真实回访），
  输出件数 / 路径 / 样例件 / 时间跨度，并给覆盖率（`grouped/traced`，恒等有测试）。
  实测 `transporter-network.ofm`：`3 种路径，6/6 件已归档，覆盖率 100%`。
  关键判断：**`flow` 事件被排除在路径之外**——引擎把 `flow` 记在推送方对象上、此时件已离开，
  计入会把 `S→K` 变成 `S→K→S`（假回访）；`transport_home` 仍保留（是运输历史，非假回访，已记为已知限制）。

### 模型与引擎

- **`explicit` 固定路线（第一刀）**：`task_sequence_mode: "explicit"` + 一串 `travel` 目标
  （`"home"` / `{ "object": 名字 }` / `{ "position": [x, y] }`），执行器按表逐段行驶，
  段间复用路网距离与 `traffic_control` 准入（阻塞期间 blocked、**不计 travel**）。
  `append`（缺省）不序列化该字段、行为**逐位不变**；`finish_task` 的收尾逻辑未改，
  路线要回 home 就显式写 `target: "home"`。
  依赖任务上下文的 `pickup` / `requester` **可以解析但 validate 明确报未实现**，不静默当 home。
  实测几何精确：`home`(0,0) 瞬时 → `{object:"P"}`(10,0) 耗时 10 → `{position:[15,2]}` 耗时
  **5.385 = √(5²+2²)**；零距离路线在 dispatch 报 `SimError::Config`，避免同刻无限重派。

- **trace 事件带上 flow item id**：`TraceEvent.data["item"]` 现写 `FlowItem::id`（与 `items_created` 同源），
  并**真正新增 `enter` / `exit` 两个边界事件**（进入/离开对象各一次，成对且有序）。
  `arrive` / `flow` / `process_done` / `setup_done` / `transport_pickup|depart|arrive|home` 均带该键；
  **无法唯一确定是哪件时不写**（宁可缺键也不写错值）。`data["token"]` 仍是 process-flow token /
  processor 相位令牌，文档已明确两者语义不同。
  实测一件物件的完整路径：
  `arrive(S) → enter(S) → exit(S) → enter(P) → transport_pickup/depart/arrive(T) → flow(P)
   → process_done(P) → exit(P) → enter(K) → exit(K) → transport_home(T)`，
  两件并发时 id `{1,2}` 不串。
  这为 Item Trace（「这一件走了哪条路径」）提供了数据基础。

- **任务腿 `utilize` / `break`**：`TaskStep` 新增 `Extra { kind, duration, condition }`，
  在既有 append 机制里作为额外腿执行（**不引入 `task_sequence_mode`**，旧 travel/load/unload 的
  数值与顺序逐位不变）：`utilize` 计入执行器 `utilize` 细分并保持 Busy、不可派新活；
  `break` 计入 `idle` 但任务槽仍占用（计划内休整既不是可派活的空闲，也不是资源阻塞，
  用 blocked 会污染等待统计）。GUI 的组合框对这类腿**只读展示**（附带 `x` 可删除），
  避免点一下就把 duration 静默丢掉；JSON 里可正常编辑。
  实测同一模型三种情形：append `utilize 3.0 / idle 0.0 / completed 3`、
  `utilize ×3` 得 `utilize 12.0 / completed 2`、`break ×2` 得 `idle 14.0 / completed 2`——
  两种腿都真实占用了执行器的时间。
  已知缺口：**`explicit` 整链模式未实现**（需要链状态机重写 + travel 目标语义 + 与
  `OperatorReleased` 的同步），设计草案已写入 `docs/FEATURE_PARITY_75.md` 供下一轮照做。

- **Tracked Variable 类型体系**：`tracked_variables` 增加 `kind: level | state | staytime | kinetic`
  （**缺省 `level` 且序列化时不写该键**，旧模型 JSON 与行为逐位不变）：
  - `level` —— 按间隔采样瞬时值（原行为）；
  - `kinetic` —— 相邻采样的变化率 `(vₙ−vₙ₋₁)/(tₙ−tₙ₋₁)`，首采样为 0；
  - `state` —— 复现结束时给出全模型各状态（idle/busy/blocked/down/off_shift）累计占比；
  - `staytime` —— 复现结束时给出停留时间分布（n / min / p50 / p90 / max / mean，分位数线性插值）。
  结果分两处：`tracked_series`（level/kinetic，语义不变）与新增的 `tracked_summaries`
  （state/staytime，空时不序列化）。实测同一模型四种并存：`Clock` 1..10、`Rate` 首点 0 后恒 3、
  `States` 合计 1、`Stay` 的 n 与 `items_completed` 一致且 p50 = 加工时长 5.0。

- **只读 SQL 子集 `query()`**：`query("SELECT <cols|*> FROM <table> [WHERE <expr>] [ORDER BY <col> [ASC|DESC]] [LIMIT n]")`
  查询模型的全局表；`WHERE` 复用既有表达式语言（`$全局变量`、`picklist(...)` 都能用），
  返回文本（列 `|`、行 `\n`，无匹配返回空串而非错误）。解析器独立在 `crates/openflex-model/src/sql.rs`，
  `expr.rs` 只做接线；`validate` 与 `run` 走**同一条解析路径**，因此未知表／未知列／语法错误在校验期
  就以**逐字相同**的文案报出（实测三种坏查询 `exit=2`）。
  引擎侧经 `EvalContext::global_table` / `supports_global_tables` 接线后 `run` 可用；
  未接线的上下文会报 `query() needs an evaluation context that can read model global tables`，
  而不是误导性的「未知表」。实测 `SELECT name FROM Shipments ORDER BY prio LIMIT 2` 得 `b\nc`。


### 验证

- `cargo test --workspace --locked`：**839 passed / 0 failed**。
- GUI **132 项**、`openflex-model` **184 项**、`openflex-core --lib` **125 项**、report+CLI **187 项**。
- `clippy` 0 警告、`fmt` 干净；示例模型全部 validate 通过。
- 独立实测：`explicit` 路线 `{position:[15,2]}` 耗时 **5.385 = √(5²+2²)**；
  `query("SELECT name FROM Shipments ORDER BY prio LIMIT 2")` 得 `b\nc`；
  Tracked 四型并存（`kinetic` 首点 0 后恒 3、`staytime` 的 p50 = 加工时长）；
  Item Trace 单件 13 步 / 4 对象 / 跨度 42；`--paths` 三组覆盖率 100%；
  `chart` 利用率条与统计表口径一致（`Server` 79.8%）。

## v1.34.0 — 2026-11-21

继续消化 `docs/FEATURE_PARITY_75.md` 的缺口清单：**自定义性能指标**与**跟踪变量**两类「模型自己声明的量」进入报告与解析链路，**实验生命周期 6 个钩子**、**执行器状态细分**让报告回答得了「这辆车空驶多少」这类问题，并修掉一处**状态限定故障时钟**的真实缺陷。GUI 同步补上指标编辑器、跟踪时序汇总与迷你折线、模型触发器面板。
发布节奏保持**至少 3 小时一版**。

### 模型与引擎（FlexSim 7.5 剩余缺口）

- **schema 与 `Model` 字段自动对齐**：新增 `tracked_variables` / `tracked_interval` 的 schema 定义；
  并加了一条**自动解析 `Model` 源码**的双向断言——`Model` 的每个字段都必须在 `$defs.model.properties` 里，
  反向也不允许 schema 出现不存在的字段。以后谁加字段忘写 schema，`cargo test -p openflex-model` 当场红。
  已实测该断言能抓到临时改名的 `tracked_interval`。内联校验器顺带补了 `minimum`/`exclusiveMinimum`/`maximum`。
- **Tracked Variables（跟踪变量）**：模型新增 `tracked_variables: [{ name, expression }]` 与
  `tracked_interval`，运行期按固定间隔把表达式采样成时间序列，写进 `SimReport.tracked_series`
  （`name → [(time, value)]`，空时不序列化）；采样点为 `warmup + k·interval … stop_time`，
  `SimulationHandle::tracked_series()` 供 GUI 取用。实测 `Clock = time` 得
  `(1,1)…(5,5)`、`TwiceClock = time * 2` 得 `(1,2)…(5,10)`，两条互不污染；
  未定义符号在运行期是明确 `SimError`。新增 5 项引擎测试。
- **自定义性能指标（Performance Measure）**：模型新增 `performance_measures: [{ name, expression, unit }]`，
  复现结束时用表达式求值（可读 `time` / `created` / `completed`、全局变量、对象字段），
  `SimReport` 聚合成 `name → Aggregate`（多复现给 mean/std/CI/min/max/n），
  `openflex run --format json` 自动输出、`openflex report --format table` 追加 `Performance measures` 一节。
  未定义变量是明确错误：`expression error at performance_measures.X: unknown global variable 'Missing'`。
  实测 `$Batch * 2` 精确给出 20.0（CI 0）、`completed / time` 给出 0.27、`completed` 给出 137.33 ±34.81（n=3）。

- **故障时钟按状态计时的正确性修复**：状态限定模式（`failure.count_time_in_states`）下，
  `Breakdown` 事件到达时没有先结算当前状态区间，读到的是过期的 `up_time`，导致真实故障被推迟到
  下一次状态切换（例如 processing-only 的首次故障从 t=25 变成 t=40）。新增
  `settle_failure_clock(index)`：事件处理前按当前 state/phase/block_reason 结算、且不递增 generation
  （避免把当前事件判成 stale）；顺带补上 `waiting_for_operator`／`waiting_for_transport` 两个状态的映射。
  空列表（墙钟）路径完全未改，并有 `[]` 与省略字段**逐位相等**的兼容测试。
  实测同一模型：只按 processing 计时 `down_fraction` 8.3%，墙钟 25%。
- **修正任务腿语义的误导注释**：`TaskStep` 文档原称「标准序列是 travel, travel, load, unload, travel」，
  实际 `task_sequence` 条目只**消耗基线计数**，超出部分按顺序**追加为额外腿**、不是可重排的标准腿。
  注释已改正，并加 `task_sequence_appends_extra_legs_in_order` 钉住现行语义。

- **实验生命周期钩子（6 个边界）**：`ModelTriggers` 新增 `start_of_experiment` / `end_of_experiment` /
  `start_of_scenario` / `end_of_scenario` / `start_of_replication` / `end_of_replication`；
  `is_empty()` 把 6 个新字段全部计入，**空钩子模型的 JSON 与之前逐字节相同**（有对比测试）。
  复现边界由引擎触发，实验/场景边界经新公开 API `fire_model_hook_actions` 执行、`hook_logs` 取日志；
  单次 `run` 不触发 experiment/scenario 钩子。实测 `experiment` 输出顺序
  `exp_start → scn_start → scn_end → scn_start → scn_end → exp_end`。脚本预编译列表补齐 6 个钩子。

- **任务执行器状态细分**：`StateTimes` 新增 `travel_empty` / `travel_loaded` / `loading` / `unloading` / `utilize`
  （对非执行器恒 0 且不序列化，`ObjectAggregate` 用 `Option` + skip），报告在**模型含执行器时**才追加这 5 列。
  任务链上的切换点：取货腿=空驶、装载、送达腿=载货、卸载、原地等待/服务=utilize、回程=空驶；
  被 `traffic_control` 挡住的时间归 `blocked`，**不进任何 travel 计时**。
  CLI 实测单次运输：`travel_empty 27 / travel_loaded 20 / loading 2 / unloading 3 / utilize 8`（合计 60 = 仿真时长）。
  顺带修了 `node_network_distance` 端点距离 bug：lead/trail 原来按模型顺序取首/末节点，
  反向路径会多算两段，改为按实际起止节点计算（既有 92.43 网络测试仍通过）。

### 报告与 CLI

- **性能指标进入所有渲染出口**：`experiment` 文本表按指标名追加列（CJK 宽度按东亚字符宽度对齐）、
  `report.csv` 追加独立指标段、`experiment.csv` 每指标列对 `<name>,<name>_ci95`、
  Markdown 追加 `## Performance measures` 小节、xlsx 的 Summary 页补同名指标块。
  新增 i18n 四键（性能指标 / 均值 / 95% 置信区间 / 样本数）。
- **修掉两处既有缺陷**：n=1 时指标不再打 `±0.000`（四个出口统一走 `fmt_ci_opt`，显示 `—`）；
  `run --format table` 与 `report --format table` 对同一份报告的输出现在**逐字节相同**
  （该节下沉到 report crate，测试用 `assert_eq!` 锁住）。
  无指标的模型，六种输出经 `cmp` 验证与改动前**逐字节一致**。新增 15 项测试。

### 桌面 GUI（功能补齐）

- **跟踪时序迷你折线**：`Tracked series` 表格的 `shape` 列按采样画一条 84×14 的折线（无坐标轴无图例），
  单调上升/下降与持平一眼可分；单次采样不画点而是标 `1 sample`（避免被当成恒定序列），
  持平序列把水平值标在线中间。GUI 测试 **131 项**。
- **指标单位**：报告只带聚合值，不够显示 `unit`；GUI 同时持有模型与报告，因此
  `Performance measures` 表格的 mean 列会带上模型声明的单位（`12.500 items`），未声明则不追加。
  GUI 测试 **130 项**。
- **报告展示跟踪时序**：模型声明了 `tracked_variables` 时，报告面板新增 `Tracked series (n)` 表格
  （name / samples / first / last / range），并注明序列只覆盖计量阶段。无跟踪变量时不出现该块。
  GUI 测试 **129 项**。
- **性能指标编辑器**：左侧新增 `Performance measures (n)` 面板，逐行编辑 `name / expression / unit`，
  `+ measure` 加行、`-` 删行，`Apply measures` 走 `commit_model_edit`（空名字或空表达式先报错、模型不动，
  空行自动忽略，空 unit 不写进 JSON）。GUI 测试 **128 项**。
- **报告展示自定义性能指标**：模型声明了 `performance_measures` 时，报告面板新增
  `Performance measures` 表格（measure / mean / ±95% / n），复用既有的「n<2 不显示区间」规则。
  无指标的模型不出现该块。GUI 测试 **127 项**。

- **模型触发器面板**：左侧新增 `Model triggers` 折叠面板，`on start` / `on reset` / `on stop`
  各一个 JSON 编辑区（空缓冲区只在模型里有动作时才预填，避免覆盖用户输入），
  点 `Apply model triggers` 走 `commit_model_edit` 单一入口——JSON 不合法时先报错、模型不动。
  GUI 测试 **126 项**。


### 验证

- `cargo test --workspace --locked`：**749 passed / 0 failed**。
- GUI 单元测试 **131 项**；`openflex-model` **163 项**；`openflex-core --lib` **103 项**。
- `clippy` 0、`fmt` 干净；11 个示例模型全部 validate 通过。
- 独立实测：`$Batch * 2` = 20.0（CI 0）、`Clock = time` 采样 `(1,1)…(5,5)`、
  执行器细分 `27/20/2/3/8` 合计等于仿真时长、状态限定故障 `down_fraction` 8.3% 对墙钟 25%。

## v1.33.0 — 2026-11-21

本轮以 **FlexSimChinese7.5 全功能对齐**为目标：对照 326 个帮助页建立可执行的功能矩阵，并把矩阵里「小/中」量级的可补缺口批量补齐——对象库 38/39、交通控制准入互斥真正生效、PickLists/全局变量/表达式接线、加工时点钩子与模型级触发器、GUI 对象取样器。发布节奏保持**至少 3 小时一版**。

### FlexSim 7.5 对象库对齐

对照 `FlexSimChinese7.5/help/FlexSimObjectLibrary` 的权威清单，补齐了缺失的 8 个对象。

- **流体对象**：新增 `ticker`（流体时钟 + 子组件表）、`fluid_processor`（速率/容量/损耗，
  下游堵塞时物料在器内回压）、`fluid_blender`（连续配比混合 + 损耗）、
  `fluid_mixer`（按步表整批释放）、`fluid_splitter`（按下表百分比分流，下游不足时**按比例缩放**保持配比）。
- **两个离散↔流体桥接**：`item_to_fluid`（每个流物件转成 `fluid_per_unit × units_per_item` 体积）、
  `fluid_to_item`（每 `fluid_per_item` 体积打包成一个流物件）。校验器不再把这两个对象当成
  「流体与离散不能相连」的违例，它们正是跨界连接器。
- **装饰对象**：`visual_tool`（box/plane/column/sphere + 尺寸/颜色/文字），完全不参与仿真，
  也不出现在统计报告里。
- 新对象全部走快照/校验/JSON 往返，GUI 的「添加对象」选择器、类型配色、属性表单均已接线
  （分流器提供逐端口百分比编辑并实时提示总和超 100%）。
- 示例模型 `examples/fluid-plant.ofm`：`ticker → generator → processor → splitter(50/25/25)`
  分送水箱/混合器/打包桥，实测三条支路 `2.52 / 1.25 / 1.25` 体积每秒，与百分比一致。
- **Travel Network 对象**：`network_node` 已是一等可摆放对象——节点间用普通 A 连接连成路网，
  边长默认两节点直线距离、可用 `params.edges` 覆盖；运输腿走「起点→最近节点 → 最短路 →
  最近节点→终点」，模型无节点时回退 `transporter.network` 或直线。新增 `traffic_control`
  对象与 capacity/mode/speed_factor 参数（准入互斥尚未在引擎强制，已在矩阵中标注 🟡）。
  实测：A→N1→N2→N3→B 绕行路径得到 92.43（直线仅 70），显式边长 10 覆盖 100 的直线距离。
- **`traffic_control` 准入互斥生效**：控制区成员 = 连到该对象的 `network_node` 集合；
  运输腿（取货/送达/回程）进入「两端最近节点都在同一控制区」的路径时申请名额，
  `capacity` 满则通过 `TrafficWait` 日历事件重试（非忙等），到达即释放；
  `mode: un_timed` 不限名额但按 `speed_factor` 缩放该段旅行时间；
  占用随 `ObjectRt.traffic_hold` 进快照，`restore` 时从持有表重建计数、不泄漏不丢失。
  端到端实测（两辆车、10 米腿、100 秒）：`capacity=1` 产生 964 次等待重试且两车都完成，
  `capacity=2` 与 `un_timed` 均 0 次等待。
  **已知代价**：等待用 0.1 秒固定间隔轮询，车队大时事件量偏高（后续可改为释放时定向唤醒）。
- **PickLists 与全局变量**：模型新增 `picklists`（命名选项列表，条目 name/value）与
  `global_variables`（`kind: number|string` + 值），空集合不出现在 JSON；
  校验覆盖标识符、kind 与值类型一致、条目名非空不重名、列表至少一项；
  `global_variables`/`picklists` 同时补进 JSON Schema，`openflex schema` 可直接用。
- 测试：模型层 `flexsim_parity`（10 项）、引擎行为 `flexsim_fluid_objects`（12 项）、
  引擎路网路由（3 项）、引擎交通控制（4 项）、GUI 选择器与表单（2 项）、
  模型概念与 schema（13 项）。

- **对象取样器（Sampler）**：`setup operator` / `process operator` / `home` / `charger` / `shaft`
  从自由文本改为「下拉候选 + 可手输」的组合——操作员候选只列 Dispatcher 与 Resource，
  `home`/`charger`/`shaft` 列全部对象名（按字母序）；输错名字时旁边出现琥珀色 `unknown` 提示而不是静默接受。
  这补上 `docs/FEATURE_PARITY_75.md` 里「Sampler ⛔」这一行。

- **加工时点钩子与模型触发器**：`SimTrigger` 新增 `setup_finish` / `process_finish`，在处理器的
  「setup 结束」与「加工时钟结束」两个边界触发（早于 `on_exit`；`setup_time = 0` 无独立 setup 阶段故不触发）；
  `Model` 新增 `model_triggers`（`on_start` / `on_reset` / `on_stop`），分别在第 0 秒前、预热边界、
  到达 stop_time 时执行，失败作为 `SimError::Script` 上报而不是静默；顺带把对象 14 类触发器的脚本
  预编译补齐（此前只覆盖 8 类）。新增 3 项引擎测试。
- **表达式可读全局变量与 PickList**：`$Name` 引用全局变量（按声明的 kind 返回数字或字符串），
  裸名 `Name` 仅在引擎无同名值时回退到全局变量（保证 `time`/`item.type` 等既有路径语义不变），
  `picklist("Shift","Night")` 返回条目取值；未定义变量/列表/条目、以及字符串参与算术都是明确的
  求值错误，绝不静默成 0。`EvalContext` 用带默认实现的新方法扩展，外部调用点零破坏。

### 验证

- `cargo test --workspace --locked`：**688 passed / 0 failed**。
- GUI 单元测试 **125 项**；`openflex-model` 146 项；`openflex-core --lib` 81 项。
- `clippy` 0、`fmt` 干净；11 个示例模型全部 validate 通过，GUI 启动无 panic。
- 交通控制端到端实测（CLI + trace，两车/10 米腿/100 秒）：`capacity=1` **7** 次等待事件（改造前 964）、`capacity=2` 与 `un_timed` 均为 0 次，两车完成时刻不变。

## v1.32.0 — 2026-11-21

本轮把 `SimReport` 里**一直存在却没在桌面版显示**的统计数据全部接出来（流程 token 与块级统计、流体液位与流量、Little 定律校验、逐复现明细、峰值内容），并统一了「利用率」在四处展示中的对象范围。发布节奏保持**至少 3 小时一版**。

### 桌面 GUI（功能补齐）

- **对比结果标明可比较性**：报告 JSON→CSV 的对比表在两边复现次数不同时给出琥珀提示
  `different replication counts: baseline 1 vs current 5`；对比 CSV 的注释头也写明各自的
  `(N replication(s))`。此前拿单次运行当基线、与 5 次复现比较时，Δ 会把噪声当成改进。
  新增 `comparisons_flag_mismatched_replication_counts` 测试（GUI 109 项）。

- **关窗也拦运行中的任务**：关闭守卫现在同时覆盖「有未保存改动」与「后台任务仍在跑」两种情况，
  后者提示 `A run is still in progress — close again to abandon it`。此前 Run/实验/优化跑到一半直接关窗，
  那段运算白费且没有任何提示。新增 `closing_also_warns_about_a_run_in_flight` 测试（GUI 110 项）。

- **`AVG UTIL` 只统计有意义的对象**：此前对全部对象求平均（含 source/sink/queue/dispatcher/resource），
  在 `mm1` 上显示 **20%** —— 而真正的加工设备利用率是 **79.8%**，低报 4 倍会直接误导产能判断。
  现在只对「有容量含义」的类型取平均（处理器/传送带/货架/执行器/流体设备等）。
  `counts_for_utilization()` 覆盖 source/sink/queue/dispatcher/resource 与加工类，
  新增 `average_utilization_ignores_sources_and_queues` 测试（GUI 111 项）。

- **`STATE MIX` 沿用同一过滤**：`AVG UTIL` 修好后，`STATE MIX` 仍统计全部对象——源与汇恒为 idle，
  把 `mm1` 的 busy 占比从 **79.8%** 拉到 **20%**。现在两张卡片共用 `counts_for_utilization()`。
  新增 `state_mix_uses_the_same_object_filter` 测试（GUI 112 项）。

- **利用率柱状图同样过滤**：不再为源/汇/队列画 0% 的柱子（那会让真正的设备柱被挤窄）；
  若模型没有任何有容量含义的对象，图内显示 `no objects with a utilisation figure`。
  新增 `utilisation_bars_skip_kinds_without_capacity` 测试（GUI 113 项）。

- **逐对象 `Util` 列对无容量对象显示 `—`**：源/汇/队列行的 `0%` 会被读成「利用率很低」而非「不适用」；
  这一列与 `AVG UTIL`、`STATE MIX`、柱状图现在共用同一个判定。
  顺带确认 `Thru` 列**不需要**过滤（实测源/队列/汇的吞吐量都非零、各有含义）。
  新增 `the_per_object_util_column_marks_not_applicable` 测试（GUI 114 项）。

- **流程统计可见**：报告区新增 `Process flow (N token(s) completed)` 折叠面板——`created / completed / block(s)`
  摘要加一张块级表（block / kind / in / out / avg stay）。`SimReport.process_flow` 一直在，
  CLI 会打印而 GUI 从未展示；用带 `create → sink` 的两个块的模型实测得到
  `created 5 · completed 5`、块统计 `P.C1 (create, 5)` 与 `P.S1 (sink, 5)`。
  新增 `the_flow_report_summarises_tokens_and_blocks` 测试（GUI 115 项）。

- **流体统计可见**：报告区新增 `Fluid (N volume/min throughput)` 折叠面板——`inflow / throughput / overflow`
  摘要加一张对象表（level avg / level max / thru / overflow）。`SimReport.fluid` 一直在、CLI 会打印，
  GUI 从未展示；用 `generator → tank → valve → sink` 实测得到 `inflow 4.00 · throughput 3.63 · overflow 0.00`。
  新增 `the_fluid_report_names_levels_and_flows` 测试（GUI 116 项）。

- **Little 定律校验可见**：报告区新增 `Little's law check (…)` 面板——标题区分「全部在容差内」与
  `N mismatch(es)`，表内逐对象给出 `WIP / 预测值 / 相对误差`，超容差的行走琥珀色。这是 CLI 用来判定
  「结果可信」的关卡，GUI 此前完全没有展示（`mm1` 上三个对象误差均 < 0.008%）。
  新增 `the_little_check_panel_reports_each_object` 测试（GUI 117 项）。

- **逐复现明细**：复现数 > 1 时报告区新增 `Replications (N)` 表——每行的 `# / seed / created / completed`。
  此前只给聚合后的均值与 CI，用户无法核对单次结果或复算某个 seed。
  新增 `replication_detail_lists_seeds_and_counts` 测试（GUI 118 项）。

- **报告表新增 `Max` 列**：逐对象的**峰值内容**（`max_content.mean`），紧跟在 `WIP` 之后，并可点击排序（第 4 列）。
  平均值会掩盖短时堆积，峰值才能回答「这条队列有没有被撑爆」。表格因此从 8 列变 9 列，排序键同步右移。
  新增 `the_stats_table_has_a_peak_content_column` 测试（GUI 119 项）。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**627 passed / 0 failed**。
- GUI 单元测试 **119 项**。
- GUI `clippy` 0、`fmt` 干净；以 `mm1`、带流程块的模型、`generator→tank→valve→sink` 流体模型启动均无 panic。

## v1.31.0 — 2026-11-21

本轮集中在**报告的可信度**：单次复现不再伪装成高精度、预热时间被明确标注、AGV 电量进入画布提示；并让 `Add row` 与其余表操作一样遵守「先校验后快照」。发布节奏保持**至少 3 小时一版**。

### 桌面 GUI（功能补齐）

- **`append_table_row` 补齐约定**：它此前在确认表存在**之前**就记快照（对不存在的表留下空历史项），
  也不清分析结果；现在先判存在、校验通过后才 `snapshot_labelled` 并调用 `clear_analysis_results()`，
  与另外四个表操作一致。新增 `appending_a_row_to_a_missing_table_is_a_noop` 测试（GUI 105 项），
  调用点计数同步为 17。

- **单次复现不再显示 `±0.000`**：`ci95` 在 `replications = 1` 时是 0，画在表格里会被读成「误差极小」。
  报告表与实验表的 CI 列在复现数不足 2 时改为 `—`；CSV 仍写数值 0（便于脚本解析），文件头已注明复现次数。
  新增 `a_single_replication_shows_no_interval` 测试（GUI 106 项）。

- **报告标注预热时间**：配置了 `run.warmup` 时，指标卡下方显示 `warm-up 10000 min ignored`（琥珀、等宽），
  悬停说明「模型时间的前 N 个单位不计入任何统计」。同一个模型有无预热会得到不同的总数，此前报告里看不出来。
  新增 `the_report_says_when_warmup_was_excluded` 测试（GUI 107 项）。

- **AGV 悬停显示电量**：AGV 的悬停摘要新增一行 `battery 73% · charging 25%`（来自 `ObjectStats` 的
  `battery_remaining` 与 `charging_fraction`）；非 AGV 对象不受影响。
  新增 `agv_tooltip_adds_the_battery_line` 测试（GUI 108 项）。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**616 passed / 0 failed**。
- GUI 单元测试 **108 项**。
- GUI `clippy` 0、`fmt` 干净；以单复现模型与 `agv-charging.ofm` 启动均无 panic。

## v1.30.0 — 2026-11-20

本轮补齐**全局表的完整编辑能力**（建表、删表、增列、删行、改列名、逐格编辑），并继续收紧「不丢工作」与「界面状态与模型一致」：切换/关闭/另存路径都不再静默丢弃改动，模型描述可编辑，画布与树的多选行为统一。发布节奏保持**至少 3 小时一版**。

### 桌面 GUI（功能补齐）

- **基线也随模型失效**：`clear_analysis_results()` 现在一并清空 `baseline` 与 `show_compare`——此前改结构或撤销后，
  基线仍留着，新的当前报告会与**旧模型**的基线做差，得到的 Δ 毫无意义。
  两个相关测试的字段清单同步扩展（GUI 93 项）。

- **模型树反映多选**：`tree_row` 现在对「主选中」与「框选/Shift 选中的其余对象」一视同仁地高亮；
  此前只有主选中行亮起，框选 5 个对象在树里只看得出 1 个。新增 `the_tree_lights_up_the_whole_selection`
  测试（GUI 94 项）。

- **画布 Ctrl/Cmd+点击追加选择**：按住 Ctrl/Cmd（或 Shift）在 3D/2D 画布上点对象会把该对象加入现有选区，
  不再重置为单选；`select_inner()` 因此拆出 `select()` 与 `select_without_resetting()`。
  新增 `additive_canvas_clicks_extend_the_selection` 测试（GUI 95 项）。

- **`Align` 说明最少需要几个对象**：`batch_align` 在选区不足 2 个时给出 `align needs at least two objects`，
  与 `Grid`（2 个）、`Distribute`（3 个）的提示口径一致；这三个按钮都没有按选区数量禁用，
  所以此前点 `Align X` 会完全没反应。新增 `arrange_actions_explain_their_minimum_selection` 测试（GUI 96 项）。

- **`Apply` 后重建触发器/班次缓冲**：`apply_form()` 会把 `Triggers`/`Timetable` 的文本解析进参数并提交，
  随后再用提交后的模型重写这两个缓冲（它们存在 `EditForm` 上）。此前提交后文本框仍显示旧内容，
  再点一次 Apply 会把旧触发器写回去。新增 `applying_the_form_rewrites_the_json_buffers` 测试（GUI 97 项）。

- **应用最优解后刷新编辑器**：`apply_best_point()` 会重写对象参数，此前只清了句柄与结果，表单里仍显示旧值，
  再点 Apply 就会把旧参数写回。现在调用新的 `refresh_editor_from_model()`（对失效索引也安全）。
  `Apply JSON` 旁另加一句 `replaces the form's current values`，提示 JSON 模式以自身为准。
  新增 `refreshing_the_editor_needs_a_valid_selection` 测试（GUI 98 项）。

- **模型描述可编辑**：`Model settings` 新增 `DESC` 输入框，`apply_model_settings()` 写入 `model.description`
  （该字段会被 CLI 与 Studio 显示）。此前 GUI 只能改名字，描述始终停留在文件里的原值。
  新增 `model_description_round_trips_through_the_settings_form` 测试（GUI 99 项）。

- **可以新建全局表**：`Global tables` 面板在空状态下提供 `+ table`，创建 2 列 1 行的 `table1`（自动取未占用的名字），
  之后即可用既有单元格编辑器填写。此前模型没有表时面板只显示「表在 JSON 里」，无任何入口。
  新增 `a_new_global_table_is_valid_and_empty` 测试（GUI 100 项），
  `every_structural_edit_path_clears_results` 的调用点数同步为 12。

- **可以删除整张全局表**：表头新增 `x`（悬停 `delete the whole table '<name>'`），`remove_global_table()`
  走快照与校验并清理该表的编辑缓冲；`+ table` 也从空状态移到面板头部，已有表时仍可继续新增。
  新增 `deleting_a_global_table_removes_it_and_its_buffers` 测试（GUI 101 项），调用点计数同步为 13。

- **全局表可增列、可删行**：工具栏新增 `+ col`（追加 `colN` 列，所有行同步补 0）与 `- row`（删末行，
  保留至少一行）；两者都进快照与校验，并清掉该表的编辑缓冲。此前只能加行，列数固定。
  新增 `table_columns_and_rows_can_grow_and_shrink` 测试（GUI 102 项），调用点计数同步为 15。

- **两个视图快捷键**：`F` 取景当前选区（等价于 Frame selection），`Ctrl/Cmd+2` 切换平面 2D（与工具栏按钮同一路径，
  会顺带把相机切到俯视并写入偏好）；帮助面板与一致性测试同步登记。

- **全局表列名可重命名**：列名从只读胶囊改为输入框（失焦即提交），`rename_table_column()` 拒绝空名与重名，
  走快照与校验并清掉该表的缓冲。此前列名只能去 JSON 里改。
  新增 `table_columns_can_be_renamed` 测试（GUI 103 项），调用点计数同步为 16。

- **表格报错与提示更具体**：行宽不符时错误信息带上表名（`table2 row 0 needs 2 cell(s), got 1`），
  单元格输入框悬停说明「数字 / true,false / 文本，提交时自动判定类型」。
  新增 `table_errors_name_the_table` 测试（GUI 104 项）。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**612 passed / 0 failed**。
- GUI 单元测试 **104 项**；「交互约定」九条各有一条测试锁住（见 `docs/GUI.md`）。
- GUI `clippy` 0、`fmt` 干净；以多张全局表、带描述、`by_label`、任务序列等模型启动均无 panic。

## v1.29.0 — 2026-11-20

本轮把桌面 GUI 的**数据可信性**与**不丢工作**做实：所有会写回模型的数值统一为 f64、结果随模型结构变化失效、后台任务的结果都带来源模型；同时补齐「丢弃文档 / 批量删除 / 关闭窗口」的二次确认，并让导出与另存的落盘位置可预测。发布节奏保持**至少 3 小时一版**。

### 桌面 GUI（功能补齐）

- **文档记录交互约定**：`docs/GUI.md` 新增「交互约定」一节，把九条由测试锁住的行为（先校验后快照、结果随模型失效、
  位置操作不清结果、丢弃文档/批量删除的二次确认、异步结果带来源、数值一律 f64、路径可预测、提示与实现同步）
  连同对应测试名列成表格；能力表版本号与测试计数（93 项）同步。
- **保存后复位关闭确认**：`close_confirmed` 在成功保存时清零，避免「保存后再改、再关窗」跳过提示。
  新增 `a_save_resets_the_close_confirmation` 测试（GUI 93 项）。
- **关闭窗口前提示未保存**：窗口的关闭按钮在有未保存改动时第一次被 `CancelClose` 拦下，状态栏给出
  `Unsaved changes — close again to discard them, or Save first`，第二次才真正退出。
  此前点关闭就直接结束进程，未保存的改动全部丢失。新增
  `closing_with_unsaved_edits_needs_a_second_press` 测试（GUI 92 项）。
- **守卫状态及时失效**：成功保存或任何一次新编辑（`snapshot_labelled`）都会清除 `pending_discard`，
  不会出现「几十分钟前按过一次 New，现在按 Open 就直接丢改动」的残留提示。
  新增 `saving_or_editing_cancels_a_pending_discard_prompt` 测试（GUI 91 项）。
- **未保存守卫覆盖所有打开入口**：`load()` 内部先过 `may_replace_document()`，因此工具栏、`Recent` 菜单、
  内置文件浏览器、示例加载都受保护（此前只有工具栏的两个按钮）；已确认过的示例加载改走
  `load_unchecked()`，`load_example()` 自己持守卫，不会再重复问两次。
  新增 `the_discard_guard_keys_on_the_action_you_pressed` 测试（GUI 90 项）。
- **文件路径统一走 `report_path()`**：时序 CSV 与视图 SVG 的备用文件名（`openflex-timeseries.csv`、
  `openflex-view.svg`）改为与其它导出同一套「模型同目录 / `~/Documents`」规则；
  `preset_path()` 对未保存的模型不再返回 `None`（那会让「保存布局预设」静默失败），改用
  `~/.config/openflex/<模型名>.layout.json`。新增 `preset_path_falls_back_to_the_config_dir` 测试（GUI 89 项）。
- **另存建议为绝对路径**：未保存的模型按 `Save` 时，对话框预填 `~/Documents/<name>.ofm`
  （没有 Documents 则落到家目录），不再是裸文件名 `untitled.ofm`——后者会保存到启动目录。
  新增 `save_as_suggestion_is_absolute` 测试（GUI 88 项）。
- **导出路径可预测**：未保存的模型导出 CSV/JSON/SVG 时，文件落到 `~/Documents/<模型名>.<ext>`
  （没有 Documents 则落到家目录），而不是依赖 GUI 启动时的进程工作目录——从桌面图标启动时那里是 `/`。
  已保存的模型仍写在模型同目录。新增 `export_paths_are_predictable_without_a_saved_model` 测试（GUI 87 项）。
- **预设删除提示**：布局预设的 `x` 悬停给出 `delete the preset '<name>'`。
- **替换未保存文档需二次确认**：`New` / `Open` / 内置示例在 `model_dirty` 时第一次只给出
  `Unsaved changes — press <动作> again to discard them, or Save first`，再按一次才真正替换；
  工具栏按钮与 `Ctrl/Cmd+N`、`Ctrl/Cmd+O` 走同一个 `may_replace_document()` 守卫。
  此前改完模型直接点 New 就会静默丢失全部改动。新增
  `replacing_a_dirty_document_needs_a_second_press` 测试（GUI 86 项）。
- **"unsaved edits" 覆盖 JSON 模式**：参数 JSON 与流程 JSON 的编辑现在也点亮未保存提示；`Triggers` /
  `Timetable` 两个框通过 `EditForm::show` 的返回值汇入同一个 `form_changed`。此前只有表单控件会触发提示，
  用 JSON 改参数时看不到任何提示。新增 `json_buffer_edits_mark_the_form_dirty` 测试（GUI 85 项）。
- **重命名与连接编辑也清结果**：报告按对象名索引，改名会让旧条目失效；连接增删与端口策略改变路由。
  三处都接入 `clear_analysis_results()`，现在共 **11 个结构性调用点**（撤销、重做、应用最优、模型设置、
  新增、删除、复制、重命名、连接增/删、端口策略）。新增 `every_structural_edit_path_clears_results`
  测试按调用点数与六个缓冲区锁定该约定（GUI 84 项）。
- **结果清理收敛为 `clear_analysis_results()`**：撤销/重做、应用最优解、模型设置、新增/删除/复制对象
  都改走同一个函数（报告、实验行、优化行与最优解、时序、trace）；此前这些路径只清 `report`，
  实验与优化的旧行会留在界面上。位置类操作（移动/旋转/对齐/分布/网格/自动布局）不影响统计，故不清。
  新增 `structural_edits_invalidate_results_but_moves_do_not` 测试（GUI 83 项）。
- **撤销/重做清空分析结果**：撤销或重做改变模型后，报告、实验结果、优化结果与最优解、时序采样、trace
  一并清空，状态栏注明 `(results cleared)`。此前这些面板仍显示撤销前的数字，容易被当成本模型的结论。
  新增 `undo_and_redo_drop_stale_analysis_results` 测试（GUI 82 项）。
- **对比结果窗口**：`Compare to file` 的结果除了状态栏摘要，还会打开一个可滚动的窗口逐条列出
  （`+` 绿、`-` 红、`~` 琥珀、其余灰），长摘要不再被下一条状态消息顶掉。
  新增 `compare_report_lines_are_colour_coded` 测试（GUI 81 项）。
- **与磁盘版本对比**：文件菜单新增 `Compare to file`——逐对象/逐连接比较内存模型与 `.ofm` 文件，
  状态栏给出 `vs <path>: +1 object(s): X · -1 object(s): Y · ~2 edited: Z · connections +1/-0 · run settings changed`
  或 `No differences against <path>`。此前想知道「改了哪些」只能靠 git diff 原始 JSON。
  新增 `compare_finds_added_removed_and_edited_objects` 测试（GUI 80 项）。
- **帮助面板的维护区**：`F1` 对话框新增 `HOUSEKEEPING`——`Reset window layout` 删除
  `~/.config/openflex/app.ron`（下次启动回到默认窗口/面板），`Clear recent files` 清空最近文件；
  两者都在状态栏报告结果。新增 `housekeeping_actions_are_safe_without_config` 测试（GUI 79 项）。
- **Recent 列表标记失效文件**：菜单里已删除的路径显示为暗色 `name (missing)`，悬停给出完整路径；
  点击时不再尝试加载，而是从列表中移除并说明原因。此前这些条目混在正常项里，点了才报错。
  新增 `missing_recent_entries_are_dropped_on_click` 测试（GUI 78 项）。
- **布局预设的读写反馈**：`load_presets()` 不再对损坏或缺字段的边车文件静默返回，而是给出具体错误；
  `save_presets()` 返回是否写盘成功，`store_layout_preset` / `delete_layout_preset` 据此区分
  `Saved …` 与 `Saved … (not written to disk)`，覆盖同名预设时提示 `Overwrote …`（此前是静默替换）。
  新增 `preset_status_lines_distinguish_write_results` 测试（GUI 77 项）。
- **被拒绝的编辑不再污染历史**：`add_connection` 原先在输入校验**之前**就记快照——空输入、无效端口、自连、
  重复边都会在历史里留下一条空记录；`batch_move`/`batch_rotate`/`apply_form`/`apply_json` 同样是先快照后
  `validate()`。五处统一改为「先校验、通过后才记快照」。新增 `rejected_edits_do_not_leave_history_entries`
  测试（GUI 76 项）。
- **新 transfer 块的目标取自模型**：`+transfer` 以前硬编码 `"object": "Sink1"`，在没有该名字的模型里
  Apply 就会校验失败。现在优先取模型中的第一个 sink，没有 sink 就取第一个对象，空模型才回落到 `Sink1`。
  新增 `new_transfer_block_targets_an_object_in_the_model` 测试（GUI 75 项）。
- **Run / 实验 / 优化可放弃**：三者运行中按钮分别变为 `Stop run` / `Stop experiment` / `Stop`，
  点击后丢弃接收端，状态栏说明 `… abandoned — its result will be ignored`。引擎本身没有取消钩子
  （`run_model` 一次跑完复现），因此这里放弃的是「结果落盘」，界面不再被锁住。
  新增 `abandoning_a_background_job_drops_its_channel` 测试（GUI 74 项）。
- **采样可取消**：时序采样运行中按钮变成 `Cancel`，点击后工作线程通过原子标志提前停止，状态栏报告
  `Sample cancelled after N sample(s) — partial data kept`；此前大模型采样期间面板被锁住，只能等它跑完。
  新增 `sampling_can_be_cancelled` 测试（GUI 73 项）。
- **保存成功清理标记**：普通 `Save` 成功后也清 `form_dirty` 与错误横幅，并把模型文件计入 `Outputs`。
- **批量删除二次确认**：一次删除 3 个及以上对象时，第一次 `Delete` 只给出
  `Press Delete again to remove the selection`，再按一次才执行；删 1–2 个仍是一步完成。
  待确认的名单在选中变化（点击、全选、框选、组内全选）时立即失效，避免隔了很久误删。
  新增 `group_deletes_need_a_second_press` 测试（GUI 72 项）。
- **可选字段记住上次的值**：`optional_u32` / `optional_f64` 取消勾选再勾选时，不再回落到默认的 `1`，
  而是恢复用户上次填的数字（存在 egui 的临时数据里，按标签区分）。新增
  `optional_fields_restore_the_remembered_value` 测试（GUI 71 项）。
- **表单坐标改用 f64**：`EditForm` 的 `pos`/`z` 与批量工具的 `dx`/`dy`/`rot` 原为 `f32`——打开表单再 Apply
  会对模型坐标做一次 f32 往返，`16777217` 这类值会被改成 `16777216`。改为 f64 后表单往返是恒等变换。
  新增 `form_position_round_trips_exactly` 测试（GUI 70 项）。至此 GUI 侧所有「会写回模型」的数值输入均为 f64。
- **实验与优化参数改用 f64**：`exp_start`/`exp_step` 与优化变量的 `low`/`high` 原是 `f32`，与 `DragValue`
  的取值域不匹配；f32 只有 24 位尾数，`2^24` 与 `2^24+1` 会折叠成同一个数。改为 f64 并去掉多余的类型转换。
  新增 `sweep_and_bounds_use_f64` 测试（GUI 69 项）。
- **一键应用最优解**：优化结果表下方新增 `Apply best to the model`——把最优变量值按配置的 JSON 路径写回模型，
  走 `snapshot_labelled("apply best point")` 与 `validate()`，并清空旧的报告/句柄。此前只能手抄这几行数值。
  非数值路径或变量数与结果不符都会给出明确错误。新增 `best_point_applies_through_json_paths` 测试（GUI 68 项）。
- **基线捕获给出摘要**：`set` 基线成功后显示 `Baseline captured: <模型> · completed N`，替换旧基线时改为
  `Baseline replaced: …`，不再是干巴巴的 "Baseline captured"。
- **跳转/单步提示统一**：`jump_to_event()` 失败时给出 `Jump failed: <原因>` 并停止播放标记；成功时写
  `Jumped to t=7.500 min`。`step_event()` 的 `Stepped to …` 也补上时间单位。新增
  `jump_and_step_status_lines_name_the_unit` 测试（GUI 67 项）。
- **修复：Play 卡在「播放中」**：`advance()` 步进失败时只写错误横幅，不会清 `running`/`paused`，工具栏会一直
  显示在播放；现在同时停止播放并给出 `Run stopped: <原因>`。`Play` 也改为先 `new_handle()` 再宣布 Playing。
  另加 `a_failed_step_clears_the_playing_flags` 与 `play_only_starts_when_the_handle_exists` 测试（GUI 66 项）。
- **切换对象时提示丢弃编辑**：当前对象有未应用的改动却在对象树里点了别的对象时，状态栏给出
  `Discarded unsaved edits on <name>`；此前 `form_dirty` 只在切换到同对象时被清掉，跨对象编辑会静默丢失。
  新增 `selecting_another_object_reports_discarded_edits` 测试（GUI 64 项）。
- **三个 JSON 框实时校验**：`Triggers` / `Timetable` / `Process Flow` 下方各加一行状态——`valid JSON` 为绿色，
  出错时显示解析器的具体信息（含位置），不再只在 Apply 时才报错。`json_status()` 区分「可为 null」
  （timetable、flow）与「必须为对象」（triggers）：`null` 本身是合法 JSON，需要显式拒绝。
  新增 `json_status_accepts_null_only_for_optional_boxes` 测试（GUI 63 项）。
- **路径提示取自当前模型**：实验 `PATH` 与优化变量路径的占位提示原来写死 `objects/0/params/process_time/value`
  （在 `mm1` 里索引 0 是源点，该字段根本不存在）。改为 `example_path()` 从当前模型取第一个真实数值叶子，
  无模型时回落到通用示例。新增 `example_path_is_a_real_path_in_the_model` 测试（GUI 62 项）。
- **修复：`Timetable` 提示字段写错**：表单里的示例写的是 `{"windows": [{start, duration, repeat_every}]}`，但
  `TimeTable.windows` 实际是 `DowntimeWindow { start, end }`——照抄示例会得到 `unknown field 'duration'`。
  提示已改为 `{"windows": [{"start": 0, "end": 28800}]}`，`docs/GUI.md` 对应行同步修正（并注明 `shifts` 才是
  `{start, duration, repeat_every}`）。新增 `timetable_hint_matches_the_model_schema` 测试（GUI 61 项）。
- **`BasicFr` 表单显示状态**：`process flow / timetable / emptier` 三个布尔从一行纯文本改为 `on/off` 彩色标签，
  并在开启时提示去哪里编辑（流程走画布、emptier 走 JSON），不再一句「(edit via JSON)」打发。
- **run 设置改用 f64**：`model_stop` / `model_warmup` 原是 `f32`，而 `DragValue` 的范围到 `1e9`——该量级下 f32
  的整数精度只有 64，`1e9+1` 会变成 `1e9`。改为 f64 后与模型字段同类型，往返不失真。
  新增 `run_settings_keep_precision_at_large_values` 测试（GUI 59 项）。
- **分布步长防除零**：`batch_distribute` 的 `(last-first)/(count-1)` 加显式守卫（`count > 1` 才除），
  即使前置检查将来被改动也不会 panic；`distribute_spacing_survives_a_two_object_selection` 测试（GUI 60 项）。
- **窗口与面板状态持久化**：启用 eframe 的 `persistence`（纯 Rust，无新增系统依赖），把 `persistence_path` 指向
  `~/.config/openflex/app.ron`，窗口尺寸/位置、侧栏宽度与各折叠面板的开合状态重启后保留。
  另加 20 秒一次的 `request_repaint_after`——eframe 只在窗口事件时自动保存，只读不操作时状态原本不会落盘。
  实测生成 5088 字节的 `app.ron`，内含窗口几何与各 CollapsingHeader 的 `open/open_height`。
  新增 `persistence_path_is_a_file_next_to_the_other_configs` 测试（GUI 58 项）。
- **切换分布保留参数**：在分布下拉里改类型时，新分布会继承旧分布里**同名字段**（`normal(mean 8)` → `exponential`
  仍是 8；`gamma(shape 3, scale 4)` → `weibull` 不变；`integer_uniform(1..6)` → `triangular` 保留 1 和 6），
  没有共同字段才回落到默认值。此前每次切换都会重置成默认，手填的数值全丢。
  新增 `switching_distribution_keeps_shared_parameters` 测试（GUI 55 项）。
- **`by_label` 队列排序可编辑**：队列 `order` 下拉新增 `by_label` 选项，并在其下给出标签输入框；
  此前只有 `fifo/lifo/priority` 三项，`by_label` 显示为 `by_label(…) — edit via JSON`。
  新增 `queue_order_by_label_round_trips` 测试（GUI 56 项），确认 `{"by_label":"…"}` 的序列化形式。
- **任务序列编辑器**：调度器的 `task_sequence` 从「`N (edit via JSON)`」改为真正的编辑器——每步可选
  `travel/load/unload`、可 `up` 上移与 `x` 删除，另有 `+travel/+load/+unload` 追加与 `clear`；
  切换种类时保留该步原有的 `duration`/`condition`。新增 `task_steps_keep_their_details_when_the_kind_changes`
  测试（GUI 57 项）。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**601 passed / 0 failed**。
- GUI 单元测试 **93 项**；其中「交互约定」九条各有一条测试锁住（见 `docs/GUI.md`）。
- GUI `clippy` 0、`fmt` 干净；以 `mm1.ofm`、无效模型、`by_label` 队列、任务序列、全局表等模型启动均无 panic。

## v1.28.0 — 2026-11-20

本轮集中在**结果可信**与**图表可读**：后台任务的每个结果都记录来源模型、检查点与基线的归属可辨、报告与图表补上可解释的刻度与汇总；并修掉连接自连/重复、检查点「最新」排序、自动布局漏对象等缺陷。发布节奏保持**至少 3 小时一版**。

### 桌面 GUI（功能补齐）

- **基线归属校验**：`Load baseline` 时若基线报告的模型名与当前模型不同，状态栏提示
  `(warning: baseline is for 'X', current model is 'Y')`，避免把别的模型的报告当成本模型基线。
  新增 `baseline_note_only_fires_on_a_name_mismatch` 测试（GUI 42 项）。
- **「最新检查点」排序修正**：`Load checkpoint` 原按文件名字典序取最后一项，`checkpoint-9.00` 会排在
  `checkpoint-100.00` 之后；改为解析文件名中的模型时间排序（`checkpoint_time()`），并加
  `checkpoints_sort_by_model_time_not_by_name` 测试（GUI 46 项）。
- **实验柱状图**：最优场景的柱子改为绿色（表格里也仍以 `*` 标注），悬停时描边并在柱顶显示完成数。
- **端口策略编辑可撤销**：`set_connection_strategy` 先做越界检查再记快照，并把动作名写进历史
  （`connection: rr / random / hold / first / by type / by label / by percent / by condition`）；
  此前用的是无标签的 `snapshot()`，越界时还会留下空历史项。
- **连接删除提示**：连接列表的 `x` 悬停时提示 `remove A -> B`，避免在长列表里点错行。
- **连接添加的校验**：`add_connection` 现在拒绝自连与重复边（流程连线早有同样规则，对象连线没有），
  错误信息为 `{from} -> {to} already exists` / `an object cannot connect to itself`。
  新增 `connections_reject_self_and_duplicates` 测试（GUI 54 项）。
- **复制后选中副本**：`Duplicate`（`Ctrl/Cmd+D` 与批量按钮）现在把选区切到刚复制出的对象，接着就能移动它们；
  此前复制后选区仍停在源对象上，看不出发生了什么。命名续号已有唯一性循环，补 `duplicate_names_never_collide`
  测试（GUI 53 项）。
- **自动布局覆盖全部对象**：`layout_order()` 的 BFS 在「没有源点」的图（例如只含环）上不会入队，
  但未访问对象仍会追加到末尾，另加 `debug_assert_eq!` 保证顺序长度等于对象数；新增
  `layout_order_covers_every_object_including_cycles` 测试（GUI 51 项）。
- **全局状态占比**：报告顶部新增 `STATE MIX` 条（busy/blocked/idle/down 的模型级占比）与一行百分比文字，
  补上「整机队有多忙」这一视角（此前只有逐对象的四段小条）。`state_mix()` 对空模型返回全零而不是 NaN，
  新增 `state_mix_normalises_and_handles_zero` 测试（GUI 50 项）。
- **时序图可读数**：图内新增 50% 刻度参考线与中点数值；鼠标悬停显示竖线标记与 `t=… min · <值>` 读数
  （`series_index_at()`，越界与空样本都有定义）。此前只能看到曲线形状，读不出具体数值。
  新增 `series_hover_index_stays_in_range` 测试（GUI 49 项）。
- **热力图可解释**：没有报告时下拉项显示 `(run first)` 并禁用（此前静默回退到类型色）；
  选中指标后显示**色标**（绿→黄→红，两端标注实际 min/max，利用率固定 0–100%）。
  画布底部那三块无语义的色块改为一行 `coloured by <metric> · scale in the toolbar`。
  新增 `heatmap_legend_range_falls_back_when_empty` 测试（GUI 48 项）。
- **缩放范围统一**：取景计算的 clamp 是 `0.15–2.5`，滚轮是 `0.2–6.0`，取景后一滚轮画面就会跳；
  改为共用 `ZOOM_MIN/ZOOM_MAX`。两条范围断言写成了编译期常量，被 clippy 指出后改为运行时计算。
- **相机俯仰角统一**：`Top`/2D 预设用 `1.45`，而拖动时的 clamp 上限是 `1.4`，选完 Top 再拖会突然跳变；
  改为共用 `PITCH_MIN/PITCH_MAX` 常量。新增 `camera_presets_stay_inside_the_drag_range` 测试（GUI 47 项）。
- **检查点收尾**：恢复检查点后清空平滑插值、回放位置、事件日志与播放状态（此前会沿用上一次运行的动画与日志）；
  保存在同一模型时间重复保存时改为加 `-2`、`-3` 后缀，不再静默覆盖。新增
  `checkpoint_names_do_not_overwrite_each_other` 测试（GUI 45 项）。
- **基线来源可见**：`BASELINE` 行在 `load` 旁显示 `from <模型名>`——与当前模型一致时为暗色，不一致时转**琥珀**并提示
  `baseline was captured from a different model`。新增 `baseline_ownership_matches_by_name` 测试（GUI 44 项）。
- **四类异步结果都能溯源**：`Run` / 时序采样 / 实验 / 优化现在都把模型名随结果送回，
  若当前文档已改名或换模型，状态栏标注 `from 'X' (model changed since)`；此前换模型后旧时序曲线与新结果
  会被当成本模型的数据。新增 `every_worker_reports_the_model_it_ran` 测试（GUI 43 项）锁住这一点。
  四处判断收敛为 `result_is_stale(&source)` 一个辅助函数，避免再出现「只改了其中两处」的情况。
- **报告与模型一致性**：`Run` 结束时若模型名与报告里的不一致（运行期间换了文档），状态栏追加
  `· model changed during the run`，避免把旧报告当新结果看待。
- **上下文菜单编辑补齐**：右键 `Reset position` 现在记快照、走 `validate()` 并刷新表单（此前三样都没有）；
  右键旋转 `±15°` 后若该对象被选中，也会刷新侧栏表单，不再显示旧的 `rot`。
- **网格排布基准**：`Grid N` 的原点改为选区**左上角**（与 `Align` 一致），不再取「先点选的那个」。
  新增 `grid_layout_anchors_on_the_selection_corner` 测试（GUI 41 项）。
- **`reset_for_new_document()`**：New / Load / 内置示例三条入口合并到同一个重置函数（编辑器缓冲、事件日志、
  运行状态、撤销栈），并新增 `opening_a_document_clears_every_buffer` 测试（GUI 40 项）锁住这一约定。
- **`New model` 收尾**：同样改用 `clear_object_editor()` 并清空事件日志与播放状态；此前新建后上一个模型的
  插值缓存、表格缓冲与事件日志仍在。
- **内置示例加载收尾**：`load_example()` 走内置示例的分支改为复用 `clear_object_editor()`，并同时清空事件日志、
  撤销/重做与运行状态；此前只清了选中与表单，上一个模型的插值缓存、表格缓冲与事件日志会残留。
- **全局表编辑收尾**：编辑缓冲随文档一起清空（撤销/加载模型时 `clear_object_editor()` 一并 `table_edit.clear()`），
  且 `commit_table_row()` 先校验行列形状**再**记快照——被拒绝的编辑不再留下空的历史项。
  新增 `edited_row_must_match_the_table_width` 测试（GUI 39 项）。
- **全局表可编辑**：`Global tables` 面板从只读预览改为**逐单元格编辑**（等宽输入框，失焦即提交），
  新增 `+ row`（追加一行默认值）与 `reset`（丢弃缓冲、从模型重新读取）；
  `commit_table_row()` 走 `snapshot_labelled("edit global table")` 与 `validate()`，
  单元格按文本自动识别为数字/布尔/字符串（`parse_table_cell()`）。新增
  `table_cells_parse_by_looking_at_the_text` 测试（GUI 38 项）。
- **界面偏好持久化**：`~/.config/openflex/ui.json` 保存 `flat_view / tree_by_stage / parallel / play_speed / show_json`，
  启动时 `load_ui_prefs()` 读回（速度会 clamp 到 0.25–8）；切换开关即写盘。新增 `ui_prefs_round_trip` 测试（GUI 37 项）。
- **补回 `parallel` 开关**：`parallel_runs` 字段与两条运行路径一直在，但工具栏从未渲染过它可以关闭的控件——
  在 `RUN` 组补上 `parallel on/off` 按钮，状态同样持久化。
- **Trace run 移出 UI 线程**：`simulate_traced()` 改在后台线程执行（通道携带耗时），每帧 `poll_trace()` 收结果，
  运行时按钮显示 `Tracing…` 并禁用，状态栏报告 `Traced N event(s) in X.XXs`；此前大模型会直接卡住窗口。
- **`Delete` 删除选中**：新增 `Delete` / `Backspace` 快捷键（文本框有焦点时不触发），走 `batch_delete()` 的
  引用清理与撤销路径；帮助对话框补上该键与 `2D on/off`、相机预设三项。
  新增 `help_dialog_lists_every_shortcut` 测试（GUI 36 项）：帮助里列出的每个键都能在源码中找到对应的
  `egui::Key::*`，防止文档与实现再次脱节。
- **两个内置示例**：空状态新增 `agv-delivery`（调度器 + AGV 把成品送到汇点）与 `fluid-line`
  （发电机 → 水箱 → 阀门 → 汇点），覆盖 AGV 与流体两条此前没有示例的路径。
  两者都用 CLI 跑过：`agv-delivery` 完成 30 件、Agv1 利用率 0.45；`fluid-line` 流量 4.0 → 3.63。
  空状态的示例改为**横向胶囊按钮**（悬停给出说明），五个示例不再一人一行占满面板。
- **跳转失败提示**：点击事件日志但模型无法启动时，状态栏给出 `Jump failed: the model could not be started`，
  不再静默无反应。
- **检查点自动扫描**：启动时调用一次 `refresh_checkpoints()`，已存在的 `checkpoints/*.json` 直接出现在列表里，
  不必先点 `Refresh list`。
- **另存为收尾**：`Save as…` 成功后清空 `model_dirty`/`form_dirty` 并调用 `save_presets()`，
  避免「刚另存完仍显示未保存 `*`」以及布局预设写到旧路径的边车文件。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**562 passed / 0 failed**。
- GUI 单元测试 **54 项**（取景、BFS 布局、内置示例、字体、热力色、明暗、Pareto、事件格式、XML 转义、
  重命名引用、删除清理、trace 行、多边形命中、路径收集、字节格式、校验缓存、撤销标签、表格单元格、
  检查点排序、相机范围、状态占比、时序读数等）。
- GUI `clippy` 0、`fmt` 干净；以 9 种方式启动（含中文模型、AGV、流体、无参数）均无 panic。

## v1.27.0 — 2026-11-20

本轮继续补桌面 GUI 的编辑体验：**可排序报告表**、**两级模型树**、**带标签的撤销历史**、**窗口标题同步**、
四类 CSV 自描述头，并修掉两个真实缺陷（流程块删不掉、对齐基准依赖点选顺序）。发布节奏保持**至少 3 小时一版**。

- **对齐基准**：`Align X/Y` 改为按选中集合的**最小坐标**对齐（旧实现取「先点选的那个」，结果依赖框选顺序），
  状态栏回显 `Aligned on X at -1.0`；新增 `align_uses_the_selection_extreme` 测试（GUI 35 项）。
- **两级模型树**：每个 kind 组内再按**流程阶段**（沿连接从源点 BFS 的跳数）折叠成 `stage 0/1/2…`，
  孤立对象自成一档；树头部 `by stage / flat` 按钮可切换。
  抽出 `tree_row()` 渲染行、`stage_of()` 计算阶段，新增 `stage_of_counts_hops_from_the_sources` 测试（GUI 34 项）。
- **报告表可排序**：8 个列头变成按钮，点击按该列排序（首次降序、再点升序），当前列高亮并带 `▾`；
  可按 In / Out / WIP / Stay / Util / Thru / ±CI 排序，或按对象名。新增 `report_rows_sort_by_the_selected_metric`
  测试（GUI 33 项）。
- **修复：流程块删不掉**：`remove_flow_block` 的 `retain` 闭包里 `|id|` 与函数参数 `id` 同名，
  比较变成「块名 != 块名」恒为假，删除永远无效；改用 `target` 后按精确 id 删除，并加
  `removing_a_flow_block_by_id_actually_removes_it` 回归测试（GUI 32 项）。
- **窗口标题同步**：操作系统标题栏改为 `OpenFlex — <模型名><未保存 *><— playing/— running>`，
  有变化时才发 `ViewportCommand::Title`；`window_title_text()` + `window_title_reflects_document_state` 测试（GUI 31 项）。
- **撤销历史列表**：撤销栈改为 `(json, label)` 二元组，16 个主要编辑入口（新增/删除/复制/移动/旋转/对齐/分布/网格/
  自动布局/重命名/连接增删/表单/JSON/模型设置/布局预设）都用 `snapshot_labelled()` 记录动作名；
  左侧新增 `History (n undo / m redo)` 面板，按序号列出每步并可**点击回退到该步**，状态栏显示 `Undo: <动作>`。
  新增 `undo_stack_keeps_a_label_per_step` 测试（GUI 30 项）。

### 文档

- **`docs/GUI.md` 刷新**：能力表补上 v1.26.0 起新增的 9 项（文件浏览、产物清单、trace、事件日志、平面 2D、校验状态、
  时间单位、warm-up、CSV 自描述、全局表），删除重复的 `运行` 行，单元测试计数更正为 29 项；
  路线图与「与 FlexSim 差距」表移除早已交付的项，改为真实的 5 项剩余（纹理/实时光照、层级树、协作、历史列表、本地化）。

### 桌面 GUI（功能补齐）

- **事件日志筛选/导出**：事件面板新增过滤框（按 `kind` 或对象名，忽略大小写）、`CSV` 导出（写出
  `<model>.events.csv`，含逗号/引号的对象名会转义）与 `clear`；无匹配时提示 `no events match the filter`。
  新增 `event_log_matches_filters_case_insensitively` 测试（GUI 27 项）。

- **常驻校验状态**：状态栏新增 `valid` / `N issue(s)` 指示（悬停列出前 6 条问题）；
  此前校验结果只在选中对象的表单里可见，没选中时无从判断模型是否可跑。
- **校验结果缓存**：新增 `model_gen` 世代号与 `validity_cache`，`validation_issues()` 只在模型变化后重算。
  200 对象 / 201 连接的模型单次 `validate_collect` 约 3–5 ms，每帧调用会明显浪费；新增
  `validation_cache_follows_the_model_generation` 测试（GUI 28 项）。

- **Play 也记录事件**：`advance()` 在推进到目标时间前，把途经的事件写入日志（每次最多 50 条，日志上限仍为 200，
  每步只裁剪一次）。此前只有 `Step event` 会填充日志，用 Play 观察时右侧面板一直空白。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**543 passed / 0 failed**。
- GUI 单元测试 **35 项**。
- GUI `clippy` 0、`fmt` 干净；以 `mm1.ofm`、`routing.ofm`、`process-flow-transfer.ofm`、`agv-charging.ofm`、
  `engine-integration.ofm`、`elevator.ofm`、`conveyor.ofm`、中文模型与无参数共 9 种方式启动均无 panic。

## v1.26.0 — 2026-11-20

本轮把桌面 GUI 的**平面视图**做成真正的二维布局（含旋转、命中、导出），补齐**产物清单**、**优化字段选择器**、**内置文件浏览器**与**真实 trace**，并修掉一批「静默丢弃 / 无法撤销 / 状态残留」的问题。发布节奏保持**至少 3 小时一版**。

### 桌面 GUI（功能补齐）

- **真实 trace**：工具栏 `STATE` 组新增 `Trace run`——用 `simulate_traced()` 完整跑一次并把引擎事件流留在内存，
  状态栏报告事件数（截断时注明）与完成件数；`Export trace` 写出 `<model>.trace.jsonl`，
  行格式与 `openflex run --trace` 一致（`simulate_traced` 同一序列化路径）。
  此前桌面版的“事件日志”只记录 GUI 自己步进过的事件，看不到完整事件流。
- 新增 `trace_export_lines_match_the_cli_jsonl` 测试（GUI 18 项）。
- **优化无结果提示**：优化跑完却没有结果时，状态栏区分「某个变量没有 JSON 路径」与「检查路径与上下界」，
  不再只给一句 `Optimisation finished without results`（该提示通常意味着路径写错，而不是真的没有更优点）。
- **实验失败可见**：某个场景运行失败时不再静默丢弃，状态栏追加 `N scenario(s) failed`。
- **新增对象摆放**：新对象落在模型右侧的**第一个空位**（列内向下避让），连续新增不再全堆在 `y=0` 一条线上；
  新增 `new_objects_do_not_stack_on_one_spot` 测试（GUI 26 项）。
- **编辑器缓冲清理**：撤销/重做与加载模型现在共享 `clear_object_editor()`，一并清空 `params_text`/`flow_text`/`rename_text`
  与插值缓存；此前撤销后侧栏仍留着旧对象的 JSON，继续编辑会把过期内容写回。
- **撤销覆盖补齐**：`apply_form()` 与 `apply_model_settings()` 现在都会 `snapshot()`——此前改表单或 run 参数
  无法撤销，也不点亮未保存标记；模型设置的提示同时带上 seed。
- **流程块 ID 唯一**：新增块时不再用「块数+1」编号（删除后会撞名），改为从 `len+1` 起找第一个未被占用的
  `前缀+数字`；新增 `flow_block_ids_avoid_reuse` 测试（GUI 25 项）。
- **流程连线校验**：连接前检查两端都是**已存在的块**、禁止自连、跳过重复边，并在状态栏回显
  `A -> B added` / `already exists`；不合法的连接不再静默写入、等到 Apply 才报错。
  新增 `flow_connection_input_rules` 测试（GUI 24 项）。
- **SVG 跟随视图模式**：平面模式下导出改用**俯视投影**（`project_for_export`，绕开相机 yaw/pitch），
  导出图与 2D 画布完全一致；足迹尺寸按当前模式取（平面模式用各 kind 的平面尺寸，3D 用 38×18），
  平面模式下名称底板同样落在足迹中心，导出的图与屏幕看到的一致。
- **平面模式标签**：2D 模式下名称底板移到**足迹中心**（平面里没有“上方”可用），状态角标改为右上角**彩色小圆点**，
  9px 文字在小足迹上读不清的问题一并解决。
- **平面模式命中测试**：旋转后的平面足迹用**多边形命中**（`point_in_polygon`）代替轴对齐包围盒，
  斜放的对象不会再“点这块选中那块”；新增 `point_in_polygon_handles_rotated_footprints` 测试（GUI 23 项）。
- **平面模式支持旋转**：2D 模式下对象按其 `rot` 画成**旋转多边形**（此前被忽略），北向刻度只在未旋转时绘制
  （原判定写成了恒真，等于每个对象都画刻度）。
- **平面 2D 模式**：工具栏新增 `2D on/off` 切换——关闭高度的**俯视平面**渲染（圆角底板 + 深色描边 + 北向刻度），
  开启时自动把相机切到俯视角。此前只能在 3D 里转相机，看不到真正的平面布局。
- **形体区分**：队列改画成**带分隔线的托盘**、调度器为**带三条辐条的枢纽**、资源为**带四个刻度的圆站台**、
  电梯为**带轿厢的井道**，均带接触阴影；此前这四类都退化成同一个通用方块。
- **产物清单**：右侧新增 `Outputs (n)` 折叠区，列出本次会话写出的文件（文件名、大小 B/KiB/MiB、完整路径悬停、
  文件已删除时转暗），可 `Clear list`。报告 CSV/JSON、对比 CSV、时序 CSV、基线 JSON、trace JSONL、视图 SVG
  七条导出路径都接入 `remember_output()`。新增 `format_bytes_switches_units` 测试（GUI 22 项）。
- **优化字段选择器**：优化面板新增 `pick field`，列出模型里**所有数值叶子**（`objects/<n>/params/…`，忽略字符串与布尔），
  点击即填入最后一个变量的路径，不再需要手打斜杠路径。`numeric_paths()` + `numeric_paths_finds_every_number` 测试（GUI 21 项）。
- **校验问题可点击**：`validate_collect` 的每条问题都带定位，点击即选中对应对象（`objects.<name>.…` 解析为名称，
  索引式定位不误判）；截断时提示剩余条数，模型无问题时显示绿色 `model is valid`。
  新增 `issue_locations_map_to_object_names` 测试（GUI 20 项）。
- **实验面板控件**：`stop-time/parameter` 改分段按钮，`PATH/START/STEP/RUNS/REPS` 统一成大写标签；
  **补回丢失的 `REPS` 输入框**（字段与逻辑一直在，控件在某次重排中被挤掉，导致复现次数无法设置）。
- **流程块控件**：`+create/delay/assign/decision/transfer/sink` 改为**按块类型着色的胶囊按钮**（与画布同一套配色），
  块列表改为等宽胶囊并内嵌 `x`，`Apply process flow` 改主按钮。
- **内置文件浏览器**：工具栏新增 `Browse…`，打开一个原生窗口列出当前目录（文件夹在前、`.ofm` 在后，忽略隐藏与非模型文件），
  支持 `Up` / `Home` 与逐层进入，选中即加载。不引入 `rfd` 等新依赖（Linux 上需要 GTK，会让 CI 变复杂）。
  新增 `browser_lists_dirs_before_models` 测试（GUI 19 项），此前只能手输路径打开模型。
- **优化耗时**：最优解提示改为 `Best (in X.XXs): …`，与运行/实验的耗时口径一致。
- **实验耗时**：实验结束的状态栏改为 `Experiment finished in X.XXs: N runs (Y.YYs each)`，
  便于判断要不要继续加样本。
- **运行耗时**：`Run` 结束的状态栏改为 `Run finished in X.XXs: N completed, M objects · K items/s`，
  通道随之携带 `Instant::elapsed()`；此前只报完成数，看不出这次跑得快还是慢。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**534 passed / 0 failed**。
- GUI 单元测试 **26 项**（取景、BFS 布局、内置示例、字体路径、热力色、明暗、Pareto、事件格式、XML 转义、
  重命名引用、删除清理、trace 行格式、两处多边形命中、路径收集、字节格式等）。
- GUI `clippy` 0、`fmt` 干净；以 `mm1.ofm`、`routing.ofm`、`process-flow-transfer.ofm`、`agv-charging.ofm`、
  `engine-integration.ofm`、`elevator.ofm`、`conveyor.ofm`、中文模型与无参数共 9 种方式启动均无 panic。

## v1.25.0 — 2026-11-20

本轮把桌面 GUI 从「能看到」补到「能改、能存、不丢状态」：新增**对象重命名**、**安全删除**、**全局表预览**与**检查点删除**，并修掉一批会静默失败或残留状态的问题。
发布节奏按要求保持**至少 3 小时一版**。

### 桌面 GUI（功能补齐）

- **对象重命名**：属性面板头部名称**就地编辑**（Enter/失焦提交、Esc 还原）；`rename_object()` 经撤销栈、
  重名拒绝，`rewrite_name()` 改写全部引用（连接端点、`home`/`charger`/`network`、Process Flow 块），
  随后 `validate()`。新增 `rename_rewrites_every_reference` 测试。
- **安全删除**：新增 `delete_objects()`，删除对象时同步清理连接、`home`/`charger`/`dispatcher`/`*_operator`
  引用与 `network` 边；删除被引用对象不再因 `validate()` 失败而无反应，批量与右键删除共用该路径。
  新增 `delete_cleans_dangling_references` 测试。
- **全局表只读预览**：左侧 `Global tables (n)` 折叠区显示 `行数 × 列数`、列名胶囊与前 4 行值，
  超出提示剩余行数；空模型给出 `Tables live in the model JSON`。
- **检查点列表操作**：显示当前文件夹，检查点改为等宽胶囊按钮，新增 `x` 删除（带错误反馈）。
- **视觉状态重置**：`Reset` 与加载新模型时清空 `smooth` 插值缓存、`replay_index` 与事件日志。

### 桌面 GUI（外观）

- **形体光影**：对象描边改为深色细线（选中时 1.6px 暖色环），机械类与源/汇/车辆都加**接触阴影**，
  圆鼓高光收敛为小亮点，车辆圆角改为全圆角，形体与地面有了接触感。

- **选中态**：选中对象不再覆盖为固定琥珀色，而是把当前状态/热力色**提亮 35%**（`shade_light`），
  既看得出被选中，也保留状态含义；新增 `shade_light_keeps_a_selected_body_recognisable` 测试。
- **快捷键守卫补全**：`Ctrl/Cmd+S`、`O`、`N` 在文本框聚焦时不再触发；路径为空时 `Ctrl+O` 给出提示而不是静默失败。
- **Save 回退到另存为**：无路径的模型（`New`、内置示例）点 `Save` 会打开另存为对话框并预填 `<模型名>.ofm`，
  不再静默无响应；内置示例同时清空路径框并提示 `not saved yet`。
- **新建模型即脏**：`New` 生成的文档从未落盘，因此立即点亮 `*` 并提示 `New model — not saved yet`。
- **撤销/重做计入脏标记**：撤销或重做同样改变文档内容，会点亮品牌后的 `*`。
- **模型脏标记**：品牌名后出现琥珀 `*`（悬停「unsaved model changes」），由 `snapshot()` 置位、保存/加载清除；
  与表单级 `unsaved edits` 区分「模型已改」与「当前对象表单未应用」。
- **优化成本提示**：`Run` 旁显示 `~N runs` 预估评估次数（grid 按 `steps^变量数`、random 按样本数、genetic 按样本×步数）。
- **采样上限**：时序采样最多 20 000 点（0.1s 间隔 × 1e6 时长会生成千万行并卡住 UI），
  触顶时状态栏提示「raise the interval」。
- **未保存标记持久化**：`unsaved edits` 改为持久标志（`form_dirty`），Apply / Apply JSON / 切换选中才清除，
  不再只在一帧内闪烁。
- **参数 JSON 编辑器**：实时解析校验，非法时输入框描红并提示 `invalid JSON — check the brackets`，
  合法时显示 `valid JSON`；`Apply JSON` 改为主按钮且随有效性启用，输入框全宽。
- **错误清除**：报告 CSV / 对比 CSV / 时序 CSV 导出成功后清空遗留的错误横幅，避免“已成功但仍显示红条”。
- **错误横幅**：改为暗红底 + 暖色左轨 + 浅色文字的告警条，`Dismiss` 改为无底右手按钮，不再像多出来的一行工具栏。
- **基线按钮态**：`set`（需当前报告）、`save`（需已捕获基线）、`load`（需已保存模型路径）按可用性禁用，
  `load` 悬停说明读取 `<model>.baseline.json`。
- **导出按钮态**：`compare CSV` 在未捕获基线时显示为禁用，而不是直接消失（位置不再跳动）。
- **时序图坐标**：图高 120→150px，左上角显示最大值、底部左右显示首/末时间戳。
- **Pareto 散点**：四分之一参考线、被支配点先画（前沿压在上层）、前沿点加琥珀光晕环、
  左下/左上角加 `max stay ->` 与 `^ completed` 轴标注。
- **利用率柱状图**：圆角柱 + 50%/100% 参考线 + 柱顶百分比（宽度足够时），鼠标划过时描边并显示对象名。
- **对比表**：表头改暗色小字，数值等宽，delta 增加 `+/-/=` 方向符号（不只靠颜色区分），列名改为 before/after/delta。
- **面板标题**：改为「3px 靛蓝竖条 + 字距放开的 10px 大写标题」，比原来的灰色小字更像章节标题。
- **视图 SVG 导出**：改用与界面一致的径向渐变暗底、标题、双层连接线 + 箭头，对象名放进圆角底板并做 **XML 转义**
  （新增 `xml_escape_protects_svg_text` 测试）。
- **物品点**：圆点加接触阴影、尺寸微调；超过 8 件时以琥珀 `+N` 标注余量，满队列不再拖成一条线。
- **选区两级**：主选中提亮 35%、其余选中提亮 16%（都保留状态色），取代此前「主选中琥珀、其余压暗」的对照。
- **实时计数**：工具栏的 `t / created / completed` 改为「暗色微标签 + 等宽数值」三组，播放时数字不抖动。
- **后台任务指示**：状态栏在跑报告/实验/采样/优化时显示 8 点旋转弧，`ctx.request_repaint()` 保持动效；
  同时把 `opt_running`、`series_running` 纳入 100ms 重绘轮询，后台任务结束后界面立即回稳。
- **进度条**：加高到 10px、按状态着色（运行=靛蓝、完成=绿色），文字改小号等宽 `已完成 / 总时长`。
- **流程画布背景**：改为随平移/缩放移动的**点阵网格**，画布高度 190→210px，并用 `with_clip_rect` 裁剪，节点不会溢出到侧栏。
- **阴影常量**：接触阴影抽为 `palette::SHADOW`，机械/源汇/车辆/物品共用同一 alpha。
- **连接线**：改为「宽而淡的光晕 + 细核心线」双层描边，末端加小箭头指向下游对象，路由在网格上更清晰。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**525 passed / 0 failed**。
- GUI 单元测试共 **17 项**（取景、BFS 布局、内置示例、字体路径、热力色、明暗、Pareto、事件格式、XML 转义、
  重命名引用、删除清理等）。
- GUI `clippy` 0、`fmt` 干净；以 `mm1.ofm`、`routing.ofm`、`process-flow-transfer.ofm`、`agv-charging.ofm`、
  `engine-integration.ofm`、中文模型与无参数共 7 种方式启动均无 panic。

## v1.24.0 — 2026-11-20

本轮为 **GUI 观感** 专项：不新增功能，只把界面从「能用的调试面板」打磨到可以交付的水平。
发布节奏按要求放慢为**至少 3 小时一版**，本轮累计 30 项改动合并为一次发布。

### 主题与配色

- **调色板**：新增 `palette`（BG / PANEL / PANEL_HI / CARD / CANVAS / STROKE / TEXT / TEXT_DIM /
  ACCENT / ACCENT_HI / AMBER / OK / ST_*），所有面板、画布、图表与状态色都从这里取色。
- **圆角与描边**：控件 6px、卡片 8px、胶囊/键帽 11px；细描边、hover 高亮环、active 靛蓝填充、选中对比色。
- **分层背景**：内容区最深（BG）、画布近黑（CANVAS）、面板次深（PANEL）、控件浅（CARD）。
- **画布景深**：中心亮池（多层低透明度描边模拟暗角）+ 网格线随距原点距离渐隐，地平线不再抢主体。
- **类型配色**：`kind_color` 重排为均匀色相环（源绿 / 存储青 / 机械靛 / 搬运琥珀 / 流体蓝绿 / 其余石板灰），
  明度接近，没有哪类对象过分抢眼。
- **状态配色统一**：`palette::ST_BUSY / ST_BLOCKED / ST_IDLE / ST_DOWN / ST_OFF` 成为画布填充、
  图例与报告表的唯一来源。

### 字体与字形

- **中文字体**：`install_cjk_font()` 按 macOS / Linux / Windows 候选路径加载系统 CJK 字体作为**回退族**，
  中文对象名与消息不再显示方框；拉丁文仍用默认字体。
- **缺字形修复**：egui 默认字体没有 `✕ ● ○ ▸ → ↑↓`，界面曾出现方框乱码。改为 `x`、绘制圆点、`->`、
  `File >`、方向键英文名。

### 布局与取景

- **自动布局**：新增 `Auto layout` 按钮；加载时若所有对象重叠在同一点，按连接 BFS（源点优先、孤立对象收尾）
  自动排成方阵并取景，首次打开即清晰。
- **投影空间取景**：`frame_maths_view()` 在 yaw/pitch **投影后**计算包围盒并反解焦点，旋转后不再裁掉标签；
  4 单位 padding + 200px 边距，缩放钳制 0.15–2.5。
- **延迟取景**：取景请求在画布尺寸测量后才执行，避免首帧用错视口。
- **新建模型 / 打开模型**：均自动取景，起始流水线首屏居中可见。
- **面板宽度**：左 330px（可拖 260–560）、右 360px（300–620），参数表单不再被挤压。

### 画布与图表

- **对象标签**：圆角深色胶囊（细描边，选中转琥珀），在任何填充色上都清晰。
- **状态角标**：盒内右下角显示 `idle / busy / blocked / down / off`，颜色不再是唯一线索（兼顾色觉障碍）。
- **悬停提示**：鼠标停留弹出 `kind / content / state / util` 气泡。
- **Process Flow 节点**：按块类型着色、投影阴影、细描边、块类型角标，选中/待连接琥珀高亮；
  默认栅格改为每行 4 块（间距 140×64）。
- **时间序列图**：25/50/75% 参考线、线下面积填充、末端圆点 + 数值标签、底部 `对象 · max · 样本数`。
- **时间序列控件**：指标改分段按钮，对象选择改胶囊 chip。
- **热力图 / 图例**：图例自动换行、暗色文字。

### 面板与表格

- **面板标题**：大写字距小标题（`MODEL TREE` / `CONNECTIONS` / `REPORT` …）+ 细分隔线（并移除重复分隔线）。
- **底部状态栏**：运行状态圆点（绿=运行中、琥珀=后台任务、空心=空闲）+ 消息 + 右侧 `t=…` 与品牌标题。
- **工具栏**：按 `FILE` / `RUN` / `STATE` / `VIEW` 分组；`Run`、`Play` 改**主按钮**（琥珀底 + 深色字）。
- **模型树**：类型色圆点 + 名称左对齐 + 坐标/旋转右对齐等宽小字；头部过滤框加宽、`x` 清空、右侧对象总数。
- **新增对象行**：kind 下拉收窄、名称框 96px、`Add` 主按钮。
- **批量操作区**：大号琥珀计数 + 快捷键提示，对齐/分布/网格/复制/删除统一圆角按钮（删除暖红描边），
  `MOVE` / `ROT` 分行。
- **对象属性头部**：类型色圆点 + 大号名称 + 圆角 kind 标签；`x/y/z` 等宽轴标签 + 两位小数拖动框。
- **Apply 行**：主按钮 + 编辑后 `unsaved edits` 琥珀提示，`JSON` 开关右对齐。
- **模型设置面板**：`NAME / UNIT / STOP / WARMUP / SEED / REPS` 大写字距标签，`Apply settings` 主按钮。
- **分布编辑器**：标签暗色小字、下拉固定 118px，多行字段纵向对齐。
- **可选字段**：`optional_u32` / `optional_f64` 改为「复选框 + 同行数值框」，勾选时不再整表回流。
- **报告区**：顶部 2×2 **指标卡片**（COMPLETED 含 ±CI、CREATED、AVG WIP、AVG UTIL）；
  表格对象名前加 34×8 **四段状态条**；导出拆成 `EXPORT` / `BASELINE` 两行。
- **实验表**：等宽小字、最佳场景绿色加 `*`、CI 列暗色；优化模式改分段按钮。
- **优化结果表**：新增 `#` 名次列（第 1 名琥珀、Pareto 前沿绿色）。
- **空状态卡片**：报告 / 事件日志 / 检查点无数据时显示统一提示卡片。
- **对话框**：快捷键按 FILE/RUN/SELECT/VIEW/HELP 分组并用**等宽键帽**；另存为改为说明 + `PATH` 标签 +
  等宽全宽输入框 + 主按钮。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**521 passed / 0 failed**。
- GUI `clippy` 0、`fmt` 干净；以 `mm1.ofm`、`process-flow-transfer.ofm`、`agv-charging.ofm`、
  中文模型与无参数五种方式启动均无 panic。
- GUI 单元测试增至 **13 项**（新增视口取景、BFS 布局顺序、内置示例、字体路径、指标卡尺寸等）。

## v1.23.0 — 2026-11-19

### 桌面 GUI

- **悬停提示**：鼠标停在对象上弹出气泡，显示名称与 `kind / content / state / util`（无运行数据时显示 kind），
  与状态角标、标签描边互补，不必点开表单即可查看实时状态。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**518 passed / 0 failed**；clippy 0、fmt 干净、启动无 panic。

## v1.22.0 — 2026-11-19

### 桌面 GUI

- **可读性增强**：对象标签改为**深色描边 + 白色文字**，在热力图的红/黄填充上仍清晰；
  盒内右下角新增**状态角标**（`idle` / `busy` / `blocked` / `down` / `off`），颜色不再是唯一状态线索（对齐 FlexSim 的标签与状态显示）。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**518 passed / 0 failed**；clippy 0、fmt 干净、启动无 panic。

## v1.21.0 — 2026-11-19

### 桌面 GUI

- **Fit / Frame selection**：工具栏 `Fit` 按钮与右键菜单 `Frame selection` 按当前选中对象（无选中则全模型）的中点居中，
  并按跨度自动缩放（0.15–4×，12 单位跨度对应 1×）；逻辑抽为纯函数 `frame_maths` 并新增测试。
- GUI 测试增至 **10 项**。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**518 passed / 0 failed**；clippy 0、fmt 干净、启动无 panic。

## v1.20.0 — 2026-11-19

### 桌面 GUI

- **报告表扩展**：对象统计表新增 `Thru`（吞吐/时间单位）与 `±CI`（吞吐的 95% 半宽）两列；
  对象名悬停显示 `idle / busy / blocked / down` 四态占比，与 CLI 报告同口径。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**517 passed / 0 failed**；clippy 0、fmt 干净、启动无 panic。

## v1.19.0 — 2026-11-19

### 桌面 GUI

- **并行复现开关**：工具栏新增 `parallel` 复选框（默认开启）。`Run` 与实验扫描都用 `run_model_parallel(model, 0)`
  在多核上跑独立复现，结果与串行逐位一致；关闭后走 `run_model` 串行路径。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**517 passed / 0 failed**；clippy 0、fmt 干净、启动无 panic。

## v1.18.0 — 2026-11-19

### 桌面 GUI

- **报告 CSV 增强**：每个指标同时输出均值与 `ci95`（`input/output/avg_wip/avg_stay/throughput/utilization` 各带 `_ci95` 列），
  并新增 `idle/busy/blocked/down` 四列，与 CLI / Studio 表格口径一致。

### 环境

- 磁盘再次被 `target/debug/build`（80 GB）占满导致链接失败；清理后剩余 73 GB，重建与全量测试恢复。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**517 passed / 0 failed**；clippy 0、fmt 干净、启动无 panic。

## v1.17.0 — 2026-11-19

### 桌面 GUI

- **空状态引导**：未加载模型时显示 `New model` 与三个内置示例（`mm1` / `routing` / `batch-queue`）按钮；
  `load_example()` 先找 `examples/`（相对仓库与 bundle 的四种路径），找不到则用**编译进二进制**的内置模型，
  因此安装版没有示例目录也能立刻试用。
- 新增单元测试 `builtin_examples_parse_and_validate`（GUI 测试增至 9 项）。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**517 passed / 0 failed**；clippy 0、fmt 干净、启动无 panic。

### 发布修复

- Linux GUI job 因 `azure.archive.ubuntu.com` 暂时不可达导致 apt 失败，release 只发出 13 个资产；
  release workflow 的 apt 步骤改为 **4 次重试 + `Acquire::Retries=5`**，重跑该 job 后补齐
  `openflex-gui_1.17.0_amd64.deb` 与 `openflex-gui-1.17.0-1.x86_64.rpm`，并重算 `SHA256SUMS.txt`，14 项全部校验通过。

## v1.16.0 — 2026-11-19

### 桌面 GUI

- **班次日历编辑**：对象表单新增 `Timetable (JSON)` 折叠区，可直接编辑 `{"windows": [{start, duration, repeat_every}]}`，
  `null` 表示无班次限制；Apply 时解析为 `TimeTable` 参与校验。
- 模型层新增 `ObjectParams::timetable_mut()` / `set_timetable()`（fluid 类对象无日历，返回 `None`/`false`）。

### 文档

- `docs/UNIMPLEMENTED.md` 第 11 项更新：OnContentChange/OnStateChange/OnReset/OnStop/OnResume 均已实现且 GUI 可编辑；仅 OnDraw 为 schema-only。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**516 passed / 0 failed**；clippy 0、fmt 干净、启动无 panic。
- 发布前清理了 47 个临时校验目录（释放约 3 GB，此前磁盘满导致链接失败）。

## v1.15.0 — 2026-11-19

### 桌面 GUI

- **连接端口策略编辑**：连接列表每行新增策略下拉（`first` / `rr` / `random` / `hold`），改动经 `snapshot()` 入撤销栈与 `validate()`；
  列表标签同时显示类型前缀 `[A]`/`[S]` 与策略后缀。
- 新增 `set_connection_strategy()` 方法。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**516 passed / 0 failed**；clippy 0、fmt 干净、启动无 panic。

## v1.14.0 — 2026-11-19

### 桌面 GUI

- **连接类型 A/S**：新建连接的工具栏增加 `A`/`S` 下拉；`S` 写入 `ConnectionKind::S`（资源链路，不占输出端口、不参与物流路由），
  连接列表以 `[A]` / `[S]` 前缀显示，避免只有 A 连接可编辑的缺口。

### 文档

- `docs/UNIMPLEMENTED.md` 第 12 项更新：A/S 语义已双向可用，仅剩 Pull 的按端口条件策略。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**516 passed / 0 failed**；clippy 0、fmt 干净、启动无 panic。

## v1.13.0 — 2026-11-19

### 桌面 GUI

- **模型树增强**：每个对象行显示 `(x, y)` 坐标与旋转角（如 `45°`），同 kind 组内提供 `select all` 一键选中该组全部对象。

### 文档

- `docs/UNIMPLEMENTED.md` 第 29 项更新：模型树已具备分组/过滤/坐标与旋转显示/组内全选，仅缺多级父子嵌套。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**516 passed / 0 failed**；clippy 0、fmt 干净、启动无 panic。

## v1.12.0 — 2026-11-19

### 桌面 GUI

- **Process Flow 画布平移/缩放**：支持拖拽空白处平移、`Cmd/Ctrl + 滚轮` 缩放（0.4–2.5×）、
  `Reset view` 复位，画布右上角显示当前缩放；节点拖拽按缩放比例换算位移。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**516 passed / 0 failed**；clippy 0、fmt 干净、启动无 panic。
- v1.12.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v1.11.0 — 2026-11-19

### 桌面 GUI

- **播放速度滑块**：工具栏新增 `Speed`（0.25–8×，对数）。`Play` 时每帧推进 `step_size × speed` 的模型时间，
  帧间隔按速度调整（16–240 ms），与 Studio 的 animation speed 对齐；仿真本身仍按完整步长推进，不影响结果。

### 文档

- `docs/UNIMPLEMENTED.md` 第 18 项（交互式运行控制）标记完成：桌面 GUI 与 Studio 均有时钟速度、事件步进/跳转。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**516 passed / 0 failed**；clippy 0、fmt 干净、启动无 panic。

## v1.10.0 — 2026-11-19

### 示例与文档

- **新增 `examples/objects/person-reserve.ofm`**：两人同走廊、各自 `grid_step` + `grid_reserve: 2`，
  跟随者提前让行；`openflex run` 实测 6 件完成、两人都参与（Person1/2 各 in 3）。
- 新增回归测试 `crates/openflex-core/tests/person_reserve_example.rs`（示例可解析、可跑、不卡死）。
- `docs/UNIMPLEMENTED.md` 第 2、3 行按当前实现更新（Person：逐格 + 预约等待；AGV：多桩/排队策略/防死锁/network 行驶）。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**516 passed / 0 failed**；clippy 0、fmt 干净。

## v1.9.0 — 2026-11-19

### 桌面 GUI

- **最近文件持久化**：`Recent` 列表写入 `~/.config/openflex/recent.json`（尊重 `XDG_CONFIG_HOME`、Windows 用 `APPDATA`），
  启动时自动加载；菜单项显示文件名并对悬停显示完整路径，新增 `Clear recent`。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**515 passed / 0 failed**；clippy 0、fmt 干净。
- 实机启动后确认 `~/.config/openflex/recent.json` 已写入。
- v1.9.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v1.8.0 — 2026-11-19

### 桌面 GUI

- **深色主题**：启动时统一设置 panel/window/extreme 背景色，界面与画布配色协调。
- **应用图标**：程序内生成 64×64 RGBA 图标（深蓝底 + 浅色圆环），用作窗口图标，无需打包二进制资源；
  新增单元测试校验中心不透明、角落透明（GUI 测试增至 8 项）。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**515 passed / 0 failed**；clippy 0、fmt 干净、启动无 panic。
- v1.8.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v1.7.0 — 2026-11-19

### 桌面 GUI

- **视图导出 SVG**：工具栏 `Export view SVG` 以画布相同的投影（相机角度/缩放/旋转/热力色/连接线）生成
  `<model>.view.svg`，便于把当前视图贴进文档或提交到版本库。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**514 passed / 0 failed**；clippy 0、fmt 干净、启动无 panic。
- v1.7.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v1.6.0 — 2026-11-19

### 桌面 GUI

- **方向键驱动仿真**：`→` 按当前步长推进一次仿真；`↑`/`↓` 增减步长（0.1…1000）；输入框聚焦时自动让位。
  帮助对话框同步列出这两项。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**514 passed / 0 failed**；clippy 0、fmt 干净、启动无 panic。
- v1.6.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v1.5.0 — 2026-11-19

### 桌面 GUI

- **基线持久化**：报告面板新增 `Save baseline` / `Load baseline`，把基线写为 `<model>.baseline.json`
  （完整 `SimReport`），重开应用后可继续对比；与已有的 `Set baseline` / `Compare` / `Export compare CSV` 配合使用。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**514 passed / 0 failed**；clippy 0、fmt 干净、启动无 panic。
- v1.5.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v1.4.0 — 2026-11-19

### 桌面 GUI

- **实验多复现**：实验面板新增 `reps`（1…50），每个场景以 `run.replications = reps` 运行，
  结果表扩展为 `value / completed / ±CI95` 三列（含置信区间），柱状图仍按均值对比。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**514 passed / 0 failed**；clippy 0、fmt 干净、启动无 panic。
- v1.4.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v1.3.0 — 2026-11-19

### 桌面 GUI

- **全选与复制选中**：`Ctrl/Cmd+A` 选中全部对象；`Ctrl/Cmd+D` 或批量面板的 `Duplicate` 复制所选对象
  （唯一 `_2/_3` 命名、位置 +2、参数深拷贝、不复制连接），入撤销栈并经校验。
- **快捷键避让输入框**：在文本输入/代码编辑器内输入时，全局快捷键（撤销/重做/播放/重置/F1/全选/复制）不再触发。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**514 passed / 0 failed**；clippy 0、fmt 干净、启动无 panic。
- v1.3.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v1.2.0 — 2026-11-19

### 桌面 GUI

- **命名布局预设**：左侧 `Layout presets (n)` 面板可保存当前对象布局为命名预设、点击名称一键应用（入撤销栈）、`✕` 删除；
  预设持久化在模型旁的 `<model>.layout.json` 边车文件，随模型目录一起管理。

### 验证

- `cargo test --workspace --locked`（含 GUI）：**514 passed / 0 failed**；GUI clippy 0、fmt 干净。
- v1.2.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v1.1.0 — 2026-11-19

### 桌面 GUI

- **单元测试**：为 GUI 增加 7 项 `#[cfg(test)]` 测试（`set_json_number` 合法/非法路径、`parse_params` 变体、`heat_colour` 色带与夹紧、
  `shade` 明暗、`pareto_front` 非支配点、`EventEntry` 日志格式）。
- **修复**：`set_json_number` 只改写数值/空值叶子，避免优化器路径误覆盖对象或数组。
- **CI**：`gui (ubuntu-22.04)` job 增加 `cargo test -p openflex-gui` 步骤。

### 验证

- `cargo test --workspace --locked`（**含 GUI**）：**514 passed / 0 failed**。
- GUI clippy 0、fmt 干净；v1.1.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v1.00.0 — 2026-11-19

### 桌面 GUI

- **检查点列表**：右侧 `Checkpoints (n)` 折叠面板列出目录内全部 `*.json` 及其模型时间（读取 `now` 字段），
  点击任一项即按名恢复该检查点；`Refresh list` 重新扫描，保存后自动刷新。

### 验证

- 工作区 **507 passed / 0 failed**；GUI 编译、clippy 0、fmt 干净、启动无 panic。
- v1.00.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.99.0 — 2026-11-19

### 桌面 GUI

- **检查点保存/恢复**：工具栏 `Save checkpoint` 把当前 `SimulationHandle::snapshot()` 写入
  `<checkpoint dir>/checkpoint-<time>.json`；`Load checkpoint` 读取该目录最新文件，
  经 `Checkpoint::from_json_verified`（SHA-256 自校验）后恢复到新句柄并刷新视图。

### 验证

- 工作区 **507 passed / 0 failed**；GUI 编译、clippy 0、fmt 干净、启动无 panic。
- v0.99.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.98.0 — 2026-11-19

### 桌面 GUI

- **模型设置面板**：左侧新增 `Model settings`——模型名、时间单位（seconds/minutes/hours/days）、
  `run.stop_time` / `warmup` / `seed` / `replications`，`Apply settings` 经 `validate()` 写回并重置句柄与报告；
  加载/新建模型时字段自动同步。

### 验证

- 工作区 **507 passed / 0 failed**；GUI 编译、clippy 0、fmt 干净、启动无 panic。
- v0.98.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.97.0 — 2026-11-19

### 桌面 GUI

- **触发器编辑**：每个对象的表单底部新增 `Triggers (JSON)` 折叠区，可编辑 `on_create/on_entry/on_exit/on_receive/on_message/
  on_breakdown/on_repair/on_photo_eye/on_content_change/on_state_change` 等全部触发器；Apply 时解析为 `Triggers` 并参与校验。
- 模型层新增 `ObjectParams::triggers_mut()`（与既有 `triggers()` 对应），供 GUI 写回所有 kind 的共享触发器。

### 验证

- 工作区 **507 passed / 0 failed**（model 与 core 零回归）；GUI 编译、clippy 0、fmt 干净、启动无 panic。
- v0.97.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.96.0 — 2026-11-19

### 桌面 GUI

- **指标热力图**：工具栏 `Heatmap` 下拉（off / utilization / throughput / WIP），运行后按报告值给对象着色
  （utilization 直接 0..1，其余按当前最大值归一），使用绿→黄→红三段插值；画布左下角显示色带与当前指标。

### 验证

- 工作区 **507 passed / 0 failed**；GUI 编译、clippy 0、fmt 干净、启动无 panic。
- v0.96.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.95.0 — 2026-11-19

### 桌面 GUI

- **批量分布**：`Distribute X/Y` 把选中对象按坐标排序后，在首末之间等距分布（≥3 个对象）。
- **网格排布**：`Grid 4` 以选中首个对象为原点，按 `√n` 列、4 单位间距把选择排成方形网格。
- 两者都经撤销栈与 `validate()`，失败时回滚并提示。

### 验证

- 工作区 **507 passed / 0 failed**；GUI 编译、clippy 0、fmt 干净、启动无 panic。
- v0.95.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.94.0 — 2026-11-19

### 桌面 GUI

- **实时校验**：对象参数表单下方调用 `validate_collect` 显示当前模型的问题列表（`location: message`，最多 20 条），
  Apply 之前即可发现错误。
- **数值范围约束**：`queue.max_content`（1…1e6）、`conveyor.speed`（0…1000）、`conveyor.length`（0…1e5）加上滑框范围，
  避免意外输入负数或极端值。

### 验证

- 工作区 **507 passed / 0 failed**；GUI 编译、clippy 0、fmt 干净、启动无 panic。
- v0.94.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.93.0 — 2026-11-19

### 桌面 GUI

- **枚举型参数改为下拉**：新增通用 `enum_picker` 辅助；
  - `rack`：rack type（floor/gravity/push_back/drive_in）与 pick policy（fifo/level_order/nearest）；
  - `queue`：order（fifo/lifo/priority；`by_label` 仍走 JSON）；
  - `fluid_valve`：open curve（linear/equal_percentage）。

### 验证

- 工作区 **507 passed / 0 failed**；GUI 编译、clippy 0、fmt 干净、启动无 panic。
- v0.93.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.92.0 — 2026-11-19

### 桌面 GUI / Studio 回放打磨

- **桌面 GUI**：时间序列图上用琥珀竖线标出当前 `Replay` 采样位置，与对象 `[n]` 标签联动。
- **浏览器 Studio**：trace 列表行可点击（`data-trace-jump`），点击即把时间轴滑块移到该事件时间并刷新 nearest 提示。

### 验证

- `node --check` 通过；工作区 **507 passed / 0 failed**；GUI/CLI clippy 0、fmt 干净。
- v0.92.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.91.0 — 2026-11-19

### 浏览器 Studio

- **快捷键帮助面板**：工具栏 `?` 按钮弹出面板，列出拖拽旋转/滚轮缩放/点击与 Shift 多选/框选/方向键微调/
  撤销重做/保存/播放暂停/删除（`data-studio-help`、`data-help-panel` 标记，测试已覆盖）。

### 验证

- `node --check` 通过；CLI 集成测试（含标记断言）与工作区 **507 passed / 0 failed**；`fmt` 干净。
- v0.91.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.90.0 — 2026-11-19

### 引擎（Fluid）

- **管满背压**（task-153）：新增 `pipe_volume()`（`π/4·d²·length`）；pipe 的通过量上限为
  `volume / residence × dt`（residence 取 `delay` 或 `length/speed`），**流率模式与压差模式都生效**；
  未给 `diameter` 的 pipe 仍视为无容量上限，既有示例与测试数值不变（零回归）。
- 新增测试 `crates/openflex-core/tests/pipe_full.rs`（2 项）：无直径 → ≈100/s 直通；`d=0.1, length=10, delay=30`
  → 通过量被限制在约 2.6/s 量级。

### 验证

- `cargo test --workspace --locked --exclude openflex-gui`：**507 passed / 0 failed**。
- 4 个 fluid 示例（line/pipe/pressure/backpressure）数值与改动前一致；`fluid_safety_net` 3 项通过。
- core `clippy` 0、`fmt` 干净；v0.90.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.89.0 — 2026-11-19

### 浏览器 Studio

- **复制对象**：对象操作区新增 `Duplicate`——深拷贝参数、自动生成唯一名（`<name>_2`、`_3`…）、位置偏移 `+2`，
  经 `POST /api/model` 保存（服务端校验）后刷新视图、选中副本并写入操作历史。

### 验证

- `node --check`（Studio 脚本）通过；CLI 26+86 测试全绿；工作区 505 项测试通过；`fmt` 干净。
- v0.89.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.88.0 — 2026-11-19

### 桌面 GUI

- **事件时间跳转**：事件日志改为结构化条目（`time/kind/object`）；点击任一行触发 `jump_to_event`——
  重建 `SimulationHandle` 并 `step_until(该事件时间)`，视图与统计随即回到那一刻，状态栏显示 `Jumped to t=…`。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 115）；GUI 编译、clippy 0、启动无 panic。
- v0.88.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.87.0 — 2026-11-19

### 引擎（Person）

- **多格预约**（task-152）：`TransporterParams.grid_reserve`（默认 0）指定逐格移动的人**提前预约前方 N 格**；
  预约格存在 `ObjectRt.person_reserved`，其他人在判断阻挡时同时检查"站位"与"预约"，因此跟随者会提前让行而不是撞到下一格。
- 预约窗口在每次移动后与路径建立时刷新、路径走完即清空；`grid_reserve = 0` 时行为与之前完全一致（零回归）。
- 新增单元测试 `person_reservations_block_followers_early`：前车预约第 2 格 → 后车停在原格；释放预约后继续前进。

### 验证

- `cargo test --workspace --locked --exclude openflex-gui`：**505 passed**（0 failed）。
- core/model `clippy` 0、`fmt` 干净；v0.87.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.86.0 — 2026-11-19

### 桌面 GUI

- **逐事件步进**：新增 `Step event` 按钮（`SimulationHandle::next_event` + `step_event`），每次只推进一个事件；
  右侧新增**事件日志**折叠面板，显示最近 200 条 `t / event / object`（等宽小字、自动滚底），`Reset` 时清空。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 115）；GUI 编译、clippy 0、启动无 panic。
- v0.86.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.85.0 — 2026-11-19

### 引擎（AGV）

- **节点等待防死锁**（task-151）：AGV 在目标节点被**索引更小**的 AGV 占用时等待（0.5 重试、trace `agv_wait`），
  连续 3 轮后**升级放行**（不再等待、计数清零），保证环形/相向场景不会永久挂起；等待计数存入
  `ObjectRt.agv_wait_rounds`（checkpoint 兼容）。
- 新增单元测试 `agv_node_wait_escalates_instead_of_blocking_forever`：首轮计数 +1，达到阈值后计数清零并继续。

### 验证

- `cargo test --workspace --locked --exclude openflex-gui`：**504 passed**（30 个 test 结果行）。
- core `clippy` 0、`fmt` 干净；v0.85.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.84.0 — 2026-11-19

### 引擎（Person）

- **逐格返回 home**（task-150）：`grid_step` 的人完成服务后按 A* 路径**逐格走回 home**，走完才 `settle_task`；
  `ObjectRt.person_returning` 标记回程、复用逐格占用等待；无 grid 或无 home 时保持一次性 travel（零回归）。
- 新增单元测试 `person_grid_step_walks_home_cell_by_cell`：释放后进入回程、路径非空、逐事件走完后位置回到 home 且任务结算。

### 验证

- `cargo test --workspace --locked --exclude openflex-gui`：**497 passed**（30 个 test 结果行）。
- core `clippy` 0、`fmt` 干净；v0.84.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.83.0 — 2026-11-19

### 引擎（AGV）

- **充电排队策略**（task-149）：新增 `AgvParams.charge_queue_order`（`fifo`（默认）/ `lowest_battery` / `longest_waiting`）；
  `ObjectRt.charge_queue_since` 记录开始等待的时刻；槽位释放时由**释放者**的策略决定弹出哪台 AGV。
- 新增单元测试 `charger_queue_orders_by_policy`：lowest_battery 先服务电量最低者，FIFO 按到达顺序。

### 验证

- `cargo test --workspace --locked --exclude openflex-gui`：**496 passed**（30 个 test 结果行）。
- core/model `clippy` 0、`fmt` 干净；v0.83.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.82.0 — 2026-11-19

### 桌面 GUI

- **新增对象**：模型树上方新增 kind 下拉（覆盖全部 28 种对象）+ 可选名字 + `Add`；
  自动编号（`processor1`…）、source 使用安全的 5s 固定间隔默认参数、位置自动排在现有最右侧对象之后、入撤销栈并经 `validate()`。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 115）；GUI 编译、clippy 0、启动无 panic。
- v0.82.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.81.0 — 2026-11-19

### 引擎（AGV）

- **多充电桩选择**（task-148）：`AgvParams.chargers: Vec<String>` 与 `charger` 一起构成候选集合；
  `charger_slot` 优先选**有空位且最近**的桩（同等空闲时比距离），全部占满才回到最近的桩排队；
  单桩模型行为不变（零回归）。
- 新增单元测试 `agv_prefers_a_free_charger_over_a_busy_one`：两桩都空闲 → 选最近；C1 占用 → 选 C2；两桩都满 → 回落到最近并排队。
- Fluid：为 tank 出流与密度/压头关系补注释（`p = ρ·g·h` 已在压力场生效；释放量仍按液位）。

### 验证

- `cargo test --workspace --locked --exclude openflex-gui`：**495 passed**（30 个 test 结果行）。
- core/model `clippy` 0、`fmt` 干净；v0.81.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.80.0 — 2026-11-19

### 引擎（Fluid）

- **阀门开度曲线接线**（task-86）：新增 `valve_effective_fraction(ratio, opening, curve)`——
  `linear` 按开度线性缩放，`equal_percentage` 用标准 `R^(x-1)`（R = 50）归一化，二者与 `ratio` 相乘并夹紧到 0..1；
  阀门传输量由该有效开度决定。
- 新增集成测试 `crates/openflex-core/tests/valve_opening.rs`（4 项）：
  全开 ≈10/s、线性 50% ≈5/s、等百分比 50% ≈1.4/s、同开度下等百分比低于线性。

### 验证

- `cargo test --workspace --locked --exclude openflex-gui`：**494 passed**（30 个 test 结果行）。
- core `clippy` 0、`fmt` 干净；v0.80.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.79.0 — 2026-11-19

### 编辑体验

- **桌面 GUI 批量旋转**：多选面板新增 `rotate to <deg>°` + Apply，把统一角度写入所选对象的 `Object.rot`
  （0° 清空字段），经撤销栈与校验；选择面板对单个对象也可见。
- **Studio 批量旋转**：`Batch edit` 面板新增 `rot°`，`Apply position/Z/rot` 一次写入全部选中对象。

### 验证

- `node --check`（Studio 脚本）通过；工作区 490 项测试全绿（CLI 26+86）；fmt 干净。
- v0.79.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.78.0 — 2026-11-19

### 渲染一致性

- **Studio WebGL 3D 支持 `Object.rot`**：立方体角点在物体空间旋转后再平移，与 2D/Canvas 3D 路径一致。
- **等轴测 SVG 支持旋转**：`openflex layout --svg`（isometric）直接旋转 footprint 角点，
  `--html` 交互预览（含帧动画）继承同一份 SVG。

### 验证

- `node --check`（Studio 脚本）通过；工作区 490 项测试全绿；CLI clippy 0、fmt 干净。
- 实测 `rot: 30` 模型：平面 SVG 输出 `rotate(30.0 …)`，`--html` 含 4 处 `rotate(`。
- v0.78.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.77.0 — 2026-11-19

### 浏览器 Studio

- **读取/编辑 `Object.rot`**：对象编辑面板新增 `rot°` 输入（0–360，0 时删除字段），`liveIdentity` / `applyEdit` 同步写入。
- **视图支持旋转**：2D 俯视用 `ctx.rotate` 围绕对象中心旋转矩形；3D Canvas 在投影前把本地角点按角度旋转
  （`lx*cos - ly*sin` 等），与桌面 GUI 的写法一致。

### 验证

- `node --check`（提取的脚本）通过；CLI 26+86 测试全绿；工作区 490 项测试通过；clippy/fmt 干净。
- v0.77.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.76.0 — 2026-11-19

### 桌面 GUI / CLI

- **旋转写入模型**：GUI 右键菜单的 `-15° / +15° / 0°` 现在写 `Object.rot`（归一化到 `[0,360)`，0° 写回 `None`），
  经撤销栈与 `validate()` 后保存进 `.ofm`——旋转成为模型的一部分而非会话状态。
- **SVG 布局导出带旋转**：`openflex layout --svg` 的对象组用 `<g transform="rotate(deg cx cy)">` 包裹节点与端口，
  文字标签保持水平可读。实测 `rot: 30` 的模型输出 `rotate(30.0 ...)`。

### 验证

- `cargo test --workspace --locked --exclude openflex-gui`：**490 passed**（CLI 26+86、core 255、report 14、model 115）。
- GUI 与 CLI clippy 0、fmt 干净；v0.76.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.75.0 — 2026-11-19

### 桌面 GUI

- **对象旋转**：右键对象菜单提供 `-15° / +15° / 0°`，对象以旋转后的 footprint 绘制；
  新增 `draw_rotated_box()`——按旋转角画四边形，并把前/右侧面向下挤出（明暗区分），使平面画布上有立体感。
- **形状统一**：机台、传送带（带 `→` 方向箭头）、货架、默认盒体都走同一套旋转+挤出绘制。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；GUI 编译、clippy 0、启动无 panic。
- v0.75.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.74.0 — 2026-11-19

### 桌面 GUI

- **框选多选**：在 3D 视图空白处拖出矩形选择对象（`Shift` 追加），选中对象统一高亮、状态栏报告数量。
- **批量操作**：`Align X` / `Align Y`（以首个选中对象为基准）、`Delete`（删除并清理连接）、按 `dx/dy` 批量移动、`Clear`；
  所有批量操作走 `snapshot()` 撤销栈 + `validate()` 校验。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；GUI 编译、clippy 0、启动无 panic。
- v0.74.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.73.0 — 2026-11-19

### 桌面 GUI

- **运行结果对比**：`Set baseline` 保存当前报告为基线；`Compare` 切换并排对比表（每个对象的 Util base / Util now / Δ，
  改善绿色、变差红色）；`Export compare CSV` 输出 `object,metric,baseline,current,delta,delta_percent`
  （含 `<all>` completed 行与每对象 input/output/wip/stay/utilization）。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；GUI 编译、clippy 0、启动无 panic。
- v0.73.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.72.0 — 2026-11-19

### 桌面 GUI

- **快捷键**：`Ctrl/Cmd+O` 打开、`Ctrl/Cmd+N` 新建、`Ctrl/Cmd+S` 保存、`Ctrl/Cmd+Z` / `Ctrl/Cmd+Shift+Z` 撤销/重做、
  `Space` 播放/暂停、`R` 重置、`F1` 打开快捷键帮助对话框（工具栏也有 `Help`）。
- **对象右键菜单**：3D 视图中右键对象弹出操作面板——Focus camera（相机居中到该对象）、Reset position（z 归零）、
  Delete object（删除对象并清理其连接，校验通过才提交）。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；GUI 编译、clippy 0、启动无 panic。
- v0.72.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.71.0 — 2026-11-18

### 桌面 GUI 安装包

- **macOS Universal `.app`**：`gui (macos-latest)` 现在同时构建 `aarch64-apple-darwin` 与 `x86_64-apple-darwin`
  的 `openflex-gui`，用 `lipo` 合并为 universal 可执行文件再打进 `OpenFlex.app`
  （产物名 `OpenFlex-<ver>-universal-apple-darwin.app.zip`），Apple Silicon 与 Intel Mac 均可直接运行。

### 验证

- 本地以 `--target universal-apple-darwin` 跑打包脚本，产出 `OpenFlex-0.70.0-universal-apple-darwin.app.zip`，
  `plutil -lint Info.plist` 通过；工作流 YAML 由 ruby 解析通过。
- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）。

## v0.70.0 — 2026-11-18

### 桌面 GUI

- **报告表扩展**：逐对象显示 `In / Out / WIP / Stay / Util` 六列。
- **报告导出**：`Export report CSV`（object/kind/input/output/avg_wip/max_wip/avg_stay/throughput/utilization）与
  `Export report JSON`（完整 `SimReport`）。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；GUI 编译、clippy 0、启动无 panic。
- v0.70.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.69.0 — 2026-11-18

### 桌面 GUI

- **Pareto 前沿**：勾选第二目标（max stay）后，优化结果下方绘制散点图（x = max stay、y = completed），
  非支配解用琥珀色高亮并统计数量（`Pareto front: n of m points`）。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；GUI 编译、clippy 0、启动无 panic。
- v0.69.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.68.0 — 2026-11-18

### 桌面 GUI

- **按 kind 的 3D 几何体与明暗**：processor/multiprocessor/combiner/separator/merge/split 画成带顶板与指示灯的机台；
  conveyor/escalator 为带侧轨的长条；rack 为分层货架；source/sink 为圆鼓；person/transporter/agv/robot/crane/asrs 为胶囊；
  fluid 系列为带液位的罐体；其余为带上沿高光的盒体。新增 `shade()` 实现伪光照（左上光源）。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；GUI 编译、clippy 0、启动无 panic。
- v0.68.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.67.0 — 2026-11-18

### 桌面 GUI

- **多变量优化**：变量列表（`+ variable` / `✕`），每个变量一套 JSON path + `[low, high]`；grid 为笛卡尔积（>400 点拒绝并提示），
  random 与 genetic 在多维盒内采样/进化。
- **双目标结果表**：可选「2nd: max stay」，结果表同时显示 `completed` 与 `max avg stay`，最佳点按主目标选取。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；GUI 编译、clippy 0、启动无 panic。
- v0.67.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.66.0 — 2026-11-18

### 桌面 GUI

- **优化器扩展**：`Optimise` 面板新增 `grid / random / genetic` 三种搜索模式（单变量，目标为 `completed` 最大）：
  - random：`samples` 次均匀采样（LCG 可复现）；
  - genetic：种群 12、6 代，elitist 保留 + 均匀交叉 + 变异；
  - 结果按目标降序取 top 12，显示最佳点。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；GUI 编译、clippy 0、启动无 panic。
- v0.66.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.65.0 — 2026-11-18

### 桌面 GUI

- **吞吐/利用率曲线**：时间序列采样同时记录 `content / throughput / utilization`，图表可在三者间切换。
- **时间序列 CSV 导出**：`Export CSV` 写出 `time,object,content,throughput,utilization`（文件名 `<model>.timeseries.csv`）。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；GUI 编译、clippy 0、启动无 panic。
- v0.65.0 三平台产物（含 GUI 安装包）发布并 SHA256 校验。

## v0.64.0 — 2026-11-18

### 桌面 GUI

- **参数表单覆盖剩余对象 kind**：rack（capacity/levels/channels/depth/aisles）、elevator（speed/stop time/door open-close/
  max load/load per task/shaft）、dispatcher（max tasks/preempt retry/delay）、basic_te（speed/capacity）、
  fluid_generator（flow rate/stop time）、fluid_tank（capacity/initial level/max outflow/density/gravity）、
  fluid_valve（open/ratio/pressure threshold/opening）、fluid_pipe（length/speed/flow rate/temperature）；
  basic_fr / sink / fluid_sink 显示 process flow/timetable/emptier/triggers 状态并提示走 JSON。
  枚举型字段（rack_type/pick_policy）仍通过 JSON 编辑。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；GUI 编译、clippy 0、启动无 panic。
- v0.64.0 三平台产物发布并 SHA256 校验。

## v0.63.0 — 2026-11-17

### 桌面 GUI 安装包（三平台）

- 新增 `gui (ubuntu-22.04 / windows-2022 / macos-latest)` CI job，随发布产出：
  - Windows **MSI**（`packaging/windows/openflex-gui.wxs`：per-user 安装、开始菜单与桌面快捷方式、PATH）；
  - macOS **`.app.zip`**（复用 `scripts/package-gui-app.sh`）；
  - Linux **deb/rpm**（`packaging/gui/nfpm-gui-{deb,rpm}.yaml` + `build-gui-packages.sh`，声明 libxkbcommon/libX11/libGL 依赖）。
- GUI 任务 `continue-on-error`，不会阻塞 CLI 发布；文档 `docs/GUI.md` 增加三平台安装包说明与本地打包命令。

### 验证

- `bash -n`、`xmllint`（WiX）、`ruby -YAML`（工作流）全部通过；本地 `.app` 打包与启动验证。
- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）。

## v0.62.0 — 2026-11-17

### 桌面 GUI

- **时间轴回放**：时间序列采样后出现 `Replay t` 滑块，拖动可在采样点上回放各对象 WIP（3D 视图标签与 item 点随之变化），
  并可一键回到 `live`。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；GUI 编译、clippy 0、启动无 panic。
- v0.62.0 三平台产物发布并 SHA256 校验。

## v0.61.0 — 2026-11-17

### 桌面 GUI

- **Grid search 优化面板**：指定 JSON path 与 `[low, high]/steps`，后台逐点运行，显示最优值（`completed` 最大）
  与完整 `param → completed` 表。

### 发布

- 修复 `deb/rpm (arm64)` 因 GitHub API 限流（下载 nfpm 403）导致的发布失败：`release.yml` 固定 nfpm 2.38.0；
  v0.60.0 重跑后 11 产物齐全并 SHA256 校验通过。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；GUI 编译、clippy 0、启动无 panic。
- v0.61.0 三平台产物发布并 SHA256 校验。

## v0.60.0 — 2026-11-17

### 桌面 GUI

- **时间序列图**：`Sample` 按可配 interval 用 `SimulationHandle` 逐段推进采样，多对象切换、折线绘制、max 标注。
- **状态饼图**：汇总所有对象 `state_times`（idle/busy/blocked/down/off-shift），画扇区并标注百分比。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；GUI 编译、clippy 0、启动无 panic。
- v0.60.0 三平台产物发布并 SHA256 校验。

## v0.59.0 — 2026-11-17

### 桌面 GUI

- **Process Flow 可视化画布**：块渲染为圆角矩形（默认 3 列网格布局），**拖拽移动节点**、**点击两个节点建立连接**
  （自动写入 flow `connections` 并刷新）、选中/待连接高亮、连接线绘制。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；GUI 编译、clippy 0、启动无 panic。
- v0.59.0 三平台产物发布并 SHA256 校验。

## v0.58.0 — 2026-11-17

### 桌面 GUI

- **macOS `.app` 打包**：新增 `scripts/package-gui-app.sh`——release 构建 `openflex-gui`、组装 `OpenFlex.app`
  （`Info.plist` 含 `.ofm` 文档类型、`LSMinimumSystemVersion 11.0`、高分屏）、ad-hoc 签名、输出 `OpenFlex-<ver>-<triple>.app.zip`；
  本地实测 `open dist/gui/OpenFlex.app` 可启动。

### 验证

- `bash -n` + `plutil -lint Info.plist` 通过；`.app` 启动进程确认。
- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；v0.58.0 三平台产物发布。

## v0.57.0 — 2026-11-17

### 桌面 GUI

- **Process Flow 面板**：块 JSON 编辑、`+create/delay/assign/decision/transfer/sink` 快速加块（自动生成 id 与默认参数）、
  块列表删除（连带清理相关连接）、from/to 连接、`Apply process flow` 合并回对象参数并走完整校验。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；GUI 编译、clippy 0、启动无 panic。
- v0.57.0 三平台产物发布并 SHA256 校验。

## v0.56.0 — 2026-11-17

### 桌面 GUI

- **撤销/重做**：每个编辑（表单/JSON/连接增删）前保存快照，栈上限 50；工具栏按钮 + `Ctrl+Z` / `Ctrl+Shift+Z`。
- **文件操作**：`New`（source→queue→processor→sink 模板）、`Save as…` 对话框、`Recent` 最近 8 个模型、`Ctrl+S` 保存。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；GUI 编译、clippy 0、启动无 panic。
- v0.56.0 三平台产物发布并 SHA256 校验。

## v0.55.0 — 2026-11-17

### 桌面 GUI

- **平滑动画**：执行器位置在帧间指数缓动（引擎只在事件边界更新位置）。
- **参数扫描实验**：实验面板新增 `parameter` 模式，按 JSON path（如 `objects/0/params/process_time/value`）
  扫描数值并逐次运行，柱状图 + 值/completed 表。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；GUI 编译、clippy 0、启动无 panic。
- v0.55.0 三平台产物发布并 SHA256 校验。

## v0.54.0 — 2026-11-17

### 桌面 GUI

- **item 圆点**：对象上方按瞬时 `content` 画小圆点（最多 8 个）。
- **执行器实时位置**：transporter/person/AGV/robot/crane/asrs/elevator 按引擎 `position`/`z` 绘制（运行时随任务移动）。
- **实验面板**：stop-time 扫描（起点/步长/次数），后台逐次 `run_model`，柱状图 + `stop → completed` 表。

### 引擎

- `ObjectStats` 新增 `position: Option<[f64; 2]>` 与 `z: f64`（`#[serde(default)]`），`stats()`/`stats_at()` 双路径填充；
  测试 `object_stats_expose_the_live_position`。

### 验证

- 490 个测试全绿（core 255、CLI 86、report 14、model 全部）；GUI 编译、clippy 0。
- v0.54.0 三平台产物发布并 SHA256 校验。

## v0.53.0 — 2026-11-17

### 桌面 GUI

- **实时可视化**：对象按引擎瞬时 `state` 着色（busy 红 / blocked 橙 / idle 绿 / down 灰 / off-shift 深灰），
  标签显示瞬时持有件数 `[n]`（回退到 ≈平均 WIP）、顶部进度条（`t/stop`）、状态图例。

### 引擎

- `ObjectStats` 新增 `content: u32`（瞬时持有件数）、`state: Option<State>`、`phase: Option<Phase>`
  （`#[serde(default)]`，`state/phase` 仅 `Some` 时序列化）；`State`/`Phase` 改为 `pub` + snake_case；
  `stats()` 与 `stats_at()` 双路径填充。测试 `object_stats_expose_live_content_and_state`。

### 验证

- 489 个测试全绿（core 254、CLI 86、report 14、model 全部）；GUI 编译、clippy 0、启动无 panic。
- v0.53.0 三平台产物发布并 SHA256 校验。

## v0.52.0 — 2026-11-17

### 桌面 GUI

- **按 kind 的参数表单**：Distribution 类型下拉（constant/uniform/normal/exponential/triangular/gamma/weibull/lognormal/erlang/beta/
  integer_uniform/pareto/rayleigh/poisson/bernoulli）+ 数值控件；覆盖 source（interarrival/items/arrivals）、queue（max content/batch）、
  processor（setup/process time/operators）、conveyor（speed/length/accumulating/spacing）、resource（capacity）、
  transporter/person/robot/crane/asrs（capacity/speed/load/unload/home）、agv（charge time/battery/charger）；其余 kind 走 JSON 高级模式。
- **3D 增强**：地面网格、对象地面阴影、相机预设 Iso/Top/Front/Side。

### 验证

- GUI 编译通过、clippy 0、fmt 干净、启动无 panic。
- 488 个测试全绿（core 253、CLI 86、report 14、model 全部）；v0.52.0 三平台产物发布。

## v0.51.0 — 2026-11-17

### 桌面 GUI

- **模型树**：按 kind 分组 + 名称/kind 过滤；点击选中。
- **参数编辑**：选中对象可编辑 `params` JSON 并 Apply（按 kind 反序列化 + 全模型校验后替换）。
- **连接编辑**：列表/删除、from-to-port 添加（校验后原子更新）。
- **运行控制**：`Step` 按步推进、`Play`/`Pause`/`Resume`（`SimulationHandle`）、`Reset`、实时 `t`/created/completed；
  `Run` 仍在后台线程跑完整复现。
- **报告**：Completed ± CI、Created、利用率柱状图、逐对象 In/Out/Util。
- 新增 `docs/GUI.md`（能力矩阵、与 FlexSim 差距、路线图）。

### AGV（端到端已通）

- 修复诊断测试模型里的 `process_operator` 拼写（对象名不匹配 → 无操作员约束 → AGV 从不派任务）；
  端到端测试改为断言 `output_count > 0`、`charging_fraction > 0`、`battery_remaining <= capacity`，**去掉 `#[ignore]`**。
- 新增 `examples/objects/agv-charging.ofm`：实测 `--trace <FILE>` 得 4×`agv_charge_travel`、3×`agv_charge_start`、
  1×`agv_charge_wait`（排队）、2×`agv_charged`；`Agv1` battery −30.0、charging_fraction 0.225。

### 验证

- 488 个测试全绿（core 253 **0 ignored**、CLI 86、report 14、model 全部）；GUI 编译通过、clippy 0、fmt 干净。
- v0.51.0 三平台产物发布并 SHA256 校验。

## v0.50.0 — 2026-11-17

### 原生桌面 GUI（新 crate `openflex-gui`）

- eframe/egui 桌面应用：打开/保存 `.ofm` 模型、对象列表与 pos/z 编辑、透视 3D 布局视图
  （拖拽旋转、滚轮缩放、点击选中）、Run（后台线程 `run_model`）与逐对象统计表、错误/状态提示。
- 复用 model/core crate，不复制模拟逻辑；`cargo build -p openflex-gui --release` 后运行。
- CI 的 test/clippy/coverage 排除该 crate（Linux 桌面依赖），release 仍只打包 CLI（GUI 从源码构建，
  docs/install.md 已说明）。

### 验证

- 489 个测试全绿（core 253、CLI 86、report 14、model 全部；GUI 编译通过、无 warning）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.50.0 三平台产物发布并 SHA256 校验。

## v0.49.0 — 2026-11-17

### AGV（定位与修复）

- **修复无限充电循环**：`agv_mid_task_charge` 在“满电仍不足以覆盖本段”（`capacity < consumption × distance`）时
  不再重充，而是扣到 0、减少 `task_distance` 后继续任务（此前 charge→resume→charge 死循环，AGV `output_count=0`）。
- **新增逐步诊断助手** `agv_task_chain_diagnosis`（`#[ignore]`，`--ignored --nocapture` 打印事件与 AGV/processor 状态）。
- **记录未修根因**：`transport_arrive` 后 `requesting_operator` 已为 false → 不授予操作员 → processor 挂 Idle、AGV `tasks` 永为 1；
  已写入 UNIMPLEMENTED 并附复现命令。CLI `--trace` 用法仍未确认（`run --trace` 被拒）。

### 验证

- 488 个测试全绿（core 253 + 2 ignored 诊断；CLI 86、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.49.0 三平台产物发布并 SHA256 校验。

## v0.48.0 — 2026-11-17

### Studio

- **命名布局预设**：保存当前对象 pos 布局为命名预设（localStorage，最多 10 个）、下拉一键应用（入 undo/历史）、删除。

### 已知问题（诚实记录）

- 新增 `#[ignore]` 诊断测试 `agv_battery_drains_on_a_real_task`：真实模型中 AGV `output_count == 0`、
  `battery` 不下降 → `finish_task → drain_agv_battery → ChargeTravel` 链未被触发。
  **AGV 充电/行驶能力目前仅引擎级单测覆盖**；根因在 AGV 任务链（dispatch→Pickup→Arrive→Loaded→处理→Released→Home），
  已记录到 UNIMPLEMENTED 并附 4 步定位方案。

### 验证

- 488 个测试全绿（core 253，其中 1 个 ignored 诊断测试；CLI 86、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.48.0 三平台产物发布并 SHA256 校验。

## v0.47.0 — 2026-11-17

### AGV

- **前往充电桩 network 行驶**：有 `home` 时用 `transport_object_distance(home, charger)`（network 最短路优先、无 network 回退欧氏），
  距离存 `charge_trip_distance`；到达时按 `consumption_per_distance × 距离` 扣电（允许负值）后进入充电（沿用容量排队）。
  无 `home` 保持直线欧氏（旧行为）。测试：network 有向边 30、speed 2 → `ChargeTravel` 时间 **15.0**；到达 `battery == Some(-30.0)`。

### Studio

- **时间序列多对象叠加**：选择多个对象绘制不同颜色折线 + 图例 + hover 值；支持导出时间序列 CSV。

### 发布

- 确认 v0.47 发布前 GHCR latest 指向 0.46.0（404 for 0.47），性能基线沿用 v0.46（0.80/2.56s）。

### 验证

- 487 个测试全绿（core 252、CLI 86、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.47.0 三平台产物发布并 SHA256 校验。

## v0.46.0 — 2026-11-16

### AGV

- **前往充电桩行驶时间**：`drain_agv_battery` 在配了 `charger` 时计算当前位置→charger 直线距离 ÷ speed，
  调度 `EventKind::ChargeTravel`（trace `charge_travel`）；到站后调用 `start_charging`（沿用容量排队）；未配 charger 零回归。
- 测试：AGV `[0,0]`、charger `[10,0]`、speed 2、电量 0 → `ChargeTravel` 时间恰为 **5.0**，推进后 `charge_started_at == 5.0`。

### Studio

- **操作历史面板**：记录最近 50 次操作（编辑/拖拽/布局/对齐/增删/元数据/连接/导入）的时间与描述；
  点击历史行可恢复到该步快照（清空 undo/redo 栈）。

### 验证

- 486 个测试全绿（core 251、CLI 86、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.46.0 三平台产物发布并 SHA256 校验。

## v0.45.0 — 2026-11-16

### AGV

- **电池/充电报告字段**：`ObjectStats.battery_remaining: Option<f64>`（非 AGV 省略）与 `charging_fraction: f64`
  （已完成充电时长/统计窗口）；`start_charging` 记录真正开始时刻（排队不计）、`ChargeDone` 累计；
  `stats()`/`stats_at()` 双路径一致。Studio 状态面板自动读取（不再 n/a）。
- 前往充电桩行驶时间与 `agv-charging.ofm` 示例留待下一批。

### Studio

- **检查点删除/重命名**：`DELETE /api/checkpoints/<file>` 与 `POST /api/checkpoint-rename`（安全 basename、
  拒绝路径穿越/覆盖），面板每行 Rename/Delete 并刷新。

### 性能

- v0.44 基线补测：100k validate **0.78s** / run **2.41s** / **62,153 events/s**（load 3.4–3.7），与 v0.43 一致、无回归。

### 验证

- 485 个测试全绿（core 250、CLI 86、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.45.0 三平台产物发布并 SHA256 校验。

## v0.44.0 — 2026-11-16

### Person

- **逐格占用等待（接线 B）**：`handle_person_step` pop 下一格前检查是否被索引更小的 person 占用（tasks>0、距离<0.5）→
  trace `person_wait`、`now+0.5` 重试、不 pop；小 id 先行避免死锁。测试：两人同路径逐格跟随时后者等待、释放后进入、
  路径 4→3；开放网格总时间仍 == `grid_distance/speed`；默认关闭零回归。

### Studio

- **连接编辑器**：Connections 面板列出 from/to/port/strategy，支持添加/删除，保存走 `POST /api/model` 并刷新。

### 性能基线

- **挂账清零**：v0.33–v0.43 共 11 版逐版补测（每版 3 次中位数）。v0.43 正式基线（load 3.3–3.7）：
  100k validate **0.77s** / run **2.41s** / **62,120 events/s**；v0.36–v0.43 与 v0.30 差 5–8% 且随 load 单调 → 无回归；
  相比 v0.11.0 快约 **110×**。performance.md 新增 §3.0 逐版本基线表，删除“待补测”挂账。

### 验证

- 483 个测试全绿（core 249、CLI 85、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.44.0 三平台产物发布并 SHA256 校验。

## v0.43.0 — 2026-11-16

### Person

- **逐格移动（`grid_step`）**：`TransporterParams.grid_step == Some(true)` 且模型有 grid 时，pickup 段用 `grid_path`
  拆成逐格 `PersonStep` 事件（每步 `cell_size/speed × density`），队列走完后吸附到 requester 精确坐标并复用原 arrive 逻辑；
  trace `person_step`。默认关闭零行为变化（单测覆盖 4 格拆分与默认单段）。
- 简化：仅 pickup 段逐格、无逐格占用等待（接线 B）、跳过 dwell/load 时间。

### Studio

- **模型元数据编辑**：name/description/time_unit/run.stop_time/warmup/seed/replications 表单，保存走 `POST /api/model` 并刷新。

### 验证

- 482 个测试全绿（core 248、CLI 85、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.43.0 三平台产物发布并 SHA256 校验。

## v0.42.0 — 2026-11-16

### Studio

- **模型校验面板**：`Validate model` 调用 `/api/validate`，显示通过/错误列表；模型树对报错对象加红色标记。

### Person（准备）

- `TransporterParams.grid_step: Option<bool>`（默认 None，零行为变化），为逐格移动接线准备；
  `grid_path()` 已就绪，引擎侧 `PersonStep` 事件/路径队列待下一批（已拆 A/B 两步）。

### 发布文档

- 补记 GHCR 0.40.0（`e72290052fa5`，32 分钟）与 0.41.0（`f03dc8595851`，19 分钟）；0.40 run success。
- 性能待测清单扩到 v0.33–v0.41（跨 10 小时无 load<3 窗口）。

### 验证

- 481 个测试全绿（core 247、CLI 84、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.42.0 三平台产物发布并 SHA256 校验。

## v0.41.0 — 2026-11-16

### Person

- **`grid_path()` 路径回溯**：复用 `grid_distance` 的 8 邻接代价模型 + predecessor 回溯，返回含起点终点的格子序列；
  越界/障碍/不可达返回 None；`grid_distance` 未改动。单测覆盖直线（长度与距离一致）、绕障、不可达。
- 发现：绕障时 `grid_path` 加权代价可小于 `grid_distance`（tie-breaking 差异），路径合法且不劣——接线前记录。

### Studio

- **视图状态持久化**：相机/视图模式/网格/热力图指标/吸附网格存入 localStorage，刷新后恢复；`Reset view state` 清除。

### 验证

- 480 个测试全绿（core 247、CLI 84、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.41.0 三平台产物发布并 SHA256 校验。

## v0.40.0 — 2026-11-16

### Studio

- **trace 列表分页**：每页 100 条 + Prev/Next/页码（`N / M (total events)`），避免 50000 事件卡顿；
  时间轴 nearest 事件在当前页内高亮。
- **对比 CSV 导出**：Compare 面板 `Export comparison CSV`（object,metric,a,b,delta,delta_percent）。

### Person（评估，未实现）

- 逐格移动前置能力评估：需新增 `grid_path()`（A* 返回格子序列）+ `PersonStep` 事件 + 默认关闭开关，
  约 120–150 行；已记录到 UNIMPLEMENTED，下一轮先做 `grid_path` + 单测。

### 验证

- 478 个测试全绿（core 246、CLI 84、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.40.0 三平台产物发布并 SHA256 校验。

## v0.39.0 — 2026-11-16

### Person

- **pickup 起点格等待（途中避让最小版）**：开始 pickup 段前若共享起点格被索引更小的 person 占用（tasks>0、距离<0.5）
  → 不下格、trace `person_wait`、`now+0.5` 重试；与目标格等待共用“小 id 先行”，前后跟随不死锁。

### Studio

- **报告对比面板**：载入两份 report JSON（或当前报告 + 文件），按对象显示 input/output/utilization 差异
  （delta 与百分比、正负着色）。

### 验证

- 478 个测试全绿（core 246、CLI 84、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.39.0 三平台产物发布并 SHA256 校验。

## v0.38.0 — 2026-11-16

### Person

- **目标格占用等待（最小版）**：Person 到达目标格时若被索引更小的 person 占用（tasks>0 且距离<0.5）→ 不进入、
  trace `person_wait`、`now+0.5` 重试；小 id 先行打破相向死锁；测试验证等待后最终完成。
- 保留 8 邻接 A* 与密度折减；非 Person 执行器不受影响。

### Studio

- **trace 回放**：`POST /api/trace {stop_time,max_events}` 返回 `simulate_traced` 事件；前端 `Load trace`、
  按对象/时间过滤事件列表，拖动时间轴显示该时刻最近事件并高亮对应行。

### 验证

- 477 个测试全绿（core 245、CLI 84、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.38.0 三平台产物发布并 SHA256 校验。

## v0.37.0 — 2026-11-16

### AGV

- **充电桩容量排队**：`charger` 指向 resource（读 `capacity`，默认 1）；有空位则占用并充电，否则 FIFO 排队
  （`charging=true`、不接任务、`charge_until=INFINITY`），`ChargeDone` 释放槽位并唤醒队首；
  未配 `charger` 时原“原地充电”路径逐位不变；trace `agv_charge_start`/`agv_charge_wait`。
- 单元测试 `agv_charger_slots_queue` 精确验证两阶段（入队/接替）。

### Studio

- **报告导出**：`Export report JSON` / `Export report CSV`（当前 `/api/run` 报告）；对象状态面板增加 AGV 充电状态（无数据显示 n/a）。

### 验证

- 475 个测试全绿（core 244、CLI 83、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.37.0 三平台产物发布并 SHA256 校验。

## v0.36.0 — 2026-11-16

### 对象

- **电梯门态统计**：`StateTimes` 新增 `door_opening`/`door_closing`（`#[serde(default)]`），在
  `handle_transport_arrive`/`handle_operator_released` 累计；`stats()` 与 `stats_at()` 双路径一致；
  确定性测试断言 opening/closing 各 2.0（1e-9）。
- **报告显示**：table/markdown/json 的电梯对象行输出 `door_opening`/`door_closing`（`door_open` 预留）。

### 验证

- 473 个测试全绿（core 243、CLI 83、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.36.0 三平台产物发布并 SHA256 校验。

## v0.35.0 — 2026-11-16

### Studio

- **指标热力图**：工具栏选择 Utilization（默认）/Throughput/WIP，报告产生后 3D（Canvas/WebGL）与 2D 对象按绿→黄→红插值着色；
  右下角图例显示当前指标与 min/max。

### 对象现状勘察（下轮落地）

- Person：已有 8 邻接/密度折减，缺“同格/相邻等待”（需让行事件与死锁避免）；
- AGV：`charger` 目前只是标签（无容量排队），可复用 resource 的 capacity/requests；
- 电梯：门开/关仅用于延迟，`StateTimes` 缺 opening/open/closing 累计（3 字段 + 2 处累计点）。

### 验证

- 473 个测试全绿（core 242、CLI 83、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.35.0 三平台产物发布并 SHA256 校验。

## v0.34.0 — 2026-11-16

### Studio

- **布局 JSON 导入/导出**：`Export layout`（name/kind/pos 快照）与 `Import layout`（按 name 仅应用 pos、入 undo 栈）。
- **事件 detail 透传**：确认 core `EventPreview.detail` 经 `/api/step-event` 返回并显示在事件列表/CSV（测试断言非空 detail）。

### Fluid

- 诊断确认：pressure-mode 下 pipe 有实际流量（tank 释放主导），`fluid-darcy` 场景 throughput 比 ≈2 而非 √2——
  Darcy capacity 在示例中未成为瓶颈；已在 UNIMPLEMENTED 记录，构造 pipe 为瓶颈的模型后再补 √2 测试。

### 验证

- 474 个测试全绿（core 242、CLI 83、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.34.0 三平台产物发布并 SHA256 校验。

## v0.33.0 — 2026-11-16

### Studio

- **事件列表导出 CSV**（time/seq/kind/object/detail）、**检查点列表导出 JSON**、**3D 视角预设**（等距/俯视/前视/侧视）。

### 文档

- 性能噪声结论落定：v0.31/v0.32 的 10–18% 偏差经 load 5.3 下 10k 复测（67,032 events/s，与 v0.30 的 75,366 同属波动）
  确认为测量噪声，**不是代码回归**；performance.md 与 VERIFICATION 已去掉“待确认”措辞。
- Fluid：确认 pressure-mode 已有 tank→pipe 压力传播（`ρgh`），但 **volume 传递/pipe acceptance 仍走旧逻辑**，
  导致 Darcy 场景 pipe throughput=0——已记录为阻塞点与下轮方案（`acceptance = darcy Q × dt`）。

### 验证

- 473 个测试全绿（core 242、CLI 83、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.33.0 三平台产物发布并 SHA256 校验。

## v0.32.0 — 2026-11-15

### Fluid

- 统一 `pipe_capacity(pipe, delta_p: Option<f64>, fallback)`：有压差且参数齐全时走 Darcy（`Q=√(Δp/R)` + ρ(T)），
  否则线性 fallback（逐位不变）；`fluid_advance` pressure-mode 分支改为调用该辅助。
- 明确 pressure-mode 开启条件（tank.density/gravity、valve.pressure_open/close、pipe.temperature）。
- 待接线：valve `opening/open_curve`、tank 按当前密度、`fluid-darcy.ofm` 与 √2 集成测试、链路压力传播。

### Studio

- **WebGL PNG 截图**（`preserveDrawingBuffer`/重绘截取）与**场景动画时间轴**（Run 后拖动时间滑块近似更新）。

### 验证

- 473 个测试全绿（core 242、CLI 83、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.32.0 三平台产物发布并 SHA256 校验。

## v0.31.0 — 2026-11-15

### Fluid

- **Darcy 接线**：pressure-mode 的 pipe 流量在 `diameter+friction_factor+density` 齐全时按
  `R=f·L/(D·2ρA²)`、`Q=√(Δp/R)` 计算（含 ρ(T) 线性修正）；缺参数回退原线性标定（`fluid_safety_net` 3 项固定默认行为）。
- distribute cap 与 valve opening 开度曲线留待下一批（代码已注释 TODO 位置）。

### Studio

- WebGL 标签**深度遮挡**（被更近对象包围盒遮挡时隐藏）、连接端点圆点、选中琥珀描边（关闭深度测试），
  与 Canvas fallback 一致。

### 发布

- container run 经验值补全：v0.24–v0.30 实测 tag → 镜像可用 **25–35 分钟**，全部 success；
  `in_progress` ≠ 失败，超 60 分钟才需重跑（已写入 install/releasing/VERIFICATION）。

### 验证

- 472 个测试全绿（core 241、CLI 83、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.31.0 三平台产物发布并 SHA256 校验。

## v0.30.0 — 2026-11-15

### Fluid

- **Darcy 可选参数入模型**（零行为变化，安全网固定默认值）：`FluidPipeParams.diameter/friction_factor/density/
  temperature_coefficient`、`FluidValveParams.opening/open_curve`（linear/equal_percentage）。
- 新增 `core/tests/fluid_safety_net.rs`（3 项）：默认参数下 fluid-line 速率钉死（10/5/2.5/2.5）、两次运行逐位相等。
- engine 侧 Darcy 分支待接线（已定位三处接入点并给出实现草案）。

### Studio

- **WebGL 文字标签**：DOM overlay 随相机投影显示对象名/kind；WebGL 回退路径与实体箭头（若完成）。

### 验证

- 470 个测试全绿（core 240、CLI 83、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.30.0 三平台产物发布并 SHA256 校验。

## v0.29.0 — 2026-11-15

### Studio

- **WebGL 3D 渲染（最小版）**：新增 WebGL canvas（`data-webgl`），手写 shader 绘制对象盒体与连接线，
  透视相机与现有 yaw/pitch/zoom 一致；视图切换 3D (WebGL) / 3D (Canvas) / 2D，WebGL 不可用时自动回退并提示。
- 交互保持旋转/缩放/拖拽/双击聚焦/多选高亮。

### 发布

- GHCR 0.28.0 已确认发布（`dea3c81c6a5f`，发布后约 30 分钟），latest 前移；container run 仍卡在 push 后的
  收尾步骤（镜像已可拉取），文档记录处理建议。

### 验证

- 468 个测试全绿（core 237、CLI 83、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.29.0 三平台产物发布并 SHA256 校验。

## v0.28.0 — 2026-11-15

### Studio

- 检查点面板显示 SHA-256（前 12 位）并提供 **Verify**（服务端 `POST /api/verify-checkpoint` 用
  `Checkpoint::from_json_verified` 校验，篡改返回 422）。
- 3D/2D 左下角 **X/Y/Z 坐标轴**、**网格开关**（默认开）、右下角显示选中对象坐标 X/Y/Z。

### 测试基础设施

- 新增 `core/tests/task_sequence_timeline.rs`：锁定“旧短格式任务序列 = 无 task_sequence”逐位等价，
  并记录含额外段时的当前行为快照（为后续事件链改造提供安全网）。
- `docs/UNIMPLEMENTED.md` 明确标注任务序列“额外段在收尾阶段调度”为已知简化。

### 验证

- 468 个测试全绿（core 237、CLI 83、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.28.0 三平台产物发布并 SHA256 校验。

## v0.27.0 — 2026-11-15

### 检查点

- **校验和验证入口**：`Checkpoint::from_json_verified(text, expected_sha256)`（大小写不敏感，不匹配返回明确错误，
  `None` 等价原解析）+ `Checkpoint::sha256_of(text)`，与 manifest 的 SHA-256 对接。

### Studio

- **3D 框选**（Marquee 3D 开关或 Shift/Ctrl+空白拖框，按屏幕包围盒中心命中）、**多选整体拖拽**（按相同增量移动、
  吸附作用于锚点）、**批量编辑**（X/Y/Z 统一设置，入 undo 栈）。

### 验证

- 466 个测试全绿（core 235、CLI 83、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.27.0 三平台产物发布并 SHA256 校验。

## v0.26.0 — 2026-11-15

### 检查点完整性

- **manifest SHA-256**：新增无依赖 `core/src/sha256.rs`（FIPS 180-4，含标准向量测试），每个 checkpoint
  entry 记录文件 SHA-256（串行写盘后计算、并行主线程读回计算）；旧 manifest 兼容（字段可选）。

### Studio

- **多选与批量操作**：Ctrl/Shift 点击与框选、选中集合高亮；对齐/分布/布局作用于选中集合（无选中时作用于全部）；
  批量删除/复制。

### 验证

- 462 个测试全绿（core 234、CLI 83、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.26.0 三平台产物发布并 SHA256 校验。

## v0.25.0 — 2026-11-15

### 对象

- **conveyor 严格最小间距**：`Belt::compress()` 在每次推进前强制相邻 item ≥ spacing（过近项被压到间距网格，
  不重叠），配合最小约束步长实现“前车不动后车被夹住、前车让位后跟进”；行程时间/吞吐不受影响。

### Studio

- **2D 俯视视图**（按钮 + 快捷键 V），与 3D 共享 pos 编辑；**网格吸附**（10/20/50）、
  **对齐**（左/右/上/下）与**等距分布**（水平/垂直）工具，均可撤销。

### 验证

- 460 个测试全绿（core 231、CLI 83、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.25.0 三平台产物发布并 SHA256 校验。

## v0.24.0 — 2026-11-15

### 调度与检查点

- **任务序列中途事件化**：超出标准 4 段的额外段通过 `EventKind::TaskStepDone` 逐段独立调度
  （`pending_steps`，`describe_event` 加 `task_step_done`，可用 step_event 预览）；旧短格式行为逐位不变。
- **manifest history 上限**：合并时滚动保留最近 50 条。

### Studio

- 3D 深度遮挡（画家算法 + 屏幕包围盒）+ 标签避让（被近处对象遮挡时隐藏、标签冲突上移）；
  新增 **Export SVG**（当前视图静态矢量）与 **Export model JSON**（内存模型，未保存也可导出）。

### 验证

- 460 个测试全绿（core 229、CLI 83、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.24.0 三平台产物发布并 SHA256 校验。

## v0.23.0 — 2026-11-15

### 运行与调度

- **并行检查点**：`run_model_with_checkpoints_parallel(model, dir, stop_time, workers)`（每复现写 checkpoint，
  workers=0 自适应，结果与串行逐项一致）。
- **抢占恢复延迟**：`DispatcherParams.preempt_retry_delay` / `preempt_max_retries`；修复“victim 已不持有执行器导致同刻
  无限抢占”的死循环（`preempt_epoch` 去重 + 清理 stale 任务）。

### Studio

- 对象拖拽移动（屏幕反投影到 pos）、自动布局（grid/circle/force）、方向键微调（Shift 精细）、可撤销。

### 验证

- 455 个测试全绿（core 226、CLI 83、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.23.0 三平台产物发布并 SHA256 校验。

## v0.22.0 — 2026-11-15

### 任务与并行

- **完整任务序列**：`TaskStep` 支持短格式与 `{"kind","duration","condition"}`，条件分支与每段自定义时长；
  优先级队列与抢占恢复沿用 A10；示例 `task-sequence-advanced.ofm`。
- **worker 自适应**：`run_model_parallel(model, 0)` 按 `available_parallelism`，结果与串行逐位一致。
- **manifest 合并**：多次运行按 index 合并 entries 并记录 `history`（时间戳/stop_time/复现数）。

### Studio

- 透视 3D（近大远小 + 深度排序 + 双击聚焦/复位）、OnDraw 徽标与“未执行（仅 schema）”提示、右键菜单隐藏修复。

### 发布

- 新增 `scripts/release.sh`：5 步固定流程（预检→bump→sync→verify→提示 tag/push），支持 `--dry-run`/`--phase`；
  `docs/releasing.md` 以其为标准发版流程。GHCR 0.21.0 与 latest 已确认（`826f4abbdba1`）。

### 验证

- 455 个测试全绿（core 219、CLI 83、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.22.0 三平台产物发布并 SHA256 校验。

## v0.21.0 — 2026-11-15

### 运行与对象

- **检查点目录管理**：`run_model_with_checkpoints_retained`（`manifest.json` + `keep=Some(n)` 保留最新 n 个），
  CLI `--keep-checkpoints N`，Studio 检查点面板（列表/恢复）。
- **多复现并行**：`run_model_parallel(model, workers)`（线程按复现下标分区，报告顺序确定，与串行逐位一致）。
- **conveyor 精确回退**：`Belt::advance()` 最小约束步长迭代，dwell 到期/阻塞解除后在同一 dt 剩余时间继续移动
  （旧实现整步消耗）；测试 [10, 9] 跟进。

### 验证

- 450 个测试全绿（core 218、CLI 83、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.21.0 三平台产物发布并 SHA256 校验。

## v0.20.0 — 2026-11-15

### 运行与对象

- **多复现检查点**：core `run_model_with_checkpoints`（每复现一个 checkpoint）+ CLI `run --save-checkpoints <dir>`；
  每个 checkpoint 可 resume 且与直接运行逐位一致。
- **逐段变速 conveyor**：`segments` 按段积分（跨段携带剩余时间），photo eye/station/出口时间按段速度；
  测试证明非等效单速（10@1+10@2 到中点 10s、全程 15s）。
- **A* 路径缓存**：`(执行器, from, to)` 距离缓存，restore 时清空；结果不变。

### 统计

- `run --batch-means-objects`：每对象 input/output/avg_wip/utilization 的批均值与 t 区间（与系统级并列）。

### 工程

- CI 全局 `CARGO_INCREMENTAL=0` + coverage 磁盘日志（避免 runner 磁盘满）；GHCR 0.19.0 已发布（latest=0.19.0）。

### 验证

- 439 个测试全绿（core 211、CLI 81、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.20.0 三平台产物发布并 SHA256 校验。

## v0.19.0 — 2026-11-15

### 对象与路径

- **Conveyor 分段**：`segments: [{length, speed, angle?}]` 按段累加行程时间（10@1 + 10@2 = 15），空数组保持旧行为。
- **AGV/robot 路径规划**：AGV/ASRS 走 network 最短路（支持有向边）；robot 有 grid 时 8 邻接绕障（无则 3D 欧氏）。
- **EventPreview.detail**：`step_event`/`next_event` 返回 `name=value` 细节，Studio 事件列表展示。

### 统计

- **批均值**：`run --batch-means [N]` 与 `report --batch-means`，系统级 Items created/completed/in system 的
  每批均值、批间方差与 t 区间（table/markdown/json 并列复现 CI）；batches≥2 且 ≤复现数。

### 验证

- 430 个测试全绿（core 191、CLI 79、report 14、model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.19.0 三平台产物发布并 SHA256 校验。

## v0.18.0 — 2026-11-15

### 逐事件单步

- `SimulationHandle::step_event()`（每次恰好一个事件，返回 `EventPreview{time,seq,kind,object}`）与 `next_event()` 预览；
  `(time, seq)` 严格单调。修复 `step_event` 的统计漂移（`stats_at` 只读快照、去掉中途 finalize、phase 用 phase_start），
  逐事件步进与一次性运行**逐位一致**。
- Studio 新增 Step Event、Preview next event、事件列表（当前高亮，最多 200 条）。

### 对象

- 新增 `robot`（3D 欧氏含 z）、`crane`（max(垂直,水平)）、`asrs_vehicle`（network）、`escalator`（复用 conveyor 皮带）；
  参数复用 `TransporterParams`/`ConveyorParams`；示例 `robot.ofm`/`crane.ofm`/`asrs-vehicle.ofm`/`escalator.ofm`。
- schema 同步 4 个新 kind（共 28 个）。

### 验证

- 424 个测试全绿（core 含 handle 9、engine_integration 24；CLI 78；model 全部）。
- 黄金案例零回归；clippy 0；fmt 干净；v0.18.0 三平台产物发布并 SHA256 校验。

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
