//! Terminal charts for a simulation report.
//!
//! This is the half of the "dashboard" idea that works without a graphics
//! engine: a [`SimReport`] already carries everything a terminal needs, so the
//! chart module only decides how to draw it. Three data sources are supported,
//! all read from the existing report:
//!
//! - **utilisation** — one bar per object whose "busy" fraction means something
//!   (the same judgement the GUI uses),
//! - **performance measures** — one bar per model-defined measure, annotated
//!   with its sample count and interval,
//! - **tracked series** — a time series as a one-line sparkline.
//!
//! Output is plain text built from Unicode block characters, so it survives
//! copy-paste into Markdown, issues and chat. Every renderer has a defined
//! answer for the awkward inputs (no data, all zeros, a single sample, a
//! constant series) instead of printing an empty canvas.

use std::fmt::Write as _;

use openflex_core::SimReport;
use openflex_model::{ObjectKind, TimeUnit};

use crate::{NO_INTERVAL, ReportError, display_width, fmt_percent, i18n, pad_right};

/// Lower bound of the drawing area, in characters.
pub const MIN_WIDTH: usize = 10;
/// Upper bound of the drawing area, in characters.
pub const MAX_WIDTH: usize = 120;
/// More rows than this are bucketed, so a long series cannot flood a terminal.
const MAX_ROWS: usize = 40;

/// Block characters used by the sparkline, from lowest to highest.
const SPARK_LEVELS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartKind {
    /// Horizontal bars, one per label (or one per bucket of a series).
    Bar,
    /// One-line sparkline, best for time series.
    Line,
}

/// How a value is printed next to its bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueFormat {
    /// Three decimals, as used by the performance-measure tables.
    Number,
    /// One-decimal percentage, as used by the utilisation tables.
    Percent,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChartSeries {
    /// Row label (object, measure, series name or a timestamp).
    pub label: String,
    /// `(x, y)` samples; a bar chart uses one row per point.
    pub points: Vec<(f64, f64)>,
    /// Extra text shown after the value (sample count, interval, …).
    pub note: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChartSpec {
    pub title: String,
    pub series: Vec<ChartSeries>,
    pub kind: ChartKind,
    pub width: usize,
    pub format: ValueFormat,
}

impl ChartSeries {
    /// A label with a single value, the usual shape of a bar row.
    pub fn single(label: impl Into<String>, value: f64) -> Self {
        Self {
            label: label.into(),
            points: vec![(0.0, value)],
            note: String::new(),
        }
    }

    /// A time series: `(time, value)` pairs.
    pub fn series(label: impl Into<String>, points: Vec<(f64, f64)>) -> Self {
        Self {
            label: label.into(),
            points,
            note: String::new(),
        }
    }

    /// Attach the annotation shown after the value.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = note.into();
        self
    }
}

/// Clamp a requested drawing width into the supported range.
pub fn clamp_width(width: usize) -> usize {
    width.clamp(MIN_WIDTH, MAX_WIDTH)
}

// ---------------------------------------------------------------------------
// Data sources
// ---------------------------------------------------------------------------

/// Objects whose "busy" fraction says something about capacity.
///
/// Sources, sinks and queues are always "busy" or always "idle" by design, so
/// their utilisation is not a capacity signal. This mirrors the judgement the
/// desktop GUI uses; the two surfaces must not disagree about the same model.
pub fn counts_for_utilization(kind: ObjectKind) -> bool {
    !matches!(
        kind,
        ObjectKind::Source
            | ObjectKind::Sink
            | ObjectKind::Queue
            | ObjectKind::Dispatcher
            | ObjectKind::Resource
    )
}

/// Bar chart of the utilisation of every capacity-bearing object.
pub fn utilization_spec(report: &SimReport, width: usize) -> ChartSpec {
    let series = report
        .objects
        .iter()
        .filter(|(_, stats)| counts_for_utilization(stats.kind))
        .map(|(name, stats)| {
            ChartSeries::single(name.clone(), stats.utilization.mean)
                .with_note(format!("n={}", stats.utilization.n))
        })
        .collect();
    ChartSpec {
        title: format!(
            "{}{}{}",
            i18n::t("chart_utilization"),
            i18n::t("title_separator"),
            report.model
        ),
        series,
        kind: ChartKind::Bar,
        width,
        format: ValueFormat::Percent,
    }
}

/// Bar chart of the model-defined performance measures.
///
/// `n < 2` has no meaningful interval, so the note says so explicitly instead
/// of printing `±0.000`.
pub fn measures_spec(report: &SimReport, width: usize) -> ChartSpec {
    let series = report
        .performance_measures
        .iter()
        .map(|(name, aggregate)| {
            let note = if aggregate.n >= 2 {
                format!(
                    "n={} {} ±{:.3}",
                    aggregate.n,
                    i18n::t("interval"),
                    aggregate.ci95
                )
            } else {
                format!("n={} {}", aggregate.n, NO_INTERVAL)
            };
            ChartSeries::single(name.clone(), aggregate.mean).with_note(note)
        })
        .collect();
    ChartSpec {
        title: format!(
            "{}{}{}",
            i18n::t("chart_measures"),
            i18n::t("title_separator"),
            report.model
        ),
        series,
        kind: ChartKind::Bar,
        width,
        format: ValueFormat::Number,
    }
}

/// Line chart of one tracked series.
///
/// The error lists the available names, which is the only way to discover a
/// typo without opening the report JSON.
pub fn series_spec(report: &SimReport, name: &str, width: usize) -> Result<ChartSpec, ReportError> {
    let Some(points) = report.tracked_series.get(name) else {
        let available: Vec<&str> = report.tracked_series.keys().map(String::as_str).collect();
        return Err(ReportError::UnknownSeries {
            name: name.to_string(),
            available: if available.is_empty() {
                "none".to_string()
            } else {
                available.join(", ")
            },
        });
    };
    Ok(ChartSpec {
        title: format!(
            "{}{}{} ({})",
            i18n::t("chart_series"),
            i18n::t("title_separator"),
            name,
            unit_label(report.time_unit)
        ),
        series: vec![ChartSeries::series(name, points.clone())],
        kind: ChartKind::Line,
        width,
        format: ValueFormat::Number,
    })
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render a chart as plain text.
pub fn render(spec: &ChartSpec) -> String {
    let mut out = String::new();
    let width = clamp_width(spec.width);
    let _ = writeln!(out, "{}", spec.title);
    let _ = writeln!(
        out,
        "{}",
        "─".repeat(display_width(&spec.title).min(width + 26))
    );
    match spec.kind {
        ChartKind::Bar => out.push_str(&render_bar(spec, width)),
        ChartKind::Line => out.push_str(&render_line(spec, width)),
    }
    out
}

/// Render a chart wrapped in a Markdown code block with a `##` heading.
pub fn render_markdown(spec: &ChartSpec) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "## {}", spec.title);
    let _ = writeln!(out);
    let _ = writeln!(out, "```");
    let _ = write!(out, "{}", render_body(spec));
    let _ = writeln!(out, "```");
    out
}

/// The drawing itself, without the plain-text title (used by Markdown).
fn render_body(spec: &ChartSpec) -> String {
    let width = clamp_width(spec.width);
    match spec.kind {
        ChartKind::Bar => render_bar(spec, width),
        ChartKind::Line => render_line(spec, width),
    }
}

/// Turn every series into one `(label, value)` row.
///
/// A single-point series is one row. A multi-point series (a time series asked
/// for as bars) becomes one row per sample; when there are more samples than
/// [`MAX_ROWS`] they are bucketed by time so the table stays readable.
fn bar_rows(spec: &ChartSpec) -> (Vec<(String, f64, String)>, Option<usize>) {
    let mut rows: Vec<(String, f64, String)> = Vec::new();
    let mut bucketed_from: Option<usize> = None;

    for series in &spec.series {
        if series.points.len() <= 1 {
            let value = series
                .points
                .first()
                .map(|(_, value)| *value)
                .unwrap_or(0.0);
            rows.push((series.label.clone(), value, series.note.clone()));
            continue;
        }

        let samples = series.points.len();
        let groups = samples.clamp(1, MAX_ROWS);
        if samples > MAX_ROWS {
            bucketed_from = Some(samples);
        }
        // Bucket by index: each bucket averages its samples, and the label is
        // the age of the last sample in the bucket.
        for bucket in 0..groups {
            let start = bucket * samples / groups;
            let end = ((bucket + 1) * samples / groups)
                .max(start + 1)
                .min(samples);
            let slice = &series.points[start..end];
            let mean = slice.iter().map(|(_, value)| *value).sum::<f64>() / slice.len() as f64;
            let label = if samples > MAX_ROWS {
                format!(
                    "{}..{}",
                    fmt_time(slice[0].0),
                    fmt_time(slice[slice.len() - 1].0)
                )
            } else {
                fmt_time(slice[slice.len() - 1].0)
            };
            rows.push((label, mean, series.note.clone()));
        }
    }
    (rows, bucketed_from)
}

fn fmt_time(value: f64) -> String {
    format!("{value:.2}")
}

fn format_value(value: f64, format: ValueFormat) -> String {
    match format {
        ValueFormat::Number => format!("{value:.3}"),
        ValueFormat::Percent => fmt_percent(value),
    }
}

fn render_bar(spec: &ChartSpec, width: usize) -> String {
    let (rows, bucketed_from) = bar_rows(spec);
    if rows.is_empty() {
        return format!("({})\n", i18n::t("chart_no_data"));
    }

    let max = rows
        .iter()
        .map(|(_, value, _)| value.abs())
        .fold(0.0f64, f64::max);
    if max <= 0.0 {
        let mut out = format!("({})\n", i18n::t("chart_all_zero"));
        for (label, _, _) in &rows {
            let _ = writeln!(
                out,
                "{} {}",
                pad_right(label, label_width(&rows)),
                format_value(0.0, spec.format)
            );
        }
        return out;
    }

    let note_width = rows
        .iter()
        .map(|(_, _, note)| display_width(note))
        .max()
        .unwrap_or(0);
    let label_column = label_width(&rows);
    let value_column = rows
        .iter()
        .map(|(_, value, _)| display_width(&format_value(*value, spec.format)))
        .max()
        .unwrap_or(0);

    let mut out = String::new();
    if rows.iter().any(|(_, value, _)| *value < 0.0) {
        let _ = writeln!(out, "({})", i18n::t("chart_negative"));
    }
    if let Some(samples) = bucketed_from {
        let _ = writeln!(
            out,
            "({} {} -> {} bars, {})",
            samples,
            i18n::t("chart_samples"),
            rows.len(),
            i18n::t("chart_bucketed")
        );
    }
    for (label, value, note) in &rows {
        let bar_len = if *value == 0.0 {
            0
        } else {
            // A non-zero value always shows at least one cell: a bar that
            // rounds to nothing would read as zero.
            ((value.abs() / max) * width as f64).round().max(1.0) as usize
        };
        let bar = format!("{:<width$}", "█".repeat(bar_len.min(width)));
        let mut line = format!(
            "{} ▏{} {:>value_column$}",
            pad_right(label, label_column),
            bar,
            format_value(*value, spec.format),
        );
        if !note.is_empty() {
            let _ = write!(line, "  {}", pad_right(note, note_width));
        }
        let _ = writeln!(out, "{}", line.trim_end());
    }
    out
}

fn label_width(rows: &[(String, f64, String)]) -> usize {
    rows.iter()
        .map(|(label, _, _)| display_width(label))
        .max()
        .unwrap_or(0)
}

/// One-line sparkline per series, with its numeric range underneath.
fn render_line(spec: &ChartSpec, width: usize) -> String {
    if spec.series.is_empty() {
        return format!("({})\n", i18n::t("chart_no_data"));
    }
    let label_column = spec
        .series
        .iter()
        .map(|series| display_width(&series.label))
        .max()
        .unwrap_or(0);
    let mut out = String::new();
    for series in &spec.series {
        if series.points.is_empty() {
            let _ = writeln!(
                out,
                "{} ({})",
                pad_right(&series.label, label_column),
                i18n::t("chart_no_data")
            );
            continue;
        }
        if series.points.len() == 1 {
            let (time, value) = series.points[0];
            let _ = writeln!(
                out,
                "{} ({}, t={} v={})",
                pad_right(&series.label, label_column),
                i18n::t("chart_single_point"),
                fmt_time(time),
                format_value(value, spec.format)
            );
            continue;
        }
        let values: Vec<f64> = series.points.iter().map(|(_, value)| *value).collect();
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if (max - min).abs() <= f64::EPSILON {
            let _ = writeln!(
                out,
                "{} ({}, {})",
                pad_right(&series.label, label_column),
                i18n::t("chart_constant"),
                format_value(min, spec.format)
            );
            continue;
        }
        let spark = sparkline(&series.points, width, min, max);
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let _ = writeln!(
            out,
            "{} {} {} .. {}  ({} {})",
            pad_right(&series.label, label_column),
            spark,
            format_value(min, spec.format),
            format_value(max, spec.format),
            i18n::t("chart_mean"),
            format_value(mean, spec.format)
        );
    }
    out
}

/// Resample the series to `width` columns and map each bucket to a block.
fn sparkline(points: &[(f64, f64)], width: usize, min: f64, max: f64) -> String {
    let columns = points.len().min(width).max(1);
    let mut spark = String::with_capacity(columns);
    for column in 0..columns {
        let start = column * points.len() / columns;
        let end = ((column + 1) * points.len() / columns).max(start + 1);
        let slice = &points[start..end.min(points.len())];
        let mean = slice.iter().map(|(_, value)| *value).sum::<f64>() / slice.len() as f64;
        let position = ((mean - min) / (max - min)).clamp(0.0, 1.0);
        let level = (position * (SPARK_LEVELS.len() - 1) as f64).round() as usize;
        spark.push(SPARK_LEVELS[level.min(SPARK_LEVELS.len() - 1)]);
    }
    spark
}

/// Time unit label used in chart titles.
pub fn unit_label(unit: TimeUnit) -> &'static str {
    i18n::unit(unit)
}
