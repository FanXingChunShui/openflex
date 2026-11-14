//! OpenFlex discrete-event simulation engine.
//!
//! ```
//! use openflex_core::run_model;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let model = openflex_model::Model::from_json(r#"{
//!   "format": "openflex-model", "version": 1, "name": "demo",
//!   "run": {"stop_time": 100.0},
//!   "objects": [
//!     {"name": "Source1", "kind": "source", "params": {"interarrival": {"distribution": "constant", "value": 5.0}, "max_arrivals": 10}},
//!     {"name": "Sink1", "kind": "sink", "params": {}}
//!   ],
//!   "connections": [{"from": "Source1", "to": "Sink1"}]
//! }"#)?;
//! let report = run_model(&model)?;
//! assert_eq!(report.replications.len(), 1);
//! assert!((report.completed.mean - 10.0).abs() < 1e-9);
//! # Ok(())
//! # }
//! ```

pub mod conveyor;
pub mod engine;
pub mod error;
pub mod shifts;
pub mod stats;
pub mod trace;

pub use engine::{FlowItem, derive_seed, run_model, simulate, simulate_traced};
pub use error::SimError;
pub use stats::{
    Aggregate, LittleCheck, LittleEntry, ObjectAggregate, ObjectStats, ReplicationResult,
    SimConfig, SimReport, StateTimes, t_critical_95,
};
pub use trace::{MAX_TRACE_EVENTS, TraceEvent, TraceLog};
