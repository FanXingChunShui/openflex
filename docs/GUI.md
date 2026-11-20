# 桌面 GUI（`openflex-gui`）

原生桌面窗口（eframe/egui），与浏览器版 `openflex studio` 共用同一套 model/core crate。
本页记录当前能力、与 FlexSim 桌面界面的差距，以及后续补齐计划。

```sh
cargo build -p openflex-gui --release
./target/release/openflex-gui examples/mm1.ofm   # 也可以不带参数启动后在窗口内填路径
```

## 当前能力（v1.26.0）

| 区域 | 功能 |
|---|---|
| 文件 | **New**（生成 source→queue→processor→sink 模板）、Open（路径输入）、Save、**Save as…**、**Recent 最近 8 个（持久化到 `~/.config/openflex/recent.json`，含 hover 全路径与 Clear recent）**、加载时校验 |
| 文件浏览 | 工具栏 `Browse…` 打开内置目录浏览器（文件夹在前、`.ofm` 在后，支持 `Up`/`Home`，选中即加载），不依赖 GTK |
| 产物清单 | 右侧 `Outputs (n)` 列出本会话写出的文件（名称、大小、完整路径悬停），`Clear list` 清空 |
| 快捷键 | `Ctrl/Cmd+A` 全选、`Ctrl/Cmd+D` 复制选中（自动 `_2` 命名 + 偏移）、`Ctrl/Cmd+O` 打开、`Ctrl/Cmd+N` 新建、`Ctrl/Cmd+S` 保存、`Ctrl/Cmd+Z` 撤销、`Ctrl/Cmd+Shift+Z` 重做、`Space` 播放/暂停、`R` 重置、`F1` 快捷键帮助对话框、右方向键步进、上/下方向键调整步长 |
| Studio 帮助 | 工具栏 `?` 按钮弹出快捷键面板（拖拽/滚轮/框选/方向键微调/撤销/保存/播放/删除，`data-help-panel`） |
| Studio 复制 | 浏览器 Studio 对象操作区新增 `Duplicate`（深拷贝参数、自动 `_2` 后缀命名、位置偏移 +2，经 `/api/model` 保存并入历史） |
| 重命名 | 属性面板头部名称可就地编辑（Enter/失焦提交、Esc 还原），自动改写全部引用（连接、home/charger/network、流程块） |
| 对象菜单 | 3D 视图**右键对象**：Focus camera（居中）、Reset position（z 归零）、**旋转 -15°/+15°/0°**（写 `Object.rot`，入撤销栈）、Delete object（删除并清理连接），均走校验 |
| 布局预设 | 左侧 `Layout presets (n)`：输入名字 `Save` 保存当前所有对象位置，点击名称即应用（入撤销栈），`x` 删除；存于模型旁的 `<model>.layout.json` |
| 视图导出 | 工具栏 `Export view SVG` 用与画布相同的投影（含旋转、热力色）输出 `<model>.view.svg`，可直接进文档或版本库 |
| 模型设置 | 左侧 `Model settings` 折叠区：name、time_unit（seconds/minutes/hours/days）、run.stop_time/warmup/seed/replications，`Apply settings` 经校验写回并重置当前运行 |
| 空状态引导 | 未加载模型时给出 `New model` 与三个内置示例（`mm1`/`routing`/`batch-queue`）一键打开；内置示例编译进二进制，无 `examples/` 目录时也可用 |
| 模型树 | **新增对象**（kind 下拉 28 种 + 可选名字，自动编号与安全默认参数，右侧自动摆放）、按 kind 分组（可折叠）、名称/kind 过滤、每行显示 `(x, y)` 与旋转角、组内 `select all`、点击选中 |
| 多选与批量 | 3D 视图**拖框多选**（Shift 追加），选中集合高亮；批量 **Align X/Y**（以首个为基准）、**Distribute X/Y**（首末之间等距分布，≥3 个）、**Grid 4**（方形网格排布）、
**Duplicate**（复制选中，唯一命名）、**Delete**（清理连接）、**按 dx/dy 移动**、**统一设置 rot°**、**Clear** |
| 可用日历 | 表单底部 `Timetable (JSON)` 折叠区直接编辑 `{"windows": [{"start": 0, "end": 28800}]}`（互不重叠的 `[start, end)` 区间，`null` 表示始终可用）；Apply 时解析为 `TimeTable`。注意与 `shifts` 区分：后者用 `{start, duration, repeat_every}` |
| 触发器编辑 | 表单底部 `Triggers (JSON)` 折叠区可编辑全部触发器（on_create/on_entry/on_exit/on_receive/on_message/on_breakdown/on_repair/on_photo_eye/on_content_change/on_state_change…），Apply 时解析为 `Triggers` 并校验 |
| 实时校验 | 选中对象的表单下方实时显示 `validate_collect` 的问题列表（location: message，最多 20 条），Apply 前即可发现问题 |
| 对象编辑 | `x/y/z` 拖拽数值、**按 kind 的参数表单**（Distribution 类型下拉 + 数值：source/queue/processor/conveyor/resource/transporter/person/robot/crane/asrs/agv）、Apply 校验后提交；JSON 高级模式（复选框） |
| 连接编辑 | 列表 + 删除、from/to/port 添加（校验后原子替换模型） |
| 3D 视图 | 透视投影、**地面网格**、对象地面阴影、按 kind 着色、连接线、选中高亮、拖拽旋转、滚轮缩放、点击选中、**相机预设 Iso/Top/Front/Side**、**按 kind 的几何体与明暗**（机台带顶板与指示灯、传送带带方向箭头、货架分层、源/汇圆鼓、人员/车辆胶囊、流体罐带液位、其余盒体）、**可旋转 footprint + 侧面挤出**（右键对象菜单 -15°/+15°/0°，写入 `Object.rot` 并随模型保存）、右侧挤出面模拟立体感 |
| 单元测试 | `crates/openflex-gui` 内置 **29 项**测试（JSON 路径、参数解析、热力色带、明暗、Pareto、事件格式、XML 转义、取景、BFS 布局、内置示例、字体路径、重命名引用、删除清理、trace 行格式、平面命中、路径收集、字节格式、校验缓存等），CI 的 GUI Linux job 会执行 |
| 外观 | 启动时应用深色主题（面板/窗口/画布分层配色），窗口与 macOS bundle 使用程序生成的 64×64 图标（无二进制资源） |
| Trace | 工具栏 `Trace run` 用 `simulate_traced()` 跑一次并保留引擎事件流；`Export trace` 写 `<model>.trace.jsonl`（与 CLI `--trace` 同格式） |
| 事件日志 | 右侧 `Events (n)`：过滤（kind/对象，忽略大小写）、点击跳转到该时刻、`CSV` 导出、`clear`；`Play` 与 `Step event` 都会填充 |
| 平面 2D | 工具栏 `2D on/off`：俯视平面渲染（按 kind 的足迹尺寸、旋转多边形、多边形命中、中心标签、状态彩点），导出 SVG 同样走俯视投影 |
| 校验状态 | 状态栏常驻 `valid` / `N issue(s)`（悬停列前 6 条）；表单内问题可点击定位对象；结果按模型世代缓存 |
| 时间单位 | 进度条、状态栏时钟、`COMPLETED / unit` 卡片、时序轴、`Thru / unit` 列头都带 `s/min/h/d` |
| warm-up | 进度条旁 `warm-up until X` → `measuring`；时序图对 warm-up 区加琥珀底色与分界线 |
| CSV 自描述 | 报告/对比/时序/事件四类 CSV 顶部写 `# model / # time_unit / …` 注释行，Excel 自动跳过 |
| 全局表 | 左侧 `Global tables (n)` 显示每张表的行×列、列名胶囊与前 4 行值 |
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
| 优化 | **grid / random / genetic** 三种搜索，**支持多变量**（`+ variable` 添加 JSON path + `[low, high]`）；
grid 为笛卡尔积（>400 点时拒绝）、random 为 `samples` 次采样、genetic 为种群 12/6 代；可选**第二目标**（max avg stay）形成双列结果表 + **Pareto 前沿散点图**（x = max stay，y = completed，非支配点高亮），
按主目标降序显示 top 12 与最佳点 |
| 报告 | Completed ± CI、Created、**利用率柱状图**、逐对象 **In/Out/WIP/Stay/Util** 表、**Export report CSV / JSON**、**Set baseline / Save baseline / Load baseline / Compare / Export compare CSV**（基线可存为 `<model>.baseline.json` 以跨会话对比，Δ利用率着色）、**状态饼图**（idle/busy/blocked/down/off-shift 合计）、**时间序列折线**（按 interval 采样；指标可切 WIP / throughput / utilization；对象可切换；**Export CSV** 导出 `time,object,content,throughput,utilization`）、**Replay 滑块 + 折线标记** |

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

## 与 FlexSim 桌面界面差距（按优先级）

| 优先级 | 缺口 | 说明 |
|---|---|---|
| ~~P0~~ | ~~参数表单（按 kind 的控件）~~ | **已完成（v0.52.0 / v0.64.0 / v0.93.0）**：Distribution 下拉 + 数值控件，覆盖全部 28 种对象；rack type/pick policy、queue order、valve curve 已是下拉，仅 `by_label` 这类带参枚举走 JSON |
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
