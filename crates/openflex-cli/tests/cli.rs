//! End-to-end tests for the `openflex` binary.
//!
//! Every test drives the real executable (`CARGO_BIN_EXE_openflex`) in a
//! throw-away directory, so the tests cover argument parsing, file handling,
//! exit codes and the JSON contract at the same time.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

/// Process-flow model: 10 tokens through Create -> Delay -> Assign -> Decision
/// -> two sinks. Mirrors `examples/objects/process-flow-token.ofm`.
const FLOW_MODEL: &str = r#"{
  "format": "openflex-model",
  "version": 1,
  "name": "flow-demo",
  "time_unit": "seconds",
  "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
  "objects": [],
  "process_flow": {
    "blocks": [
      { "id": "Create1", "kind": "create", "first_arrival": 0.0,
        "interarrival": { "distribution": "constant", "value": 1000.0 },
        "quantity": 10, "max_creations": 1 },
      { "id": "Delay1", "kind": "delay", "time": { "distribution": "constant", "value": 2.0 } },
      { "id": "Assign1", "kind": "assign", "actions": [
          { "action": "set_label", "name": "urgent", "value": "item.id % 2" } ] },
      { "id": "Decide", "kind": "decision", "conditions": [
          { "condition": "item.label.urgent == 1", "port": 0 },
          { "condition": "item.label.urgent == 0", "port": 1 } ] },
      { "id": "SinkUrgent", "kind": "sink" },
      { "id": "SinkNormal", "kind": "sink" }
    ],
    "connections": [
      { "from": "Create1", "to": "Delay1" },
      { "from": "Delay1", "to": "Assign1" },
      { "from": "Assign1", "to": "Decide" },
      { "from": "Decide", "to": "SinkUrgent", "port": 0 },
      { "from": "Decide", "to": "SinkNormal", "port": 1 }
    ]
  }
}
"#;

/// Fluid line: generator -> tank -> valve -> sink, matching
/// `examples/objects/fluid-line.ofm`.
const FLUID_MODEL: &str = r#"{
  "format": "openflex-model",
  "version": 1,
  "name": "fluid-demo",
  "time_unit": "seconds",
  "run": { "stop_time": 60.0, "warmup": 0.0, "seed": 1, "replications": 1 },
  "fluid_step": 0.1,
  "objects": [
    { "name": "Generator1", "kind": "fluid_generator", "pos": [0, 0], "params": { "flow_rate": 10.0 } },
    { "name": "Tank1", "kind": "fluid_tank", "pos": [5, 0], "params": { "capacity": 100.0, "initial_level": 0.0, "max_outflow": 5.0 } },
    { "name": "Valve1", "kind": "fluid_valve", "pos": [10, 0], "params": { "open": true, "ratio": 0.5 } },
    { "name": "Sink1", "kind": "fluid_sink", "pos": [15, 0], "params": {} }
  ],
  "connections": [
    { "from": "Generator1", "to": "Tank1" },
    { "from": "Tank1", "to": "Valve1" },
    { "from": "Valve1", "to": "Sink1" }
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

/// Run the binary with extra environment variables (used for `OPENFLEX_LANG`).
fn openflex_env(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(BIN);
    command.args(args);
    for (key, value) in env {
        command.env(key, value);
    }
    command
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

// ---------------------------------------------------------------------------
// C4: completions, progress, structured errors, layout, serve
// ---------------------------------------------------------------------------

#[test]
fn completions_emit_a_script_for_every_supported_shell() {
    for shell in ["bash", "zsh", "fish", "powershell"] {
        let output = openflex(&["completions", shell]);
        assert_eq!(code(&output), 0, "{shell}: {}", stderr(&output));
        let script = stdout(&output);
        assert!(
            script.contains("openflex"),
            "{shell} completions do not mention the binary"
        );
        assert!(
            script.len() > 200,
            "{shell} completions look truncated: {} bytes",
            script.len()
        );
    }

    let unknown = openflex(&["completions", "tcsh"]);
    assert_eq!(code(&unknown), 2, "unknown shells are a usage error");
}

#[test]
fn progress_goes_to_stderr_and_keeps_stdout_parseable() {
    let dir = TempDir::new("progress");
    let model = dir.write("demo.ofm", MODEL);

    let output = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "4",
        "--parallel",
        "2",
        "--progress",
        "--format",
        "json",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    // The report on stdout must stay pure JSON.
    let report = json(&stdout(&output));
    assert_eq!(report["replications"].as_array().map(Vec::len), Some(4));
    let progress = stderr(&output);
    assert!(
        progress.contains("replication"),
        "progress must be reported on stderr: {progress}"
    );

    // --quiet disables progress, exactly like the other notes.
    let quiet = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "2",
        "--progress",
        "--quiet",
        "--format",
        "json",
    ]);
    assert_eq!(code(&quiet), 0, "stderr: {}", stderr(&quiet));
    assert!(
        !stderr(&quiet).contains("replication"),
        "stderr: {}",
        stderr(&quiet)
    );
    json(&stdout(&quiet));
}

#[test]
fn json_output_turns_errors_into_structured_objects() {
    let dir = TempDir::new("json-error");
    let model = dir.write(
        "broken.ofm",
        "{\n  \"format\": \"openflex-model\",\n  \"name\": \"broken\",\n",
    );

    let output = openflex(&["run", &path_str(&model), "--format", "json"]);
    assert_eq!(code(&output), 2, "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).is_empty(),
        "an error must not print a report: {}",
        stdout(&output)
    );

    let error = json(stderr(&output).trim());
    assert_eq!(error["error"]["code"], "model_invalid");
    let message = error["error"]["message"]
        .as_str()
        .expect("error message must be a string");
    assert!(message.contains("invalid model"), "message: {message}");
    assert!(
        error["error"]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("broken.ofm")),
        "the path must be part of the error object: {error}"
    );
}

#[test]
fn error_format_json_flag_works_without_format_json() {
    let dir = TempDir::new("error-format");
    let model = dir.write("broken.ofm", "{\n  \"name\": \"broken\",\n");

    let output = openflex(&["validate", &path_str(&model), "--error-format", "json"]);
    assert_eq!(code(&output), 2, "stderr: {}", stderr(&output));
    let error = json(stderr(&output).trim());
    assert_eq!(error["error"]["code"], "model_invalid");

    // The default remains a human-readable prefix.
    let text = openflex(&["validate", &path_str(&model)]);
    assert_eq!(code(&text), 2);
    assert!(
        stderr(&text).starts_with("error:"),
        "stderr: {}",
        stderr(&text)
    );
}

#[test]
fn layout_writes_an_svg_with_object_names_and_connections() {
    let dir = TempDir::new("layout");
    let model = dir.write("demo.ofm", MODEL);
    let svg = dir.join("layout.svg");

    let output = openflex(&[
        "layout",
        &path_str(&model),
        "--svg",
        &path_str(&svg),
        "--width",
        "900",
        "--height",
        "600",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).is_empty(),
        "--svg must write to the file, not stdout"
    );

    let text = fs::read_to_string(&svg).expect("layout.svg was not written");
    assert!(text.contains("<svg"), "not an SVG document");
    assert!(text.contains("width=\"900\""));
    assert!(text.contains("height=\"600\""));
    for name in ["Source1", "Queue1", "Processor1", "Sink1"] {
        assert!(text.contains(name), "missing object {name} in the SVG");
    }
    assert!(text.contains("class=\"link\""), "connections must be drawn");

    let to_stdout = openflex(&["layout", &path_str(&model)]);
    assert_eq!(code(&to_stdout), 0, "stderr: {}", stderr(&to_stdout));
    assert!(stdout(&to_stdout).starts_with("<?xml"));
}

#[test]
fn dry_run_validates_and_describes_the_run_without_simulating() {
    let dir = TempDir::new("dry-run");
    let model = dir.write("demo.ofm", MODEL);

    let output = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "3",
        "--parallel",
        "2",
        "--dry-run",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("dry run: demo"), "stdout: {text}");
    assert!(text.contains("replications 3"), "stdout: {text}");
    assert!(text.contains("workers: 2"), "stdout: {text}");
}

/// A child process that is killed when the test ends, even on panic.
struct ServerChild {
    child: Child,
}

impl Drop for ServerChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn connect_with_retry(address: &str) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match TcpStream::connect(address) {
            Ok(stream) => return stream,
            Err(error) => {
                if Instant::now() >= deadline {
                    panic!("could not connect to the server: {error}");
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }
}

fn http_request(address: &str, method: &str, path: &str, body: Option<&str>) -> String {
    let body = body.unwrap_or_default();
    for attempt in 0..3 {
        let mut stream = connect_with_retry(address);
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("request");
        let _ = stream.shutdown(std::net::Shutdown::Write);
        let mut response = String::new();
        match stream.read_to_string(&mut response) {
            Ok(_) => return response,
            Err(error) if attempt < 2 => {
                std::thread::sleep(Duration::from_millis(50));
                let _ = error;
            }
            Err(error) => panic!("could not read the response: {error}"),
        }
    }
    unreachable!("the loop always returns or panics")
}

/// Send headers that claim a large body without actually sending it, so the
/// server's header-only size check can be tested without a multi-megabyte write.
fn http_oversized(address: &str, method: &str, path: &str, content_length: usize) -> String {
    let mut stream = connect_with_retry(address);
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {content_length}\r\nConnection: close\r\n\r\n"
    )
    .expect("request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("could not read the response");
    response
}

fn http_get(address: &str, path: &str) -> String {
    http_request(address, "GET", path, None)
}

fn http_status(response: &str) -> &str {
    response.lines().next().unwrap_or_default()
}

fn http_body(response: &str) -> &str {
    response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or_default()
}

/// Start `openflex studio --port 0 --no-open` and return its address.
///
/// stderr is consumed by a background thread for the whole lifetime of the
/// process: if the pipe were closed after reading the first line, the server's
/// next `eprintln!` would raise SIGPIPE and kill it on Linux.
fn start_studio(model: &Path) -> (ServerChild, String) {
    let mut child = Command::new(BIN)
        .args(["studio", &path_str(model), "--port", "0", "--no-open"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("could not start `openflex studio`");
    let stderr = child.stderr.take().expect("piped stderr");
    let (sender, receiver) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            let _ = sender.send(line);
        }
    });
    let first = receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("studio did not print its address");
    let server = ServerChild { child };
    let url = first
        .split_once("listening on ")
        .map(|(_, rest)| rest.split_whitespace().next().unwrap_or_default())
        .unwrap_or_else(|| panic!("no studio URL in {first:?}"));
    let address = url
        .strip_prefix("http://")
        .unwrap_or_else(|| panic!("unexpected studio URL {url:?}"))
        .to_string();
    (server, address)
}

#[test]
fn serve_exposes_health_and_objects_on_localhost() {
    let mut child = Command::new(BIN)
        .args(["serve", "--port", "0"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("could not start `openflex serve`");
    let stderr = child.stderr.take().expect("piped stderr");
    let mut reader = BufReader::new(stderr);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .expect("could not read the listening address");
    let server = ServerChild { child };
    let _ = &server;

    let port = line
        .trim()
        .rsplit(':')
        .next()
        .and_then(|port| port.trim().parse::<u16>().ok())
        .unwrap_or_else(|| panic!("no port in {line:?}"));
    let address = format!("127.0.0.1:{port}");

    let health = http_get(&address, "/health");
    assert!(health.starts_with("HTTP/1.1 200 OK"), "health: {health}");
    assert!(health.contains("\"status\":\"ok\""), "health: {health}");

    let objects = http_get(&address, "/objects");
    assert!(objects.starts_with("HTTP/1.1 200 OK"), "objects: {objects}");
    assert!(objects.contains("\"processor\""), "objects: {objects}");

    let missing = http_get(&address, "/nope");
    assert!(
        missing.starts_with("HTTP/1.1 404 Not Found"),
        "missing: {missing}"
    );
}

// ---------------------------------------------------------------------------
// C5: parallel replications
// ---------------------------------------------------------------------------

#[test]
fn parallel_replications_match_the_serial_report_exactly() {
    let dir = TempDir::new("parallel");
    let model = dir.write("demo.ofm", MODEL);

    let serial = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "4",
        "--format",
        "json",
    ]);
    assert_eq!(code(&serial), 0, "stderr: {}", stderr(&serial));
    let parallel = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "4",
        "--parallel",
        "4",
        "--format",
        "json",
    ]);
    assert_eq!(code(&parallel), 0, "stderr: {}", stderr(&parallel));
    assert_eq!(
        json(&stdout(&serial)),
        json(&stdout(&parallel)),
        "--parallel must not change the report"
    );
}

#[test]
fn parallel_replications_keep_the_multi_seed_semantics() {
    let dir = TempDir::new("parallel-seeds");
    let model = dir.write("demo.ofm", MODEL);

    let serial = openflex(&[
        "run",
        &path_str(&model),
        "--seed",
        "1",
        "--seed",
        "2",
        "--seed",
        "3",
        "--format",
        "json",
    ]);
    assert_eq!(code(&serial), 0, "stderr: {}", stderr(&serial));
    let parallel = openflex(&[
        "run",
        &path_str(&model),
        "--seed",
        "1",
        "--seed",
        "2",
        "--seed",
        "3",
        "--parallel",
        "3",
        "--format",
        "json",
    ]);
    assert_eq!(code(&parallel), 0, "stderr: {}", stderr(&parallel));
    let serial_report = json(&stdout(&serial));
    let parallel_report = json(&stdout(&parallel));
    assert_eq!(serial_report["base_seed"], 1);
    assert_eq!(
        serial_report["replications"].as_array().map(Vec::len),
        Some(3)
    );
    assert_eq!(serial_report, parallel_report);
}

// ---------------------------------------------------------------------------
// C8: i18n
// ---------------------------------------------------------------------------

#[test]
fn openflex_lang_switches_help_and_report_headers() {
    let english = openflex_env(&["--help"], &[("OPENFLEX_LANG", "en")]);
    assert_eq!(code(&english), 0, "stderr: {}", stderr(&english));
    assert!(
        stdout(&english).contains("clean-room"),
        "English help: {}",
        stdout(&english)
    );

    let chinese = openflex_env(&["--help"], &[("OPENFLEX_LANG", "zh")]);
    assert_eq!(code(&chinese), 0, "stderr: {}", stderr(&chinese));
    assert!(
        stdout(&chinese).contains("净室实现"),
        "Chinese help: {}",
        stdout(&chinese)
    );

    let dir = TempDir::new("i18n");
    let model = dir.write("demo.ofm", MODEL);
    let reports = dir.join("reports");
    let run = openflex_env(
        &[
            "run",
            &path_str(&model),
            "--report-dir",
            &path_str(&reports),
            "--quiet",
        ],
        &[("OPENFLEX_LANG", "en")],
    );
    assert_eq!(code(&run), 0, "stderr: {}", stderr(&run));

    let chinese_table = openflex_env(
        &["report", &path_str(&reports.join("report.json"))],
        &[("OPENFLEX_LANG", "zh")],
    );
    assert_eq!(
        code(&chinese_table),
        0,
        "stderr: {}",
        stderr(&chinese_table)
    );
    let table = stdout(&chinese_table);
    for expected in ["对象", "平均在制品", "利用率", "创建数", "完成数"] {
        assert!(table.contains(expected), "missing {expected}: {table}");
    }
    assert!(table.contains("停留 (秒)"));

    let english_table = openflex_env(
        &["report", &path_str(&reports.join("report.json"))],
        &[("OPENFLEX_LANG", "en")],
    );
    assert!(stdout(&english_table).contains("Stay (s)"));

    // JSON and CSV stay machine readable and untranslated.
    let csv = openflex_env(
        &[
            "report",
            &path_str(&reports.join("report.json")),
            "--format",
            "csv",
        ],
        &[("OPENFLEX_LANG", "zh")],
    );
    assert_eq!(code(&csv), 0, "stderr: {}", stderr(&csv));
    assert!(stdout(&csv).starts_with("object,kind"));
}

// ---------------------------------------------------------------------------
// B9: Excel export and CSV import
// ---------------------------------------------------------------------------

#[test]
fn run_writes_an_excel_workbook_when_asked() {
    let dir = TempDir::new("xlsx");
    let model = dir.write("demo.ofm", MODEL);
    let workbook = dir.join("report.xlsx");

    let output = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "2",
        "--xlsx",
        &path_str(&workbook),
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));

    let bytes = fs::read(&workbook).expect("report.xlsx was not written");
    assert_eq!(&bytes[..4], b"PK\x03\x04", "an xlsx file is a zip archive");
    assert!(bytes.len() > 1_000, "the workbook looks empty");
    // The zip's local file headers are stored uncompressed, so the workbook
    // entry point and the first worksheet are visible without unpacking.
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("xl/workbook.xml"), "missing workbook part");
    assert!(
        text.contains("xl/worksheets/sheet1.xml"),
        "missing sheet part"
    );
}

#[test]
fn import_turns_a_csv_of_arrivals_into_a_runnable_model() {
    let dir = TempDir::new("import");
    let csv = dir.write(
        "arrivals.csv",
        "time,quantity,item_type\n0,1,0\n4,2,1\n7.5,1,0\n12,3,0\n",
    );
    let model = dir.join("arrivals.ofm");

    let output = openflex(&[
        "import",
        "--csv",
        &path_str(&csv),
        "--out",
        &path_str(&model),
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(model.is_file(), "the model file was not written");
    assert!(stdout(&output).contains("created"));

    let text = fs::read_to_string(&model).expect("read the generated model");
    assert!(text.contains("arrival_schedule"));
    assert!(text.contains("\"Source1\""));
    assert!(text.contains("\"Sink1\""));

    let validate = openflex(&["validate", &path_str(&model)]);
    assert_eq!(code(&validate), 0, "stderr: {}", stderr(&validate));

    let run = openflex(&["run", &path_str(&model), "--format", "json"]);
    assert_eq!(code(&run), 0, "stderr: {}", stderr(&run));
    let report = json(&stdout(&run));
    assert_eq!(report["model"], "arrivals");
    assert_eq!(report["created"]["mean"], 7.0);
}

// ---------------------------------------------------------------------------
// C6: schema export
// ---------------------------------------------------------------------------

#[test]
fn schema_prints_or_writes_the_model_json_schema() {
    let output = openflex(&["schema"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let schema = json(&stdout(&output));
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["$ref"], "#/$defs/model");
    assert!(schema["$defs"]["model"].is_object());
    assert!(schema["$defs"]["distribution"]["oneOf"].is_array());

    let dir = TempDir::new("schema");
    let path = dir.join("openflex-model.schema.json");
    let written = openflex(&["schema", "--out", &path_str(&path)]);
    assert_eq!(code(&written), 0, "stderr: {}", stderr(&written));
    assert!(
        stdout(&written).is_empty(),
        "--out must not duplicate the schema on stdout"
    );
    let text = fs::read_to_string(&path).expect("schema file was not written");
    assert_eq!(json(&text)["title"], "OpenFlex model");
}

#[test]
fn distributions_lists_every_distribution_the_model_supports() {
    let output = openflex(&["distributions"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    // 20 distributions as of the current model schema, including the ones
    // added in this release.
    for expected in ["pareto", "rayleigh", "poisson", "negative_binomial"] {
        assert!(text.contains(expected), "missing distribution {expected}");
    }
    let rows = text
        .lines()
        .filter(|line| !line.starts_with("distribution") && !line.trim().is_empty())
        .count();
    assert!(rows >= 20, "only {rows} distributions listed");
}

// ---------------------------------------------------------------------------
// B12: event trace output and the --until alias
// ---------------------------------------------------------------------------

#[test]
fn trace_writes_parseable_jsonl_events() {
    let dir = TempDir::new("trace-single");
    let model = dir.write("demo.ofm", MODEL);
    let trace = dir.join("trace.jsonl");

    let output = openflex(&[
        "run",
        &path_str(&model),
        "--stop-time",
        "100",
        "--replications",
        "1",
        "--trace",
        &path_str(&trace),
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    // The report still goes to stdout, complete and parseable.
    let report = json(&stdout(&output));
    assert_eq!(report["model"], "demo");
    assert!(
        stderr(&output).is_empty(),
        "--quiet must silence trace notes: {}",
        stderr(&output)
    );

    let text = fs::read_to_string(&trace).expect("trace file was not written");
    let lines: Vec<&str> = text.lines().filter(|line| !line.is_empty()).collect();
    assert!(!lines.is_empty(), "the trace must contain events");

    let mut events = Vec::new();
    for line in &lines {
        let event = json(line);
        assert!(event["time"].is_f64(), "missing time in {line}");
        assert!(event["seq"].is_u64(), "missing seq in {line}");
        let kind = event["event"].as_str().expect("event name");
        assert!(!kind.is_empty());
        assert!(
            event["object"]
                .as_str()
                .is_some_and(|object| !object.is_empty()),
            "missing object in {line}"
        );
        events.push(kind.to_string());
    }
    for expected in ["process_done", "flow"] {
        assert!(
            events.iter().any(|event| event == expected),
            "missing {expected} in {:?}",
            events.iter().collect::<std::collections::BTreeSet<_>>()
        );
    }
}

#[test]
fn trace_writes_one_file_per_replication() {
    let dir = TempDir::new("trace-many");
    let model = dir.write("demo.ofm", MODEL);
    let trace = dir.join("events.jsonl");

    let output = openflex(&[
        "run",
        &path_str(&model),
        "--stop-time",
        "60",
        "--replications",
        "3",
        "--trace",
        &path_str(&trace),
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));

    for index in 0..3 {
        let path = dir.join(&format!("events.rep{index}.jsonl"));
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("{} was not written", path.display()));
        assert!(!text.trim().is_empty(), "{} is empty", path.display());
        for line in text.lines() {
            json(line);
        }
    }
    assert!(
        !trace.exists(),
        "multi-replication traces use the .repN files, not the bare path"
    );

    let conflicting = openflex(&[
        "run",
        &path_str(&model),
        "--trace",
        &path_str(&trace),
        "--parallel",
        "2",
    ]);
    assert_eq!(code(&conflicting), 2, "stderr: {}", stderr(&conflicting));
    assert!(
        stderr(&conflicting).contains("--trace"),
        "stderr: {}",
        stderr(&conflicting)
    );
}

#[test]
fn until_is_an_alias_for_stop_time() {
    let dir = TempDir::new("until");
    let model = dir.write("demo.ofm", MODEL);

    let until = openflex(&[
        "run",
        &path_str(&model),
        "--until",
        "123.5",
        "--format",
        "json",
    ]);
    assert_eq!(code(&until), 0, "stderr: {}", stderr(&until));
    let stop_time = openflex(&[
        "run",
        &path_str(&model),
        "--stop-time",
        "123.5",
        "--format",
        "json",
    ]);
    assert_eq!(code(&stop_time), 0, "stderr: {}", stderr(&stop_time));

    let until = json(&stdout(&until));
    assert_eq!(until["stop_time"], 123.5);
    assert_eq!(until, json(&stdout(&stop_time)));

    let help = openflex(&["run", "--help"]);
    assert!(
        stdout(&help).contains("--until"),
        "the alias must be documented in --help"
    );
}

// ---------------------------------------------------------------------------
// C8: experiments and Markdown are localised too
// ---------------------------------------------------------------------------

#[test]
fn chinese_experiment_tables_and_markdown_reports_are_localised() {
    let dir = TempDir::new("i18n-reports");
    let model = dir.write("experiment.ofm", EXPERIMENT_MODEL);
    let zh = &[("OPENFLEX_LANG", "zh")][..];

    let table = openflex_env(&["experiment", &path_str(&model), "--format", "table"], zh);
    assert_eq!(code(&table), 0, "stderr: {}", stderr(&table));
    let text = stdout(&table);
    for expected in [
        "OpenFlex 实验：process-time",
        "场景",
        "重复",
        "创建",
        "完成",
        "吞吐率",
        "利用率",
        "baseline",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }

    let markdown = openflex_env(
        &["experiment", &path_str(&model), "--format", "markdown"],
        zh,
    );
    assert_eq!(code(&markdown), 0, "stderr: {}", stderr(&markdown));
    let text = stdout(&markdown);
    for expected in [
        "# OpenFlex 实验：process-time",
        "## 场景",
        "| 场景 | 重复次数 | 创建数 | 完成数 | 吞吐率 | 平均在制品 | 平均利用率 |",
        "## 如何复现",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }

    // JSON stays a machine contract: field names are never translated.
    let encoded = openflex_env(&["experiment", &path_str(&model), "--format", "json"], zh);
    assert_eq!(code(&encoded), 0, "stderr: {}", stderr(&encoded));
    let experiment = json(&stdout(&encoded));
    assert_eq!(experiment["experiment"], "process-time");
    assert_eq!(experiment["scenarios"].as_array().map(Vec::len), Some(2));

    // The Markdown form of a plain simulation report is localised as well.
    let reports = dir.join("reports");
    let run = openflex_env(
        &[
            "run",
            &path_str(&model),
            "--report-dir",
            &path_str(&reports),
            "--quiet",
        ],
        zh,
    );
    assert_eq!(code(&run), 0, "stderr: {}", stderr(&run));
    let sim_markdown = openflex_env(
        &[
            "report",
            &path_str(&reports.join("report.json")),
            "--format",
            "markdown",
        ],
        zh,
    );
    assert_eq!(code(&sim_markdown), 0, "stderr: {}", stderr(&sim_markdown));
    let text = stdout(&sim_markdown);
    for expected in [
        "# OpenFlex 报告：experiment-demo",
        "## 对象",
        "| 对象 | 类型 | 输入 | 输出 |",
        "## 总计",
        "## 如何复现",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
}

// ---------------------------------------------------------------------------
// B12: checkpoints (--save-checkpoint / --resume)
// ---------------------------------------------------------------------------

#[test]
fn checkpoint_round_trip_matches_a_direct_run() {
    let dir = TempDir::new("checkpoint-roundtrip");
    let model = dir.write("demo.ofm", MODEL);
    let checkpoint = dir.join("checkpoint.json");

    let direct = openflex(&[
        "run",
        &path_str(&model),
        "--stop-time",
        "200",
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(code(&direct), 0, "stderr: {}", stderr(&direct));

    let partial = openflex(&[
        "run",
        &path_str(&model),
        "--stop-time",
        "80",
        "--save-checkpoint",
        &path_str(&checkpoint),
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(code(&partial), 0, "stderr: {}", stderr(&partial));
    assert!(
        stderr(&partial).is_empty(),
        "--quiet must silence the checkpoint note: {}",
        stderr(&partial)
    );
    assert_eq!(json(&stdout(&partial))["stop_time"], 80.0);

    let saved = json(&fs::read_to_string(&checkpoint).expect("checkpoint was not written"));
    assert_eq!(saved["format_version"], 1);
    assert_eq!(saved["model_name"], "demo");
    assert_eq!(saved["now"].as_f64(), Some(80.0));
    assert!(
        saved["events"]
            .as_array()
            .is_some_and(|events| !events.is_empty()),
        "the checkpoint must carry the pending event calendar"
    );

    let resumed = openflex(&[
        "run",
        &path_str(&model),
        "--until",
        "200",
        "--resume",
        &path_str(&checkpoint),
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(code(&resumed), 0, "stderr: {}", stderr(&resumed));
    assert_eq!(
        json(&stdout(&direct)),
        json(&stdout(&resumed)),
        "resuming to 200 must reproduce the direct run to 200"
    );
}

#[test]
fn checkpoint_combines_with_trace_and_can_be_saved_again() {
    let dir = TempDir::new("checkpoint-trace");
    let model = dir.write("demo.ofm", MODEL);
    let checkpoint = dir.join("checkpoint.json");
    let trace = dir.join("trace.jsonl");

    let saved = openflex(&[
        "run",
        &path_str(&model),
        "--stop-time",
        "60",
        "--save-checkpoint",
        &path_str(&checkpoint),
        "--trace",
        &path_str(&trace),
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(code(&saved), 0, "stderr: {}", stderr(&saved));
    assert!(checkpoint.is_file(), "checkpoint was not written");
    let events = fs::read_to_string(&trace).expect("trace was not written");
    assert!(!events.trim().is_empty(), "trace must not be empty");
    assert_eq!(json(&stdout(&saved))["stop_time"], 60.0);

    let second = dir.join("checkpoint-120.json");
    let resumed = openflex(&[
        "run",
        &path_str(&model),
        "--until",
        "120",
        "--resume",
        &path_str(&checkpoint),
        "--save-checkpoint",
        &path_str(&second),
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(code(&resumed), 0, "stderr: {}", stderr(&resumed));
    assert_eq!(json(&stdout(&resumed))["stop_time"], 120.0);
    let saved_again = json(&fs::read_to_string(&second).expect("second checkpoint missing"));
    assert_eq!(saved_again["now"].as_f64(), Some(120.0));
}

#[test]
fn checkpoint_file_errors_have_stable_exit_codes() {
    let dir = TempDir::new("checkpoint-errors");
    let model = dir.write("demo.ofm", MODEL);

    let missing = dir.join("missing.json");
    let output = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "1",
        "--resume",
        &path_str(&missing),
    ]);
    assert_eq!(code(&output), 1, "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("missing.json"),
        "stderr: {}",
        stderr(&output)
    );

    let broken = dir.write("broken.json", "{ this is not json");
    let output = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "1",
        "--resume",
        &path_str(&broken),
    ]);
    assert_eq!(code(&output), 2, "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("invalid checkpoint"),
        "stderr: {}",
        stderr(&output)
    );

    // A checkpoint from another model must be rejected before simulating.
    let checkpoint = dir.join("checkpoint.json");
    let saved = openflex(&[
        "run",
        &path_str(&model),
        "--stop-time",
        "50",
        "--save-checkpoint",
        &path_str(&checkpoint),
        "--quiet",
    ]);
    assert_eq!(code(&saved), 0, "stderr: {}", stderr(&saved));
    let other = dir.write(
        "other.ofm",
        &MODEL.replace("\"name\": \"demo\"", "\"name\": \"other\""),
    );
    let output = openflex(&[
        "run",
        &path_str(&other),
        "--replications",
        "1",
        "--resume",
        &path_str(&checkpoint),
    ]);
    assert_eq!(code(&output), 2, "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("belongs to model"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn checkpoint_option_conflicts_are_usage_errors() {
    let dir = TempDir::new("checkpoint-conflicts");
    let model = dir.write("demo.ofm", MODEL);
    let checkpoint = dir.join("checkpoint.json");
    let model_path = path_str(&model);
    let checkpoint_path = path_str(&checkpoint);

    let cases: &[(&[&str], &str)] = &[
        (
            &[
                "run",
                &model_path,
                "--save-checkpoint",
                &checkpoint_path,
                "--parallel",
                "2",
            ],
            "--save-checkpoint",
        ),
        (
            &[
                "run",
                &model_path,
                "--save-checkpoint",
                &checkpoint_path,
                "--replications",
                "2",
            ],
            "single replication",
        ),
        (
            &[
                "run",
                &model_path,
                "--resume",
                &checkpoint_path,
                "--parallel",
                "2",
                "--replications",
                "1",
            ],
            "--resume",
        ),
        (
            &[
                "run",
                &model_path,
                "--resume",
                &checkpoint_path,
                "--seed",
                "1",
                "--seed",
                "2",
            ],
            "several --seed",
        ),
        (
            &[
                "run",
                &model_path,
                "--resume",
                &checkpoint_path,
                "--trace",
                &checkpoint_path,
                "--replications",
                "1",
            ],
            "--trace",
        ),
        (
            &[
                "run",
                &model_path,
                "--resume",
                &checkpoint_path,
                "--replications",
                "2",
            ],
            "exactly one replication",
        ),
    ];
    for (args, expected) in cases {
        let output = openflex(args);
        assert_eq!(code(&output), 2, "{args:?} stderr: {}", stderr(&output));
        assert!(
            stderr(&output).contains(expected),
            "{args:?} should mention {expected:?}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn resume_rejects_an_earlier_stop_time_and_a_different_warmup() {
    let dir = TempDir::new("checkpoint-limits");
    let model = dir.write("demo.ofm", MODEL);
    let checkpoint = dir.join("checkpoint.json");

    let saved = openflex(&[
        "run",
        &path_str(&model),
        "--stop-time",
        "100",
        "--save-checkpoint",
        &path_str(&checkpoint),
        "--quiet",
    ]);
    assert_eq!(code(&saved), 0, "stderr: {}", stderr(&saved));

    let earlier = openflex(&[
        "run",
        &path_str(&model),
        "--until",
        "50",
        "--replications",
        "1",
        "--resume",
        &path_str(&checkpoint),
    ]);
    assert_eq!(code(&earlier), 2, "stderr: {}", stderr(&earlier));
    assert!(
        stderr(&earlier).contains("before the checkpoint time"),
        "stderr: {}",
        stderr(&earlier)
    );

    let warmup = openflex(&[
        "run",
        &path_str(&model),
        "--until",
        "150",
        "--warmup",
        "10",
        "--replications",
        "1",
        "--resume",
        &path_str(&checkpoint),
    ]);
    assert_eq!(code(&warmup), 2, "stderr: {}", stderr(&warmup));
    assert!(
        stderr(&warmup).contains("warm-up"),
        "stderr: {}",
        stderr(&warmup)
    );
}

// ---------------------------------------------------------------------------
// experiment --trace
// ---------------------------------------------------------------------------

#[test]
fn experiment_writes_traces_next_to_scenario_reports() {
    let dir = TempDir::new("experiment-trace");
    let model = dir.write("experiment.ofm", EXPERIMENT_MODEL);
    let reports = dir.join("experiments");

    let output = openflex(&[
        "experiment",
        &path_str(&model),
        "--report-dir",
        &path_str(&reports),
        "--trace",
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));

    for scenario in ["baseline", "faster"] {
        for replication in 0..2 {
            let path = reports
                .join("process-time")
                .join(scenario)
                .join(format!("trace.rep{replication}.jsonl"));
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|_| panic!("{} was not written", path.display()));
            let lines: Vec<&str> = text.lines().filter(|line| !line.is_empty()).collect();
            assert!(!lines.is_empty(), "{} is empty", path.display());
            for line in lines {
                let event = json(line);
                assert!(event["event"].as_str().is_some());
                assert!(event["object"].as_str().is_some());
                assert!(event["time"].is_f64());
            }
        }
    }

    // A single-replication scenario uses the unsuffixed file name.
    let single_text = EXPERIMENT_MODEL.replace("\"replications\": 2", "\"replications\": 1");
    let single = dir.write("experiment-single.ofm", &single_text);
    let single_reports = dir.join("experiments-single");
    let output = openflex(&[
        "experiment",
        &path_str(&single),
        "--report-dir",
        &path_str(&single_reports),
        "--trace",
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(
        single_reports
            .join("process-time")
            .join("baseline")
            .join("trace.jsonl")
            .is_file(),
        "a single replication writes trace.jsonl"
    );

    // --trace requires --report-dir to know where the files belong.
    let missing_dir = openflex(&["experiment", &path_str(&model), "--trace"]);
    assert_eq!(code(&missing_dir), 2, "stderr: {}", stderr(&missing_dir));
    assert!(
        stderr(&missing_dir).contains("report-dir"),
        "stderr: {}",
        stderr(&missing_dir)
    );
}

// ---------------------------------------------------------------------------
// --resume and --set
// ---------------------------------------------------------------------------

#[test]
fn resume_rejects_parameter_overrides() {
    let dir = TempDir::new("checkpoint-set");
    let model = dir.write("demo.ofm", MODEL);
    let checkpoint = dir.join("checkpoint.json");

    let saved = openflex(&[
        "run",
        &path_str(&model),
        "--stop-time",
        "90",
        "--save-checkpoint",
        &path_str(&checkpoint),
        "--quiet",
    ]);
    assert_eq!(code(&saved), 0, "stderr: {}", stderr(&saved));

    let output = openflex(&[
        "run",
        &path_str(&model),
        "--until",
        "150",
        "--replications",
        "1",
        "--resume",
        &path_str(&checkpoint),
        "--set",
        "Processor1.process_time.mean=2.0",
    ]);
    assert_eq!(code(&output), 2, "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("--set") && stderr(&output).contains("--resume"),
        "stderr: {}",
        stderr(&output)
    );
}

// ---------------------------------------------------------------------------
// B9: object/connection CSV import
// ---------------------------------------------------------------------------

#[test]
fn import_builds_a_model_from_object_and_connection_tables() {
    let dir = TempDir::new("import-structure");
    let objects = dir.write(
        "objects.csv",
        "name,kind,pos_x,pos_y\nSource,source,-200,0\nQueue1,queue,0,0\nProc1,processor,150,0\nSink1,sink,300,0\n",
    );
    let connections = dir.write(
        "connections.csv",
        "from,to,port,strategy\nSource,Queue1,0,round_robin\nQueue1,Proc1,,\nProc1,Sink1,,\n",
    );
    let model = dir.join("line.ofm");

    let output = openflex(&[
        "import",
        "--objects",
        &path_str(&objects),
        "--connections",
        &path_str(&connections),
        "--out",
        &path_str(&model),
        "--name",
        "imported-line",
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(model.is_file(), "the model file was not written");

    let text = fs::read_to_string(&model).expect("read the model");
    for expected in ["Source", "Queue1", "Proc1", "Sink1", "round_robin"] {
        assert!(text.contains(expected), "missing {expected}");
    }

    let validate = openflex(&["validate", &path_str(&model)]);
    assert_eq!(code(&validate), 0, "stderr: {}", stderr(&validate));

    let run = openflex(&[
        "run",
        &path_str(&model),
        "--stop-time",
        "100",
        "--warmup",
        "0",
        "--replications",
        "1",
        "--format",
        "json",
    ]);
    assert_eq!(code(&run), 0, "stderr: {}", stderr(&run));
    let report = json(&stdout(&run));
    assert_eq!(report["model"], "imported-line");
    assert!(
        report["created"]["mean"].as_f64().unwrap_or_default() > 0.0,
        "the imported line must create items: {}",
        report["created"]
    );
    assert!(report["objects"]["Queue1"].is_object());
}

#[test]
fn import_adds_a_default_source_to_a_structure_only_model() {
    let dir = TempDir::new("import-default-source");
    let objects = dir.write("objects.csv", "Buffer,queue,0,0\nOut,sink,200,0\n");
    let connections = dir.write("connections.csv", "Buffer,Out\n");
    let model = dir.join("buffers.ofm");

    let output = openflex(&[
        "import",
        "--objects",
        &path_str(&objects),
        "--connections",
        &path_str(&connections),
        "--out",
        &path_str(&model),
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = fs::read_to_string(&model).expect("read the model");
    assert!(
        text.contains("\"Source1\""),
        "default source missing: {text}"
    );
    assert!(
        text.contains("\"interarrival\""),
        "default source needs an inter-arrival time"
    );

    let validate = openflex(&["validate", &path_str(&model)]);
    assert_eq!(code(&validate), 0, "stderr: {}", stderr(&validate));

    // No input tables at all is a usage error.
    let bare = openflex(&["import", "--out", &path_str(&dir.join("empty.ofm"))]);
    assert_eq!(code(&bare), 2, "stderr: {}", stderr(&bare));
    assert!(
        stderr(&bare).contains("--csv") && stderr(&bare).contains("--objects"),
        "stderr: {}",
        stderr(&bare)
    );
}

// ---------------------------------------------------------------------------
// B8: isometric layout and animation frames
// ---------------------------------------------------------------------------

#[test]
fn layout_can_render_an_isometric_3d_preview() {
    let dir = TempDir::new("layout-iso");
    let model = dir.write("demo.ofm", MODEL);
    let svg = dir.join("iso.svg");

    let output = openflex(&[
        "layout",
        &path_str(&model),
        "--dimension",
        "3d",
        "--svg",
        &path_str(&svg),
        "--width",
        "1000",
        "--height",
        "700",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = fs::read_to_string(&svg).expect("iso.svg was not written");
    assert!(text.contains("data-projection=\"isometric\""));
    assert!(
        text.matches("<polygon").count() >= 9,
        "each box needs a top and two side faces"
    );
    for name in ["Source1", "Queue1", "Processor1", "Sink1"] {
        assert!(text.contains(name), "missing {name}");
    }
    assert!(text.contains("class=\"link\""), "3D connections missing");
    assert!(text.contains("face-top") && text.contains("face-left"));

    // The default remains the flat 2D renderer.
    let flat = openflex(&["layout", &path_str(&model)]);
    assert_eq!(code(&flat), 0, "stderr: {}", stderr(&flat));
    assert!(!stdout(&flat).contains("isometric"));
    assert!(stdout(&flat).contains("<rect class=\"node"));
}

#[test]
fn layout_animation_writes_a_frame_sequence() {
    let dir = TempDir::new("layout-animate");
    let model = dir.write("demo.ofm", MODEL);
    let base = dir.join("frame.svg");

    let output = openflex(&[
        "layout",
        &path_str(&model),
        "--dimension",
        "3d",
        "--animate",
        "3",
        "--svg",
        &path_str(&base),
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    for index in 0..3 {
        let path = dir.join(&format!("frame-{index}.svg"));
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("{} was not written", path.display()));
        assert!(
            text.contains("class=\"item\""),
            "frame {index} has no item marker"
        );
    }
    assert!(
        !base.exists(),
        "the base path is not written for animations"
    );

    let missing_svg = openflex(&["layout", &path_str(&model), "--animate", "2"]);
    assert_eq!(code(&missing_svg), 2, "stderr: {}", stderr(&missing_svg));
    assert!(stderr(&missing_svg).contains("svg"));
}

// ---------------------------------------------------------------------------
// D4: optimize
// ---------------------------------------------------------------------------

#[test]
fn optimize_finds_the_highest_throughput_parameter() {
    let dir = TempDir::new("optimize-throughput");
    let model = dir.write("demo.ofm", MODEL);

    let output = openflex(&[
        "optimize",
        &path_str(&model),
        "--param",
        "Processor1.process_time.mean=2..6",
        "--steps",
        "3",
        "--objective",
        "throughput",
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let result = json(&stdout(&output));
    assert_eq!(result["objective"], "throughput");
    assert_eq!(result["direction"], "maximize");
    let candidates = result["candidates"].as_array().expect("candidates");
    assert_eq!(candidates.len(), 3);
    assert_eq!(
        result["best"]["parameters"]["Processor1.process_time.mean"], 2.0,
        "a faster processor must win on throughput: {}",
        result["best"]
    );
    let best_metric = result["best"]["metric"].as_f64().expect("metric");
    for candidate in candidates {
        assert!(
            candidate["metric"].as_f64().unwrap_or_default() <= best_metric + 1e-12,
            "a candidate beats the reported best"
        );
    }

    // --object switches a WIP objective to one object; a faster processor
    // keeps the queue emptier, so the smallest process time wins.
    let wip = openflex(&[
        "optimize",
        &path_str(&model),
        "--param",
        "Processor1.process_time.mean=2..6",
        "--steps",
        "3",
        "--objective",
        "wip",
        "--object",
        "Queue1",
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(code(&wip), 0, "stderr: {}", stderr(&wip));
    let result = json(&stdout(&wip));
    assert_eq!(result["direction"], "minimize");
    assert_eq!(
        result["best"]["parameters"]["Processor1.process_time.mean"], 2.0,
        "a faster processor should keep the queue emptier: {}",
        result["best"]
    );
}

#[test]
fn optimize_validates_parameters_and_objectives() {
    let dir = TempDir::new("optimize-errors");
    let model = dir.write("demo.ofm", MODEL);

    for bad in ["Processor1.process_time.mean=bad", "broken"] {
        let output = openflex(&["optimize", &path_str(&model), "--param", bad, "--quiet"]);
        assert_eq!(code(&output), 2, "{bad}: stderr: {}", stderr(&output));
    }

    let missing_object = openflex(&[
        "optimize",
        &path_str(&model),
        "--param",
        "Processor1.process_time.mean=2..6",
        "--steps",
        "2",
        "--objective",
        "utilization",
        "--object",
        "Nope",
        "--quiet",
    ]);
    assert_eq!(
        code(&missing_object),
        2,
        "stderr: {}",
        stderr(&missing_object)
    );
    assert!(stderr(&missing_object).contains("--object"));

    let no_params = openflex(&["optimize", &path_str(&model), "--quiet"]);
    assert_eq!(code(&no_params), 2, "stderr: {}", stderr(&no_params));
}

#[test]
fn optimize_samples_a_large_grid_and_renders_table_and_csv() {
    let dir = TempDir::new("optimize-samples");
    let model = dir.write("demo.ofm", MODEL);

    let sampled = openflex(&[
        "optimize",
        &path_str(&model),
        "--param",
        "Processor1.process_time.mean=2..6",
        "--param",
        "Queue1.max_content=5,10,15",
        "--steps",
        "3",
        "--samples",
        "2",
        "--format",
        "table",
        "--quiet",
    ]);
    assert_eq!(code(&sampled), 0, "stderr: {}", stderr(&sampled));
    let table = stdout(&sampled);
    assert!(table.contains("OpenFlex optimize: demo"), "table: {table}");
    assert!(table.contains("best:"), "table: {table}");
    assert_eq!(table.lines().filter(|line| line.contains('*')).count(), 1);

    let csv = openflex(&[
        "optimize",
        &path_str(&model),
        "--param",
        "Processor1.process_time.mean=2..6",
        "--steps",
        "2",
        "--format",
        "csv",
        "--quiet",
    ]);
    assert_eq!(code(&csv), 0, "stderr: {}", stderr(&csv));
    let csv = stdout(&csv);
    assert!(
        csv.starts_with("rank,Processor1.process_time.mean,throughput,best,created,completed,avg_wip,utilization"),
        "csv: {csv}"
    );

    // Candidate evaluation can run on a thread pool; the ranking is unchanged.
    let serial = openflex(&[
        "optimize",
        &path_str(&model),
        "--param",
        "Processor1.process_time.mean=2..6",
        "--steps",
        "3",
        "--format",
        "json",
        "--quiet",
    ]);
    let parallel = openflex(&[
        "optimize",
        &path_str(&model),
        "--param",
        "Processor1.process_time.mean=2..6",
        "--steps",
        "3",
        "--format",
        "json",
        "--parallel",
        "2",
        "--quiet",
    ]);
    assert_eq!(code(&serial), 0, "stderr: {}", stderr(&serial));
    assert_eq!(code(&parallel), 0, "stderr: {}", stderr(&parallel));
    assert_eq!(json(&stdout(&serial)), json(&stdout(&parallel)));
}

// ---------------------------------------------------------------------------
// B8: interactive HTML preview
// ---------------------------------------------------------------------------

#[test]
fn layout_html_embeds_svg_and_animation_frames() {
    let dir = TempDir::new("layout-html");
    let model = dir.write("demo.ofm", MODEL);
    let page = dir.join("preview.html");

    let output = openflex(&[
        "layout",
        &path_str(&model),
        "--dimension",
        "3d",
        "--html",
        &path_str(&page),
        "--animate",
        "3",
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let html = fs::read_to_string(&page).expect("preview.html was not written");

    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("data-projection=\"isometric\""));
    for name in ["Source1", "Queue1", "Processor1", "Sink1"] {
        assert!(html.contains(name), "missing object {name} in the HTML");
    }
    assert!(
        html.contains("<script>"),
        "the interaction script is missing"
    );
    assert!(
        html.contains("type=\"application/json\" id=\"frames\""),
        "the frames array is missing"
    );
    assert!(html.contains("id=\"play\""), "play/pause control missing");
    assert!(!html.contains("https://cdn"), "no external CDN allowed");

    // The embedded JSON array has one entry per frame.
    let start = html
        .find("id=\"frames\">")
        .map(|index| index + "id=\"frames\">".len())
        .expect("frames script");
    let end = html[start..]
        .find("</script>")
        .map(|offset| start + offset)
        .expect("frames script end");
    let frames: Vec<f64> = serde_json::from_str(&html[start..end]).expect("frame array");
    assert_eq!(frames.len(), 3);

    // 2D HTML is supported too and has a single frame.
    let flat = dir.join("flat.html");
    let output = openflex(&[
        "layout",
        &path_str(&model),
        "--html",
        &path_str(&flat),
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let html = fs::read_to_string(&flat).expect("flat.html was not written");
    assert!(html.contains("data-projection=\"flat\""));
    assert!(!html.contains("id=\"play\""));

    // --svg and --html are mutually exclusive.
    let conflict = openflex(&[
        "layout",
        &path_str(&model),
        "--html",
        &path_str(&page),
        "--svg",
        &path_str(&dir.join("x.svg")),
    ]);
    assert_eq!(code(&conflict), 2, "stderr: {}", stderr(&conflict));
}

// ---------------------------------------------------------------------------
// B7: process-flow report section
// ---------------------------------------------------------------------------

#[test]
fn flow_is_added_to_table_markdown_and_csv() {
    let dir = TempDir::new("flow-report");
    let model = dir.write("flow.ofm", FLOW_MODEL);

    let table = openflex(&["run", &path_str(&model), "--flow", "--quiet"]);
    assert_eq!(code(&table), 0, "stderr: {}", stderr(&table));
    let text = stdout(&table);
    for expected in [
        "Process Flow",
        "Tokens created",
        "Create1",
        "Delay1",
        "SinkUrgent",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }

    let markdown = openflex(&[
        "run",
        &path_str(&model),
        "--flow",
        "--format",
        "markdown",
        "--quiet",
    ]);
    assert_eq!(code(&markdown), 0, "stderr: {}", stderr(&markdown));
    let text = stdout(&markdown);
    assert!(text.contains("## Process Flow"), "markdown: {text}");
    assert!(text.contains("| Create1 | create |"), "markdown: {text}");

    let csv = openflex(&[
        "run",
        &path_str(&model),
        "--flow",
        "--format",
        "csv",
        "--quiet",
    ]);
    assert_eq!(code(&csv), 0, "stderr: {}", stderr(&csv));
    let text = stdout(&csv);
    assert!(
        text.contains("flow_block,kind,input,output,content_avg,avg_stay_s"),
        "csv: {text}"
    );
    assert!(text.contains("Create1,create"), "csv: {text}");

    // JSON always carries the structured field, with or without --flow.
    let json_output = openflex(&["run", &path_str(&model), "--format", "json", "--quiet"]);
    assert_eq!(code(&json_output), 0, "stderr: {}", stderr(&json_output));
    let report = json(&stdout(&json_output));
    assert!(report["process_flow"]["blocks"].is_array());
}

#[test]
fn flow_flag_on_a_model_without_process_flow_is_harmless() {
    let dir = TempDir::new("flow-none");
    let model = dir.write("demo.ofm", MODEL);

    let output = openflex(&["run", &path_str(&model), "--flow", "--quiet"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(
        !stdout(&output).contains("Process Flow"),
        "stdout: {}",
        stdout(&output)
    );

    // The flag is documented in --help.
    let help = openflex(&["run", "--help"]);
    assert!(stdout(&help).contains("--flow"), "run --help lacks --flow");
}

// ---------------------------------------------------------------------------
// B5: fluid report section and re-rendering
// ---------------------------------------------------------------------------

fn with_experiment(model: &str) -> String {
    let mut value: serde_json::Value = serde_json::from_str(model).expect("valid fixture");
    value["experiments"] = serde_json::json!([
        { "name": "single", "replications": 1, "scenarios": [
            { "name": "baseline", "overrides": [] }
        ] }
    ]);
    serde_json::to_string_pretty(&value).expect("serialise fixture")
}

#[test]
fn fluid_is_added_to_table_markdown_and_csv() {
    let dir = TempDir::new("fluid-report");
    let model = dir.write("fluid.ofm", FLUID_MODEL);

    let table = openflex(&["run", &path_str(&model), "--fluid", "--quiet"]);
    assert_eq!(code(&table), 0, "stderr: {}", stderr(&table));
    let text = stdout(&table);
    for expected in ["Fluid", "Total inflow", "Generator1", "Tank1", "Sink1"] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }

    let markdown = openflex(&[
        "run",
        &path_str(&model),
        "--fluid",
        "--format",
        "markdown",
        "--quiet",
    ]);
    assert_eq!(code(&markdown), 0, "stderr: {}", stderr(&markdown));
    let text = stdout(&markdown);
    assert!(text.contains("## Fluid"), "markdown: {text}");
    assert!(text.contains("| Tank1 | fluid_tank |"), "markdown: {text}");

    let csv = openflex(&[
        "run",
        &path_str(&model),
        "--fluid",
        "--format",
        "csv",
        "--quiet",
    ]);
    assert_eq!(code(&csv), 0, "stderr: {}", stderr(&csv));
    let text = stdout(&csv);
    assert!(
        text.contains("fluid_object,kind,level_avg,level_max,throughput,overflow,time_unit"),
        "csv: {text}"
    );
    assert!(text.contains("Tank1,fluid_tank"), "csv: {text}");

    // JSON always carries the structured field, with or without --fluid.
    let json_output = openflex(&["run", &path_str(&model), "--format", "json", "--quiet"]);
    assert_eq!(code(&json_output), 0, "stderr: {}", stderr(&json_output));
    let report = json(&stdout(&json_output));
    assert!(report["fluid"]["objects"].is_array());
}

#[test]
fn fluid_flag_on_a_model_without_fluid_is_harmless() {
    let dir = TempDir::new("fluid-none");
    let model = dir.write("demo.ofm", MODEL);

    let output = openflex(&["run", &path_str(&model), "--fluid", "--quiet"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(
        !stdout(&output).contains("Fluid"),
        "stdout: {}",
        stdout(&output)
    );

    let help = openflex(&["run", "--help"]);
    assert!(
        stdout(&help).contains("--fluid"),
        "run --help lacks --fluid"
    );
}

#[test]
fn report_rerenders_flow_and_fluid_sections() {
    let dir = TempDir::new("report-sections");

    let flow_model = dir.write("flow.ofm", FLOW_MODEL);
    let flow_reports = dir.join("flow-reports");
    let run = openflex(&[
        "run",
        &path_str(&flow_model),
        "--report-dir",
        &path_str(&flow_reports),
        "--quiet",
    ]);
    assert_eq!(code(&run), 0, "stderr: {}", stderr(&run));
    let flow_report = flow_reports.join("report.json");

    let without = openflex(&["report", &path_str(&flow_report)]);
    assert_eq!(code(&without), 0, "stderr: {}", stderr(&without));
    assert!(!stdout(&without).contains("Process Flow"));

    let with = openflex(&["report", &path_str(&flow_report), "--flow"]);
    assert_eq!(code(&with), 0, "stderr: {}", stderr(&with));
    assert!(
        stdout(&with).contains("Process Flow"),
        "report --flow: {}",
        stdout(&with)
    );

    let fluid_model = dir.write("fluid.ofm", FLUID_MODEL);
    let fluid_reports = dir.join("fluid-reports");
    let run = openflex(&[
        "run",
        &path_str(&fluid_model),
        "--report-dir",
        &path_str(&fluid_reports),
        "--quiet",
    ]);
    assert_eq!(code(&run), 0, "stderr: {}", stderr(&run));
    let fluid_report = fluid_reports.join("report.json");

    let with = openflex(&[
        "report",
        &path_str(&fluid_report),
        "--fluid",
        "--format",
        "markdown",
    ]);
    assert_eq!(code(&with), 0, "stderr: {}", stderr(&with));
    let text = stdout(&with);
    assert!(text.contains("## Fluid"), "report --fluid: {text}");
    assert!(
        text.contains("| Tank1 | fluid_tank |"),
        "report --fluid: {text}"
    );
}

#[test]
fn experiment_and_report_rerender_support_flow_and_fluid() {
    let dir = TempDir::new("experiment-sections");

    let flow_model = dir.write("flow-experiment.ofm", &with_experiment(FLOW_MODEL));
    let flow_reports = dir.join("flow-experiments");
    let run = openflex(&[
        "experiment",
        &path_str(&flow_model),
        "--report-dir",
        &path_str(&flow_reports),
        "--flow",
        "--quiet",
    ]);
    assert_eq!(code(&run), 0, "stderr: {}", stderr(&run));
    assert!(
        stdout(&run).contains("Process Flow"),
        "experiment --flow did not render the flow section"
    );
    let readback = openflex(&["report", "--experiment", &path_str(&flow_reports), "--flow"]);
    assert_eq!(code(&readback), 0, "stderr: {}", stderr(&readback));
    assert!(
        stdout(&readback).contains("Process Flow"),
        "report --experiment --flow did not render the flow section"
    );

    let fluid_model = dir.write("fluid-experiment.ofm", &with_experiment(FLUID_MODEL));
    let fluid_reports = dir.join("fluid-experiments");
    let run = openflex(&[
        "experiment",
        &path_str(&fluid_model),
        "--report-dir",
        &path_str(&fluid_reports),
        "--fluid",
        "--quiet",
    ]);
    assert_eq!(code(&run), 0, "stderr: {}", stderr(&run));
    assert!(
        stdout(&run).contains("Fluid"),
        "experiment --fluid did not render the fluid section"
    );
    let readback = openflex(&[
        "report",
        "--experiment",
        &path_str(&fluid_reports),
        "--fluid",
        "--format",
        "markdown",
    ]);
    assert_eq!(code(&readback), 0, "stderr: {}", stderr(&readback));
    assert!(
        stdout(&readback).contains("## Fluid"),
        "report --experiment --fluid did not render the fluid section"
    );
}

// ---------------------------------------------------------------------------
// B8: openflex studio
// ---------------------------------------------------------------------------

#[test]
fn studio_serves_the_ui_and_read_only_apis() {
    let dir = TempDir::new("studio-ui");
    let model = dir.write("demo.ofm", MODEL);
    let (_server, address) = start_studio(&model);

    let page = http_get(&address, "/");
    assert!(http_status(&page).contains("200 OK"), "page: {page}");
    let html = http_body(&page);
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("OpenFlex Studio"));
    assert!(!html.contains("__MODEL_NAME__"));

    let model_response = http_get(&address, "/api/model");
    assert!(http_status(&model_response).contains("200 OK"));
    let model = json(http_body(&model_response));
    assert_eq!(model["name"], "demo");
    assert_eq!(model["objects"].as_array().map(Vec::len), Some(4));

    let objects = json(http_body(&http_get(&address, "/api/objects")));
    assert!(
        objects["objects"]
            .as_array()
            .is_some_and(|objects| objects.iter().any(|object| object["kind"] == "processor"))
    );

    let health = http_get(&address, "/health");
    assert!(http_status(&health).contains("200 OK"));
    assert_eq!(json(http_body(&health))["status"], "ok");
}

#[test]
fn studio_run_endpoint_returns_a_report() {
    let dir = TempDir::new("studio-run");
    let model = dir.write("demo.ofm", MODEL);
    let (_server, address) = start_studio(&model);

    let response = http_request(&address, "POST", "/api/run", Some("{}"));
    assert!(
        http_status(&response).contains("200 OK"),
        "response: {response}"
    );
    let report = json(http_body(&response));
    assert_eq!(report["model"], "demo");
    assert!(report["objects"]["Queue1"].is_object());
    assert_eq!(report["stop_time"], 200.0);

    let override_response =
        http_request(&address, "POST", "/api/run", Some(r#"{"stop_time": 50}"#));
    assert!(http_status(&override_response).contains("200 OK"));
    let report = json(http_body(&override_response));
    assert_eq!(report["stop_time"], 50.0);
}

#[test]
fn studio_saves_valid_models_and_rejects_invalid_ones() {
    let dir = TempDir::new("studio-save");
    let model = dir.write("demo.ofm", MODEL);
    let (mut server, address) = start_studio(&model);

    let mut current = json(http_body(&http_get(&address, "/api/model")));
    current["description"] = serde_json::json!("edited in studio");
    let save = http_request(
        &address,
        "POST",
        "/api/model",
        Some(&serde_json::to_string(&current).expect("serialise")),
    );
    assert!(http_status(&save).contains("200 OK"), "save: {save}");
    let saved = json(http_body(&http_get(&address, "/api/model")));
    assert_eq!(saved["description"], "edited in studio");

    let validate = openflex(&["validate", &path_str(&model)]);
    assert_eq!(code(&validate), 0, "stderr: {}", stderr(&validate));
    let text = fs::read_to_string(&model).expect("read saved model");
    assert!(text.contains("edited in studio"));

    // A model with a dangling connection must be rejected (422) and leave the
    // file untouched.
    let mut invalid = saved.clone();
    invalid["connections"] = serde_json::json!([{ "from": "Source1", "to": "Nope" }]);
    let reject = http_request(
        &address,
        "POST",
        "/api/model",
        Some(&serde_json::to_string(&invalid).expect("serialise")),
    );
    assert!(http_status(&reject).contains("422"), "reject: {reject}");
    assert!(http_body(&reject).contains("invalid model"));
    let validate = openflex(&["validate", &path_str(&model)]);
    assert_eq!(code(&validate), 0, "stderr: {}", stderr(&validate));

    let _ = server.child.kill();
    let _ = server.child.wait();
}

#[test]
fn studio_rejects_unknown_paths_and_oversized_bodies() {
    let dir = TempDir::new("studio-guards");
    let model = dir.write("demo.ofm", MODEL);
    let (_server, address) = start_studio(&model);

    let missing = http_get(&address, "/does-not-exist");
    assert!(http_status(&missing).contains("404"), "missing: {missing}");

    let oversized = http_oversized(&address, "POST", "/api/model", 2_200_000);
    assert!(
        http_status(&oversized).contains("413"),
        "oversized: {}",
        http_status(&oversized)
    );

    let bad_run = http_request(&address, "POST", "/api/run", Some("not json"));
    assert!(http_status(&bad_run).contains("400"), "bad run: {bad_run}");
}

#[test]
fn studio_can_add_and_delete_objects_through_the_api() {
    let dir = TempDir::new("studio-crud");
    let model = dir.write("demo.ofm", MODEL);
    let (_server, address) = start_studio(&model);

    // The page carries the new editing controls.
    let html = http_body(&http_get(&address, "/")).to_string();
    for marker in [
        "id=\"add-object\"",
        "id=\"duplicate-object\"",
        "id=\"delete-object\"",
        "id=\"show-help\"",
        "data-help-panel=\"1\"",
        "data-studio-help=\"1\"",
        "id=\"edit-process-flow\"",
        "id=\"save-flow\"",
        "id=\"speed\"",
        "id=\"pause-animation\"",
        "id=\"block-list\"",
        "id=\"add-block\"",
        "id=\"add-flow-connection\"",
        "id=\"undo\"",
        "id=\"redo\"",
        "data-undo=\"1\"",
        "data-redo=\"1\"",
        "undoStack",
        "redoStack",
        "data-field",
        "OBJECT_FIELDS",
        "BLOCK_FIELDS",
        "id=\"step\"",
        "id=\"step-forward\"",
        "id=\"resume\"",
        "id=\"reset\"",
        "id=\"step-until\"",
        "id=\"breakpoint-object\"",
        "id=\"experiment-panel\"",
        "id=\"scenario-list\"",
        "id=\"add-scenario\"",
        "id=\"run-experiment\"",
        "id=\"dashboard\"",
        "id=\"export-png\"",
        "id=\"replication-chart\"",
        "id=\"replication-metric\"",
        "id=\"object-search\"",
        "data-object-tree",
        "data-kind-group",
        "tree-kind",
        "id=\"pause-run\"",
        "id=\"resume-run\"",
        "id=\"stop-run\"",
        "id=\"run-state\"",
        "id=\"object-menu\"",
        "data-action=\"rename\"",
        "data-action=\"duplicate\"",
        "id=\"export-csv\"",
        "id=\"timeseries-chart\"",
        "id=\"load-timeseries\"",
        "id=\"timeseries-interval\"",
        "id=\"timeseries-object\"",
        "id=\"global-tables\"",
        "id=\"checkpoints\"",
        "data-checkpoint-panel",
        "data-restore-checkpoint",
        "data-projection=\"perspective\"",
        "data-depth-sort=\"1\"",
        "data-focus-object=\"1\"",
        "data-drag-object=\"1\"",
        "data-layout-grid=\"1\"",
        "data-layout-circle=\"1\"",
        "data-layout-force=\"1\"",
        "data-nudge=\"10\"",
        "data-occlusion=\"1\"",
        "data-view-2d=\"1\"",
        "data-multi-select=\"1\"",
        "data-box-select=\"1\"",
        "data-selection-count=\"1\"",
        "data-batch-delete=\"1\"",
        "data-batch-duplicate=\"1\"",
        "data-marquee-3d=\"1\"",
        "data-group-drag=\"1\"",
        "data-batch-edit=\"1\"",
        "data-checkpoint-verify=\"1\"",
        "data-axis=\"1\"",
        "data-grid-toggle=\"1\"",
        "data-object-coords=\"1\"",
        "data-webgl=\"1\"",
        "data-view-webgl=\"1\"",
        "data-view-canvas=\"1\"",
        "data-webgl-labels=\"1\"",
        "data-webgl-depth=\"1\"",
        "data-webgl-export=\"1\"",
        "data-timeline=\"1\"",
        "data-export-events=\"1\"",
        "data-trace-load=\"1\"",
        "data-trace-list=\"1\"",
        "data-compare-panel=\"1\"",
        "data-compare-load=\"1\"",
        "data-compare-export=\"1\"",
        "data-view-persist=\"1\"",
        "data-reset-view=\"1\"",
        "data-validate-panel=\"1\"",
        "data-validate-run=\"1\"",
        "data-metadata-panel=\"1\"",
        "data-metadata-save=\"1\"",
        "data-connections-panel=\"1\"",
        "data-connection-add=\"1\"",
        "data-checkpoint-delete=\"1\"",
        "data-checkpoint-rename=\"1\"",
        "data-history-panel=\"1\"",
        "data-history-jump=",
        "data-timeseries-multi=\"1\"",
        "data-timeseries-export=\"1\"",
        "data-layout-preset-save=\"1\"",
        "data-layout-preset-apply=\"1\"",
        "data-layout-preset-delete=\"1\"",
        "function saveLayoutPreset",
        "function applyLayoutPreset",
        "function deleteLayoutPreset",
        "function exportTimeSeriesCsv",
        "function updateTimeseriesTooltip",
        "function recordHistory",
        "function jumpHistory",
        "function deleteCheckpoint",
        "function renameCheckpoint",
        "data-connection-delete=\"1\"",
        "function addConnection",
        "function deleteConnection",
        "function populateMetadata",
        "function saveMetadata",
        "function validateModel",
        "function objectHasValidationError",
        "function persistViewState",
        "function restoreViewState",
        "function resetViewState",
        "data-virtual-list=\"1\"",
        "function exportCompareCsv",
        "function loadCompareFile",
        "function renderCompare",
        "function loadTrace",
        "function showTraceAt",
        "data-export-checkpoints=\"1\"",
        "data-export-layout=\"1\"",
        "data-import-layout=\"1\"",
        "data-heatmap=\"1\"",
        "data-heatmap-metric=\"1\"",
        "data-object-state=\"1\"",
        "data-export-report-json=\"1\"",
        "data-export-report-csv=\"1\"",
        "data-agv-battery=\"1\"",
        "function exportReportJson",
        "function exportReportCsv",
        "function heatmapHex",
        "function updateHeatmapUi",
        "function exportLayoutJson",
        "function importLayoutFile",
        "data-view-preset=\"iso\"",
        "function exportEventsCsv",
        "function exportCheckpointsJson",
        "function setViewPreset",
        "function exportScenePng",
        "function updateTimeline",
        "timelineSlot",
        "function updateWebGLLabels",
        "projectWorld",
        "gl_Position",
        "precision mediump float",
        "a_position",
        "u_view_projection",
        "function applyBatchPosition",
        "state.marquee3d",
        "data-snap-grid=\"1\"",
        "data-align-distribute=\"1\"",
        "function drawScene2d",
        "function snapValue",
        "id=\"export-svg\"",
        "id=\"export-model-json\"",
        "function exportSvg",
        "function exportModelJson",
        "unproject",
        "applyLayout",
        "data-on-draw-warning",
        "data-on-draw-badge",
        "on_draw 未执行",
        "id=\"step-event\"",
        "id=\"next-event\"",
        "id=\"event-list\"",
        "data-event-list",
        "data-event-detail",
        "data-table-editor",
        "data-table-cell",
        "id=\"add-table-row\"",
        "id=\"save-table\"",
        "id=\"add-table\"",
        "id=\"flow-canvas\"",
        "flow-node",
        "draggable",
    ] {
        assert!(html.contains(marker), "missing marker {marker}");
    }

    // Add a queue: safe defaults, auto name and position, validated and saved.
    let added = http_request(
        &address,
        "POST",
        "/api/model/objects",
        Some(r#"{"kind":"queue"}"#),
    );
    assert!(
        http_status(&added).contains("200 OK"),
        "add: {}",
        http_body(&added)
    );
    let added = json(http_body(&added));
    assert_eq!(added["added"], "Queue2");
    assert_eq!(added["model"]["objects"].as_array().map(Vec::len), Some(5));
    let validate = openflex(&["validate", &path_str(&model)]);
    assert_eq!(code(&validate), 0, "stderr: {}", stderr(&validate));

    // Delete the processor: both connections to it are removed as well.
    let deleted = http_request(&address, "DELETE", "/api/model/objects/Processor1", None);
    assert!(
        http_status(&deleted).contains("200 OK"),
        "delete: {}",
        http_body(&deleted)
    );
    let deleted = json(http_body(&deleted));
    assert_eq!(deleted["removed"], "Processor1");
    assert_eq!(deleted["connections_removed"], 2);
    assert_eq!(
        deleted["model"]["connections"].as_array().map(Vec::len),
        Some(1)
    );
    let text = fs::read_to_string(&model).expect("read model");
    assert!(!text.contains("Processor1"));
    let validate = openflex(&["validate", &path_str(&model)]);
    assert_eq!(code(&validate), 0, "stderr: {}", stderr(&validate));

    let missing = http_request(&address, "DELETE", "/api/model/objects/NoSuchObject", None);
    assert!(http_status(&missing).contains("404"), "missing: {missing}");
}

#[test]
fn studio_flow_json_can_be_edited_and_validated() {
    let dir = TempDir::new("studio-flow-json");
    let model = dir.write("flow.ofm", FLOW_MODEL);
    let (_server, address) = start_studio(&model);

    let mut current = json(http_body(&http_get(&address, "/api/model")));
    current["process_flow"]["blocks"][0]["quantity"] = serde_json::json!(5);
    let save = http_request(
        &address,
        "POST",
        "/api/model",
        Some(&serde_json::to_string(&current).expect("serialise")),
    );
    assert!(
        http_status(&save).contains("200 OK"),
        "save flow: {}",
        http_body(&save)
    );
    let text = fs::read_to_string(&model).expect("read model");
    assert!(text.contains("\"quantity\": 5"));
    let validate = openflex(&["validate", &path_str(&model)]);
    assert_eq!(code(&validate), 0, "stderr: {}", stderr(&validate));

    let run = http_request(&address, "POST", "/api/run", Some("{}"));
    assert!(http_status(&run).contains("200 OK"));
    let report = json(http_body(&run));
    assert_eq!(report["process_flow"]["tokens_created"], 5.0);

    // An invalid process_flow is rejected by the same validation.
    current["process_flow"]["blocks"][0]["kind"] = serde_json::json!("nope");
    let reject = http_request(
        &address,
        "POST",
        "/api/model",
        Some(&serde_json::to_string(&current).expect("serialise")),
    );
    assert!(
        http_status(&reject).contains("422"),
        "reject: {}",
        http_body(&reject)
    );
}

#[test]
fn studio_delete_cleans_references_to_the_object() {
    let dir = TempDir::new("studio-delete-refs");
    let model = dir.write("refs.ofm", MODEL);
    let (_server, address) = start_studio(&model);

    // Install a model whose transporter points its `home` at Processor1.
    let mut current = json(http_body(&http_get(&address, "/api/model")));
    current["objects"]
        .as_array_mut()
        .expect("objects")
        .push(serde_json::json!({
            "name": "Transporter1",
            "kind": "transporter",
            "pos": [0, 100],
            "params": { "home": "Processor1" }
        }));
    let save = http_request(
        &address,
        "POST",
        "/api/model",
        Some(&serde_json::to_string(&current).expect("serialise")),
    );
    assert!(
        http_status(&save).contains("200 OK"),
        "setup: {}",
        http_body(&save)
    );

    let deleted = http_request(&address, "DELETE", "/api/model/objects/Processor1", None);
    assert!(
        http_status(&deleted).contains("200 OK"),
        "delete: {}",
        http_body(&deleted)
    );
    let deleted = json(http_body(&deleted));
    assert!(
        deleted["references_cleaned"].as_u64().unwrap_or_default() >= 1,
        "references not cleaned: {deleted}"
    );
    let text = fs::read_to_string(&model).expect("read model");
    assert!(
        !text.contains("\"home\": \"Processor1\""),
        "reference survived: {text}"
    );
    let validate = openflex(&["validate", &path_str(&model)]);
    assert_eq!(code(&validate), 0, "stderr: {}", stderr(&validate));
}

#[test]
fn studio_flow_block_editor_can_add_a_block_that_validates() {
    let dir = TempDir::new("studio-flow-block");
    let model = dir.write("flow.ofm", FLOW_MODEL);
    let (_server, address) = start_studio(&model);

    let html = http_body(&http_get(&address, "/")).to_string();
    for marker in [
        "id=\"block-list\"",
        "id=\"add-block\"",
        "id=\"delete-block\"",
        "data-field",
    ] {
        assert!(
            html.contains(marker),
            "missing block editor marker {marker}"
        );
    }

    // This mirrors what the block editor does: append a Delay block between
    // Delay1 and Assign1, then save the whole model.
    let mut current = json(http_body(&http_get(&address, "/api/model")));
    current["process_flow"]["blocks"]
        .as_array_mut()
        .expect("blocks")
        .push(serde_json::json!({
            "id": "Delay2",
            "kind": "delay",
            "time": { "distribution": "constant", "value": 3.0 }
        }));
    let connections = current["process_flow"]["connections"]
        .as_array_mut()
        .expect("flow connections");
    for connection in connections.iter_mut() {
        if connection["from"] == "Delay1" && connection["to"] == "Assign1" {
            connection["to"] = serde_json::json!("Delay2");
        }
    }
    connections.push(serde_json::json!({ "from": "Delay2", "to": "Assign1" }));

    let save = http_request(
        &address,
        "POST",
        "/api/model",
        Some(&serde_json::to_string(&current).expect("serialise")),
    );
    assert!(
        http_status(&save).contains("200 OK"),
        "save block: {}",
        http_body(&save)
    );
    let text = fs::read_to_string(&model).expect("read model");
    assert!(
        text.contains("\"Delay2\""),
        "new block missing from the file"
    );
    let validate = openflex(&["validate", &path_str(&model)]);
    assert_eq!(code(&validate), 0, "stderr: {}", stderr(&validate));

    let run = http_request(&address, "POST", "/api/run", Some("{}"));
    assert!(http_status(&run).contains("200 OK"));
    let report = json(http_body(&run));
    assert_eq!(report["process_flow"]["tokens_created"], 10.0);
}

#[test]
fn studio_step_api_matches_a_direct_run_and_resets() {
    let dir = TempDir::new("studio-step");
    let model = dir.write("demo.ofm", MODEL);
    let (_server, address) = start_studio(&model);

    let first = http_request(&address, "POST", "/api/step", Some(r#"{"until":50}"#));
    assert!(
        http_status(&first).contains("200 OK"),
        "step 50: {}",
        http_body(&first)
    );
    let first = json(http_body(&first));
    assert_eq!(first["stepped_to"], 50.0);
    assert_eq!(first["report"]["stop_time"], 50.0);
    assert!(first["breakpoint"].is_null());

    let second = http_request(&address, "POST", "/api/step", Some(r#"{"until":100}"#));
    assert!(http_status(&second).contains("200 OK"));
    let second = json(http_body(&second));
    assert_eq!(second["report"]["stop_time"], 100.0);

    let direct = http_request(&address, "POST", "/api/run", Some(r#"{"stop_time":100}"#));
    assert!(http_status(&direct).contains("200 OK"));
    assert_eq!(
        second["report"],
        json(http_body(&direct)),
        "incremental step to 100 must equal a direct run to 100"
    );

    let reset = http_request(&address, "POST", "/api/reset", Some("{}"));
    assert!(http_status(&reset).contains("200 OK"));
    assert_eq!(json(http_body(&reset))["stepped_to"], 0.0);

    let invalid = http_request(&address, "POST", "/api/step", Some(r#"{"until":0}"#));
    assert!(http_status(&invalid).contains("422"), "invalid: {invalid}");
}

#[test]
fn studio_step_can_stop_at_an_object_breakpoint() {
    let dir = TempDir::new("studio-breakpoint");
    let model = dir.write("demo.ofm", MODEL);
    let (_server, address) = start_studio(&model);

    let response = http_request(
        &address,
        "POST",
        "/api/step",
        Some(r#"{"until":200,"object":"Processor1"}"#),
    );
    assert!(
        http_status(&response).contains("200 OK"),
        "breakpoint: {}",
        http_body(&response)
    );
    let response = json(http_body(&response));
    let hit = &response["breakpoint"];
    assert_eq!(hit["object"], "Processor1");
    assert!(
        hit["time"]
            .as_f64()
            .is_some_and(|time| time > 0.0 && time <= 200.0)
    );
    let stepped_to = response["stepped_to"].as_f64().expect("stepped_to");
    assert!(stepped_to <= 200.0);
    assert_eq!(response["report"]["stop_time"], stepped_to);
}

#[test]
fn studio_experiment_endpoint_runs_scenarios() {
    let dir = TempDir::new("studio-experiment");
    let model = dir.write("demo.ofm", MODEL);
    let (_server, address) = start_studio(&model);

    let body = serde_json::json!({
        "experiment": "studio-speed",
        "replications": 2,
        "scenarios": [
            { "name": "baseline", "overrides": [] },
            { "name": "faster", "overrides": [
                { "object": "Processor1", "param": "process_time.mean", "value": 1.5 }
            ] }
        ]
    });
    let response = http_request(
        &address,
        "POST",
        "/api/experiment",
        Some(&serde_json::to_string(&body).expect("serialise")),
    );
    assert!(
        http_status(&response).contains("200 OK"),
        "experiment: {}",
        http_body(&response)
    );
    let experiment = json(http_body(&response));
    assert_eq!(experiment["experiment"], "studio-speed");
    let scenarios = experiment["scenarios"].as_array().expect("scenarios");
    assert_eq!(scenarios.len(), 2);
    assert_eq!(scenarios[0]["name"], "baseline");
    assert_eq!(scenarios[1]["name"], "faster");
    for scenario in scenarios {
        assert_eq!(
            scenario["report"]["replications"].as_array().map(Vec::len),
            Some(2)
        );
    }
    let baseline = scenarios[0]["report"]["completed"]["mean"]
        .as_f64()
        .unwrap_or_default();
    let faster = scenarios[1]["report"]["completed"]["mean"]
        .as_f64()
        .unwrap_or_default();
    assert!(
        faster > baseline,
        "a faster processor must complete more items ({baseline} vs {faster})"
    );
}

#[test]
fn import_reads_an_xlsx_workbook() {
    let dir = TempDir::new("import-xlsx");
    let workbook_path = dir.join("model.xlsx");
    let mut workbook = rust_xlsxwriter::Workbook::new();

    let objects = workbook.add_worksheet();
    objects.set_name("objects").expect("sheet name");
    for (column, header) in ["name", "kind", "pos_x", "pos_y"].iter().enumerate() {
        objects
            .write_string(0, column as u16, *header)
            .expect("header");
    }
    let object_rows = [
        ["Source", "source", "-100", "0"],
        ["Queue1", "queue", "0", "0"],
        ["Processor1", "processor", "100", "0"],
        ["Sink1", "sink", "200", "0"],
    ];
    for (row, cells) in object_rows.iter().enumerate() {
        for (column, value) in cells.iter().enumerate() {
            if column >= 2 {
                objects
                    .write_number(
                        row as u32 + 1,
                        column as u16,
                        value.parse::<f64>().expect("number"),
                    )
                    .expect("position");
            } else {
                objects
                    .write_string(row as u32 + 1, column as u16, *value)
                    .expect("cell");
            }
        }
    }

    let connections = workbook.add_worksheet();
    connections.set_name("connections").expect("sheet name");
    for (column, header) in ["from", "to"].iter().enumerate() {
        connections
            .write_string(0, column as u16, *header)
            .expect("header");
    }
    for (row, pair) in [
        ["Source", "Queue1"],
        ["Queue1", "Processor1"],
        ["Processor1", "Sink1"],
    ]
    .iter()
    .enumerate()
    {
        for (column, value) in pair.iter().enumerate() {
            connections
                .write_string(row as u32 + 1, column as u16, *value)
                .expect("cell");
        }
    }

    let arrivals = workbook.add_worksheet();
    arrivals.set_name("arrivals").expect("sheet name");
    arrivals.write_string(0, 0, "time").expect("header");
    arrivals.write_string(0, 1, "quantity").expect("header");
    for (row, (time, quantity)) in [(0.0, 1.0), (2.0, 1.0), (4.0, 1.0)].iter().enumerate() {
        arrivals
            .write_number(row as u32 + 1, 0, *time)
            .expect("time");
        arrivals
            .write_number(row as u32 + 1, 1, *quantity)
            .expect("quantity");
    }
    workbook.save(&workbook_path).expect("save xlsx");

    let model = dir.join("from-xlsx.ofm");
    let output = openflex(&[
        "import",
        "--xlsx",
        &path_str(&workbook_path),
        "--out",
        &path_str(&model),
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(model.is_file(), "the model was not written");
    let text = fs::read_to_string(&model).expect("read model");
    for expected in [
        "Source",
        "Queue1",
        "Processor1",
        "Sink1",
        "arrival_schedule",
    ] {
        assert!(
            text.contains(expected),
            "missing {expected} in the imported model"
        );
    }

    let validate = openflex(&["validate", &path_str(&model)]);
    assert_eq!(code(&validate), 0, "stderr: {}", stderr(&validate));

    let run = openflex(&[
        "run",
        &path_str(&model),
        "--stop-time",
        "50",
        "--warmup",
        "0",
        "--replications",
        "1",
        "--format",
        "json",
    ]);
    assert_eq!(code(&run), 0, "stderr: {}", stderr(&run));
    let report = json(&stdout(&run));
    assert!(
        report["created"]["mean"].as_f64().unwrap_or_default() > 0.0,
        "the imported model must create items: {}",
        report["created"]
    );

    // A file that is not a zip archive is rejected with a usage exit code.
    let broken = dir.write("broken.xlsx", "this is not a workbook");
    let output = openflex(&[
        "import",
        "--xlsx",
        &path_str(&broken),
        "--out",
        &path_str(&dir.join("broken.ofm")),
    ]);
    assert_eq!(code(&output), 2, "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("invalid workbook"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn optimize_genetic_is_deterministic_and_beats_a_single_sample_grid() {
    let dir = TempDir::new("optimize-genetic");
    let model = dir.write("demo.ofm", MODEL);
    let model_path = path_str(&model);

    let grid = openflex(&[
        "optimize",
        &model_path,
        "--param",
        "Processor1.process_time.mean=2..6",
        "--steps",
        "5",
        "--samples",
        "1",
        "--seed",
        "3",
        "--objective",
        "throughput",
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(code(&grid), 0, "stderr: {}", stderr(&grid));
    let grid = json(&stdout(&grid));

    let genetic_args = [
        "optimize",
        &model_path,
        "--param",
        "Processor1.process_time.mean=2..6",
        "--algorithm",
        "genetic",
        "--population",
        "20",
        "--generations",
        "8",
        "--seed",
        "3",
        "--objective",
        "throughput",
        "--format",
        "json",
        "--quiet",
    ];
    let first = openflex(&genetic_args);
    let second = openflex(&genetic_args);
    assert_eq!(code(&first), 0, "stderr: {}", stderr(&first));
    assert_eq!(code(&second), 0, "stderr: {}", stderr(&second));
    assert_eq!(
        json(&stdout(&first)),
        json(&stdout(&second)),
        "the genetic search must be deterministic for a fixed seed"
    );
    let genetic = json(&stdout(&first));
    let genetic_score = genetic["best"]["score"].as_f64().expect("score");
    let grid_score = grid["best"]["score"].as_f64().expect("score");
    assert!(
        genetic_score > grid_score,
        "genetic search should improve on a one-sample grid ({grid_score} -> {genetic_score})"
    );

    // Random search is deterministic for a fixed seed too.
    let random_args = [
        "optimize",
        &model_path,
        "--param",
        "Processor1.process_time.mean=2..6",
        "--algorithm",
        "random",
        "--samples",
        "6",
        "--seed",
        "9",
        "--objective",
        "throughput",
        "--format",
        "json",
        "--quiet",
    ];
    let random_a = openflex(&random_args);
    let random_b = openflex(&random_args);
    assert_eq!(code(&random_a), 0, "stderr: {}", stderr(&random_a));
    assert_eq!(json(&stdout(&random_a)), json(&stdout(&random_b)));
}

#[test]
fn optimize_supports_weighted_multi_objective_and_pareto() {
    let dir = TempDir::new("optimize-pareto");
    let model = dir.write("demo.ofm", MODEL);

    let output = openflex(&[
        "optimize",
        &path_str(&model),
        "--param",
        "Processor1.process_time.mean=2..6",
        "--steps",
        "4",
        "--objective",
        "throughput,wip",
        "--pareto",
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let result = json(&stdout(&output));
    assert_eq!(result["objective"], "throughput,wip");
    assert_eq!(result["pareto_enabled"], true);
    assert!(
        result["pareto_front"]
            .as_array()
            .is_some_and(|front| !front.is_empty()),
        "pareto front missing: {result}"
    );
    let objectives = result["objectives"].as_array().expect("objectives");
    assert_eq!(objectives.len(), 2);
    assert_eq!(objectives[0]["direction"], "maximize");
    assert_eq!(objectives[1]["direction"], "minimize");
    let best = &result["best"];
    assert!(best["score"].is_f64());
    assert!(best["objective_values"]["throughput"].is_f64());
    assert!(best["objective_values"]["wip"].is_f64());
}

#[test]
fn migrate_writes_a_valid_model_and_rejects_unknown_versions() {
    let dir = TempDir::new("migrate");
    let model = dir.write("demo.ofm", MODEL);
    let output_path = dir.join("migrated.ofm");

    let output = openflex(&[
        "migrate",
        &path_str(&model),
        "--out",
        &path_str(&output_path),
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(output_path.is_file());
    let validate = openflex(&["validate", &path_str(&output_path)]);
    assert_eq!(code(&validate), 0, "stderr: {}", stderr(&validate));

    // --in-place also writes atomically and keeps the model valid.
    let in_place = dir.write("in-place.ofm", MODEL);
    let output = openflex(&["migrate", &path_str(&in_place), "--in-place", "--quiet"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let validate = openflex(&["validate", &path_str(&in_place)]);
    assert_eq!(code(&validate), 0, "stderr: {}", stderr(&validate));

    let mut value: serde_json::Value = serde_json::from_str(MODEL).expect("fixture");
    value["version"] = serde_json::json!(99);
    let future = dir.write(
        "future.ofm",
        &serde_json::to_string(&value).expect("serialise"),
    );
    let output = openflex(&["migrate", &path_str(&future), "--in-place"]);
    assert_eq!(code(&output), 2, "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("version 99"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn run_timeseries_writes_interval_samples() {
    let dir = TempDir::new("timeseries");
    let model = dir.write("demo.ofm", MODEL);
    let series = dir.join("series.csv");

    let output = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "1",
        "--timeseries",
        &path_str(&series),
        "--interval",
        "50",
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = fs::read_to_string(&series).expect("timeseries was not written");
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines.len() > 1, "timeseries has no samples");
    assert_eq!(
        lines[0],
        "replication,time,object,kind,content_avg,utilization,throughput"
    );
    let queue_rows: Vec<&str> = lines[1..]
        .iter()
        .copied()
        .filter(|line| line.split(',').nth(2) == Some("Queue1"))
        .collect();
    assert_eq!(queue_rows.len(), 4, "expected one sample per interval");
    for row in &queue_rows {
        let time: f64 = row.split(',').nth(1).expect("time").parse().expect("time");
        assert!((50.0..=200.0).contains(&time), "time {time}");
        let content: f64 = row
            .split(',')
            .nth(4)
            .expect("content")
            .parse()
            .expect("content");
        assert!(content >= 0.0);
    }

    let invalid = openflex(&[
        "run",
        &path_str(&model),
        "--timeseries",
        &path_str(&dir.join("bad.csv")),
        "--interval",
        "0",
        "--quiet",
    ]);
    assert_eq!(code(&invalid), 2, "stderr: {}", stderr(&invalid));
}

#[test]
fn optimize_custom_weights_change_the_weighted_score() {
    let dir = TempDir::new("optimize-weights");
    let model = dir.write("demo.ofm", MODEL);

    let output = openflex(&[
        "optimize",
        &path_str(&model),
        "--param",
        "Processor1.process_time.mean=2..6",
        "--steps",
        "3",
        "--objective",
        "throughput,wip",
        "--weight",
        "throughput=0.7,wip=0.3",
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let result = json(&stdout(&output));
    assert_eq!(result["weights"]["throughput"], 0.7);
    assert_eq!(result["weights"]["wip"], 0.3);
    let best = &result["best"];
    let throughput = best["objective_values"]["throughput"]
        .as_f64()
        .expect("throughput");
    let wip = best["objective_values"]["wip"].as_f64().expect("wip");
    let score = best["score"].as_f64().expect("score");
    assert!(
        (score - (0.7 * throughput - 0.3 * wip)).abs() < 1e-12,
        "weighted score {score} != 0.7*{throughput} - 0.3*{wip}"
    );

    for bad_weight in ["throughput=0.7", "throughput=1,wip=1,utilization=1"] {
        let invalid = openflex(&[
            "optimize",
            &path_str(&model),
            "--param",
            "Processor1.process_time.mean=2..6",
            "--steps",
            "2",
            "--objective",
            "throughput,wip",
            "--weight",
            bad_weight,
            "--quiet",
        ]);
        assert_eq!(
            code(&invalid),
            2,
            "{bad_weight}: stderr: {}",
            stderr(&invalid)
        );
    }
}

#[test]
fn studio_rename_and_duplicate_keep_the_model_valid() {
    let dir = TempDir::new("studio-rename");
    let model = dir.write("demo.ofm", MODEL);
    let (_server, address) = start_studio(&model);

    let renamed = http_request(
        &address,
        "POST",
        "/api/model/objects/Processor1/rename",
        Some(r#"{"name":"ProcX"}"#),
    );
    assert!(
        http_status(&renamed).contains("200 OK"),
        "rename: {}",
        http_body(&renamed)
    );
    let renamed = json(http_body(&renamed));
    assert_eq!(renamed["renamed"], "ProcX");
    assert!(renamed["references_updated"].as_u64().unwrap_or_default() >= 2);
    let text = fs::read_to_string(&model).expect("read model");
    assert!(!text.contains("\"Processor1\""));
    assert!(text.contains("\"from\": \"ProcX\""));
    assert!(text.contains("\"to\": \"ProcX\""));
    let validate = openflex(&["validate", &path_str(&model)]);
    assert_eq!(code(&validate), 0, "stderr: {}", stderr(&validate));

    let duplicated = http_request(
        &address,
        "POST",
        "/api/model/objects/ProcX/duplicate",
        Some("{}"),
    );
    assert!(
        http_status(&duplicated).contains("200 OK"),
        "duplicate: {}",
        http_body(&duplicated)
    );
    let duplicated = json(http_body(&duplicated));
    assert_eq!(duplicated["duplicated"], "ProcXCopy2");
    assert_eq!(
        duplicated["model"]["objects"].as_array().map(Vec::len),
        Some(5)
    );
    let validate = openflex(&["validate", &path_str(&model)]);
    assert_eq!(code(&validate), 0, "stderr: {}", stderr(&validate));

    let bad = http_request(
        &address,
        "POST",
        "/api/model/objects/ProcX/rename",
        Some(r#"{"name":"Sink1"}"#),
    );
    assert!(http_status(&bad).contains("422"), "bad rename: {bad}");
}

#[test]
fn studio_timeseries_endpoint_returns_samples() {
    let dir = TempDir::new("studio-timeseries");
    let model = dir.write("demo.ofm", MODEL);
    let (_server, address) = start_studio(&model);

    let response = http_request(
        &address,
        "POST",
        "/api/timeseries",
        Some(r#"{"interval":50}"#),
    );
    assert!(
        http_status(&response).contains("200 OK"),
        "timeseries: {}",
        http_body(&response)
    );
    let payload = json(http_body(&response));
    assert_eq!(payload["interval"], 50.0);
    let objects = payload["objects"].as_array().expect("objects");
    assert!(objects.iter().any(|name| name == "Queue1"));
    let series = payload["series"].as_array().expect("series");
    assert!(!series.is_empty());
    for row in series {
        let time = row["time"].as_f64().expect("time");
        assert!((50.0..=200.0).contains(&time));
        assert!(row["content"].as_f64().is_some());
    }

    let invalid = http_request(
        &address,
        "POST",
        "/api/timeseries",
        Some(r#"{"interval":0}"#),
    );
    assert!(http_status(&invalid).contains("422"), "invalid: {invalid}");
}

#[test]
fn studio_global_table_editor_updates_the_model() {
    let dir = TempDir::new("studio-tables");
    let model = dir.write("demo.ofm", MODEL);
    let (_server, address) = start_studio(&model);

    let html = http_body(&http_get(&address, "/")).to_string();
    for marker in [
        "id=\"global-tables\"",
        "id=\"checkpoints\"",
        "data-checkpoint-panel",
        "data-restore-checkpoint",
        "data-projection=\"perspective\"",
        "data-depth-sort=\"1\"",
        "data-focus-object=\"1\"",
        "data-drag-object=\"1\"",
        "data-layout-grid=\"1\"",
        "data-layout-circle=\"1\"",
        "data-layout-force=\"1\"",
        "data-nudge=\"10\"",
        "data-occlusion=\"1\"",
        "data-view-2d=\"1\"",
        "data-multi-select=\"1\"",
        "data-box-select=\"1\"",
        "data-selection-count=\"1\"",
        "data-batch-delete=\"1\"",
        "data-batch-duplicate=\"1\"",
        "data-marquee-3d=\"1\"",
        "data-group-drag=\"1\"",
        "data-batch-edit=\"1\"",
        "data-checkpoint-verify=\"1\"",
        "data-axis=\"1\"",
        "data-grid-toggle=\"1\"",
        "data-object-coords=\"1\"",
        "data-webgl=\"1\"",
        "data-view-webgl=\"1\"",
        "data-view-canvas=\"1\"",
        "data-webgl-labels=\"1\"",
        "data-webgl-depth=\"1\"",
        "data-webgl-export=\"1\"",
        "data-timeline=\"1\"",
        "data-export-events=\"1\"",
        "data-trace-load=\"1\"",
        "data-trace-list=\"1\"",
        "data-compare-panel=\"1\"",
        "data-compare-load=\"1\"",
        "data-compare-export=\"1\"",
        "data-view-persist=\"1\"",
        "data-reset-view=\"1\"",
        "data-validate-panel=\"1\"",
        "data-validate-run=\"1\"",
        "data-metadata-panel=\"1\"",
        "data-metadata-save=\"1\"",
        "data-connections-panel=\"1\"",
        "data-connection-add=\"1\"",
        "data-checkpoint-delete=\"1\"",
        "data-checkpoint-rename=\"1\"",
        "data-history-panel=\"1\"",
        "data-history-jump=",
        "data-timeseries-multi=\"1\"",
        "data-timeseries-export=\"1\"",
        "data-layout-preset-save=\"1\"",
        "data-layout-preset-apply=\"1\"",
        "data-layout-preset-delete=\"1\"",
        "function saveLayoutPreset",
        "function applyLayoutPreset",
        "function deleteLayoutPreset",
        "function exportTimeSeriesCsv",
        "function updateTimeseriesTooltip",
        "function recordHistory",
        "function jumpHistory",
        "function deleteCheckpoint",
        "function renameCheckpoint",
        "data-connection-delete=\"1\"",
        "function addConnection",
        "function deleteConnection",
        "function populateMetadata",
        "function saveMetadata",
        "function validateModel",
        "function objectHasValidationError",
        "function persistViewState",
        "function restoreViewState",
        "function resetViewState",
        "data-virtual-list=\"1\"",
        "function exportCompareCsv",
        "function loadCompareFile",
        "function renderCompare",
        "function loadTrace",
        "function showTraceAt",
        "data-export-checkpoints=\"1\"",
        "data-export-layout=\"1\"",
        "data-import-layout=\"1\"",
        "data-heatmap=\"1\"",
        "data-heatmap-metric=\"1\"",
        "data-object-state=\"1\"",
        "data-export-report-json=\"1\"",
        "data-export-report-csv=\"1\"",
        "data-agv-battery=\"1\"",
        "function exportReportJson",
        "function exportReportCsv",
        "function heatmapHex",
        "function updateHeatmapUi",
        "function exportLayoutJson",
        "function importLayoutFile",
        "data-view-preset=\"iso\"",
        "function exportEventsCsv",
        "function exportCheckpointsJson",
        "function setViewPreset",
        "function exportScenePng",
        "function updateTimeline",
        "timelineSlot",
        "function updateWebGLLabels",
        "projectWorld",
        "gl_Position",
        "precision mediump float",
        "a_position",
        "u_view_projection",
        "function applyBatchPosition",
        "state.marquee3d",
        "data-snap-grid=\"1\"",
        "data-align-distribute=\"1\"",
        "function drawScene2d",
        "function snapValue",
        "id=\"export-svg\"",
        "id=\"export-model-json\"",
        "function exportSvg",
        "function exportModelJson",
        "unproject",
        "applyLayout",
        "data-on-draw-warning",
        "data-on-draw-badge",
        "on_draw 未执行",
        "id=\"step-event\"",
        "id=\"next-event\"",
        "id=\"event-list\"",
        "data-event-list",
        "data-event-detail",
        "data-table-editor",
        "data-table-cell",
        "id=\"add-table-row\"",
    ] {
        assert!(
            html.contains(marker),
            "missing table editor marker {marker}"
        );
    }

    let mut current = json(http_body(&http_get(&address, "/api/model")));
    current["global_tables"] = serde_json::json!({
        "Arrivals": {
            "columns": ["time", "quantity"],
            "rows": [[0, 1], [2, 1]]
        }
    });
    let save = http_request(
        &address,
        "POST",
        "/api/model",
        Some(&serde_json::to_string(&current).expect("serialise")),
    );
    assert!(
        http_status(&save).contains("200 OK"),
        "setup: {}",
        http_body(&save)
    );

    let set = http_request(
        &address,
        "POST",
        "/api/table",
        Some(r#"{"table":"Arrivals","action":"set_cell","row":0,"column":1,"value":5}"#),
    );
    assert!(
        http_status(&set).contains("200 OK"),
        "set cell: {}",
        http_body(&set)
    );
    let set = json(http_body(&set));
    assert_eq!(set["model"]["global_tables"]["Arrivals"]["rows"][0][1], 5.0);
    let validate = openflex(&["validate", &path_str(&model)]);
    assert_eq!(code(&validate), 0, "stderr: {}", stderr(&validate));

    let insert = http_request(
        &address,
        "POST",
        "/api/table",
        Some(r#"{"table":"Arrivals","action":"insert_row","values":[4,2]}"#),
    );
    assert!(
        http_status(&insert).contains("200 OK"),
        "insert row: {}",
        http_body(&insert)
    );
    let insert = json(http_body(&insert));
    assert_eq!(
        insert["model"]["global_tables"]["Arrivals"]["rows"]
            .as_array()
            .map(Vec::len),
        Some(3)
    );

    let remove = http_request(
        &address,
        "POST",
        "/api/table",
        Some(r#"{"table":"Arrivals","action":"remove_row","row":2}"#),
    );
    assert!(
        http_status(&remove).contains("200 OK"),
        "remove row: {}",
        http_body(&remove)
    );
    let remove = json(http_body(&remove));
    assert_eq!(
        remove["model"]["global_tables"]["Arrivals"]["rows"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    let text = fs::read_to_string(&model).expect("read model");
    assert!(text.contains("\"time\"") && text.contains("\"quantity\""));
    let validate = openflex(&["validate", &path_str(&model)]);
    assert_eq!(code(&validate), 0, "stderr: {}", stderr(&validate));
}

#[test]
fn studio_native_handle_pause_and_resume() {
    let dir = TempDir::new("studio-pause");
    let model = dir.write("demo.ofm", MODEL);
    let (_server, address) = start_studio(&model);

    let step = http_request(&address, "POST", "/api/step", Some(r#"{"until":80}"#));
    assert!(
        http_status(&step).contains("200 OK"),
        "step: {}",
        http_body(&step)
    );
    let step = json(http_body(&step));
    assert_eq!(step["stepped_to"], 80.0);

    let paused = http_request(&address, "POST", "/api/pause", Some("{}"));
    assert!(
        http_status(&paused).contains("200 OK"),
        "pause: {}",
        http_body(&paused)
    );
    let paused = json(http_body(&paused));
    assert_eq!(paused["state"]["paused"], true);
    assert_eq!(paused["state"]["now"], 80.0);

    let resumed = http_request(&address, "POST", "/api/resume", Some("{}"));
    assert!(
        http_status(&resumed).contains("200 OK"),
        "resume: {}",
        http_body(&resumed)
    );
    assert_eq!(json(http_body(&resumed))["state"]["paused"], false);

    let step = http_request(&address, "POST", "/api/step", Some(r#"{"until":200}"#));
    assert!(http_status(&step).contains("200 OK"));
    let step = json(http_body(&step));
    let direct = http_request(&address, "POST", "/api/run", Some(r#"{"stop_time":200}"#));
    assert_eq!(step["report"], json(http_body(&direct)));

    let reset = http_request(&address, "POST", "/api/reset", Some("{}"));
    assert!(http_status(&reset).contains("200 OK"));
}

#[test]
fn studio_step_event_uses_the_native_handle() {
    let dir = TempDir::new("studio-step-event");
    let model = dir.write("demo.ofm", MODEL);
    let (_server, address) = start_studio(&model);

    let preview = http_get(&address, "/api/next-event");
    assert!(
        http_status(&preview).contains("200 OK"),
        "preview: {preview}"
    );
    let preview = json(http_body(&preview));
    assert!(preview["event"].is_object(), "no next event: {preview}");
    assert!(preview["event"]["kind"].is_string());
    assert!(preview["event"]["time"].is_f64());

    let mut events = 0;
    let mut detail_seen = false;
    for _ in 0..500 {
        let response = http_request(&address, "POST", "/api/step-event", Some("{}"));
        assert!(
            http_status(&response).contains("200 OK"),
            "step event: {}",
            http_body(&response)
        );
        let response = json(http_body(&response));
        if response["finished"] == true {
            break;
        }
        assert!(response["event"].is_object(), "event missing: {response}");
        assert!(response["event"]["seq"].is_u64(), "seq missing: {response}");
        assert!(
            response["event"]["object"].is_string(),
            "object missing: {response}"
        );
        if response["event"]["detail"].is_string() {
            detail_seen = true;
        }
        events += 1;
    }
    assert!(events > 0, "no events were stepped");
    assert!(
        detail_seen,
        "EventPreview.detail was not exposed by /api/step-event"
    );

    // Native step_event stops at the last event; advance the remaining time
    // to the model stop_time before comparing with a one-shot run.
    let finish = http_request(&address, "POST", "/api/step", Some(r#"{"until":200}"#));
    assert!(
        http_status(&finish).contains("200 OK"),
        "finish: {}",
        http_body(&finish)
    );
    let last_report = json(http_body(&finish))["report"].clone();

    let direct = http_request(&address, "POST", "/api/run", Some("{}"));
    assert_eq!(
        last_report,
        json(http_body(&direct)),
        "stepping every event (then advancing to stop) must equal one direct run"
    );
}

#[test]
fn run_batch_means_appear_in_reports() {
    let dir = TempDir::new("batch-means");
    let model = dir.write("demo.ofm", MODEL);

    let output = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "10",
        "--batch-means",
        "5",
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let result = json(&stdout(&output));
    let batch = &result["batch_means"];
    assert_eq!(batch["batches"], 5);
    assert_eq!(batch["samples"], 10);
    let created = &batch["items_created"];
    let per_batch = created["per_batch"].as_array().expect("per_batch");
    assert_eq!(per_batch.len(), 5);
    let mean = created["mean"].as_f64().expect("mean");
    let expected = per_batch
        .iter()
        .map(|value| value.as_f64().unwrap_or_default())
        .sum::<f64>()
        / 5.0;
    assert!((mean - expected).abs() < 1e-9);
    assert!(created["variance"].as_f64().unwrap_or_default() >= 0.0);
    assert!(created["ci95"].as_f64().unwrap_or_default() >= 0.0);

    // The bare flag defaults to ten batches.
    let output = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "10",
        "--batch-means",
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(json(&stdout(&output))["batch_means"]["batches"], 10);

    let table = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "10",
        "--batch-means",
        "5",
        "--format",
        "table",
        "--quiet",
    ]);
    let table = stdout(&table);
    assert!(
        table.contains("Batch means (5 batches over 10 replications)"),
        "{table}"
    );
    assert!(table.contains("Batch variance"));

    let markdown = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "10",
        "--batch-means",
        "5",
        "--format",
        "markdown",
        "--quiet",
    ]);
    assert!(stdout(&markdown).contains("## Batch means"));

    let too_few = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "3",
        "--batch-means",
        "5",
        "--quiet",
    ]);
    assert_eq!(code(&too_few), 2, "stderr: {}", stderr(&too_few));
}

#[test]
fn run_save_checkpoints_writes_one_file_per_replication() {
    let dir = TempDir::new("save-checkpoints");
    let model = dir.write("demo.ofm", MODEL);
    let checkpoints = dir.join("checkpoints");

    let output = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "3",
        "--save-checkpoints",
        &path_str(&checkpoints),
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    for index in 0..3 {
        assert!(
            checkpoints
                .join(format!("checkpoint.rep{index}.json"))
                .is_file(),
            "checkpoint {index} missing"
        );
    }
    let files = fs::read_dir(&checkpoints).expect("checkpoints").count();
    assert_eq!(files, 3);

    let resumed = openflex(&[
        "run",
        &path_str(&model),
        "--resume",
        &path_str(&checkpoints.join("checkpoint.rep1.json")),
        "--replications",
        "1",
        "--quiet",
    ]);
    assert_eq!(code(&resumed), 0, "stderr: {}", stderr(&resumed));
}

#[test]
fn batch_means_can_include_per_object_metrics() {
    let dir = TempDir::new("batch-means-objects");
    let model = dir.write("demo.ofm", MODEL);

    let output = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "10",
        "--batch-means",
        "5",
        "--batch-means-objects",
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let result = json(&stdout(&output));
    let objects = result["batch_means"]["objects"]
        .as_object()
        .expect("object batch means");
    for name in ["Processor1", "Queue1"] {
        let object = objects
            .get(name)
            .unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(
            object["input"]["per_batch"].as_array().map(Vec::len),
            Some(5)
        );
        assert!(object["input"]["mean"].is_f64());
        assert!(object["avg_wip"]["ci95"].is_f64());
    }

    let table = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "10",
        "--batch-means-objects",
        "--format",
        "table",
        "--quiet",
    ]);
    let table = stdout(&table);
    assert!(table.contains("Batch means by object"), "{table}");
    assert!(table.contains("Processor1") && table.contains("utilization"));

    let too_few = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "3",
        "--batch-means-objects",
        "--quiet",
    ]);
    assert_eq!(code(&too_few), 2, "stderr: {}", stderr(&too_few));
}

#[test]
fn run_keep_checkpoints_retains_only_the_newest_files() {
    let dir = TempDir::new("keep-checkpoints");
    let model = dir.write("demo.ofm", MODEL);
    let checkpoints = dir.join("checkpoints");

    let output = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "3",
        "--save-checkpoints",
        &path_str(&checkpoints),
        "--keep-checkpoints",
        "2",
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let names: Vec<String> = fs::read_dir(&checkpoints)
        .expect("checkpoints")
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    assert_eq!(names.len(), 2, "kept files: {names:?}");
    assert!(names.contains(&"checkpoint.rep1.json".to_string()));
    assert!(names.contains(&"checkpoint.rep2.json".to_string()));
    assert!(!names.contains(&"checkpoint.rep0.json".to_string()));

    // The flag is only meaningful with --save-checkpoints.
    let invalid = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "2",
        "--keep-checkpoints",
        "1",
        "--quiet",
    ]);
    assert_eq!(code(&invalid), 2, "stderr: {}", stderr(&invalid));
}

#[test]
fn studio_checkpoint_panel_lists_and_restores() {
    let dir = TempDir::new("studio-checkpoints");
    let model = dir.write("demo.ofm", MODEL);
    let checkpoints = dir.join("checkpoints");
    let output = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "3",
        "--save-checkpoints",
        &path_str(&checkpoints),
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));

    let (_server, address) = start_studio(&model);
    let listed = http_get(&address, "/api/checkpoints");
    assert!(http_status(&listed).contains("200 OK"), "list: {listed}");
    let listed = json(http_body(&listed));
    let entries = listed["checkpoints"].as_array().expect("checkpoints");
    assert_eq!(entries.len(), 3);
    for (index, entry) in entries.iter().enumerate() {
        assert_eq!(entry["index"], index as u64);
        assert_eq!(entry["now"], 200.0);
        assert!(entry["size"].as_u64().unwrap_or_default() > 0);
        assert_eq!(entry["sha256_short"].as_str().map(str::len), Some(12));
    }
    let sha = entries[0]["sha256"].as_str().expect("sha256").to_string();
    let verified = http_request(
        &address,
        "POST",
        "/api/verify-checkpoint",
        Some(&format!(
            r#"{{"file":"checkpoint.rep0.json","sha256":"{sha}"}}"#
        )),
    );
    assert!(
        http_status(&verified).contains("200 OK"),
        "verify: {}",
        http_body(&verified)
    );
    assert_eq!(json(http_body(&verified))["verified"], true);
    let tampered = http_request(
        &address,
        "POST",
        "/api/verify-checkpoint",
        Some(
            r#"{"file":"checkpoint.rep0.json","sha256":"0000000000000000000000000000000000000000000000000000000000000000"}"#,
        ),
    );
    assert!(
        http_status(&tampered).contains("422"),
        "tampered: {tampered}"
    );
    assert_eq!(json(http_body(&tampered))["verified"], false);

    let restored = http_request(
        &address,
        "POST",
        "/api/resume-checkpoint",
        Some(r#"{"file":"checkpoint.rep1.json"}"#),
    );
    assert!(
        http_status(&restored).contains("200 OK"),
        "restore: {}",
        http_body(&restored)
    );
    let restored = json(http_body(&restored));
    assert_eq!(restored["state"]["now"], 200.0);
    assert!(restored["state"]["paused"].is_boolean());

    let missing = http_request(
        &address,
        "POST",
        "/api/resume-checkpoint",
        Some(r#"{"file":"nope.json"}"#),
    );
    assert!(http_status(&missing).contains("404"), "missing: {missing}");
}

#[test]
fn studio_trace_endpoint_returns_events() {
    let dir = TempDir::new("studio-trace");
    let model = dir.write("demo.ofm", MODEL);
    let (_server, address) = start_studio(&model);

    let response = http_request(&address, "POST", "/api/trace", Some(r#"{"max_events":5}"#));
    assert!(
        http_status(&response).contains("200 OK"),
        "trace: {}",
        http_body(&response)
    );
    let payload = json(http_body(&response));
    assert_eq!(payload["count"], 5);
    assert_eq!(payload["truncated"], true);
    assert!(payload["total"].as_u64().unwrap_or_default() > 5);
    let events = payload["events"].as_array().expect("events");
    assert_eq!(events.len(), 5);
    for event in events {
        assert!(event["time"].is_f64());
        assert!(event["event"].is_string());
        assert!(event["object"].is_string());
    }
}

#[test]
fn studio_validate_endpoint_reports_model_errors() {
    let dir = TempDir::new("studio-validate");
    let model = dir.write("demo.ofm", MODEL);
    let (_server, address) = start_studio(&model);

    let valid = http_request(&address, "POST", "/api/validate", Some(MODEL));
    assert!(
        http_status(&valid).contains("200 OK"),
        "valid: {}",
        http_body(&valid)
    );
    let valid = json(http_body(&valid));
    assert_eq!(valid["ok"], true);
    assert_eq!(valid["count"], 0);

    let mut broken: serde_json::Value = serde_json::from_str(MODEL).expect("fixture");
    broken["connections"][0]["to"] = serde_json::json!("MissingSink");
    let broken = http_request(
        &address,
        "POST",
        "/api/validate",
        Some(&serde_json::to_string(&broken).expect("serialise")),
    );
    assert!(
        http_status(&broken).contains("200 OK"),
        "broken: {}",
        http_body(&broken)
    );
    let broken = json(http_body(&broken));
    assert_eq!(broken["ok"], false);
    assert!(broken["count"].as_u64().unwrap_or_default() > 0);
    let errors = broken["errors"].as_array().expect("errors");
    assert!(
        errors
            .iter()
            .any(|issue| { issue["location"].is_string() && issue["message"].is_string() })
    );
}

#[test]
fn studio_checkpoint_rename_and_delete() {
    let dir = TempDir::new("studio-checkpoint-manage");
    let model = dir.write("demo.ofm", MODEL);
    let checkpoints = dir.join("checkpoints");
    let output = openflex(&[
        "run",
        &path_str(&model),
        "--replications",
        "2",
        "--save-checkpoints",
        &path_str(&checkpoints),
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let (_server, address) = start_studio(&model);

    let renamed = http_request(
        &address,
        "POST",
        "/api/checkpoint-rename",
        Some(r#"{"file":"checkpoint.rep0.json","new_name":"checkpoint.rep0-renamed.json"}"#),
    );
    assert!(
        http_status(&renamed).contains("200 OK"),
        "rename: {}",
        http_body(&renamed)
    );
    assert!(checkpoints.join("checkpoint.rep0-renamed.json").is_file());
    assert!(!checkpoints.join("checkpoint.rep0.json").exists());

    let bad = http_request(
        &address,
        "POST",
        "/api/checkpoint-rename",
        Some(r#"{"file":"checkpoint.rep1.json","new_name":"../escape.json"}"#),
    );
    assert!(http_status(&bad).contains("422"), "bad rename: {bad}");

    let deleted = http_request(
        &address,
        "DELETE",
        "/api/checkpoints/checkpoint.rep1.json",
        None,
    );
    assert!(
        http_status(&deleted).contains("200 OK"),
        "delete: {}",
        http_body(&deleted)
    );
    assert!(!checkpoints.join("checkpoint.rep1.json").exists());

    let listed = http_get(&address, "/api/checkpoints");
    let listed = json(http_body(&listed));
    let entries = listed["checkpoints"].as_array().expect("checkpoints");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["file"], "checkpoint.rep0-renamed.json");
}

// ---------------------------------------------------------------------------
// Model-defined performance measures (task-159)
// ---------------------------------------------------------------------------

/// Model with three measures, including a CJK name and an underscore name.
const MEASURE_MODEL: &str = r#"{
  "format": "openflex-model",
  "version": 1,
  "name": "measure-demo",
  "time_unit": "seconds",
  "run": { "stop_time": 120.0, "warmup": 0.0, "seed": 5, "replications": 2 },
  "global_variables": { "Batch": { "kind": "number", "value": 10 } },
  "performance_measures": [
    { "name": "DoubleBatch", "expression": "$Batch * 2" },
    { "name": "done_per_time", "expression": "completed / time" },
    { "name": "完成量", "expression": "completed" }
  ],
  "objects": [
    { "name": "Source1", "kind": "source", "params": {
        "interarrival": { "distribution": "exponential", "mean": 4.0 } } },
    { "name": "Sink1", "kind": "sink", "params": {} }
  ],
  "connections": [ { "from": "Source1", "to": "Sink1" } ],
  "experiments": [
    { "name": "measures", "replications": 2, "scenarios": [
      { "name": "baseline", "overrides": [] },
      { "name": "wide", "overrides": [
        { "object": "Source1", "param": "interarrival",
          "value": { "distribution": "exponential", "mean": 2.0 } } ] }
    ] }
  ]
}
"#;

/// Same shape as `MEASURE_MODEL` but with a single replication, which is where
/// an interval must not be printed.
const MEASURE_MODEL_ONE_REP: &str = r#"{
  "format": "openflex-model",
  "version": 1,
  "name": "measure-one",
  "time_unit": "seconds",
  "run": { "stop_time": 120.0, "warmup": 0.0, "seed": 5, "replications": 1 },
  "performance_measures": [ { "name": "done", "expression": "completed" } ],
  "objects": [
    { "name": "Source1", "kind": "source", "params": {
        "interarrival": { "distribution": "exponential", "mean": 4.0 } } },
    { "name": "Sink1", "kind": "sink", "params": {} }
  ],
  "connections": [ { "from": "Source1", "to": "Sink1" } ],
  "experiments": [
    { "name": "single", "replications": 1, "scenarios": [ { "name": "baseline", "overrides": [] } ] }
  ]
}
"#;

#[test]
fn experiment_table_shows_measure_columns() {
    let dir = TempDir::new("measures-table");
    let model = dir.write("measure.ofm", MEASURE_MODEL);

    let output = openflex(&["experiment", &path_str(&model), "--quiet"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);

    let header = text
        .lines()
        .find(|line| line.starts_with("Scenario"))
        .expect("scenario header row");
    for name in ["DoubleBatch", "done_per_time", "完成量"] {
        assert!(header.contains(name), "header lost {name}: {header}");
    }
    let baseline = text
        .lines()
        .find(|line| line.starts_with("baseline"))
        .expect("baseline row");
    // `$Batch * 2` is exactly 20 whatever the run does.
    assert!(
        baseline.contains("20.000"),
        "row lost the value: {baseline}"
    );
}

#[test]
fn experiment_csv_and_markdown_show_measures() {
    let dir = TempDir::new("measures-formats");
    let model = dir.write("measure.ofm", MEASURE_MODEL);

    let csv = openflex(&[
        "experiment",
        &path_str(&model),
        "--format",
        "csv",
        "--quiet",
    ]);
    assert_eq!(code(&csv), 0, "stderr: {}", stderr(&csv));
    let text = stdout(&csv);
    assert!(
        text.contains("# performance measures"),
        "csv needs the note:\n{text}"
    );
    let header = text
        .lines()
        .find(|line| line.starts_with("experiment,"))
        .expect("csv header");
    assert!(header.contains("DoubleBatch"), "csv header: {header}");
    assert!(header.contains("完成量_ci95"), "csv header: {header}");
    let row = text
        .lines()
        .find(|line| line.starts_with("measures,baseline"))
        .expect("baseline csv row");
    assert!(row.contains("20.000000"), "csv row: {row}");

    let markdown = openflex(&[
        "experiment",
        &path_str(&model),
        "--format",
        "markdown",
        "--quiet",
    ]);
    assert_eq!(code(&markdown), 0, "stderr: {}", stderr(&markdown));
    let text = stdout(&markdown);
    assert!(
        text.contains("## Performance measures"),
        "markdown needs the section:\n{text}"
    );
    assert!(text.contains("| 完成量 |"), "markdown rows:\n{text}");
}

#[test]
fn run_report_files_carry_measures() {
    let dir = TempDir::new("measures-report-dir");
    let model = dir.write("measure.ofm", MEASURE_MODEL);
    let reports = dir.join("reports");

    let output = openflex(&[
        "run",
        &path_str(&model),
        "--report-dir",
        &path_str(&reports),
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));

    let csv = fs::read_to_string(reports.join("report.csv")).expect("report.csv");
    assert!(csv.contains("measure,mean,std_dev,ci95,min,max,n"), "{csv}");
    assert!(csv.contains("DoubleBatch,20.000000"), "{csv}");

    let markdown = fs::read_to_string(reports.join("report.md")).expect("report.md");
    assert!(markdown.contains("## Performance measures"), "{markdown}");

    let json = json(&fs::read_to_string(reports.join("report.json")).expect("report.json"));
    assert!(json["performance_measures"]["DoubleBatch"].is_object());
}

#[test]
fn run_table_shows_measures_like_report_table() {
    // The `run` and `report` table paths used to disagree: only `report`
    // printed the measure section. Both must now show the same block.
    let dir = TempDir::new("measures-run-table");
    let model = dir.write("measure.ofm", MEASURE_MODEL);
    let reports = dir.join("reports");

    let run = openflex(&[
        "run",
        &path_str(&model),
        "--report-dir",
        &path_str(&reports),
        "--quiet",
    ]);
    assert_eq!(code(&run), 0, "stderr: {}", stderr(&run));
    let run_text = stdout(&run);
    assert!(
        run_text.contains("Performance measures"),
        "run --format table lost the section:\n{run_text}"
    );

    let reread = openflex(&["report", &path_str(&reports.join("report.json")), "--quiet"]);
    assert_eq!(code(&reread), 0, "stderr: {}", stderr(&reread));
    let reread_text = stdout(&reread);
    assert!(
        reread_text.contains("Performance measures"),
        "report --format table lost the section:\n{reread_text}"
    );
    // Same input, same rendering: the two paths must agree byte for byte.
    assert_eq!(
        run_text, reread_text,
        "run and report disagree on the same report"
    );
}

#[test]
fn single_replication_measure_has_no_interval() {
    let dir = TempDir::new("measures-one-rep");
    let model = dir.write("one.ofm", MEASURE_MODEL_ONE_REP);

    let run = openflex(&["run", &path_str(&model), "--quiet"]);
    assert_eq!(code(&run), 0, "stderr: {}", stderr(&run));
    let text = stdout(&run);
    assert!(
        !text.contains("±0.000"),
        "a single sample must not show an interval:\n{text}"
    );

    let experiment = openflex(&["experiment", &path_str(&model), "--quiet"]);
    assert_eq!(code(&experiment), 0, "stderr: {}", stderr(&experiment));
    let text = stdout(&experiment);
    assert!(!text.contains("±0.000"), "experiment table:\n{text}");
    let row = text
        .lines()
        .find(|line| line.starts_with("baseline"))
        .expect("baseline row");
    assert!(
        row.contains("—"),
        "the interval cell must show a dash: {row}"
    );
}

#[test]
fn measure_names_do_not_break_csv_columns() {
    let dir = TempDir::new("measures-csv-columns");
    let model = dir.write("measure.ofm", MEASURE_MODEL);
    let reports = dir.join("reports");
    assert_eq!(
        code(&openflex(&[
            "run",
            &path_str(&model),
            "--report-dir",
            &path_str(&reports),
            "--quiet"
        ])),
        0
    );

    let csv = fs::read_to_string(reports.join("report.csv")).expect("report.csv");
    let block = csv
        .split_once("measure,mean,std_dev,ci95,min,max,n\n")
        .map(|(_, rest)| rest)
        .expect("measure block");
    let rows: Vec<&str> = block.lines().filter(|line| !line.is_empty()).collect();
    assert_eq!(rows.len(), 3, "one row per measure:\n{csv}");
    for row in rows {
        // The CJK and underscore names must not introduce separators.
        assert_eq!(row.matches(',').count(), 6, "column count shifted: {row}");
    }
    assert!(csv.contains("完成量,"), "the CJK name must survive:\n{csv}");
}

// ---------------------------------------------------------------------------
// Item trace (task-170)
// ---------------------------------------------------------------------------

/// Run the fixture model with `--trace` and return the trace file path.
fn traced_fixture(dir: &TempDir) -> PathBuf {
    let model = dir.write("trace-demo.ofm", MODEL);
    let trace = dir.join("trace.jsonl");
    let output = openflex(&[
        "run",
        &path_str(&model),
        "--stop-time",
        "120",
        "--trace",
        &path_str(&trace),
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(trace.is_file(), "run --trace did not write the file");
    trace
}

#[test]
fn trace_summary_lists_counts_without_the_step_table() {
    let dir = TempDir::new("trace-summary");
    let trace = traced_fixture(&dir);

    let output = openflex(&["trace", &path_str(&trace), "--quiet"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("Trace summary"), "{text}");
    assert!(text.contains("item events"), "{text}");
    assert!(
        !text.contains("  Object"),
        "the unfiltered summary must not print every step:\n{text}"
    );

    // The JSON form carries the same numbers for scripts.
    let json_output = openflex(&["trace", &path_str(&trace), "--format", "json", "--quiet"]);
    assert_eq!(code(&json_output), 0);
    let summary = json(stdout(&json_output).as_str());
    assert!(summary["events"].as_u64().unwrap_or(0) > 0);
    assert!(summary["items"].as_u64().unwrap_or(0) > 0);
    assert_eq!(summary["truncated"], serde_json::Value::Bool(false));
    assert!(
        summary["hint"]
            .as_str()
            .unwrap_or_default()
            .contains("--items")
    );
}

#[test]
fn trace_items_lists_every_tagged_id() {
    let dir = TempDir::new("trace-items");
    let trace = traced_fixture(&dir);

    let listed = openflex(&[
        "trace",
        &path_str(&trace),
        "--items",
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(code(&listed), 0, "stderr: {}", stderr(&listed));
    let listed = json(stdout(&listed).as_str());
    let ids: Vec<u64> = listed
        .as_array()
        .expect("an array of items")
        .iter()
        .filter_map(|entry| entry["item"].as_u64())
        .collect();
    assert!(!ids.is_empty(), "no item was traced");

    // The id set must match the `item` keys present in the trace file itself.
    let raw = fs::read_to_string(&trace).expect("read trace");
    let mut expected: Vec<u64> = raw
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|event| event["data"]["item"].as_f64())
        .map(|item| item as u64)
        .collect();
    expected.sort_unstable();
    expected.dedup();
    assert_eq!(ids, expected, "the listed ids differ from the trace file");
}

#[test]
fn trace_item_prints_the_complete_path() {
    let dir = TempDir::new("trace-item");
    let trace = traced_fixture(&dir);

    let output = openflex(&[
        "trace",
        &path_str(&trace),
        "--item",
        "1",
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let trace_json = json(stdout(&output).as_str());
    assert_eq!(trace_json["item"], 1);
    let steps = trace_json["steps"].as_array().expect("steps");
    assert!(!steps.is_empty(), "the item has no steps");

    // Step count and ordering must match the trace file for that item.
    let raw = fs::read_to_string(&trace).expect("read trace");
    let mut expected: Vec<(f64, u64)> = raw
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| event["data"]["item"].as_f64() == Some(1.0))
        .map(|event| {
            (
                event["time"].as_f64().unwrap_or_default(),
                event["seq"].as_u64().unwrap_or_default(),
            )
        })
        .collect();
    expected.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    assert_eq!(
        steps.len(),
        expected.len(),
        "step count differs from the trace"
    );
    for (step, (time, seq)) in steps.iter().zip(&expected) {
        assert_eq!(step["time"].as_f64(), Some(*time));
        assert_eq!(step["seq"].as_u64(), Some(*seq));
    }

    // The table form names the objects and the step count in one line.
    let table = openflex(&["trace", &path_str(&trace), "--item", "1", "--quiet"]);
    assert_eq!(code(&table), 0);
    let text = stdout(&table);
    assert!(text.contains("Item trace: item 1"), "{text}");
    assert!(text.contains("Steps:"), "{text}");
    assert!(text.contains("Objects visited:"), "{text}");
}

#[test]
fn trace_unknown_item_is_an_explicit_error() {
    let dir = TempDir::new("trace-unknown");
    let trace = traced_fixture(&dir);

    let output = openflex(&["trace", &path_str(&trace), "--item", "999999"]);
    assert_eq!(code(&output), 2, "a missing id is a usage error");
    let message = stderr(&output);
    assert!(message.contains("does not appear"), "{message}");
    assert!(message.contains("999999"), "{message}");
    assert!(
        message.contains("traced items"),
        "the error must list what can be traced: {message}"
    );
}

#[test]
fn trace_missing_file_is_a_clean_error() {
    let dir = TempDir::new("trace-missing");
    let missing = dir.join("nope.jsonl");

    let output = openflex(&["trace", &path_str(&missing)]);
    assert_eq!(code(&output), 1, "an unreadable input is a runtime error");
    let message = stderr(&output);
    assert!(
        message.contains("nope.jsonl"),
        "the path must be reported: {message}"
    );
    assert!(message.contains("could not read"), "{message}");
    // The I/O cause must be printed once, not repeated through a cause chain.
    // The OS wording differs (`No such file or directory` on Unix,
    // `The system cannot find the file specified` on Windows), but both carry
    // the `(os error N)` suffix, so that is what this counts.
    assert_eq!(
        message.matches("(os error 2)").count(),
        1,
        "duplicated I/O cause: {message}"
    );
}

#[test]
fn trace_output_can_be_written_to_a_file() {
    let dir = TempDir::new("trace-output");
    let trace = traced_fixture(&dir);
    let path = dir.join("item.md");

    let output = openflex(&[
        "trace",
        &path_str(&trace),
        "--item",
        "1",
        "--format",
        "markdown",
        "--output",
        &path_str(&path),
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(stdout(&output).is_empty(), "nothing must go to stdout");
    let text = fs::read_to_string(&path).expect("item.md");
    assert!(text.starts_with("# Item trace: item 1"), "{text}");
    assert!(text.contains("| Time | Object | Event |"), "{text}");
}

#[test]
fn trace_without_item_tags_says_so() {
    let dir = TempDir::new("trace-untagged");
    // A hand-written trace that only holds relational events (no item key).
    let trace = dir.write(
        "untagged.jsonl",
        "{\"time\":0.0,\"seq\":0,\"event\":\"operator_loaded\",\"object\":\"T\",\"data\":{}}\n",
    );

    let summary = openflex(&["trace", &path_str(&trace), "--quiet"]);
    assert_eq!(code(&summary), 0, "stderr: {}", stderr(&summary));
    assert!(
        stdout(&summary).contains("0 traced items"),
        "{}",
        stdout(&summary)
    );

    let unknown = openflex(&["trace", &path_str(&trace), "--item", "1"]);
    assert_eq!(code(&unknown), 2);
    let message = stderr(&unknown);
    assert!(
        message.contains("no event tagged with an item id"),
        "an untagged trace must say why nothing can be traced: {message}"
    );
}

// ---------------------------------------------------------------------------
// Cross-item path aggregation (task-172)
// ---------------------------------------------------------------------------

#[test]
fn trace_paths_groups_items_and_reports_coverage() {
    let dir = TempDir::new("trace-paths");
    let trace = traced_fixture(&dir);

    let output = openflex(&["trace", &path_str(&trace), "--paths", "--quiet"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("Path groups:"), "{text}");
    assert!(text.contains("coverage 100.0%"), "{text}");
    assert!(text.contains("distinct paths"), "{text}");
    assert!(text.contains("Sample items"), "{text}");

    // Every traced id must appear in exactly one group, and the group sizes
    // must add up to the number of traced items.
    let json_output = openflex(&[
        "trace",
        &path_str(&trace),
        "--paths",
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(code(&json_output), 0, "stderr: {}", stderr(&json_output));
    let summary = json(stdout(&json_output).as_str());
    let groups = summary["groups"].as_array().expect("groups");
    let mut grouped: Vec<u64> = groups
        .iter()
        .flat_map(|group| group["items"].as_array().expect("items"))
        .filter_map(|item| item.as_u64())
        .collect();
    grouped.sort_unstable();
    let traced = summary["traced_items"].as_u64().unwrap_or(0) as usize;
    assert_eq!(grouped.len(), traced, "group sizes must cover every item");
    assert_eq!(summary["grouped_items"].as_u64(), Some(traced as u64));
    assert_eq!(summary["coverage"], 1.0);

    // The ids in the groups are exactly the ids `--items` lists.
    let listed = openflex(&[
        "trace",
        &path_str(&trace),
        "--items",
        "--format",
        "json",
        "--quiet",
    ]);
    let listed = json(stdout(&listed).as_str());
    let mut listed_ids: Vec<u64> = listed
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|entry| entry["item"].as_u64())
        .collect();
    listed_ids.sort_unstable();
    assert_eq!(grouped, listed_ids, "groups and --items disagree");
}

#[test]
fn trace_paths_compresses_consecutive_visits() {
    let dir = TempDir::new("trace-paths-compress");
    // One item is created, enters and exits the same source, then goes to the
    // sink: the compressed path must not repeat the source.
    let model = dir.write(
        "flow.ofm",
        r#"{
  "format": "openflex-model", "version": 1, "name": "paths-demo",
  "time_unit": "seconds",
  "run": { "stop_time": 10.0, "warmup": 0.0, "seed": 1, "replications": 1 },
  "objects": [
    { "name": "S", "kind": "source", "params": {
        "interarrival": { "distribution": "constant", "value": 100.0 },
        "max_arrivals": 1 } },
    { "name": "K", "kind": "sink", "params": {} }
  ],
  "connections": [ { "from": "S", "to": "K" } ]
}"#,
    );
    let trace = dir.join("trace.jsonl");
    let run = openflex(&[
        "run",
        &path_str(&model),
        "--trace",
        &path_str(&trace),
        "--quiet",
    ]);
    assert_eq!(code(&run), 0, "stderr: {}", stderr(&run));

    let paths = openflex(&[
        "trace",
        &path_str(&trace),
        "--paths",
        "--format",
        "json",
        "--quiet",
    ]);
    assert_eq!(code(&paths), 0, "stderr: {}", stderr(&paths));
    let summary = json(stdout(&paths).as_str());
    let path = summary["groups"][0]["path"].as_array().expect("path");
    let names: Vec<&str> = path.iter().filter_map(|name| name.as_str()).collect();
    assert_eq!(
        names,
        ["S", "K"],
        "adjacent visits are collapsed: {names:?}"
    );
    assert_eq!(summary["groups"][0]["count"], 1);
}

#[test]
fn trace_paths_exclude_the_other_selectors() {
    let dir = TempDir::new("trace-paths-conflict");
    let trace = traced_fixture(&dir);

    let with_item = openflex(&["trace", &path_str(&trace), "--paths", "--item", "1"]);
    assert_eq!(code(&with_item), 2, "clap must reject --paths with --item");

    let with_items = openflex(&["trace", &path_str(&trace), "--paths", "--items"]);
    assert_eq!(
        code(&with_items),
        2,
        "clap must reject --paths with --items"
    );
}

#[test]
fn trace_paths_on_an_untagged_trace_explains_itself() {
    let dir = TempDir::new("trace-paths-untagged");
    let trace = dir.write(
        "untagged.jsonl",
        "{\"time\":0.0,\"seq\":0,\"event\":\"operator_loaded\",\"object\":\"T\",\"data\":{}}\n",
    );

    let output = openflex(&["trace", &path_str(&trace), "--paths", "--quiet"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("no event in this trace is tagged"),
        "the empty result must explain itself: {}",
        stdout(&output)
    );

    let json_output = openflex(&[
        "trace",
        &path_str(&trace),
        "--paths",
        "--format",
        "json",
        "--quiet",
    ]);
    let summary = json(stdout(&json_output).as_str());
    assert_eq!(summary["groups"].as_array().map(Vec::len), Some(0));
    assert_eq!(summary["coverage"], 0.0);
    assert!(
        summary["hint"]
            .as_str()
            .unwrap_or_default()
            .contains("no event")
    );
}

// ---------------------------------------------------------------------------
// Terminal charts (task-174)
// ---------------------------------------------------------------------------

/// Model with one measured and one tracked series, for chart tests.
const CHART_MODEL: &str = r#"{
  "format": "openflex-model",
  "version": 1,
  "name": "chart-demo",
  "time_unit": "seconds",
  "run": { "stop_time": 60.0, "warmup": 0.0, "seed": 3, "replications": 2 },
  "performance_measures": [
    { "name": "Done", "expression": "completed" },
    { "name": "Zero", "expression": "0" }
  ],
  "tracked_variables": [ { "name": "WIP", "expression": "Queue1.content" } ],
  "tracked_interval": 1.0,
  "objects": [
    { "name": "Source1", "kind": "source", "params": {
        "interarrival": { "distribution": "exponential", "mean": 5.0 } } },
    { "name": "Queue1", "kind": "queue", "params": {} },
    { "name": "Proc1", "kind": "processor", "params": {
        "process_time": { "distribution": "constant", "value": 4.0 } } },
    { "name": "Sink1", "kind": "sink", "params": {} }
  ],
  "connections": [
    { "from": "Source1", "to": "Queue1" },
    { "from": "Queue1", "to": "Proc1" },
    { "from": "Proc1", "to": "Sink1" }
  ]
}
"#;

/// Run a model and return the path of its archived `report.json`.
fn charted_report(dir: &TempDir, name: &str, model: &str) -> PathBuf {
    let model = dir.write(name, model);
    let reports = dir.join("reports");
    let output = openflex(&[
        "run",
        &path_str(&model),
        "--report-dir",
        &path_str(&reports),
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    reports.join("report.json")
}

#[test]
fn chart_utilization_draws_bars() {
    let dir = TempDir::new("chart-utilization");
    let report = charted_report(&dir, "chart.ofm", CHART_MODEL);

    let output = openflex(&[
        "chart",
        &path_str(&report),
        "--of",
        "utilization",
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("Utilisation"), "{text}");
    // Only the processor has a capacity signal; queues and sinks are skipped.
    assert!(
        text.contains("Proc1"),
        "the processor must be charted: {text}"
    );
    assert!(
        !text.contains("Queue1"),
        "the queue must not be charted: {text}"
    );
    assert!(text.contains('█'), "a bar must be drawn: {text}");
    assert!(
        text.contains('%'),
        "utilisation prints a percentage: {text}"
    );
}

#[test]
fn chart_measures_annotate_the_interval() {
    let dir = TempDir::new("chart-measures");
    let report = charted_report(&dir, "chart.ofm", CHART_MODEL);

    let output = openflex(&["chart", &path_str(&report), "--of", "measures", "--quiet"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("Done"), "{text}");
    assert!(text.contains("Zero"), "{text}");
    assert!(
        text.contains("n=2"),
        "the sample count must be shown: {text}"
    );
    assert!(text.contains("95% CI ±"), "n>=2 shows the interval: {text}");
    assert!(text.contains('█'), "a bar must be drawn: {text}");
}

#[test]
fn chart_series_draws_a_sparkline_or_bar() {
    let dir = TempDir::new("chart-series");
    let report = charted_report(&dir, "chart.ofm", CHART_MODEL);

    let line = openflex(&["chart", &path_str(&report), "--of", "series:WIP", "--quiet"]);
    assert_eq!(code(&line), 0, "stderr: {}", stderr(&line));
    let text = stdout(&line);
    assert!(text.contains("Tracked series: WIP"), "{text}");
    assert!(
        text.chars().any(|c| ('▁'..='█').contains(&c)),
        "a sparkline must be drawn: {text}"
    );

    let bars = openflex(&[
        "chart",
        &path_str(&report),
        "--of",
        "series:WIP",
        "--kind",
        "bar",
        "--quiet",
    ]);
    assert_eq!(code(&bars), 0, "stderr: {}", stderr(&bars));
    assert!(stdout(&bars).contains('█'), "{}", stdout(&bars));
}

#[test]
fn chart_unknown_series_lists_the_available_names() {
    let dir = TempDir::new("chart-unknown-series");
    let report = charted_report(&dir, "chart.ofm", CHART_MODEL);

    let output = openflex(&["chart", &path_str(&report), "--of", "series:Nope"]);
    assert_eq!(code(&output), 2, "a typo is a usage error");
    let message = stderr(&output);
    assert!(message.contains("no tracked series 'Nope'"), "{message}");
    assert!(
        message.contains("WIP"),
        "the available names must be listed: {message}"
    );
}

#[test]
fn chart_line_needs_a_series() {
    let dir = TempDir::new("chart-line-usage");
    let report = charted_report(&dir, "chart.ofm", CHART_MODEL);

    let output = openflex(&[
        "chart",
        &path_str(&report),
        "--of",
        "utilization",
        "--kind",
        "line",
    ]);
    assert_eq!(code(&output), 2);
    assert!(
        stderr(&output).contains("needs a time series"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn chart_width_is_clamped() {
    let dir = TempDir::new("chart-width");
    let report = charted_report(&dir, "chart.ofm", CHART_MODEL);
    let bars = |width: &str| {
        let output = openflex(&[
            "chart",
            &path_str(&report),
            "--of",
            "utilization",
            "--width",
            width,
            "--quiet",
        ]);
        assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
        stdout(&output)
            .lines()
            .find(|line| line.starts_with("Proc1"))
            .map(|line| line.chars().filter(|c| *c == '█').count())
            .unwrap_or(0)
    };
    assert_eq!(bars("1"), openflex_report::MIN_WIDTH, "too small clamps up");
    assert_eq!(
        bars("5000"),
        openflex_report::MAX_WIDTH,
        "too large clamps down"
    );
}

#[test]
fn chart_markdown_and_output_file() {
    let dir = TempDir::new("chart-markdown");
    let report = charted_report(&dir, "chart.ofm", CHART_MODEL);
    let path = dir.join("chart.md");

    let output = openflex(&[
        "chart",
        &path_str(&report),
        "--of",
        "utilization",
        "--format",
        "markdown",
        "--output",
        &path_str(&path),
        "--quiet",
    ]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(stdout(&output).is_empty(), "nothing must go to stdout");
    let text = fs::read_to_string(&path).expect("chart.md");
    assert!(text.starts_with("## Utilisation"), "{text}");
    assert!(text.contains("```"), "{text}");
}

#[test]
fn chart_without_measures_says_no_data() {
    let dir = TempDir::new("chart-no-data");
    // `MODEL` declares no measures and no tracked series.
    let report = charted_report(&dir, "plain.ofm", MODEL);

    let output = openflex(&["chart", &path_str(&report), "--of", "measures", "--quiet"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("(no data)"), "{}", stdout(&output));
}

/// `document` writes a model record: structure, triggers, and validation, with
/// no run involved.
#[test]
fn document_describes_the_model_without_running_it() {
    let dir = TempDir::new("document");
    let model = dir.write(
        "doc.ofm",
        r#"{
          "format": "openflex-model", "version": 1, "name": "doc-model",
          "run": { "stop_time": 50.0, "warmup": 1.0, "seed": 3, "replications": 1 },
          "user_commands": [ { "name": "ping", "body": "log(1);" } ],
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 5.0 },
                "max_arrivals": 2, "first_arrival": 0.0,
                "triggers": { "on_create": [ { "action": "script",
                  "source": "log(\"made\");" } ] } } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
    );
    let output = openflex(&["document", &path_str(&model)]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("# Model: doc-model"), "{text}");
    assert!(text.contains("| `S` | source |"), "object row: {text}");
    assert!(
        text.contains("| `S` | `K` | default |"),
        "connection row: {text}"
    );
    assert!(
        text.contains("| `S` | `on_create` |"),
        "trigger row: {text}"
    );
    assert!(text.contains("User commands: `ping`"), "{text}");
    assert!(text.contains("Valid: no issues"), "{text}");

    let output = openflex(&["document", &path_str(&model), "--format", "json"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let doc = json(&stdout(&output));
    assert_eq!(doc["object_count"].as_u64(), Some(2));
    assert_eq!(doc["connection_count"].as_u64(), Some(1));
    assert_eq!(doc["validation"]["ok"].as_bool(), Some(true));
    assert_eq!(doc["objects"][0]["name"].as_str(), Some("S"));
    assert_eq!(doc["triggers"][0]["object"].as_str(), Some("S"));

    // An invalid model is documented, not rejected: the issues are part of it.
    let broken = dir.write(
        "broken.ofm",
        r#"{
          "format": "openflex-model", "version": 1, "name": "broken",
          "run": { "stop_time": 10.0, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {} },
            { "name": "S", "kind": "sink", "params": {} }
          ],
          "connections": []
        }"#,
    );
    let output = openflex(&["document", &path_str(&broken)]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("## Validation"), "{text}");
    assert!(text.contains("issue(s)"), "issues are reported: {text}");
}

/// `run --logs` prints script log messages to stderr, so `--format json` on
/// stdout stays parseable.
#[test]
fn run_logs_prints_messages_without_corrupting_stdout() {
    let dir = TempDir::new("run-logs");
    let model = dir.write(
        "logs.ofm",
        r#"{
          "format": "openflex-model", "version": 1, "name": "run-logs",
          "run": { "stop_time": 20.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 5.0 },
                "max_arrivals": 1, "first_arrival": 0.0,
                "triggers": { "on_create": [ { "action": "script",
                  "source": "log(\"visible in stderr\");" } ] } } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
    );
    let output = openflex(&["run", &path_str(&model), "--logs", "--format", "json"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("visible in stderr"),
        "logs go to stderr: {}",
        stderr(&output)
    );
    // stdout must still be valid JSON after the logs are printed.
    let report = json(&stdout(&output));
    assert_eq!(report["completed"]["mean"].as_f64(), Some(1.0));

    // A model with no log() calls says so instead of printing nothing.
    let quiet = dir.write(
        "quiet.ofm",
        r#"{
          "format": "openflex-model", "version": 1, "name": "quiet",
          "run": { "stop_time": 10.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 5.0 },
                "max_arrivals": 1, "first_arrival": 0.0 } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
    );
    let output = openflex(&["run", &path_str(&quiet), "--logs"]);
    assert!(output.status.success());
    assert!(
        stderr(&output).contains("no script log messages"),
        "stderr: {}",
        stderr(&output)
    );
}

/// A saved trace must carry the `log()` text: the CLI used to have its own
/// writer that dropped it, so `openflex trace --logs` printed empty messages.
#[test]
fn trace_files_carry_script_log_messages() {
    let dir = TempDir::new("trace-logs");
    let model = dir.write(
        "logs.ofm",
        r#"{
          "format": "openflex-model", "version": 1, "name": "logs",
          "run": { "stop_time": 20.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 5.0 },
                "max_arrivals": 1, "first_arrival": 0.0,
                "triggers": { "on_create": [ { "action": "script",
                  "source": "log(\"hello from trace\");" } ] } } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
    );
    let trace = dir.join("events.jsonl");
    let output = openflex(&["run", &path_str(&model), "--trace", &path_str(&trace)]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let text = std::fs::read_to_string(&trace).expect("trace file");
    assert!(
        text.contains("hello from trace"),
        "the message must reach the trace file: {text}"
    );

    let output = openflex(&["trace", &path_str(&trace), "--logs"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("hello from trace"),
        "--logs must print it: {}",
        stdout(&output)
    );
}

/// `eval` is the scratchpad: it answers model questions without running the
/// model, and says so plainly when a path needs a run.
#[test]
fn eval_answers_expressions_without_running_the_model() {
    let dir = TempDir::new("eval-expr");
    let model = dir.write(
        "eval.ofm",
        r#"{
          "format": "openflex-model", "version": 1, "name": "eval-expr",
          "run": { "stop_time": 10.0, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {} },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
    );
    let output = openflex(&["eval", &path_str(&model), "--expr", "1 + 2 * 3"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output).trim(), "7");

    // `time` comes from the flag, so a formula can be checked at any instant.
    let output = openflex(&[
        "eval",
        &path_str(&model),
        "--expr",
        "time * 2",
        "--time",
        "5",
    ]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output).trim(), "10");
}

#[test]
fn eval_runs_a_sandboxed_script() {
    let dir = TempDir::new("eval-script");
    let model = dir.write(
        "eval.ofm",
        r#"{
          "format": "openflex-model", "version": 1, "name": "eval-script",
          "run": { "stop_time": 10.0, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {} },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
    );
    let output = openflex(&[
        "eval",
        &path_str(&model),
        "--script",
        r#"log("hello"); set_label("x", 7.0);"#,
    ]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("hello"), "logs are printed: {text}");
    assert!(text.contains("x = 7"), "labels are printed: {text}");
}

/// Engine state needs a run, so `eval` must say that instead of guessing a
/// value (a wrong answer here would be worse than no answer).
#[test]
fn eval_reports_paths_that_need_a_run() {
    let dir = TempDir::new("eval-state");
    let model = dir.write(
        "eval.ofm",
        r#"{
          "format": "openflex-model", "version": 1, "name": "eval-state",
          "run": { "stop_time": 10.0, "replications": 1 },
          "objects": [
            { "name": "Q", "kind": "queue", "params": {} },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "Q", "to": "K" } ]
        }"#,
    );
    let output = openflex(&["eval", &path_str(&model), "--expr", "Q.content"]);
    assert!(!output.status.success(), "an unanswerable path must fail");
    let text = stderr(&output);
    assert!(text.contains("unknown variable"), "stderr: {text}");

    // A script that references an unknown variable gets the same
    // "available:" hint the engine prints, so the scratchpad teaches too.
    let output = openflex(&["eval", &path_str(&model), "--script", "log(nope);"]);
    assert!(!output.status.success());
    let text = stderr(&output);
    assert!(text.contains("available:"), "stderr: {text}");
    assert!(text.contains("set_label("), "stderr: {text}");

    // Both inputs at once is a usage error, not a silent pick.
    let output = openflex(&["eval", &path_str(&model), "--expr", "1", "--script", "1;"]);
    assert!(!output.status.success());
}

/// A scratchpad run must see the model's user commands, exactly like a script
/// inside the model would — otherwise `eval` rejects scripts that work in the
/// real thing.
#[test]
fn eval_can_call_the_models_user_commands() {
    let dir = TempDir::new("eval-commands");
    let model = dir.write(
        "eval.ofm",
        r#"{
          "format": "openflex-model", "version": 1, "name": "eval-commands",
          "run": { "stop_time": 10.0, "replications": 1 },
          "user_commands": [
            { "name": "ping", "body": "log(\"pong\");" },
            { "name": "setx", "parameters": [ { "name": "v" } ], "body": "set_label(\"x\", v);" }
          ],
          "objects": [
            { "name": "S", "kind": "source", "params": {} },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
    );
    let output = openflex(&[
        "eval",
        &path_str(&model),
        "--script",
        r#"ping(); setx(3.0); log("x=" + label("x"));"#,
    ]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("pong"), "the command ran: {text}");
    assert!(
        text.contains("x=3.0"),
        "its argument reached the label: {text}"
    );
    // A command that does not exist still fails, and the line-number caveat is
    // printed instead of a number we cannot compute.
    let output = openflex(&["eval", &path_str(&model), "--script", "nope();"]);
    assert!(!output.status.success());
    let text = stderr(&output);
    assert!(text.contains("Function not found"), "stderr: {text}");
    assert!(
        text.contains("shifted by the model's user-command definitions"),
        "stderr: {text}"
    );
}
