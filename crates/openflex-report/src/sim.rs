//! Rendering of a single [`SimReport`].
//!
//! The table is meant for a terminal, CSV for spreadsheets, Markdown for pull
//! requests and JSON for tooling. All times use the model's time unit and all
//! numbers use one fixed convention: counts with one decimal, times with two,
//! percentages with one.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use openflex_core::{Aggregate, ObjectAggregate, SimReport};

use crate::{
    ReportError, fmt_ci, fmt_count, fmt_percent, fmt_rate, fmt_time, i18n, in_system,
    reproduce_command, write_file,
};

pub const REPORT_JSON: &str = "report.json";
pub const REPORT_CSV: &str = "report.csv";
pub const REPORT_MARKDOWN: &str = "report.md";
pub const REPLICATIONS_CSV: &str = "replications.csv";

const WIDTH: usize = 80;

/// Pretty JSON, suitable for machine consumption and for the report archive.
pub fn to_json(report: &SimReport) -> Result<String, ReportError> {
    let mut text = serde_json::to_string_pretty(report)?;
    text.push('\n');
    Ok(text)
}

/// Fixed-width terminal table of the key metrics.
///
/// The labels follow `OPENFLEX_LANG`; the numbers and their formatting do not,
/// so a translated report can still be diffed against an English one.
pub fn to_table(report: &SimReport) -> String {
    let mut out = String::new();
    let unit = i18n::unit(report.time_unit);
    let _ = writeln!(
        out,
        "{}{}{}",
        i18n::t("report_title"),
        i18n::t("title_separator"),
        report.model
    );
    if !report.description.is_empty() {
        let _ = writeln!(out, "{}", report.description);
    }
    let _ = writeln!(
        out,
        "{} {} {}, {} {} {}, {} {}, {} {}",
        i18n::t("stop_time"),
        fmt_time(report.stop_time),
        unit,
        i18n::t("warmup"),
        fmt_time(report.warmup),
        unit,
        i18n::t("replications"),
        report.replications.len().max(1),
        i18n::t("base_seed"),
        report.base_seed
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{:<22} {:>9} {:>9} {:>9} {:>9} {:>10} {:>8}",
        i18n::t("object"),
        i18n::t("in"),
        i18n::t("out"),
        i18n::t("avg_wip"),
        i18n::t("max_wip"),
        format!("{} ({unit})", i18n::t("stay")),
        i18n::t("util")
    );
    let _ = writeln!(out, "{}", "-".repeat(WIDTH));
    for (name, stats) in &report.objects {
        let _ = writeln!(out, "{}", row(name, stats));
    }
    let _ = writeln!(out, "{}", "-".repeat(WIDTH));
    let in_system = in_system(report);
    let _ = writeln!(
        out,
        "{}",
        total_row(i18n::t("items_created"), &report.created)
    );
    let _ = writeln!(
        out,
        "{}",
        total_row(i18n::t("items_completed"), &report.completed)
    );
    let _ = writeln!(out, "{}", total_row(i18n::t("items_in_system"), &in_system));
    out
}

fn row(name: &str, stats: &ObjectAggregate) -> String {
    let mut row = String::new();
    let _ = write!(row, "{name:<22}");
    let _ = write!(row, " {:>9.1}", stats.input_count.mean);
    let _ = write!(row, " {:>9.1}", stats.output_count.mean);
    let _ = write!(row, " {:>9.2}", stats.avg_content.mean);
    let _ = write!(row, " {:>9.1}", stats.max_content.mean);
    let _ = write!(row, " {:>10.2}", stats.avg_stay_time.mean);
    let _ = write!(row, " {:>8.1}", stats.utilization.mean * 100.0);
    row
}

fn total_row(label: &str, aggregate: &Aggregate) -> String {
    let mut row = format!("{label:<22} {:>9.1}", aggregate.mean);
    if aggregate.n > 1 {
        let _ = write!(row, "  (95% CI {})", fmt_ci(aggregate.ci95));
    }
    row
}

/// CSV with one row per object; multi-replication values are means.
pub fn to_csv(report: &SimReport) -> String {
    let mut out = String::new();
    let unit = report.time_unit.label();
    let _ = writeln!(
        out,
        "object,kind,input,output,avg_wip,max_wip,avg_stay_{unit},utilization,throughput,idle,busy,blocked,down"
    );
    for (name, stats) in &report.objects {
        let _ = writeln!(
            out,
            "{name},{:?},{:.3},{:.3},{:.3},{:.3},{:.3},{:.4},{:.6},{:.4},{:.4},{:.4},{:.4}",
            stats.kind,
            stats.input_count.mean,
            stats.output_count.mean,
            stats.avg_content.mean,
            stats.max_content.mean,
            stats.avg_stay_time.mean,
            stats.utilization.mean,
            stats.throughput.mean,
            stats.idle_fraction.mean,
            stats.busy_fraction.mean,
            stats.blocked_fraction.mean,
            stats.down_fraction.mean
        );
    }
    out
}

/// CSV with one row per replication *and* object, so every sample that feeds
/// the aggregates can be inspected and plotted.
///
/// Only written when a report has more than one replication; the aggregated
/// view lives in `report.csv`.
pub fn to_replications_csv(report: &SimReport) -> String {
    let mut out = String::new();
    let unit = report.time_unit.label();
    let _ = writeln!(
        out,
        "replication,seed,object,kind,input,output,avg_wip,max_wip,avg_stay_{unit},utilization,throughput,idle,busy,blocked,down"
    );
    for replication in &report.replications {
        let seed = replication.seed;
        for (name, stats) in &replication.objects {
            let _ = writeln!(
                out,
                "{},{seed},{name},{:?},{},{},{:.3},{},{:.3},{:.4},{:.6},{:.4},{:.4},{:.4},{:.4}",
                replication.index,
                stats.kind,
                stats.input_count,
                stats.output_count,
                stats.avg_content,
                stats.max_content,
                stats.avg_stay_time,
                stats.utilization,
                stats.throughput,
                stats.state_times.fraction("idle"),
                stats.state_times.fraction("busy"),
                stats.state_times.fraction("blocked"),
                stats.state_times.fraction("down")
            );
        }
    }
    out
}

/// Markdown report, friendly to pull requests and documentation.
///
/// Titles, headings and column names follow `OPENFLEX_LANG`; the shell commands
/// in the "how to reproduce" block never do, so a localised report still runs
/// as written.
pub fn to_markdown(report: &SimReport) -> String {
    let mut out = String::new();
    let unit = i18n::unit(report.time_unit);
    let _ = writeln!(
        out,
        "# {}{}{}",
        i18n::t("report_title"),
        i18n::t("title_separator"),
        report.model
    );
    let _ = writeln!(out);
    if !report.description.is_empty() {
        let _ = writeln!(out, "{}\n", report.description);
    }
    let _ = writeln!(
        out,
        "- {}: {} {unit}\n- {}: {} {unit}\n- {}: {}\n- {}: {}\n",
        i18n::t("stop_time_label"),
        fmt_time(report.stop_time),
        i18n::t("warmup_label"),
        fmt_time(report.warmup),
        i18n::t("replications_label"),
        report.replications.len().max(1),
        i18n::t("base_seed_label"),
        report.base_seed
    );
    let _ = writeln!(out, "## {}\n", i18n::t("objects"));
    let _ = writeln!(
        out,
        "| {} | {} | {} | {} | {} | {} | {} ({unit}) | {} | {} | {} | {} |",
        i18n::t("object"),
        i18n::t("kind"),
        i18n::t("in"),
        i18n::t("out"),
        i18n::t("avg_wip_md"),
        i18n::t("max_wip_md"),
        i18n::t("stay"),
        i18n::t("stay_ci"),
        i18n::t("utilisation"),
        i18n::t("util_ci"),
        i18n::t("throughput")
    );
    let _ = writeln!(
        out,
        "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|"
    );
    for (name, stats) in &report.objects {
        let _ = writeln!(
            out,
            "| {name} | {:?} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            stats.kind,
            fmt_count(stats.input_count.mean),
            fmt_count(stats.output_count.mean),
            fmt_time(stats.avg_content.mean),
            fmt_count(stats.max_content.mean),
            fmt_time(stats.avg_stay_time.mean),
            fmt_ci(stats.avg_stay_time.ci95),
            fmt_percent(stats.utilization.mean),
            fmt_ci(stats.utilization.ci95),
            fmt_rate(stats.throughput.mean)
        );
    }
    let in_system = in_system(report);
    let _ = writeln!(out, "\n## {}\n", i18n::t("totals"));
    let _ = writeln!(
        out,
        "| {} | {} | 95% CI |",
        i18n::t("metric"),
        i18n::t("mean")
    );
    let _ = writeln!(out, "|---|---:|---:|");
    let _ = writeln!(
        out,
        "| {} | {} | {} |",
        i18n::t("items_created"),
        fmt_count(report.created.mean),
        fmt_ci(report.created.ci95)
    );
    let _ = writeln!(
        out,
        "| {} | {} | {} |",
        i18n::t("items_completed"),
        fmt_count(report.completed.mean),
        fmt_ci(report.completed.ci95)
    );
    let _ = writeln!(
        out,
        "| {} | {} | {} |",
        i18n::t("items_in_system"),
        fmt_count(in_system.mean),
        fmt_ci(in_system.ci95)
    );
    let _ = writeln!(out, "\n## {}\n", i18n::t("how_to_reproduce"));
    let _ = writeln!(out, "```sh");
    let _ = writeln!(out, "{}", reproduce_command(report));
    let _ = writeln!(out, "openflex report report.json --format markdown");
    let _ = writeln!(out, "```");
    out
}

/// Write `report.json`, `report.csv` and `report.md` into `directory`, plus
/// `replications.csv` when the report has more than one replication.
pub fn write_reports(report: &SimReport, directory: &Path) -> Result<Vec<PathBuf>, ReportError> {
    let mut written = Vec::new();

    let json_path = directory.join(REPORT_JSON);
    write_file(&json_path, &to_json(report)?)?;
    written.push(json_path);

    let csv_path = directory.join(REPORT_CSV);
    write_file(&csv_path, &to_csv(report))?;
    written.push(csv_path);

    let markdown_path = directory.join(REPORT_MARKDOWN);
    write_file(&markdown_path, &to_markdown(report))?;
    written.push(markdown_path);

    if report.replications.len() > 1 {
        let path = directory.join(REPLICATIONS_CSV);
        write_file(&path, &to_replications_csv(report))?;
        written.push(path);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::sample_report;

    #[test]
    fn csv_has_one_row_per_object() {
        let report = sample_report();
        let csv = to_csv(&report);
        assert_eq!(csv.lines().count(), 2, "header plus one object");
        assert!(csv.contains("Queue1,Queue,"));
    }

    #[test]
    fn table_includes_item_totals() {
        let table = to_table(&sample_report());
        assert!(table.contains("Items created"));
        assert!(table.contains("Items completed"));
        assert!(table.contains("Items in system"));
        assert!(table.contains("Stay (s)"));
    }

    #[test]
    fn replications_csv_is_empty_without_samples() {
        let report = sample_report();
        assert_eq!(to_replications_csv(&report).lines().count(), 1);
    }
}
