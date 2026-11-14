//! OpenFlex command line interface.
//!
//! `openflex` is the headless front end to the engine: it validates a model,
//! runs it with any number of replications, runs the experiments declared in
//! the model, re-renders saved reports, compares two scenarios, and prints or
//! exports the statistics.
//!
//! Exit codes are stable so the CLI can be scripted:
//!
//! - `0` success
//! - `1` runtime error (unreadable file, simulation failure)
//! - `2` usage error or invalid model (`validate` found a problem)

mod templates;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow, bail};
use clap::{Parser, Subcommand, ValueEnum};
use openflex_core::{SimConfig, SimReport, derive_seed, run_model, simulate};
use openflex_model::{Model, ObjectKind, Scenario};
use openflex_report as report;

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
                  deterministic for a given seed, and every report can be exported as JSON, CSV \
                  or Markdown.",
    after_help = "EXIT CODES:\n  0  success\n  1  runtime error (unreadable input, simulation failure)\n  2  usage error or invalid model\n\nEXAMPLES:\n  openflex new demo && cd demo && openflex validate basic.ofm\n  openflex run basic.ofm --stop-time 1000 --seed 7 --format json\n  openflex run basic.ofm --replications 10 --report-dir reports\n  openflex compare reports/report.json reports-faster/report.json\n  openflex report reports/report.json --format markdown --output summary.md\n  openflex experiment basic.ofm --format json --report-dir experiments",
    propagate_version = true
)]
struct Cli {
    /// Suppress informational messages on stderr (the report output is unchanged).
    #[arg(long, global = true)]
    quiet: bool,
    /// Disable ANSI colours; accepted for compatibility, output is plain text today.
    #[arg(long = "no-color", global = true)]
    no_color: bool,
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
        /// Override the stop time (model time units).
        #[arg(long)]
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

/// A CLI failure with the process exit code it should produce.
struct Failure {
    code: i32,
    error: anyhow::Error,
}

impl Failure {
    /// An error caused by the environment or by a run in progress (exit 1).
    fn runtime(error: impl Into<anyhow::Error>) -> Self {
        Self {
            code: EXIT_RUNTIME,
            error: error.into(),
        }
    }

    /// An error caused by how the CLI was called or by an invalid model (exit 2).
    fn usage(error: impl Into<anyhow::Error>) -> Self {
        Self {
            code: EXIT_USAGE,
            error: error.into(),
        }
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
    let cli = Cli::parse();
    let quiet = cli.quiet;
    // `--no-color` is accepted so scripts can pass it unconditionally; the
    // renderers emit plain text, so nothing has to be stripped today.
    let _ = cli.no_color;
    if let Err(failure) = run(cli.command, quiet) {
        eprintln!("error: {:#}", failure.error);
        std::process::exit(failure.code);
    }
}

fn run(command: Command, quiet: bool) -> Result<(), Failure> {
    match command {
        Command::Validate { model } => {
            let model = load_model(&model)?;
            println!(
                "ok: {} ({} objects, {} connections)",
                model.name,
                model.objects.len(),
                model.connections.len()
            );
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
            let report = execute(&model, &seed)?;
            let text = report::render_sim(&report, format.into())?;
            emit(&text, None, quiet)?;
            if let Some(directory) = report_dir {
                let files = report::write_reports(&report, &directory)?;
                note(quiet, &format!("wrote {}", join_paths(&files)));
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
                    let sim = report::read_report(&path)?;
                    report::render_sim(&sim, format.into())?
                }
                (None, Some(directory)) => {
                    let experiment = report::read_experiment(&directory)?;
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
            let before = report::read_report(&baseline)?;
            let after = report::read_report(&candidate)?;
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
                    let (summary, _) = describe_kind(*kind);
                    println!("{:<12} {}", kind.as_str(), summary);
                }
                Ok(())
            }
            ObjectCommand::Describe { kind } => {
                let Some(kind) = ObjectKind::parse(&kind) else {
                    return Err(Failure::usage(anyhow!("unknown object kind '{kind}'")));
                };
                let (summary, parameters) = describe_kind(kind);
                println!("{}: {summary}\n\nParameters:\n{parameters}", kind.as_str());
                Ok(())
            }
        },
        Command::Distributions => {
            print_distributions();
            Ok(())
        }
    }
}

/// Read and validate a model, attaching the file path to every diagnostic.
fn load_model(path: &Path) -> Result<Model, Failure> {
    let text = fs::read_to_string(path).map_err(|error| {
        Failure::runtime(anyhow!("could not read model {}: {error}", path.display()))
    })?;
    Model::from_json(&text)
        .map_err(|error| Failure::usage(anyhow!("invalid model {}: {error}", path.display())))
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

/// Run a model, honouring repeated `--seed` flags.
///
/// One seed (or none) delegates to [`run_model`], which derives the seed of
/// each replication from the base seed. Several seeds mean one replication per
/// seed; the engine has no "list of seeds" entry point, so the CLI drives
/// [`simulate`] directly and aggregates with the engine's own report builder.
fn execute(model: &Model, seeds: &[u64]) -> Result<SimReport, Failure> {
    if seeds.len() <= 1 {
        return Ok(run_model(model)?);
    }
    let mut results = Vec::with_capacity(seeds.len());
    for (index, seed) in seeds.iter().enumerate() {
        let config = SimConfig::from_run_settings(&model.run, derive_seed(*seed, 0));
        results.push(simulate(model, &config, index as u32)?);
    }
    let mut report = SimReport::from_replications(model, results);
    report.base_seed = seeds[0];
    Ok(report)
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
                overrides: Vec::new(),
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
            let report = execute(&scenario_model, &[])?;
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
            note(quiet, &format!("wrote {}", join_paths(&files)));
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
            note(quiet, &format!("wrote {}", path.display()));
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
    println!("created {}", path.display());
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

fn describe_kind(kind: ObjectKind) -> (&'static str, &'static str) {
    match kind {
        ObjectKind::Source => (
            "Creates flow items and releases them to the next object.",
            "interarrival: distribution (default: constant 0)\nitems_per_arrival: integer, default 1\nmax_arrivals: optional integer\nfirst_arrival: time of the first arrival, default 0\nstop_time: optional time after which no items are created\nitem_type: integer label for new items, default 0\nlabels: map of label name to number\ntriggers: on_create, on_entry, on_exit, on_receive, on_message",
        ),
        ObjectKind::Queue => (
            "Stores flow items until the next object can take them.",
            "max_content: capacity, default 1000\norder: fifo | lifo | priority | by_label(<name>)\nbatch_size: optional integer (release items in batches)\nbatch_max_wait: time, default 0 (wait forever)",
        ),
        ObjectKind::Processor => (
            "Processes one or more flow items with setup and process times.",
            "setup_time: distribution, default constant 0\nprocess_time: distribution, default constant 1\nmax_content: capacity including the item in process, default 1\nchangeover_time: distribution, default constant 0\nfailure: { mtbf, mttr } optional failure/repair cycle\nsetup_operator / process_operator: dispatcher names (reserved)",
        ),
        ObjectKind::Sink => (
            "Removes finished flow items and counts them.",
            "No parameters.",
        ),
        ObjectKind::Combiner => (
            "Combines components into one container or joins items together.",
            "components_per_container: integer, default 2\nmode: pack | join",
        ),
        ObjectKind::Separator => (
            "Splits a container or a batch into parts.",
            "split_count: integer, 0 = split into all parts",
        ),
        ObjectKind::Conveyor => (
            "Moves items along a belt with a fixed speed.",
            "speed: distance per time unit, default 1\nlength: default 10\naccumulating: boolean, default false\nitem_spacing: default 1\nmax_content: default 1000",
        ),
        ObjectKind::Merge => (
            "Merges several input flows into one output flow.",
            "wait_for_all: boolean, default false",
        ),
        ObjectKind::Split => (
            "Splits one input flow over several outputs.",
            "by_item_type: boolean, default false (round robin otherwise)",
        ),
        ObjectKind::Transporter => (
            "Carries flow items between objects (task executer).",
            "capacity: integer, default 1\nspeed: distance per time unit, default 1\nload_time / unload_time: distributions\nhome: optional object name",
        ),
        ObjectKind::Dispatcher => (
            "Hands out transport tasks to transporters.",
            "max_tasks: integer, 0 = unlimited",
        ),
        ObjectKind::Resource => (
            "A pool of operators or equipment that can be reserved.",
            "capacity: integer, default 1\ndowntimes: list of { start, end } windows",
        ),
        ObjectKind::Rack => (
            "Stores items in levels (a specialised queue).",
            "capacity: integer, default 100\nlevels: integer, default 1",
        ),
        ObjectKind::BasicFr => ("A generic fixed resource.", "No parameters."),
        ObjectKind::BasicTe => (
            "A generic task executer.",
            "speed: distance per time unit, default 1\ncapacity: integer, default 1",
        ),
    }
}

fn print_distributions() {
    let rows = [
        ("constant", "value"),
        ("uniform", "min, max"),
        ("normal", "mean, stddev"),
        ("exponential", "mean"),
        ("triangular", "min, mode, max"),
        ("gamma", "shape, scale"),
        ("weibull", "shape, scale"),
        ("lognormal", "meanlog, sdlog"),
        ("erlang", "shape, scale"),
        ("beta", "alpha, beta"),
        ("integer_uniform", "min, max"),
        ("empirical", "values, weights (optional)"),
    ];
    println!("{:<16} parameters", "distribution");
    for (name, parameters) in rows {
        println!("{name:<16} {parameters}");
    }
    println!("\nJSON form: {{\"distribution\": \"exponential\", \"mean\": 5.0}}");
}
