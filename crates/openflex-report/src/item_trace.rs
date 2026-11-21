//! Item-level tracing: the path one flow item took through the model.
//!
//! The engine records every processed event into a [`TraceLog`]. Item-related
//! events carry `data["item"]`, the [`FlowItem::id`] of the item that moved
//! (`enter`, `exit`, `arrive`, `flow`, `process_done`, `transport_*`, ...). This
//! module regroups those events by id so the CLI can answer "which path did
//! *this* item take?" without guessing: the id is authoritative, and events
//! that cannot be attributed to one item simply have no `item` key.
//!
//! Ordering is `(time, seq)`: several events share a timestamp (a source emits
//! `arrive`, `enter` and `exit` at the same instant), and `seq` is the engine's
//! processing order, which keeps the path stable.
//!
//! [`FlowItem::id`]: openflex_core::FlowItem

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use openflex_core::{TraceEvent, TraceLog};
use serde::{Deserialize, Serialize};

use crate::{Format, ReportError, fmt_time, i18n};

/// One item and the ordered path it took.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemTrace {
    /// `FlowItem::id`, the same id space as `items_created`.
    pub item: u64,
    pub steps: Vec<ItemTraceStep>,
}

/// One step of an [`ItemTrace`]: what happened, where, and when.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemTraceStep {
    /// Model time of the event.
    pub time: f64,
    /// Engine processing order, used to order events that share a timestamp.
    pub seq: u64,
    /// Name of the object the event belongs to.
    pub object: String,
    /// Event kind (`enter`, `exit`, `process_done`, `transport_arrive`, ...).
    pub event: String,
}

impl ItemTrace {
    /// Number of distinct objects the item entered (duplicates collapsed).
    pub fn object_count(&self) -> usize {
        let mut names: Vec<&str> = self.steps.iter().map(|step| step.object.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        names.len()
    }

    /// Time between the first and the last recorded step (0 for a single step).
    pub fn span(&self) -> f64 {
        match (self.steps.first(), self.steps.last()) {
            (Some(first), Some(last)) => last.time - first.time,
            _ => 0.0,
        }
    }

    /// Model time of the first step of the path.
    pub fn started_at(&self) -> Option<f64> {
        self.steps.first().map(|step| step.time)
    }

    /// Model time of the last step.
    pub fn ended_at(&self) -> Option<f64> {
        self.steps.last().map(|step| step.time)
    }
}

/// Extract the item id an event was attributed to.
///
/// The engine only writes `item` when the ownership is unambiguous; a missing
/// key means "not attributable", which is different from item `0`, so the
/// event is skipped rather than guessed.
pub fn item_of(event: &TraceEvent) -> Option<u64> {
    let value = *event.data.get("item")?;
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    Some(value as u64)
}

/// Regroup the events of `log` by item id, oldest step first.
///
/// `items` restricts the result to those ids; `None` returns every traced item
/// in ascending id order. Ids that never appear are simply absent — callers
/// that asked for a specific id should treat an empty result as "not traced"
/// (see [`item_trace`], which makes that case explicit).
pub fn item_traces(log: &TraceLog, items: Option<&[u64]>) -> Vec<ItemTrace> {
    let mut grouped: BTreeMap<u64, Vec<ItemTraceStep>> = BTreeMap::new();
    for event in &log.events {
        let Some(item) = item_of(event) else { continue };
        if let Some(wanted) = items
            && !wanted.contains(&item)
        {
            continue;
        }
        grouped.entry(item).or_default().push(ItemTraceStep {
            time: event.time,
            seq: event.seq,
            object: event.object.clone(),
            event: event.event.clone(),
        });
    }

    grouped
        .into_iter()
        .map(|(item, mut steps)| {
            steps.sort_by(|a, b| a.time.total_cmp(&b.time).then_with(|| a.seq.cmp(&b.seq)));
            ItemTrace { item, steps }
        })
        .collect()
}

/// Path of one item, or `None` when the trace has no event for it.
pub fn item_trace(log: &TraceLog, item: u64) -> Option<ItemTrace> {
    item_traces(log, Some(&[item])).into_iter().next()
}

/// Every traced item with its step count, ascending by id.
pub fn traced_items(log: &TraceLog) -> Vec<(u64, usize)> {
    item_traces(log, None)
        .into_iter()
        .map(|trace| (trace.item, trace.steps.len()))
        .collect()
}

/// Number of events that carry an item id.
pub fn item_event_count(log: &TraceLog) -> usize {
    log.events
        .iter()
        .filter(|event| item_of(event).is_some())
        .count()
}

// ---------------------------------------------------------------------------
// Reading a trace file
// ---------------------------------------------------------------------------

/// Read a JSONL trace written by `openflex run --trace`.
///
/// One event per line; blank lines are ignored. The error names the file and
/// the 1-based line, which is the only way to find a truncated or hand-edited
/// trace.
pub fn read_trace_file(path: &Path) -> Result<TraceLog, ReportError> {
    let text = std::fs::read_to_string(path).map_err(|source| ReportError::Read {
        path: path.display().to_string(),
        source,
    })?;
    let mut log = TraceLog::default();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let event: TraceEvent =
            serde_json::from_str(line).map_err(|source| ReportError::TraceLine {
                path: path.display().to_string(),
                line: index + 1,
                source,
            })?;
        log.push(event);
    }
    Ok(log)
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render one item path in the requested format.
pub fn render_item_trace(trace: &ItemTrace, format: Format) -> Result<String, ReportError> {
    Ok(match format {
        Format::Table => item_trace_table(trace),
        Format::Markdown => item_trace_markdown(trace),
        Format::Csv => item_trace_csv(trace),
        Format::Json => {
            let mut text = serde_json::to_string_pretty(trace)?;
            text.push('\n');
            text
        }
    })
}

/// Render the list of traced items in the requested format.
pub fn render_item_list(log: &TraceLog, format: Format) -> Result<String, ReportError> {
    let items = traced_items(log);
    Ok(match format {
        Format::Json => {
            let mut text = serde_json::to_string_pretty(
                &items
                    .iter()
                    .map(|(item, steps)| serde_json::json!({"item": item, "steps": steps}))
                    .collect::<Vec<_>>(),
            )?;
            text.push('\n');
            text
        }
        Format::Csv => {
            let mut out = String::from("item,steps\n");
            for (item, steps) in &items {
                let _ = writeln!(out, "{item},{steps}");
            }
            out
        }
        Format::Markdown => {
            let mut out = String::new();
            let _ = writeln!(out, "# {}: {}", i18n::t("traced_items"), items.len());
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "{} {}, {} {}\n",
                i18n::t("trace_events"),
                log.events.len(),
                i18n::t("item_events"),
                item_event_count(log)
            );
            let _ = writeln!(out, "| {} | {} |", i18n::t("item"), i18n::t("steps"));
            let _ = writeln!(out, "|---:|---:|");
            for (item, steps) in &items {
                let _ = writeln!(out, "| {item} | {steps} |");
            }
            out
        }
        Format::Table => {
            let mut out = summary_header(log, items.len());
            let _ = writeln!(out);
            let _ = writeln!(out, "{:>8} {:>8}", i18n::t("item"), i18n::t("steps"));
            let _ = writeln!(out, "{}", "-".repeat(24));
            for (item, steps) in &items {
                let _ = writeln!(out, "{item:>8} {steps:>8}");
            }
            out
        }
    })
}

/// Render a trace summary when the caller did not ask for a specific item.
///
/// Deliberately compact: a trace can hold tens of thousands of events, and the
/// full path of every item would drown the terminal.
pub fn render_trace_summary(log: &TraceLog, format: Format) -> Result<String, ReportError> {
    let items = traced_items(log);
    let item_events = item_event_count(log);
    let (first, last) = time_range(log);
    let hint = format!(
        "openflex trace <file> --items  (list {} item{}), --item <id> (full path)",
        items.len(),
        if items.len() == 1 { "" } else { "s" }
    );
    Ok(match format {
        Format::Json => {
            let mut text = serde_json::to_string_pretty(&serde_json::json!({
                "events": log.events.len(),
                "item_events": item_events,
                "items": items.len(),
                "first_item": items.first().map(|(item, _)| *item),
                "last_item": items.last().map(|(item, _)| *item),
                "start": first,
                "end": last,
                "truncated": log.truncated,
                "hint": hint,
            }))?;
            text.push('\n');
            text
        }
        Format::Csv => {
            let mut out = String::from("events,item_events,items,start,end,truncated\n");
            let _ = writeln!(
                out,
                "{},{},{},{:.6},{:.6},{}",
                log.events.len(),
                item_events,
                items.len(),
                first.unwrap_or(0.0),
                last.unwrap_or(0.0),
                log.truncated
            );
            out
        }
        Format::Markdown => {
            let mut out = String::new();
            let _ = writeln!(out, "# {}", i18n::t("trace_summary"));
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "- {}: {}\n- {}: {}\n- {}: {}\n",
                i18n::t("trace_events"),
                log.events.len(),
                i18n::t("item_events"),
                item_events,
                i18n::t("traced_items"),
                items.len()
            );
            if let (Some(first), Some(last)) = (first, last) {
                let _ = writeln!(
                    out,
                    "{} {} … {}",
                    i18n::t("time_range"),
                    fmt_time(first),
                    fmt_time(last)
                );
            }
            if log.truncated {
                let _ = writeln!(out, "\n{}", i18n::t("trace_truncated"));
            }
            let _ = writeln!(out, "\n{}\n", hint);
            out
        }
        Format::Table => {
            let mut out = summary_header(log, items.len());
            if log.truncated {
                let _ = writeln!(out, "{}", i18n::t("trace_truncated"));
            }
            let _ = writeln!(out, "{hint}");
            out
        }
    })
}

fn time_range(log: &TraceLog) -> (Option<f64>, Option<f64>) {
    let mut first: Option<f64> = None;
    let mut last: Option<f64> = None;
    for event in &log.events {
        first = Some(first.map_or(event.time, |value| value.min(event.time)));
        last = Some(last.map_or(event.time, |value| value.max(event.time)));
    }
    (first, last)
}

fn summary_header(log: &TraceLog, items: usize) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}: {} {}, {} {}, {} {}",
        i18n::t("trace_summary"),
        log.events.len(),
        i18n::t("trace_events"),
        item_event_count(log),
        i18n::t("item_events"),
        items,
        i18n::t("traced_items_lower")
    );
    if let (Some(first), Some(last)) = time_range(log) {
        let _ = writeln!(
            out,
            "{}: {} … {}  ({})",
            i18n::t("time_range"),
            fmt_time(first),
            fmt_time(last),
            i18n::t("model_time_units")
        );
    }
    out
}

fn item_trace_table(trace: &ItemTrace) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{} {}", i18n::t("item_trace_prefix"), trace.item);
    let _ = writeln!(out, "{}", item_trace_summary_line(trace));
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{:>10}  {:<24} {}",
        i18n::t("time"),
        i18n::t("object"),
        i18n::t("event")
    );
    let _ = writeln!(out, "{}", "-".repeat(60));
    for step in &trace.steps {
        let _ = writeln!(
            out,
            "{:>10}  {}  {}",
            fmt_time(step.time),
            crate::pad_right(&step.object, 24),
            step.event
        );
    }
    out
}

fn item_trace_markdown(trace: &ItemTrace) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# {} {}", i18n::t("item_trace_prefix"), trace.item);
    let _ = writeln!(out);
    let _ = writeln!(out, "{}", item_trace_summary_line(trace));
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "| {} | {} | {} |",
        i18n::t("time"),
        i18n::t("object"),
        i18n::t("event")
    );
    let _ = writeln!(out, "|---:|---|---|");
    for step in &trace.steps {
        let _ = writeln!(
            out,
            "| {} | {} | {} |",
            fmt_time(step.time),
            markdown_cell(&step.object),
            markdown_cell(&step.event)
        );
    }
    out
}

fn item_trace_csv(trace: &ItemTrace) -> String {
    let mut out = String::from("time,object,event\n");
    for step in &trace.steps {
        let _ = writeln!(
            out,
            "{:.6},{},{}",
            step.time,
            csv_cell(&step.object),
            csv_cell(&step.event)
        );
    }
    out
}

fn item_trace_summary_line(trace: &ItemTrace) -> String {
    let separator = i18n::t("title_separator");
    format!(
        "{}{separator}{}  {}{separator}{}  {}{separator}{} {}",
        i18n::t("steps"),
        trace.steps.len(),
        i18n::t("objects_visited"),
        trace.object_count(),
        i18n::t("span"),
        fmt_time(trace.span()),
        i18n::t("model_time_units")
    )
}

fn markdown_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn csv_cell(value: &str) -> String {
    crate::csv_field(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(time: f64, seq: u64, object: &str, event: &str, item: Option<u64>) -> TraceEvent {
        let mut value = TraceEvent::new(time, seq, event, object);
        if let Some(item) = item {
            value = value.with("item", item as f64);
        }
        value
    }

    #[test]
    fn extracts_paths_per_item_in_time_order() {
        let mut log = TraceLog::default();
        // Written out of order on purpose: sorting is by (time, seq).
        log.push(event(2.0, 3, "B", "exit", Some(7)));
        log.push(event(0.0, 1, "A", "arrive", Some(7)));
        log.push(event(1.0, 2, "A", "enter", Some(7)));
        log.push(event(0.0, 4, "C", "arrive", Some(8)));

        let traces = item_traces(&log, None);
        assert_eq!(traces.len(), 2);
        assert_eq!(traces[0].item, 7);
        assert_eq!(
            traces[0]
                .steps
                .iter()
                .map(|step| step.event.as_str())
                .collect::<Vec<_>>(),
            ["arrive", "enter", "exit"]
        );

        let only = item_traces(&log, Some(&[8]));
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].item, 8);
        assert_eq!(only[0].steps.len(), 1);
    }

    #[test]
    fn ignores_events_without_an_item_id() {
        let mut log = TraceLog::default();
        log.push(event(0.0, 1, "T", "operator_loaded", None));
        log.push(event(1.0, 2, "P", "enter", Some(3)));
        assert_eq!(item_event_count(&log), 1);
        assert_eq!(traced_items(&log), vec![(3, 1)]);
    }
}
