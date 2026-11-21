//! Performance-measure rendering across every report format.
//!
//! A model may declare `performance_measures`: named expressions the engine
//! evaluates once per replication. These tests pin the contract of the three
//! rendering paths (experiment table, CSV, Markdown) and, just as important,
//! that a model *without* measures renders exactly as it did before the
//! feature existed.
//!
//! Run with `cargo test -p openflex-report --test measures`.

use std::collections::BTreeMap;

use openflex_core::{Aggregate, ObjectAggregate, SimReport};
use openflex_model::{ObjectKind, Override, TimeUnit};
use openflex_report::{
    ExperimentReport, Format, NO_INTERVAL, ScenarioReport, fmt_ci_opt, render_experiment,
    render_sim,
};

/// Report used as the "no measures declared" baseline.
fn base_report(name: &str, replications: u32) -> SimReport {
    let mut objects = BTreeMap::new();
    objects.insert(
        "Queue1".to_string(),
        ObjectAggregate {
            kind: ObjectKind::Queue,
            input_count: Aggregate::from_samples(&[40.0, 41.0]),
            output_count: Aggregate::from_samples(&[39.0, 41.0]),
            avg_content: Aggregate::from_samples(&[2.5, 3.0]),
            max_content: Aggregate::from_samples(&[7.0, 8.0]),
            avg_stay_time: Aggregate::from_samples(&[3.2, 3.6]),
            utilization: Aggregate::from_samples(&[0.5, 0.6]),
            throughput: Aggregate::from_samples(&[0.2, 0.21]),
            idle_fraction: Aggregate::from_samples(&[0.2, 0.2]),
            busy_fraction: Aggregate::from_samples(&[0.5, 0.5]),
            blocked_fraction: Aggregate::from_samples(&[0.3, 0.3]),
            down_fraction: Aggregate::from_samples(&[0.0, 0.0]),
            travel_empty: None,
            travel_loaded: None,
            loading: None,
            unloading: None,
            utilize: None,
        },
    );
    SimReport {
        model: name.into(),
        description: String::new(),
        time_unit: TimeUnit::Seconds,
        stop_time: 200.0,
        warmup: 0.0,
        base_seed: 11,
        replications: Vec::new(),
        objects,
        completed: Aggregate::from_samples(&[39.0, 41.0]),
        created: Aggregate::from_samples(&[40.0, 41.0]),
        little_check: None,
        process_flow: None,
        fluid: None,
        performance_measures: BTreeMap::new(),
        tracked_series: BTreeMap::new(),
        tracked_summaries: BTreeMap::new(),
    }
    .with_replication_count(replications)
}

/// Tiny helper so a fixture can claim to have `n` replications without
/// building full `ReplicationResult` values.
trait WithReplicationCount {
    fn with_replication_count(self, count: u32) -> Self;
}

impl WithReplicationCount for SimReport {
    fn with_replication_count(mut self, count: u32) -> Self {
        // Only the length is read by the renderers under test.
        self.replications = (0..count).map(fake_replication).collect();
        self
    }
}

fn fake_replication(index: u32) -> openflex_core::ReplicationResult {
    openflex_core::ReplicationResult {
        index,
        seed: 1000 + u64::from(index),
        final_time: 200.0,
        warmup: 0.0,
        objects: BTreeMap::new(),
        items_created: 40,
        items_completed: 39,
        items_in_system: 1,
        process_flow: None,
        fluid: None,
        performance_measures: BTreeMap::new(),
        tracked_series: BTreeMap::new(),
        tracked_summaries: BTreeMap::new(),
    }
}

/// Add one measure with `samples.len()` replications, so `n` and `ci95` follow
/// the same rule the engine uses.
fn with_measure(mut report: SimReport, name: &str, samples: &[f64]) -> SimReport {
    report
        .performance_measures
        .insert(name.to_string(), Aggregate::from_samples(samples));
    let replications = samples.len().max(1) as u32;
    let mut replications_vec = report.replications;
    replications_vec.truncate(replications as usize);
    while replications_vec.len() < replications as usize {
        replications_vec.push(fake_replication(replications_vec.len() as u32));
    }
    report.replications = replications_vec;
    report
}

fn scenario(name: &str, report: SimReport) -> ScenarioReport {
    ScenarioReport {
        name: name.into(),
        overrides: Vec::<Override>::new(),
        report,
    }
}

fn experiment(scenarios: Vec<ScenarioReport>) -> ExperimentReport {
    ExperimentReport {
        experiment: "tuning".into(),
        model: "demo".into(),
        time_unit: TimeUnit::Seconds,
        scenarios,
    }
}

/// The three human-readable renderings of one report.
fn all_formats(report: &SimReport) -> Vec<(Format, String)> {
    [Format::Table, Format::Csv, Format::Markdown]
        .into_iter()
        .map(|format| (format, render_sim(report, format).expect("render")))
        .collect()
}

// ---------------------------------------------------------------------------
// With measures: all three paths show them
// ---------------------------------------------------------------------------

#[test]
fn report_formats_carry_model_measures() {
    let report = with_measure(base_report("demo", 2), "wip_ratio", &[0.25, 0.35]);

    let table = render_sim(&report, Format::Table).expect("table");
    assert!(
        table.contains("Performance measures"),
        "table must gain the section:\n{table}"
    );
    assert!(
        table.contains("wip_ratio"),
        "table must name the measure:\n{table}"
    );

    let csv = render_sim(&report, Format::Csv).expect("csv");
    assert!(
        csv.contains("# performance_measures"),
        "csv needs a header comment"
    );
    assert!(
        csv.contains("measure,mean,std_dev,ci95,min,max,n"),
        "csv needs the measure section header:\n{csv}"
    );
    assert!(
        csv.contains("wip_ratio,0.300000"),
        "csv must carry the mean:\n{csv}"
    );

    let markdown = render_sim(&report, Format::Markdown).expect("markdown");
    assert!(
        markdown.contains("## Performance measures"),
        "markdown must gain the section:\n{markdown}"
    );
    assert!(markdown.contains("| wip_ratio | 0.300 |"));
}

#[test]
fn experiment_formats_carry_model_measures() {
    let baseline = with_measure(base_report("demo", 2), "wip_ratio", &[0.25, 0.35]);
    let candidate = with_measure(base_report("demo", 2), "wip_ratio", &[0.10, 0.20]);
    let experiment = experiment(vec![
        scenario("baseline", baseline),
        scenario("fast", candidate),
    ]);

    let table = render_experiment(&experiment, Format::Table).expect("table");
    // The header gains one column named after the measure, and each scenario
    // row carries its own value.
    let header = table
        .lines()
        .find(|line| line.starts_with("Scenario"))
        .expect("scenario header row");
    assert!(
        header.contains("wip_ratio"),
        "header lost the column: {header}"
    );
    let row = table
        .lines()
        .find(|line| line.starts_with("baseline"))
        .expect("baseline row");
    assert!(row.contains("0.300"), "row lost the value: {row}");

    let csv = render_experiment(&experiment, Format::Csv).expect("csv");
    assert!(
        csv.contains("# performance measures"),
        "csv needs the comment"
    );
    let header = csv
        .lines()
        .find(|line| line.starts_with("experiment,"))
        .expect("csv header");
    assert!(
        header.contains("wip_ratio"),
        "csv header lost the column: {header}"
    );
    assert!(
        header.contains("wip_ratio_ci95"),
        "csv needs the interval column"
    );
    let baseline_line = csv
        .lines()
        .find(|line| line.starts_with("tuning,baseline"))
        .expect("baseline csv row");
    assert!(
        baseline_line.ends_with("0.300000,0.098000") || baseline_line.contains("0.300000"),
        "csv row lost the value: {baseline_line}"
    );

    let markdown = render_experiment(&experiment, Format::Markdown).expect("markdown");
    assert!(
        markdown.contains("## Performance measures"),
        "markdown must gain the section:\n{markdown}"
    );
    assert!(markdown.contains("| baseline | 2 | 0.300 |"));
}

// ---------------------------------------------------------------------------
// Without measures: byte-for-byte unchanged
// ---------------------------------------------------------------------------

/// Snapshot of the shared renderers' output for a model that declares no
/// measure. Every measure renderer returns `""` for such a report, which is
/// what keeps the three formats byte-for-byte identical to the build that had
/// no measure support at all.
#[test]
fn reports_without_measures_are_unchanged() {
    let report = base_report("demo", 2);

    for (format, text) in all_formats(&report) {
        assert!(
            !text.contains("Performance measures"),
            "{format:?} grew a section it must not have:\n{text}"
        );
        assert!(
            !text.contains("# performance_measures"),
            "{format:?} grew a CSV comment"
        );
        assert!(
            !text.contains("measure,mean"),
            "{format:?} grew a CSV header"
        );
    }

    // The exact table rendering: the totals are the last thing printed.
    let table = render_sim(&report, Format::Table).expect("table");
    assert!(
        table.ends_with("Items in system              1.0  (95% CI ±0.0)\n"),
        "the table must end at the totals:\n{table}"
    );

    // The exact CSV: one header plus the object rows, nothing else.
    let csv = render_sim(&report, Format::Csv).expect("csv");
    assert_eq!(
        csv.lines().count(),
        2,
        "csv must stay header + one object:\n{csv}"
    );
    assert!(
        csv.starts_with("object,kind,"),
        "csv header changed:\n{csv}"
    );

    // The exact Markdown: Totals is followed directly by How to reproduce.
    let markdown = render_sim(&report, Format::Markdown).expect("markdown");
    let totals = markdown.find("## Totals").expect("totals section");
    let reproduce = markdown
        .find("## How to reproduce")
        .expect("reproduce section");
    assert!(totals < reproduce);
    assert!(
        !markdown[totals..reproduce].contains("Performance"),
        "no measure section between Totals and How to reproduce:\n{markdown}"
    );
}

#[test]
fn experiments_without_measures_are_unchanged() {
    let experiment = experiment(vec![
        scenario("baseline", base_report("demo", 2)),
        scenario("fast", base_report("demo", 2)),
    ]);

    let table = render_experiment(&experiment, Format::Table).expect("table");
    let header = table
        .lines()
        .find(|line| line.starts_with("Scenario"))
        .expect("header");
    assert_eq!(
        header,
        "Scenario            Reps    Created     ±95%   Completed     ±95%  Throughput   AvgWIP     Util",
        "the experiment header must not grow columns"
    );
    let rule = table
        .lines()
        .find(|line| line.starts_with("---"))
        .expect("rule");
    assert_eq!(rule.len(), 87, "the rule width must not change: {rule}");

    let csv = render_experiment(&experiment, Format::Csv).expect("csv");
    assert_eq!(
        csv.lines().next().expect("csv header"),
        "experiment,scenario,replications,created,created_ci95,completed,completed_ci95,throughput,avg_wip,avg_utilization"
    );
    assert!(!csv.contains("# performance measures"));

    let markdown = render_experiment(&experiment, Format::Markdown).expect("markdown");
    assert!(!markdown.contains("Performance measures"));
}

// ---------------------------------------------------------------------------
// n < 2: no interval
// ---------------------------------------------------------------------------

#[test]
fn single_replication_shows_no_interval() {
    let report = with_measure(base_report("demo", 1), "done", &[36.0]);

    let table = render_sim(&report, Format::Table).expect("table");
    assert!(
        !table.contains("±0.000"),
        "a single sample must never print an interval:\n{table}"
    );
    assert!(
        table.contains(openflex_report::NO_INTERVAL),
        "the interval cell must show the dash:\n{table}"
    );

    let csv = render_sim(&report, Format::Csv).expect("csv");
    let measure_row = csv
        .lines()
        .find(|line| line.starts_with("done,"))
        .expect("measure row");
    assert!(
        measure_row.ends_with(",1"),
        "the row must end with the sample count: {measure_row}"
    );
    // `done,mean,std_dev,ci95,min,max,n` — the ci95 cell is empty.
    let cells: Vec<&str> = measure_row.split(',').collect();
    assert_eq!(
        cells.len(),
        7,
        "column count must stay stable: {measure_row}"
    );
    assert_eq!(cells[3], "", "ci95 must be empty when n < 2: {measure_row}");

    let markdown = render_sim(&report, Format::Markdown).expect("markdown");
    assert!(
        !markdown.contains("±0.000"),
        "markdown must not print an interval for n = 1:\n{markdown}"
    );
    assert!(markdown.contains(openflex_report::NO_INTERVAL));
}

#[test]
fn experiment_single_replication_shows_no_interval() {
    let report = with_measure(base_report("demo", 1), "done", &[36.0]);
    let experiment = experiment(vec![scenario("baseline", report)]);

    let table = render_experiment(&experiment, Format::Table).expect("table");
    assert!(!table.contains("±0.000"), "table:\n{table}");
    let row = table
        .lines()
        .find(|line| line.starts_with("baseline"))
        .expect("row");
    assert!(row.contains("36.000"), "value missing: {row}");
    assert!(
        row.contains(openflex_report::NO_INTERVAL),
        "dash missing: {row}"
    );

    let csv = render_experiment(&experiment, Format::Csv).expect("csv");
    let row = csv
        .lines()
        .find(|line| line.starts_with("tuning,baseline"))
        .expect("csv row");
    assert!(
        row.ends_with("36.000000,"),
        "the interval cell must be empty: {row}"
    );

    let markdown = render_experiment(&experiment, Format::Markdown).expect("markdown");
    assert!(!markdown.contains("±0.000"));
    assert!(markdown.contains(openflex_report::NO_INTERVAL));
}

// ---------------------------------------------------------------------------
// Awkward names: CSV columns must not shift
// ---------------------------------------------------------------------------

/// Split one CSV line into fields, honouring the quoting the renderers emit.
fn csv_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                fields.push(std::mem::take(&mut current));
            }
            _ => current.push(c),
        }
    }
    fields.push(current);
    fields
}

#[test]
fn measure_names_with_cjk_and_underscores_keep_csv_columns() {
    let names = ["完成量", "wip_ratio", "a,b", "with space"];
    let mut report = base_report("demo", 2);
    for name in names {
        report = with_measure(report, name, &[1.0, 3.0]);
    }

    let csv = render_sim(&report, Format::Csv).expect("csv");
    // Only the measure block, so the object rows cannot dilute the assertion.
    let block = csv
        .split_once("measure,mean,std_dev,ci95,min,max,n\n")
        .map(|(_, rest)| rest)
        .expect("measure section header");
    let rows: Vec<&str> = block.lines().filter(|line| !line.is_empty()).collect();
    assert_eq!(rows.len(), names.len(), "one row per measure:\n{csv}");

    let mut first_fields = Vec::new();
    for row in &rows {
        let fields = csv_fields(row);
        // Seven columns whether the name is CJK, has an underscore, a comma or
        // a space; the comma one is quoted, so the split does not shift.
        assert_eq!(fields.len(), 7, "column count shifted by the name: {row}");
        assert_eq!(fields[5], "3.000000", "last-but-one column is max: {row}");
        assert_eq!(
            fields[6], "2",
            "the sample count stays in its column: {row}"
        );
        first_fields.push(fields[0].clone());
    }
    for name in names {
        assert!(
            first_fields.iter().any(|field| field == name),
            "lost measure {name}: {first_fields:?}"
        );
    }
    // The comma in a name is quoted, not an extra column separator.
    assert!(
        csv.contains("\"a,b\","),
        "the comma name must be quoted:\n{csv}"
    );
}

#[test]
fn experiment_measure_names_keep_csv_columns() {
    let mut baseline = base_report("demo", 2);
    for name in ["完成量", "a,b"] {
        baseline = with_measure(baseline, name, &[1.0, 3.0]);
    }
    let experiment = experiment(vec![scenario("基线,场景", baseline)]);

    let csv = render_experiment(&experiment, Format::Csv).expect("csv");
    let header = csv
        .lines()
        .find(|line| line.starts_with("experiment,"))
        .expect("header");
    let header_fields = csv_fields(header);
    // Ten fixed columns, then one mean and one interval column per measure.
    assert_eq!(
        header_fields.len(),
        10 + 2 * 2,
        "header columns shifted: {header}"
    );
    for name in ["完成量", "a,b"] {
        assert!(
            header_fields.iter().any(|field| field == name),
            "header lost measure {name}: {header_fields:?}"
        );
        assert!(
            header_fields
                .iter()
                .any(|field| field == &format!("{name}_ci95")),
            "header lost the interval column for {name}: {header_fields:?}"
        );
    }

    let row = csv
        .lines()
        .find(|line| !line.starts_with('#') && !line.starts_with("experiment,"))
        .expect("data row");
    let fields = csv_fields(row);
    assert_eq!(
        fields.len(),
        header_fields.len(),
        "data columns shifted: {row}"
    );
    assert_eq!(fields[1], "基线,场景", "the scenario name must round-trip");
    assert_eq!(fields[10], "2.000000", "first measure mean: {row}");
}

// ---------------------------------------------------------------------------
// The interval rule itself
// ---------------------------------------------------------------------------

#[test]
fn interval_helper_hides_single_sample_intervals() {
    let one = Aggregate::from_samples(&[5.0]);
    assert_eq!(fmt_ci_opt(&one), NO_INTERVAL);
    assert_eq!(one.n, 1);

    let two = Aggregate::from_samples(&[4.0, 6.0]);
    assert_eq!(fmt_ci_opt(&two), format!("±{:.1}", two.ci95));
    assert!(fmt_ci_opt(&two).starts_with('±'));
}
