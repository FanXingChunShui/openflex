//! Item Trace: extracting and rendering the path of one flow item.
//!
//! The engine tags item-related trace events with `data["item"]` (the
//! `FlowItem::id`). These tests pin the extraction contract — grouping by id,
//! `(time, seq)` ordering, ignoring untagged events — and the rendering of the
//! three surfaces the CLI exposes.

use openflex_core::{TraceEvent, TraceLog};
use openflex_report::{
    Format, ItemTrace, ItemTraceStep, item_event_count, item_of, item_trace, item_traces,
    render_item_list, render_item_trace, render_trace_summary, traced_items,
};

fn event(time: f64, seq: u64, object: &str, event: &str, item: Option<u64>) -> TraceEvent {
    let mut value = TraceEvent::new(time, seq, event, object);
    if let Some(item) = item {
        value = value.with("item", item as f64);
    }
    value
}

/// A trace that looks like a real run: one item through S → P → K, plus a
/// second item created later, plus events that are not attributable to an item.
fn sample_log() -> TraceLog {
    let mut log = TraceLog::default();
    // item 1: five steps, two of them at the same timestamp.
    log.push(event(0.0, 0, "S", "arrive", Some(1)));
    log.push(event(0.0, 1, "S", "enter", Some(1)));
    log.push(event(0.0, 2, "S", "exit", Some(1)));
    log.push(event(5.0, 8, "P", "enter", Some(1)));
    log.push(event(7.0, 9, "P", "process_done", Some(1)));
    // item 2: still in P when the trace ended.
    log.push(event(3.0, 4, "S", "arrive", Some(2)));
    log.push(event(3.0, 5, "S", "enter", Some(2)));
    // Events that belong to no single item must be ignored.
    log.push(event(1.0, 3, "T", "operator_loaded", None));
    log.push(event(2.0, 6, "Dispatcher", "preempt_retry", None));
    // Written out of order on purpose: extraction sorts by (time, seq).
    log.events.reverse();
    log
}

#[test]
fn groups_steps_per_item_in_time_order() {
    let log = sample_log();
    let traces = item_traces(&log, None);
    assert_eq!(traces.len(), 2, "two tagged items");
    assert_eq!(traces[0].item, 1, "ascending id order");
    assert_eq!(traces[1].item, 2);

    let item1 = &traces[0];
    assert_eq!(item1.steps.len(), 5, "one step per tagged event");
    let times: Vec<f64> = item1.steps.iter().map(|step| step.time).collect();
    assert!(
        times.windows(2).all(|pair| pair[0] <= pair[1]),
        "ascending time: {times:?}"
    );
    assert_eq!(item1.steps[0].event, "arrive");
    assert_eq!(item1.steps[4].event, "process_done");
    // `S enter` and `S exit` share t=0; seq keeps them in processing order.
    assert_eq!(item1.steps[1].event, "enter");
    assert_eq!(item1.steps[2].event, "exit");
}

#[test]
fn filters_to_the_requested_ids() {
    let log = sample_log();
    let only = item_traces(&log, Some(&[2]));
    assert_eq!(only.len(), 1);
    assert_eq!(only[0].item, 2);
    assert_eq!(only[0].steps.len(), 2);

    // A filter with an id that never appears yields nothing; the caller turns
    // that into an explicit error, it is never an empty table.
    assert!(item_traces(&log, Some(&[404])).is_empty());
    assert!(item_trace(&log, 404).is_none());
}

#[test]
fn traced_items_matches_the_tagged_events() {
    let log = sample_log();
    let items = traced_items(&log);
    assert_eq!(items, vec![(1, 5), (2, 2)]);
    assert_eq!(
        item_event_count(&log),
        items.iter().map(|(_, steps)| steps).sum::<usize>()
    );
    // Untagged events are not item events and must not invent ids.
    assert_eq!(item_event_count(&log), 7);
    assert!(
        log.events
            .iter()
            .filter(|event| item_of(event).is_none())
            .count()
            == 2
    );
}

#[test]
fn item_metrics_count_distinct_objects_and_span() {
    let log = sample_log();
    let trace = item_trace(&log, 1).expect("item 1");
    assert_eq!(trace.steps.len(), 5);
    // S, P — two distinct objects even though S appears three times.
    assert_eq!(trace.object_count(), 2);
    assert_eq!(trace.span(), 7.0);
    assert_eq!(trace.started_at(), Some(0.0));
    assert_eq!(trace.ended_at(), Some(7.0));
}

#[test]
fn summary_does_not_print_every_step() {
    let log = sample_log();
    let text = render_trace_summary(&log, Format::Table).expect("summary");
    assert!(text.contains("Trace summary"), "{text}");
    assert!(text.contains("2 traced items"), "{text}");
    assert!(
        !text.contains("  Object"),
        "the summary must not print the step table:\n{text}"
    );
    assert!(!text.contains("process_done"), "no step lines:\n{text}");
    assert!(
        text.contains("--items"),
        "the summary must say how to continue:\n{text}"
    );
}

#[test]
fn renders_one_item_in_every_format() {
    let log = sample_log();
    let trace = item_trace(&log, 1).expect("item 1");

    let table = render_item_trace(&trace, Format::Table).expect("table");
    assert!(table.contains("Item trace: item 1"), "{table}");
    assert!(table.contains("Steps: 5"), "{table}");
    assert!(table.contains("Objects visited: 2"), "{table}");
    assert!(table.contains("process_done"), "{table}");

    let markdown = render_item_trace(&trace, Format::Markdown).expect("markdown");
    assert!(markdown.contains("| Time | Object | Event |"), "{markdown}");
    assert!(
        markdown.contains("| 7.00 | P | process_done |"),
        "{markdown}"
    );

    let csv = render_item_trace(&trace, Format::Csv).expect("csv");
    assert_eq!(csv.lines().count(), 1 + 5, "header plus five steps");

    let json = render_item_trace(&trace, Format::Json).expect("json");
    let parsed: ItemTrace = serde_json::from_str(&json).expect("round trip");
    assert_eq!(parsed, trace);
}

#[test]
fn renders_the_item_list() {
    let log = sample_log();
    let table = render_item_list(&log, Format::Table).expect("table");
    assert!(table.contains("2 traced items"), "{table}");
    assert!(
        table.contains("Steps"),
        "the list needs a column header:\n{table}"
    );
    assert!(
        table.lines().any(|line| line.trim() == "1        5"),
        "{table}"
    );

    let csv = render_item_list(&log, Format::Csv).expect("csv");
    assert_eq!(
        csv.lines().collect::<Vec<_>>(),
        ["item,steps", "1,5", "2,2"]
    );

    let markdown = render_item_list(&log, Format::Markdown).expect("markdown");
    assert!(markdown.contains("| 1 | 5 |"), "{markdown}");

    let json = render_item_list(&log, Format::Json).expect("json");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("json");
    assert_eq!(parsed[0]["item"], 1);
    assert_eq!(parsed[0]["steps"], 5);
}

#[test]
fn reads_a_jsonl_trace_file() {
    let directory = std::env::temp_dir().join(format!(
        "openflex-item-trace-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&directory).expect("temp dir");
    let path = directory.join("trace.jsonl");
    std::fs::write(
        &path,
        concat!(
            "{\"time\":0.0,\"seq\":0,\"event\":\"arrive\",\"object\":\"S\",\"data\":{\"item\":1.0}}\n",
            "\n",
            "{\"time\":1.0,\"seq\":1,\"event\":\"enter\",\"object\":\"P\",\"data\":{\"item\":1.0}}\n",
        ),
    )
    .expect("write trace");

    let log = openflex_report::read_trace_file(&path).expect("read trace");
    assert_eq!(log.len(), 2, "blank lines are skipped");
    let trace = item_trace(&log, 1).expect("item 1");
    assert_eq!(trace.steps.len(), 2);

    // A malformed line must name the file and the 1-based line number.
    std::fs::write(
        &path,
        "{\"time\":0.0,\"seq\":0,\"event\":\"arrive\",\"object\":\"S\"}\nnot-json\n",
    )
    .expect("write broken trace");
    let error = openflex_report::read_trace_file(&path).expect_err("broken trace");
    let message = error.to_string();
    assert!(message.contains("trace.jsonl"), "{message}");
    assert!(message.contains("line 2"), "{message}");

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn step_carries_the_source_event_verbatim() {
    let log = sample_log();
    let trace = item_trace(&log, 2).expect("item 2");
    assert_eq!(
        trace.steps[0],
        ItemTraceStep {
            time: 3.0,
            seq: 4,
            object: "S".into(),
            event: "arrive".into(),
        }
    );
}
