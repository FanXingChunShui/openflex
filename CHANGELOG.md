# Changelog

## Unreleased

### 桌面 GUI（外观）

- **配色与主题**：新增 `palette`（BG/PANEL/CARD/CANVAS/STROKE/TEXT/TEXT_DIM/ACCENT/ACCENT_HI/AMBER/OK），
  圆角（6–8px）、细描边、hover/active 高亮环、选中对比色、等宽 hint；面板/工具栏/画布分层配色，内容区最深。
- **底部状态栏**：运行状态圆点（运行=绿、后台任务=琥珀、空闲=空心灰）、消息、右侧当前时间 `t=…` 与品牌色标题。
- **面板标题**：左/右面板改用大写字距小标题（`MODEL TREE` / `CONNECTIONS` / `REPORT` …）+ 细分隔线。
- **面板宽度**：左 330px（可拖 260–560）、右 360px（300–620），参数表单不再被挤压。
- **字形修复**：egui 默认字体缺 `✕ ● ○ ▸ → ↑↓`，改为 `x` / 绘制圆点 / `->` / `File >` / 方向键英文名，消除方框乱码。

### 桌面 GUI（可读性与布局）

- **自动布局**：新增 `Auto layout` 按钮与加载时检测——所有对象重叠在同一点时按连接顺序（BFS，源点优先、
  孤立对象收尾）排成方阵，首次打开即清晰；工具栏手动可重排。
- **加载后取景**：打开模型即 `frame_selection()`，并把请求延迟到画布尺寸测量后执行，避免首帧用错尺寸。
- **投影空间取景**：`frame_maths_view()` 在 **yaw/pitch 投影后** 计算包围盒与反解焦点，旋转后再也不会裁掉标签；
  留出 4 单位 padding 与 200px 边距，缩放钳制 0.15–2.5。
- 新增测试：`framed_points_stay_inside_the_viewport`（4 组相机角 × 全部点落在视口内）、
  `layout_order_is_breadth_first_from_the_sources`、`builtin_examples_parse_and_validate`、`frame_view_keeps_a_rotated_group_inside_the_canvas`。
  GUI 测试共 **12 项**。

### 说明

- 为控制发布节奏，本轮改动只合入主干，**不发布**；下次发布窗口在上一版（v1.23.0）之后 ≥3 小时。

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
