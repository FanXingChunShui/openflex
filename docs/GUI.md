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
| 文件 | **New**（生成 source→queue→processor→sink 模板）、Open（路径输入）、Save、**Save as…**、**Recent 最近 8 个**、加载时校验；**快捷键 Ctrl+Z / Ctrl+Shift+Z / Ctrl+S** |
| 模型树 | 按 kind 分组（可折叠）、名称/kind 过滤、点击选中 |
| 对象编辑 | `x/y/z` 拖拽数值、**按 kind 的参数表单**（Distribution 类型下拉 + 数值：source/queue/processor/conveyor/resource/transporter/person/robot/crane/asrs/agv）、Apply 校验后提交；JSON 高级模式（复选框） |
| 连接编辑 | 列表 + 删除、from/to/port 添加（校验后原子替换模型） |
| 3D 视图 | 透视投影、**地面网格**、对象地面阴影、按 kind 着色、连接线、选中高亮、拖拽旋转、滚轮缩放、点击选中、**相机预设 Iso/Top/Front/Side** |
| 运行 | `Run`（后台线程跑完整复现出报告）、`Step`（按 step 推进）、`Play`/`Pause`/`Resume`（`SimulationHandle`）、`Reset`、**进度条**、实时 `t`/created/completed |
| 实时可视化 | 对象按**瞬时状态**着色（busy 红 / blocked 橙 / idle 绿 / down 灰 / off-shift 深灰）、`[n]` 显示瞬时持有件数、**对象上方 item 圆点**、**执行器（transporter/person/AGV/robot/crane/asrs/elevator）按引擎实时位置绘制**、状态图例 |
| Process Flow | 折叠面板：块 JSON 编辑、`+create/delay/assign/decision/transfer/sink` 快速加块、块列表删除（连带清理连接）、from/to 连接、`Apply process flow`（合并回参数并校验） |
| 实验 | **stop-time 扫描**与**参数扫描**（JSON path 如 `objects/0/params/process_time/value`，起点/步长/次数）→ 后台逐次运行，柱状图 + 值/completed 表 |
| 报告 | Completed ± CI、Created、**利用率柱状图**、逐对象 In/Out/Util 表 |

编辑历史：**撤销/重做栈（每个编辑前快照，上限 50）**，工具栏按钮 + 快捷键。

## 打包为可双击应用

macOS：

```sh
bash scripts/package-gui-app.sh          # 生成 dist/gui/OpenFlex.app 与 zip
open dist/gui/OpenFlex.app               # 双击同样可启动
```

脚本会以 release 模式构建 `openflex-gui`、组装 `OpenFlex.app`（`Info.plist` 声明 `.ofm` 文档类型与
`LSMinimumSystemVersion 11.0`）、做 ad-hoc 签名并打出 `OpenFlex-<version>-<triple>.app.zip`。
Windows 上 `cargo build --release -p openflex-gui` 产出的 `openflex-gui.exe` 本身即可双击运行。

## 与 FlexSim 桌面界面差距（按优先级）

| 优先级 | 缺口 | 说明 |
|---|---|---|
| ~~P0~~ | ~~参数表单（按 kind 的控件）~~ | **已完成（v0.52.0）**：Distribution 下拉 + 数值控件，覆盖 10 种常用对象；其余 kind 走 JSON |
| P0 | 3D 场景增强（部分完成） | 已有地面网格/阴影/相机预设；仍缺纹理、光照、对象几何体（圆柱/机台）、旋转 |
| P1 | 运行可视化（部分完成） | ✅ 状态着色 + 瞬时 WIP + item 圆点 + 执行器实时位置 + 进度条；平滑动画用帧间指数缓动；仍缺逐事件回放 |
| P1 | 流程图/Process Flow 编辑（部分完成） | ✅ 块增删/连接/JSON 编辑（v0.57.0）；仍缺图形化拖拽画布 |
| P1 | 实验与优化 UI（部分完成） | ✅ stop-time 与参数扫描实验；优化器（CLI `optimize`）未接入 |
| ~~P2~~ | 撤销/重做 | **已完成（v0.56.0）** |
| ~~P2~~ | 文件菜单/最近文件/新建 | **已完成（v0.56.0）**：New / Save as / Recent / 快捷键 |
| P2 | 统计图表 | 只有利用率柱状图；缺时间序列/状态饼图/吞吐曲线 |
| ~~P3~~ | 打包成 .app 双击启动 | **已完成（v0.58.0）**：`scripts/package-gui-app.sh` 生成 `OpenFlex.app` + zip；Windows 的 `.exe` 可直接双击（MSI 附带 GUI 仍未做） |

## 路线图

1. **参数表单**：按 kind 生成控件（数值/文本/分布），保留 JSON 视图作为高级模式。
2. **3D 增强**：地面网格、方向光、对象按 kind 的简单体（盒子/圆柱）、相机预设（iso/top/front）。
3. **运行可视化**：用 `SimulationHandle` 的事件流做 flow item 移动动画与状态着色。
4. **实验/优化**：接入 `experiment`/`optimize` 的 Rust API，在 GUI 中显示结果表。
5. **打包**：macOS `.app`（Info.plist + icon）与 Windows `.exe`（附 MSI 可选组件）。
