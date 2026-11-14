//! Structured reports for the experiments declared in a model.
//!
//! `openflex experiment` used to print one text block per scenario. It now
//! builds an [`ExperimentReport`] first: that structure is what `--format json`
//! emits, what `--report-dir` archives as `experiment.json`, and what
//! `openflex report --experiment <dir>` reads back.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use openflex_core::SimReport;
use openflex_model::{ObjectKind, Override, TimeUnit};
use serde::{Deserialize, Serialize};

use crate::{
    ReportError, fmt_ci, fmt_count, fmt_percent, fmt_rate, fmt_time, i18n, sim, write_file,
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
    let _ = writeln!(out);
    let _ = writeln!(
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
    let _ = writeln!(out, "{}", "-".repeat(87));
    for scenario in &experiment.scenarios {
        let summary = summarize(scenario);
        let _ = writeln!(
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
    let _ = writeln!(
        out,
        "experiment,scenario,replications,created,created_ci95,completed,completed_ci95,throughput,avg_wip,avg_utilization"
    );
    for scenario in &experiment.scenarios {
        let summary = summarize(scenario);
        let _ = writeln!(
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

/// Quote a CSV field that contains a comma, a quote or a newline.
fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
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
