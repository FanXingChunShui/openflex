//! Terminal charts: bar geometry, the awkward-input messages, and alignment.
//!
//! The chart module only reads a `SimReport`, so these tests build small
//! reports by hand instead of running the engine: the geometry and the
//! fallbacks are what need pinning, not the simulation.

use openflex_core::{Aggregate, SimReport};
use openflex_model::ObjectKind;
use openflex_report::{
    ChartKind, ChartSeries, ChartSpec, MAX_WIDTH, MIN_WIDTH, ValueFormat, clamp_width,
    counts_for_utilization, measures_spec, render_chart, render_chart_markdown, utilization_spec,
};

/// One aggregate as JSON, so a fixture only names the mean it cares about.
fn agg(mean: f64, n: u32) -> serde_json::Value {
    serde_json::json!({
        "mean": mean,
        "std_dev": 0.0,
        "min": mean,
        "max": mean,
        "ci95": 0.0,
        "n": n,
    })
}

/// An object aggregate with every required column at `mean`.
fn object(kind: &str, mean: f64) -> serde_json::Value {
    serde_json::json!({
        "kind": kind,
        "input_count": agg(mean, 1),
        "output_count": agg(mean, 1),
        "avg_content": agg(mean, 1),
        "max_content": agg(mean, 1),
        "avg_stay_time": agg(mean, 1),
        "utilization": agg(mean, 1),
        "throughput": agg(mean, 1),
        "idle_fraction": agg(mean, 1),
        "busy_fraction": agg(mean, 1),
        "blocked_fraction": agg(0.0, 1),
        "down_fraction": agg(0.0, 1),
    })
}

/// Minimal report built through serde: the extra report fields keep growing,
/// and only the ones without `serde(default)` need to be listed here.
fn report_with_objects(objects: serde_json::Value) -> SimReport {
    let value = serde_json::json!({
        "model": "chart-demo",
        "description": "",
        "time_unit": "seconds",
        "stop_time": 100.0,
        "warmup": 0.0,
        "base_seed": 1,
        "replications": [],
        "objects": objects,
        "completed": agg(0.0, 1),
        "created": agg(0.0, 1),
    });
    serde_json::from_value(value).expect("fixture report")
}

fn report() -> SimReport {
    report_with_objects(serde_json::json!({}))
}

fn spec(labels: &[(&str, f64)], kind: ChartKind, width: usize) -> ChartSpec {
    ChartSpec {
        title: "demo".into(),
        series: labels
            .iter()
            .map(|(label, value)| ChartSeries::single(*label, *value))
            .collect(),
        kind,
        width,
        format: ValueFormat::Number,
    }
}

/// Number of bar cells on one rendered line (everything between the axis and
/// the value, ignoring the padding used to align values).
fn bar_cells(line: &str) -> usize {
    line.chars().filter(|c| *c == '█').count()
}

#[test]
fn bar_length_follows_the_value() {
    let text = render_chart(&spec(
        &[("full", 10.0), ("half", 5.0), ("zero", 0.0)],
        ChartKind::Bar,
        40,
    ));
    let lines: Vec<&str> = text.lines().collect();
    let full = lines
        .iter()
        .find(|line| line.starts_with("full"))
        .expect("full");
    let half = lines
        .iter()
        .find(|line| line.starts_with("half"))
        .expect("half");
    let zero = lines
        .iter()
        .find(|line| line.starts_with("zero"))
        .expect("zero");

    assert_eq!(
        bar_cells(full),
        40,
        "the largest value fills the width: {full}"
    );
    assert_eq!(
        bar_cells(half),
        20,
        "half the value is half the width: {half}"
    );
    assert_eq!(bar_cells(zero), 0, "zero is an empty bar: {zero}");
    // A tiny non-zero value still shows one cell, so it cannot read as zero.
    let small = render_chart(&spec(
        &[("full", 100.0), ("small", 0.1)],
        ChartKind::Bar,
        40,
    ));
    let small_line = small
        .lines()
        .find(|line| line.starts_with("small"))
        .expect("small");
    assert_eq!(
        bar_cells(small_line),
        1,
        "non-zero must be visible: {small_line}"
    );
}

#[test]
fn empty_and_all_zero_inputs_say_so() {
    let empty = render_chart(&spec(&[], ChartKind::Bar, 40));
    assert!(empty.contains("(no data)"), "{empty}");
    assert_eq!(
        empty.lines().count(),
        3,
        "title, rule and the message: {empty}"
    );

    let all_zero = render_chart(&spec(&[("a", 0.0), ("b", 0.0)], ChartKind::Bar, 40));
    assert!(
        all_zero.contains("all values are zero; nothing to draw"),
        "{all_zero}"
    );
    // The labels are still listed so the reader knows what was measured.
    assert!(
        all_zero.contains("a 0.000") && all_zero.contains("b 0.000"),
        "{all_zero}"
    );
}

#[test]
fn line_explains_single_points_and_constants() {
    let single = ChartSpec {
        title: "demo".into(),
        series: vec![ChartSeries::series("once", vec![(5.0, 3.0)])],
        kind: ChartKind::Line,
        width: 40,
        format: ValueFormat::Number,
    };
    let text = render_chart(&single);
    assert!(text.contains("single point"), "{text}");
    assert!(
        text.contains("t=5.00"),
        "the sample time is reported: {text}"
    );

    let constant = ChartSpec {
        title: "demo".into(),
        series: vec![ChartSeries::series(
            "flat",
            vec![(0.0, 2.0), (1.0, 2.0), (2.0, 2.0)],
        )],
        kind: ChartKind::Line,
        width: 40,
        format: ValueFormat::Number,
    };
    let text = render_chart(&constant);
    assert!(text.contains("constant, 2.000"), "{text}");
    assert!(
        !text.contains('▁'),
        "a constant series draws nothing: {text}"
    );
}

#[test]
fn line_draws_one_column_per_resampled_sample() {
    let points: Vec<(f64, f64)> = (0..10).map(|index| (index as f64, index as f64)).collect();
    let chart = ChartSpec {
        title: "ramp".into(),
        series: vec![ChartSeries::series("ramp", points)],
        kind: ChartKind::Line,
        width: 10,
        format: ValueFormat::Number,
    };
    let text = render_chart(&chart);
    let line = text.lines().last().expect("sparkline");
    assert!(
        line.contains('▁') && line.contains('█'),
        "lowest and highest levels appear: {line}"
    );
    // Levels must be monotonically non-decreasing for a rising ramp.
    let blocks: Vec<char> = line.chars().filter(|c| ('▁'..='█').contains(c)).collect();
    assert_eq!(blocks.len(), 10, "one column per sample: {line}");
    assert!(
        blocks.windows(2).all(|pair| pair[0] <= pair[1]),
        "{blocks:?}"
    );
}

#[test]
fn cjk_labels_do_not_shift_the_axis() {
    let text = render_chart(&spec(
        &[("服务台", 3.0), ("Proc1", 6.0)],
        ChartKind::Bar,
        20,
    ));
    let axis_columns: Vec<usize> = text
        .lines()
        .filter(|line| line.contains('▏'))
        .map(|line| {
            let prefix = line.split('▏').next().unwrap_or_default();
            openflex_report::display_width(prefix)
        })
        .collect();
    assert_eq!(axis_columns.len(), 2);
    assert_eq!(
        axis_columns[0], axis_columns[1],
        "bars start in the same column: {text}"
    );
}

#[test]
fn width_is_clamped_to_the_supported_range() {
    assert_eq!(clamp_width(1), MIN_WIDTH);
    assert_eq!(clamp_width(0), MIN_WIDTH);
    assert_eq!(clamp_width(40), 40);
    assert_eq!(clamp_width(usize::MAX), MAX_WIDTH);

    // The clamp is applied by the renderer too, not only by the CLI.
    let text = render_chart(&spec(&[("a", 1.0), ("b", 2.0)], ChartKind::Bar, 1));
    assert_eq!(
        bar_cells(text.lines().find(|l| l.starts_with('b')).expect("b")),
        MIN_WIDTH
    );
    let text = render_chart(&spec(&[("a", 1.0), ("b", 2.0)], ChartKind::Bar, 1000));
    assert_eq!(
        bar_cells(text.lines().find(|l| l.starts_with('b')).expect("b")),
        MAX_WIDTH
    );
}

#[test]
fn measures_carry_sample_count_and_interval_note() {
    let mut report = report();
    report
        .performance_measures
        .insert("many".into(), Aggregate::from_samples(&[1.0, 3.0]));
    report
        .performance_measures
        .insert("one".into(), Aggregate::from_samples(&[7.0]));
    let spec = measures_spec(&report, 40);
    assert_eq!(spec.kind, ChartKind::Bar);
    let many = spec
        .series
        .iter()
        .find(|series| series.label == "many")
        .expect("many");
    let one = spec
        .series
        .iter()
        .find(|series| series.label == "one")
        .expect("one");
    assert!(
        many.note.contains("95% CI ±"),
        "n=2 shows the interval: {}",
        many.note
    );
    assert!(
        one.note.contains('—'),
        "n=1 shows the dash, not ±0.000: {}",
        one.note
    );
    assert!(
        !one.note.contains("±"),
        "no fake interval for n=1: {}",
        one.note
    );
}

#[test]
fn utilization_only_asks_the_objects_that_have_capacity() {
    for kind in [
        ObjectKind::Processor,
        ObjectKind::Conveyor,
        ObjectKind::Rack,
    ] {
        assert!(counts_for_utilization(kind), "{kind:?} has capacity");
    }
    for kind in [ObjectKind::Source, ObjectKind::Sink, ObjectKind::Queue] {
        assert!(
            !counts_for_utilization(kind),
            "{kind:?} has no capacity signal"
        );
    }

    let report = report_with_objects(serde_json::json!({
        "Proc1": object("processor", 0.5),
        "Queue1": object("queue", 0.5),
        "Source1": object("source", 0.5),
        "Sink1": object("sink", 0.5),
    }));
    let spec = utilization_spec(&report, 40);
    let labels: Vec<&str> = spec
        .series
        .iter()
        .map(|series| series.label.as_str())
        .collect();
    assert_eq!(
        labels,
        ["Proc1"],
        "queues, sources and sinks must not be charted"
    );
    assert_eq!(spec.format, ValueFormat::Percent);
}

#[test]
fn markdown_wraps_the_drawing_in_a_fence() {
    let text = render_chart_markdown(&spec(&[("a", 1.0)], ChartKind::Bar, 20));
    assert!(text.starts_with("## demo"), "{text}");
    assert!(text.contains("```"), "{text}");
    assert!(text.contains('█'), "{text}");
    assert!(text.trim_end().ends_with("```"), "{text}");
}
