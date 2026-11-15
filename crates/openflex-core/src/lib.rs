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

pub mod checkpoint;
pub mod conveyor;
pub mod engine;
pub mod error;
pub mod script;
pub mod shifts;
pub mod stats;
pub mod trace;

pub use checkpoint::{CHECKPOINT_FORMAT_VERSION, Checkpoint};
pub use engine::{
    EventPreview, FlowItem, ReplicationCheckpoint, SimulationHandle, SimulationState, derive_seed,
    resume_from_checkpoint, run_model, run_model_with_checkpoints, simulate, simulate_traced,
    simulate_until, simulate_with_checkpoint, simulate_with_checkpoint_traced,
};
pub use error::SimError;
pub use script::{
    CompiledScript, ScriptContext, ScriptEngine, ScriptError, ScriptMessage, ScriptOutcome,
    compile as compile_script,
};
pub use stats::{
    Aggregate, FlowBlockReport, FlowReport, FluidObjectReport, FluidReport, LittleCheck,
    LittleEntry, ObjectAggregate, ObjectStats, ReplicationFlow, ReplicationFlowBlock,
    ReplicationFluid, ReplicationFluidObject, ReplicationResult, SimConfig, SimReport, StateTimes,
    t_critical_95,
};
pub use trace::{MAX_TRACE_EVENTS, TraceEvent, TraceLog};
