# 功能矩阵 ✅ 行的取证台账（PARITY_EVIDENCE）

> **用途**：`docs/FEATURE_PARITY_75.md` 里每一行 ✅ 都必须能在这里指到**实现符号 + 至少一个测试**。
> 一个没有测试支撑的 ✅ 比一个诚实的 🟡 危险得多——读者会据此判断"这个功能可以依赖"。
>
> **证据来源**：本台账只引用本仓库 `crates/` 的源码与测试（`grep` 核对，2026-11-22），
> 功能语义的原始定义来自公开帮助文档（`FlexSimChinese7.5/help` 的 326 页，见
> [FEATURE_PARITY_75.md](FEATURE_PARITY_75.md)）。**未接触 FlexSim 源代码或二进制。**

## 判据

| 结论 | 含义 |
|---|---|
| **有证据** | 有实现符号，且至少一个测试**断言行为**（数值 / 事件 / 输出 / 求值结果）。 |
| **有证据（schema 级）** | 只有"能解析/能校验"的测试；用于纯 schema 或纯形态差异的行，已注明。 |
| **有证据（表单往返级）** | GUI 编辑器的往返测试（断言改完参数后模型里的值正确），不断言仿真行为。 |
| **证据薄弱** | 有实现，但测试只覆盖相邻行为、或只有解析测试，或（文档类）无任何测试。**已降级为 🟡**。 |
| **未找到** | 实现符号在，但**引擎/运行路径根本不使用它**，或找不到实现。**已降级为 🟡**。 |

## 台账（41 行 ✅ 审计 + 2 行升级）

| # | 功能（矩阵行） | 实现位置 | 证据（测试名 / 文件） | 结论 |
|---|---|---|---|---|
| 1 | Flowitems（流动实体） | `engine.rs` 的 `struct FlowItem { id, item_type, labels, created, entry_time }` | `source_respects_max_arrivals`（golden.rs，断言 created 数）；`mm1_throughput_matches_arrival_rate`（golden.rs） | 有证据 |
| 2 | Itemtype | `action.rs` `Action::SetItemType`；`model.rs` `PortStrategy::ByItemType` | `split_by_item_type_routes_to_the_matching_port`（objects.rs，断言按 type 分流到不同出口的件数） | 有证据 |
| 3 | Pick Lists | `Model.picklists`；`validate_picklists`；`expr.rs` `EvalContext::picklist_entry`；`picklist.value_of` | `picklist_lookup_returns_the_entry_value`（flexsim_concepts.rs，断言 `picklist("Shift","Night")` 求值结果）；`unknown_picklist_and_unknown_entry_are_distinct_errors` | 有证据 |
| 4 | Return Values（代码字段返回值） | `expr.rs` 表达式求值；`action.rs` `ConditionalPort.script_condition`（`result` 约定） | `script_condition_can_return_a_boolean_and_query_objects`（script.rs，断言布尔返回值改变路由）；`script_trigger_example_routes_by_script_condition` | 有证据 |
| 5 | Order of Events（固定事件序列） | `engine.rs` 处理器阶段机（entry → setup → process → exit） | `setup_and_process_finish_hooks_fire_at_the_phase_boundaries`（engine.rs，断言 setup 结束于 t=2、process 结束于 t=5，即阶段先后）；`processor_on_entry_and_on_exit_triggers_fire`（objects.rs） | 有证据 |
| 6 | FlexSim Concepts Overview | —（概念索引页，无功能） | — | **证据薄弱（概念页，无可执行行为可测）** → 已降级 |
| 7 | FixedResources（Source/Queue/Processor/Sink/Combiner/Separator/MultiProcessor/BasicFR/Rack） | `model.rs` 对应 `*Params` + `engine.rs` 各 kind 分支 | 9 个 kind 均有行为测试：`multiprocessor_slots_raise_the_throughput`、`breakdown_preempts_processing_and_repairs`、`rack_min_dwell_delays_release`、Combiner 见 `combiner_pack_releases_one_container_per_cycle` / `combiner_join_merges_one_item_per_port`；Separator 见 `separator_unpacks_quantity_plus_container`（qty+1）/ `separator_splits_into_components_plus_container`；BasicFR 的清空行为见 `emptier_flushes_upstream_on_a_schedule`（均 objects.rs / engine_integration.rs） | 有证据 |
| 8 | NetworkNode / TrafficControl | `engine.rs` `traffic_try_acquire` / `traffic_release` / `traffic_wait_for_slot` / `traffic_hold` | `traffic_control_mutual_exclusion_waits_and_releases`；`traffic_control_capacity_two_admits_both`；`traffic_control_admissions_survive_checkpoint_restore`（均 engine.rs，断言等待次数与完成时刻） | 有证据 |
| 9 | Statistics Window（实时状态与吞吐） | `stats.rs` `ObjectStats` / `StateTimes` / `Aggregate` | `state_fractions_sum_to_one`（stats.rs）；`mm1_matches_analytic_queue_length`（golden.rs，断言 `avg_content ≈ 3.2`） | 有证据 |
| 10 | Source Page（三种到达风格 + 到达表） | `SourceParams`（interarrival / first_arrival / max_arrivals / items_per_arrival / arrival_schedule_from_table）；`engine.rs` 表驱动到达 | `mm1_throughput_matches_arrival_rate`；`source_respects_max_arrivals`；`source_can_load_its_arrival_schedule_from_a_table`（table_driven.rs） | 有证据 |
| 11 | Processor Page（setup/process/操作员/抢占） | `ProcessorParams`（setup_time/process_time/max_content/setup_operator/process_operator/operator_priority/operator_preempt） | `breakdown_preempts_processing_and_repairs`；`operator_preempt_interrupts_and_resumes`（objects.rs，断言抢占与恢复时刻） | 有证据 |
| 12 | ProcessTimes Page（Same as Setup） | `ProcessorParams.num_setup_operators` / `num_process_operators` / `use_same_num_operators`；`validate.rs` 一致性校验 | `a10_same_as_setup_reuses_the_setup_crew`（p0_semantics.rs，断言复用同样操作员时吞吐更高） | 有证据 |
| 13 | NetworkNode Page（最大停留车辆数 `max_travelers`） | —（**该字段已移除**：审计发现它声明了但引擎从不读取，用户设了值模型毫无变化。实现节点级限额需要重写 network 路由的占用模型，不做半成品，故删除字段/控件/文案） | `removed_max_travelers_is_rejected`（engine.rs，断言含该字段的模型被拒绝）；`network_nodes_without_max_travelers_are_unchanged`（无字段模型报告逐位不变） | **已移除（不是降级：功能本就不存在）** |
| 14 | Traffic Control Page（互斥 / 非定时） | `TrafficControlParams.mode`（`mutual_exclusion` / `un_timed`）+ `traffic_controls_for_leg` | `traffic_control_un_timed_applies_speed_only`（断言 un_timed 只缩放速度不限名额）；`traffic_control_mutual_exclusion_waits_and_releases` | 有证据 |
| 15 | Send To Port（7 策略 + Hold） | `model.rs` `PortStrategy`（FirstAvailable/RoundRobin/Random/ByPercentage/ByCondition/ByItemType/ByLabel/Hold）；`engine.rs` 路由 | `a04_hold_keeps_items_until_release`；`percentage_routing_is_respected`（golden.rs，断言 40/60 与 10/90 的百分比分流各到对应出口）；`script_condition_can_return_a_boolean_and_query_objects`（script.rs，断言 ByCondition 按脚本布尔值改变路由）；`split_by_item_type_routes_to_the_matching_port`；`by_label`（engine_integration.rs）；round_robin（p0_semantics.rs） | 有证据 |
| 16 | Pull Strategy | `PullParams.strategy`；`engine.rs` 拉取 | `a03_pull_strategy_round_robin_uses_every_upstream`（断言轮询到全部上游） | 有证据 |
| 17 | Pull Requirement | `PullParams.requirement`；`engine.rs` `pull_requirement_met` | `a03_pull_requirement_skips_items`（断言被拒件被跳过、只取满足条件的件） | 有证据 |
| 18 | Process Time / Setup Time | `ProcessorParams.process_time` / `setup_time`（`Distribution` 或表达式） | `setup_and_process_finish_hooks_fire_at_the_phase_boundaries`（断言 setup=2、process=3 的边界时刻）；`station_holds_items_for_its_process_time`（objects.rs） | 有证据 |
| 19 | Minimum Staytime（Rack） | `RackParams.min_dwell`；`engine.rs` rack 释放定时器 | `rack_min_dwell_delays_release`（engine_integration.rs，断言每个件至少滞留 min_dwell） | 有证据 |
| 20 | Load / Unload Time | `transport_load_time` / `transport_unload_time`（engine.rs） | `explicit_chain_with_load_unload_is_deterministic`（只断言两次运行事件流逐位相同，**未断言 load/unload 时长数值**） | **证据薄弱（未断言时长）** → 已降级 |
| 21 | Entry Exit Trigger（OnEntry/OnExit） | `Triggers.on_entry` / `on_exit` | `processor_on_entry_and_on_exit_triggers_fire`（objects.rs，断言 on_exit 设的标签被下游路由看到） | 有证据 |
| 22 | Reset Trigger | `Triggers.on_reset` | `on_reset_trigger_runs`（engine_integration.rs，断言预热边界触发并可释放件） | 有证据 |
| 23 | Breakdown/Repair Trigger | `Triggers.on_breakdown` / `on_repair` | `repair_trigger_fires`（objects.rs，断言修复标签+1）；`breakdown_preempts_processing_and_repairs` | 有证据 |
| 24 | Setup/Process Finish Trigger | `Triggers.on_setup_finish` / `on_process_finish`；`SimTrigger::SetupFinish/ProcessFinish` | `setup_and_process_finish_hooks_fire_at_the_phase_boundaries`（断言各触发一次、时刻 2 与 5） | 有证据 |
| 25 | Global Tables | `Model.global_tables`；`table_cell` / `set_cell` / `insert_row` / `remove_row`；`expr.rs` 表达式 `table(name,row,col)` | `global_table_function_selects_a_row_with_an_expression`（engine_integration.rs，断言两类型件按表值得到不同输入数）；`set_cell_updates_a_table_in_memory`、`insert_and_remove_rows`（table_driven.rs） | 有证据 |
| 26 | Model Triggers | `ModelTriggers.on_start/on_reset/on_stop`；`fire_model_actions` | `model_triggers_fire_at_start_reset_and_stop`（engine.rs，断言三处各触发）；`model_trigger_errors_are_reported` | 有证据 |
| 27 | Event List | `trace.rs` `TraceLog`/`TraceEvent`；`simulate_traced`；CLI `--trace`；Studio 事件列表 | `trace_writes_parseable_jsonl_events`（cli.rs）；`reads_a_jsonl_trace_file`（item_trace.rs） | 有证据 |
| 28 | Quick Properties（上下文属性面板） | `openflex-gui` `apply_form` + 各 kind 表单控件 | `the_visual_tool_form_round_trips_its_params`；`form_position_round_trips_exactly`（GUI，断言改后参数值正确） | 有证据（表单往返级） |
| 29 | Find Objects（按名称/类型查找） | `openflex-gui` 模型树内联过滤（`self.search` + name/kind 子串匹配，main.rs 树渲染处） | 无（`grep fn .*search` 无测试；过滤逻辑内联在 UI 闭包里，不可单测） | **证据薄弱（无测试，逻辑不可单测）** → 已降级 |
| 30 | Reports：State Report（状态占比） | `stats.rs` `StateTimes.fraction`；报告 `state_times` | `state_tracked_variable_reports_state_shares`（engine.rs，断言占比）；`state_fractions_sum_to_one` | 有证据 |
| 31 | Experimenter：`experiments[]` | `Model.experiments`；`openflex-report/src/experiment.rs`；CLI `experiment`；GUI 面板 | `returns_one_outcome_per_scenario_with_full_reports`（experiments.rs，断言 2×2=4 个结果）；`a_failing_scenario_does_not_stop_its_siblings` | 有证据 |
| 32 | Dashboard Item Trace（逐件轨迹） | `openflex-report/src/item_trace.rs`（`traced_items`/路径分组）；CLI `trace --item/--items/--paths` | `groups_steps_per_item_in_time_order`；`traced_items_matches_the_tagged_events`；`item_metrics_count_distinct_objects_and_span`（item_trace.rs） | 有证据 |
| 33 | Experimenter：Scenarios（含 override/stop/warmup） | `Scenario{overrides, stop_time, warmup}`；`experiment.rs` `prepare_scenario` | `a faster processor must raise throughput`（cli.rs，断言 baseline vs faster 两场景吞吐不同）；`experiment_json_contains_every_scenario`；`scenario_stop_time_and_warmup_are_accepted`（validation.rs，schema 级） | 有证据 |
| 34 | Experimenter：Performance Measures | `Model.performance_measures`；`evaluate_performance_measures`；`report`/`xlsx`/`experiment` 出口 | `report_formats_carry_model_measures`（measures.rs）；`experiment_formats_carry_model_measures`；`performance_measure_single_and_multi_replication_stats`（engine.rs）；`unknown_performance_measure_variable_is_an_error` | 有证据 |
| 35 | Experimenter：Analyzing Results | `report` 汇总 / `compare.rs` / `batch.rs`（batch-means + Student-t） | `compare_reports_the_metric_deltas`（cli.rs）；`report_formats_carry_model_measures`；`batch.rs` 单测；`a12_small_sample_ci_uses_the_t_table`（p0_semantics.rs） | 有证据 |
| 36 | FlexSim XML（文本可 diff） | 无 XML；模型是 JSON（`serde_json` 往返） | `global_tables_roundtrip_through_json`（tables_and_connections.rs，schema 级）；无"文本可版本化"的行为测试 | **证据薄弱（形态声明，仅 schema 级往返）** → 已降级 |
| 37 | Table Editor（编辑表数据） | `openflex-gui` 全局表编辑器（`add_global_table`/`append_table_column`/`commit_table_row`/`pop_table_row`） | `table_columns_can_be_renamed`；`table_columns_and_rows_can_grow_and_shrink`；`deleting_a_global_table_removes_it_and_its_buffers`（GUI） | 有证据（表单往返级） |
| 38 | Getting Started（软件定位与建模流程） | `README.md`、`docs/` | 无测试（文档本身即产出） | **证据薄弱（纯文档，无校验）** → 已降级 |
| 39 | First Model（三品种加工示例） | `examples/` 共 72 个模型 | 有 72 个可 validate 的示例与 golden 测试，但**没有一个对应"三品种加工"的示例** | **证据薄弱（无一对一示例）** → 已降级 |
| 40 | Terminology（术语表） | `docs/spec/flexsim-to-openflex.md` 术语表 | 无测试（纯文档） | **证据薄弱（纯文档，无校验）** → 已降级 |
| 41 | Conveyor（直/弯）、Join Conveyor | `ConveyorParams`（speed/length/spacing/curve/segments/join_inputs）；`conveyor.rs` 分段积分 | `segmented_belt_integrates_speed_per_band`；`segmented_belt_order_matters_for_intermediate_positions`（conveyor.rs）；`join_inputs_must_match_the_connections`（objects.rs） | 有证据 |
| **42** | **Performance Measure（PickLists 页，每复现值）** —— 原 🟡，本轮升级 | 同 #34：`Model.performance_measures` + `evaluate_performance_measures` | 同 #34：`performance_measure_single_and_multi_replication_stats`；`report_formats_carry_model_measures` | **已升级 ✅（原说明与代码矛盾）** |
| **43** | **Tracked Variables（四种 kind）** —— 原 🟡，本轮升级 | `TrackedVariable{name, expression, kind: TrackedKind}`（Level/State/Staytime/Kinetic）+ `Model.tracked_interval`；`SimReport.tracked_series` / `tracked_summaries` | `kinetic_tracked_variable_reports_the_rate_of_change`；`staytime_tracked_variable_reports_the_distribution`；`state_tracked_variable_reports_state_shares`（均 engine.rs，断言数值） | **已升级 ✅（原说明称"只有一种 kind"，与 `TrackedKind` 四变体矛盾）** |

### 备注（审计中发现、但不在本任务写权限内的问题）

- **`basic_te` 没有任何测试提及**（39 个 kind 中唯一）：`BasicTeParams` 有解析与 GUI 表单，但没有断言其行为的测试。
  矩阵里它属于 🟡 的 TaskExecuters 行，未升 ✅。
- **`Motor` 已落地**（`MotorParams{running, triggers}` + 4 个行为测试 `stopped_motor_freezes_conveyor_and_acceptance` 等），
  但矩阵里「Motor（开/关控制、同步 dog gap）」仍写着"没有 Motor 对象"——本轮只修正该句描述，状态保持 🟡（dog gap 同步确未实现）。
- **`max_travelers` 是"声明了但没有行为"的典型**：字段、GUI 控件、`objects describe` 文本都有，引擎从不读它。
  这类"三处文档都说有、只有引擎没有"的字段最容易制造虚标，建议加一条校验：`validate` 对"已声明但未接线"的参数给出 warning。
- **矩阵的 ✅ 计数在近期被并行改动过**（207 行、41 ✅），与任务卡里写的"42 行 ✅"不同步；本台账以当前文件的 41 行为准。

## 与矩阵的一致性

| 状态 | 审计前 | 审计后 |
|---|---:|---:|
| ✅ | 41 | **35** |
| 🟡 | 96 | **103** |
| ⛔ | 70 | **69** |
| 合计 | 207 | 207 |

降级 8 行（见上表 #6/#13/#20/#29/#36/#38/#39/#40），升级 2 行（#42/#43），
另把 1 行 ⛔ 修正为 🟡（SQL 查询已实现只读子集，见下）。

### 附：本轮一并修正的一处 ⛔ 误标

| 功能 | 原状态 | 现状 | 实现与证据 |
|---|---|---|---|
| SQL Queries（`query()`） | ⛔「无 SQL」 | 🟡 | `crates/openflex-model/src/sql.rs`：只读子集 `SELECT … FROM … [WHERE] [ORDER BY] [LIMIT]`，表达式里可调 `query("…")`（`expr.rs`）；测试 `parses_every_clause` / `executes_and_formats_rows` / `filters_and_sorts` / `empty_results_are_not_errors` / `errors_name_the_fragment`。缺 JOIN/子查询/聚合/分组 |