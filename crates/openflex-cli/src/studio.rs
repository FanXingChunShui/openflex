//! `openflex studio` — a local, self-contained web UI for a model.
//!
//! The server is intentionally tiny: a blocking `std::net::TcpListener` on
//! `127.0.0.1`, hand-written HTTP/1.1, `serde_json` for the API, and a single
//! embedded HTML document with inline CSS/JavaScript and Canvas drawing. There
//! are no external assets, no CDN, no async runtime and no new dependencies.
//!
//! Endpoints:
//!
//! - `GET /` — the studio page
//! - `GET /api/model` — the current model (JSON)
//! - `POST /api/model` — validate and save an edited model (JSON)
//! - `GET /api/objects` — object-kind catalog for the editor
//! - `POST /api/run` — run the current model, returns a `SimReport` JSON
//! - `POST /api/step` — incrementally advance to a time/object breakpoint
//! - `POST /api/reset` — forget the incremental step state
//!
//! Pause/Resume in the UI is implemented by segmenting a run through
//! `/api/step` (checkpoint state). When the engine gains a native pause/resume
//! API this endpoint can be replaced without changing the page contract.
//! - `POST /api/experiment` — run scenario/override experiments
//! - `POST /api/table` — edit a global table cell/row through the model API
//!
//! Run control currently segments through checkpoints. When the core
//! `SimulationHandle` (step_until/pause/resume) lands, the page's `runLoop`
//! can call a native handle endpoint without changing the UI contract; the
//! wiring point is marked with `TODO(simulation-handle)`.
//! - `GET /health` — liveness probe
//!
//! Safety guards: the socket is bound to loopback only, request bodies are
//! capped at [`MAX_BODY`], headers at [`MAX_HEADER`], paths are a fixed
//! whitelist, and model saves are atomic (`write` + `rename`) after
//! `Model::from_json` validation.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use openflex_model::Model;

use crate::objects;

/// Maximum accepted request body (2 MiB).
pub const MAX_BODY: usize = 2 * 1024 * 1024;
/// Maximum accepted request header block (64 KiB).
pub const MAX_HEADER: usize = 64 * 1024;

const VERSION: &str = env!("CARGO_PKG_VERSION");
/// How many scenarios/replications the experiment endpoint accepts.
const MAX_SCENARIOS: usize = 20;
const MAX_REPLICATIONS: u32 = 100;

/// One reply from the native simulation worker.
#[derive(Debug, serde::Serialize)]
struct RunnerReply {
    report: openflex_core::SimReport,
    state: openflex_core::SimulationState,
    event: Option<openflex_core::EventPreview>,
}

#[derive(Debug, serde::Serialize)]
struct RunnerEventReply {
    event: Option<openflex_core::EventPreview>,
    state: openflex_core::SimulationState,
}

enum RunnerCommand {
    Step {
        until: f64,
        reply: std::sync::mpsc::Sender<Result<RunnerReply, String>>,
    },
    Pause {
        reply: std::sync::mpsc::Sender<Result<RunnerReply, String>>,
    },
    Resume {
        reply: std::sync::mpsc::Sender<Result<RunnerReply, String>>,
    },
    StepEvent {
        reply: std::sync::mpsc::Sender<Result<RunnerReply, String>>,
    },
    NextEvent {
        reply: std::sync::mpsc::Sender<Result<RunnerEventReply, String>>,
    },
    Restore {
        checkpoint: Box<openflex_core::Checkpoint>,
        reply: std::sync::mpsc::Sender<Result<RunnerReply, String>>,
    },
    Reset {
        reply: std::sync::mpsc::Sender<Result<RunnerReply, String>>,
    },
    Shutdown,
}

/// Owns a `SimulationHandle` on a worker thread.
///
/// A handle borrows its model, so it cannot live next to the model in the same
/// struct. The worker thread owns both and the HTTP layer talks to it through a
/// channel; this keeps the engine alive between requests (true native pause /
/// resume) without unsafe self-references.
struct NativeRunner {
    sender: std::sync::mpsc::Sender<RunnerCommand>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl NativeRunner {
    fn new(model: Model) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        let join = std::thread::spawn(move || run_worker(model, receiver));
        Self {
            sender,
            join: Some(join),
        }
    }

    fn request(
        &self,
        command: impl FnOnce(std::sync::mpsc::Sender<Result<RunnerReply, String>>) -> RunnerCommand,
    ) -> Result<RunnerReply, String> {
        let (reply, received) = std::sync::mpsc::channel();
        self.sender
            .send(command(reply))
            .map_err(|_| "the simulation worker stopped".to_string())?;
        received
            .recv()
            .map_err(|_| "the simulation worker stopped".to_string())?
    }

    fn step(&self, until: f64) -> Result<RunnerReply, String> {
        self.request(|reply| RunnerCommand::Step { until, reply })
    }

    fn pause(&self) -> Result<RunnerReply, String> {
        self.request(|reply| RunnerCommand::Pause { reply })
    }

    fn resume(&self) -> Result<RunnerReply, String> {
        self.request(|reply| RunnerCommand::Resume { reply })
    }

    fn step_event(&self) -> Result<RunnerReply, String> {
        self.request(|reply| RunnerCommand::StepEvent { reply })
    }

    fn next_event(&self) -> Result<RunnerEventReply, String> {
        let (reply, received) = std::sync::mpsc::channel();
        self.sender
            .send(RunnerCommand::NextEvent { reply })
            .map_err(|_| "the simulation worker stopped".to_string())?;
        received
            .recv()
            .map_err(|_| "the simulation worker stopped".to_string())?
    }

    fn restore(&self, checkpoint: openflex_core::Checkpoint) -> Result<RunnerReply, String> {
        self.request(|reply| RunnerCommand::Restore {
            checkpoint: Box::new(checkpoint),
            reply,
        })
    }

    fn reset(&self) -> Result<RunnerReply, String> {
        self.request(|reply| RunnerCommand::Reset { reply })
    }
}

impl Drop for NativeRunner {
    fn drop(&mut self) {
        let _ = self.sender.send(RunnerCommand::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run_worker(model: Model, receiver: std::sync::mpsc::Receiver<RunnerCommand>) {
    let config = openflex_core::SimConfig::from_run_settings(
        &model.run,
        openflex_core::derive_seed(model.run.seed, 0),
    );
    let mut handle: Option<openflex_core::SimulationHandle<'_>> = None;
    while let Ok(command) = receiver.recv() {
        match command {
            RunnerCommand::Step { until, reply } => {
                if until > model.run.stop_time + f64::EPSILON {
                    let _ = reply.send(Err(format!(
                        "step target {until} exceeds stop_time {}",
                        model.run.stop_time
                    )));
                    continue;
                }
                if handle.is_none() {
                    match openflex_core::SimulationHandle::new(&model, &config, 0) {
                        Ok(created) => handle = Some(created),
                        Err(error) => {
                            let _ = reply.send(Err(error.to_string()));
                            continue;
                        }
                    }
                }
                let Some(simulation) = handle.as_mut() else {
                    let _ = reply.send(Err("simulation handle is missing".into()));
                    continue;
                };
                if let Err(error) = simulation.step_until(until) {
                    let _ = reply.send(Err(error.to_string()));
                    continue;
                }
                let mut report_model = model.clone();
                report_model.run.stop_time = until;
                report_model.run.replications = 1;
                let report = openflex_core::SimReport::from_replications(
                    &report_model,
                    vec![simulation.result()],
                );
                let _ = reply.send(Ok(RunnerReply {
                    report,
                    state: simulation.state(),
                    event: None,
                }));
            }
            RunnerCommand::Pause { reply } => {
                if handle.is_none() {
                    match openflex_core::SimulationHandle::new(&model, &config, 0) {
                        Ok(created) => handle = Some(created),
                        Err(error) => {
                            let _ = reply.send(Err(error.to_string()));
                            continue;
                        }
                    }
                }
                let simulation = handle.as_mut().expect("handle was just created");
                if let Err(error) = simulation.pause() {
                    let _ = reply.send(Err(error.to_string()));
                    continue;
                }
                let mut report_model = model.clone();
                report_model.run.stop_time = simulation.now();
                report_model.run.replications = 1;
                let report = openflex_core::SimReport::from_replications(
                    &report_model,
                    vec![simulation.result()],
                );
                let _ = reply.send(Ok(RunnerReply {
                    report,
                    state: simulation.state(),
                    event: None,
                }));
            }
            RunnerCommand::Resume { reply } => {
                let Some(simulation) = handle.as_mut() else {
                    let _ = reply.send(Err("nothing has been started yet".into()));
                    continue;
                };
                if let Err(error) = simulation.resume() {
                    let _ = reply.send(Err(error.to_string()));
                    continue;
                }
                let mut report_model = model.clone();
                report_model.run.stop_time = simulation.now();
                report_model.run.replications = 1;
                let report = openflex_core::SimReport::from_replications(
                    &report_model,
                    vec![simulation.result()],
                );
                let _ = reply.send(Ok(RunnerReply {
                    report,
                    state: simulation.state(),
                    event: None,
                }));
            }
            RunnerCommand::StepEvent { reply } => {
                if handle.is_none() {
                    match openflex_core::SimulationHandle::new(&model, &config, 0) {
                        Ok(created) => handle = Some(created),
                        Err(error) => {
                            let _ = reply.send(Err(error.to_string()));
                            continue;
                        }
                    }
                }
                let simulation = handle.as_mut().expect("handle was just created");
                match simulation.step_event() {
                    Ok(event) => {
                        let mut report_model = model.clone();
                        report_model.run.stop_time = simulation.now();
                        report_model.run.replications = 1;
                        let report = openflex_core::SimReport::from_replications(
                            &report_model,
                            vec![simulation.result()],
                        );
                        let _ = reply.send(Ok(RunnerReply {
                            report,
                            state: simulation.state(),
                            event,
                        }));
                    }
                    Err(error) => {
                        let _ = reply.send(Err(error.to_string()));
                    }
                }
            }
            RunnerCommand::NextEvent { reply } => {
                if handle.is_none() {
                    match openflex_core::SimulationHandle::new(&model, &config, 0) {
                        Ok(created) => handle = Some(created),
                        Err(error) => {
                            let _ = reply.send(Err(error.to_string()));
                            continue;
                        }
                    }
                }
                let simulation = handle.as_ref().expect("handle was just created");
                let _ = reply.send(Ok(RunnerEventReply {
                    event: simulation.next_event(),
                    state: simulation.state(),
                }));
            }
            RunnerCommand::Restore { checkpoint, reply } => {
                // A checkpoint can come from any replication, so build the
                // handle with the checkpoint's derived seed instead of the
                // studio's default replication-0 seed.
                let restore_config = openflex_core::SimConfig {
                    stop_time: model.run.stop_time,
                    warmup: checkpoint.warmup(),
                    seed: checkpoint.seed(),
                };
                let mut simulation =
                    match openflex_core::SimulationHandle::new(&model, &restore_config, 0) {
                        Ok(simulation) => simulation,
                        Err(error) => {
                            let _ = reply.send(Err(error.to_string()));
                            continue;
                        }
                    };
                if let Err(error) = simulation.restore(&checkpoint) {
                    let _ = reply.send(Err(error.to_string()));
                    continue;
                }
                let mut report_model = model.clone();
                report_model.run.stop_time = simulation.now();
                report_model.run.replications = 1;
                let report = openflex_core::SimReport::from_replications(
                    &report_model,
                    vec![simulation.result()],
                );
                let state = simulation.state();
                handle = Some(simulation);
                let _ = reply.send(Ok(RunnerReply {
                    report,
                    state,
                    event: None,
                }));
            }
            RunnerCommand::Reset { reply } => {
                handle = None;
                let _ = reply.send(Ok(RunnerReply {
                    report: openflex_core::SimReport::from_replications(&model, Vec::new()),
                    state: openflex_core::SimulationState {
                        now: 0.0,
                        paused: false,
                        items_created: 0,
                        items_completed: 0,
                        objects: std::collections::BTreeMap::new(),
                    },
                    event: None,
                }));
            }
            RunnerCommand::Shutdown => break,
        }
    }
}

fn ensure_runner(state: &mut StudioState) -> &NativeRunner {
    if state.runner.is_none() {
        state.runner = Some(NativeRunner::new(state.model.clone()));
    }
    state.runner.as_ref().expect("runner was just created")
}

struct StudioState {
    model: Model,
    runner: Option<NativeRunner>,
    checkpoints: PathBuf,
}

/// Serve the studio until the process is killed.
pub fn serve(
    model_path: &Path,
    port: u16,
    open: bool,
    quiet: bool,
    checkpoints: &Path,
) -> Result<(), String> {
    let text = fs::read_to_string(model_path)
        .map_err(|error| format!("could not read model {}: {error}", model_path.display()))?;
    let model = Model::from_json(&text)
        .map_err(|error| format!("invalid model {}: {error}", model_path.display()))?;
    let mut state = StudioState {
        model,
        runner: None,
        checkpoints: checkpoints.to_path_buf(),
    };
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("could not bind 127.0.0.1:{port}: {error}"))?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("could not read the listening address: {error}"))?;
    let url = format!("http://{address}");
    eprintln!(
        "openflex studio listening on {url} (model {})",
        model_path.display()
    );
    if !quiet {
        eprintln!("open {url} in a browser; press Ctrl-C to stop");
    }
    if open {
        open_browser(&url);
    }

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                if let Err(error) = handle_connection(stream, &mut state, model_path) {
                    eprintln!("studio request failed: {error}");
                }
            }
            Err(error) => eprintln!("studio connection failed: {error}"),
        }
    }
    Ok(())
}

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
}

struct HttpError {
    status: &'static str,
    message: String,
}

impl HttpError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: "400 Bad Request",
            message: message.into(),
        }
    }
}

fn handle_connection(
    mut stream: TcpStream,
    state: &mut StudioState,
    model_path: &Path,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let request = match read_request(&stream) {
        Ok(request) => request,
        Err(error) => {
            write_json(
                &mut stream,
                error.status,
                &serde_json::json!({ "ok": false, "error": error.message }).to_string(),
            )?;
            return Ok(());
        }
    };

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/" | "/index.html") => write_response(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            &index_html(&state.model),
        ),
        ("GET", "/api/model") => {
            let text = serde_json::to_string_pretty(&state.model).unwrap_or_else(|_| "{}".into());
            write_response(&mut stream, "200 OK", "application/json", &text)
        }
        ("GET", "/api/objects") => write_response(
            &mut stream,
            "200 OK",
            "application/json",
            &objects::objects_json().to_string(),
        ),
        ("GET", "/health") => write_response(
            &mut stream,
            "200 OK",
            "application/json",
            &serde_json::json!({ "status": "ok", "version": VERSION }).to_string(),
        ),
        ("POST", "/api/model") => save_model(&mut stream, state, model_path, &request.body),
        ("POST", "/api/model/objects") => add_object(&mut stream, state, model_path, &request.body),
        ("POST", "/api/run") => run_model_endpoint(&mut stream, state, &request.body),
        ("POST", "/api/step") => step_endpoint(&mut stream, state, &request.body),
        ("POST", "/api/step-event") => step_event_endpoint(&mut stream, state),
        ("GET", "/api/next-event") => next_event_endpoint(&mut stream, state),
        ("GET", "/api/checkpoints") => list_checkpoints_endpoint(&mut stream, state),
        ("POST", "/api/resume-checkpoint") => {
            resume_checkpoint_endpoint(&mut stream, state, &request.body)
        }
        ("POST", "/api/verify-checkpoint") => {
            verify_checkpoint_endpoint(&mut stream, state, &request.body)
        }
        ("POST", "/api/pause") => {
            let runner = ensure_runner(state);
            match runner.pause() {
                Ok(reply) => write_json(
                    &mut stream,
                    "200 OK",
                    &serde_json::json!({
                        "ok": true,
                        "report": reply.report,
                        "state": reply.state,
                    })
                    .to_string(),
                ),
                Err(error) => write_json(
                    &mut stream,
                    "422 Unprocessable Entity",
                    &serde_json::json!({ "ok": false, "error": error }).to_string(),
                ),
            }
        }
        ("POST", "/api/resume") => {
            let runner = ensure_runner(state);
            match runner.resume() {
                Ok(reply) => write_json(
                    &mut stream,
                    "200 OK",
                    &serde_json::json!({
                        "ok": true,
                        "report": reply.report,
                        "state": reply.state,
                    })
                    .to_string(),
                ),
                Err(error) => write_json(
                    &mut stream,
                    "422 Unprocessable Entity",
                    &serde_json::json!({ "ok": false, "error": error }).to_string(),
                ),
            }
        }
        ("POST", "/api/reset") => {
            if let Some(runner) = state.runner.as_ref() {
                let _ = runner.reset();
            }
            write_json(
                &mut stream,
                "200 OK",
                &serde_json::json!({ "ok": true, "stepped_to": 0.0 }).to_string(),
            )
        }
        ("POST", "/api/experiment") => {
            experiment_endpoint(&mut stream, &state.model, &request.body)
        }
        ("POST", "/api/timeseries") => {
            timeseries_endpoint(&mut stream, &state.model, &request.body)
        }
        ("POST", "/api/table") => table_endpoint(&mut stream, state, model_path, &request.body),
        (method, path) if method == "POST" && path.starts_with("/api/model/objects/") => {
            if let Some(old) = path
                .strip_prefix("/api/model/objects/")
                .and_then(|rest| rest.strip_suffix("/rename"))
            {
                let old = match percent_decode(old) {
                    Ok(name) => name,
                    Err(error) => return bad_path(&mut stream, error),
                };
                rename_object(&mut stream, state, model_path, &old, &request.body)
            } else if let Some(old) = path
                .strip_prefix("/api/model/objects/")
                .and_then(|rest| rest.strip_suffix("/duplicate"))
            {
                let old = match percent_decode(old) {
                    Ok(name) => name,
                    Err(error) => return bad_path(&mut stream, error),
                };
                duplicate_object(&mut stream, state, model_path, &old)
            } else {
                not_found(&mut stream)
            }
        }
        (method, path) if method == "DELETE" && path.starts_with("/api/model/objects/") => {
            let name = match percent_decode(path.trim_start_matches("/api/model/objects/")) {
                Ok(name) => name,
                Err(error) => {
                    return write_json(
                        &mut stream,
                        "400 Bad Request",
                        &serde_json::json!({ "ok": false, "error": error }).to_string(),
                    );
                }
            };
            delete_object(&mut stream, state, model_path, &name)
        }
        ("GET", path) if path.starts_with("/api/") || path.starts_with("/assets/") => {
            not_found(&mut stream)
        }
        _ => not_found(&mut stream),
    }
}

fn not_found(stream: &mut TcpStream) -> std::io::Result<()> {
    write_json(
        stream,
        "404 Not Found",
        &serde_json::json!({ "ok": false, "error": "unknown endpoint" }).to_string(),
    )
}

fn save_model(
    stream: &mut TcpStream,
    state: &mut StudioState,
    model_path: &Path,
    body: &[u8],
) -> std::io::Result<()> {
    let text = match std::str::from_utf8(body) {
        Ok(text) => text,
        Err(error) => {
            return write_json(
                stream,
                "400 Bad Request",
                &serde_json::json!({ "ok": false, "error": format!("body is not UTF-8: {error}") })
                    .to_string(),
            );
        }
    };
    let new_model = match Model::from_json(text) {
        Ok(model) => model,
        Err(error) => {
            return write_json(
                stream,
                "422 Unprocessable Entity",
                &serde_json::json!({ "ok": false, "error": format!("invalid model: {error}") })
                    .to_string(),
            );
        }
    };
    let pretty = match serde_json::to_string_pretty(&new_model) {
        Ok(pretty) => format!("{pretty}\n"),
        Err(error) => {
            return write_json(
                stream,
                "500 Internal Server Error",
                &serde_json::json!({ "ok": false, "error": error.to_string() }).to_string(),
            );
        }
    };
    if let Err(error) = write_atomic(model_path, &pretty) {
        return write_json(
            stream,
            "500 Internal Server Error",
            &serde_json::json!({ "ok": false, "error": error }).to_string(),
        );
    }
    state.model = new_model;
    state.runner = None;
    write_json(
        stream,
        "200 OK",
        &serde_json::json!({
            "ok": true,
            "name": state.model.name,
            "objects": state.model.objects.len()
        })
        .to_string(),
    )
}

#[derive(Debug, Default, serde::Deserialize)]
struct RunOverrides {
    stop_time: Option<f64>,
    seed: Option<u64>,
    replications: Option<u32>,
}

fn run_model_endpoint(
    stream: &mut TcpStream,
    state: &mut StudioState,
    body: &[u8],
) -> std::io::Result<()> {
    let overrides: RunOverrides = if body.is_empty() || body == b"{}" {
        RunOverrides::default()
    } else {
        match serde_json::from_slice(body) {
            Ok(overrides) => overrides,
            Err(error) => {
                return write_json(
                    stream,
                    "400 Bad Request",
                    &serde_json::json!({ "ok": false, "error": format!("invalid run options: {error}") })
                        .to_string(),
                );
            }
        }
    };
    state.runner = None;
    let mut run_model = state.model.clone();
    if let Some(stop_time) = overrides.stop_time {
        if stop_time <= 0.0 || stop_time <= run_model.run.warmup {
            return write_json(
                stream,
                "422 Unprocessable Entity",
                &serde_json::json!({ "ok": false, "error": "stop_time must be greater than warm-up" })
                    .to_string(),
            );
        }
        run_model.run.stop_time = stop_time;
    }
    if let Some(seed) = overrides.seed {
        run_model.run.seed = seed;
    }
    if let Some(replications) = overrides.replications {
        if replications == 0 {
            return write_json(
                stream,
                "422 Unprocessable Entity",
                &serde_json::json!({ "ok": false, "error": "replications must be at least 1" })
                    .to_string(),
            );
        }
        run_model.run.replications = replications;
    }
    if let Err(error) = run_model.validate() {
        return write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": format!("invalid model: {error}") })
                .to_string(),
        );
    }
    match openflex_core::run_model(&run_model) {
        Ok(report) => {
            let text = serde_json::to_string(&report).unwrap_or_else(|_| "{}".into());
            write_response(stream, "200 OK", "application/json", &text)
        }
        Err(error) => {
            let status = match error {
                openflex_core::SimError::Config(_) | openflex_core::SimError::Model(_) => {
                    "422 Unprocessable Entity"
                }
                _ => "500 Internal Server Error",
            };
            write_json(
                stream,
                status,
                &serde_json::json!({ "ok": false, "error": error.to_string() }).to_string(),
            )
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct StepRequest {
    until: f64,
    #[serde(default)]
    object: Option<String>,
}

/// Advance one replication to `until` (or to the first event of `object`),
/// reusing the checkpoint from the previous step.
fn step_endpoint(
    stream: &mut TcpStream,
    state: &mut StudioState,
    body: &[u8],
) -> std::io::Result<()> {
    let request: StepRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(error) => {
            return write_json(
                stream,
                "400 Bad Request",
                &serde_json::json!({ "ok": false, "error": format!("invalid step request: {error}") })
                    .to_string(),
            );
        }
    };
    if !request.until.is_finite() || request.until <= 0.0 {
        return write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": "until must be a positive finite time" })
                .to_string(),
        );
    }
    if request.until <= state.model.run.warmup {
        return write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": "until must be greater than the warm-up" })
                .to_string(),
        );
    }
    let seed = openflex_core::derive_seed(state.model.run.seed, 0);
    let warmup = state.model.run.warmup;

    // An object breakpoint is resolved with one traced pass over the requested
    // window; the native handle then steps to the event time.
    let mut target = request.until;
    let mut breakpoint = serde_json::Value::Null;
    if let Some(object) = request
        .object
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        let config = openflex_core::SimConfig {
            stop_time: request.until,
            warmup,
            seed,
        };
        match openflex_core::simulate_traced(&state.model, &config, 0) {
            Ok((_, trace)) => {
                if let Some(event) = trace.events.iter().find(|event| {
                    event.object == object
                        && event.time > warmup + 1e-9
                        && event.time <= request.until + 1e-9
                }) {
                    target = event.time.min(request.until);
                    breakpoint = serde_json::json!({
                        "object": event.object,
                        "time": event.time,
                        "event": event.event,
                    });
                }
            }
            Err(error) => return write_sim_error(stream, error),
        }
    }

    let runner = ensure_runner(state);
    let reply = match runner.step(target) {
        Ok(reply) => reply,
        Err(error) => {
            return write_json(
                stream,
                "422 Unprocessable Entity",
                &serde_json::json!({ "ok": false, "error": error }).to_string(),
            );
        }
    };
    let report = reply.report;
    let text = serde_json::json!({
        "report": report,
        "stepped_to": target,
        "breakpoint": breakpoint,
    });
    write_json(stream, "200 OK", &text.to_string())
}

#[derive(Debug, serde::Deserialize)]
struct ExperimentRequest {
    #[serde(default)]
    experiment: Option<String>,
    #[serde(default)]
    replications: Option<u32>,
    #[serde(default)]
    scenarios: Vec<ExperimentScenario>,
}

#[derive(Debug, serde::Deserialize)]
struct ExperimentScenario {
    name: String,
    #[serde(default)]
    overrides: Vec<ExperimentOverride>,
}

#[derive(Debug, serde::Deserialize)]
struct ExperimentOverride {
    object: String,
    param: String,
    value: serde_json::Value,
}

/// Run scenario/parameter experiments inside the studio.
fn experiment_endpoint(stream: &mut TcpStream, model: &Model, body: &[u8]) -> std::io::Result<()> {
    let request: ExperimentRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(error) => {
            return write_json(
                stream,
                "400 Bad Request",
                &serde_json::json!({ "ok": false, "error": format!("invalid experiment request: {error}") })
                    .to_string(),
            );
        }
    };
    let scenarios = if request.scenarios.is_empty() {
        vec![ExperimentScenario {
            name: "baseline".into(),
            overrides: Vec::new(),
        }]
    } else {
        request.scenarios
    };
    if scenarios.len() > MAX_SCENARIOS {
        return write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": format!("at most {MAX_SCENARIOS} scenarios are supported") })
                .to_string(),
        );
    }
    let replications = request
        .replications
        .unwrap_or(model.run.replications.max(1));
    if replications == 0 || replications > MAX_REPLICATIONS {
        return write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": format!("replications must be between 1 and {MAX_REPLICATIONS}") })
                .to_string(),
        );
    }

    let mut reports = Vec::with_capacity(scenarios.len());
    for scenario in scenarios {
        let mut scenario_model = model.clone();
        for over in &scenario.overrides {
            scenario_model = match scenario_model.with_override(
                &over.object,
                &over.param,
                over.value.clone(),
            ) {
                Ok(model) => model,
                Err(error) => {
                    return write_json(
                        stream,
                        "422 Unprocessable Entity",
                        &serde_json::json!({ "ok": false, "error": format!("scenario '{}': {error}", scenario.name) })
                            .to_string(),
                    );
                }
            };
        }
        scenario_model.run.replications = replications;
        if let Err(error) = scenario_model.validate() {
            return write_json(
                stream,
                "422 Unprocessable Entity",
                &serde_json::json!({ "ok": false, "error": format!("scenario '{}': {error}", scenario.name) })
                    .to_string(),
            );
        }
        match openflex_core::run_model(&scenario_model) {
            Ok(report) => reports.push(openflex_report::ScenarioReport {
                name: scenario.name.clone(),
                overrides: scenario
                    .overrides
                    .iter()
                    .map(|over| openflex_model::Override {
                        object: over.object.clone(),
                        param: over.param.clone(),
                        value: over.value.clone(),
                    })
                    .collect(),
                report,
            }),
            Err(error) => return write_sim_error(stream, error),
        }
    }
    let experiment = openflex_report::ExperimentReport {
        experiment: request.experiment.unwrap_or_else(|| "studio".into()),
        model: model.name.clone(),
        time_unit: model.time_unit,
        scenarios: reports,
    };
    let text = serde_json::to_string(&experiment).unwrap_or_else(|_| "{}".into());
    write_response(stream, "200 OK", "application/json", &text)
}

fn write_sim_error(stream: &mut TcpStream, error: openflex_core::SimError) -> std::io::Result<()> {
    let status = match error {
        openflex_core::SimError::Config(_) | openflex_core::SimError::Model(_) => {
            "422 Unprocessable Entity"
        }
        _ => "500 Internal Server Error",
    };
    write_json(
        stream,
        status,
        &serde_json::json!({ "ok": false, "error": error.to_string() }).to_string(),
    )
}

#[derive(Debug, serde::Deserialize)]
struct AddObjectRequest {
    kind: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    pos: Option<[f64; 2]>,
}

/// Add an object with kind-specific safe defaults, then validate and save.
fn add_object(
    stream: &mut TcpStream,
    state: &mut StudioState,
    model_path: &Path,
    body: &[u8],
) -> std::io::Result<()> {
    let request: AddObjectRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(error) => {
            return write_json(
                stream,
                "400 Bad Request",
                &serde_json::json!({ "ok": false, "error": format!("invalid add request: {error}") })
                    .to_string(),
            );
        }
    };
    let kind = match openflex_model::ObjectKind::parse(&request.kind.to_ascii_lowercase()) {
        Some(kind) => kind,
        None => {
            return write_json(
                stream,
                "422 Unprocessable Entity",
                &serde_json::json!({ "ok": false, "error": format!("unknown object kind '{}'", request.kind) })
                    .to_string(),
            );
        }
    };
    let mut value = serde_json::to_value(&state.model).unwrap_or_else(|_| serde_json::json!({}));
    let Some(objects) = value
        .get_mut("objects")
        .and_then(|objects| objects.as_array_mut())
    else {
        return write_json(
            stream,
            "500 Internal Server Error",
            &serde_json::json!({ "ok": false, "error": "model has no objects array" }).to_string(),
        );
    };
    let name = match request
        .name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
    {
        Some(name) => name,
        None => unique_object_name(objects, kind.as_str()),
    };
    if objects
        .iter()
        .any(|object| object.get("name").and_then(|name| name.as_str()) == Some(name.as_str()))
    {
        return write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": format!("an object named '{name}' already exists") })
                .to_string(),
        );
    }
    let pos = request.pos.unwrap_or_else(|| next_position(objects));
    objects.push(serde_json::json!({
        "name": name,
        "kind": kind.as_str(),
        "pos": pos,
        "params": default_params(kind),
    }));
    let new_model = match Model::from_json(&value.to_string()) {
        Ok(model) => model,
        Err(error) => {
            return write_json(
                stream,
                "422 Unprocessable Entity",
                &serde_json::json!({ "ok": false, "error": format!("invalid model after add: {error}") })
                    .to_string(),
            );
        }
    };
    persist_model(
        stream,
        state,
        model_path,
        new_model,
        serde_json::json!({ "added": name }),
    )
}

/// Delete one object and clean connections/references to it, then validate.
fn delete_object(
    stream: &mut TcpStream,
    state: &mut StudioState,
    model_path: &Path,
    name: &str,
) -> std::io::Result<()> {
    let mut value = serde_json::to_value(&state.model).unwrap_or_else(|_| serde_json::json!({}));
    let Some(objects) = value
        .get_mut("objects")
        .and_then(|objects| objects.as_array_mut())
    else {
        return write_json(
            stream,
            "500 Internal Server Error",
            &serde_json::json!({ "ok": false, "error": "model has no objects array" }).to_string(),
        );
    };
    let Some(index) = objects
        .iter()
        .position(|object| object.get("name").and_then(|name| name.as_str()) == Some(name))
    else {
        return write_json(
            stream,
            "404 Not Found",
            &serde_json::json!({ "ok": false, "error": format!("unknown object '{name}'") })
                .to_string(),
        );
    };
    objects.remove(index);
    let connections_removed = value
        .get_mut("connections")
        .and_then(|connections| connections.as_array_mut())
        .map(|connections| {
            let before = connections.len();
            connections.retain(|connection| {
                connection.get("from").and_then(|from| from.as_str()) != Some(name)
                    && connection.get("to").and_then(|to| to.as_str()) != Some(name)
            });
            before - connections.len()
        })
        .unwrap_or(0);
    let references_cleaned = value
        .get_mut("objects")
        .and_then(|objects| objects.as_array_mut())
        .map(|objects| {
            objects
                .iter_mut()
                .map(|object| {
                    object
                        .get_mut("params")
                        .map(|params| clean_references(params, name))
                        .unwrap_or(0)
                })
                .sum()
        })
        .unwrap_or(0);
    let new_model = match Model::from_json(&value.to_string()) {
        Ok(model) => model,
        Err(error) => {
            return write_json(
                stream,
                "422 Unprocessable Entity",
                &serde_json::json!({ "ok": false, "error": format!("invalid model after delete: {error}") })
                    .to_string(),
            );
        }
    };
    persist_model(
        stream,
        state,
        model_path,
        new_model,
        serde_json::json!({
            "removed": name,
            "connections_removed": connections_removed,
            "references_cleaned": references_cleaned,
        }),
    )
}

/// Remove `home`/`charger`/`dispatcher`-style references to a deleted object.
fn clean_references(value: &mut serde_json::Value, name: &str) -> usize {
    match value {
        serde_json::Value::Object(map) => {
            let mut cleaned = 0;
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                let remove = matches!(
                    key.as_str(),
                    "home"
                        | "charger"
                        | "dispatcher"
                        | "home_floor"
                        | "setup_operator"
                        | "process_operator"
                ) && map.get(&key).and_then(|value| value.as_str()) == Some(name);
                if remove {
                    map.remove(&key);
                    cleaned += 1;
                } else if let Some(child) = map.get_mut(&key) {
                    cleaned += clean_references(child, name);
                }
            }
            cleaned
        }
        serde_json::Value::Array(items) => items
            .iter_mut()
            .map(|item| clean_references(item, name))
            .sum(),
        _ => 0,
    }
}

fn default_params(kind: openflex_model::ObjectKind) -> serde_json::Value {
    if kind == openflex_model::ObjectKind::Source {
        // The engine default inter-arrival is 0, which would create items at
        // the same instant forever; a studio-added source gets a safe default.
        serde_json::json!({ "interarrival": { "distribution": "constant", "value": 5.0 } })
    } else {
        serde_json::json!({})
    }
}

fn unique_object_name(objects: &[serde_json::Value], kind: &str) -> String {
    let prefix = {
        let mut chars = kind.chars();
        match chars.next() {
            Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
            None => "Object".to_string(),
        }
    };
    for suffix in 1.. {
        let candidate = format!("{prefix}{suffix}");
        if !objects.iter().any(|object| {
            object.get("name").and_then(|name| name.as_str()) == Some(candidate.as_str())
        }) {
            return candidate;
        }
    }
    unreachable!("an unused suffix always exists")
}

fn next_position(objects: &[serde_json::Value]) -> [f64; 2] {
    let max_x = objects
        .iter()
        .filter_map(|object| {
            object
                .get("pos")
                .and_then(|pos| pos.as_array())
                .and_then(|pos| pos.first())
                .and_then(|x| x.as_f64())
        })
        .fold(0.0_f64, f64::max);
    [max_x + 150.0, 200.0]
}

/// Persist a validated model atomically and return it to the UI.
fn persist_model(
    stream: &mut TcpStream,
    state: &mut StudioState,
    model_path: &Path,
    new_model: Model,
    extra: serde_json::Value,
) -> std::io::Result<()> {
    let pretty = match serde_json::to_string_pretty(&new_model) {
        Ok(pretty) => format!("{pretty}\n"),
        Err(error) => {
            return write_json(
                stream,
                "500 Internal Server Error",
                &serde_json::json!({ "ok": false, "error": error.to_string() }).to_string(),
            );
        }
    };
    if let Err(error) = write_atomic(model_path, &pretty) {
        return write_json(
            stream,
            "500 Internal Server Error",
            &serde_json::json!({ "ok": false, "error": error }).to_string(),
        );
    }
    state.model = new_model;
    state.runner = None;
    let mut response = serde_json::json!({ "ok": true, "model": state.model });
    if let (Some(target), Some(source)) = (response.as_object_mut(), extra.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    write_json(stream, "200 OK", &response.to_string())
}

fn percent_decode(input: &str) -> Result<String, String> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                if index + 2 >= bytes.len() {
                    return Err(format!("invalid percent-encoding in '{input}'"));
                }
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                    .map_err(|_| format!("invalid percent-encoding in '{input}'"))?;
                let byte = u8::from_str_radix(hex, 16)
                    .map_err(|_| format!("invalid percent-encoding in '{input}'"))?;
                decoded.push(byte);
                index += 3;
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).map_err(|_| format!("invalid UTF-8 in path '{input}'"))
}

fn bad_path(stream: &mut TcpStream, error: String) -> std::io::Result<()> {
    write_json(
        stream,
        "400 Bad Request",
        &serde_json::json!({ "ok": false, "error": error }).to_string(),
    )
}

#[derive(Debug, serde::Deserialize)]
struct RenameRequest {
    name: String,
}

/// Rename an object and update connections/references that point at it.
fn rename_object(
    stream: &mut TcpStream,
    state: &mut StudioState,
    model_path: &Path,
    old_name: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let request: RenameRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(error) => {
            return write_json(
                stream,
                "400 Bad Request",
                &serde_json::json!({ "ok": false, "error": format!("invalid rename request: {error}") })
                    .to_string(),
            );
        }
    };
    let new_name = request.name.trim();
    if new_name.is_empty() {
        return write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": "the new object name cannot be empty" })
                .to_string(),
        );
    }
    if new_name == old_name {
        return write_json(
            stream,
            "200 OK",
            &serde_json::json!({ "ok": true, "renamed": new_name, "model": state.model })
                .to_string(),
        );
    }
    let mut value = serde_json::to_value(&state.model).unwrap_or_else(|_| serde_json::json!({}));
    let Some(objects) = value
        .get_mut("objects")
        .and_then(|objects| objects.as_array_mut())
    else {
        return write_json(
            stream,
            "500 Internal Server Error",
            &serde_json::json!({ "ok": false, "error": "model has no objects array" }).to_string(),
        );
    };
    if objects
        .iter()
        .any(|object| object.get("name").and_then(|name| name.as_str()) == Some(new_name))
    {
        return write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": format!("an object named '{new_name}' already exists") })
                .to_string(),
        );
    }
    let Some(index) = objects
        .iter()
        .position(|object| object.get("name").and_then(|name| name.as_str()) == Some(old_name))
    else {
        return write_json(
            stream,
            "404 Not Found",
            &serde_json::json!({ "ok": false, "error": format!("unknown object '{old_name}'") })
                .to_string(),
        );
    };
    objects[index]["name"] = serde_json::json!(new_name);
    let mut references = 0;
    if let Some(connections) = value
        .get_mut("connections")
        .and_then(|connections| connections.as_array_mut())
    {
        for connection in connections {
            for side in ["from", "to"] {
                if connection.get(side).and_then(|name| name.as_str()) == Some(old_name) {
                    connection[side] = serde_json::json!(new_name);
                    references += 1;
                }
            }
        }
    }
    if let Some(objects) = value
        .get_mut("objects")
        .and_then(|objects| objects.as_array_mut())
    {
        for object in objects {
            if let Some(params) = object.get_mut("params") {
                references += rename_references(params, old_name, new_name);
            }
        }
    }
    let new_model = match Model::from_json(&value.to_string()) {
        Ok(model) => model,
        Err(error) => {
            return write_json(
                stream,
                "422 Unprocessable Entity",
                &serde_json::json!({ "ok": false, "error": format!("invalid model after rename: {error}") })
                    .to_string(),
            );
        }
    };
    persist_model(
        stream,
        state,
        model_path,
        new_model,
        serde_json::json!({ "renamed": new_name, "references_updated": references }),
    )
}

/// Recursively rename `home`/`charger`-style references.
fn rename_references(value: &mut serde_json::Value, old: &str, new: &str) -> usize {
    match value {
        serde_json::Value::Object(map) => {
            let mut updated = 0;
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                let replace = matches!(
                    key.as_str(),
                    "home"
                        | "charger"
                        | "dispatcher"
                        | "home_floor"
                        | "setup_operator"
                        | "process_operator"
                ) && map.get(&key).and_then(|value| value.as_str()) == Some(old);
                if replace {
                    map.insert(key, serde_json::json!(new));
                    updated += 1;
                } else if let Some(child) = map.get_mut(&key) {
                    updated += rename_references(child, old, new);
                }
            }
            updated
        }
        serde_json::Value::Array(items) => items
            .iter_mut()
            .map(|item| rename_references(item, old, new))
            .sum(),
        _ => 0,
    }
}

/// Copy an object with a unique name and a small position offset.
fn duplicate_object(
    stream: &mut TcpStream,
    state: &mut StudioState,
    model_path: &Path,
    name: &str,
) -> std::io::Result<()> {
    let mut value = serde_json::to_value(&state.model).unwrap_or_else(|_| serde_json::json!({}));
    let Some(objects) = value
        .get_mut("objects")
        .and_then(|objects| objects.as_array_mut())
    else {
        return write_json(
            stream,
            "500 Internal Server Error",
            &serde_json::json!({ "ok": false, "error": "model has no objects array" }).to_string(),
        );
    };
    let Some(source) = objects
        .iter()
        .find(|object| object.get("name").and_then(|name| name.as_str()) == Some(name))
        .cloned()
    else {
        return write_json(
            stream,
            "404 Not Found",
            &serde_json::json!({ "ok": false, "error": format!("unknown object '{name}'") })
                .to_string(),
        );
    };
    let copy_name = unique_copy_name(objects, name);
    let mut copy = source;
    copy["name"] = serde_json::json!(copy_name);
    if let Some(pos) = copy.get_mut("pos").and_then(|pos| pos.as_array_mut()) {
        if let Some(x) = pos.first_mut() {
            *x = serde_json::json!(x.as_f64().unwrap_or(0.0) + 40.0);
        }
        if let Some(y) = pos.get_mut(1) {
            *y = serde_json::json!(y.as_f64().unwrap_or(0.0) + 40.0);
        }
    }
    objects.push(copy);
    let new_model = match Model::from_json(&value.to_string()) {
        Ok(model) => model,
        Err(error) => {
            return write_json(
                stream,
                "422 Unprocessable Entity",
                &serde_json::json!({ "ok": false, "error": format!("invalid model after duplicate: {error}") })
                    .to_string(),
            );
        }
    };
    persist_model(
        stream,
        state,
        model_path,
        new_model,
        serde_json::json!({ "duplicated": copy_name }),
    )
}

fn unique_copy_name(objects: &[serde_json::Value], source: &str) -> String {
    for suffix in 2.. {
        let candidate = format!("{source}Copy{suffix}");
        if !objects.iter().any(|object| {
            object.get("name").and_then(|name| name.as_str()) == Some(candidate.as_str())
        }) {
            return candidate;
        }
    }
    unreachable!("an unused suffix always exists")
}

#[derive(Debug, serde::Deserialize)]
struct TimeseriesRequest {
    interval: f64,
}

/// Sample interval-average object content for the dashboard timeseries chart.
fn timeseries_endpoint(stream: &mut TcpStream, model: &Model, body: &[u8]) -> std::io::Result<()> {
    let request: TimeseriesRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(error) => {
            return write_json(
                stream,
                "400 Bad Request",
                &serde_json::json!({ "ok": false, "error": format!("invalid timeseries request: {error}") })
                    .to_string(),
            );
        }
    };
    if !request.interval.is_finite() || request.interval <= 0.0 {
        return write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": "interval must be positive" }).to_string(),
        );
    }
    let stop = model.run.stop_time;
    let warmup = model.run.warmup;
    if stop <= warmup {
        return write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": "stop_time must be greater than warm-up" })
                .to_string(),
        );
    }
    if (stop - warmup) / request.interval > 5_000.0 {
        return write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": "too many samples; increase interval" })
                .to_string(),
        );
    }
    let seed = openflex_core::derive_seed(model.run.seed, 0);
    let mut previous = None;
    let mut previous_time = warmup;
    let mut previous_cumulative = std::collections::BTreeMap::new();
    let mut rows = Vec::new();
    let mut target = warmup + request.interval;
    let mut object_names = Vec::new();
    loop {
        let target_time = target.min(stop);
        let config = openflex_core::SimConfig {
            stop_time: target_time,
            warmup,
            seed,
        };
        let (result, checkpoint) =
            match openflex_core::simulate_with_checkpoint(model, &config, 0, previous.as_ref()) {
                Ok(result) => result,
                Err(error) => return write_sim_error(stream, error),
            };
        previous = Some(checkpoint);
        let elapsed = (target_time - warmup).max(0.0);
        let duration = (target_time - previous_time).max(f64::EPSILON);
        for (name, stats) in &result.objects {
            if object_names.is_empty() || !object_names.contains(name) {
                object_names.push(name.clone());
            }
            let cumulative = stats.avg_content * elapsed;
            let before = previous_cumulative.get(name).copied().unwrap_or(0.0);
            let content = ((cumulative - before) / duration).max(0.0);
            rows.push(serde_json::json!({
                "time": target_time,
                "object": name,
                "content": content,
                "utilization": stats.utilization,
            }));
            previous_cumulative.insert(name.clone(), cumulative);
        }
        previous_time = target_time;
        if target_time >= stop {
            break;
        }
        target += request.interval;
    }
    let payload = serde_json::json!({
        "interval": request.interval,
        "stop_time": stop,
        "warmup": warmup,
        "objects": object_names,
        "series": rows,
    });
    write_json(stream, "200 OK", &payload.to_string())
}

#[derive(Debug, serde::Deserialize)]
struct TableRequest {
    table: String,
    action: String,
    #[serde(default)]
    row: Option<usize>,
    #[serde(default)]
    column: Option<usize>,
    #[serde(default)]
    value: Option<serde_json::Value>,
    #[serde(default)]
    values: Option<Vec<serde_json::Value>>,
}

/// Edit a global table through the model's editing API and persist it.
fn table_endpoint(
    stream: &mut TcpStream,
    state: &mut StudioState,
    model_path: &Path,
    body: &[u8],
) -> std::io::Result<()> {
    let request: TableRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(error) => {
            return write_json(
                stream,
                "400 Bad Request",
                &serde_json::json!({ "ok": false, "error": format!("invalid table request: {error}") })
                    .to_string(),
            );
        }
    };
    let mut model = state.model.clone();
    let result = match request.action.as_str() {
        "set_cell" => {
            let (Some(row), Some(column)) = (request.row, request.column) else {
                return write_json(
                    stream,
                    "422 Unprocessable Entity",
                    &serde_json::json!({ "ok": false, "error": "set_cell needs row and column" })
                        .to_string(),
                );
            };
            let value = request.value.unwrap_or(serde_json::Value::Null);
            match serde_json::from_value::<openflex_model::TableCell>(value) {
                Ok(value) => model
                    .set_table_cell(&request.table, row, column, value)
                    .map_err(|error| error.to_string()),
                Err(error) => Err(format!("invalid cell value: {error}")),
            }
        }
        "insert_row" => {
            let Some(table) = model.table_mut(&request.table) else {
                return write_json(
                    stream,
                    "404 Not Found",
                    &serde_json::json!({ "ok": false, "error": format!("unknown global table '{}'", request.table) })
                        .to_string(),
                );
            };
            let width = table.width();
            let cells = match request.values {
                Some(values) => values
                    .into_iter()
                    .map(|value| {
                        serde_json::from_value::<openflex_model::TableCell>(value)
                            .map_err(|error| format!("invalid cell value: {error}"))
                    })
                    .collect::<Result<Vec<_>, _>>(),
                None => Ok((0..width)
                    .map(|_| openflex_model::TableCell::Number(0.0))
                    .collect()),
            };
            match cells {
                Ok(cells) => table.insert_row(cells),
                Err(error) => Err(error),
            }
        }
        "remove_row" => {
            let Some(row) = request.row else {
                return write_json(
                    stream,
                    "422 Unprocessable Entity",
                    &serde_json::json!({ "ok": false, "error": "remove_row needs row" })
                        .to_string(),
                );
            };
            let Some(table) = model.table_mut(&request.table) else {
                return write_json(
                    stream,
                    "404 Not Found",
                    &serde_json::json!({ "ok": false, "error": format!("unknown global table '{}'", request.table) })
                        .to_string(),
                );
            };
            if table.remove_row(row).is_some() {
                Ok(())
            } else {
                Err(format!("row {row} is out of range"))
            }
        }
        other => Err(format!("unknown table action '{other}'")),
    };
    if let Err(error) = result {
        return write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": error }).to_string(),
        );
    }
    if let Err(error) = model.validate() {
        return write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": format!("invalid model after table edit: {error}") })
                .to_string(),
        );
    }
    persist_model(
        stream,
        state,
        model_path,
        model,
        serde_json::json!({ "table": request.table }),
    )
}

#[derive(Debug, serde::Serialize)]
struct CheckpointEntry {
    index: u32,
    file: String,
    now: f64,
    warmup: f64,
    model_name: String,
    size: u64,
    sha256: String,
    sha256_short: String,
}

fn list_checkpoints_endpoint(stream: &mut TcpStream, state: &StudioState) -> std::io::Result<()> {
    let mut entries = Vec::new();
    if let Ok(directory) = fs::read_dir(&state.checkpoints) {
        for entry in directory.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(index) = name
                .strip_prefix("checkpoint.rep")
                .and_then(|rest| rest.strip_suffix(".json"))
                .and_then(|index| index.parse::<u32>().ok())
            else {
                continue;
            };
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(checkpoint) = openflex_core::Checkpoint::from_json(&text) else {
                continue;
            };
            let sha256 = openflex_core::Checkpoint::sha256_of(&text);
            entries.push(CheckpointEntry {
                index,
                file: name.to_string(),
                now: checkpoint.now(),
                warmup: checkpoint.warmup(),
                model_name: checkpoint.model_name().to_string(),
                size: entry.metadata().map(|metadata| metadata.len()).unwrap_or(0),
                sha256_short: sha256.chars().take(12).collect(),
                sha256,
            });
        }
    }
    entries.sort_by_key(|entry| entry.index);
    write_json(
        stream,
        "200 OK",
        &serde_json::json!({
            "directory": state.checkpoints.display().to_string(),
            "checkpoints": entries,
        })
        .to_string(),
    )
}

#[derive(Debug, serde::Deserialize)]
struct VerifyCheckpointRequest {
    file: String,
    #[serde(default)]
    sha256: Option<String>,
}

fn verify_checkpoint_endpoint(
    stream: &mut TcpStream,
    state: &StudioState,
    body: &[u8],
) -> std::io::Result<()> {
    let request: VerifyCheckpointRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(error) => {
            return write_json(
                stream,
                "400 Bad Request",
                &serde_json::json!({ "ok": false, "error": format!("invalid verify request: {error}") })
                    .to_string(),
            );
        }
    };
    let Some(name) = Path::new(&request.file)
        .file_name()
        .and_then(|name| name.to_str())
    else {
        return write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": "invalid checkpoint file name" })
                .to_string(),
        );
    };
    let path = state.checkpoints.join(name);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            return write_json(
                stream,
                "404 Not Found",
                &serde_json::json!({ "ok": false, "error": format!("could not read {}: {error}", path.display()) })
                    .to_string(),
            );
        }
    };
    let actual = openflex_core::Checkpoint::sha256_of(&text);
    let expected = request.sha256.as_deref();
    match openflex_core::Checkpoint::from_json_verified(&text, expected) {
        Ok(checkpoint) => write_json(
            stream,
            "200 OK",
            &serde_json::json!({
                "ok": true,
                "verified": true,
                "file": name,
                "sha256": actual,
                "sha256_short": actual.chars().take(12).collect::<String>(),
                "now": checkpoint.now(),
                "model_name": checkpoint.model_name(),
            })
            .to_string(),
        ),
        Err(error) => write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({
                "ok": false,
                "verified": false,
                "file": name,
                "sha256": actual,
                "sha256_short": actual.chars().take(12).collect::<String>(),
                "error": error.to_string(),
            })
            .to_string(),
        ),
    }
}

#[derive(Debug, serde::Deserialize)]
struct ResumeCheckpointRequest {
    file: String,
}

fn resume_checkpoint_endpoint(
    stream: &mut TcpStream,
    state: &mut StudioState,
    body: &[u8],
) -> std::io::Result<()> {
    let request: ResumeCheckpointRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(error) => {
            return write_json(
                stream,
                "400 Bad Request",
                &serde_json::json!({ "ok": false, "error": format!("invalid checkpoint request: {error}") })
                    .to_string(),
            );
        }
    };
    let Some(name) = Path::new(&request.file)
        .file_name()
        .and_then(|name| name.to_str())
    else {
        return write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": "invalid checkpoint file name" })
                .to_string(),
        );
    };
    let path = state.checkpoints.join(name);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            return write_json(
                stream,
                "404 Not Found",
                &serde_json::json!({ "ok": false, "error": format!("could not read {}: {error}", path.display()) })
                    .to_string(),
            );
        }
    };
    let checkpoint = match openflex_core::Checkpoint::from_json(&text) {
        Ok(checkpoint) => checkpoint,
        Err(error) => {
            return write_json(
                stream,
                "422 Unprocessable Entity",
                &serde_json::json!({ "ok": false, "error": format!("invalid checkpoint: {error}") }).to_string(),
            );
        }
    };
    if checkpoint.model_name() != state.model.name {
        return write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": format!(
                "checkpoint belongs to model '{}', not '{}'",
                checkpoint.model_name(),
                state.model.name
            ) })
            .to_string(),
        );
    }
    if (checkpoint.warmup() - state.model.run.warmup).abs() > f64::EPSILON {
        return write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": format!(
                "checkpoint warm-up {} does not match the model warm-up {}",
                checkpoint.warmup(),
                state.model.run.warmup
            ) })
            .to_string(),
        );
    }
    let runner = ensure_runner(state);
    match runner.restore(checkpoint) {
        Ok(reply) => write_json(
            stream,
            "200 OK",
            &serde_json::json!({
                "ok": true,
                "report": reply.report,
                "state": reply.state,
            })
            .to_string(),
        ),
        Err(error) => write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": error }).to_string(),
        ),
    }
}

fn next_event_endpoint(stream: &mut TcpStream, state: &mut StudioState) -> std::io::Result<()> {
    let runner = ensure_runner(state);
    match runner.next_event() {
        Ok(reply) => write_json(
            stream,
            "200 OK",
            &serde_json::json!({
                "now": reply.state.now,
                "event": reply.event,
                "finished": reply.event.is_none(),
            })
            .to_string(),
        ),
        Err(error) => write_json(
            stream,
            "500 Internal Server Error",
            &serde_json::json!({ "ok": false, "error": error }).to_string(),
        ),
    }
}

/// Advance exactly one engine event through the native simulation handle.
fn step_event_endpoint(stream: &mut TcpStream, state: &mut StudioState) -> std::io::Result<()> {
    let runner = ensure_runner(state);
    match runner.step_event() {
        Ok(reply) => {
            let finished = reply.event.is_none();
            write_json(
                stream,
                "200 OK",
                &serde_json::json!({
                    "report": reply.report,
                    "state": reply.state,
                    "event": reply.event,
                    "finished": finished,
                })
                .to_string(),
            )
        }
        Err(error) => write_json(
            stream,
            "422 Unprocessable Entity",
            &serde_json::json!({ "ok": false, "error": error }).to_string(),
        ),
    }
}

/// Read one HTTP request with strict header/body limits.
fn read_request(stream: &TcpStream) -> Result<Request, HttpError> {
    let mut reader = BufReader::new(stream.try_clone().map_err(|error| HttpError {
        status: "500 Internal Server Error",
        message: error.to_string(),
    })?);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|error| HttpError::bad_request(format!("could not read request line: {error}")))?;
    if request_line.is_empty() {
        return Err(HttpError::bad_request("empty request"));
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default();
    if method.is_empty() || target.is_empty() {
        return Err(HttpError::bad_request("malformed request line"));
    }
    let path = target.split('?').next().unwrap_or_default().to_string();

    let mut header_bytes = request_line.len();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .map_err(|error| HttpError::bad_request(format!("could not read headers: {error}")))?;
        if read == 0 {
            break;
        }
        header_bytes += read;
        if header_bytes > MAX_HEADER {
            return Err(HttpError {
                status: "431 Request Header Fields Too Large",
                message: "request headers exceed the 64 KiB limit".into(),
            });
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim();
            if name == "content-length" {
                content_length = value.parse::<usize>().map_err(|error| {
                    HttpError::bad_request(format!("invalid Content-Length: {error}"))
                })?;
            }
            if name == "transfer-encoding" {
                return Err(HttpError::bad_request(
                    "Transfer-Encoding is not supported; send Content-Length",
                ));
            }
        }
    }
    if content_length > MAX_BODY {
        // Reject from the headers alone: do not read an attacker-controlled
        // amount of data just to answer it.
        return Err(HttpError {
            status: "413 Payload Too Large",
            message: format!("request body exceeds the {MAX_BODY} byte limit"),
        });
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader
            .read_exact(&mut body)
            .map_err(|error| HttpError::bad_request(format!("could not read body: {error}")))?;
    }
    Ok(Request { method, path, body })
}

fn write_json(stream: &mut TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    write_response(stream, status, "application/json", body)
}

fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    let directory = parent.unwrap_or_else(|| Path::new("."));
    let temporary = directory.join(format!(".openflex-studio-{}.tmp", std::process::id()));
    if let Err(error) = fs::write(&temporary, text) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("could not write {}: {error}", temporary.display()));
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("could not replace {}: {error}", path.display()));
    }
    Ok(())
}

fn index_html(model: &Model) -> String {
    include_str!("studio.html")
        .replace("__MODEL_NAME__", &escape_html(&model.name))
        .replace("__MODEL_FILE__", &escape_html(&model.name))
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn open_browser(url: &str) {
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    if let Err(error) = result {
        eprintln!("could not open a browser ({error}); open {url} manually");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_escapes_the_model_name() {
        let model = Model::from_json(
            r#"{"format":"openflex-model","version":1,"name":"a<b","objects":[{"name":"S","kind":"source"},{"name":"K","kind":"sink"}],"connections":[{"from":"S","to":"K"}]}"#,
        )
        .expect("fixture");
        let html = index_html(&model);
        assert!(html.contains("a&lt;b"));
        assert!(html.contains("OpenFlex Studio"));
    }

    #[test]
    fn request_size_limits_are_documented_values() {
        assert_eq!(MAX_BODY, 2 * 1024 * 1024);
        assert_eq!(MAX_HEADER, 64 * 1024);
    }
}
