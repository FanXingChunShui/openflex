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
        "id=\"delete-object\"",
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
