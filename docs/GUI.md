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
| 文件 | Open（路径输入）、Save（写回原文件）、加载时校验并报告对象数/stop |
| 模型树 | 按 kind 分组（可折叠）、名称/kind 过滤、点击选中 |
| 对象编辑 | `x/y/z` 拖拽数值、**按 kind 的参数表单**（Distribution 类型下拉 + 数值：source/queue/processor/conveyor/resource/transporter/person/robot/crane/asrs/agv）、Apply 校验后提交；JSON 高级模式（复选框） |
| 连接编辑 | 列表 + 删除、from/to/port 添加（校验后原子替换模型） |
| 3D 视图 | 透视投影、**地面网格**、对象地面阴影、按 kind 着色、连接线、选中高亮、拖拽旋转、滚轮缩放、点击选中、**相机预设 Iso/Top/Front/Side** |
| 运行 | `Run`（后台线程跑完整复现出报告）、`Step`（按 step 推进）、`Play`/`Pause`/`Resume`（`SimulationHandle`）、`Reset`、**进度条**、实时 `t`/created/completed |
| 实时可视化 | 对象按**瞬时状态**着色（busy 红 / blocked 橙 / idle 绿 / down 灰 / off-shift 深灰）、`[n]` 显示瞬时持有件数、状态图例 |
| 报告 | Completed ± CI、Created、**利用率柱状图**、逐对象 In/Out/Util 表 |

快捷键：无（工具栏按钮；后续补）。撤销/重做：无（每次编辑直接替换模型，可用 Save 前确认）。

## 与 FlexSim 桌面界面差距（按优先级）

| 优先级 | 缺口 | 说明 |
|---|---|---|
| ~~P0~~ | ~~参数表单（按 kind 的控件）~~ | **已完成（v0.52.0）**：Distribution 下拉 + 数值控件，覆盖 10 种常用对象；其余 kind 走 JSON |
| P0 | 3D 场景增强（部分完成） | 已有地面网格/阴影/相机预设；仍缺纹理、光照、对象几何体（圆柱/机台）、旋转 |
| P1 | 运行可视化（部分完成） | ✅ 状态着色 + 瞬时 WIP + 进度条（引擎已暴露 `content`/`state`/`phase`）；仍缺 flow item 移动动画与逐事件回放 |
| P1 | 流程图/Process Flow 编辑 | 只有对象与连接，没有 process flow 节点编辑 |
| P1 | 实验与优化 UI | CLI 有 `experiment`/`optimize`，GUI 未接入 |
| P2 | 撤销/重做 | 编辑器无 undo 栈（Studio 有） |
| P2 | 文件菜单/最近文件/新建向导 | 只有路径输入 |
| P2 | 统计图表 | 只有利用率柱状图；缺时间序列/状态饼图/吞吐曲线 |
| P3 | 打包成 .app/.exe 双击启动 | 需要 macOS bundle + Windows 安装器附带 GUI |

## 路线图

1. **参数表单**：按 kind 生成控件（数值/文本/分布），保留 JSON 视图作为高级模式。
2. **3D 增强**：地面网格、方向光、对象按 kind 的简单体（盒子/圆柱）、相机预设（iso/top/front）。
3. **运行可视化**：用 `SimulationHandle` 的事件流做 flow item 移动动画与状态着色。
4. **实验/优化**：接入 `experiment`/`optimize` 的 Rust API，在 GUI 中显示结果表。
5. **打包**：macOS `.app`（Info.plist + icon）与 Windows `.exe`（附 MSI 可选组件）。
