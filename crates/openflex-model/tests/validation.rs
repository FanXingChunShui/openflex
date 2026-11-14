//! Strict-schema and new-field validation tests (A1, A2, A6, A7, A8, A10, A13).

use openflex_model::Model;

/// Minimal valid model with the given `objects` array body and optional
/// connections. Sources and sinks are present so structural checks pass.
fn model(objects: &str, connections: &str) -> String {
    format!(
        r#"{{
  "format": "openflex-model", "version": 1, "name": "strict-test",
  "run": {{ "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 }},
  "objects": [ {objects} ],
  "connections": [ {connections} ]
}}"#
    )
}

fn source(params: &str) -> String {
    format!(r#"{{ "name": "S", "kind": "source", "params": {{ {params} }} }}"#)
}

fn sink(params: &str) -> String {
    format!(r#"{{ "name": "K", "kind": "sink", "params": {{ {params} }} }}"#)
}

fn load_error(json: &str) -> String {
    Model::from_json(json)
        .expect_err("model should be rejected")
        .to_string()
}

fn validation_error(json: &str) -> String {
    let model = Model::parse_json(json).expect("model should parse");
    model
        .validate()
        .expect_err("model should fail validation")
        .to_string()
}

// ---------------------------------------------------------------------------
// A1: unknown parameters are rejected with the object and key name
// ---------------------------------------------------------------------------

#[test]
fn unknown_source_parameter_is_rejected_with_key_and_object() {
    let error = load_error(&model(
        &source(
            r#""interarrival": { "distribution": "constant", "value": 1.0 }, "inter_arrival": { "distribution": "constant", "value": 2.0 }"#,
        ),
        "",
    ));
    assert!(error.contains("unknown field"), "{error}");
    assert!(error.contains("inter_arrival"), "{error}");
    assert!(error.contains("object 'S'"), "{error}");
}

#[test]
fn unknown_processor_parameter_is_rejected() {
    let error = load_error(&model(
        r#"{ "name": "P", "kind": "processor", "params": { "process_tme": { "distribution": "constant", "value": 1.0 } } }"#,
        "",
    ));
    assert!(error.contains("unknown field"), "{error}");
    assert!(error.contains("process_tme"), "{error}");
}

#[test]
fn unknown_object_key_is_rejected_with_object_name() {
    let error = load_error(&model(
        r#"{ "name": "S", "kind": "source", "label": "nope", "params": {} }"#,
        "",
    ));
    assert!(error.contains("unknown key"), "{error}");
    assert!(error.contains("label"), "{error}");
    assert!(error.contains("object 'S'"), "{error}");
}

#[test]
fn unknown_run_setting_is_rejected() {
    let json = r#"{
      "format": "openflex-model", "version": 1, "name": "bad-run",
      "run": { "stop_time": 10.0, "stop_tme": 5.0 },
      "objects": [ { "name": "S", "kind": "source", "params": {} },
                   { "name": "K", "kind": "sink", "params": {} } ],
      "connections": [ { "from": "S", "to": "K" } ]
    }"#;
    let error = load_error(json);
    assert!(error.contains("unknown field"), "{error}");
    assert!(error.contains("stop_tme"), "{error}");
}

#[test]
fn unknown_connection_key_is_rejected() {
    let json = model(
        &format!("{}, {}", source(""), sink("")),
        r#"{ "from": "S", "to": "K", "weight": 2.0 }"#,
    );
    let error = load_error(&json);
    assert!(error.contains("unknown field"), "{error}");
    assert!(error.contains("weight"), "{error}");
}

#[test]
fn unknown_scenario_key_is_rejected() {
    let json = model(
        &format!("{}, {}", source(""), sink("")),
        r#"{ "from": "S", "to": "K" }"#,
    )
    .replace(
        r#""objects""#,
        r#""experiments": [ { "name": "E", "scenarios": [ { "name": "base", "stopp": 5.0 } ] } ], "objects""#,
    );
    let error = load_error(&json);
    assert!(error.contains("unknown field"), "{error}");
    assert!(error.contains("stopp"), "{error}");
}

#[test]
fn unknown_distribution_parameter_is_rejected() {
    let error = load_error(&model(
        &format!(
            "{}, {}",
            source(
                r#""interarrival": { "distribution": "exponential", "mean": 5.0, "rate": 0.2 }"#,
            ),
            sink("")
        ),
        r#"{ "from": "S", "to": "K" }"#,
    ));
    assert!(error.contains("unknown key"), "{error}");
    assert!(error.contains("rate"), "{error}");
    assert!(error.contains("object 'S'"), "{error}");
}

#[test]
fn unknown_port_strategy_key_is_rejected() {
    let json = model(
        &format!("{}, {}", source(""), sink("")),
        r#"{ "from": "S", "to": "K", "strategy": { "type": "round_robin", "weight": 1.0 } }"#,
    );
    let error = load_error(&json);
    assert!(error.contains("unknown key"), "{error}");
    assert!(error.contains("weight"), "{error}");
}

// ---------------------------------------------------------------------------
// A2: arrival schedule / arrival sequence
// ---------------------------------------------------------------------------

#[test]
fn arrival_schedule_is_accepted() {
    let json = model(
        &format!(
            "{}, {}",
            source(
                r#""arrival_schedule": { "rows": [
                 { "time": 0.0, "quantity": 2 },
                 { "time": 5.0, "quantity": 1, "item_type": 3, "labels": { "priority": 1.0 } }
               ] }"#,
            ),
            sink("")
        ),
        r#"{ "from": "S", "to": "K" }"#,
    );
    let model = Model::from_json(&json).expect("schedule should load");
    let params = model
        .object("S")
        .and_then(|object| object.source().cloned());
    assert_eq!(
        params.map(|params| params.effective_arrival_style()),
        Some(openflex_model::ArrivalStyle::ArrivalSchedule)
    );
}

#[test]
fn arrival_sequence_is_accepted() {
    let json = model(
        &format!(
            "{}, {}",
            source(r#""arrival_sequence": [ { "quantity": 2 }, { "quantity": 3 } ]"#),
            sink("")
        ),
        r#"{ "from": "S", "to": "K" }"#,
    );
    let model = Model::from_json(&json).expect("sequence should load");
    let params = model
        .object("S")
        .and_then(|object| object.source().cloned())
        .expect("source params");
    assert_eq!(
        params.effective_arrival_style(),
        openflex_model::ArrivalStyle::ArrivalSequence
    );
    assert_eq!(params.arrival_sequence().map(|rows| rows.len()), Some(2));
}

#[test]
fn arrival_schedule_and_interarrival_are_mutually_exclusive() {
    let json = model(
        &source(
            r#""interarrival": { "distribution": "constant", "value": 5.0 },
               "arrival_schedule": { "rows": [ { "time": 0.0 } ] }"#,
        ),
        r#"{ "from": "S", "to": "K" }"#,
    );
    let error = validation_error(&json);
    assert!(error.contains("interarrival"), "{error}");
    assert!(
        error.contains("mutually exclusive") || error.contains("cannot be combined"),
        "{error}"
    );
}

#[test]
fn arrival_schedule_and_sequence_are_mutually_exclusive() {
    let json = model(
        &source(
            r#""arrival_schedule": { "rows": [ { "time": 0.0 } ] },
               "arrival_sequence": [ { "quantity": 1 } ]"#,
        ),
        r#"{ "from": "S", "to": "K" }"#,
    );
    let error = validation_error(&json);
    assert!(error.contains("mutually exclusive"), "{error}");
}

#[test]
fn arrival_schedule_times_must_not_decrease() {
    let json = model(
        &source(r#""arrival_schedule": { "rows": [ { "time": 10.0 }, { "time": 5.0 } ] }"#),
        r#"{ "from": "S", "to": "K" }"#,
    );
    let error = validation_error(&json);
    assert!(error.contains("non-decreasing"), "{error}");
}

#[test]
fn empty_arrival_tables_are_rejected() {
    for table in [
        r#""arrival_schedule": { "rows": [] }"#,
        r#""arrival_sequence": []"#,
    ] {
        let json = model(&source(table), r#"{ "from": "S", "to": "K" }"#);
        let error = validation_error(&json);
        assert!(error.contains("at least one row"), "{error}");
    }
}

#[test]
fn arrival_style_must_match_the_table() {
    let json = model(
        &source(r#""arrival_style": "arrival_schedule""#),
        r#"{ "from": "S", "to": "K" }"#,
    );
    let error = validation_error(&json);
    assert!(error.contains("arrival_schedule"), "{error}");
}

// ---------------------------------------------------------------------------
// A6: separator split mode
// ---------------------------------------------------------------------------

#[test]
fn separator_split_requires_a_positive_count() {
    let json = model(
        &format!(
            "{}, {}, {}",
            source(""),
            r#"{ "name": "Sep", "kind": "separator", "params": { "separate_mode": "split", "split_count": 0 } }"#,
            sink("")
        ),
        r#""#,
    );
    let error = validation_error(&json);
    assert!(error.contains("split_count"), "{error}");
}

#[test]
fn separator_split_accepts_positive_count() {
    let json = model(
        &format!(
            "{}, {}, {}",
            source(""),
            r#"{ "name": "Sep", "kind": "separator", "params": { "separate_mode": "split", "split_count": 3 } }"#,
            sink("")
        ),
        r#""#,
    );
    Model::from_json(&json).expect("split separator should validate");
}

// ---------------------------------------------------------------------------
// A7: combiner batch mode + setup/process
// ---------------------------------------------------------------------------

#[test]
fn combiner_batch_mode_is_accepted_with_setup_and_process() {
    let json = model(
        &format!(
            "{}, {}, {}",
            source(""),
            r#"{ "name": "C", "kind": "combiner", "params": {
                 "mode": "batch", "components_per_container": 4,
                 "setup_time": { "distribution": "constant", "value": 2.0 },
                 "process_time": { "distribution": "exponential", "mean": 5.0 },
                 "setup_operator": "Ops", "process_operator": "Ops" } }"#,
            sink("")
        ),
        r#""#,
    );
    Model::from_json(&json).expect("batch combiner should validate");
}

#[test]
fn combiner_setup_time_is_validated() {
    let json = model(
        &format!(
            "{}, {}, {}",
            source(""),
            r#"{ "name": "C", "kind": "combiner", "params": {
                 "mode": "batch",
                 "setup_time": { "distribution": "exponential", "mean": 0.0 } } }"#,
            sink("")
        ),
        r#""#,
    );
    let error = validation_error(&json);
    assert!(error.contains("setup_time"), "{error}");
}

// ---------------------------------------------------------------------------
// A8 / A10: failure clocks and operator parameters
// ---------------------------------------------------------------------------

#[test]
fn failure_parameters_accept_first_failure_and_state_clock() {
    let json = model(
        &format!(
            "{}, {}, {}",
            source(""),
            r#"{ "name": "P", "kind": "processor", "params": {
                 "failure": {
                   "mtbf": { "distribution": "constant", "value": 100.0 },
                   "mttr": { "distribution": "constant", "value": 10.0 },
                   "first_failure_time": -1.0,
                   "count_time_in_states": ["processing"] } } }"#,
            sink("")
        ),
        r#""#,
    );
    Model::from_json(&json).expect("failure params should validate");
}

#[test]
fn failure_state_clock_rejects_unknown_states() {
    let json = model(
        &format!(
            "{}, {}, {}",
            source(""),
            r#"{ "name": "P", "kind": "processor", "params": {
                 "failure": {
                   "mtbf": { "distribution": "constant", "value": 100.0 },
                   "mttr": { "distribution": "constant", "value": 10.0 },
                   "count_time_in_states": ["procrastinating"] } } }"#,
            sink("")
        ),
        r#""#,
    );
    let error = validation_error(&json);
    assert!(error.contains("procrastinating"), "{error}");
}

#[test]
fn operator_counts_are_validated() {
    let json = model(
        &format!(
            "{}, {}, {}",
            source(""),
            r#"{ "name": "P", "kind": "processor", "params": {
                 "num_process_operators": 0 } }"#,
            sink("")
        ),
        r#""#,
    );
    let error = validation_error(&json);
    assert!(error.contains("num_process_operators"), "{error}");
}

#[test]
fn operator_parameters_roundtrip() {
    let json = model(
        &format!(
            "{}, {}, {}",
            source(""),
            r#"{ "name": "P", "kind": "processor", "params": {
                 "setup_operator": "Ops", "process_operator": "Ops",
                 "num_setup_operators": 2, "num_process_operators": 2,
                 "use_same_num_operators": true, "operator_priority": 5,
                 "operator_preempt": true } }"#,
            sink("")
        ),
        r#""#,
    );
    let model = Model::from_json(&json).expect("operator params should validate");
    let processor = model
        .object("P")
        .and_then(|object| object.processor().cloned())
        .expect("processor params");
    assert_eq!(processor.num_setup_operators, Some(2));
    assert_eq!(processor.operator_priority, 5);
    assert!(processor.operator_preempt);
}

// ---------------------------------------------------------------------------
// A13: scenario stop/warmup overrides
// ---------------------------------------------------------------------------

fn experiment_model(scenario: &str) -> String {
    model(
        &format!("{}, {}", source(""), sink("")),
        r#"{ "from": "S", "to": "K" }"#,
    )
    .replace(
        r#""objects""#,
        &format!(
            r#""experiments": [ {{ "name": "E", "scenarios": [ {{ "name": "base", {scenario} }} ] }} ], "objects""#
        ),
    )
}

#[test]
fn scenario_stop_time_and_warmup_are_accepted() {
    let json = experiment_model(r#""stop_time": 50.0, "warmup": 10.0"#);
    let model = Model::from_json(&json).expect("scenario should validate");
    let scenario = &model.experiments[0].scenarios[0];
    assert_eq!(scenario.stop_time, Some(50.0));
    assert_eq!(scenario.warmup, Some(10.0));
}

#[test]
fn scenario_stop_time_must_be_positive() {
    let error = validation_error(&experiment_model(r#""stop_time": 0.0"#));
    assert!(error.contains("stop_time"), "{error}");
}

#[test]
fn scenario_warmup_must_be_shorter_than_stop() {
    let error = validation_error(&experiment_model(r#""stop_time": 20.0, "warmup": 25.0"#));
    assert!(error.contains("warmup"), "{error}");
}

#[test]
fn scenario_warmup_defaults_to_model_run_settings_for_comparison() {
    // The model stops at 100, so a scenario warmup of 150 is impossible even
    // without an explicit scenario stop time.
    let error = validation_error(&experiment_model(r#""warmup": 150.0"#));
    assert!(error.contains("warmup"), "{error}");
}

// ---------------------------------------------------------------------------
// A3 / A4: pull configuration, hold strategy and release_item
// ---------------------------------------------------------------------------

#[test]
fn pull_configuration_parses_and_is_reachable() {
    let json = model(
        &format!(
            "{}, {}",
            source(
                r#""pull": { "enabled": true, "strategy": { "type": "round_robin" },
               "requirement": "item.type == 1" }"#,
            ),
            sink("")
        ),
        r#"{ "from": "S", "to": "K" }"#,
    );
    let model = Model::from_json(&json).expect("pull params should validate");
    let pull = model
        .object("S")
        .and_then(|object| object.pull())
        .expect("pull");
    assert!(pull.enabled);
    assert_eq!(
        pull.strategy,
        Some(openflex_model::PortStrategy::RoundRobin)
    );
    assert!(pull.requirement.is_some());
}

#[test]
fn hold_strategy_and_release_item_action_parse() {
    let json = r#"{
          "format": "openflex-model", "version": 1, "name": "hold",
          "run": { "stop_time": 10.0 },
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "triggers": { "on_exit": [ { "action": "release_item" } ] } } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K", "strategy": { "type": "hold" } } ]
        }"#;
    let parsed = Model::from_json(json).expect("hold strategy and release_item should validate");
    assert_eq!(
        parsed.connections[0].strategy,
        Some(openflex_model::PortStrategy::Hold)
    );
    let triggers = parsed.object("S").expect("source").params.triggers();
    assert_eq!(triggers.on_exit, vec![openflex_model::Action::ReleaseItem]);
}
