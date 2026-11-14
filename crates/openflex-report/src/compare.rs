//! A/B comparison of two simulation reports.
//!
//! `openflex compare baseline.json candidate.json` answers the question an
//! experimenter asks first: which scenario moved which metric, and by how much?
//! The comparison is per object and covers throughput, utilisation, average
//! work in process (WIP), average stay time and completed items. Every number
//! carries the absolute delta and, when the baseline is not zero, the relative
//! change in percent.
//!
//! Objects that exist in only one of the reports are kept in the output with a
//! missing side, so a model change that adds or removes an object is visible
//! rather than silently ignored.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

use openflex_core::{ObjectAggregate, SimReport};
use openflex_model::{ObjectKind, TimeUnit};
use serde::{Deserialize, Serialize};

use crate::{fmt_count, fmt_percent, fmt_rate, fmt_time};

/// The metrics compared for every object, in display order.
const METRICS: [&str; 5] = [
    "throughput",
    "utilization",
    "avg_wip",
    "avg_stay_time",
    "items_completed",
];

/// One side of the comparison: where the report came from and what it ran.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportRef {
    /// File the report was read from (as given on the command line).
    pub path: String,
    pub model: String,
    pub replications: usize,
    pub stop_time: f64,
    pub warmup: f64,
    pub time_unit: TimeUnit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    pub baseline: ReportRef,
    pub candidate: ReportRef,
    pub objects: Vec<ObjectComparison>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectComparison {
    pub object: String,
    pub kind: ObjectKind,
    pub metrics: Vec<MetricComparison>,
}

/// One metric of one object: both values, the absolute difference and the
/// relative difference in percent (`25.0` means the candidate is 25% higher).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricComparison {
    pub metric: String,
    pub unit: String,
    pub baseline: Option<f64>,
    pub candidate: Option<f64>,
    pub delta: Option<f64>,
    pub delta_percent: Option<f64>,
}

/// Compare two reports; `baseline_path` and `candidate_path` are only used as
/// labels in the output.
pub fn compare_reports(
    baseline_path: &Path,
    baseline: &SimReport,
    candidate_path: &Path,
    candidate: &SimReport,
) -> Comparison {
    let names: BTreeSet<&String> = baseline
        .objects
        .keys()
        .chain(candidate.objects.keys())
        .collect();

    let objects = names
        .into_iter()
        .map(|name| {
            let before = baseline.objects.get(name);
            let after = candidate.objects.get(name);
            let kind = before
                .or(after)
                .map(|stats| stats.kind)
                .unwrap_or(ObjectKind::BasicFr);
            let time_unit = candidate.time_unit.label();
            let metrics = METRICS
                .iter()
                .map(|metric| {
                    let baseline_value = before.and_then(|stats| metric_value(stats, metric));
                    let candidate_value = after.and_then(|stats| metric_value(stats, metric));
                    let delta = match (baseline_value, candidate_value) {
                        (Some(before), Some(after)) => Some(after - before),
                        _ => None,
                    };
                    let delta_percent = match (baseline_value, delta) {
                        (Some(before), Some(delta)) if before.abs() > f64::EPSILON => {
                            Some(delta / before * 100.0)
                        }
                        _ => None,
                    };
                    MetricComparison {
                        metric: (*metric).to_string(),
                        unit: metric_unit(metric, time_unit),
                        baseline: baseline_value,
                        candidate: candidate_value,
                        delta,
                        delta_percent,
                    }
                })
                .collect();
            ObjectComparison {
                object: name.clone(),
                kind,
                metrics,
            }
        })
        .collect();

    Comparison {
        baseline: report_ref(baseline_path, baseline),
        candidate: report_ref(candidate_path, candidate),
        objects,
    }
}

fn report_ref(path: &Path, report: &SimReport) -> ReportRef {
    ReportRef {
        path: path.display().to_string(),
        model: report.model.clone(),
        replications: report.replications.len().max(1),
        stop_time: report.stop_time,
        warmup: report.warmup,
        time_unit: report.time_unit,
    }
}

fn metric_value(stats: &ObjectAggregate, metric: &str) -> Option<f64> {
    Some(match metric {
        "throughput" => stats.throughput.mean,
        "utilization" => stats.utilization.mean,
        "avg_wip" => stats.avg_content.mean,
        "avg_stay_time" => stats.avg_stay_time.mean,
        "items_completed" => stats.output_count.mean,
        _ => return None,
    })
}

fn metric_unit(metric: &str, time_unit: &str) -> String {
    match metric {
        "throughput" => format!("items/{time_unit}"),
        "utilization" => "%".to_string(),
        "avg_wip" => "items".to_string(),
        "avg_stay_time" => time_unit.to_string(),
        _ => "items".to_string(),
    }
}

/// Format a metric value with the same conventions as the reports: counts with
/// one decimal, times with two, percentages with one.
fn display(metric: &str, value: Option<f64>) -> String {
    let Some(value) = value else {
        return "-".to_string();
    };
    match metric {
        "utilization" => fmt_percent(value),
        "throughput" => fmt_rate(value),
        "avg_wip" => format!("{value:.2}"),
        "avg_stay_time" => fmt_time(value),
        _ => fmt_count(value),
    }
}

fn display_delta(metric: &str, value: Option<f64>) -> String {
    let Some(value) = value else {
        return "-".to_string();
    };
    match metric {
        "utilization" => format!("{:+.1}pp", value * 100.0),
        "throughput" => format!("{value:+.4}"),
        "avg_wip" => format!("{value:+.2}"),
        "avg_stay_time" => format!("{value:+.2}"),
        _ => format!("{value:+.1}"),
    }
}

fn display_percent(value: Option<f64>) -> String {
    match value {
        Some(value) => format!("{value:+.1}%"),
        None => "-".to_string(),
    }
}

fn title(comparison: &Comparison) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "OpenFlex comparison");
    let _ = writeln!(
        out,
        "baseline : {} ({}, {} replication{})",
        comparison.baseline.path,
        comparison.baseline.model,
        comparison.baseline.replications,
        if comparison.baseline.replications == 1 {
            ""
        } else {
            "s"
        }
    );
    let _ = writeln!(
        out,
        "candidate: {} ({}, {} replication{})",
        comparison.candidate.path,
        comparison.candidate.model,
        comparison.candidate.replications,
        if comparison.candidate.replications == 1 {
            ""
        } else {
            "s"
        }
    );
    out
}

/// Terminal table: one line per object and metric.
pub fn to_table(comparison: &Comparison) -> String {
    let mut out = title(comparison);
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{:<20} {:<14} {:>12} {:>12} {:>12} {:>10}",
        "Object", "Metric", "Baseline", "Candidate", "Delta", "Delta %"
    );
    let _ = writeln!(out, "{}", "-".repeat(84));
    for object in &comparison.objects {
        for metric in &object.metrics {
            let _ = writeln!(
                out,
                "{:<20} {:<14} {:>12} {:>12} {:>12} {:>10}",
                object.object,
                metric.metric,
                display(&metric.metric, metric.baseline),
                display(&metric.metric, metric.candidate),
                display_delta(&metric.metric, metric.delta),
                display_percent(metric.delta_percent)
            );
        }
    }
    out
}

pub fn to_json(comparison: &Comparison) -> Result<String, crate::ReportError> {
    let mut text = serde_json::to_string_pretty(comparison)?;
    text.push('\n');
    Ok(text)
}

pub fn to_csv(comparison: &Comparison) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "object,kind,metric,unit,baseline,candidate,delta,delta_percent"
    );
    for object in &comparison.objects {
        for metric in &object.metrics {
            let _ = writeln!(
                out,
                "{},{:?},{},{},{},{},{},{}",
                object.object,
                object.kind,
                metric.metric,
                metric.unit,
                metric
                    .baseline
                    .map(|value| format!("{value:.6}"))
                    .unwrap_or_default(),
                metric
                    .candidate
                    .map(|value| format!("{value:.6}"))
                    .unwrap_or_default(),
                metric
                    .delta
                    .map(|value| format!("{value:.6}"))
                    .unwrap_or_default(),
                metric
                    .delta_percent
                    .map(|value| format!("{value:.4}"))
                    .unwrap_or_default()
            );
        }
    }
    out
}

pub fn to_markdown(comparison: &Comparison) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# OpenFlex comparison\n");
    let _ = writeln!(
        out,
        "- **Baseline**: `{}` ({}, {} replications)\n- **Candidate**: `{}` ({}, {} replications)\n",
        comparison.baseline.path,
        comparison.baseline.model,
        comparison.baseline.replications,
        comparison.candidate.path,
        comparison.candidate.model,
        comparison.candidate.replications
    );
    let _ = writeln!(out, "## Objects\n");
    let _ = writeln!(
        out,
        "| Object | Metric | Baseline | Candidate | Delta | Delta % |"
    );
    let _ = writeln!(out, "|---|---|---:|---:|---:|---:|");
    for object in &comparison.objects {
        for metric in &object.metrics {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} | {} |",
                object.object,
                metric.metric,
                display(&metric.metric, metric.baseline),
                display(&metric.metric, metric.candidate),
                display_delta(&metric.metric, metric.delta),
                display_percent(metric.delta_percent)
            );
        }
    }
    out
}
