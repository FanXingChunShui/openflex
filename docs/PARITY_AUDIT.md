# PARITY_AUDIT：51 行 ✅ 的第二轮取证复核

**审计日期**：2026-11-25
**审计范围**：`docs/FEATURE_PARITY_75.md` 当前文件中所有功能行 ✅。**审计开始时按卡口径 51 行功能 ✅**；审计发现后 Lead 已把 Event List 降级为 🟡，并给 First Model 补了 3 个行为测试，**当前矩阵为 50 行 ✅**。本表按当前矩阵 50 行记录，审计发现保留在末节；`PARITY_EVIDENCE.md` 台账为 43 行。
**只读纪律**：本文件是唯一新增/修改文件；未改测试、源码、矩阵 ✅/🟡 标记。降级建议仅供 Lead 决策。

## 方法与判据

- 测试名存在性：`scripts/check-doc-evidence.sh`（47 个引用名全部解析）。
- 测试通过性：`CARGO_INCREMENTAL=0 cargo test --workspace --locked` **全绿**（测试名存在 + 全量套件通过；未逐条启动 51 个过滤进程，避免把“存在”误报成“单测独立通过”）。
- **断言强度**：`行为` = 测试断言数值/事件/路由/输出等运行行为；`弱（仅解析/序列化）` = 只验证字段解析、schema、JSON 往返；`无证据` = 行内与台账均没有断言该行为的测试。
- **CLI 复现**：`可` 表示能用现有 CLI 在模型级端到端跑出该行为；`否` 表示需要 GUI/进程内状态或当前 CLI 没有对应命令。命令里的 `<model>` 指最小可运行模型。

## 逐行审计

| # | 功能行 | 引用测试 | 测试通过 | 断言强度 | CLI 复现 | 结论 |
|---:|---|---|---|---|---|---|
| 1 | Flowitems（流动实体） | `source_respects_max_arrivals`; `mm1_throughput_matches_arrival_rate` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 2 | Itemtype（每个流项目自带类型值） | `split_by_item_type_routes_to_the_matching_port` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 3 | Pick Lists（点选式列表） | `picklist_lookup_returns_the_entry_value`; `unknown_picklist_and_unknown_entry_are_distinct_errors` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 4 | Return Values（代码字段返回值语义） | `script_condition_can_return_a_boolean_and_query_objects`; `script_trigger_example_routes_by_script_condition` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 5 | Order of Events（对象固定事件序列） | `setup_and_process_finish_hooks_fire_at_the_phase_boundaries`; `processor_on_entry_and_on_exit_triggers_fire` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 6 | FixedResources（九个固定资源对象） | `multiprocessor_slots_raise_the_throughput`; `breakdown_preempts_processing_and_repairs`; `rack_min_dwell_delays_release`; `combiner_pack_releases_one_container_per_cycle`; `combiner_join_merges_one_item_per_port`; `separator_unpacks_quantity_plus_container`; `separator_splits_into_components_plus_container`; `emptier_flushes_upstream_on_a_schedule` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 7 | NetworkNode / TrafficControl | `traffic_control_mutual_exclusion_waits_and_releases`; `traffic_control_capacity_two_admits_both`; `traffic_control_admissions_survive_checkpoint_restore` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 8 | Statistics Window（实时状态与吞吐面板） | `state_fractions_sum_to_one`; `mm1_matches_analytic_queue_length` | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 9 | Source Page（三种到达风格 + 到达表） | `mm1_throughput_matches_arrival_rate`; `source_respects_max_arrivals`; `source_can_load_its_arrival_schedule_from_a_table` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 10 | Processor Page（setup/process/操作员/抢占） | `breakdown_preempts_processing_and_repairs`; `operator_preempt_interrupts_and_resumes` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 11 | ProcessTimes Page（operator 数与 Same as Setup） | `a10_same_as_setup_reuses_the_setup_crew` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 12 | MultiProcessor Page（顺序工序 + 每步操作员） | `multiprocessor_steps_sum_process_time`; `multiprocessor_steps_include_setup_time`; `multiprocessor_steps_keep_slots_parallel`; `multiprocessor_empty_steps_matches_absent_field`; `multiprocessor_report_hashes_without_steps_are_unchanged`; `multiprocessor_step_waits_for_operator`; `multiprocessor_step_releases_operator_for_next_item`; `multiprocessor_step_waits_for_two_operators`; `multiprocessor_step_operators_are_per_lane`; `multiprocessor_steps_are_deterministic` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 13 | Traffic Control Page（互斥 / 非定时） | `traffic_control_un_timed_applies_speed_only`; `traffic_control_mutual_exclusion_waits_and_releases` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 14 | Fluid Marks Page（低/中/高液位触发） | `fluid_mark_rise_fires_once`; `fluid_mark_fall_fires_once`; `fluid_mark_jitter_counts_each_crossing`; `fluid_mark_and_full_hook_coexist` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 15 | Fluid Steps Page（分批 + 步前后延迟钩子） | `mixer_delay_zero_fires_the_hooks_and_keeps_the_report`; `mixer_delay_hooks_are_one_delay_apart`; `mixer_delay_hooks_fire_per_step` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 16 | Send To Port（7 策略 + Hold） | `a04_hold_keeps_items_until_release`; `percentage_routing_is_respected`; `script_condition_can_return_a_boolean_and_query_objects`; `split_by_item_type_routes_to_the_matching_port`; `split_round_robin_balances_the_outputs`; `s_connection_does_not_consume_an_output_port` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 17 | Pull Strategy | `a03_pull_strategy_round_robin_uses_every_upstream` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 18 | Pull Requirement | `a03_pull_requirement_skips_items` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 19 | Process Time / Setup Time | `setup_and_process_finish_hooks_fire_at_the_phase_boundaries`; `station_holds_items_for_its_process_time` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 20 | Minimum Staytime（Rack） | `rack_min_dwell_delays_release` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 21 | Load / Unload Time | `explicit_load_and_unload_durations_are_exact`; `explicit_chain_with_load_unload_is_deterministic` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 22 | Rise Fall Through Mark Triggers（液位穿越） | `fluid_mark_rise_fires_once`; `fluid_mark_fall_fires_once`; `fluid_mark_jitter_counts_each_crossing` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 23 | Entry Exit Trigger（OnEntry/OnExit） | `processor_on_entry_and_on_exit_triggers_fire` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 24 | Reset Trigger | `on_reset_trigger_runs` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 25 | OnChange Trigger（watchlist 变量变化） | `on_change_fires_once_per_changed_sample`; `on_change_never_fires_for_a_constant`; `on_change_script_runs`; `on_change_on_a_summary_is_rejected` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 26 | Breakdown/Repair Trigger | `repair_trigger_fires`; `breakdown_preempts_processing_and_repairs` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 27 | Down/Up Trigger（MTBF 组级） | `mtbf_group_stops_every_member_once`; `mtbf_group_differs_from_independent_failures`; `mtbf_groups_are_isolated`; `mtbf_group_checkpoint_resumes`; `mtbf_group_is_deterministic` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 28 | Load/Unload Trigger（OnLoad/OnUnload） | `on_load_script_is_precompiled`; `on_load_fires_at_dwell_plus_load`; `on_load_label_is_visible_in_the_unload_hook` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 29 | Setup/Process Finish Trigger | `setup_and_process_finish_hooks_fire_at_the_phase_boundaries` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 30 | OnEmpty / OnFull Trigger（流体满/空） | `fluid_on_full_fires_once`; `fluid_on_empty_fires_once`; `fluid_boundary_hooks_do_not_change_reports` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 31 | OnEntryRequest Trigger（TrafficControl 准入裁决） | `on_entry_request_admit_zero_is_retried_later`; `on_entry_request_without_admit_keeps_the_default_path`; `on_entry_request_script_is_precompiled` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 32 | OnReceiveTaskSequence | `receive_task_sequence_fires_once_per_dispatch`; `receive_task_sequence_hook_can_change_the_model` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 33 | Start/End of Warmup / Run / Scenario / Experiment | `replication_lifecycle_hooks_fire_once_per_run`; `experiment_lifecycle_hooks_run_in_order`; `lifecycle_hook_errors_are_reported`; `schema_covers_lifecycle_hooks` | 是（存在且全量绿） | 行为（含 schema 辅助断言） | 可（模型级）：`openflex run` / `openflex experiment` | 已验证 |
| 34 | Performance Measure（每复现值） | `performance_measure_single_and_multi_replication_stats`; `report_formats_carry_model_measures`; `unknown_performance_measure_variable_is_an_error` | 是（存在且全量绿） | 行为 | 可：`openflex run <model> --format json` | 已验证 |
| 35 | Global Tables | `global_table_function_selects_a_row_with_an_expression`; `set_cell_updates_a_table_in_memory`; `insert_and_remove_rows` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 36 | User Events（按设定时刻执行） | `user_event_runs_once_at_its_time`; `user_events_at_the_same_time_keep_declaration_order`; `user_event_negative_time_is_rejected`; `user_event_past_the_stop_time_does_not_run` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 37 | Model Triggers | `model_triggers_fire_at_start_reset_and_stop`; `model_trigger_errors_are_reported` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 38 | Tracked Variables（四种 kind） | `kinetic_tracked_variable_reports_the_rate_of_change`; `staytime_tracked_variable_reports_the_distribution`; `state_tracked_variable_reports_state_shares` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 39 | Quick Properties（上下文属性面板） | `the_visual_tool_form_round_trips_its_params`; `form_position_round_trips_exactly` | 是（存在且全量绿） | 弱（仅 GUI 表单往返，未断言仿真行为） | 否（GUI 交互） | 证据偏弱，建议补测（UI 行为级） |
| 40 | Find Objects（按名称/类型查找） | `the_tree_search_matches_names_and_kinds` | 是（存在且全量绿） | 行为（GUI 纯函数，未验证面板交互） | 否（GUI 交互） | 已验证（UI 逻辑级） |
| 41 | Reports and Statistics：State Report（状态占比报表） | `state_tracked_variable_reports_state_shares`; `state_fractions_sum_to_one` | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |
| 42 | Reports and Statistics：Model Documentation | `document_describes_the_model_without_running_it` | 是（存在且全量绿） | 行为 | 可：`openflex document <model> --format json` | 已验证 |
| 43 | Experimenter：模型的场景声明（experiments[]） | `returns_one_outcome_per_scenario_with_full_reports`; `a_failing_scenario_does_not_stop_its_siblings` | 是（存在且全量绿） | 行为 | 可：`openflex experiment <model>` | 已验证 |
| 44 | Dashboard Item Trace（单件轨迹） | `groups_steps_per_item_in_time_order`; `traced_items_matches_the_tagged_events`; `item_metrics_count_distinct_objects_and_span` | 是（存在且全量绿） | 行为 | 可：`openflex run --trace <file>` + `openflex trace --item <id> <file>` | 已验证 |
| 45 | Experimenter：Scenarios | `a faster processor must raise throughput`; `experiment_json_contains_every_scenario`; `scenario_stop_time_and_warmup_are_accepted` | 是（存在且全量绿） | 行为（含 schema 辅助断言） | 可：`openflex experiment <model>` | 已验证 |
| 46 | Experimenter：Performance Measures | `report_formats_carry_model_measures`; `experiment_formats_carry_model_measures`; `performance_measure_single_and_multi_replication_stats` | 是（存在且全量绿） | 行为 | 可：`openflex experiment <model>` | 已验证 |
| 47 | Experimenter：Analyzing Results | `compare_reports_the_metric_deltas`; `report_formats_carry_model_measures`; batch.rs 单测; `a12_small_sample_ci_uses_the_t_table` | 是（存在且全量绿） | 行为 | 可：`openflex report --compare ...` / `openflex batch ...` | 已验证 |
| 48 | Table Editor（编辑任意节点表数据） | `table_columns_can_be_renamed`; `table_columns_and_rows_can_grow_and_shrink`; `deleting_a_global_table_removes_it_and_its_buffers` | 是（存在且全量绿） | 弱（仅 GUI 表格往返/数据变更，未断言仿真行为） | 否（GUI 交互） | 证据偏弱，建议补测（UI 行为级） |
| 49 | First Model（三品种加工示例） | `three_varieties_model_keeps_its_shape`; `three_varieties_runs_five_replications_and_the_tester_is_the_bottleneck`; `three_varieties_is_deterministic_across_repeated_runs`（审计发现后在 examples.rs 补齐） | 是（存在且全量绿） | 行为 | 可：`openflex run examples/three-varieties.ofm --format json` | 已验证（审计后补测） |
| 50 | Conveyor（直/弯）、Join Conveyor | `segmented_belt_integrates_speed_per_band`; `segmented_belt_order_matters_for_intermediate_positions`; `join_inputs_must_match_the_connections` | 是（存在且全量绿） | 行为 | 可（模型级）：`openflex run <model> --format json` | 已验证 |


## 汇总（当前矩阵）

- 当前矩阵功能 ✅：**50**（审计开始时 51；Event List 已按发现降级为 🟡）。
- **已验证**：**48**
- **证据偏弱，建议补测**：**2**
  - Quick Properties（第 195 行）：只测 GUI 表单往返，不测仿真行为。
  - Table Editor（第 288 行）：只测 GUI 表格数据变更，不测仿真行为。
- **证据缺失，应降级为 🟡**：**0**（First Model 已由 Lead 补测转 verified）。
- 另：Find Objects 是 GUI 纯函数行为测试，算“已验证（UI 逻辑级）”，不是面板级端到端。

## 审计发现与跟进

1. **Event List（原第 175 行 ✅）——已由 Lead 降级为 🟡**。原引用 `trace_writes_parseable_jsonl_events` / `reads_a_jsonl_trace_file` 只测 trace JSONL 可写/可读，证明不了“待处理事件列表/快进”；矩阵现描述已改为已发生事件的记录/回放，并写明降级理由。
2. **First Model（第 310 行）——审计发现无测试，已补**。Lead 新增 `three_varieties_model_keeps_its_shape`、`three_varieties_runs_five_replications_and_the_tester_is_the_bottleneck`、`three_varieties_is_deterministic_across_repeated_runs`（`crates/openflex-core/tests/examples.rs`），已跑绿；本表按补测后的 ✅ 记为已验证。
3. **Model Triggers 行内文字自相矛盾**：该行说“仍无实验生命周期钩子”，但 Start/End 行已把 9 个生命周期钩子列为 ✅ 并有测试。属于描述 stale，不是证据失败；建议统一措辞。
4. **矩阵/台账/卡计数不一致**：审计开始时矩阵 51 ✅、台账 43 行、卡 51；Event List 降级后矩阵 50。建议后续用同一个脚本生成计数，避免范围漂移。

## CLI 复现说明

- 仿真类行统一可用 `openflex run <model> --format json`；这只是模型级复现，不是对每个引用测试的逐条重放。
- Model Documentation 用 `openflex document <model> --format json`；Item Trace 用 `openflex run --trace <file>` + `openflex trace --item <id> <file>`；Experimenter 用 `openflex experiment <model>` / `openflex report --compare`。
- Quick Properties、Find Objects、Table Editor 属于 GUI 交互；当前 CLI 没有等价的面板操作，故不能端到端复现。
