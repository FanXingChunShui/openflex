//! Excel (`.xlsx`) export of a simulation report.
//!
//! The workbook is the format analysts ask for in practice: one sheet with the
//! headline numbers, one row per object, and every replication sample so the
//! data can be pivoted or charted without re-running the model. It is written
//! with `rust_xlsxwriter` (pure Rust, no system Office dependency).
//!
//! Only the report is exported — the workbook contains no formulas, so it opens
//! as static data everywhere.

use std::fs;
use std::path::Path;

use openflex_core::SimReport;
use rust_xlsxwriter::{Format, Workbook, XlsxError};

use crate::{ReportError, i18n, in_system};

const SUMMARY_SHEET: &str = "Summary";
const OBJECTS_SHEET: &str = "Objects";
const REPLICATIONS_SHEET: &str = "Replications";

fn file_error(path: &Path, source: XlsxError) -> ReportError {
    ReportError::XlsxFile {
        path: path.display().to_string(),
        source,
    }
}

/// Write `report` as an `.xlsx` workbook at `path`, creating parent directories.
pub fn write_xlsx(report: &SimReport, path: &Path) -> Result<(), ReportError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|source| ReportError::Io {
            path: parent.display().to_string(),
            source,
        })?;
    }

    let mut workbook = Workbook::new();

    let bold = Format::new().set_bold();
    let heading = Format::new().set_bold().set_font_size(14);
    let number = Format::new().set_num_format("0.000");
    let rate = Format::new().set_num_format("0.0000");

    write_summary(&mut workbook, report, &bold, &heading, &number)?;
    write_objects(&mut workbook, report, &bold, &number, &rate)?;
    write_replications(&mut workbook, report, &bold, &number, &rate)?;

    workbook
        .save(path)
        .map_err(|source| file_error(path, source))?;
    Ok(())
}

fn write_summary(
    workbook: &mut Workbook,
    report: &SimReport,
    bold: &Format,
    heading: &Format,
    number: &Format,
) -> Result<(), ReportError> {
    let worksheet = workbook.add_worksheet();
    worksheet
        .set_name(SUMMARY_SHEET)
        .map_err(|source| file_error(Path::new(SUMMARY_SHEET), source))?;
    worksheet.set_column_width(0, 28)?;
    worksheet.set_column_width(1, 24)?;

    worksheet.write_string_with_format(0, 0, i18n::t("report_title"), heading)?;
    worksheet.write_string(1, 0, i18n::t("model"))?;
    worksheet.write_string(1, 1, &report.model)?;
    worksheet.write_string(2, 0, "description")?;
    worksheet.write_string(2, 1, &report.description)?;
    worksheet.write_string(3, 0, i18n::t("stop_time"))?;
    worksheet.write_number_with_format(3, 1, report.stop_time, number)?;
    worksheet.write_string(4, 0, i18n::t("warmup"))?;
    worksheet.write_number_with_format(4, 1, report.warmup, number)?;
    worksheet.write_string(5, 0, i18n::t("replications"))?;
    worksheet.write_number(5, 1, report.replications.len().max(1) as f64)?;
    worksheet.write_string(6, 0, i18n::t("base_seed"))?;
    worksheet.write_number(6, 1, report.base_seed as f64)?;
    worksheet.write_string(7, 0, "time unit")?;
    worksheet.write_string(7, 1, i18n::unit(report.time_unit))?;

    let in_system = in_system(report);
    let rows: [(&str, f64, f64); 3] = [
        (
            i18n::t("items_created"),
            report.created.mean,
            report.created.ci95,
        ),
        (
            i18n::t("items_completed"),
            report.completed.mean,
            report.completed.ci95,
        ),
        (i18n::t("items_in_system"), in_system.mean, in_system.ci95),
    ];
    worksheet.write_string_with_format(9, 0, i18n::t("metric"), bold)?;
    worksheet.write_string_with_format(9, 1, i18n::t("mean"), bold)?;
    worksheet.write_string_with_format(9, 2, "95% CI", bold)?;
    for (offset, (label, mean, ci95)) in rows.iter().enumerate() {
        let row = 10 + offset as u32;
        worksheet.write_string(row, 0, *label)?;
        worksheet.write_number_with_format(row, 1, *mean, number)?;
        worksheet.write_number_with_format(row, 2, *ci95, number)?;
    }
    worksheet.set_column_width(2, 14)?;
    Ok(())
}

fn write_objects(
    workbook: &mut Workbook,
    report: &SimReport,
    bold: &Format,
    number: &Format,
    rate: &Format,
) -> Result<(), ReportError> {
    let worksheet = workbook.add_worksheet();
    worksheet
        .set_name(OBJECTS_SHEET)
        .map_err(|source| file_error(Path::new(OBJECTS_SHEET), source))?;
    let headers = object_headers(report);
    for (column, header) in headers.iter().enumerate() {
        worksheet.write_string_with_format(0, column as u16, header.as_str(), bold)?;
        worksheet.set_column_width(column as u16, 14)?;
    }
    for (row, (name, stats)) in report.objects.iter().enumerate() {
        let row = row as u32 + 1;
        worksheet.write_string(row, 0, name)?;
        worksheet.write_string(row, 1, format!("{:?}", stats.kind))?;
        worksheet.write_number_with_format(row, 2, stats.input_count.mean, number)?;
        worksheet.write_number_with_format(row, 3, stats.output_count.mean, number)?;
        worksheet.write_number_with_format(row, 4, stats.avg_content.mean, number)?;
        worksheet.write_number_with_format(row, 5, stats.max_content.mean, number)?;
        worksheet.write_number_with_format(row, 6, stats.avg_stay_time.mean, number)?;
        worksheet.write_number_with_format(row, 7, stats.utilization.mean, rate)?;
        worksheet.write_number_with_format(row, 8, stats.throughput.mean, rate)?;
        worksheet.write_number_with_format(row, 9, stats.idle_fraction.mean, rate)?;
        worksheet.write_number_with_format(row, 10, stats.busy_fraction.mean, rate)?;
        worksheet.write_number_with_format(row, 11, stats.blocked_fraction.mean, rate)?;
        worksheet.write_number_with_format(row, 12, stats.down_fraction.mean, rate)?;
    }
    worksheet.set_freeze_panes(1, 0)?;
    Ok(())
}

fn write_replications(
    workbook: &mut Workbook,
    report: &SimReport,
    bold: &Format,
    number: &Format,
    rate: &Format,
) -> Result<(), ReportError> {
    let worksheet = workbook.add_worksheet();
    worksheet
        .set_name(REPLICATIONS_SHEET)
        .map_err(|source| file_error(Path::new(REPLICATIONS_SHEET), source))?;
    let headers = replication_headers(report);
    for (column, header) in headers.iter().enumerate() {
        worksheet.write_string_with_format(0, column as u16, header.as_str(), bold)?;
        worksheet.set_column_width(column as u16, 14)?;
    }

    let mut row = 1u32;
    for replication in &report.replications {
        for (name, stats) in &replication.objects {
            worksheet.write_number(row, 0, replication.index as f64)?;
            worksheet.write_number(row, 1, replication.seed as f64)?;
            worksheet.write_string(row, 2, name)?;
            worksheet.write_string(row, 3, format!("{:?}", stats.kind))?;
            worksheet.write_number(row, 4, stats.input_count as f64)?;
            worksheet.write_number(row, 5, stats.output_count as f64)?;
            worksheet.write_number_with_format(row, 6, stats.avg_content, number)?;
            worksheet.write_number_with_format(row, 7, stats.max_content as f64, number)?;
            worksheet.write_number_with_format(row, 8, stats.avg_stay_time, number)?;
            worksheet.write_number_with_format(row, 9, stats.utilization, rate)?;
            worksheet.write_number_with_format(row, 10, stats.throughput, rate)?;
            worksheet.write_number_with_format(
                row,
                11,
                stats.state_times.fraction("idle"),
                rate,
            )?;
            worksheet.write_number_with_format(
                row,
                12,
                stats.state_times.fraction("busy"),
                rate,
            )?;
            worksheet.write_number_with_format(
                row,
                13,
                stats.state_times.fraction("blocked"),
                rate,
            )?;
            worksheet.write_number_with_format(
                row,
                14,
                stats.state_times.fraction("down"),
                rate,
            )?;
            row += 1;
        }
    }
    if report.replications.is_empty() {
        worksheet.write_string_with_format(1, 0, "no per-replication samples", bold)?;
    }
    worksheet.set_freeze_panes(1, 0)?;
    Ok(())
}

/// Headers of the `Objects` sheet. The stay-time column carries the model's
/// time unit, exactly like `report.csv`, so the two exports can be compared.
fn object_headers(report: &SimReport) -> Vec<String> {
    vec![
        "object".into(),
        "kind".into(),
        "input".into(),
        "output".into(),
        "avg_wip".into(),
        "max_wip".into(),
        format!("avg_stay_{}", i18n::unit(report.time_unit)),
        "utilization".into(),
        "throughput".into(),
        "idle".into(),
        "busy".into(),
        "blocked".into(),
        "down".into(),
    ]
}

/// Headers of the `Replications` sheet (one row per replication and object).
fn replication_headers(report: &SimReport) -> Vec<String> {
    let mut headers = vec!["replication".to_string(), "seed".to_string()];
    headers.extend(object_headers(report));
    headers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::sample_report;

    #[test]
    fn writes_a_zip_container_with_all_sheets() {
        let directory = std::env::temp_dir().join(format!(
            "openflex-xlsx-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ));
        fs::create_dir_all(&directory).expect("temp dir");
        let path = directory.join("report.xlsx");
        write_xlsx(&sample_report(), &path).expect("write xlsx");
        let bytes = fs::read(&path).expect("read xlsx");
        assert_eq!(&bytes[..4], b"PK\x03\x04", "xlsx is a zip container");
        assert!(bytes.len() > 1_000);
        let _ = fs::remove_dir_all(&directory);
    }
}
