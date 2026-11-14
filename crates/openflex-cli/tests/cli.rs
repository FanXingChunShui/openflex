//! End-to-end tests for the `openflex` binary.
//!
//! Every test drives the real executable (`CARGO_BIN_EXE_openflex`) in a
//! throw-away directory, so the tests cover argument parsing, file handling,
//! exit codes and the JSON contract at the same time.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BIN: &str = env!("CARGO_BIN_EXE_openflex");

/// Small two-object model: fast enough to run in every test.
const MODEL: &str = r#"{
  "format": "openflex-model",
  "version": 1,
  "name": "demo",
  "description": "test fixture",
  "time_unit": "seconds",
  "run": { "stop_time": 200.0, "warmup": 0.0, "seed": 5, "replications": 1 },
  "objects": [
    { "name": "Source1", "kind": "source", "params": {
        "interarrival": { "distribution": "exponential", "mean": 4.0 } } },
    { "name": "Queue1", "kind": "queue", "params": {} },
    { "name": "Processor1", "kind": "processor", "params": {
        "process_time": { "distribution": "exponential", "mean": 3.0 } } },
    { "name": "Sink1", "kind": "sink", "params": {} }
  ],
  "connections": [
    { "from": "Source1", "to": "Queue1" },
    { "from": "Queue1", "to": "Processor1" },
    { "from": "Processor1", "to": "Sink1" }
  ]
}
"#;

/// Model with an experiment that has a baseline and a faster scenario.
const EXPERIMENT_MODEL: &str = r#"{
  "format": "openflex-model",
  "version": 1,
  "name": "experiment-demo",
  "time_unit": "minutes",
  "run": { "stop_time": 120.0, "warmup": 0.0, "seed": 11, "replications": 1 },
  "objects": [
    { "name": "Source1", "kind": "source", "params": {
        "interarrival": { "distribution": "exponential", "mean": 4.0 } } },
    { "name": "Processor1", "kind": "processor", "params": {
        "process_time": { "distribution": "exponential", "mean": 3.0 } } },
    { "name": "Sink1", "kind": "sink", "params": {} }
  ],
  "connections": [
    { "from": "Source1", "to": "Processor1" },
    { "from": "Processor1", "to": "Sink1" }
  ],
  "experiments": [
    { "name": "process-time", "replications": 2, "scenarios": [
      { "name": "baseline", "overrides": [] },
      { "name": "faster", "overrides": [
        { "object": "Processor1", "param": "process_time",
          "value": { "distribution": "exponential", "mean": 1.5 } } ] }
    ] }
  ]
}
"#;

/// A directory that removes itself, keeping synthetic runs out of `/tmp`.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("openflex-cli-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).expect("could not create the test directory");
        Self { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.join(name);
        fs::write(&path, contents).expect("could not write the test fixture");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn openflex(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("could not start the openflex binary")
}

fn path_str(path: &Path) -> String {
    path.to_str().expect("test paths are UTF-8").to_string()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn code(output: &Output) -> i32 {
    output
        .status
        .code()
        .expect("process ended without an exit code")
}

fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).expect("command did not print valid JSON")
}

#[test]
fn validate_accepts_a_valid_model() {
    let dir = TempDir::new("validate-ok");
    let model = dir.write("demo.ofm", MODEL);

    let output = openflex(&["validate", &path_str(&model)]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("ok: demo"),
        "stdout: {}",
        stdout(&output)
    );
    assert!(stdout(&output).contains("4 objects"));
}

#[test]
fn validate_rejects_broken_json_with_exit_code_two() {
    let dir = TempDir::new("validate-json");
    // Missing closing brackets: serde_json reports the line and column.
    let model = dir.write(
        "broken.ofm",
        "{\n  \"format\": \"openflex-model\",\n  \"name\": \"broken\",\n",
    );

    let output = openflex(&["validate", &path_str(&model)]);
    assert_eq!(code(&output), 2, "stderr: {}", stderr(&output));
    let message = stderr(&output);
    assert!(message.contains("invalid model"), "stderr: {message}");
    assert!(
        message.contains("broken.ofm"),
        "the file path must be reported: {message}"
    );
    assert!(
        message.contains("line"),
        "the line number must be reported: {message}"
    );
}

#[test]
fn validate_rejects_an_invalid_connection_with_exit_code_two() {
    let dir = TempDir::new("validate-rules");
    let model = dir.write(
        "invalid.ofm",
        r#"{
  "format": "openflex-model", "version": 1, "name": "bad",
  "objects": [ { "name": "Source1", "kind": "source" }, { "name": "Sink1", "kind": "sink" } ],
  "connections": [ { "from": "Source1", "to": "Nope" } ]
}"#,
    );

    let output = openflex(&["validate", &path_str(&model)]);
    assert_eq!(code(&output), 2, "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("Nope"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn run_json_report_is_parseable() {
    let dir = TempDir::new("run-json");
    let model = dir.write("demo.ofm", MODEL);

    let output = openflex(&[
        "run",
        &path_str(&model),
        "--stop-time",
        "100",
        "--replications",
        "3",
        "--format",
        "json",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));

    let report = json(&stdout(&output));
    assert_eq!(report["model"], "demo");
    assert_eq!(report["replications"].as_array().map(Vec::len), Some(3));
    assert!(report["objects"]["Queue1"].is_object());
    assert_eq!(report["created"]["n"], 3);
    assert!(report["stop_time"].as_f64().unwrap_or_default() > 0.0);
}

#[test]
fn repeated_seed_flags_run_one_replication_per_seed() {
    let dir = TempDir::new("run-seeds");
    let model = dir.write("demo.ofm", MODEL);

    let output = openflex(&[
        "run",
        &path_str(&model),
        "--seed",
        "1",
        "--seed",
        "2",
        "--format",
        "json",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));

    let report = json(&stdout(&output));
    let replications = report["replications"]
        .as_array()
        .expect("replications array");
    assert_eq!(replications.len(), 2);
    assert_ne!(replications[0]["seed"], replications[1]["seed"]);
}

#[test]
fn run_writes_reports_and_a_replications_csv() {
    let dir = TempDir::new("run-reports");
    let model = dir.write("demo.ofm", MODEL);
    let reports = dir.join("reports");

    let output = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "2",
        "--report-dir",
        &path_str(&reports),
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));

    for name in ["report.json", "report.csv", "report.md", "replications.csv"] {
        assert!(reports.join(name).is_file(), "{name} was not written");
    }
    let samples = fs::read_to_string(reports.join("replications.csv")).expect("read csv");
    // Header plus four objects times two replications.
    assert_eq!(samples.lines().count(), 1 + 4 * 2);
    assert!(
        stderr(&output).contains("wrote"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn report_rerenders_a_saved_json_report() {
    let dir = TempDir::new("report-formats");
    let model = dir.write("demo.ofm", MODEL);
    let reports = dir.join("reports");
    assert_eq!(
        code(&openflex(&[
            "run",
            &path_str(&model),
            "--report-dir",
            &path_str(&reports)
        ])),
        0
    );
    let saved = reports.join("report.json");

    let csv = openflex(&["report", &path_str(&saved), "--format", "csv"]);
    assert_eq!(code(&csv), 0, "stderr: {}", stderr(&csv));
    assert!(stdout(&csv).contains("object,kind"));
    assert!(stdout(&csv).contains("Queue1"));

    let markdown = openflex(&["report", &path_str(&saved), "--format", "markdown"]);
    assert_eq!(code(&markdown), 0, "stderr: {}", stderr(&markdown));
    assert!(stdout(&markdown).contains("# OpenFlex report: demo"));
    assert!(stdout(&markdown).contains("How to reproduce"));

    let table = openflex(&["report", &path_str(&saved)]);
    assert_eq!(code(&table), 0);
    assert!(stdout(&table).contains("Items created"));

    // Re-rendering as JSON must reproduce the archived report exactly.
    let reencoded = openflex(&["report", &path_str(&saved), "--format", "json"]);
    assert_eq!(code(&reencoded), 0, "stderr: {}", stderr(&reencoded));
    let archived = fs::read_to_string(&saved).expect("read archived report");
    assert_eq!(json(&stdout(&reencoded)), json(&archived));
}

#[test]
fn report_rejects_malformed_json_with_a_line_number() {
    let dir = TempDir::new("report-broken");
    let path = dir.write("report.json", "{\n  \"model\": \"demo\",\n");

    let output = openflex(&["report", &path_str(&path), "--format", "table"]);
    assert_eq!(code(&output), 1, "stderr: {}", stderr(&output));
    let message = stderr(&output);
    assert!(
        message.contains("report.json"),
        "the path must be reported: {message}"
    );
    assert!(
        message.contains("line"),
        "the line number must be reported: {message}"
    );
}

#[test]
fn report_writes_to_an_output_file() {
    let dir = TempDir::new("report-output");
    let model = dir.write("demo.ofm", MODEL);
    let reports = dir.join("reports");
    assert_eq!(
        code(&openflex(&[
            "run",
            &path_str(&model),
            "--report-dir",
            &path_str(&reports)
        ])),
        0
    );
    let output_path = dir.join("summary.md");

    let output = openflex(&[
        "report",
        &path_str(&reports.join("report.json")),
        "--format",
        "markdown",
        "--output",
        &path_str(&output_path),
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).is_empty(),
        "nothing must go to stdout: {}",
        stdout(&output)
    );

    let text = fs::read_to_string(&output_path).expect("summary.md was not written");
    assert!(text.contains("# OpenFlex report: demo"));
}

#[test]
fn experiment_json_contains_every_scenario() {
    let dir = TempDir::new("experiment-json");
    let model = dir.write("experiment.ofm", EXPERIMENT_MODEL);

    let output = openflex(&["experiment", &path_str(&model), "--format", "json"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));

    let experiment = json(&stdout(&output));
    assert_eq!(experiment["experiment"], "process-time");
    let scenarios = experiment["scenarios"].as_array().expect("scenarios array");
    assert_eq!(scenarios.len(), 2);
    assert_eq!(scenarios[0]["name"], "baseline");
    assert_eq!(scenarios[1]["name"], "faster");
    assert_eq!(scenarios[1]["overrides"].as_array().map(Vec::len), Some(1));
    for scenario in scenarios {
        assert_eq!(
            scenario["report"]["replications"].as_array().map(Vec::len),
            Some(2),
            "each scenario runs the experiment replications"
        );
        assert!(scenario["report"]["objects"].is_object());
    }
}

#[test]
fn report_reads_an_experiment_directory() {
    let dir = TempDir::new("experiment-readback");
    let model = dir.write("experiment.ofm", EXPERIMENT_MODEL);
    let reports = dir.join("experiments");

    let experiment = openflex(&[
        "experiment",
        &path_str(&model),
        "--report-dir",
        &path_str(&reports),
        "--quiet",
    ]);
    assert_eq!(code(&experiment), 0, "stderr: {}", stderr(&experiment));
    assert!(
        reports
            .join("process-time")
            .join("experiment.json")
            .is_file()
    );

    let readback = openflex(&[
        "report",
        "--experiment",
        &path_str(&reports),
        "--format",
        "json",
    ]);
    assert_eq!(code(&readback), 0, "stderr: {}", stderr(&readback));
    let experiment = json(&stdout(&readback));
    assert_eq!(experiment["experiment"], "process-time");
    assert_eq!(experiment["scenarios"].as_array().map(Vec::len), Some(2));
}

#[test]
fn compare_reports_the_metric_deltas() {
    let dir = TempDir::new("compare");
    let model = dir.write("demo.ofm", MODEL);
    let baseline = dir.join("baseline");
    let candidate = dir.join("candidate");

    let first = openflex(&[
        "run",
        &path_str(&model),
        "--report-dir",
        &path_str(&baseline),
    ]);
    assert_eq!(code(&first), 0, "stderr: {}", stderr(&first));
    let second = openflex(&[
        "run",
        &path_str(&model),
        "--set",
        "Processor1.process_time.mean=1.5",
        "--report-dir",
        &path_str(&candidate),
    ]);
    assert_eq!(code(&second), 0, "stderr: {}", stderr(&second));

    let output = openflex(&[
        "compare",
        &path_str(&baseline.join("report.json")),
        &path_str(&candidate.join("report.json")),
        "--format",
        "json",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));

    let comparison = json(&stdout(&output));
    assert!(comparison["baseline"]["path"].is_string());
    let objects = comparison["objects"].as_array().expect("objects array");
    let processor = objects
        .iter()
        .find(|object| object["object"] == "Processor1")
        .expect("Processor1 in the comparison");
    let metrics = processor["metrics"].as_array().expect("metrics array");
    let names: Vec<&str> = metrics
        .iter()
        .filter_map(|metric| metric["metric"].as_str())
        .collect();
    for expected in ["throughput", "utilization", "avg_wip", "avg_stay_time"] {
        assert!(
            names.contains(&expected),
            "missing metric {expected} in {names:?}"
        );
    }
    let throughput = metrics
        .iter()
        .find(|metric| metric["metric"] == "throughput")
        .expect("throughput metric");
    assert!(
        throughput["delta"].is_f64(),
        "delta must be present: {throughput}"
    );
    assert!(
        throughput["delta"].as_f64().unwrap_or_default() > 0.0,
        "a faster processor must raise throughput: {throughput}"
    );
}

#[test]
fn compare_missing_file_exits_with_one() {
    let dir = TempDir::new("compare-missing");
    let model = dir.write("demo.ofm", MODEL);
    let reports = dir.join("reports");
    assert_eq!(
        code(&openflex(&[
            "run",
            &path_str(&model),
            "--report-dir",
            &path_str(&reports)
        ])),
        0
    );

    let output = openflex(&[
        "compare",
        &path_str(&reports.join("report.json")),
        &path_str(&dir.join("does-not-exist.json")),
    ]);
    assert_eq!(code(&output), 1, "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("does-not-exist.json"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn quiet_suppresses_progress_messages_but_not_the_report() {
    let dir = TempDir::new("quiet");
    let model = dir.write("demo.ofm", MODEL);
    let reports = dir.join("reports");

    let output = openflex(&[
        "run",
        &path_str(&model),
        "--report-dir",
        &path_str(&reports),
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    // `--quiet` silences the CLI's own progress notes; diagnostics from other
    // crates in the same process are out of its control, so only the "wrote"
    // line is asserted to be gone.
    assert!(
        !stderr(&output).contains("wrote"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(
        stdout(&output).contains("OpenFlex report"),
        "stdout: {}",
        stdout(&output)
    );
}

#[test]
fn new_scaffolds_a_valid_model() {
    let dir = TempDir::new("new");
    let project = dir.join("project");

    let output = openflex(&["new", &path_str(&project), "--template", "mm1", "--quiet"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(project.join("mm1.ofm").is_file());
    assert!(stdout(&output).contains("created"));

    let validate = openflex(&["validate", &path_str(&project.join("mm1.ofm"))]);
    assert_eq!(code(&validate), 0, "stderr: {}", stderr(&validate));
}

#[test]
fn unknown_subcommands_and_missing_arguments_exit_with_two() {
    let unknown = openflex(&["frobnicate"]);
    assert_eq!(code(&unknown), 2);

    let no_arguments = openflex(&["report"]);
    assert_eq!(code(&no_arguments), 2, "stderr: {}", stderr(&no_arguments));
}

#[test]
fn no_color_is_accepted() {
    let dir = TempDir::new("no-color");
    let model = dir.write("demo.ofm", MODEL);

    let output = openflex(&[
        "run",
        &path_str(&model),
        "--stop-time",
        "50",
        "--no-color",
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
}
