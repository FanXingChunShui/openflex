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

fn http_get(address: &str, path: &str) -> String {
    let mut stream = TcpStream::connect(address).expect("could not connect to the server");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .expect("request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("could not read the response");
    response
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
