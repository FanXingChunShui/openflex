//! Simulation checkpoints (B12).
//!
//! A checkpoint captures everything a run needs to continue later: the event
//! calendar (including events scheduled after the stop time), every object's
//! runtime state, both random streams, the counters and the accumulated
//! statistics. Restoring a checkpoint and running to a later stop time gives
//! exactly the same [`crate::stats::ReplicationResult`] as a single run to that
//! stop time with the same seed and warm-up.
//!
//! The snapshot covers the event calendar (including events after the saved
//! stop time), all object state (belt positions, station dwells, shift pauses,
//! operator tasks, pull/downtime bookkeeping), both `ChaCha12` streams and the
//! statistical accumulators. Infinite timers are stored as `null`; the JSON
//! parser must preserve f64 precision (the crate enables
//! `serde_json/float_roundtrip` for this).
//!
//! Checkpoints do not embed the model: the caller passes the same
//! [`openflex_model::Model`] again and the engine verifies its name and object
//! list. Tracing is not part of a checkpoint; a resumed run starts with an
//! empty trace (and [`crate::simulate_with_checkpoint_traced`] rejects the
//! combination).
//!
//! ```
//! use openflex_core::{SimConfig, simulate_with_checkpoint};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let model = openflex_model::Model::from_json(r#"{
//!   "format": "openflex-model", "version": 1, "name": "checkpoint-demo",
//!   "objects": [
//!     {"name": "S", "kind": "source", "params": {
//!         "interarrival": {"distribution": "exponential", "mean": 5.0}}},
//!     {"name": "Q", "kind": "queue", "params": {}},
//!     {"name": "P", "kind": "processor", "params": {
//!         "process_time": {"distribution": "exponential", "mean": 4.0}}},
//!     {"name": "K", "kind": "sink", "params": {}}
//!   ],
//!   "connections": [
//!     {"from": "S", "to": "Q"}, {"from": "Q", "to": "P"}, {"from": "P", "to": "K"}
//!   ]
//! }"#)?;
//! let first = SimConfig { stop_time: 100.0, warmup: 0.0, seed: 7 };
//! let (_, checkpoint) = simulate_with_checkpoint(&model, &first, 0, None)?;
//! let json = checkpoint.to_json()?;
//! let checkpoint = openflex_core::Checkpoint::from_json(&json)?;
//! let second = SimConfig { stop_time: 200.0, ..first };
//! let (resumed, _) = simulate_with_checkpoint(&model, &second, 0, Some(&checkpoint))?;
//! let (direct, _) = simulate_with_checkpoint(&model, &second, 0, None)?;
//! assert_eq!(resumed, direct);
//! # Ok(())
//! # }
//! ```

use rand_chacha::ChaCha12Rng;
use serde::{Deserialize, Serialize};

use crate::engine::{Event, ExplicitState, FlowRuntime, FluidRuntime, ObjectRt};
use crate::error::SimError;
use crate::stats::SimConfig;

/// Version of the checkpoint payload. Bumped whenever the snapshot layout
/// changes; [`Checkpoint::from_json`] refuses other versions.
pub const CHECKPOINT_FORMAT_VERSION: u32 = 1;

/// A resumable snapshot of one replication.
///
/// The payload is JSON serialisable; use [`Checkpoint::to_json`] and
/// [`Checkpoint::from_json`] so the format version is validated.
///
/// Compatibility policy: `explicit_routes` was added after the initial
/// checkpoint layout. Older files that omit it deserialize with an empty map,
/// which is deliberately interpreted as "the explicit chain starts from
/// scratch"; `format_version` is unchanged because the addition is
/// backward-compatible and the old failure mode was already a chain restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Layout version, see [`CHECKPOINT_FORMAT_VERSION`].
    pub format_version: u32,
    /// Name of the model that produced the snapshot.
    pub model_name: String,
    /// Model time of the snapshot (the stop time of the saved run).
    pub now: f64,

    /// Object names in model order; used to refuse a mismatched model.
    pub(crate) object_names: Vec<String>,
    /// Next event sequence number.
    pub(crate) next_seq: u64,
    /// The remaining event calendar, ordered by `(time, seq)` on restore.
    pub(crate) events: Vec<Event>,
    /// Runtime state of every object, in model order.
    pub(crate) objects: Vec<ObjectRt>,
    /// General random stream.
    pub(crate) rng: ChaCha12Rng,
    /// Arrival-only random stream.
    pub(crate) arrival_rng: ChaCha12Rng,
    /// Total items created (internal ids).
    pub(crate) items_created: u64,
    /// Items created inside the statistical window.
    pub(crate) stats_created: u64,
    /// Items completed inside the statistical window.
    pub(crate) stats_completed: u64,
    /// Processed event counter (feeds the safety valve).
    pub(crate) event_count: u64,
    /// Process-flow token runtime (B7); `None` when the model has no flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) process_flow: Option<FlowRuntime>,
    /// Fluid sub-system (B5); `None` when the model has no fluid objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fluid: Option<FluidRuntime>,
    /// task-183: explicit task-chain state, keyed by object name. Older
    /// checkpoints omit this field; restoring them is explicitly the
    /// "chain starts from scratch" compatibility policy.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub(crate) explicit_routes: std::collections::BTreeMap<String, ExplicitState>,
    /// Run settings of the saved run.
    pub(crate) config: SimConfig,
}

impl Checkpoint {
    /// Serialise to pretty JSON.
    pub fn to_json(&self) -> Result<String, SimError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| SimError::Config(format!("could not serialise checkpoint: {error}")))
    }

    /// Parse a checkpoint and validate its format version.
    /// task-77: parse a checkpoint and, when `expected_sha256` is given,
    /// verify the SHA-256 of `text` against it first (the value a manifest
    /// entry records). A mismatch is reported as a configuration error.
    pub fn from_json_verified(text: &str, expected_sha256: Option<&str>) -> Result<Self, SimError> {
        if let Some(expected) = expected_sha256 {
            let actual = crate::sha256::hex(text.as_bytes());
            if !actual.eq_ignore_ascii_case(expected) {
                return Err(SimError::Config(format!(
                    "checkpoint checksum mismatch: expected {expected}, got {actual}"
                )));
            }
        }
        Self::from_json(text)
    }

    /// task-77: SHA-256 of this checkpoint's JSON text (hex).
    pub fn sha256_of(text: &str) -> String {
        crate::sha256::hex(text.as_bytes())
    }

    pub fn from_json(text: &str) -> Result<Self, SimError> {
        let checkpoint: Checkpoint = serde_json::from_str(text)
            .map_err(|error| SimError::Config(format!("invalid checkpoint JSON: {error}")))?;
        if checkpoint.format_version != CHECKPOINT_FORMAT_VERSION {
            return Err(SimError::Config(format!(
                "unsupported checkpoint format version {} (this build supports {})",
                checkpoint.format_version, CHECKPOINT_FORMAT_VERSION
            )));
        }
        Ok(checkpoint)
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn object_names(&self) -> &[String] {
        &self.object_names
    }

    /// Model time of the snapshot.
    pub fn now(&self) -> f64 {
        self.now
    }

    /// Stop time of the run that produced the snapshot.
    pub fn stop_time(&self) -> f64 {
        self.config.stop_time
    }

    pub fn warmup(&self) -> f64 {
        self.config.warmup
    }

    /// Derived random seed of the replication (not the model's base seed).
    pub fn seed(&self) -> u64 {
        self.config.seed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_json_is_reported() {
        let error = Checkpoint::from_json("{}").expect_err("an empty object is not a checkpoint");
        assert!(
            error.to_string().contains("invalid checkpoint JSON"),
            "{error}"
        );
    }
}
