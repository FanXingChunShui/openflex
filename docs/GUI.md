# 桌面 GUI（`openflex-gui`）

原生桌面窗口（eframe/egui），与浏览器版 `openflex studio` 共用同一套 model/core crate。
本页记录当前能力、与 FlexSim 桌面界面的差距，以及后续补齐计划。

```sh
cargo build -p openflex-gui --release
./target/release/openflex-gui examples/mm1.ofm   # 也可以不带参数启动后在窗口内填路径
```

## 当前能力（v1.33.0）

| 区域 | 功能 |
|---|---|
| 文件 | **New**（生成 source→queue→processor→sink 模板）、Open（路径输入）、Save、**Save as…**、**Recent 最近 8 个（持久化到 `~/.config/openflex/recent.json`，含 hover 全路径与 Clear recent）**、加载时校验 |
| 文件浏览 | 工具栏 `Browse…` 打开内置目录浏览器（文件夹在前、`.ofm` 在后，支持 `Up`/`Home`，选中即加载），不依赖 GTK |
| 产物清单 | 右侧 `Outputs (n)` 列出本会话写出的文件（名称、大小、完整路径悬停），`Clear list` 清空 |
| 快捷键 | `Ctrl/Cmd+A` 全选、`Ctrl/Cmd+D` 复制选中（自动 `_2` 命名 + 偏移、复制后选中副本）、`Ctrl/Cmd+O` 打开、`Ctrl/Cmd+N` 新建、`Ctrl/Cmd+S` 保存、`Ctrl/Cmd+Z` 撤销、`Ctrl/Cmd+Shift+Z` 重做、`Ctrl/Cmd+2` 切换平面 2D、`F` 取景选区、`Space` 播放/暂停、`R` 重置、`Delete`/`Backspace` 删除选中（≥3 个需按两次）、`F1` 帮助、右方向键步进、上/下方向键调整步长；`Ctrl/Cmd+点击` 在画布上追加选择 |
| Studio 帮助 | 工具栏 `?` 按钮弹出快捷键面板（拖拽/滚轮/框选/方向键微调/撤销/保存/播放/删除，`data-help-panel`） |
| Studio 复制 | 浏览器 Studio 对象操作区新增 `Duplicate`（深拷贝参数、自动 `_2` 后缀命名、位置偏移 +2，经 `/api/model` 保存并入历史） |
| 重命名 | 属性面板头部名称可就地编辑（Enter/失焦提交、Esc 还原），自动改写全部引用（连接、home/charger/network、流程块） |
| 对象菜单 | 3D 视图**右键对象**：Focus camera（居中）、Reset position（z 归零）、**旋转 -15°/+15°/0°**（写 `Object.rot`，入撤销栈）、Delete object（删除并清理连接），均走校验 |
| 布局预设 | 左侧 `Layout presets (n)`：输入名字 `Save` 保存当前所有对象位置，点击名称即应用（入撤销栈），`x` 删除；存于模型旁的 `<model>.layout.json` |
| 视图导出 | 工具栏 `Export view SVG` 用与画布相同的投影（含旋转、热力色）输出 `<model>.view.svg`，可直接进文档或版本库 |
| 模型设置 | 左侧 `Model settings` 折叠区：name、time_unit（seconds/minutes/hours/days）、run.stop_time/warmup/seed/replications，`Apply settings` 经校验写回并重置当前运行 |
| 空状态引导 | 未加载模型时给出 `New model` 与三个内置示例（`mm1`/`routing`/`batch-queue`）一键打开；内置示例编译进二进制，无 `examples/` 目录时也可用 |
| 模型树 | **新增对象**（kind 下拉 38 种 + 可选名字，自动编号与安全默认参数，右侧自动摆放）、按 kind 分组（可折叠）、名称/kind 过滤、每行显示 `(x, y)` 与旋转角、组内 `select all`、点击选中 |
| 多选与批量 | 3D 视图**拖框多选**（Shift 追加），选中集合高亮；批量 **Align X/Y**（以首个为基准）、**Distribute X/Y**（首末之间等距分布，≥3 个）、**Grid 4**（方形网格排布）、 **Duplicate**（复制选中，唯一命名）、**Delete**（清理连接）、**按 dx/dy 移动**、**统一设置 rot°**、**Clear** |
| 可用日历 | 表单底部 `Timetable (JSON)` 折叠区直接编辑 `{"windows": [{"start": 0, "end": 28800}]}`（互不重叠的 `[start, end)` 区间，`null` 表示始终可用）；Apply 时解析为 `TimeTable`。注意与 `shifts` 区分：后者用 `{start, duration, repeat_every}` |
| 触发器编辑 | 表单底部 `Triggers (JSON)` 折叠区可编辑全部触发器（on_create/on_entry/on_exit/on_receive/on_message/on_breakdown/on_repair/on_photo_eye/on_content_change/on_state_change…），Apply 时解析为 `Triggers` 并校验 |
| 对象取样器 | 引用其他对象的字段不再靠盲打名字：`setup operator` / `process operator` 只列 **Dispatcher 与 Resource**， `home` / `shaft` 列全部对象名（字母序），下拉候选 + `or type` 可手输；输入的名字在模型里不存在时旁边显示琥珀色 `unknown` 提示（悬停说明原因），而不是静默接受 |
| 实时校验 | 选中对象的表单下方实时显示 `validate_collect` 的问题列表（location: message，最多 20 条），Apply 前即可发现问题 |
| 对象编辑 | `x/y/z` 拖拽数值、**按 kind 的参数表单**（Distribution 类型下拉 + 数值：source/queue/processor/conveyor/resource/transporter/person/robot/crane/asrs/agv）、Apply 校验后提交；JSON 高级模式（复选框） |
| 连接编辑 | 列表 + 删除、from/to/port 添加（校验后原子替换模型） |
| 3D 视图 | 透视投影、**地面网格**、对象地面阴影、按 kind 着色、连接线、选中高亮、拖拽旋转、滚轮缩放、点击选中、**相机预设 Iso/Top/Front/Side**、**按 kind 的几何体与明暗**（机台带顶板与指示灯、传送带带方向箭头、货架分层、源/汇圆鼓、人员/车辆胶囊、流体罐带液位、其余盒体）、**可旋转 footprint + 侧面挤出**（右键对象菜单 -15°/+15°/0°，写入 `Object.rot` 并随模型保存）、右侧挤出面模拟立体感 |
| 单元测试 | `crates/openflex-gui` 内置 **135 项**测试（v1.33.0 发布时为 126 项，之后随指标编辑器、追踪序列、报告改动与路线编辑器共增加 7 项；交互约定见下节），CI 的 GUI Linux job 会执行 |
| 外观 | 启动时应用深色主题（面板/窗口/画布分层配色），窗口与 macOS bundle 使用程序生成的 64×64 图标（无二进制资源） |
| Trace | 工具栏 `Trace run` 用 `simulate_traced()` 跑一次并保留引擎事件流；`Export trace` 写 `<model>.trace.jsonl`（与 CLI `--trace` 同格式） |
| 事件日志 | 右侧 `Events (n)`：过滤（kind/对象，忽略大小写）、点击跳转到该时刻、`CSV` 导出、`clear`；`Play` 与 `Step event` 都会填充 |
| 平面 2D | 工具栏 `2D on/off`：俯视平面渲染（按 kind 的足迹尺寸、旋转多边形、多边形命中、中心标签、状态彩点），导出 SVG 同样走俯视投影 |
| 校验状态 | 状态栏常驻 `valid` / `N issue(s)`（悬停列前 6 条）；表单内问题可点击定位对象；结果按模型世代缓存 |
| 时间单位 | 进度条、状态栏时钟、`COMPLETED / unit` 卡片、时序轴、`Thru / unit` 列头都带 `s/min/h/d` |
| warm-up | 进度条旁 `warm-up until X` → `measuring`；时序图对 warm-up 区加琥珀底色与分界线 |
| CSV 自描述 | 报告/对比/时序/事件四类 CSV 顶部写 `# model / # time_unit / …` 注释行，Excel 自动跳过 |
| 全局表 | 左侧 `Global tables (n)`：`+ table` 建表、`x` 删整表、列名可直接重命名、`+ col` / `- row` 调整形状， 单元格逐格编辑（数字/布尔/文本自动识别） |
| 全局变量 | 左侧 `Global variables (n)`：输入名字 `+ variable` 新建（默认 number 型、初值 0、重名被拒）， 逐条改值（失焦提交，保持声明的 `kind`），`x` 删除；表达式用 `$Name` 读取 |
| 取列表 | 左侧 `Pick lists (n)`：输入名字 `+ pick list` 新建（自动带一个起始条目）， 列表内可增删改条目（`name` / `value` 逐格编辑，条目名重复被拒），`x` 删整表； **每个列表至少保留一个条目**（删最后一条被拒并给出原因）；表达式用 `picklist("list","entry")` 取值 |
| 性能指标 | 左侧 `Performance measures (n)`：逐行编辑 `name` / `expression` / `unit` 三列， `+ measure` 加一行、`-` 删该行、`Apply measures` 提交； 提交前先做本地检查——**有名字没表达式**（或反之）直接报错且模型不动， 整行全空则**自动忽略**（可留空行不影响提交），`unit` 留空时**不写进 JSON**； 最终经 `commit_model_edit` 走校验→快照→清结果。编辑器下方提示表达式可读 `time` / `created` / `completed` / `$globals` |
| 已声明实验 | 左侧 `Declared experiments (n)` 列出模型 JSON 里 `experiments` 的每个实验（场景 × 复现），可 `run` 单个或 `Run all`；后台线程执行，结果表给 `completed`/吞吐/成败，失败行琥珀色并悬停显示原因 |
| 路线可视化 | dispatcher 声明 `explicit` 路线时，画布画出**琥珀色虚线**（每段末端小圆点），与蓝色实线的流连线区分；`home`/`pickup` 回到 dispatcher 自身，`requester` 依赖运行时故画到那里即停 |
| 跟踪汇总 | 报告面板的 `Tracked summaries (n)` 块：`state` 显示各状态占比（省略零占比，并注明覆盖整个模型），`staytime` 显示 n / min / p50 / p90 / max / mean；仅当模型声明了这两类跟踪变量时出现 |
| 任务序列 | `task_sequence` 编辑区有 `append` / `explicit` 模式切换；explicit 下每个 travel 腿配目标选择器（`home` / `object` / `position` / 无），object 给名字输入、position 给 x/y 输入；`utilize` / `break` 腿在组合框里只读展示（琥珀色 + `x` 可删），避免点击时丢掉 duration |
| 模型触发器 | 左侧 `Model triggers` 折叠区：三段**多行 JSON** 编辑区，分别对应 `on start`（首个事件前）、 `on reset`（预热边界）、 `on stop`（到达 stop_time），`Apply model triggers` 解析为动作列表并经校验写回； 预填规则：缓冲区为空**且**模型已声明动作时才填入格式化 JSON， 否则保持用户正在编辑的文本不被覆盖（空缓冲区提交即清空该段） |
| 检查点 | 右侧 `Checkpoints (n)` 面板列出目录内文件与模型时间，点击任意项即恢复该检查点；工具栏另有 `Save checkpoint` / `Load checkpoint`（最新）与目录输入：保存 `SimulationHandle::snapshot()` 为 `<dir>/checkpoint-<time>.json`，恢复时读取该目录最新文件并经 `Checkpoint::from_json_verified` 校验 |
| 运行 |**`parallel` 开关**：`Run` 与实验用 `run_model_parallel(model, 0)` 在多核上跑复现（结果与串行逐位一致），关闭则退回串行 |
| 运行 |**播放速度滑块（0.25–8×，对数刻度）**：Play 时每帧推进的模型时间与帧率随之缩放 |
| 运行 | `Run`（后台线程跑完整复现出报告）、`Step`（按 step 推进）、**`Step event`（逐事件推进并记录到事件日志）**、`Play`/`Pause`/`Resume`（`SimulationHandle`）、`Reset`、**进度条**、实时 `t`/created/completed |
| 事件日志 | 右侧可折叠面板显示最近 200 条 `t / event / object`（`Step event` 或 `Play` 时填充，自动滚到底部）；**点击任一行可跳转到该事件的模型时间**（重建句柄并推进到该时刻） |
| Studio trace 跳转 | Studio 的 trace 列表行可点击，把时间轴滑块移到该事件时间并同步 nearest 提示 |
| 指标卡片 | 报告顶部 2×2 卡片：COMPLETED（含 ±CI）、CREATED、AVG WIP、AVG UTIL |
| 状态配色 | `palette::ST_*` 单一来源；报告表每行名前有四段状态条（busy/blocked/idle/down），图例自动换行 |
| 图表样式 | 时间序列：参考线 + 面积填充 + 末点数值；Process Flow：按块类型着色 + 阴影 + 选中高亮 |
| 图表细节 | 利用率柱：圆角柱 + 50/100% 参考线 + 柱顶百分比 + hover 显示对象名；Pareto：四分之一参考线 + 前沿光晕环 + 轴标注 |
| 指标热力图 | 工具栏 `Heatmap` 下拉（off/utilization/throughput/WIP）按报告给对象着色（绿→黄→红），左下角显示色带与当前指标 |
| 实时可视化 | 鼠标悬停对象弹**提示气泡**：名称 + kind/内容量/状态/利用率 |
| 实时可视化 | 对象标签带**深色描边**（热力色/亮色下仍可读），盒内右下角显示**状态角标**（idle/busy/blocked/down/off），颜色不再是唯一线索 |
| 实时可视化 | 鼠标悬停对象弹**提示气泡**：名称 + kind/内容量/状态/利用率 |
| 实时可视化 | 对象按**瞬时状态**着色（busy 红 / blocked 橙 / idle 绿 / down 灰 / off-shift 深灰）、`[n]` 显示瞬时持有件数、**对象上方 item 圆点**、**执行器（transporter/person/AGV/robot/crane/asrs/elevator）按引擎实时位置绘制**、状态图例 |
| Process Flow | **可视化节点画布（支持拖拽平移、Cmd+滚轮缩放 0.4–2.5×、Reset view 与当前缩放显示）**（块画成圆角矩形、拖拽移动、点击两个块建立连接、选中高亮）+ 块 JSON 编辑、`+create/delay/assign/decision/transfer/sink` 快速加块、块列表删除（连带清理连接）、from/to 连接、`Apply process flow` |
| 实验 |**每个场景可设 `reps`（1…50）复现次数**，结果表显示 `value / completed / ±CI95`；图形仍是柱状对比 |
| 实验 | **stop-time 扫描**与**参数扫描**（JSON path，起点/步长/次数）→ 后台逐次运行，柱状图 + 值/completed 表 |
| 优化 | **grid / random / genetic** 三种搜索，**支持多变量**（`+ variable` 添加 JSON path + `[low, high]`）； grid 为笛卡尔积（>400 点时拒绝）、random 为 `samples` 次采样、genetic 为种群 12/6 代；可选**第二目标**（max avg stay）形成双列结果表 + **Pareto 前沿散点图**（x = max stay，y = completed，非支配点高亮）， 按主目标降序显示 top 12 与最佳点 |
| 报告 | Completed ± CI、Created、**利用率柱状图**、逐对象 **In/Out/WIP/Stay/Util** 表、**Export report CSV / JSON**、**Set baseline / Save baseline / Load baseline / Compare / Export compare CSV**（基线可存为 `<model>.baseline.json` 以跨会话对比，Δ利用率着色）、**状态饼图**（idle/busy/blocked/down/off-shift 合计）、**时间序列折线**（按 interval 采样；指标可切 WIP / throughput / utilization；对象可切换；**Export CSV** 导出 `time,object,content,throughput,utilization`）、**Replay 滑块 + 折线标记**；模型定义的 **Performance measures** 单独成块（measure / mean / ±95% / n，**mean 列带模型声明的 `unit`**，`n<2` 时区间显示 `—` 而不是 `±0.000`）；**Tracked series (n)** 表格（name / shape / samples / first / last），`shape` 列是 **84×14 迷你折线**——只有 1 个采样时显示 `1 sample`（不画点，避免误读成常量序列），全部值相同时标注该水平值，表格下方注明「序列只覆盖计量阶段」；**序列取自第一个复现**（`SimReport.tracked_series` 的定义如此），不是全复现聚合——要看逐复现差异请看旁边的 `Replications (n)` 表 |

编辑历史：**撤销/重做栈（每个编辑前快照，上限 50）**，工具栏按钮 + 快捷键。

## 安装包（三平台）

发布产物里包含桌面 GUI 的安装包（`gui (*)` CI job 构建，与 CLI 一起发布）：

| 平台 | 文件 | 说明 |
|---|---|---|
| Windows | `openflex-gui-<ver>-windows-x64.msi` | per-user 安装到 `%LOCALAPPDATA%\OpenFlex GUI\bin`，开始菜单 + 桌面快捷方式，无需管理员 |
| macOS | `OpenFlex-<ver>-universal-apple-darwin.app.zip` | **Universal**（Apple Silicon + Intel，`lipo` 合并）；解压即得 `OpenFlex.app`（ad-hoc 签名；首次打开需在「系统设置 → 隐私与安全性」放行） |
| Linux | `openflex-gui_<ver>_<arch>.deb` / `openflex-gui-<ver>-1.<arch>.rpm` | 安装到 `/usr/bin/openflex-gui`，依赖 libxkbcommon/libX11/libGL 等 |

本地自行打包：

```sh
# macOS .app
bash scripts/package-gui-app.sh --out dist
# Linux deb/rpm（需要 nfpm）
packaging/gui/build-gui-packages.sh --version <ver> --format both --out-dir dist
# Windows MSI（需要 WiX 5）
wix build packaging\windows\openflex-gui.wxs -d Version=<ver> ^
  -d BinaryPath=target\release\openflex-gui.exe -arch x64 -o dist\openflex-gui-<ver>-windows-x64.msi
```

## 与浏览器版 Studio 的一致性

`Object.rot`（度，逆时针）是模型字段，两套 UI 都读写它：

- 桌面 GUI：右键对象菜单 `-15° / +15° / 0°`，或多选后在批量面板填 `rotate to` 统一设置；
- 浏览器 Studio：对象编辑面板的 `rot°` 输入框，或多选后批量编辑的 `rot°`；
- 渲染路径全部按角度旋转 footprint：桌面 GUI（旋转 + 侧面挤出）、Studio **2D 俯视**、Studio **3D Canvas**、
  Studio **WebGL 3D**（角点在物体空间旋转后投影）；桌面版额外把前/右侧面挤出做出立体感；
- `openflex layout --svg` 平面视图用 `<g transform="rotate(deg cx cy)">`，等轴测视图直接旋转 footprint 角点，
  `--html` 交互预览继承同一份 SVG，可随模型一起进版本库。

## 交互约定（v1.30.0 起，v1.33.0 补「单一编辑入口」）

这些规则由单元测试锁住，改代码时若破坏约定会直接失败：

| 约定 | 行为 | 测试 |
|---|---|---|
| **先校验后快照** | 任何编辑先 `validate()`，通过后才写撤销栈；被拒绝的编辑不留下空历史项 | `rejected_edits_do_not_leave_history_entries` |
| **单一编辑入口** | `commit_model_edit(label, edit)` 是所有结构性编辑的唯一入口：在模型副本上执行改动 → `validate()` → 通过后才 `snapshot_labelled()` 入撤销栈、采用副本、并调用 `clear_analysis_results()` 清掉过期结果；校验失败则原样保留旧模型并暴露原因。新增编辑器一律走它 | `a_model_edit_goes_through_one_funnel` |
| **少于两次复现不报区间** | 只有样本数 ≥ 2 才显示置信区间，否则一律显示 `—`（`±0.000` 会被误读成"非常精确"）。覆盖四处：**对象表的 `±CI` 列**（判据 `report.replications.len() < 2`）、**`Performance measures` 块的 `±95%` 列**（判据 `measure.n < 2`）、**CLI 的文本/CSV/Markdown 出口**（统一走 `fmt_ci_opt` / `fmt_ci_precise`，`n < 2` 返回 `NO_INTERVAL`）、**实验表的 `±CI95`**（`aggregate.n >= 2` 才填） | `performance_measures_show_no_interval_for_one_replication`、`fmt_ci_opt(one) == NO_INTERVAL` |
| **结果随模型失效** | 撤销/重做、新增/删除/复制/重命名、连接增删、端口策略、应用最优、模型设置都会调用 `clear_analysis_results()`（报告、实验、优化、时序、trace） | `every_structural_edit_path_clears_results` |
| **位置操作不清结果** | 移动/旋转/对齐/分布/网格/自动布局不改变统计，故保留结果 | `structural_edits_invalidate_results_but_moves_do_not` |
| **丢弃文档要二次确认** | `New` / `Open`（含 Recent、文件浏览器、示例）与关闭窗口，在 `model_dirty` 时第一次只提示，第二次才执行 | `closing_with_unsaved_edits_needs_a_second_press` |
| **批量删除要二次确认** | 一次删除 ≥3 个对象需按两次 `Delete`；选区一变确认即失效 | `group_deletes_need_a_second_press` |
| **异步结果带来源** | Run/时序/实验/优化都回传模型名，换文档后状态栏标注 `from 'X' (model changed since)` | `every_worker_reports_the_model_it_ran` |
| **数值一律 f64** | 所有会写回模型的数值（run 设置、扫描、优化边界、坐标、批量位移）都用 f64，表单往返是恒等变换 | `form_position_round_trips_exactly` |
| **路径可预测** | 已保存模型的边车文件在同目录；未保存的导出与另存建议落到 `~/Documents/<模型名>.*`，布局预设落到 `~/.config/openflex/` | `export_paths_are_predictable_without_a_saved_model` |
| **提示与实现同步** | 帮助面板列出的每个快捷键都能在源码中找到对应绑定 | `help_dialog_lists_every_shortcut` |

## 与 FlexSim 桌面界面差距（按优先级）

| 优先级 | 缺口 | 说明 |
|---|---|---|
| ~~P0~~ | ~~参数表单（按 kind 的控件）~~ | **已完成（v0.52.0 / v0.64.0 / v0.93.0）**：Distribution 下拉 + 数值控件，覆盖全部 38 种对象；rack type/pick policy、queue order、valve curve 已是下拉，仅 `by_label` 这类带参枚举走 JSON |
| P0 | 3D 场景增强（部分完成） | ✅ 地面网格/阴影/相机预设/按 kind 几何体/明暗/旋转 + 侧面挤出（v0.75.0）；仍缺纹理贴图与实时光照模型 |
| P1 | 运行可视化 | ✅ 状态着色 + 瞬时 WIP + item 圆点 + 执行器实时位置 + 进度条 + 平滑动画 + **采样回放滑块**（v0.62.0） |
| P1 | 流程图/Process Flow 编辑 | ✅ 可视化节点画布 + 块增删/连接/JSON 编辑 + 平移/缩放（v0.59.0 / v1.12.0）；节点位置保存在内存（不写入模型） |
| P1 | 报告对比 | ✅ Set baseline + 并排对比表（Δ Utilization 绿/红）+ compare CSV（v0.73.0） |
| P1 | 实验与优化 UI | ✅ stop-time/参数扫描 + grid/random/genetic 多变量搜索 + 双目标表 + Pareto 前沿散点图（v0.69.0） |
| ~~P2~~ | 撤销/重做 | **已完成（v0.56.0）** |
| ~~P2~~ | 文件菜单/最近文件/新建 | **已完成（v0.56.0）**：New / Save as / Recent / 快捷键 |
| P2 | 统计图表 | ✅ 利用率柱状图 + 状态饼图 + 时间序列（WIP/吞吐/利用率）+ 时间序列/报告 CSV + 报告 JSON 导出（v0.70.0） |
| ~~P3~~ | 打包成 .app 双击启动 | **已完成（v0.58.0，universal v0.71.0，GUI MSI 随 v0.79.0 起发布）**：`scripts/package-gui-app.sh` 生成 `OpenFlex.app` + zip；Windows `.msi` 含 GUI，Linux `.deb`/`.rpm` 同批发布 |
| P2 | 本地化 | 见下方路线图第 5 项（层级树与历史列表已完成） |

## 路线图（v1.26.0 之后）

上面 1–5 项均已交付（参数表单 v0.52/v0.64、3D 增强 v0.75、运行可视化 v0.62、
实验/优化 v0.69、macOS `.app` v0.58/universal v0.71 + Windows MSI）。当前的剩余项：

1. **纹理与实时光照**：需要美术资源与 GPU 渲染路径，当前仍是程序化配色 + 伪造明暗。
2. ~~多级层级树~~：**已完成（v1.27.0 起）**——kind 组内按流程阶段再分一层，可 `by stage / flat` 切换。
3. **协作/多窗口**：无文档共享编辑、无多视口。
4. ~~完整 Undo 历史可视化~~：**已完成（v1.27.0 起）**——撤销栈带标签，左侧 `History` 面板可点击回退。
5. **本地化**：界面文案仍以英文为主（中文标签依赖系统 CJK 字体回退）。
