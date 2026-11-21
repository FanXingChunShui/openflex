//! Structured reports for the experiments declared in a model.
//!
//! `openflex experiment` used to print one text block per scenario. It now
//! builds an [`ExperimentReport`] first: that structure is what `--format json`
//! emits, what `--report-dir` archives as `experiment.json`, and what
//! `openflex report --experiment <dir>` reads back.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use openflex_core::engine::{fire_model_hook_actions, hook_logs};
use openflex_core::{
    SimConfig, SimError, SimReport, run_model, run_model_parallel, simulate_traced,
};
use openflex_model::{Action, Experiment, Model, ObjectKind, Override, Scenario, TimeUnit};
use serde::{Deserialize, Serialize};

use crate::{
    ReportError, csv_field, fmt_ci, fmt_ci_precise, fmt_count, fmt_percent, fmt_rate, fmt_time,
    i18n, measure_columns, sim, write_file,
};

pub const EXPERIMENT_FILE: &str = "experiment.json";

/// One experiment: a name and the scenarios that were run for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExperimentReport {
    pub experiment: String,
    /// Name of the model the experiment belongs to.
    pub model: String,
    pub time_unit: TimeUnit,
    pub scenarios: Vec<ScenarioReport>,
}

/// One scenario: its parameter overrides plus the full simulation report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioReport {
    pub name: String,
    /// Parameter replacements applied before the run (empty for a baseline).
    #[serde(default)]
    pub overrides: Vec<Override>,
    pub report: SimReport,
}

impl ExperimentReport {
    /// Wrap a single report as a one-scenario experiment. Used when
    /// `openflex report --experiment` is pointed at a plain report directory.
    pub fn single(report: SimReport) -> Self {
        Self {
            experiment: "report".to_string(),
            model: report.model.clone(),
            time_unit: report.time_unit,
            scenarios: vec![ScenarioReport {
                name: "baseline".to_string(),
                overrides: Vec::new(),
                report,
            }],
        }
    }

    /// Number of replications per scenario (the first scenario's count).
    pub fn replications(&self) -> u32 {
        self.scenarios
            .first()
            .map(|scenario| scenario.report.replications.len().max(1) as u32)
            .unwrap_or(0)
    }

    /// Total number of runs archived by this experiment.
    pub fn runs(&self) -> usize {
        self.scenarios
            .iter()
            .map(|scenario| scenario.report.replications.len().max(1))
            .sum()
    }
}

/// Read `experiment.json` from an explicit file path.
pub fn read_experiment_file(path: &Path) -> Result<ExperimentReport, ReportError> {
    let text = std::fs::read_to_string(path).map_err(|source| ReportError::Read {
        path: path.display().to_string(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| ReportError::Parse {
        path: path.display().to_string(),
        source,
    })
}

pub fn to_json(experiment: &ExperimentReport) -> Result<String, ReportError> {
    let mut text = serde_json::to_string_pretty(experiment)?;
    text.push('\n');
    Ok(text)
}

/// Summary metrics of one scenario, computed from its simulation report.
struct Summary {
    name: String,
    replications: u32,
    created: f64,
    created_ci: f64,
    completed: f64,
    completed_ci: f64,
    throughput: f64,
    avg_wip: f64,
    utilization: f64,
}

fn summarize(scenario: &ScenarioReport) -> Summary {
    let report = &scenario.report;
    let duration = (report.stop_time - report.warmup).max(f64::EPSILON);
    let avg_wip = report
        .objects
        .values()
        .map(|stats| stats.avg_content.mean)
        .sum::<f64>();
    let utilization = average_utilization(report);
    Summary {
        name: sanitize(&scenario.name),
        replications: report.replications.len().max(1) as u32,
        created: report.created.mean,
        created_ci: report.created.ci95,
        completed: report.completed.mean,
        completed_ci: report.completed.ci95,
        throughput: report.completed.mean / duration,
        avg_wip,
        utilization,
    }
}

/// Utilisation of the processors, or of every object when the model has no
/// processor. This keeps the scenario summary meaningful for simple models.
fn average_utilization(report: &SimReport) -> f64 {
    let processors: Vec<f64> = report
        .objects
        .values()
        .filter(|stats| stats.kind == ObjectKind::Processor)
        .map(|stats| stats.utilization.mean)
        .collect();
    let values = if processors.is_empty() {
        report
            .objects
            .values()
            .map(|stats| stats.utilization.mean)
            .collect::<Vec<f64>>()
    } else {
        processors
    };
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// Escape a measure or scenario name for a Markdown table cell.
fn markdown_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

/// Cell of the experiment table for one measure in one scenario.
///
/// Shows the mean to three decimals; the 95% interval is appended only when
/// the measure has at least two samples, so a single replication never prints
/// a meaningless `±0.000`.
fn measure_cell(aggregate: Option<&openflex_core::Aggregate>) -> String {
    match aggregate {
        // `mean ±ci` with the interval omitted when there is only one sample,
        // so the cell reads the same way whether or not the interval exists.
        Some(aggregate) if aggregate.n >= 2 => {
            format!("{:.3} {}", aggregate.mean, fmt_ci_precise(aggregate))
        }
        Some(aggregate) => format!("{:.3} {}", aggregate.mean, crate::NO_INTERVAL),
        None => crate::NO_INTERVAL.to_string(),
    }
}

/// Display width of every measure column: wide enough for the header and for
/// the widest cell, so the table stays aligned without truncating names.
fn measure_widths(experiment: &ExperimentReport, measures: &[String]) -> Vec<usize> {
    measures
        .iter()
        .map(|name| {
            let widest = experiment
                .scenarios
                .iter()
                .map(|scenario| {
                    crate::display_width(&measure_cell(
                        scenario.report.performance_measures.get(name),
                    ))
                })
                .max()
                .unwrap_or(0);
            crate::display_width(name).max(widest).max(8)
        })
        .collect()
}

fn header(experiment: &ExperimentReport) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}{}{}",
        i18n::t("experiment_title"),
        i18n::t("title_separator"),
        experiment.experiment
    );
    let _ = writeln!(
        out,
        "{}",
        i18n::experiment_table_summary(
            &experiment.model,
            experiment.scenarios.len(),
            experiment.replications()
        )
    );
    out
}

/// Terminal table: one summary row per scenario, followed by the full table of
/// every scenario so nothing that `openflex run` would show is lost.
pub fn to_table(experiment: &ExperimentReport) -> String {
    to_table_with_sections(experiment, false, false)
}

/// Terminal table with optional flow/fluid sections in every scenario table.
pub fn to_table_with_sections(
    experiment: &ExperimentReport,
    include_flow: bool,
    include_fluid: bool,
) -> String {
    let mut out = header(experiment);
    let measures = measure_columns(experiment.scenarios.iter().map(|s| &s.report));
    let measure_widths = measure_widths(experiment, &measures);
    let _ = writeln!(out);
    let _ = write!(
        out,
        "{:<18} {:>5} {:>10} {:>8} {:>11} {:>8} {:>11} {:>8} {:>8}",
        i18n::t("scenario"),
        i18n::t("reps"),
        i18n::t("created_count"),
        "±95%",
        i18n::t("completed_count"),
        "±95%",
        i18n::t("throughput"),
        i18n::t("avg_wip"),
        i18n::t("util_short")
    );
    for (name, width) in measures.iter().zip(&measure_widths) {
        let _ = write!(out, " {}", crate::pad_left(name, *width));
    }
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{}",
        "-".repeat(
            87 + measures
                .iter()
                .zip(&measure_widths)
                .map(|(_, w)| w + 1)
                .sum::<usize>()
        )
    );
    for scenario in &experiment.scenarios {
        let summary = summarize(scenario);
        let _ = write!(
            out,
            "{:<18} {:>5} {:>10} {:>8} {:>11} {:>8} {:>11} {:>8} {:>8}",
            summary.name,
            summary.replications,
            fmt_count(summary.created),
            fmt_ci(summary.created_ci).trim_start_matches('±'),
            fmt_count(summary.completed),
            fmt_ci(summary.completed_ci).trim_start_matches('±'),
            fmt_rate(summary.throughput),
            fmt_time(summary.avg_wip),
            fmt_percent(summary.utilization)
        );
        for (name, width) in measures.iter().zip(&measure_widths) {
            let cell = measure_cell(scenario.report.performance_measures.get(name));
            let _ = write!(out, " {}", crate::pad_left(&cell, *width));
        }
        let _ = writeln!(out);
    }
    for scenario in &experiment.scenarios {
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "-- {}{}{} --",
            i18n::t("scenario"),
            i18n::t("title_separator"),
            sanitize(&scenario.name)
        );
        if !scenario.overrides.is_empty() {
            for over in &scenario.overrides {
                let _ = writeln!(
                    out,
                    "   {} {}.{} = {}",
                    i18n::t("override"),
                    over.object,
                    over.param,
                    serde_json::to_string(&over.value).unwrap_or_else(|_| "?".into())
                );
            }
        }
        let _ = writeln!(
            out,
            "{}",
            sim::to_table_with_sections(&scenario.report, include_flow, include_fluid)
        );
    }
    out
}

pub fn to_csv(experiment: &ExperimentReport) -> String {
    to_csv_with_sections(experiment, false, false)
}

/// CSV with optional per-scenario flow/fluid sections after the summary table.
pub fn to_csv_with_sections(
    experiment: &ExperimentReport,
    include_flow: bool,
    include_fluid: bool,
) -> String {
    let mut out = String::new();
    let measures = measure_columns(experiment.scenarios.iter().map(|s| &s.report));
    let mut header = String::from(
        "experiment,scenario,replications,created,created_ci95,completed,completed_ci95,throughput,avg_wip,avg_utilization",
    );
    for name in &measures {
        // Measure names are user data: quote them so a comma cannot shift the
        // columns, and suffix `_ci95` for the interval column.
        let _ = write!(header, ",{},{}_ci95", csv_field(name), csv_field(name));
    }
    if !measures.is_empty() {
        // Head comment: a spreadsheets/mail-merge reader knows why some
        // interval cells are empty, and the header still names every column.
        let _ = writeln!(
            out,
            "# performance measures: one column pair per measure (mean and 95% CI); ci95 is empty when n < 2"
        );
    }
    let _ = writeln!(out, "{header}");
    for scenario in &experiment.scenarios {
        let summary = summarize(scenario);
        let _ = write!(
            out,
            "{},{},{},{:.3},{:.3},{:.3},{:.3},{:.6},{:.4},{:.4}",
            csv_field(&experiment.experiment),
            csv_field(&scenario.name),
            summary.replications,
            summary.created,
            summary.created_ci,
            summary.completed,
            summary.completed_ci,
            summary.throughput,
            summary.avg_wip,
            summary.utilization
        );
        for name in &measures {
            match scenario.report.performance_measures.get(name) {
                Some(aggregate) if aggregate.n >= 2 => {
                    let _ = write!(out, ",{:.6},{:.6}", aggregate.mean, aggregate.ci95);
                }
                Some(aggregate) => {
                    let _ = write!(out, ",{:.6},", aggregate.mean);
                }
                None => {
                    let _ = write!(out, ",,");
                }
            }
        }
        let _ = writeln!(out);
    }
    if include_flow || include_fluid {
        for scenario in &experiment.scenarios {
            let _ = writeln!(out, "\n# scenario: {}", scenario.name);
            if include_flow {
                out.push_str(&sim::flow_csv(&scenario.report));
            }
            if include_fluid {
                out.push_str(&sim::fluid_csv(&scenario.report));
            }
        }
    }
    out
}

pub fn to_markdown(experiment: &ExperimentReport) -> String {
    to_markdown_with_sections(experiment, false, false)
}

/// Markdown with optional per-scenario flow/fluid sections.
pub fn to_markdown_with_sections(
    experiment: &ExperimentReport,
    include_flow: bool,
    include_fluid: bool,
) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# {}{}{}",
        i18n::t("experiment_title"),
        i18n::t("title_separator"),
        experiment.experiment
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{}\n",
        i18n::experiment_markdown_summary(
            &experiment.model,
            experiment.scenarios.len(),
            experiment.replications(),
            experiment.runs()
        )
    );
    let _ = writeln!(out, "## {}\n", i18n::t("scenarios"));
    let _ = writeln!(
        out,
        "| {} | {} | {} | {} | {} | {} | {} |",
        i18n::t("scenario"),
        i18n::t("replications_label"),
        i18n::t("items_created"),
        i18n::t("items_completed"),
        i18n::t("throughput"),
        i18n::t("avg_wip_md"),
        i18n::t("avg_utilization_md")
    );
    let _ = writeln!(out, "|---|---:|---:|---:|---:|---:|---:|");
    for scenario in &experiment.scenarios {
        let summary = summarize(scenario);
        let _ = writeln!(
            out,
            "| {} | {} | {} {} | {} {} | {} | {} | {} |",
            summary.name,
            summary.replications,
            fmt_count(summary.created),
            fmt_ci(summary.created_ci),
            fmt_count(summary.completed),
            fmt_ci(summary.completed_ci),
            fmt_rate(summary.throughput),
            fmt_time(summary.avg_wip),
            fmt_percent(summary.utilization)
        );
    }
    let measures = measure_columns(experiment.scenarios.iter().map(|s| &s.report));
    if !measures.is_empty() {
        let _ = writeln!(out, "\n## {}\n", i18n::t("performance_measures"));
        let _ = write!(
            out,
            "| {} | {}",
            i18n::t("scenario"),
            i18n::t("replications_label")
        );
        for name in &measures {
            let _ = write!(out, " | {} | {}", markdown_cell(name), i18n::t("interval"));
        }
        let _ = writeln!(out, " |");
        let _ = write!(out, "|---|---:");
        for _ in &measures {
            let _ = write!(out, "|---:|---:");
        }
        let _ = writeln!(out, "|");
        for scenario in &experiment.scenarios {
            let replications = scenario.report.replications.len().max(1);
            let _ = write!(
                out,
                "| {} | {}",
                markdown_cell(&scenario.name),
                replications
            );
            for name in &measures {
                let _ = match scenario.report.performance_measures.get(name) {
                    Some(aggregate) if aggregate.n >= 2 => write!(
                        out,
                        " | {:.3} | {}",
                        aggregate.mean,
                        fmt_ci_precise(aggregate)
                    ),
                    Some(aggregate) => {
                        write!(out, " | {:.3} | {}", aggregate.mean, crate::NO_INTERVAL)
                    }
                    None => write!(out, " | {} | {}", crate::NO_INTERVAL, crate::NO_INTERVAL),
                };
            }
            let _ = writeln!(out, " |");
        }
    }
    let _ = writeln!(out, "\n## {}\n", i18n::t("how_to_reproduce"));
    let _ = writeln!(out, "```sh");
    let _ = writeln!(out, "openflex experiment <model file> --report-dir reports");
    let _ = writeln!(
        out,
        "openflex report --experiment reports --format markdown"
    );
    let _ = writeln!(out, "```");
    if include_flow || include_fluid {
        for scenario in &experiment.scenarios {
            let _ = writeln!(out, "\n### {}", scenario.name);
            if include_flow {
                out.push_str(&sim::flow_markdown(&scenario.report));
            }
            if include_fluid {
                out.push_str(&sim::fluid_markdown(&scenario.report));
            }
        }
    }
    out
}

/// Write `experiment.json` plus one report directory per scenario.
///
/// Layout: `<root>/<experiment>/experiment.json` and
/// `<root>/<experiment>/<scenario>/report.{json,csv,md}`. The `experiment.json`
/// file is the archive that `openflex report --experiment` reads back.
pub fn write_experiment_reports(
    experiment: &ExperimentReport,
    root: &Path,
) -> Result<Vec<PathBuf>, ReportError> {
    write_experiment_reports_with_sections(experiment, root, false, false)
}

/// Like [`write_experiment_reports`], optionally including flow/fluid sections
/// in every archived scenario report.
pub fn write_experiment_reports_with_sections(
    experiment: &ExperimentReport,
    root: &Path,
    include_flow: bool,
    include_fluid: bool,
) -> Result<Vec<PathBuf>, ReportError> {
    let mut written = Vec::new();
    let base = root.join(sanitize(&experiment.experiment));

    let summary_path = base.join(EXPERIMENT_FILE);
    write_file(&summary_path, &to_json(experiment)?)?;
    written.push(summary_path);

    for scenario in &experiment.scenarios {
        let directory = base.join(sanitize(&scenario.name));
        written.extend(sim::write_reports_with_sections(
            &scenario.report,
            &directory,
            include_flow,
            include_fluid,
        )?);
    }
    Ok(written)
}

/// Make a scenario or experiment name safe for use as a directory name.
pub fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "unnamed".to_string()
    } else {
        cleaned
    }
}

// ---------------------------------------------------------------------------
// Running the experiments declared in a model
// ---------------------------------------------------------------------------
//
// The CLI used to own this loop. Moving it here (next to the renderers that
// consume its output) lets any host — the desktop GUI, a notebook, a CI job —
// run the scenarios a model declares and get the full reports back, without
// re-implementing override application, replication counts or lifecycle hooks.

/// How a scenario failed, so a caller can map it to its own severity (the CLI
/// turns `Usage` into exit code 2 and `Runtime` into exit code 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioErrorKind {
    /// The model or the scenario definition is wrong: a bad override, a bad
    /// stop time, an invalid parameter.
    Usage,
    /// The simulation ran and failed.
    Runtime,
}

/// One scenario of one declared experiment, with the report it produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioOutcome {
    /// Name of the experiment that declares this scenario.
    pub experiment: String,
    /// Scenario name (`baseline` when the experiment declares none).
    pub scenario: String,
    /// Parameter replacements that were applied before the run.
    #[serde(default)]
    pub overrides: Vec<Override>,
    /// The complete report: objects, aggregates, measures, tracked series, …
    /// `None` when the scenario failed.
    pub report: Option<SimReport>,
    /// Failure text, using the same wording the CLI reports.
    pub error: Option<String>,
    /// Classification of `error`; `None` when the scenario succeeded.
    pub error_kind: Option<ScenarioErrorKind>,
    /// Trace files written for this scenario (empty unless tracing).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace_files: Vec<PathBuf>,
    /// `log` lines emitted by the model lifecycle hooks around this scenario.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub logs: Vec<String>,
}

impl ScenarioOutcome {
    /// Whether the scenario produced a report.
    pub fn succeeded(&self) -> bool {
        self.report.is_some()
    }

    /// The report, if the scenario succeeded.
    pub fn report(&self) -> Option<&SimReport> {
        self.report.as_ref()
    }
}

/// Options for [`run_declared_experiments_with_options`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExperimentOptions {
    /// Workers for the replications of each scenario; `None` runs serially.
    ///
    /// `Some(0)` means "use the available parallelism", matching the engine.
    pub parallel: Option<usize>,
    /// Only run the experiment with this name.
    pub experiment: Option<String>,
    /// Write one JSONL trace per replication under
    /// `<root>/<experiment>/<scenario>/`.
    pub trace_root: Option<PathBuf>,
}

/// Run every experiment declared by `model`, one outcome per scenario, in
/// declaration order (experiments as declared, then their scenarios).
///
/// `parallel` spreads the replications of each scenario over the available
/// cores; the reports are bit-for-bit identical to a serial run.
pub fn run_declared_experiments(model: &Model, parallel: bool) -> Vec<ScenarioOutcome> {
    let options = ExperimentOptions {
        parallel: if parallel {
            // 0 = "available parallelism", the engine's own convention.
            Some(0)
        } else {
            None
        },
        ..ExperimentOptions::default()
    };
    run_declared_experiments_with_options(model, &options)
}

/// [`run_declared_experiments`] with explicit options.
///
/// A scenario that fails does not stop the experiment: its outcome carries the
/// error and the remaining scenarios still run, which is what a GUI needs to
/// show "3 of 4 scenarios ran".
pub fn run_declared_experiments_with_options(
    model: &Model,
    options: &ExperimentOptions,
) -> Vec<ScenarioOutcome> {
    let selected: Vec<&Experiment> = model
        .experiments
        .iter()
        .filter(|experiment| {
            options
                .experiment
                .as_deref()
                .is_none_or(|wanted| experiment.name == wanted)
        })
        .collect();

    let mut outcomes = Vec::new();
    for experiment in selected {
        let scenarios = scenarios_of(experiment);
        let experiment_config = SimConfig::from_run_settings(&model.run, model.run.seed);
        let mut experiment_logs = Vec::new();
        if let Err(error) = fire_hook(
            model,
            &experiment_config,
            model.model_triggers.start_of_experiment.clone(),
            "model_triggers.start_of_experiment",
            &mut experiment_logs,
        ) {
            // The experiment cannot start; every scenario is reported as
            // failed with the same reason instead of silently disappearing.
            for scenario in &scenarios {
                outcomes.push(failed_outcome(
                    experiment,
                    scenario,
                    ScenarioErrorKind::Runtime,
                    error.to_string(),
                    Vec::new(),
                    experiment_logs.clone(),
                ));
            }
            continue;
        }

        for scenario in &scenarios {
            let scenario_model = match prepare_scenario(model, experiment, scenario) {
                Ok(model) => model,
                Err(error) => {
                    outcomes.push(failed_outcome(
                        experiment,
                        scenario,
                        ScenarioErrorKind::Usage,
                        format!(
                            "experiment {} scenario {}: {error}",
                            experiment.name, scenario.name
                        ),
                        Vec::new(),
                        experiment_logs.clone(),
                    ));
                    continue;
                }
            };

            let scenario_config =
                SimConfig::from_run_settings(&scenario_model.run, scenario_model.run.seed);
            let mut logs = experiment_logs.clone();
            if let Err(error) = fire_hook(
                &scenario_model,
                &scenario_config,
                scenario_model.model_triggers.start_of_scenario.clone(),
                "model_triggers.start_of_scenario",
                &mut logs,
            ) {
                outcomes.push(failed_outcome(
                    experiment,
                    scenario,
                    ScenarioErrorKind::Runtime,
                    error.to_string(),
                    Vec::new(),
                    logs,
                ));
                continue;
            }

            let trace_dir = options.trace_root.as_ref().map(|root| {
                root.join(sanitize(&experiment.name))
                    .join(sanitize(&scenario.name))
            });
            let (report, trace_files) =
                match run_scenario(&scenario_model, options, trace_dir.as_deref()) {
                    Ok(result) => result,
                    Err((kind, message)) => {
                        outcomes.push(failed_outcome(
                            experiment,
                            scenario,
                            kind,
                            message,
                            Vec::new(),
                            logs,
                        ));
                        continue;
                    }
                };

            if let Err(error) = fire_hook(
                &scenario_model,
                &scenario_config,
                scenario_model.model_triggers.end_of_scenario.clone(),
                "model_triggers.end_of_scenario",
                &mut logs,
            ) {
                // The run itself succeeded; keep the report and surface the
                // end-of-scenario hook failure as the scenario's error.
                outcomes.push(ScenarioOutcome {
                    experiment: experiment.name.clone(),
                    scenario: scenario.name.clone(),
                    overrides: scenario.overrides.clone(),
                    report: Some(report),
                    error: Some(error.to_string()),
                    error_kind: Some(ScenarioErrorKind::Runtime),
                    trace_files,
                    logs,
                });
                continue;
            }

            outcomes.push(ScenarioOutcome {
                experiment: experiment.name.clone(),
                scenario: scenario.name.clone(),
                overrides: scenario.overrides.clone(),
                report: Some(report),
                error: None,
                error_kind: None,
                trace_files,
                logs,
            });
        }

        let mut end_logs = experiment_logs;
        if fire_hook(
            model,
            &experiment_config,
            model.model_triggers.end_of_experiment.clone(),
            "model_triggers.end_of_experiment",
            &mut end_logs,
        )
        .is_ok()
        {
            // Attach the end-of-experiment log lines to the experiment's last
            // outcome so a host does not lose them.
            if let Some(last) = outcomes
                .last_mut()
                .filter(|outcome| outcome.experiment == experiment.name)
            {
                last.logs = end_logs;
            }
        }
    }
    outcomes
}

/// Group outcomes into the per-experiment reports the renderers consume.
///
/// Only successful scenarios appear; a host that wants to show failures reads
/// [`ScenarioOutcome::error`] from the outcomes themselves.
pub fn experiment_reports(outcomes: &[ScenarioOutcome], model: &Model) -> Vec<ExperimentReport> {
    let mut reports: Vec<ExperimentReport> = Vec::new();
    for outcome in outcomes {
        let Some(report) = &outcome.report else {
            continue;
        };
        let index = match reports
            .iter()
            .position(|existing| existing.experiment == outcome.experiment)
        {
            Some(index) => index,
            None => {
                reports.push(ExperimentReport {
                    experiment: outcome.experiment.clone(),
                    model: model.name.clone(),
                    time_unit: model.time_unit,
                    scenarios: Vec::new(),
                });
                reports.len() - 1
            }
        };
        reports[index].scenarios.push(ScenarioReport {
            name: outcome.scenario.clone(),
            overrides: outcome.overrides.clone(),
            report: report.clone(),
        });
    }
    reports
}

/// Scenarios of an experiment, defaulting to a single `baseline`.
fn scenarios_of(experiment: &Experiment) -> Vec<Scenario> {
    if experiment.scenarios.is_empty() {
        vec![Scenario {
            name: "baseline".into(),
            ..Scenario::default()
        }]
    } else {
        experiment.scenarios.clone()
    }
}

/// Apply the scenario's overrides and run settings to a copy of the model.
fn prepare_scenario(
    model: &Model,
    experiment: &Experiment,
    scenario: &Scenario,
) -> Result<Model, openflex_model::ModelError> {
    let mut scenario_model = model.clone();
    for over in &scenario.overrides {
        scenario_model =
            scenario_model.with_override(&over.object, &over.param, over.value.clone())?;
    }
    scenario_model.run.replications = experiment.replications;
    if let Some(stop_time) = scenario.stop_time {
        scenario_model.run.stop_time = stop_time;
    }
    if let Some(warmup) = scenario.warmup {
        scenario_model.run.warmup = warmup;
    }
    Ok(scenario_model)
}

/// Run one scenario, optionally writing a trace per replication.
fn run_scenario(
    model: &Model,
    options: &ExperimentOptions,
    trace_dir: Option<&Path>,
) -> Result<(SimReport, Vec<PathBuf>), (ScenarioErrorKind, String)> {
    match trace_dir {
        Some(directory) => {
            std::fs::create_dir_all(directory).map_err(|error| {
                (
                    ScenarioErrorKind::Runtime,
                    format!("could not create {}: {error}", directory.display()),
                )
            })?;
            let count = model.run.replications.max(1);
            let several = count > 1;
            let mut results = Vec::with_capacity(count as usize);
            let mut files = Vec::new();
            for index in 0..count {
                let seed = openflex_core::derive_seed(model.run.seed, index);
                let config = SimConfig::from_run_settings(&model.run, seed);
                let (result, log) = simulate_traced(model, &config, index)
                    .map_err(|error| (sim_error_kind(&error), error.to_string()))?;
                let file = if several {
                    directory.join(format!("trace.rep{index}.jsonl"))
                } else {
                    directory.join("trace.jsonl")
                };
                crate::write_trace_jsonl(&file, &log)
                    .map_err(|error| (ScenarioErrorKind::Runtime, error.to_string()))?;
                files.push(file);
                results.push(result);
            }
            Ok((SimReport::from_replications(model, results), files))
        }
        None => {
            let report = match options.parallel {
                Some(workers) => run_model_parallel(model, workers),
                None => run_model(model),
            }
            .map_err(|error| (sim_error_kind(&error), error.to_string()))?;
            Ok((report, Vec::new()))
        }
    }
}

/// Map an engine error onto the CLI/UI severity it deserves.
fn sim_error_kind(error: &SimError) -> ScenarioErrorKind {
    match error {
        SimError::Config(_) | SimError::Model(_) => ScenarioErrorKind::Usage,
        _ => ScenarioErrorKind::Runtime,
    }
}

fn fire_hook(
    model: &Model,
    config: &SimConfig,
    actions: Vec<Action>,
    location: &str,
    logs: &mut Vec<String>,
) -> Result<(), SimError> {
    let trace = fire_model_hook_actions(model, config, actions, location)?;
    logs.extend(hook_logs(&trace));
    Ok(())
}

fn failed_outcome(
    experiment: &Experiment,
    scenario: &Scenario,
    kind: ScenarioErrorKind,
    error: String,
    trace_files: Vec<PathBuf>,
    logs: Vec<String>,
) -> ScenarioOutcome {
    ScenarioOutcome {
        experiment: experiment.name.clone(),
        scenario: scenario.name.clone(),
        overrides: scenario.overrides.clone(),
        report: None,
        error: Some(error),
        error_kind: Some(kind),
        trace_files,
        logs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::sample_report;

    fn sample_experiment() -> ExperimentReport {
        let baseline = sample_report();
        let mut faster = sample_report();
        faster
            .objects
            .get_mut("Queue1")
            .expect("Queue1")
            .avg_content
            .mean = 1.0;
        ExperimentReport {
            experiment: "tuning".into(),
            model: "demo".into(),
            time_unit: TimeUnit::Seconds,
            scenarios: vec![
                ScenarioReport {
                    name: "baseline".into(),
                    overrides: Vec::new(),
                    report: baseline,
                },
                ScenarioReport {
                    name: "faster".into(),
                    overrides: Vec::new(),
                    report: faster,
                },
            ],
        }
    }

    #[test]
    fn renders_experiment_in_all_formats() {
        let experiment = sample_experiment();
        for format in [
            crate::Format::Table,
            crate::Format::Json,
            crate::Format::Csv,
            crate::Format::Markdown,
        ] {
            let text = crate::render_experiment(&experiment, format).expect("render");
            assert!(
                text.contains("baseline"),
                "{format:?} lost the scenario name"
            );
            assert!(text.ends_with('\n'));
        }
        let json = to_json(&experiment).expect("json");
        let parsed: ExperimentReport = serde_json::from_str(&json).expect("round trip");
        assert_eq!(parsed, experiment);
    }

    #[test]
    fn sanitizes_directory_names() {
        assert_eq!(sanitize("baseline"), "baseline");
        assert_eq!(sanitize("a/b c"), "a_b_c");
        assert_eq!(sanitize(""), "unnamed");
    }

    #[test]
    fn csv_quotes_awkward_names() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
    }
}
