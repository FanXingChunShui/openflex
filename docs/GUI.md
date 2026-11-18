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
| Process Flow | **可视化节点画布**（块画成圆角矩形、拖拽移动、点击两个块建立连接、选中高亮）+ 块 JSON 编辑、`+create/delay/assign/decision/transfer/sink` 快速加块、块列表删除（连带清理连接）、from/to 连接、`Apply process flow` |
| 实验 | **stop-time 扫描**与**参数扫描**（JSON path，起点/步长/次数）→ 后台逐次运行，柱状图 + 值/completed 表 |
| 优化 | **grid search**：按 JSON path 在 `[low, high]` 上取 `steps` 个点，逐点运行，显示最好点与完整 `param → completed` 表 |
| 报告 | Completed ± CI、Created、**利用率柱状图**、逐对象 In/Out/Util 表、**状态饼图**（idle/busy/blocked/down/off-shift 合计）、**时间序列折线**（按 interval 采样；指标可切 WIP / throughput / utilization；对象可切换；**Export CSV** 导出 `time,object,content,throughput,utilization`）、**Replay 滑块** |

编辑历史：**撤销/重做栈（每个编辑前快照，上限 50）**，工具栏按钮 + 快捷键。

## 安装包（三平台）

发布产物里包含桌面 GUI 的安装包（`gui (*)` CI job 构建，与 CLI 一起发布）：

| 平台 | 文件 | 说明 |
|---|---|---|
| Windows | `openflex-gui-<ver>-windows-x64.msi` | per-user 安装到 `%LOCALAPPDATA%\OpenFlex GUI\bin`，开始菜单 + 桌面快捷方式，无需管理员 |
| macOS | `OpenFlex-<ver>-<triple>.app.zip` | 解压即得 `OpenFlex.app`（ad-hoc 签名；首次打开需在「系统设置 → 隐私与安全性」放行） |
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

## 与 FlexSim 桌面界面差距（按优先级）

| 优先级 | 缺口 | 说明 |
|---|---|---|
| ~~P0~~ | ~~参数表单（按 kind 的控件）~~ | **已完成（v0.52.0 + v0.64.0）**：Distribution 下拉 + 数值控件，覆盖全部 28 种对象（枚举型字段如 rack type/pick policy 仍走 JSON） |
| P0 | 3D 场景增强（部分完成） | 已有地面网格/阴影/相机预设；仍缺纹理、光照、对象几何体（圆柱/机台）、旋转 |
| P1 | 运行可视化 | ✅ 状态着色 + 瞬时 WIP + item 圆点 + 执行器实时位置 + 进度条 + 平滑动画 + **采样回放滑块**（v0.62.0） |
| P1 | 流程图/Process Flow 编辑 | ✅ 可视化节点画布 + 块增删/连接/JSON 编辑（v0.59.0）；节点位置保存在内存（不写入模型） |
| P1 | 实验与优化 UI（部分完成） | ✅ stop-time/参数扫描 + **grid search 优化**（v0.61.0）；多目标/genetic/CLI `optimize` 未接入 |
| ~~P2~~ | 撤销/重做 | **已完成（v0.56.0）** |
| ~~P2~~ | 文件菜单/最近文件/新建 | **已完成（v0.56.0）**：New / Save as / Recent / 快捷键 |
| P2 | 统计图表 | ✅ 利用率柱状图 + 状态饼图 + 时间序列（WIP/吞吐/利用率）+ CSV 导出（v0.65.0） |
| ~~P3~~ | 打包成 .app 双击启动 | **已完成（v0.58.0）**：`scripts/package-gui-app.sh` 生成 `OpenFlex.app` + zip；Windows 的 `.exe` 可直接双击（MSI 附带 GUI 仍未做） |

## 路线图

1. **参数表单**：按 kind 生成控件（数值/文本/分布），保留 JSON 视图作为高级模式。
2. **3D 增强**：地面网格、方向光、对象按 kind 的简单体（盒子/圆柱）、相机预设（iso/top/front）。
3. **运行可视化**：用 `SimulationHandle` 的事件流做 flow item 移动动画与状态着色。
4. **实验/优化**：接入 `experiment`/`optimize` 的 Rust API，在 GUI 中显示结果表。
5. **打包**：macOS `.app`（Info.plist + icon）与 Windows `.exe`（附 MSI 可选组件）。
