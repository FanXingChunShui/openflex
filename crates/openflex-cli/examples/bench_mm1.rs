//! Minimal throughput benchmark for the OpenFlex engine.
//!
//! Usage:
//!
//! ```sh
//! # built-in M/M/1, 100 000 time units, one replication (the default)
//! cargo run --release -p openflex-cli --example bench_mm1 -- --stop 100000
//!
//! # any model file (its run.stop_time is used unless --stop overrides it)
//! cargo run --release -p openflex-cli --example bench_mm1 -- \
//!     --model examples/mm1.ofm --replications 3
//! ```
//!
//! Without `--model` the benchmark builds a standard M/M/1 queue (exponential
//! arrivals with mean 5, exponential service with mean 4). With `--model` it
//! loads any `.ofm` and uses that model's run settings (overridable with
//! `--stop`). The benchmark times a normal simulation run, then replays the
//! same configuration with tracing enabled once to obtain the exact event
//! count. It reports **events/s and items/s** for the untraced run (the run
//! users actually do) plus the traced numbers for reference.
//!
//! No criterion dependency and no CI threshold: this is a quick, honest
//! throughput probe.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use openflex_core::{SimConfig, derive_seed, simulate, simulate_traced};
use openflex_model::Model;

fn main() -> ExitCode {
    let mut stop: Option<f64> = None;
    let mut replications = 1u32;
    let mut model_path: Option<PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--stop" => match args.next().and_then(|value| value.parse().ok()) {
                Some(value) if value > 0.0 => stop = Some(value),
                _ => {
                    eprintln!("error: --stop needs a positive number");
                    return ExitCode::from(2);
                }
            },
            "--replications" => match args.next().and_then(|value| value.parse().ok()) {
                Some(value) if value > 0 => replications = value,
                _ => {
                    eprintln!("error: --replications needs a positive integer");
                    return ExitCode::from(2);
                }
            },
            "--model" => match args.next() {
                Some(path) => model_path = Some(PathBuf::from(path)),
                None => {
                    eprintln!("error: --model needs a path");
                    return ExitCode::from(2);
                }
            },
            "--help" | "-h" => {
                println!("usage: bench_mm1 [--model <path>] [--stop <time>] [--replications <n>]");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("error: unknown argument {other:?} (try --help)");
                return ExitCode::from(2);
            }
        }
    }

    let mut model = match &model_path {
        Some(path) => match load_model(path) {
            Ok(model) => model,
            Err(error) => {
                eprintln!("error: {error}");
                return ExitCode::from(1);
            }
        },
        None => match mm1(100_000.0) {
            Ok(model) => model,
            Err(error) => {
                eprintln!("error: could not build the M/M/1 model: {error}");
                return ExitCode::from(1);
            }
        },
    };
    if let Some(stop) = stop {
        model.run.stop_time = stop;
    }
    if model.run.stop_time <= model.run.warmup {
        eprintln!(
            "error: stop time {} must be greater than the warm-up {}",
            model.run.stop_time, model.run.warmup
        );
        return ExitCode::from(2);
    }

    let mut events = 0usize;
    let mut traced = Duration::ZERO;
    let mut untraced = Duration::ZERO;
    let mut completed = 0u64;

    for index in 0..replications {
        let seed = derive_seed(model.run.seed, index);
        let config = SimConfig::from_run_settings(&model.run, seed);

        let started = Instant::now();
        let result = match simulate(&model, &config, index) {
            Ok(result) => result,
            Err(error) => {
                eprintln!("error: simulation failed: {error}");
                return ExitCode::from(1);
            }
        };
        untraced += started.elapsed();
        completed += result.items_completed;

        // The trace is only used to count events; its own wall time is
        // reported separately because tracing has a measurable cost.
        let started = Instant::now();
        let (_, log) = match simulate_traced(&model, &config, index) {
            Ok(result) => result,
            Err(error) => {
                eprintln!("error: traced simulation failed: {error}");
                return ExitCode::from(1);
            }
        };
        traced += started.elapsed();
        events += log.len();
        if log.truncated {
            eprintln!(
                "warning: trace hit the {}-event limit; event throughput is a lower bound",
                openflex_core::MAX_TRACE_EVENTS
            );
        }
    }

    let untraced_seconds = untraced.as_secs_f64();
    let traced_seconds = traced.as_secs_f64();
    println!("OpenFlex benchmark");
    match &model_path {
        Some(path) => println!("  model: {} ({})", model.name, path.display()),
        None => println!("  model: {} (built-in M/M/1)", model.name),
    }
    println!(
        "  stop time {}, warm-up {}, replications {replications}",
        model.run.stop_time, model.run.warmup
    );
    println!("  items completed: {completed}");
    println!("  events processed: {events}");
    println!(
        "  untraced: {:.3} s -> {:.0} events/s, {:.0} items/s",
        untraced_seconds,
        rate(events as f64, untraced_seconds),
        rate(completed as f64, untraced_seconds)
    );
    println!(
        "  traced:   {:.3} s -> {:.0} events/s, {:.0} items/s (tracing overhead included)",
        traced_seconds,
        rate(events as f64, traced_seconds),
        rate(completed as f64, traced_seconds)
    );
    ExitCode::SUCCESS
}

fn rate(work: f64, seconds: f64) -> f64 {
    if seconds <= 0.0 {
        return 0.0;
    }
    work / seconds
}

fn load_model(path: &PathBuf) -> Result<Model, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("could not read model {}: {error}", path.display()))?;
    Model::from_json(&text).map_err(|error| format!("invalid model {}: {error}", path.display()))
}

/// Standard M/M/1: exponential arrivals (mean 5), exponential service (mean 4).
fn mm1(stop_time: f64) -> Result<Model, String> {
    Model::from_json(&format!(
        r#"{{
          "format": "openflex-model", "version": 1, "name": "bench-mm1",
          "time_unit": "seconds",
          "run": {{ "stop_time": {stop_time}, "warmup": 0.0, "seed": 1, "replications": 1 }},
          "objects": [
            {{ "name": "Arrivals", "kind": "source", "pos": [0, 0], "params": {{
                "interarrival": {{ "distribution": "exponential", "mean": 5.0 }} }} }},
            {{ "name": "Queue", "kind": "queue", "pos": [5, 0], "params": {{ "max_content": 100000 }} }},
            {{ "name": "Server", "kind": "processor", "pos": [10, 0], "params": {{
                "process_time": {{ "distribution": "exponential", "mean": 4.0 }} }} }},
            {{ "name": "Exit", "kind": "sink", "pos": [15, 0], "params": {{}} }}
          ],
          "connections": [
            {{ "from": "Arrivals", "to": "Queue" }},
            {{ "from": "Queue", "to": "Server" }},
            {{ "from": "Server", "to": "Exit" }}
          ]
        }}"#
    ))
    .map_err(|error| error.to_string())
}
