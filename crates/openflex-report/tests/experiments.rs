//! Running the experiments a model declares (`run_declared_experiments`).
//!
//! The execution used to live in the CLI. These tests pin the contract the GUI
//! and the CLI both depend on: one outcome per scenario with the *full*
//! `SimReport`, failures that do not cancel their siblings, deterministic
//! parallel results, and three renderings that are byte-for-byte the ones the
//! CLI produced before the extraction (snapshots captured from the CLI).

use std::path::PathBuf;

use openflex_model::Model;
use openflex_report::{
    ExperimentOptions, Format, ScenarioErrorKind, experiment_reports, render_experiment,
    run_declared_experiments, run_declared_experiments_with_options,
};

/// Two experiments × two scenarios, one replication each so it stays fast.
const TWO_BY_TWO: &str = r#"{
  "format": "openflex-model",
  "version": 1,
  "name": "two-by-two",
  "time_unit": "seconds",
  "run": { "stop_time": 20.0, "warmup": 0.0, "seed": 7, "replications": 2 },
  "objects": [
    { "name": "S", "kind": "source", "params": {
        "interarrival": { "distribution": "constant", "value": 3.0 } } },
    { "name": "K", "kind": "sink", "params": {} }
  ],
  "connections": [ { "from": "S", "to": "K" } ],
  "experiments": [
    { "name": "first", "replications": 2, "scenarios": [
      { "name": "a" },
      { "name": "b" }
    ] },
    { "name": "second", "replications": 2, "scenarios": [
      { "name": "c", "overrides": [
        { "object": "S", "param": "interarrival.value", "value": 2.0 } ] },
      { "name": "d", "overrides": [
        { "object": "S", "param": "interarrival.value", "value": 1.0 } ] }
    ] }
  ]
}
"#;

/// One experiment whose middle scenario has an override that cannot apply.
const BROKEN_SCENARIO: &str = r#"{
  "format": "openflex-model",
  "version": 1,
  "name": "broken",
  "time_unit": "seconds",
  "run": { "stop_time": 20.0, "warmup": 0.0, "seed": 7, "replications": 1 },
  "objects": [
    { "name": "S", "kind": "source", "params": {
        "interarrival": { "distribution": "constant", "value": 3.0 } } },
    { "name": "K", "kind": "sink", "params": {} }
  ],
  "connections": [ { "from": "S", "to": "K" } ],
  "experiments": [
    { "name": "mixed", "replications": 1, "scenarios": [
      { "name": "ok" },
      { "name": "bad", "overrides": [
        { "object": "S", "param": "interarrival.value", "value": "not-a-number" } ] },
      { "name": "after" }
    ] }
  ]
}
"#;

/// Model used for the rendering snapshots; its reports were captured from the
/// CLI before the execution loop moved into this crate.
const METRIC_MODEL: &str = include_str!("fixtures/metric.ofm");
const TABLE_SNAPSHOT: &str = include_str!("fixtures/experiment-table.txt");
const CSV_SNAPSHOT: &str = include_str!("fixtures/experiment.csv");
const MARKDOWN_SNAPSHOT: &str = include_str!("fixtures/experiment.md");

fn parse_model(json: &str) -> Model {
    Model::from_json(json).expect("fixture model")
}

fn temp_dir(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "openflex-experiments-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&path).expect("temp dir");
    path
}

#[test]
fn returns_one_outcome_per_scenario_with_full_reports() {
    let model = parse_model(TWO_BY_TWO);
    let outcomes = run_declared_experiments(&model, false);

    assert_eq!(outcomes.len(), 4, "2 experiments x 2 scenarios");
    let names: Vec<(&str, &str)> = outcomes
        .iter()
        .map(|outcome| (outcome.experiment.as_str(), outcome.scenario.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("first", "a"),
            ("first", "b"),
            ("second", "c"),
            ("second", "d")
        ],
        "declaration order must be preserved"
    );
    for outcome in &outcomes {
        assert!(outcome.succeeded(), "{outcome:?}");
        assert!(outcome.error.is_none());
        let report = outcome.report().expect("full report");
        // The GUI needs more than a summary: objects, aggregates and the run
        // settings all have to be present.
        assert!(!report.objects.is_empty(), "objects are missing");
        assert!(report.created.n > 0);
        assert_eq!(
            report.replications.len(),
            2,
            "experiment replications applied"
        );
        assert_eq!(report.model, "two-by-two");
    }
    // The scenario overrides really reached the engine: the two scenarios of
    // `second` only differ in their inter-arrival time, so their throughput
    // must differ.
    assert_ne!(
        outcomes[2].report().expect("c").created.mean,
        outcomes[3].report().expect("d").created.mean,
        "the scenario override did not reach the engine"
    );
}

#[test]
fn a_failing_scenario_does_not_stop_its_siblings() {
    let model = parse_model(BROKEN_SCENARIO);
    let outcomes = run_declared_experiments(&model, false);

    assert_eq!(outcomes.len(), 3, "every scenario still reports");
    assert!(outcomes[0].succeeded(), "the first scenario runs");
    let failed = &outcomes[1];
    assert_eq!(failed.scenario, "bad");
    assert!(failed.report.is_none(), "a failed scenario has no report");
    assert_eq!(failed.error_kind, Some(ScenarioErrorKind::Usage));
    let message = failed.error.as_deref().unwrap_or_default();
    assert!(
        message.contains("experiment mixed scenario bad"),
        "{message}"
    );
    assert!(
        message.contains("not-a-number") || message.contains("invalid"),
        "{message}"
    );
    assert!(
        outcomes[2].succeeded(),
        "the scenario after the failure still runs"
    );
}

#[test]
fn parallel_and_serial_runs_agree() {
    let model = parse_model(TWO_BY_TWO);
    let serial = run_declared_experiments(&model, false);
    let parallel = run_declared_experiments(&model, true);

    assert_eq!(serial.len(), parallel.len());
    for (serial, parallel) in serial.iter().zip(&parallel) {
        assert_eq!(serial.experiment, parallel.experiment);
        assert_eq!(serial.scenario, parallel.scenario);
        assert_eq!(
            serial.report, parallel.report,
            "parallel must reproduce the serial report bit for bit"
        );
    }
}

#[test]
fn options_select_one_experiment_and_write_traces() {
    let model = parse_model(TWO_BY_TWO);
    let root = temp_dir("trace");

    let options = ExperimentOptions {
        parallel: None,
        experiment: Some("second".to_string()),
        trace_root: Some(root.clone()),
    };
    let outcomes = run_declared_experiments_with_options(&model, &options);
    assert_eq!(outcomes.len(), 2, "only the selected experiment ran");
    assert!(
        outcomes
            .iter()
            .all(|outcome| outcome.experiment == "second")
    );

    // Two replications => two trace files per scenario, in the documented
    // `<experiment>/<scenario>/` layout.
    for scenario in ["c", "d"] {
        let directory = root.join("second").join(scenario);
        assert!(
            directory.join("trace.rep0.jsonl").is_file(),
            "{directory:?}"
        );
        assert!(
            directory.join("trace.rep1.jsonl").is_file(),
            "{directory:?}"
        );
    }
    let first = outcomes[0]
        .trace_files
        .first()
        .expect("trace files recorded");
    let trace = openflex_report::read_trace_file(first).expect("trace is readable JSONL");
    assert!(!trace.events.is_empty(), "the trace has events");
    assert!(
        trace
            .events
            .iter()
            .any(|event| event.data.contains_key("item")),
        "the trace carries item ids"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn experiment_reports_group_the_successful_outcomes() {
    let model = parse_model(TWO_BY_TWO);
    let outcomes = run_declared_experiments(&model, false);
    let reports = experiment_reports(&outcomes, &model);

    assert_eq!(reports.len(), 2);
    assert_eq!(reports[0].experiment, "first");
    assert_eq!(reports[0].scenarios.len(), 2);
    assert_eq!(reports[1].experiment, "second");
    assert_eq!(reports[1].scenarios[1].name, "d");
    // Grouping keeps the same reports the outcomes carry.
    assert_eq!(
        Some(&reports[0].scenarios[0].report),
        outcomes[0].report.as_ref()
    );

    // A failed scenario is absent from the grouped report but still visible in
    // the outcomes, which is what a host shows as "3 of 4 ran".
    let broken_model = parse_model(BROKEN_SCENARIO);
    let outcomes = run_declared_experiments(&broken_model, false);
    let reports = experiment_reports(&outcomes, &broken_model);
    assert_eq!(reports.len(), 1);
    assert_eq!(
        reports[0].scenarios.len(),
        2,
        "the failed scenario is skipped"
    );
    assert_eq!(outcomes.iter().filter(|o| !o.succeeded()).count(), 1);
}

/// The three renderings must be byte-for-byte what the CLI printed before the
/// execution loop moved out of it. The snapshots were captured from
/// `openflex experiment` on this exact model.
#[test]
fn renderings_match_the_pre_extraction_cli_output() {
    let model = parse_model(METRIC_MODEL);
    let outcomes = run_declared_experiments(&model, false);
    let reports = experiment_reports(&outcomes, &model);
    assert_eq!(reports.len(), 1);

    let table = render_experiment(&reports[0], Format::Table).expect("table");
    assert_eq!(table, TABLE_SNAPSHOT, "table output changed");

    let csv = render_experiment(&reports[0], Format::Csv).expect("csv");
    assert_eq!(csv, CSV_SNAPSHOT, "csv output changed");

    let markdown = render_experiment(&reports[0], Format::Markdown).expect("markdown");
    assert_eq!(markdown, MARKDOWN_SNAPSHOT, "markdown output changed");
}

#[test]
fn a_model_without_experiments_returns_no_outcomes() {
    let model = parse_model(
        r#"{
          "format": "openflex-model", "version": 1, "name": "plain",
          "run": { "stop_time": 5.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {} },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
    );
    assert!(run_declared_experiments(&model, false).is_empty());
    assert!(run_declared_experiments(&model, true).is_empty());
}
