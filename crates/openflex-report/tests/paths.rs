//! Cross-item path aggregation (`openflex trace --paths`).
//!
//! The grouping rule is deliberately narrow: take each item's object sequence,
//! collapse **adjacent** duplicates, and group equal sequences. The tests pin
//! the boundary that matters most — an item that returns to an object it left
//! earlier keeps both visits, because that round trip is real traffic and the
//! whole point of the view is to tell routes apart.

use openflex_core::TraceLog;
use openflex_report::{
    Format, ItemTrace, ItemTraceStep, PathGroup, compress_path, item_traces, path_groups,
    render_path_groups,
};

fn step(time: f64, object: &str, event: &str) -> ItemTraceStep {
    ItemTraceStep {
        time,
        seq: 0,
        object: object.into(),
        event: event.into(),
    }
}

fn trace(item: u64, steps: Vec<ItemTraceStep>) -> ItemTrace {
    ItemTrace { item, steps }
}

/// A full source → processor → sink pass, with the enter/exit noise a real
/// trace carries.
fn pass(item: u64, start: f64) -> ItemTrace {
    trace(
        item,
        vec![
            step(start, "S", "arrive"),
            step(start, "S", "enter"),
            step(start, "S", "exit"),
            step(start + 1.0, "P", "enter"),
            step(start + 2.0, "P", "process_done"),
            step(start + 2.0, "P", "exit"),
            step(start + 2.0, "K", "enter"),
            step(start + 2.0, "K", "exit"),
        ],
    )
}

#[test]
fn compresses_adjacent_duplicates_only() {
    let steps = vec![
        step(0.0, "S", "arrive"),
        step(0.0, "S", "enter"),
        step(0.0, "S", "exit"),
        step(1.0, "P", "enter"),
        step(2.0, "P", "process_done"),
        step(3.0, "K", "enter"),
    ];
    assert_eq!(compress_path(&steps), ["S", "P", "K"]);
}

#[test]
fn keeps_a_return_visit_that_is_not_adjacent() {
    // S → P → S: the two `S` visits are not adjacent, so both survive. Only a
    // global dedupe would collapse this to S → P, and that would hide the
    // round trip an analyst is looking for.
    let steps = vec![
        step(0.0, "S", "exit"),
        step(1.0, "P", "enter"),
        step(2.0, "P", "exit"),
        step(3.0, "S", "enter"),
        step(4.0, "S", "exit"),
    ];
    assert_eq!(compress_path(&steps), ["S", "P", "S"]);
}

#[test]
fn flow_events_do_not_invent_a_return_visit() {
    // The engine records `flow` on the *sender* after the item has left it.
    // S → K with a trailing `flow S` must stay S → K; counting the sender
    // would make a plain pass look like a round trip.
    let with_flow = trace(
        1,
        vec![
            step(0.0, "S", "arrive"),
            step(0.0, "S", "enter"),
            step(0.0, "S", "exit"),
            step(0.0, "K", "enter"),
            step(0.0, "K", "exit"),
            step(0.0, "S", "flow"),
        ],
    );
    assert_eq!(
        openflex_report::visit_sequence(&with_flow),
        ["S", "K"],
        "the sender-only `flow` event must not be part of the route"
    );
    // `compress_path` itself is a pure adjacent dedupe, so the caller decides
    // which events describe a visit.
    assert_eq!(compress_path(&with_flow.steps), ["S", "K", "S"]);
}

#[test]
fn groups_items_that_took_the_same_path() {
    let groups = path_groups(&[pass(1, 0.0), pass(2, 10.0), pass(3, 20.0)]);
    assert_eq!(groups.len(), 1, "one route");
    let group = &groups[0];
    assert_eq!(group.path, ["S", "P", "K"]);
    assert_eq!(group.items, [1, 2, 3]);
    assert_eq!(group.count(), 3);
    // Span: earliest first step to latest last step across the group.
    assert_eq!(group.span, (0.0, 22.0));
}

#[test]
fn sorts_by_count_then_path_and_covers_every_item() {
    let mut traces = vec![
        pass(1, 0.0),
        pass(2, 1.0),
        // A detour through A: only one item took it.
        trace(
            3,
            vec![
                step(0.0, "S", "exit"),
                step(1.0, "A", "enter"),
                step(2.0, "K", "enter"),
            ],
        ),
        // Another detour through B: one item as well.
        trace(
            4,
            vec![
                step(0.0, "S", "exit"),
                step(1.0, "B", "enter"),
                step(2.0, "K", "enter"),
            ],
        ),
    ];
    traces.sort_by_key(|trace| trace.item);

    let groups = path_groups(&traces);
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].count(), 2, "the commonest route first");
    assert_eq!(groups[0].path, ["S", "P", "K"]);
    assert_eq!(groups[1].path, ["S", "A", "K"]);
    assert_eq!(
        groups[2].path,
        ["S", "B", "K"],
        "ties break lexicographically"
    );

    // Coverage is the deliverable's sanity check: every traced item belongs to
    // exactly one group.
    let grouped: usize = groups.iter().map(PathGroup::count).sum();
    assert_eq!(grouped, traces.len(), "sum of group sizes == traced items");
}

#[test]
fn renders_the_groups_in_every_format() {
    let log = log_with(&[pass(1, 0.0), pass(2, 10.0)]);
    let groups = path_groups(&item_traces(&log, None));
    assert_eq!(groups.len(), 1);

    let table = render_path_groups(&log, Format::Table).expect("table");
    assert!(table.contains("Path groups:"), "{table}");
    assert!(table.contains("2/2 items grouped"), "{table}");
    assert!(table.contains("coverage 100.0%"), "{table}");
    assert!(table.contains("S → P → K"), "{table}");
    assert!(table.contains("1, 2"), "sample items: {table}");

    let markdown = render_path_groups(&log, Format::Markdown).expect("markdown");
    assert!(
        markdown.contains("| Count | Path | Sample items | First | Last |"),
        "{markdown}"
    );
    assert!(markdown.contains("| 2 | S → P → K |"), "{markdown}");

    let csv = render_path_groups(&log, Format::Csv).expect("csv");
    assert_eq!(
        csv.lines().collect::<Vec<_>>(),
        [
            "count,path,items,first_time,last_time",
            "2,S -> P -> K,1;2,0.000000,12.000000",
        ]
    );

    let json = render_path_groups(&log, Format::Json).expect("json");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("json");
    assert_eq!(parsed["traced_items"], 2);
    assert_eq!(parsed["grouped_items"], 2);
    assert_eq!(parsed["coverage"], 1.0);
    assert_eq!(
        parsed["groups"][0]["path"],
        serde_json::json!(["S", "P", "K"])
    );
    assert_eq!(parsed["groups"][0]["count"], 2);
}

#[test]
fn untagged_traces_say_why_there_is_nothing_to_group() {
    let mut log = TraceLog::default();
    log.push(openflex_core::TraceEvent::new(
        0.0,
        0,
        "operator_loaded",
        "T",
    ));

    for format in [Format::Table, Format::Markdown, Format::Json] {
        let text = render_path_groups(&log, format).expect("render");
        assert!(
            text.contains("no event") || text.contains("no_tagged") || text.contains("[]"),
            "{format:?} must explain the empty result:\n{text}"
        );
    }
    assert!(
        render_path_groups(&log, Format::Table)
            .expect("table")
            .contains("no event in this trace is tagged")
    );
    assert_eq!(path_groups(&item_traces(&log, None)).len(), 0);
}

/// Build a `TraceLog` whose events mirror the given item traces.
fn log_with(traces: &[ItemTrace]) -> TraceLog {
    let mut log = TraceLog::default();
    let mut seq = 0u64;
    for trace in traces {
        for step in &trace.steps {
            log.push(
                openflex_core::TraceEvent::new(
                    step.time,
                    seq,
                    step.event.clone(),
                    step.object.clone(),
                )
                .with("item", trace.item as f64),
            );
            seq += 1;
        }
    }
    log
}
