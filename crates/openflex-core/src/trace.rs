//! Event tracing (B12).
//!
//! The engine can record every event it processes into a [`TraceLog`]. The log
//! is deliberately decoupled from [`crate::stats::ReplicationResult`], so a
//! normal run pays no cost and a host (for example the CLI's `--trace` flag)
//! can stream or serialise the events itself.
//!
//! ```
//! use openflex_core::{SimConfig, simulate_traced};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let model = openflex_model::Model::from_json(r#"{
//!   "format": "openflex-model", "version": 1, "name": "trace-demo",
//!   "run": {"stop_time": 10.0},
//!   "objects": [
//!     {"name": "S", "kind": "source", "params": {"interarrival": {"distribution": "constant", "value": 5.0}}},
//!     {"name": "K", "kind": "sink", "params": {}}
//!   ],
//!   "connections": [{"from": "S", "to": "K"}]
//! }"#)?;
//! let config = SimConfig { stop_time: 10.0, warmup: 0.0, seed: 1 };
//! let (result, trace) = simulate_traced(&model, &config, 0)?;
//! assert!(result.items_completed > 0);
//! assert!(!trace.events.is_empty());
//! # Ok(())
//! # }
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Maximum number of events a single trace keeps; further events only set
/// [`TraceLog::truncated`], so a tracing run cannot exhaust memory.
pub const MAX_TRACE_EVENTS: usize = 1_000_000;

/// One processed simulation event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceEvent {
    /// Model time of the event.
    pub time: f64,
    /// Insertion sequence number; `(time, seq)` is the processing order.
    pub seq: u64,
    /// Event kind (`arrive`, `flow`, `process_done`, `breakdown`, ...).
    pub event: String,
    /// Name of the object the event belongs to.
    pub object: String,
    /// Extra numeric fields (lane index, generation, ...), empty when unused.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub data: BTreeMap<String, f64>,
}

impl TraceEvent {
    pub fn new(time: f64, seq: u64, event: impl Into<String>, object: impl Into<String>) -> Self {
        Self {
            time,
            seq,
            event: event.into(),
            object: object.into(),
            data: BTreeMap::new(),
        }
    }

    /// Attach one numeric detail (builder style).
    pub fn with(mut self, key: &str, value: f64) -> Self {
        self.data.insert(key.to_string(), value);
        self
    }
}

/// The recorded events of one replication.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TraceLog {
    pub events: Vec<TraceEvent>,
    /// The run produced more events than [`MAX_TRACE_EVENTS`]; the tail is
    /// missing from `events`.
    pub truncated: bool,
}

impl TraceLog {
    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Push one event while enforcing [`MAX_TRACE_EVENTS`].
    pub fn push(&mut self, event: TraceEvent) {
        if self.events.len() >= MAX_TRACE_EVENTS {
            self.truncated = true;
            return;
        }
        self.events.push(event);
    }

    /// Events of one object, in processing order.
    pub fn object(&self, name: &str) -> impl Iterator<Item = &TraceEvent> {
        self.events.iter().filter(move |event| event.object == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_respects_the_limit() {
        let mut log = TraceLog::default();
        for seq in 0..MAX_TRACE_EVENTS {
            log.push(TraceEvent::new(0.0, seq as u64, "flow", "A"));
        }
        assert_eq!(log.len(), MAX_TRACE_EVENTS);
        assert!(!log.truncated);
        log.push(TraceEvent::new(0.0, 1, "flow", "A"));
        assert_eq!(log.len(), MAX_TRACE_EVENTS);
        assert!(log.truncated);
    }

    #[test]
    fn filters_by_object() {
        let mut log = TraceLog::default();
        log.push(TraceEvent::new(0.0, 0, "arrive", "S").with("quantity", 2.0));
        log.push(TraceEvent::new(1.0, 1, "flow", "Q"));
        assert_eq!(log.object("S").count(), 1);
        assert_eq!(
            log.object("S").next().map(|event| event.data["quantity"]),
            Some(2.0)
        );
    }
}
