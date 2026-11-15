//! Rendering of simulation results.
//!
//! The engine produces structured results (`openflex_core::SimReport`); this
//! crate turns them into the formats the CLI offers: a terminal table, JSON,
//! CSV and Markdown. The same renderers are used by `openflex run`,
//! `openflex experiment`, `openflex report`, `openflex compare` and the report
//! files written with `--report-dir`.
//!
//! Three result shapes are supported:
//!
//! - [`SimReport`]: one model run, possibly with several replications.
//! - [`ExperimentReport`]: the scenarios of one experiment, each with its own
//!   [`SimReport`].
//! - [`Comparison`]: two [`SimReport`]s compared object by object.
//!
//! Every renderer returns text with a trailing newline so the CLI can write it
//! to a file or to standard output unchanged.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use openflex_core::{Aggregate, SimReport};

mod batch;
mod compare;
mod experiment;
pub mod i18n;
mod sim;
mod xlsx;

pub use batch::{
    BatchMeans, BatchMeansSummary, ObjectBatchMeans, batch_means, batch_means_with_objects,
};
pub use compare::{Comparison, MetricComparison, ObjectComparison, ReportRef, compare_reports};
pub use experiment::{
    ExperimentReport, ScenarioReport, sanitize, write_experiment_reports,
    write_experiment_reports_with_sections,
};
pub use sim::{
    REPLICATIONS_CSV, REPORT_CSV, REPORT_JSON, REPORT_MARKDOWN, to_csv, to_csv_with_flow,
    to_csv_with_sections, to_json, to_json_with_batch_means, to_markdown,
    to_markdown_with_batch_means, to_markdown_with_flow, to_markdown_with_sections,
    to_replications_csv, to_table, to_table_with_batch_means, to_table_with_flow,
    to_table_with_sections, write_reports, write_reports_with_flow, write_reports_with_options,
    write_reports_with_sections,
};
pub use xlsx::write_xlsx;

/// Output formats offered by the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Table,
    Json,
    Csv,
    Markdown,
}

#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("could not serialise the report: {0}")]
    Json(#[from] serde_json::Error),
    #[error("could not read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid report {path}: {source}")]
    Parse {
        path: String,
        source: serde_json::Error,
    },
    #[error("could not write {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("batch means: {0}")]
    BatchMeans(String),
    #[error("could not build the Excel workbook: {0}")]
    Xlsx(#[from] rust_xlsxwriter::XlsxError),
    #[error("could not write {path}: {source}")]
    XlsxFile {
        path: String,
        source: rust_xlsxwriter::XlsxError,
    },
    #[error(
        "no experiment report found in {path}; expected experiment.json or \
         <experiment>/experiment.json (did you run `openflex experiment --report-dir`?)"
    )]
    MissingExperiment { path: String },
    #[error("several experiment reports found in {path} ({found}); pass one of those directories")]
    AmbiguousExperiment { path: String, found: String },
}

// ---------------------------------------------------------------------------
// Rendering entry points
// ---------------------------------------------------------------------------

/// Render one simulation report in the requested format.
pub fn render_sim(report: &SimReport, format: Format) -> Result<String, ReportError> {
    render_sim_with_sections(report, format, false, false)
}

/// Render one simulation report, optionally with the process-flow section.
///
/// JSON always carries the structured `process_flow` field; `include_flow`
/// only affects the human-readable table, Markdown and CSV renderings.
pub fn render_sim_with_flow(
    report: &SimReport,
    format: Format,
    include_flow: bool,
) -> Result<String, ReportError> {
    render_sim_with_sections(report, format, include_flow, false)
}

/// Render one simulation report with optional process-flow and fluid sections.
pub fn render_sim_with_sections(
    report: &SimReport,
    format: Format,
    include_flow: bool,
    include_fluid: bool,
) -> Result<String, ReportError> {
    render_sim_with_options(report, format, include_flow, include_fluid, None, false)
}

/// Render one simulation report with all optional sections.
///
/// `batch_batches` requests a batch-means summary with that many batches; the
/// replication count must be at least as large.
pub fn render_sim_with_options(
    report: &SimReport,
    format: Format,
    include_flow: bool,
    include_fluid: bool,
    batch_batches: Option<u32>,
    batch_objects: bool,
) -> Result<String, ReportError> {
    let batch = match batch_batches {
        Some(batches) => Some(crate::batch::batch_means_with_objects(
            report,
            batches,
            batch_objects,
        )?),
        None => None,
    };
    Ok(match format {
        Format::Table => {
            to_table_with_batch_means(report, include_flow, include_fluid, batch.as_ref())
        }
        Format::Json => to_json_with_batch_means(report, batch.as_ref())?,
        Format::Csv => to_csv_with_sections(report, include_flow, include_fluid),
        Format::Markdown => {
            to_markdown_with_batch_means(report, include_flow, include_fluid, batch.as_ref())
        }
    })
}

/// Render a whole experiment (all scenarios) in the requested format.
pub fn render_experiment(report: &ExperimentReport, format: Format) -> Result<String, ReportError> {
    match format {
        Format::Table => Ok(experiment::to_table(report)),
        Format::Json => experiment::to_json(report),
        Format::Csv => Ok(experiment::to_csv(report)),
        Format::Markdown => Ok(experiment::to_markdown(report)),
    }
}

/// Render an experiment with optional flow/fluid sections in every scenario.
pub fn render_experiment_with_sections(
    report: &ExperimentReport,
    format: Format,
    include_flow: bool,
    include_fluid: bool,
) -> Result<String, ReportError> {
    match format {
        Format::Table => Ok(experiment::to_table_with_sections(
            report,
            include_flow,
            include_fluid,
        )),
        Format::Json => experiment::to_json(report),
        Format::Csv => Ok(experiment::to_csv_with_sections(
            report,
            include_flow,
            include_fluid,
        )),
        Format::Markdown => Ok(experiment::to_markdown_with_sections(
            report,
            include_flow,
            include_fluid,
        )),
    }
}

/// Render an A/B comparison in the requested format.
pub fn render_comparison(comparison: &Comparison, format: Format) -> Result<String, ReportError> {
    Ok(match format {
        Format::Table => compare::to_table(comparison),
        Format::Json => compare::to_json(comparison)?,
        Format::Csv => compare::to_csv(comparison),
        Format::Markdown => compare::to_markdown(comparison),
    })
}

// ---------------------------------------------------------------------------
// Reading and writing reports
// ---------------------------------------------------------------------------

/// Read a `SimReport` JSON file written by `--report-dir` or `--format json`.
///
/// Parse errors keep the line and column information produced by
/// `serde_json`, which makes broken or stale report files easy to fix.
pub fn read_report(path: &Path) -> Result<SimReport, ReportError> {
    let text = fs::read_to_string(path).map_err(|source| ReportError::Read {
        path: path.display().to_string(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| ReportError::Parse {
        path: path.display().to_string(),
        source,
    })
}

/// Read an experiment directory written by `openflex experiment --report-dir`.
///
/// `path` may point at the `experiment.json` file itself, at the directory that
/// holds it (the `--report-dir` root or one experiment subdirectory), or at a
/// single report directory, which is wrapped as a one-scenario experiment.
pub fn read_experiment(path: &Path) -> Result<ExperimentReport, ReportError> {
    if path.is_file() {
        return experiment::read_experiment_file(path);
    }
    if !path.is_dir() {
        return Err(ReportError::Read {
            path: path.display().to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "no such file or directory"),
        });
    }

    let direct = path.join(experiment::EXPERIMENT_FILE);
    if direct.is_file() {
        return experiment::read_experiment_file(&direct);
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    let entries = fs::read_dir(path).map_err(|source| ReportError::Read {
        path: path.display().to_string(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| ReportError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let candidate = entry.path().join(experiment::EXPERIMENT_FILE);
        if candidate.is_file() {
            candidates.push(candidate);
        }
    }
    candidates.sort();

    match candidates.len() {
        1 => experiment::read_experiment_file(&candidates[0]),
        0 => {
            let single = path.join(REPORT_JSON);
            if single.is_file() {
                let report = read_report(&single)?;
                return Ok(ExperimentReport::single(report));
            }
            Err(ReportError::MissingExperiment {
                path: path.display().to_string(),
            })
        }
        _ => Err(ReportError::AmbiguousExperiment {
            path: path.display().to_string(),
            found: candidates
                .iter()
                .filter_map(|candidate| candidate.parent())
                .map(|directory| directory.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
        }),
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

pub(crate) fn write_file(path: &Path, contents: &str) -> Result<(), ReportError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|source| ReportError::Io {
            path: parent.display().to_string(),
            source,
        })?;
    }
    fs::write(path, contents).map_err(|source| ReportError::Io {
        path: path.display().to_string(),
        source,
    })
}

/// Items still inside the model when the run stopped, aggregated over the
/// replications. Older report files may not carry replications; for those the
/// value is estimated from created minus completed.
pub(crate) fn in_system(report: &SimReport) -> Aggregate {
    if !report.replications.is_empty() {
        let samples: Vec<f64> = report
            .replications
            .iter()
            .map(|replication| replication.items_in_system as f64)
            .collect();
        return Aggregate::from_samples(&samples);
    }
    Aggregate::from_samples(&[(report.created.mean - report.completed.mean).max(0.0)])
}

/// Counts are shown with one decimal so single and multiple replications read
/// the same way.
pub(crate) fn fmt_count(value: f64) -> String {
    format!("{value:.1}")
}

/// Times are always shown with two decimals (`3.20 s`).
pub(crate) fn fmt_time(value: f64) -> String {
    format!("{value:.2}")
}

/// Throughput and other rates use four decimals.
pub(crate) fn fmt_rate(value: f64) -> String {
    format!("{value:.4}")
}

/// Percentages carry one decimal (`62.3%`).
pub(crate) fn fmt_percent(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

/// Half-width of a 95% confidence interval, with the unit baked in.
pub(crate) fn fmt_ci(value: f64) -> String {
    format!("±{value:.1}")
}

/// Human-readable duration used in the "how to reproduce" sections.
pub(crate) fn reproduce_command(report: &SimReport) -> String {
    let mut command = String::new();
    let _ = write!(
        command,
        "openflex run <model file> --stop-time {} --warmup {} --seed {} --replications {}",
        fmt_time(report.stop_time),
        fmt_time(report.warmup),
        report.base_seed,
        report.replications.len().max(1)
    );
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use openflex_core::Aggregate;
    use openflex_model::{ObjectKind, TimeUnit};
    use std::collections::BTreeMap;

    pub(crate) fn sample_report() -> SimReport {
        let mut objects = BTreeMap::new();
        objects.insert(
            "Queue1".to_string(),
            openflex_core::ObjectAggregate {
                kind: ObjectKind::Queue,
                input_count: Aggregate::from_samples(&[100.0]),
                output_count: Aggregate::from_samples(&[99.0]),
                avg_content: Aggregate::from_samples(&[2.5]),
                max_content: Aggregate::from_samples(&[7.0]),
                avg_stay_time: Aggregate::from_samples(&[3.2]),
                utilization: Aggregate::from_samples(&[0.5]),
                throughput: Aggregate::from_samples(&[0.2]),
                idle_fraction: Aggregate::from_samples(&[0.2]),
                busy_fraction: Aggregate::from_samples(&[0.5]),
                blocked_fraction: Aggregate::from_samples(&[0.3]),
                down_fraction: Aggregate::from_samples(&[0.0]),
            },
        );
        SimReport {
            model: "demo".into(),
            description: String::new(),
            time_unit: TimeUnit::Seconds,
            stop_time: 100.0,
            warmup: 0.0,
            base_seed: 1,
            replications: Vec::new(),
            objects,
            completed: Aggregate::from_samples(&[99.0]),
            created: Aggregate::from_samples(&[100.0]),
            little_check: None,
            process_flow: None,
            fluid: None,
        }
    }

    #[test]
    fn renders_all_formats() {
        let report = sample_report();
        assert!(to_table(&report).contains("Queue1"));
        assert!(to_csv(&report).contains("object,kind"));
        assert!(to_markdown(&report).contains("| Queue1 |"));
        let json = to_json(&report).expect("json");
        assert!(json.contains("\"model\": \"demo\""));
        assert!(json.contains("\"door_opening\""));
        assert!(to_table(&report).contains("door_opening"));
        assert!(to_markdown(&report).contains("door_opening"));
        for format in [Format::Table, Format::Json, Format::Csv, Format::Markdown] {
            let text = render_sim(&report, format).expect("render");
            assert!(text.ends_with('\n'), "{format:?} must end with a newline");
        }
    }

    #[test]
    fn table_has_item_totals_and_units() {
        let report = sample_report();
        let table = to_table(&report);
        assert!(
            table.contains("Stay (s)"),
            "time unit must appear in the header"
        );
        assert!(table.contains("Util (%)"));
        assert!(table.contains("Items created"));
        assert!(table.contains("Items completed"));
        assert!(table.contains("Items in system"));
    }

    #[test]
    fn markdown_has_confidence_columns_and_reproduce_section() {
        let report = sample_report();
        let markdown = to_markdown(&report);
        assert!(markdown.contains("95% CI"));
        assert!(markdown.contains("How to reproduce"));
        assert!(markdown.contains("openflex run"));
        assert!(markdown.contains("openflex report"));
    }
}
