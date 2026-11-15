//! OpenFlex command line interface.
//!
//! `openflex` is the headless front end to the engine: it validates a model,
//! runs it with any number of replications (optionally in parallel), runs the
//! experiments declared in the model, re-renders saved reports, compares two
//! scenarios, visualises the object layout, imports CSV arrival tables, exports
//! Excel workbooks and prints or exports the statistics.
//!
//! Exit codes are stable so the CLI can be scripted:
//!
//! - `0` success
//! - `1` runtime error (unreadable file, simulation failure)
//! - `2` usage error or invalid model (`validate` found a problem)
//!
//! Errors are plain text by default. With `--format json` (or the explicit
//! `--error-format json`) they become a JSON object on stderr:
//!
//! ```json
//! {"error":{"code":"model_invalid","message":"invalid model demo.ofm: …","path":"demo.ofm"}}
//! ```
//!
//! The report itself still goes to stdout in the requested format, so
//! `openflex run model.ofm --format json 2>/dev/null | jq` keeps working.

mod checkpoint;
mod html;
mod import;
mod layout;
mod objects;
mod optimize;
mod progress;
mod schema;
mod serve;
mod studio;
mod templates;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow, bail};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use openflex_core::{
    SimConfig, SimReport, derive_seed, simulate, simulate_traced, simulate_with_checkpoint,
};
use openflex_model::{Model, ObjectKind, Scenario, TimeUnit};
use openflex_report::{self as report, i18n};
use report::i18n::Language;

use progress::Progress;

/// Exit code for runtime failures such as unreadable or malformed inputs.
const EXIT_RUNTIME: i32 = 1;
/// Exit code for usage errors and models that fail validation.
const EXIT_USAGE: i32 = 2;

#[derive(Debug, Parser)]
#[command(
    name = "openflex",
    version,
    about = "OpenFlex — a clean-room, cross-platform discrete-event simulation engine",
    long_about = "OpenFlex is a clean-room implementation of the core modelling semantics of \
                  FlexSim: a 3D object library (source, queue, processor, sink, conveyor, \
                  combiner, separator, transporter, …), statistical distributions, object \
                  triggers, routing rules and an experimenter. Models are JSON files, runs are \
                  deterministic for a given seed, and every report can be exported as JSON, CSV, \
                  Markdown or Excel.",
    propagate_version = true
)]
struct Cli {
    /// Suppress informational messages on stderr (the report output is unchanged).
    #[arg(long, global = true)]
    quiet: bool,
    /// Disable ANSI colours; accepted for compatibility, output is plain text today.
    #[arg(long = "no-color", global = true)]
    no_color: bool,
    /// Print replication progress on stderr (throttled; disabled by --quiet).
    #[arg(long, global = true)]
    progress: bool,
    /// How errors are rendered on stderr: plain text or a JSON object.
    #[arg(long, value_enum, default_value_t = ErrorFormat::Text, global = true)]
    error_format: ErrorFormat,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check a model for errors without running it.
    Validate {
        /// Path to the model file (.ofm or .json).
        model: PathBuf,
    },
    /// Run a model and print its statistics.
    Run {
        /// Path to the model file.
        model: PathBuf,
        /// Override the stop time (model time units); `--until` is an alias.
        #[arg(long, visible_alias = "until")]
        stop_time: Option<f64>,
        /// Override the warm-up period.
        #[arg(long)]
        warmup: Option<f64>,
        /// Override the base random seed; repeat for one replication per seed.
        #[arg(long, action = clap::ArgAction::Append)]
        seed: Vec<u64>,
        /// Override the number of independent replications.
        #[arg(long)]
        replications: Option<u32>,
        /// Replace one parameter, e.g. `--set Proc1.process_time.mean=4.5`.
        #[arg(long = "set", value_name = "OBJECT.PARAM=VALUE")]
        overrides: Vec<String>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Table)]
        format: Format,
        /// Also write report.json, report.csv, report.md (and replications.csv) here.
        #[arg(long)]
        report_dir: Option<PathBuf>,
        /// Run replications on N threads; the result is identical to a serial run.
        #[arg(long, value_name = "N")]
        parallel: Option<usize>,
        /// Also write the report as an Excel workbook (.xlsx).
        #[arg(long, value_name = "FILE")]
        xlsx: Option<PathBuf>,
        /// Write a JSONL event trace; with several replications the files are `FILE.rep0.jsonl`, `FILE.rep1.jsonl`, … (cannot be combined with --parallel > 1 or --resume).
        #[arg(long, value_name = "FILE")]
        trace: Option<PathBuf>,
        /// Write an interval-average object-content time series CSV.
        #[arg(long, value_name = "FILE")]
        timeseries: Option<PathBuf>,
        /// Sampling interval for --timeseries (model time units).
        #[arg(long, default_value_t = 1.0)]
        interval: f64,
        /// Compute batch-means confidence intervals with N batches (default 10).
        #[arg(long, value_name = "BATCHES", num_args = 0..=1, default_missing_value = "10")]
        batch_means: Option<u32>,
        /// Include per-object input/output/avg_wip/utilization batch means (implies --batch-means 10).
        #[arg(long)]
        batch_means_objects: bool,
        /// Save the final simulation snapshot as JSON (single replication; cannot be combined with --parallel > 1; may be combined with --trace).
        #[arg(long, value_name = "FILE", conflicts_with = "save_checkpoints")]
        save_checkpoint: Option<PathBuf>,
        /// Save one checkpoint per replication into DIR as checkpoint.rep0.json, checkpoint.rep1.json, …
        #[arg(long, value_name = "DIR", conflicts_with_all = ["save_checkpoint", "resume"])]
        save_checkpoints: Option<PathBuf>,
        /// Resume from a checkpoint written by --save-checkpoint (single replication; cannot be combined with --parallel > 1, several --seed values, --trace or --set).
        #[arg(long, value_name = "FILE")]
        resume: Option<PathBuf>,
        /// Show process-flow statistics in table/CSV/Markdown (JSON always carries them).
        #[arg(long)]
        flow: bool,
        /// Show fluid statistics in table/CSV/Markdown (JSON always carries them).
        #[arg(long)]
        fluid: bool,
        /// Validate the model and print the run plan without simulating.
        #[arg(long)]
        dry_run: bool,
    },
    /// Run the experiments declared in the model.
    Experiment {
        /// Path to the model file.
        model: PathBuf,
        /// Only run the experiment with this name.
        #[arg(long)]
        name: Option<String>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Table)]
        format: Format,
        /// Write one report directory per scenario here.
        #[arg(long)]
        report_dir: Option<PathBuf>,
        /// Write a JSONL trace per scenario replication under --report-dir.
        #[arg(long, requires = "report_dir")]
        trace: bool,
        /// Show process-flow statistics in the experiment report.
        #[arg(long)]
        flow: bool,
        /// Show fluid statistics in the experiment report.
        #[arg(long)]
        fluid: bool,
    },
    /// Re-render a saved report, or summarise an experiment directory.
    Report {
        /// Report JSON file written by `openflex run --report-dir` or `--format json`.
        #[arg(value_name = "REPORT_JSON", conflicts_with = "experiment")]
        report: Option<PathBuf>,
        /// Experiment directory written by `openflex experiment --report-dir`.
        #[arg(long, value_name = "DIR", conflicts_with = "report")]
        experiment: Option<PathBuf>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Table)]
        format: Format,
        /// Write the rendered report here instead of standard output.
        #[arg(long, short = 'o', value_name = "FILE")]
        output: Option<PathBuf>,
        /// Add the process-flow section when the saved report contains one.
        #[arg(long)]
        flow: bool,
        /// Add the fluid section when the saved report contains one.
        #[arg(long)]
        fluid: bool,
        /// Add batch-means confidence intervals with N batches (default 10).
        #[arg(long, value_name = "BATCHES", num_args = 0..=1, default_missing_value = "10")]
        batch_means: Option<u32>,
        /// Include per-object input/output/avg_wip/utilization batch means (implies --batch-means 10).
        #[arg(long)]
        batch_means_objects: bool,
    },
    /// Compare two saved reports object by object (A/B scenario analysis).
    Compare {
        /// Baseline report JSON (the "before" run).
        baseline: PathBuf,
        /// Candidate report JSON (the "after" run).
        candidate: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Table)]
        format: Format,
        /// Write the comparison here instead of standard output.
        #[arg(long, short = 'o', value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// Create a new model from a template.
    New {
        /// Directory to create (and fill) with the new model.
        directory: PathBuf,
        /// Template: `basic`, `mm1`, or `line`.
        #[arg(long, default_value = "basic")]
        template: String,
        /// Write into a non-empty directory.
        #[arg(long)]
        force: bool,
    },
    /// List or describe the supported object kinds.
    Objects {
        #[command(subcommand)]
        command: ObjectCommand,
    },
    /// List the statistical distributions and their parameters.
    Distributions,
    /// Generate shell completion scripts (bash, zsh, fish or powershell).
    Completions {
        /// Shell to generate completions for.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Export the model's object layout as an SVG drawing (2D or isometric 3D).
    Layout {
        /// Path to the model file.
        model: PathBuf,
        /// Write the SVG here instead of standard output.
        #[arg(long, value_name = "FILE", conflicts_with = "html")]
        svg: Option<PathBuf>,
        /// Write a self-contained interactive HTML preview instead.
        #[arg(long, value_name = "FILE", conflicts_with = "svg")]
        html: Option<PathBuf>,
        /// SVG width in pixels.
        #[arg(long, default_value_t = 1200)]
        width: u32,
        /// SVG height in pixels.
        #[arg(long, default_value_t = 800)]
        height: u32,
        /// Projection: flat 2d (default) or isometric 3d.
        #[arg(long, value_enum, default_value_t = Dimension::Flat2d)]
        dimension: Dimension,
        /// Generate N animation frames (requires --svg or --html); SVG writes out-0.svg, …, HTML embeds them.
        #[arg(long, value_name = "N")]
        animate: Option<u32>,
    },
    /// Migrate a model file to the current format version.
    Migrate {
        /// Path to the model file.
        model: PathBuf,
        /// Write the migrated model here instead of standard output.
        #[arg(long, value_name = "FILE", conflicts_with = "in_place")]
        out: Option<PathBuf>,
        /// Overwrite the input file (validated before writing).
        #[arg(long, conflicts_with = "out")]
        in_place: bool,
    },
    /// Search parameter combinations for the best system objective (D4).
    Optimize {
        /// Path to the model file.
        model: PathBuf,
        /// Parameter range, e.g. `Proc1.process_time.mean=2..6` or `=1,3,5`; repeat for a grid.
        #[arg(long = "param", value_name = "OBJECT.PARAM=VALUE", action = clap::ArgAction::Append)]
        params: Vec<String>,
        /// Objective(s) to optimise; comma-separate several for an equally weighted score (e.g. throughput,wip).
        #[arg(
            long = "objective",
            value_enum,
            value_delimiter = ',',
            default_value = "throughput",
            value_name = "OBJECTIVES"
        )]
        objectives: Vec<optimize::Objective>,
        /// Search strategy: grid (default), random or genetic.
        #[arg(long, value_enum, default_value = "grid")]
        algorithm: optimize::Algorithm,
        /// Genetic population size.
        #[arg(long, default_value_t = 40)]
        population: usize,
        /// Genetic generations (each generation evaluates the population).
        #[arg(long, default_value_t = 12)]
        generations: usize,
        /// Genetic mutation probability per gene.
        #[arg(long, default_value_t = 0.15)]
        mutation_rate: f64,
        /// Objective weight, e.g. --weight throughput=0.7,wip=0.3 (default: equal weights).
        #[arg(
            long = "weight",
            value_name = "OBJECTIVE=WEIGHT",
            value_delimiter = ','
        )]
        weights: Vec<String>,
        /// Output the Pareto front for multi-objective searches.
        #[arg(long)]
        pareto: bool,
        /// Number of values for each `START..END` range.
        #[arg(long, default_value_t = 5)]
        steps: u32,
        /// Override the number of replications per candidate.
        #[arg(long)]
        replications: Option<u32>,
        /// Override the base random seed.
        #[arg(long)]
        seed: Option<u64>,
        /// Evaluate candidates on N threads (results are still sorted deterministically).
        #[arg(long, value_name = "N")]
        parallel: Option<usize>,
        /// Randomly sample this many combinations instead of the full grid.
        #[arg(long, value_name = "N")]
        samples: Option<usize>,
        /// Evaluate a single object (default: the whole model).
        #[arg(long, value_name = "NAME")]
        object: Option<String>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Table)]
        format: Format,
        /// Write the search result here instead of standard output.
        #[arg(long, short = 'o', value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// Open a local web studio: 3D view, model tree with rename/duplicate/delete, global tables, parameter/process-flow editors, native event/time stepping with pause/resume, experiments and dashboard/time-series export.
    Studio {
        /// Path to the model file to edit and run.
        model: PathBuf,
        /// TCP port to bind (0 picks a free port).
        #[arg(long, default_value_t = 0)]
        port: u16,
        /// Do not open a browser window.
        #[arg(long)]
        no_open: bool,
    },
    /// Serve a minimal read-only HTTP API on 127.0.0.1 (`/health`, `/objects`).
    Serve {
        /// TCP port to bind (0 picks a free port).
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
    /// Import CSV arrival/object/connection tables as a model.
    Import {
        /// Arrival table (`time,quantity[,item_type]`).
        #[arg(long, value_name = "CSV")]
        csv: Option<PathBuf>,
        /// Object table (`name,kind,pos_x,pos_y`); must contain a sink, a source is added when missing.
        #[arg(long, value_name = "CSV")]
        objects: Option<PathBuf>,
        /// Connection table (`from,to,port,strategy`); when omitted the objects are chained in file order.
        #[arg(long, value_name = "CSV", requires = "objects")]
        connections: Option<PathBuf>,
        /// Excel workbook (.xlsx) with `objects`, `connections` and/or `arrivals` sheets.
        #[arg(long, value_name = "XLSX", conflicts_with_all = ["csv", "objects", "connections"])]
        xlsx: Option<PathBuf>,
        /// Model file to write.
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
        /// Model name (defaults to the output file stem).
        #[arg(long)]
        name: Option<String>,
        /// Time unit of the arrival timestamps.
        #[arg(long, default_value = "seconds", value_name = "UNIT")]
        time_unit: String,
    },
    /// Print the JSON Schema of the model format.
    Schema {
        /// Write the schema here instead of standard output.
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum ObjectCommand {
    /// List every object kind with a one-line description.
    List,
    /// Show the parameters of one object kind.
    Describe { kind: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    Table,
    Json,
    Csv,
    Markdown,
}

/// Projection used by `openflex layout`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Dimension {
    /// Flat top-down rectangles (the original layout).
    #[value(name = "2d")]
    Flat2d,
    /// Isometric boxes with height and shaded faces.
    #[value(name = "3d")]
    Iso3d,
}

impl From<Format> for report::Format {
    fn from(format: Format) -> Self {
        match format {
            Format::Table => report::Format::Table,
            Format::Json => report::Format::Json,
            Format::Csv => report::Format::Csv,
            Format::Markdown => report::Format::Markdown,
        }
    }
}

/// How errors are rendered on stderr.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ErrorFormat {
    /// `error: …` (localised prefix).
    Text,
    /// `{"error":{"code":…,"message":…,"path":…}}`.
    Json,
}

/// The machine-readable class of a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorKind {
    /// The model failed validation or could not be parsed (exit 2).
    ModelInvalid,
    /// The CLI was called incorrectly (exit 2).
    Usage,
    /// The environment or a run failed (exit 1).
    Runtime,
}

impl ErrorKind {
    fn code(self) -> &'static str {
        match self {
            ErrorKind::ModelInvalid => "model_invalid",
            ErrorKind::Usage => "usage_error",
            ErrorKind::Runtime => "runtime_error",
        }
    }
}

/// A CLI failure with the process exit code it should produce.
struct Failure {
    code: i32,
    kind: ErrorKind,
    error: anyhow::Error,
    /// Input file the failure belongs to, surfaced as `path` in JSON errors.
    path: Option<PathBuf>,
}

impl Failure {
    /// An error caused by the environment or by a run in progress (exit 1).
    fn runtime(error: impl Into<anyhow::Error>) -> Self {
        Self {
            code: EXIT_RUNTIME,
            kind: ErrorKind::Runtime,
            error: error.into(),
            path: None,
        }
    }

    /// An error caused by how the CLI was called or by an invalid model (exit 2).
    fn usage(error: impl Into<anyhow::Error>) -> Self {
        Self {
            code: EXIT_USAGE,
            kind: ErrorKind::Usage,
            error: error.into(),
            path: None,
        }
    }

    /// A model that could not be parsed or validated (exit 2).
    fn model(path: &Path, error: impl Into<anyhow::Error>) -> Self {
        Self {
            code: EXIT_USAGE,
            kind: ErrorKind::ModelInvalid,
            error: error.into(),
            path: Some(path.to_path_buf()),
        }
    }

    /// Attach the input path for structured error output.
    fn with_path(mut self, path: &Path) -> Self {
        self.path = Some(path.to_path_buf());
        self
    }
}

impl From<anyhow::Error> for Failure {
    fn from(error: anyhow::Error) -> Self {
        Self::runtime(error)
    }
}

impl From<String> for Failure {
    fn from(error: String) -> Self {
        Self::usage(anyhow!(error))
    }
}

impl From<report::ReportError> for Failure {
    fn from(error: report::ReportError) -> Self {
        Self::runtime(anyhow::Error::new(error))
    }
}

impl From<openflex_core::SimError> for Failure {
    fn from(error: openflex_core::SimError) -> Self {
        Self::runtime(anyhow::Error::new(error))
    }
}

impl From<openflex_model::ModelError> for Failure {
    fn from(error: openflex_model::ModelError) -> Self {
        Self::usage(anyhow::Error::new(error))
    }
}

fn main() {
    let args: Vec<OsString> = std::env::args_os().collect();
    let json_errors = wants_json_errors(&args);
    let cli = parse_cli(&args, json_errors);
    let quiet = cli.quiet;
    let progress = cli.progress && !quiet;
    if let Err(failure) = run(cli.command, quiet, progress) {
        emit_failure(&failure, json_errors);
        std::process::exit(failure.code);
    }
}

/// Build the clap command with translated `about` / `long_about` / `after_help`.
fn build_command() -> clap::Command {
    Cli::command()
        .about(i18n::t("about"))
        .long_about(i18n::t("long_about"))
        .after_help(i18n::t("after_help"))
}

/// Parse the CLI, rendering help and usage errors in the selected style.
fn parse_cli(args: &[OsString], json_errors: bool) -> Cli {
    let mut command = build_command();
    let matches = match command.try_get_matches_from_mut(args.iter().cloned()) {
        Ok(matches) => matches,
        Err(error) => exit_with_clap_error(error, json_errors),
    };
    match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => exit_with_clap_error(error, json_errors),
    }
}

fn exit_with_clap_error(error: clap::Error, json_errors: bool) -> ! {
    let code = error.exit_code();
    if error.use_stderr() {
        if json_errors {
            let message = error.to_string();
            let payload = serde_json::json!({
                "error": {
                    "code": "usage_error",
                    "message": message.trim_end(),
                }
            });
            eprintln!("{payload}");
        } else {
            let _ = error.print();
        }
    } else {
        // `--help` and `--version` are successful output.
        let _ = error.print();
    }
    std::process::exit(code);
}

/// Whether the raw arguments ask for JSON diagnostics.
///
/// `--error-format json` is the explicit switch; `--format json` implies it
/// because scripts that parse the report almost always parse errors too.
fn wants_json_errors(args: &[OsString]) -> bool {
    let value_of = |flag: &str| -> bool {
        let mut iter = args
            .iter()
            .skip(1)
            .map(|arg| arg.to_string_lossy().into_owned());
        while let Some(argument) = iter.next() {
            if argument == flag {
                return iter.next().is_some_and(|value| value == "json");
            }
            if let Some(value) = argument
                .strip_prefix(flag)
                .and_then(|rest| rest.strip_prefix('='))
            {
                return value == "json";
            }
        }
        false
    };
    value_of("--format") || value_of("--error-format")
}

fn emit_failure(failure: &Failure, json_errors: bool) {
    if json_errors {
        let mut body = serde_json::Map::new();
        body.insert("code".into(), serde_json::Value::from(failure.kind.code()));
        body.insert(
            "message".into(),
            serde_json::Value::from(format!("{:#}", failure.error)),
        );
        if let Some(path) = &failure.path {
            body.insert(
                "path".into(),
                serde_json::Value::from(path.display().to_string()),
            );
        }
        let payload = serde_json::json!({ "error": serde_json::Value::Object(body) });
        // Serialising a map of strings cannot fail, but never panic in a
        // diagnostics path if it somehow does.
        match serde_json::to_string(&payload) {
            Ok(text) => eprintln!("{text}"),
            Err(_) => eprintln!("{} {:#}", i18n::t("error_prefix"), failure.error),
        }
    } else {
        eprintln!("{} {:#}", i18n::t("error_prefix"), failure.error);
    }
}

fn run(command: Command, quiet: bool, progress: bool) -> Result<(), Failure> {
    match command {
        Command::Validate { model } => {
            let model = load_model(&model)?;
            if i18n::language() == Language::Zh {
                println!(
                    "{}：{}（{} 个对象，{} 条连接）",
                    i18n::t("ok"),
                    model.name,
                    model.objects.len(),
                    model.connections.len()
                );
            } else {
                println!(
                    "ok: {} ({} objects, {} connections)",
                    model.name,
                    model.objects.len(),
                    model.connections.len()
                );
            }
            Ok(())
        }
        Command::Run {
            model: path,
            stop_time,
            warmup,
            seed,
            replications,
            overrides,
            format,
            report_dir,
            parallel,
            xlsx,
            trace,
            timeseries,
            interval,
            batch_means,
            batch_means_objects,
            save_checkpoint,
            save_checkpoints,
            resume,
            flow,
            fluid,
            dry_run,
        } => {
            let mut model = load_model(&path)?;
            apply_run_overrides(&mut model, stop_time, warmup, &seed, replications).map_err(
                |error| Failure::usage(error.context(format!("model {}", path.display()))),
            )?;
            if resume.is_some() && !overrides.is_empty() {
                return Err(Failure::usage(anyhow!(
                    "--set cannot be combined with --resume; a checkpoint belongs to the model that produced it"
                )));
            }
            for entry in overrides {
                let (object, param, value) = parse_override(&entry).map_err(Failure::usage)?;
                model = model
                    .with_override(&object, &param, value)
                    .map_err(|error| {
                        Failure::usage(anyhow::Error::new(error).context(format!("--set {entry}")))
                    })?;
            }
            if let Some(workers) = parallel
                && workers == 0
            {
                return Err(Failure::usage(anyhow!("--parallel must be at least 1")));
            }
            if trace.is_some() && parallel.is_some_and(|workers| workers > 1) {
                return Err(Failure::usage(anyhow!(
                    "--trace cannot be combined with --parallel > 1; traces are written one replication at a time"
                )));
            }
            if timeseries.is_some() {
                if !interval.is_finite() || interval <= 0.0 {
                    return Err(Failure::usage(anyhow!("--interval must be positive")));
                }
                if parallel.is_some_and(|workers| workers > 1) {
                    return Err(Failure::usage(anyhow!(
                        "--timeseries cannot be combined with --parallel > 1"
                    )));
                }
                if trace.is_some() || save_checkpoint.is_some() || resume.is_some() {
                    return Err(Failure::usage(anyhow!(
                        "--timeseries cannot be combined with --trace, --save-checkpoint or --resume"
                    )));
                }
            }
            validate_checkpoint_options(
                &model,
                &seed,
                parallel,
                trace.as_deref(),
                save_checkpoint.as_deref(),
                resume.as_deref(),
            )?;
            if dry_run {
                print_run_plan(&model, &seed, parallel);
                return Ok(());
            }
            if let Some(directory) = save_checkpoints.as_deref() {
                if trace.is_some() || timeseries.is_some() {
                    return Err(Failure::usage(anyhow!(
                        "--save-checkpoints cannot be combined with --trace or --timeseries"
                    )));
                }
                return run_batch_checkpoints(
                    &model,
                    &seed,
                    directory,
                    &RunOutput {
                        format,
                        report_dir: report_dir.as_deref(),
                        xlsx: xlsx.as_deref(),
                        sections: Sections { flow, fluid },
                        batch_means,
                        batch_means_objects,
                        quiet,
                    },
                    progress,
                );
            }
            if save_checkpoint.is_some() || resume.is_some() {
                return run_checkpoint(
                    &model,
                    trace.as_deref(),
                    save_checkpoint.as_deref(),
                    resume.as_deref(),
                    &RunOutput {
                        format,
                        report_dir: report_dir.as_deref(),
                        xlsx: xlsx.as_deref(),
                        sections: Sections { flow, fluid },
                        batch_means,
                        batch_means_objects,
                        quiet,
                    },
                    progress,
                );
            }
            let report = match trace.as_deref() {
                Some(trace_path) => execute_traced(&model, &seed, trace_path, progress, quiet)?,
                None => execute(&model, &seed, parallel, progress)?,
            };
            if let Some(path) = timeseries.as_deref() {
                write_timeseries(&model, &seed, path, interval, quiet)?;
            }
            finish_run(
                &report,
                &RunOutput {
                    format,
                    report_dir: report_dir.as_deref(),
                    xlsx: xlsx.as_deref(),
                    sections: Sections { flow, fluid },
                    batch_means,
                    batch_means_objects,
                    quiet,
                },
            )
        }
        Command::Experiment {
            model: path,
            name,
            format,
            report_dir,
            trace,
            flow,
            fluid,
        } => {
            let model = load_model(&path)?;
            run_experiments(
                &model,
                name.as_deref(),
                format,
                report_dir.as_deref(),
                trace,
                Sections { flow, fluid },
                quiet,
            )
        }
        Command::Report {
            report: path,
            experiment,
            format,
            output,
            flow,
            fluid,
            batch_means,
            batch_means_objects,
        } => {
            let batch_batches = batch_means.or(if batch_means_objects { Some(10) } else { None });
            let text = match (path, experiment) {
                (Some(path), None) => {
                    let sim = report::read_report(&path)
                        .map_err(|error| Failure::from(error).with_path(&path))?;
                    report::render_sim_with_options(
                        &sim,
                        format.into(),
                        flow,
                        fluid,
                        batch_batches,
                        batch_means_objects,
                    )?
                }
                (None, Some(directory)) => {
                    if batch_batches.is_some() {
                        return Err(Failure::usage(anyhow!(
                            "--batch-means/--batch-means-objects are only supported for a single saved report, not an experiment directory"
                        )));
                    }
                    let experiment = report::read_experiment(&directory)
                        .map_err(|error| Failure::from(error).with_path(&directory))?;
                    report::render_experiment_with_sections(
                        &experiment,
                        format.into(),
                        flow,
                        fluid,
                    )?
                }
                _ => {
                    return Err(Failure::usage(anyhow!(
                        "report needs a REPORT_JSON file argument or --experiment DIR"
                    )));
                }
            };
            emit(&text, output.as_deref(), quiet)
        }
        Command::Compare {
            baseline,
            candidate,
            format,
            output,
        } => {
            let before = report::read_report(&baseline)
                .map_err(|error| Failure::from(error).with_path(&baseline))?;
            let after = report::read_report(&candidate)
                .map_err(|error| Failure::from(error).with_path(&candidate))?;
            let comparison = report::compare_reports(&baseline, &before, &candidate, &after);
            let text = report::render_comparison(&comparison, format.into())?;
            emit(&text, output.as_deref(), quiet)
        }
        Command::New {
            directory,
            template,
            force,
        } => create_model(&directory, &template, force, quiet),
        Command::Objects { command } => match command {
            ObjectCommand::List => {
                for kind in ObjectKind::all() {
                    let (summary, _) = objects::describe_kind(*kind);
                    println!("{:<12} {}", kind.as_str(), summary);
                }
                Ok(())
            }
            ObjectCommand::Describe { kind } => {
                let Some(kind) = ObjectKind::parse(&kind) else {
                    return Err(Failure::usage(anyhow!("unknown object kind '{kind}'")));
                };
                let (summary, parameters) = objects::describe_kind(kind);
                println!("{}: {summary}\n\nParameters:\n{parameters}", kind.as_str());
                Ok(())
            }
        },
        Command::Distributions => {
            println!("{:<20} parameters", "distribution");
            for (name, parameters) in objects::distributions() {
                println!("{name:<20} {parameters}");
            }
            println!("\nJSON form: {{\"distribution\": \"exponential\", \"mean\": 5.0}}");
            Ok(())
        }
        Command::Completions { shell } => {
            let mut command = build_command();
            clap_complete::generate(shell, &mut command, "openflex", &mut std::io::stdout());
            Ok(())
        }
        Command::Layout {
            model,
            svg,
            html,
            width,
            height,
            dimension,
            animate,
        } => {
            let model = load_model(&model)?;
            if let Some(path) = html {
                let frames = animate.unwrap_or(1);
                if frames == 0 {
                    return Err(Failure::usage(anyhow!("--animate must be at least 1")));
                }
                let projection = match dimension {
                    Dimension::Flat2d => html::Projection::Flat,
                    Dimension::Iso3d => html::Projection::Isometric,
                };
                let document = html::to_html(&model, width, height, projection, frames);
                return emit(&document, Some(&path), quiet);
            }
            if let Some(frames) = animate {
                if frames == 0 {
                    return Err(Failure::usage(anyhow!("--animate must be at least 1")));
                }
                let base = svg
                    .as_deref()
                    .ok_or_else(|| Failure::usage(anyhow!("--animate requires --svg or --html")))?;
                for index in 0..frames {
                    let frame = layout::Frame {
                        index,
                        count: frames,
                    };
                    let text = render_layout(&model, width, height, dimension, Some(frame));
                    emit(&text, Some(&frame_path(base, index)), quiet)?;
                }
                return Ok(());
            }
            let text = render_layout(&model, width, height, dimension, None);
            emit(&text, svg.as_deref(), quiet)
        }
        Command::Migrate {
            model,
            out,
            in_place,
        } => migrate_model(&model, out.as_deref(), in_place, quiet),
        Command::Optimize {
            model,
            params,
            objectives,
            algorithm,
            population,
            generations,
            mutation_rate,
            weights,
            pareto,
            steps,
            replications,
            seed,
            parallel,
            samples,
            object,
            format,
            output,
        } => run_optimize(
            &model,
            &params,
            &objectives,
            algorithm,
            &OptimizeOptions {
                population,
                generations,
                mutation_rate,
                weights,
                pareto,
                steps,
                replications,
                seed,
                parallel,
                samples,
                object: object.as_deref(),
                format,
                output: output.as_deref(),
                quiet,
                progress,
            },
        ),
        Command::Studio {
            model,
            port,
            no_open,
        } => studio::serve(&model, port, !no_open, quiet)
            .map_err(|error| Failure::runtime(anyhow!(error))),
        Command::Serve { port } => {
            serve::serve(port, quiet).map_err(|error| Failure::runtime(anyhow!(error)))
        }
        Command::Import {
            csv,
            objects,
            connections,
            xlsx,
            out,
            name,
            time_unit,
        } => import_data(&ImportOptions {
            csv: csv.as_deref(),
            objects: objects.as_deref(),
            connections: connections.as_deref(),
            xlsx: xlsx.as_deref(),
            out: &out,
            name: name.as_deref(),
            time_unit: &time_unit,
            quiet,
        }),
        Command::Schema { out } => {
            let schema = schema::generate();
            let text = serde_json::to_string_pretty(&schema)
                .map_err(|error| Failure::runtime(anyhow!(error)))?;
            emit(&(text + "\n"), out.as_deref(), quiet)
        }
    }
}

/// Read and validate a model, attaching the file path to every diagnostic.
fn load_model(path: &Path) -> Result<Model, Failure> {
    let text = fs::read_to_string(path).map_err(|error| {
        Failure::runtime(anyhow!("could not read model {}: {error}", path.display()))
            .with_path(path)
    })?;
    Model::from_json(&text)
        .map_err(|error| Failure::model(path, anyhow!("invalid model {}: {error}", path.display())))
}

fn apply_run_overrides(
    model: &mut Model,
    stop_time: Option<f64>,
    warmup: Option<f64>,
    seeds: &[u64],
    replications: Option<u32>,
) -> anyhow::Result<()> {
    if let Some(value) = stop_time {
        if value <= 0.0 {
            bail!("--stop-time must be positive");
        }
        model.run.stop_time = value;
    }
    if let Some(value) = warmup {
        if value < 0.0 {
            bail!("--warmup cannot be negative");
        }
        model.run.warmup = value;
    }
    if let Some(value) = replications
        && value == 0
    {
        bail!("--replications must be at least 1");
    }
    if seeds.len() > 1 {
        if let Some(value) = replications
            && value != seeds.len() as u32
        {
            bail!(
                "--replications {value} does not match the {} --seed values given",
                seeds.len()
            );
        }
        model.run.seed = seeds[0];
        model.run.replications = seeds.len() as u32;
    } else {
        if let Some(seed) = seeds.first() {
            model.run.seed = *seed;
        }
        if let Some(value) = replications {
            model.run.replications = value;
        }
    }
    model.validate()?;
    Ok(())
}

fn parse_override(entry: &str) -> anyhow::Result<(String, String, serde_json::Value)> {
    let (target, value) = entry
        .split_once('=')
        .with_context(|| format!("--set expects OBJECT.PARAM=VALUE, found '{entry}'"))?;
    let (object, param) = target
        .split_once('.')
        .with_context(|| format!("--set expects OBJECT.PARAM=VALUE, found '{entry}'"))?;
    let value =
        serde_json::from_str(value).unwrap_or_else(|_| serde_json::Value::String(value.into()));
    Ok((object.to_string(), param.to_string(), value))
}

/// Describe the run that would happen, without running it (`--dry-run`).
fn print_run_plan(model: &Model, seeds: &[u64], parallel: Option<usize>) {
    let replications = if seeds.len() > 1 {
        seeds.len() as u32
    } else {
        model.run.replications.max(1)
    };
    let unit = i18n::unit(model.time_unit);
    println!("dry run: {}", model.name);
    println!(
        "  stop time {} {}, warm-up {} {}, replications {}, base seed {}",
        model.run.stop_time, unit, model.run.warmup, unit, replications, model.run.seed
    );
    match parallel {
        Some(workers) => println!("  workers: {workers}"),
        None => println!("  workers: 1 (serial; pass --parallel N to use more)"),
    }
    println!(
        "  objects: {}, connections: {}",
        model.objects.len(),
        model.connections.len()
    );
}

/// Run a model, honouring repeated `--seed` flags and `--parallel`.
///
/// One seed (or none) runs `model.run.replications` replications, each with the
/// seed derived by [`derive_seed`]. Several seeds mean one replication per seed.
/// Replications are independent, so running them on a rayon pool cannot change
/// the result: the samples are collected in replication order before they are
/// aggregated by [`SimReport::from_replications`].
fn execute(
    model: &Model,
    seeds: &[u64],
    parallel: Option<usize>,
    progress: bool,
) -> Result<SimReport, Failure> {
    let configs = run_configs(model, seeds);
    let reporter = Progress::new(progress, configs.len());
    let results = match (parallel, configs.len()) {
        (Some(workers), count) if workers > 1 && count > 1 => {
            use rayon::prelude::*;
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .thread_name(|index| format!("openflex-rep-{index}"))
                .build()
                .map_err(|error| {
                    Failure::runtime(anyhow!("could not start {workers} worker threads: {error}"))
                })?;
            pool.install(|| {
                configs
                    .par_iter()
                    .map(|(index, config)| {
                        let result = simulate(model, config, *index);
                        reporter.tick();
                        result
                    })
                    .collect::<Result<Vec<_>, _>>()
            })?
        }
        _ => {
            let mut results = Vec::with_capacity(configs.len());
            for (index, config) in &configs {
                results.push(simulate(model, config, *index)?);
                reporter.tick();
            }
            results
        }
    };

    let mut report = SimReport::from_replications(model, results);
    if seeds.len() > 1 {
        report.base_seed = seeds[0];
    }
    Ok(report)
}

/// Validate the option combinations around `--save-checkpoint` / `--resume`.
///
/// A checkpoint is a single replication: it stores the event queue, object
/// states and random-number generator of one run, so the CLI refuses options
/// that would run several replications or fork the random stream.
fn validate_checkpoint_options(
    model: &Model,
    seeds: &[u64],
    parallel: Option<usize>,
    trace: Option<&Path>,
    save: Option<&Path>,
    resume: Option<&Path>,
) -> Result<(), Failure> {
    if parallel.is_some_and(|workers| workers > 1) {
        if save.is_some() {
            return Err(Failure::usage(anyhow!(
                "--save-checkpoint cannot be combined with --parallel > 1; a checkpoint captures one replication"
            )));
        }
        if resume.is_some() {
            return Err(Failure::usage(anyhow!(
                "--resume cannot be combined with --parallel > 1; a checkpoint resumes one replication"
            )));
        }
    }
    if save.is_some() && resume.is_none() && (seeds.len() > 1 || model.run.replications > 1) {
        return Err(Failure::usage(anyhow!(
            "--save-checkpoint requires a single replication; pass --replications 1"
        )));
    }
    if resume.is_some() {
        if seeds.len() > 1 {
            return Err(Failure::usage(anyhow!(
                "--resume cannot be combined with several --seed values; a checkpoint stores one replication's random state"
            )));
        }
        if model.run.replications > 1 {
            return Err(Failure::usage(anyhow!(
                "--resume runs exactly one replication; pass --replications 1"
            )));
        }
        if trace.is_some() {
            return Err(Failure::usage(anyhow!(
                "--trace cannot be combined with --resume; tracing is not supported while resuming from a checkpoint"
            )));
        }
    }
    Ok(())
}

/// Run one replication from (or up to) a checkpoint and render its report.
///
/// The engine guarantees that resuming produces exactly the same
/// [`openflex_core::ReplicationResult`] as a single run to the later stop
/// time, so the report rendered here is byte-for-byte the one `openflex run`
/// would print for the full horizon.
/// Run every replication and write one checkpoint per replication.
///
/// The core exposes per-replication `simulate_with_checkpoint`; the CLI loops
/// over `run_configs`, so a bulk core API is not required.
fn run_batch_checkpoints(
    model: &Model,
    seeds: &[u64],
    directory: &Path,
    output: &RunOutput<'_>,
    progress: bool,
) -> Result<(), Failure> {
    let configs = run_configs(model, seeds);
    let reporter = Progress::new(progress, configs.len());
    fs::create_dir_all(directory).map_err(|error| {
        Failure::runtime(anyhow!(
            "could not create checkpoint directory {}: {error}",
            directory.display()
        ))
    })?;
    let mut results = Vec::with_capacity(configs.len());
    let mut paths = Vec::with_capacity(configs.len());
    for (index, config) in &configs {
        let (result, checkpoint) =
            openflex_core::simulate_with_checkpoint(model, config, *index, None)
                .map_err(checkpoint_failure)?;
        let path = directory.join(format!("checkpoint.rep{index}.json"));
        checkpoint::save(&path, &checkpoint)?;
        paths.push(path);
        results.push(result);
        reporter.tick();
    }
    note(
        output.quiet,
        &format!(
            "{} {} checkpoint(s): {}",
            i18n::t("wrote"),
            paths.len(),
            join_paths(&paths)
        ),
    );
    let mut report = SimReport::from_replications(model, results);
    if seeds.len() > 1 {
        report.base_seed = seeds[0];
    }
    finish_run(&report, output)
}

fn run_checkpoint(
    model: &Model,
    trace: Option<&Path>,
    save: Option<&Path>,
    resume: Option<&Path>,
    output: &RunOutput<'_>,
    progress: bool,
) -> Result<(), Failure> {
    let quiet = output.quiet;
    let resume_from = match resume {
        Some(path) => Some(checkpoint::load(path)?),
        None => None,
    };
    let resume_label = resume
        .map(|path| path.display().to_string())
        .unwrap_or_default();

    let config = match &resume_from {
        Some(resume) => {
            if resume.model_name() != model.name {
                return Err(Failure::usage(anyhow!(
                    "checkpoint {} belongs to model '{}', but '{}' was given",
                    resume_label,
                    resume.model_name(),
                    model.name
                )));
            }
            if (resume.warmup() - model.run.warmup).abs() > f64::EPSILON {
                return Err(Failure::usage(anyhow!(
                    "checkpoint {} was recorded with warm-up {}, but the model now uses {}; pass the original --warmup",
                    resume_label,
                    resume.warmup(),
                    model.run.warmup
                )));
            }
            if model.run.stop_time < resume.now() {
                return Err(Failure::usage(anyhow!(
                    "--stop-time {} is before the checkpoint time {}; resume to a later time",
                    model.run.stop_time,
                    resume.now()
                )));
            }
            SimConfig {
                stop_time: model.run.stop_time,
                warmup: model.run.warmup,
                // The checkpoint stores the derived seed; the base seed only
                // affects report metadata, exactly like a normal run.
                seed: resume.seed(),
            }
        }
        None => SimConfig::from_run_settings(&model.run, derive_seed(model.run.seed, 0)),
    };

    let reporter = Progress::new(progress, 1);
    let (result, checkpoint, trace_log) = if let Some(trace_path) = trace {
        let (result, checkpoint, log) =
            openflex_core::simulate_with_checkpoint_traced(model, &config, 0, resume_from.as_ref())
                .map_err(checkpoint_failure)?;
        write_trace(trace_path, &log)?;
        note(
            quiet,
            &format!(
                "{} {} ({} events{})",
                i18n::t("wrote"),
                trace_path.display(),
                log.len(),
                if log.truncated {
                    ", truncated at the event limit"
                } else {
                    ""
                }
            ),
        );
        (result, checkpoint, Some(log))
    } else {
        let (result, checkpoint) =
            openflex_core::simulate_with_checkpoint(model, &config, 0, resume_from.as_ref())
                .map_err(checkpoint_failure)?;
        (result, checkpoint, None)
    };
    let _ = trace_log;

    if let Some(path) = save {
        checkpoint::save(path, &checkpoint)?;
        note(quiet, &format!("{} {}", i18n::t("wrote"), path.display()));
    }

    reporter.tick();
    let report = SimReport::from_replications(model, vec![result]);
    finish_run(&report, output)
}

/// Map checkpoint simulation failures to the CLI's exit-code convention.
///
/// A configuration or model mismatch is a usage error (exit 2); a failing
/// simulation is a runtime error (exit 1).
fn checkpoint_failure(error: openflex_core::SimError) -> Failure {
    use openflex_core::SimError;
    match &error {
        SimError::Config(_) | SimError::Model(_) => Failure::usage(anyhow::Error::new(error)),
        _ => Failure::runtime(anyhow::Error::new(error)),
    }
}

/// Optional report sections requested on the command line.
#[derive(Debug, Clone, Copy, Default)]
struct Sections {
    flow: bool,
    fluid: bool,
}

/// Rendering/export options shared by the plain, traced and checkpoint paths.
struct RunOutput<'a> {
    format: Format,
    report_dir: Option<&'a Path>,
    xlsx: Option<&'a Path>,
    sections: Sections,
    batch_means: Option<u32>,
    batch_means_objects: bool,
    quiet: bool,
}

/// Render and export a report (shared by plain, traced and checkpoint runs).
fn finish_run(report: &SimReport, output: &RunOutput<'_>) -> Result<(), Failure> {
    if output.sections.flow && report.process_flow.is_none() {
        note(
            output.quiet,
            "note: the model has no process flow; nothing to add to the report",
        );
    }
    if output.sections.fluid && report.fluid.is_none() {
        note(
            output.quiet,
            "note: the model has no fluid objects; nothing to add to the report",
        );
    }
    let requested_batches = output.batch_means.or(if output.batch_means_objects {
        Some(10)
    } else {
        None
    });
    let batch = match requested_batches {
        Some(batches) => {
            if batches < 2 {
                return Err(Failure::usage(anyhow!(
                    "--batch-means needs at least 2 batches"
                )));
            }
            if (report.replications.len() as u32) < batches {
                return Err(Failure::usage(anyhow!(
                    "--batch-means {batches} needs at least {batches} replications (the run has {})",
                    report.replications.len()
                )));
            }
            Some(
                report::batch_means_with_objects(report, batches, output.batch_means_objects)
                    .map_err(|error| Failure::usage(anyhow!(error)))?,
            )
        }
        None => None,
    };
    let text = report::render_sim_with_options(
        report,
        output.format.into(),
        output.sections.flow,
        output.sections.fluid,
        requested_batches,
        output.batch_means_objects,
    )?;
    emit(&text, None, output.quiet)?;
    if let Some(directory) = output.report_dir {
        let files = report::write_reports_with_options(
            report,
            directory,
            output.sections.flow,
            output.sections.fluid,
            batch.as_ref(),
        )?;
        note(
            output.quiet,
            &format!("{} {}", i18n::t("wrote"), join_paths(&files)),
        );
    }
    if let Some(path) = output.xlsx {
        report::write_xlsx(report, path)?;
        note(
            output.quiet,
            &format!("{} {}", i18n::t("wrote"), path.display()),
        );
    }
    Ok(())
}

/// Render a layout in the requested projection (and optional animation frame).
fn render_layout(
    model: &Model,
    width: u32,
    height: u32,
    dimension: Dimension,
    frame: Option<layout::Frame>,
) -> String {
    match dimension {
        Dimension::Flat2d => layout::to_svg_flat(model, width, height, frame),
        Dimension::Iso3d => layout::to_svg_3d(model, width, height, frame),
    }
}

/// Derive one animation frame path: `layout.svg` → `layout-0.svg`.
fn frame_path(base: &Path, index: u32) -> PathBuf {
    let stem = base
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "layout".to_string());
    let name = match base.extension() {
        Some(extension) => format!("{stem}-{index}.{}", extension.to_string_lossy()),
        None => format!("{stem}-{index}.svg"),
    };
    base.with_file_name(name)
}

/// Build the replication configurations for one run.
///
/// Several `--seed` values mean one replication per seed (each derived with
/// index 0); otherwise the model's replication count is expanded with
/// [`derive_seed`]. The tracing and parallel paths share this function so a run
/// cannot produce different seeds depending on how it was started.
fn run_configs(model: &Model, seeds: &[u64]) -> Vec<(u32, SimConfig)> {
    if seeds.len() > 1 {
        seeds
            .iter()
            .enumerate()
            .map(|(index, seed)| {
                (
                    index as u32,
                    SimConfig::from_run_settings(&model.run, derive_seed(*seed, 0)),
                )
            })
            .collect()
    } else {
        let replications = model.run.replications.max(1);
        (0..replications)
            .map(|index| {
                (
                    index,
                    SimConfig::from_run_settings(&model.run, derive_seed(model.run.seed, index)),
                )
            })
            .collect()
    }
}

/// Run every replication with tracing enabled and write JSONL trace files.
///
/// One replication writes exactly the requested path. Several replications
/// write `out.rep0.jsonl`, `out.rep1.jsonl`, … next to each other so each trace
/// stays self-contained and `rg`/`jq` can process them independently.
fn execute_traced(
    model: &Model,
    seeds: &[u64],
    path: &Path,
    progress: bool,
    quiet: bool,
) -> Result<SimReport, Failure> {
    let configs = run_configs(model, seeds);
    let several = configs.len() > 1;
    let reporter = Progress::new(progress, configs.len());
    let mut results = Vec::with_capacity(configs.len());

    for (index, config) in &configs {
        let (result, trace) = simulate_traced(model, config, *index)?;
        let file = if several {
            replication_trace_path(path, *index)
        } else {
            path.to_path_buf()
        };
        write_trace(&file, &trace)?;
        note(
            quiet,
            &format!(
                "{} {} ({} events{})",
                i18n::t("wrote"),
                file.display(),
                trace.len(),
                if trace.truncated {
                    ", truncated at the event limit"
                } else {
                    ""
                }
            ),
        );
        results.push(result);
        reporter.tick();
    }

    let mut report = SimReport::from_replications(model, results);
    if seeds.len() > 1 {
        report.base_seed = seeds[0];
    }
    Ok(report)
}

/// Derive the per-replication trace path: `events.jsonl` → `events.rep0.jsonl`.
fn replication_trace_path(base: &Path, index: u32) -> PathBuf {
    let stem = base
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "trace".to_string());
    let name = match base.extension() {
        Some(extension) => format!("{stem}.rep{index}.{}", extension.to_string_lossy()),
        None => format!("{stem}.rep{index}.jsonl"),
    };
    base.with_file_name(name)
}

/// One JSONL trace line.
///
/// `TraceEvent` skips `data` when it is empty, but a trace file is a data feed:
/// every line keeps the same five keys so downstream tools can rely on the
/// schema without special-casing the first event.
#[derive(serde::Serialize)]
struct TraceLine<'a> {
    time: f64,
    seq: u64,
    event: &'a str,
    object: &'a str,
    data: &'a std::collections::BTreeMap<String, f64>,
}

/// Write a [`openflex_core::TraceLog`] as JSON Lines, one event per line.
fn write_trace(path: &Path, trace: &openflex_core::TraceLog) -> Result<(), Failure> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            Failure::runtime(anyhow!("could not create {}: {error}", parent.display()))
        })?;
    }
    let file = fs::File::create(path).map_err(|error| {
        Failure::runtime(anyhow!("could not write trace {}: {error}", path.display()))
    })?;
    let mut writer = BufWriter::new(file);
    for event in &trace.events {
        let line = TraceLine {
            time: event.time,
            seq: event.seq,
            event: &event.event,
            object: &event.object,
            data: &event.data,
        };
        serde_json::to_writer(&mut writer, &line).map_err(|error| {
            Failure::runtime(anyhow!(
                "could not serialise trace event in {}: {error}",
                path.display()
            ))
        })?;
        writer.write_all(b"\n").map_err(|error| {
            Failure::runtime(anyhow!("could not write trace {}: {error}", path.display()))
        })?;
    }
    writer.flush().map_err(|error| {
        Failure::runtime(anyhow!("could not write trace {}: {error}", path.display()))
    })?;
    Ok(())
}

/// Run every replication of one scenario with tracing enabled.
///
/// The files land in `directory`, which is the same directory
/// `write_experiment_reports` uses for the scenario's own report. One
/// replication writes `trace.jsonl`; several write `trace.rep0.jsonl`, …
fn execute_scenario_traced(
    model: &Model,
    directory: &Path,
    quiet: bool,
) -> Result<SimReport, Failure> {
    let configs = run_configs(model, &[]);
    let several = configs.len() > 1;
    let mut results = Vec::with_capacity(configs.len());
    for (index, config) in &configs {
        let (result, log) = simulate_traced(model, config, *index)?;
        let file = if several {
            directory.join(format!("trace.rep{index}.jsonl"))
        } else {
            directory.join("trace.jsonl")
        };
        write_trace(&file, &log)?;
        note(
            quiet,
            &format!(
                "{} {} ({} events{})",
                i18n::t("wrote"),
                file.display(),
                log.len(),
                if log.truncated {
                    ", truncated at the event limit"
                } else {
                    ""
                }
            ),
        );
        results.push(result);
    }
    Ok(SimReport::from_replications(model, results))
}

fn run_experiments(
    model: &Model,
    name: Option<&str>,
    format: Format,
    report_dir: Option<&Path>,
    trace: bool,
    sections: Sections,
    quiet: bool,
) -> Result<(), Failure> {
    let experiments: Vec<_> = model
        .experiments
        .iter()
        .filter(|experiment| name.is_none_or(|wanted| experiment.name == wanted))
        .collect();
    if experiments.is_empty() {
        return Err(Failure::usage(anyhow!(
            "the model defines no experiments; add an `experiments` array or run `openflex run`"
        )));
    }
    for experiment in experiments {
        let scenarios = if experiment.scenarios.is_empty() {
            vec![Scenario {
                name: "baseline".into(),
                ..Default::default()
            }]
        } else {
            experiment.scenarios.clone()
        };
        let mut scenario_reports = Vec::with_capacity(scenarios.len());
        for scenario in scenarios {
            let mut scenario_model = model.clone();
            for over in &scenario.overrides {
                scenario_model = scenario_model
                    .with_override(&over.object, &over.param, over.value.clone())
                    .map_err(|error| {
                        Failure::usage(anyhow::Error::new(error).context(format!(
                            "experiment {} scenario {}",
                            experiment.name, scenario.name
                        )))
                    })?;
            }
            scenario_model.run.replications = experiment.replications;
            if let Some(stop_time) = scenario.stop_time {
                scenario_model.run.stop_time = stop_time;
            }
            if let Some(warmup) = scenario.warmup {
                scenario_model.run.warmup = warmup;
            }
            // Traces go next to the scenario's archived report, mirroring the
            // layout `write_experiment_reports` uses below.
            let trace_dir = if trace {
                report_dir.map(|root| {
                    root.join(report::sanitize(&experiment.name))
                        .join(report::sanitize(&scenario.name))
                })
            } else {
                None
            };
            let report = match trace_dir.as_deref() {
                Some(directory) => execute_scenario_traced(&scenario_model, directory, quiet)?,
                None => execute(&scenario_model, &[], None, false)?,
            };
            scenario_reports.push(report::ScenarioReport {
                name: scenario.name.clone(),
                overrides: scenario.overrides.clone(),
                report,
            });
        }
        let experiment_report = report::ExperimentReport {
            experiment: experiment.name.clone(),
            model: model.name.clone(),
            time_unit: model.time_unit,
            scenarios: scenario_reports,
        };
        let text = report::render_experiment_with_sections(
            &experiment_report,
            format.into(),
            sections.flow,
            sections.fluid,
        )?;
        emit(&text, None, quiet)?;
        if let Some(root) = report_dir {
            let files = report::write_experiment_reports_with_sections(
                &experiment_report,
                root,
                sections.flow,
                sections.fluid,
            )?;
            note(
                quiet,
                &format!("{} {}", i18n::t("wrote"), join_paths(&files)),
            );
        }
    }
    Ok(())
}

/// Write rendered text to `output`, or print it.
fn emit(text: &str, output: Option<&Path>, quiet: bool) -> Result<(), Failure> {
    match output {
        Some(path) => {
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
            {
                fs::create_dir_all(parent).map_err(|error| {
                    Failure::runtime(anyhow!("could not create {}: {error}", parent.display()))
                })?;
            }
            fs::write(path, text).map_err(|error| {
                Failure::runtime(anyhow!("could not write {}: {error}", path.display()))
            })?;
            note(quiet, &format!("{} {}", i18n::t("wrote"), path.display()));
        }
        None => print!("{text}"),
    }
    Ok(())
}

/// Print an informational message unless `--quiet` was given.
fn note(quiet: bool, message: &str) {
    if !quiet {
        eprintln!("{message}");
    }
}

fn join_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn create_model(directory: &Path, template: &str, force: bool, quiet: bool) -> Result<(), Failure> {
    if directory.exists() {
        let mut entries = fs::read_dir(directory).map_err(|error| {
            Failure::runtime(anyhow!(
                "could not inspect {}: {error}",
                directory.display()
            ))
        })?;
        if entries.next().is_some() && !force {
            return Err(Failure::usage(anyhow!(
                "{} is not empty; pass --force to write into it anyway",
                directory.display()
            )));
        }
    }
    fs::create_dir_all(directory).map_err(|error| {
        Failure::runtime(anyhow!("could not create {}: {error}", directory.display()))
    })?;
    let (file_name, contents) = match template {
        "basic" => ("basic.ofm", templates::BASIC),
        "mm1" => ("mm1.ofm", templates::MM1),
        "line" => ("line.ofm", templates::LINE),
        other => {
            return Err(Failure::usage(anyhow!(
                "unknown template '{other}' (expected basic, mm1 or line)"
            )));
        }
    };
    let path = directory.join(file_name);
    fs::write(&path, contents).map_err(|error| {
        Failure::runtime(anyhow!("could not write {}: {error}", path.display()))
    })?;
    println!("{} {}", i18n::t("created"), path.display());
    note(
        quiet,
        &format!(
            "next: openflex validate {} && openflex run {}",
            path.display(),
            path.display()
        ),
    );
    Ok(())
}

fn parse_time_unit(value: &str) -> Result<&'static str, Failure> {
    match value {
        "seconds" | "second" | "s" => Ok("seconds"),
        "minutes" | "minute" | "min" => Ok("minutes"),
        "hours" | "hour" | "h" => Ok("hours"),
        "days" | "day" | "d" => Ok("days"),
        other => Err(Failure::usage(anyhow!(
            "unknown time unit '{other}' (expected seconds, minutes, hours or days)"
        ))),
    }
}

/// Migrate a model file to the current format version.
///
/// The model crate does not expose a general migration API yet; the CLI
/// validates the declared version, re-validates the document through
/// `Model::from_json` and writes a canonical pretty representation. A future
/// `openflex_model::migrate_json` can be plugged in here without changing the
/// command contract.
/// Write an interval-average object-content series with checkpoint stepping.
///
/// For every replication the engine is advanced at fixed intervals with
/// `simulate_with_checkpoint`; `ReplicationResult` statistics are cumulative
/// from the warm-up reset, so the average content of an interval is the
/// difference of `avg_content * elapsed` divided by the interval length. This
/// gives a genuine sampled series (not just the final report) without a new
/// engine API.
fn write_timeseries(
    model: &Model,
    seeds: &[u64],
    path: &Path,
    interval: f64,
    quiet: bool,
) -> Result<(), Failure> {
    let configs = run_configs(model, seeds);
    for (_, config) in &configs {
        if config.stop_time <= config.warmup {
            return Err(Failure::usage(anyhow!(
                "model stop_time must be greater than warm-up for --timeseries"
            )));
        }
        let steps = ((config.stop_time - config.warmup) / interval).ceil();
        if steps > 10_000.0 {
            return Err(Failure::usage(anyhow!(
                "--timeseries would write more than 10,000 samples; increase --interval"
            )));
        }
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            Failure::runtime(anyhow!("could not create {}: {error}", parent.display()))
        })?;
    }
    let file = fs::File::create(path).map_err(|error| {
        Failure::runtime(anyhow!(
            "could not write timeseries {}: {error}",
            path.display()
        ))
    })?;
    let mut writer = BufWriter::new(file);
    writer
        .write_all(b"replication,time,object,kind,content_avg,utilization,throughput\n")
        .map_err(|error| {
            Failure::runtime(anyhow!("could not write {}: {error}", path.display()))
        })?;

    for (index, (_, base)) in configs.iter().enumerate() {
        let warmup = base.warmup;
        let stop = base.stop_time;
        let mut previous = None;
        let mut previous_time = warmup;
        let mut previous_cumulative: BTreeMap<String, f64> = BTreeMap::new();
        let mut target = warmup + interval;
        loop {
            let target_time = target.min(stop);
            let config = SimConfig {
                stop_time: target_time,
                warmup,
                seed: base.seed,
            };
            let (result, checkpoint) =
                simulate_with_checkpoint(model, &config, index as u32, previous.as_ref())?;
            previous = Some(checkpoint);
            let elapsed = (target_time - warmup).max(0.0);
            let duration = (target_time - previous_time).max(f64::EPSILON);
            for (name, stats) in &result.objects {
                let cumulative = stats.avg_content * elapsed;
                let before = previous_cumulative.get(name).copied().unwrap_or(0.0);
                let interval_average = ((cumulative - before) / duration).max(0.0);
                let line = format!(
                    "{index},{:.6},{},{},{:.6},{:.6},{:.6}\n",
                    target_time,
                    csv_field(name),
                    csv_field(stats.kind.as_str()),
                    interval_average,
                    stats.utilization,
                    stats.throughput
                );
                writer.write_all(line.as_bytes()).map_err(|error| {
                    Failure::runtime(anyhow!("could not write {}: {error}", path.display()))
                })?;
                previous_cumulative.insert(name.clone(), cumulative);
            }
            previous_time = target_time;
            if target_time >= stop {
                break;
            }
            target += interval;
        }
    }
    writer.flush().map_err(|error| {
        Failure::runtime(anyhow!("could not write {}: {error}", path.display()))
    })?;
    note(quiet, &format!("{} {}", i18n::t("wrote"), path.display()));
    Ok(())
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn migrate_model(
    path: &Path,
    out: Option<&Path>,
    in_place: bool,
    quiet: bool,
) -> Result<(), Failure> {
    let text = fs::read_to_string(path).map_err(|error| {
        Failure::runtime(anyhow!("could not read model {}: {error}", path.display()))
            .with_path(path)
    })?;
    let model = openflex_model::migrate_json(&text).map_err(|error| {
        Failure::usage(anyhow!("cannot migrate {}: {error}", path.display())).with_path(path)
    })?;
    let migrated = match model.to_json() {
        Ok(mut json) => {
            if !json.ends_with('\n') {
                json.push('\n');
            }
            json
        }
        Err(error) => return Err(Failure::runtime(anyhow!(error))),
    };
    match (out, in_place) {
        (Some(out), false) => {
            write_model_atomic(out, &migrated)?;
            note(quiet, &format!("{} {}", i18n::t("wrote"), out.display()));
            Ok(())
        }
        (None, true) => {
            write_model_atomic(path, &migrated)?;
            note(quiet, &format!("{} {}", i18n::t("wrote"), path.display()));
            Ok(())
        }
        (None, false) => {
            emit(&migrated, None, quiet)?;
            note(
                quiet,
                "note: the migrated model was printed to stdout; pass --out or --in-place to write it",
            );
            Ok(())
        }
        (Some(_), true) => Err(Failure::usage(anyhow!(
            "pass either --out or --in-place, not both"
        ))),
    }
}

/// Write a model atomically: validate happened before, then rename in place.
fn write_model_atomic(path: &Path, text: &str) -> Result<(), Failure> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            Failure::runtime(anyhow!("could not create {}: {error}", parent.display()))
        })?;
    }
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temporary = directory.join(format!(".openflex-migrate-{}.tmp", std::process::id()));
    if let Err(error) = fs::write(&temporary, text) {
        let _ = fs::remove_file(&temporary);
        return Err(Failure::runtime(anyhow!(
            "could not write {}: {error}",
            temporary.display()
        )));
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(Failure::runtime(anyhow!(
            "could not replace {}: {error}",
            path.display()
        )));
    }
    Ok(())
}

/// Options for one `openflex optimize` invocation.
struct OptimizeOptions<'a> {
    population: usize,
    generations: usize,
    mutation_rate: f64,
    weights: Vec<String>,
    pareto: bool,
    steps: u32,
    replications: Option<u32>,
    seed: Option<u64>,
    parallel: Option<usize>,
    samples: Option<usize>,
    object: Option<&'a str>,
    format: Format,
    output: Option<&'a Path>,
    quiet: bool,
    progress: bool,
}

/// Wrap a string error from the optimizer as a usage failure (exit 2).
fn optimize_usage(error: String) -> Failure {
    Failure::usage(anyhow!(error))
}

/// Evaluate a parameter grid and print the ranked candidates plus the best one.
fn run_optimize(
    model_path: &Path,
    params: &[String],
    objectives: &[optimize::Objective],
    algorithm: optimize::Algorithm,
    options: &OptimizeOptions<'_>,
) -> Result<(), Failure> {
    if params.is_empty() {
        return Err(Failure::usage(anyhow!(
            "optimize needs at least one --param, e.g. --param Proc1.process_time.mean=2..6"
        )));
    }
    if objectives.is_empty() {
        return Err(Failure::usage(anyhow!(
            "optimize needs at least one --objective"
        )));
    }
    if objectives.len() > 3 {
        return Err(Failure::usage(anyhow!(
            "at most three objectives can be combined"
        )));
    }
    if let Some(workers) = options.parallel
        && workers == 0
    {
        return Err(Failure::usage(anyhow!("--parallel must be at least 1")));
    }
    if options.population < 2 || options.population > 500 {
        return Err(Failure::usage(anyhow!(
            "--population must be between 2 and 500"
        )));
    }
    if options.generations == 0 || options.generations > 200 {
        return Err(Failure::usage(anyhow!(
            "--generations must be between 1 and 200"
        )));
    }
    if !options.mutation_rate.is_finite() || !(0.0..=1.0).contains(&options.mutation_rate) {
        return Err(Failure::usage(anyhow!(
            "--mutation-rate must be between 0 and 1"
        )));
    }
    let mut base = load_model(model_path)?;
    if let Some(replications) = options.replications {
        if replications == 0 {
            return Err(Failure::usage(anyhow!("--replications must be at least 1")));
        }
        base.run.replications = replications;
    }
    if let Some(seed) = options.seed {
        base.run.seed = seed;
    }
    if let Some(name) = options.object
        && base.object(name).is_none()
    {
        return Err(Failure::usage(anyhow!(
            "--object '{name}' does not exist in the model"
        )));
    }

    let ranges = params
        .iter()
        .map(|spec| optimize::parse_param(spec, options.steps).map_err(optimize_usage))
        .collect::<Result<Vec<_>, _>>()?;
    let weights = optimize::parse_weights(&options.weights, objectives).map_err(optimize_usage)?;

    let estimate = match algorithm {
        optimize::Algorithm::Grid => {
            optimize::combinations(&ranges, options.samples, base.run.seed)
                .map(|combinations| combinations.len())
                .map_err(optimize_usage)?
        }
        optimize::Algorithm::Random => options.samples.unwrap_or(50),
        optimize::Algorithm::Genetic => options.population.saturating_mul(options.generations),
    };
    note(
        options.quiet,
        &format!(
            "evaluating · {} · up to {estimate} candidate(s)…",
            match algorithm {
                optimize::Algorithm::Grid => "grid",
                optimize::Algorithm::Random => "random",
                optimize::Algorithm::Genetic => "genetic",
            }
        ),
    );
    let reporter = Progress::new(options.progress, estimate.max(1));
    let mut next_index = 0usize;
    let mut evaluate = |batch: &[Vec<f64>]| -> Result<Vec<optimize::Candidate>, Failure> {
        let models = batch
            .iter()
            .map(|values| optimize::apply(&base, &ranges, values).map_err(optimize_usage))
            .collect::<Result<Vec<_>, _>>()?;
        let reports = if let Some(workers) = options.parallel.filter(|workers| *workers > 1) {
            use rayon::prelude::*;
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .thread_name(|index| format!("openflex-opt-{index}"))
                .build()
                .map_err(|error| {
                    Failure::runtime(anyhow!("could not start {workers} worker threads: {error}"))
                })?;
            pool.install(|| {
                models
                    .par_iter()
                    .map(|model| {
                        let result = execute(model, &[], None, false);
                        reporter.tick();
                        result
                    })
                    .collect::<Result<Vec<_>, _>>()
            })?
        } else {
            let mut reports = Vec::with_capacity(models.len());
            for model in &models {
                reports.push(execute(model, &[], None, false)?);
                reporter.tick();
            }
            reports
        };
        reports
            .iter()
            .zip(batch.iter())
            .map(|(report, values)| {
                let index = next_index;
                next_index += 1;
                optimize::candidate(
                    index,
                    &ranges,
                    values,
                    report,
                    objectives,
                    &weights,
                    options.object,
                )
                .map_err(optimize_usage)
            })
            .collect()
    };

    let candidates = match algorithm {
        optimize::Algorithm::Grid => {
            let combinations = optimize::combinations(&ranges, options.samples, base.run.seed)
                .map_err(optimize_usage)?;
            evaluate(&combinations)?
        }
        optimize::Algorithm::Random => {
            let count = options.samples.unwrap_or(50).min(10_000);
            let combinations = optimize::random_combinations(&ranges, count, base.run.seed)
                .map_err(optimize_usage)?;
            evaluate(&combinations)?
        }
        optimize::Algorithm::Genetic => optimize::genetic_search(
            &ranges,
            &optimize::GeneticConfig {
                population: options.population,
                generations: options.generations,
                mutation_rate: options.mutation_rate,
            },
            base.run.seed,
            evaluate,
        )?,
    };
    if candidates.is_empty() {
        return Err(Failure::runtime(anyhow!(
            "the optimizer did not evaluate any candidate"
        )));
    }
    let best = optimize::best_index(&candidates);
    let text = optimize::render(
        &base.name,
        objectives,
        &weights,
        options.object,
        &ranges,
        &candidates,
        best,
        options.pareto,
        options.format.into(),
    )
    .map_err(|error| Failure::runtime(anyhow!(error)))?;
    emit(&text, options.output, options.quiet)?;

    if let Some(best_candidate) = candidates.get(best) {
        let parameters = best_candidate
            .assignments
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(", ");
        let label = objectives
            .iter()
            .map(|objective| objective.label())
            .collect::<Vec<_>>()
            .join("+");
        note(
            options.quiet,
            &format!(
                "best: {parameters} -> {label} {:.4} (weighted score {:.4})",
                best_candidate.metric, best_candidate.score
            ),
        );
    }
    Ok(())
}

/// Optional input tables for `openflex import`.
struct ImportOptions<'a> {
    csv: Option<&'a Path>,
    objects: Option<&'a Path>,
    connections: Option<&'a Path>,
    xlsx: Option<&'a Path>,
    out: &'a Path,
    name: Option<&'a str>,
    time_unit: &'a str,
    quiet: bool,
}

fn import_data(options: &ImportOptions<'_>) -> Result<(), Failure> {
    if options.xlsx.is_some() {
        return import_xlsx(options);
    }
    let unit = parse_time_unit(options.time_unit)?;
    if options.csv.is_none() && options.objects.is_none() {
        return Err(Failure::usage(anyhow!(
            "import needs at least one input table: --csv, --objects (with optional --connections) or --xlsx"
        )));
    }

    let arrivals = match read_table(options.csv, "arrivals")? {
        Some(text) => import::parse_csv(&text).map_err(|error| Failure::usage(anyhow!(error)))?,
        None => Vec::new(),
    };
    let object_rows = match read_table(options.objects, "objects")? {
        Some(text) => {
            import::parse_objects_csv(&text).map_err(|error| Failure::usage(anyhow!(error)))?
        }
        None => Vec::new(),
    };
    let connection_rows = match read_table(options.connections, "connections")? {
        Some(text) => {
            import::parse_connections_csv(&text).map_err(|error| Failure::usage(anyhow!(error)))?
        }
        None => Vec::new(),
    };

    let model_name = import_model_name(options);
    let (model, stop_time) =
        import::build_model(&model_name, unit, &arrivals, &object_rows, &connection_rows)
            .map_err(|error| Failure::usage(anyhow!(error)))?;
    write_imported_model(options, &model)?;
    note(
        options.quiet,
        &format!(
            "imported {} objects, {} connections, {} arrival rows; stop time {} {}",
            object_rows.len(),
            connection_rows.len(),
            arrivals.len(),
            stop_time,
            i18n::unit(unit_for(unit))
        ),
    );
    Ok(())
}

/// Read an `.xlsx` workbook and convert its sheets exactly like the CSV path.
fn import_xlsx(options: &ImportOptions<'_>) -> Result<(), Failure> {
    let unit = parse_time_unit(options.time_unit)?;
    let path = options
        .xlsx
        .ok_or_else(|| Failure::usage(anyhow!("--xlsx needs a file")))?;
    let bytes = fs::read(path).map_err(|error| {
        Failure::runtime(anyhow!(
            "could not read workbook {}: {error}",
            path.display()
        ))
        .with_path(path)
    })?;
    let imported = import::parse_xlsx(&bytes).map_err(|error| {
        Failure::usage(anyhow!("invalid workbook {}: {error}", path.display())).with_path(path)
    })?;
    let model_name = import_model_name(options);
    let (model, stop_time) = import::build_model(
        &model_name,
        unit,
        &imported.arrivals,
        &imported.objects,
        &imported.connections,
    )
    .map_err(|error| Failure::usage(anyhow!(error)))?;
    write_imported_model(options, &model)?;
    note(
        options.quiet,
        &format!(
            "imported {} objects, {} connections, {} arrival rows from {}; stop time {} {}",
            imported.objects.len(),
            imported.connections.len(),
            imported.arrivals.len(),
            path.display(),
            stop_time,
            i18n::unit(unit_for(unit))
        ),
    );
    Ok(())
}

fn import_model_name(options: &ImportOptions<'_>) -> String {
    options.name.map(str::to_string).unwrap_or_else(|| {
        options
            .out
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .filter(|stem| !stem.is_empty())
            .unwrap_or_else(|| "imported".into())
    })
}

fn unit_for(unit: &'static str) -> TimeUnit {
    match unit {
        "minutes" => TimeUnit::Minutes,
        "hours" => TimeUnit::Hours,
        "days" => TimeUnit::Days,
        _ => TimeUnit::Seconds,
    }
}

fn write_imported_model(options: &ImportOptions<'_>, model: &str) -> Result<(), Failure> {
    if let Some(parent) = options.out.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            Failure::runtime(anyhow!("could not create {}: {error}", parent.display()))
        })?;
    }
    fs::write(options.out, model).map_err(|error| {
        Failure::runtime(anyhow!(
            "could not write {}: {error}",
            options.out.display()
        ))
    })?;
    println!("{} {}", i18n::t("created"), options.out.display());
    Ok(())
}

/// Read one optional import table, attaching its path to read errors.
fn read_table(path: Option<&Path>, label: &str) -> Result<Option<String>, Failure> {
    match path {
        None => Ok(None),
        Some(path) => {
            let text = fs::read_to_string(path).map_err(|error| {
                Failure::runtime(anyhow!(
                    "could not read {label} table {}: {error}",
                    path.display()
                ))
                .with_path(path)
            })?;
            Ok(Some(text))
        }
    }
}
