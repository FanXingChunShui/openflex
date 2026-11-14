//! Simulation results and their aggregation across replications.
//!
//! The engine reports one [`ReplicationResult`] per independent replication and
//! a [`SimReport`] that aggregates them. All times are model-time seconds; the
//! CLI converts them to the model's time unit for display.

use std::collections::BTreeMap;

use openflex_model::{ObjectKind, RunSettings, TimeUnit};
use serde::{Deserialize, Serialize};

/// Simulation clock configuration for one replication.
#[derive(Debug, Clone, PartialEq)]
pub struct SimConfig {
    pub stop_time: f64,
    pub warmup: f64,
    pub seed: u64,
}

impl SimConfig {
    pub fn from_run_settings(run: &RunSettings, seed: u64) -> Self {
        Self { stop_time: run.stop_time, warmup: run.warmup, seed }
    }
}

impl Default for SimConfig {
    fn default() -> Self {
        Self { stop_time: 1_000.0, warmup: 0.0, seed: 1 }
    }
}

/// Time spent by an object in each of the standard states.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct StateTimes {
    pub idle: f64,
    pub busy: f64,
    pub blocked: f64,
    pub down: f64,
    pub off_shift: f64,
}

impl StateTimes {
    pub fn total(&self) -> f64 {
        self.idle + self.busy + self.blocked + self.down + self.off_shift
    }

    pub fn fraction(&self, state: &str) -> f64 {
        let total = self.total();
        if total <= 0.0 {
            return 0.0;
        }
        let part = match state {
            "idle" => self.idle,
            "busy" => self.busy,
            "blocked" => self.blocked,
            "down" => self.down,
            "off_shift" => self.off_shift,
            _ => 0.0,
        };
        part / total
    }
}

/// Statistics for one object in one replication.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectStats {
    pub kind: ObjectKind,
    /// Items that entered the object (accepted, not just offered).
    pub input_count: u64,
    /// Items that left the object.
    pub output_count: u64,
    /// Time-weighted average number of items held.
    pub avg_content: f64,
    /// Largest number of items held at any time.
    pub max_content: u32,
    /// Average time an item spent inside the object (in and out).
    pub avg_stay_time: f64,
    /// Time spent in each state.
    pub state_times: StateTimes,
    /// Busy fraction, i.e. utilisation for processors.
    pub utilization: f64,
    /// Output items per unit of simulated time.
    pub throughput: f64,
}

impl Default for ObjectStats {
    fn default() -> Self {
        Self {
            kind: ObjectKind::BasicFr,
            input_count: 0,
            output_count: 0,
            avg_content: 0.0,
            max_content: 0,
            avg_stay_time: 0.0,
            state_times: StateTimes::default(),
            utilization: 0.0,
            throughput: 0.0,
        }
    }
}

/// Result of one replication.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplicationResult {
    pub index: u32,
    pub seed: u64,
    /// Simulated time at the end (the configured stop time).
    pub final_time: f64,
    /// Time excluded from statistics at the beginning (warm-up).
    pub warmup: f64,
    pub objects: BTreeMap<String, ObjectStats>,
    pub items_created: u64,
    pub items_completed: u64,
    /// Items still in the model when the run stopped.
    pub items_in_system: u64,
}

/// Aggregated metric across replications.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Aggregate {
    pub mean: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
    /// Half-width of the 95% confidence interval of the mean (normal
    /// approximation, `1.96 * se`); 0 for a single replication.
    pub ci95: f64,
    pub n: u32,
}

impl Aggregate {
    pub fn from_samples(samples: &[f64]) -> Self {
        let n = samples.len() as u32;
        if samples.is_empty() {
            return Self { mean: 0.0, std_dev: 0.0, min: 0.0, max: 0.0, ci95: 0.0, n: 0 };
        }
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        let variance = if samples.len() > 1 {
            samples.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (samples.len() - 1) as f64
        } else {
            0.0
        };
        let std_dev = variance.sqrt();
        let ci95 = if samples.len() > 1 { 1.96 * std_dev / (samples.len() as f64).sqrt() } else { 0.0 };
        Self {
            mean,
            std_dev,
            min: samples.iter().copied().fold(f64::INFINITY, f64::min),
            max: samples.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            ci95,
            n,
        }
    }
}

/// Per-object aggregates for a report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectAggregate {
    pub kind: ObjectKind,
    pub input_count: Aggregate,
    pub output_count: Aggregate,
    pub avg_content: Aggregate,
    pub max_content: Aggregate,
    pub avg_stay_time: Aggregate,
    pub utilization: Aggregate,
    pub throughput: Aggregate,
    pub idle_fraction: Aggregate,
    pub busy_fraction: Aggregate,
    pub blocked_fraction: Aggregate,
    pub down_fraction: Aggregate,
}

/// The full report produced by `openflex run` / `openflex experiment`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimReport {
    pub model: String,
    pub description: String,
    pub time_unit: TimeUnit,
    pub stop_time: f64,
    pub warmup: f64,
    pub base_seed: u64,
    pub replications: Vec<ReplicationResult>,
    pub objects: BTreeMap<String, ObjectAggregate>,
    /// Throughput of every sink, aggregated.
    pub completed: Aggregate,
    /// Items created by every source, aggregated.
    pub created: Aggregate,
}

impl SimReport {
    /// Build a report from the per-replication results.
    pub fn from_replications(
        model: &openflex_model::Model,
        replications: Vec<ReplicationResult>,
    ) -> Self {
        let objects = aggregate_objects(&replications);
        let created = Aggregate::from_samples(
            &replications.iter().map(|r| r.items_created as f64).collect::<Vec<_>>(),
        );
        let completed = Aggregate::from_samples(
            &replications.iter().map(|r| r.items_completed as f64).collect::<Vec<_>>(),
        );
        Self {
            model: model.name.clone(),
            description: model.description.clone(),
            time_unit: model.time_unit,
            stop_time: model.run.stop_time,
            warmup: model.run.warmup,
            base_seed: model.run.seed,
            replications,
            objects,
            completed,
            created,
        }
    }
}

fn aggregate_objects(replications: &[ReplicationResult]) -> BTreeMap<String, ObjectAggregate> {
    let mut names: Vec<&String> = replications
        .iter()
        .flat_map(|replication| replication.objects.keys())
        .collect();
    names.sort();
    names.dedup();

    let mut aggregates = BTreeMap::new();
    for name in names {
        let mut kind = ObjectKind::BasicFr;
        let mut input = Vec::new();
        let mut output = Vec::new();
        let mut content = Vec::new();
        let mut max_content = Vec::new();
        let mut stay = Vec::new();
        let mut utilization = Vec::new();
        let mut throughput = Vec::new();
        let mut idle = Vec::new();
        let mut busy = Vec::new();
        let mut blocked = Vec::new();
        let mut down = Vec::new();
        for replication in replications {
            let Some(stats) = replication.objects.get(name) else { continue };
            kind = stats.kind;
            input.push(stats.input_count as f64);
            output.push(stats.output_count as f64);
            content.push(stats.avg_content);
            max_content.push(stats.max_content as f64);
            stay.push(stats.avg_stay_time);
            utilization.push(stats.utilization);
            throughput.push(stats.throughput);
            idle.push(stats.state_times.fraction("idle"));
            busy.push(stats.state_times.fraction("busy"));
            blocked.push(stats.state_times.fraction("blocked"));
            down.push(stats.state_times.fraction("down"));
        }
        aggregates.insert(
            name.clone(),
            ObjectAggregate {
                kind,
                input_count: Aggregate::from_samples(&input),
                output_count: Aggregate::from_samples(&output),
                avg_content: Aggregate::from_samples(&content),
                max_content: Aggregate::from_samples(&max_content),
                avg_stay_time: Aggregate::from_samples(&stay),
                utilization: Aggregate::from_samples(&utilization),
                throughput: Aggregate::from_samples(&throughput),
                idle_fraction: Aggregate::from_samples(&idle),
                busy_fraction: Aggregate::from_samples(&busy),
                blocked_fraction: Aggregate::from_samples(&blocked),
                down_fraction: Aggregate::from_samples(&down),
            },
        );
    }
    aggregates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_samples() {
        let aggregate = Aggregate::from_samples(&[1.0, 2.0, 3.0, 4.0]);
        assert!((aggregate.mean - 2.5).abs() < 1e-12);
        assert_eq!(aggregate.min, 1.0);
        assert_eq!(aggregate.max, 4.0);
        assert!(aggregate.ci95 > 0.0);
        assert_eq!(aggregate.n, 4);
    }

    #[test]
    fn single_sample_has_no_interval() {
        let aggregate = Aggregate::from_samples(&[7.0]);
        assert_eq!(aggregate.mean, 7.0);
        assert_eq!(aggregate.ci95, 0.0);
    }

    #[test]
    fn state_fractions_sum_to_one() {
        let states = StateTimes { idle: 2.0, busy: 6.0, blocked: 2.0, down: 0.0, off_shift: 0.0 };
        let sum = states.fraction("idle")
            + states.fraction("busy")
            + states.fraction("blocked")
            + states.fraction("down")
            + states.fraction("off_shift");
        assert!((sum - 1.0).abs() < 1e-12);
    }
}
