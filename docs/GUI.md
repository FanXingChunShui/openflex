# 桌面 GUI（`openflex-gui`）

原生桌面窗口（eframe/egui），与浏览器版 `openflex studio` 共用同一套 model/core crate。
本页记录当前能力、与 FlexSim 桌面界面的差距，以及后续补齐计划。

```sh
cargo build -p openflex-gui --release
./target/release/openflex-gui examples/mm1.ofm   # 也可以不带参数启动后在窗口内填路径
```

## 当前能力（v0.51.0）

| 区域 | 功能 |
|---|---|
| 文件 | **New**（生成 source→queue→processor→sink 模板）、Open（路径输入）、Save、**Save as…**、**Recent 最近 8 个**、加载时校验 |
| 快捷键 | `Ctrl/Cmd+O` 打开、`Ctrl/Cmd+N` 新建、`Ctrl/Cmd+S` 保存、`Ctrl/Cmd+Z` 撤销、`Ctrl/Cmd+Shift+Z` 重做、`Space` 播放/暂停、`R` 重置、`F1` 快捷键帮助对话框 |
| Studio 帮助 | 工具栏 `?` 按钮弹出快捷键面板（拖拽/滚轮/框选/方向键微调/撤销/保存/播放/删除，`data-help-panel`） |
| Studio 复制 | 浏览器 Studio 对象操作区新增 `Duplicate`（深拷贝参数、自动 `_2` 后缀命名、位置偏移 +2，经 `/api/model` 保存并入历史） |
| 对象菜单 | 3D 视图**右键对象**：Focus camera（居中）、Reset position（z 归零）、**旋转 -15°/+15°/0°**（写 `Object.rot`，入撤销栈）、Delete object（删除并清理连接），均走校验 |
| 模型设置 | 左侧 `Model settings` 折叠区：name、time_unit（seconds/minutes/hours/days）、run.stop_time/warmup/seed/replications，`Apply settings` 经校验写回并重置当前运行 |
| 模型树 | **新增对象**（kind 下拉 28 种 + 可选名字，自动编号与安全默认参数，右侧自动摆放）、按 kind 分组（可折叠）、名称/kind 过滤、点击选中 |
| 多选与批量 | 3D 视图**拖框多选**（Shift 追加），选中集合高亮；批量 **Align X/Y**（以首个为基准）、**Distribute X/Y**（首末之间等距分布，≥3 个）、**Grid 4**（方形网格排布）、
**Delete**（清理连接）、**按 dx/dy 移动**、**统一设置 rot°**、**Clear** |
| 触发器编辑 | 表单底部 `Triggers (JSON)` 折叠区可编辑全部触发器（on_create/on_entry/on_exit/on_receive/on_message/on_breakdown/on_repair/on_photo_eye/on_content_change/on_state_change…），Apply 时解析为 `Triggers` 并校验 |
| 实时校验 | 选中对象的表单下方实时显示 `validate_collect` 的问题列表（location: message，最多 20 条），Apply 前即可发现问题 |
| 对象编辑 | `x/y/z` 拖拽数值、**按 kind 的参数表单**（Distribution 类型下拉 + 数值：source/queue/processor/conveyor/resource/transporter/person/robot/crane/asrs/agv）、Apply 校验后提交；JSON 高级模式（复选框） |
| 连接编辑 | 列表 + 删除、from/to/port 添加（校验后原子替换模型） |
| 3D 视图 | 透视投影、**地面网格**、对象地面阴影、按 kind 着色、连接线、选中高亮、拖拽旋转、滚轮缩放、点击选中、**相机预设 Iso/Top/Front/Side**、**按 kind 的几何体与明暗**（机台带顶板与指示灯、传送带带方向箭头、货架分层、源/汇圆鼓、人员/车辆胶囊、流体罐带液位、其余盒体）、**可旋转 footprint + 侧面挤出**（右键对象菜单 -15°/+15°/0°，写入 `Object.rot` 并随模型保存）、右侧挤出面模拟立体感 |
| 单元测试 | `crates/openflex-gui` 内置 7 项测试（JSON 路径写入、参数解析、热力色带、明暗、Pareto 前沿、事件日志格式），CI 的 GUI Linux job 会执行 |
| 检查点 | 右侧 `Checkpoints (n)` 面板列出目录内文件与模型时间，点击任意项即恢复该检查点；工具栏另有 `Save checkpoint` / `Load checkpoint`（最新）与目录输入：保存 `SimulationHandle::snapshot()` 为 `<dir>/checkpoint-<time>.json`，恢复时读取该目录最新文件并经 `Checkpoint::from_json_verified` 校验 |
| 运行 | `Run`（后台线程跑完整复现出报告）、`Step`（按 step 推进）、**`Step event`（逐事件推进并记录到事件日志）**、`Play`/`Pause`/`Resume`（`SimulationHandle`）、`Reset`、**进度条**、实时 `t`/created/completed |
| 事件日志 | 右侧可折叠面板显示最近 200 条 `t / event / object`（`Step event` 或 `Play` 时填充，自动滚到底部）；**点击任一行可跳转到该事件的模型时间**（重建句柄并推进到该时刻） |
| Studio trace 跳转 | Studio 的 trace 列表行可点击，把时间轴滑块移到该事件时间并同步 nearest 提示 |
| 指标热力图 | 工具栏 `Heatmap` 下拉（off/utilization/throughput/WIP）按报告给对象着色（绿→黄→红），左下角显示色带与当前指标 |
| 实时可视化 | 对象按**瞬时状态**着色（busy 红 / blocked 橙 / idle 绿 / down 灰 / off-shift 深灰）、`[n]` 显示瞬时持有件数、**对象上方 item 圆点**、**执行器（transporter/person/AGV/robot/crane/asrs/elevator）按引擎实时位置绘制**、状态图例 |
| Process Flow | **可视化节点画布**（块画成圆角矩形、拖拽移动、点击两个块建立连接、选中高亮）+ 块 JSON 编辑、`+create/delay/assign/decision/transfer/sink` 快速加块、块列表删除（连带清理连接）、from/to 连接、`Apply process flow` |
| 实验 | **stop-time 扫描**与**参数扫描**（JSON path，起点/步长/次数）→ 后台逐次运行，柱状图 + 值/completed 表 |
| 优化 | **grid / random / genetic** 三种搜索，**支持多变量**（`+ variable` 添加 JSON path + `[low, high]`）；
grid 为笛卡尔积（>400 点时拒绝）、random 为 `samples` 次采样、genetic 为种群 12/6 代；可选**第二目标**（max avg stay）形成双列结果表 + **Pareto 前沿散点图**（x = max stay，y = completed，非支配点高亮），
按主目标降序显示 top 12 与最佳点 |
| 报告 | Completed ± CI、Created、**利用率柱状图**、逐对象 **In/Out/WIP/Stay/Util** 表、**Export report CSV / JSON**、**Set baseline / Compare / Export compare CSV**（与上一次运行对比，Δ利用率着色）、**状态饼图**（idle/busy/blocked/down/off-shift 合计）、**时间序列折线**（按 interval 采样；指标可切 WIP / throughput / utilization；对象可切换；**Export CSV** 导出 `time,object,content,throughput,utilization`）、**Replay 滑块 + 折线标记** |

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
| ~~P0~~ | ~~参数表单（按 kind 的控件）~~ | **已完成（v0.52.0 + v0.64.0）**：Distribution 下拉 + 数值控件，覆盖全部 28 种对象（枚举型字段如 rack type/pick policy 仍走 JSON） |
| P0 | 3D 场景增强（部分完成） | ✅ 地面网格/阴影/相机预设/按 kind 几何体/明暗/旋转 + 侧面挤出（v0.75.0）；仍缺纹理贴图与实时光照模型 |
| P1 | 运行可视化 | ✅ 状态着色 + 瞬时 WIP + item 圆点 + 执行器实时位置 + 进度条 + 平滑动画 + **采样回放滑块**（v0.62.0） |
| P1 | 流程图/Process Flow 编辑 | ✅ 可视化节点画布 + 块增删/连接/JSON 编辑（v0.59.0）；节点位置保存在内存（不写入模型） |
| P1 | 报告对比 | ✅ Set baseline + 并排对比表（Δ Utilization 绿/红）+ compare CSV（v0.73.0） |
| P1 | 实验与优化 UI | ✅ stop-time/参数扫描 + grid/random/genetic 多变量搜索 + 双目标表 + Pareto 前沿散点图（v0.69.0） |
| ~~P2~~ | 撤销/重做 | **已完成（v0.56.0）** |
| ~~P2~~ | 文件菜单/最近文件/新建 | **已完成（v0.56.0）**：New / Save as / Recent / 快捷键 |
| P2 | 统计图表 | ✅ 利用率柱状图 + 状态饼图 + 时间序列（WIP/吞吐/利用率）+ 时间序列/报告 CSV + 报告 JSON 导出（v0.70.0） |
| ~~P3~~ | 打包成 .app 双击启动（v0.58.0，universal v0.71.0）：`scripts/package-gui-app.sh` 生成 `OpenFlex.app` + zip；Windows 的 `.exe` 可直接双击（MSI 附带 GUI 仍未做） |

## 路线图

1. **参数表单**：按 kind 生成控件（数值/文本/分布），保留 JSON 视图作为高级模式。
2. **3D 增强**：地面网格、方向光、对象按 kind 的简单体（盒子/圆柱）、相机预设（iso/top/front）。
3. **运行可视化**：用 `SimulationHandle` 的事件流做 flow item 移动动画与状态着色。
4. **实验/优化**：接入 `experiment`/`optimize` 的 Rust API，在 GUI 中显示结果表。
5. **打包**：macOS `.app`（Info.plist + icon）与 Windows `.exe`（附 MSI 可选组件）。
