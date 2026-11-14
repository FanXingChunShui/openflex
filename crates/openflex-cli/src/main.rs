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

mod import;
mod layout;
mod objects;
mod progress;
mod schema;
mod serve;
mod templates;

use std::ffi::OsString;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow, bail};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use openflex_core::{SimConfig, SimReport, derive_seed, simulate, simulate_traced};
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
        /// Write a JSONL event trace; with several replications the files are `FILE.rep0.jsonl`, `FILE.rep1.jsonl`, … (cannot be combined with --parallel > 1).
        #[arg(long, value_name = "FILE")]
        trace: Option<PathBuf>,
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
    /// Export the model's 2D object layout as an SVG drawing.
    Layout {
        /// Path to the model file.
        model: PathBuf,
        /// Write the SVG here instead of standard output.
        #[arg(long, value_name = "FILE")]
        svg: Option<PathBuf>,
        /// SVG width in pixels.
        #[arg(long, default_value_t = 1200)]
        width: u32,
        /// SVG height in pixels.
        #[arg(long, default_value_t = 800)]
        height: u32,
    },
    /// Serve a minimal read-only HTTP API on 127.0.0.1 (`/health`, `/objects`).
    Serve {
        /// TCP port to bind (0 picks a free port).
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
    /// Import a CSV arrival table (`time,quantity[,item_type]`) as a model.
    Import {
        /// CSV file with arrival times.
        #[arg(long, value_name = "CSV")]
        csv: PathBuf,
        /// Model file to write.
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
        /// Model name (defaults to the CSV file stem).
        #[arg(long)]
        name: Option<String>,
        /// Time unit of the CSV timestamps.
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
            dry_run,
        } => {
            let mut model = load_model(&path)?;
            apply_run_overrides(&mut model, stop_time, warmup, &seed, replications).map_err(
                |error| Failure::usage(error.context(format!("model {}", path.display()))),
            )?;
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
            if dry_run {
                print_run_plan(&model, &seed, parallel);
                return Ok(());
            }
            if trace.is_some() && parallel.is_some_and(|workers| workers > 1) {
                return Err(Failure::usage(anyhow!(
                    "--trace cannot be combined with --parallel > 1; traces are written one replication at a time"
                )));
            }
            let report = match trace.as_deref() {
                Some(trace_path) => execute_traced(&model, &seed, trace_path, progress, quiet)?,
                None => execute(&model, &seed, parallel, progress)?,
            };
            let text = report::render_sim(&report, format.into())?;
            emit(&text, None, quiet)?;
            if let Some(directory) = report_dir {
                let files = report::write_reports(&report, &directory)?;
                note(
                    quiet,
                    &format!("{} {}", i18n::t("wrote"), join_paths(&files)),
                );
            }
            if let Some(path) = xlsx {
                report::write_xlsx(&report, &path)?;
                note(quiet, &format!("{} {}", i18n::t("wrote"), path.display()));
            }
            Ok(())
        }
        Command::Experiment {
            model: path,
            name,
            format,
            report_dir,
        } => {
            let model = load_model(&path)?;
            run_experiments(
                &model,
                name.as_deref(),
                format,
                report_dir.as_deref(),
                quiet,
            )
        }
        Command::Report {
            report: path,
            experiment,
            format,
            output,
        } => {
            let text = match (path, experiment) {
                (Some(path), None) => {
                    let sim = report::read_report(&path)
                        .map_err(|error| Failure::from(error).with_path(&path))?;
                    report::render_sim(&sim, format.into())?
                }
                (None, Some(directory)) => {
                    let experiment = report::read_experiment(&directory)
                        .map_err(|error| Failure::from(error).with_path(&directory))?;
                    report::render_experiment(&experiment, format.into())?
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
            width,
            height,
        } => {
            let model = load_model(&model)?;
            let text = layout::to_svg(&model, width, height);
            emit(&text, svg.as_deref(), quiet)
        }
        Command::Serve { port } => {
            serve::serve(port, quiet).map_err(|error| Failure::runtime(anyhow!(error)))
        }
        Command::Import {
            csv,
            out,
            name,
            time_unit,
        } => import_csv(&csv, &out, name.as_deref(), &time_unit, quiet),
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

fn run_experiments(
    model: &Model,
    name: Option<&str>,
    format: Format,
    report_dir: Option<&Path>,
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
            let report = execute(&scenario_model, &[], None, false)?;
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
        let text = report::render_experiment(&experiment_report, format.into())?;
        emit(&text, None, quiet)?;
        if let Some(root) = report_dir {
            let files = report::write_experiment_reports(&experiment_report, root)?;
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

fn import_csv(
    csv: &Path,
    out: &Path,
    name: Option<&str>,
    time_unit: &str,
    quiet: bool,
) -> Result<(), Failure> {
    let unit = parse_time_unit(time_unit)?;
    let text = fs::read_to_string(csv).map_err(|error| {
        Failure::runtime(anyhow!("could not read {}: {error}", csv.display())).with_path(csv)
    })?;
    let rows = import::parse_csv(&text).map_err(|error| Failure::usage(anyhow!(error)))?;
    let model_name = name.map(str::to_string).unwrap_or_else(|| {
        csv.file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .filter(|stem| !stem.is_empty())
            .unwrap_or_else(|| "imported".into())
    });
    let (model, stop_time) = import::build_model(&model_name, unit, &rows)
        .map_err(|error| Failure::usage(anyhow!(error)))?;
    if let Some(parent) = out.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            Failure::runtime(anyhow!("could not create {}: {error}", parent.display()))
        })?;
    }
    fs::write(out, model)
        .map_err(|error| Failure::runtime(anyhow!("could not write {}: {error}", out.display())))?;
    println!("{} {}", i18n::t("created"), out.display());
    note(
        quiet,
        &format!(
            "imported {} arrival rows, stop time {} {}",
            rows.len(),
            stop_time,
            i18n::unit(match unit {
                "minutes" => TimeUnit::Minutes,
                "hours" => TimeUnit::Hours,
                "days" => TimeUnit::Days,
                _ => TimeUnit::Seconds,
            })
        ),
    );
    Ok(())
}
