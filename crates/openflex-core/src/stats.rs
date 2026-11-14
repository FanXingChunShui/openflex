//! Simulation results and their aggregation across replications.
//!
//! The engine reports one [`ReplicationResult`] per independent replication and
//! a [`SimReport`] that aggregates them. All times are model-time seconds; the
//! CLI converts them to the model's time unit for display.

use std::collections::BTreeMap;

use openflex_model::{ObjectKind, RunSettings, TimeUnit};
use serde::{Deserialize, Serialize};

/// Simulation clock configuration for one replication.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimConfig {
    pub stop_time: f64,
    pub warmup: f64,
    pub seed: u64,
}

impl SimConfig {
    pub fn from_run_settings(run: &RunSettings, seed: u64) -> Self {
        Self {
            stop_time: run.stop_time,
            warmup: run.warmup,
            seed,
        }
    }
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            stop_time: 1_000.0,
            warmup: 0.0,
            seed: 1,
        }
    }
}

/// Time spent by an object in each of the standard states.
///
/// The five top-level fields are the states of FlexSim's object state profile
/// and always sum to the simulated time. `setup`, `processing`,
/// `waiting_for_operator` and `waiting_for_transport` are **details**:
///
/// * `setup + processing <= busy` (a broken-down or off-shift object keeps its
///   partial phase time but stops counting that phase),
/// * `waiting_for_operator + waiting_for_transport <= blocked`.
///
/// They are additive fields so reports written before the detail breakdown
/// existed still deserialize (`#[serde(default)]`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct StateTimes {
    pub idle: f64,
    pub busy: f64,
    pub blocked: f64,
    pub down: f64,
    pub off_shift: f64,
    /// Detail of `busy`: time spent in the processor's setup phase.
    #[serde(default)]
    pub setup: f64,
    /// Detail of `busy`: time spent in the processor's process phase.
    #[serde(default)]
    pub processing: f64,
    /// Detail of `blocked`: waiting for an operator (dispatcher or pool).
    #[serde(default)]
    pub waiting_for_operator: f64,
    /// Detail of `blocked`: waiting for an operator that travels to the object.
    #[serde(default)]
    pub waiting_for_transport: f64,
}

impl StateTimes {
    pub fn total(&self) -> f64 {
        self.idle + self.busy + self.blocked + self.down + self.off_shift
    }

    /// Time spent in one state. Accepts both the top-level state names and the
    /// detail names.
    pub fn get(&self, state: &str) -> f64 {
        match state {
            "idle" => self.idle,
            "busy" => self.busy,
            "blocked" => self.blocked,
            "down" => self.down,
            "off_shift" => self.off_shift,
            "setup" => self.setup,
            "processing" => self.processing,
            "waiting_for_operator" => self.waiting_for_operator,
            "waiting_for_transport" => self.waiting_for_transport,
            _ => 0.0,
        }
    }

    pub fn fraction(&self, state: &str) -> f64 {
        let total = self.total();
        if total <= 0.0 {
            return 0.0;
        }
        self.get(state) / total
    }

    /// Share of the busy time spent setting up (0 when the object never ran).
    pub fn setup_share_of_busy(&self) -> f64 {
        if self.busy <= 0.0 {
            0.0
        } else {
            self.setup / self.busy
        }
    }

    /// Share of the busy time spent processing (0 when the object never ran).
    pub fn processing_share_of_busy(&self) -> f64 {
        if self.busy <= 0.0 {
            0.0
        } else {
            self.processing / self.busy
        }
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

/// Statistics of one Process Flow block in one replication.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplicationFlowBlock {
    pub id: String,
    pub kind: String,
    pub input_count: u64,
    pub output_count: u64,
    pub content_avg: f64,
    pub avg_stay_time: f64,
}

/// Process Flow statistics of one replication.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplicationFlow {
    pub tokens_created: u64,
    pub tokens_completed: u64,
    pub blocks: Vec<ReplicationFlowBlock>,
}

/// Fluid statistics of one object in one replication (B5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplicationFluidObject {
    pub name: String,
    pub kind: String,
    pub level_avg: f64,
    pub level_max: f64,
    /// Volume moved per time unit.
    pub throughput: f64,
    /// Volume lost to overflow (tanks only).
    pub overflow: f64,
}

/// Fluid statistics of one replication.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplicationFluid {
    pub objects: Vec<ReplicationFluidObject>,
    pub total_throughput: f64,
    pub total_overflow: f64,
    pub total_inflow: f64,
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
    /// Process Flow statistics; `None` when the model has no process flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<ReplicationFlow>,
    /// Fluid statistics; `None` when the model has no fluid objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fluid: Option<ReplicationFluid>,
}

/// Aggregated metric across replications.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Aggregate {
    pub mean: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
    /// Half-width of the 95% confidence interval of the mean
    /// (`t_{0.975, n-1} * se`); 0 for a single replication.
    ///
    /// The Student t quantile is used instead of the normal approximation, so
    /// small replication counts produce the wide intervals they deserve. From
    /// 30 samples on the table value is close enough to 1.96 that the normal
    /// quantile is used directly.
    pub ci95: f64,
    pub n: u32,
}

/// Two-sided 95% Student-t critical value for `df` degrees of freedom.
///
/// The table covers `df = 1..=29` (i.e. samples of `n < 30`). From 30 degrees
/// of freedom on the normal quantile 1.96 is used, which is the convention for
/// "large" samples and keeps the small-sample interval honest.
pub fn t_critical_95(df: u32) -> f64 {
    const TABLE: [f64; 29] = [
        12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160,
        2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056,
        2.052, 2.048, 2.045,
    ];
    if df == 0 {
        f64::INFINITY
    } else if df as usize <= TABLE.len() {
        TABLE[df as usize - 1]
    } else {
        1.96
    }
}

impl Aggregate {
    pub fn from_samples(samples: &[f64]) -> Self {
        let n = samples.len() as u32;
        if samples.is_empty() {
            return Self {
                mean: 0.0,
                std_dev: 0.0,
                min: 0.0,
                max: 0.0,
                ci95: 0.0,
                n: 0,
            };
        }
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        let variance = if samples.len() > 1 {
            samples.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (samples.len() - 1) as f64
        } else {
            0.0
        };
        let std_dev = variance.sqrt();
        let ci95 = if samples.len() > 1 {
            let critical = if samples.len() >= 30 {
                1.96
            } else {
                t_critical_95((samples.len() - 1) as u32)
            };
            critical * std_dev / (samples.len() as f64).sqrt()
        } else {
            0.0
        };
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

/// Little's law check for one object: `L = λW`.
///
/// `L` is the time-weighted average number of items held, `λ` the throughput
/// and `W` the average stay time. The identity holds exactly for any object
/// that conserves items, so a large error is a symptom of a statistics bug (or
/// of a warm-up window that is too short).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LittleEntry {
    pub kind: ObjectKind,
    /// `L`: time-weighted average content.
    pub content_avg: f64,
    /// `λ`: items leaving the object per time unit.
    pub throughput: f64,
    /// `W`: average time an item spent in the object.
    pub stay_time: f64,
    /// `λW`, the content the identity predicts.
    pub predicted_content: f64,
    /// `|L − λW| / λW` (0 when both sides are 0).
    pub relative_error: f64,
    /// Whether the relative error is inside the tolerance.
    pub ok: bool,
}

/// Result of the Little's law self-check across every object of a report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LittleCheck {
    /// Relative error above which an object is flagged (0.10 = 10%).
    pub tolerance: f64,
    /// One entry per checked object.
    pub objects: BTreeMap<String, LittleEntry>,
    /// Names of the objects whose relative error exceeds the tolerance.
    pub mismatches: Vec<String>,
}

impl LittleCheck {
    /// Relative error tolerated before an object is flagged.
    pub const TOLERANCE: f64 = 0.10;

    /// Check every object aggregate. Objects that do not hold their items
    /// (`source`: items wait in an un-counted out-buffer) are skipped.
    pub fn from_objects(objects: &BTreeMap<String, ObjectAggregate>) -> Self {
        let mut entries = BTreeMap::new();
        let mut mismatches = Vec::new();
        for (name, aggregate) in objects {
            if aggregate.kind == ObjectKind::Source {
                continue;
            }
            let content_avg = aggregate.avg_content.mean;
            let throughput = aggregate.throughput.mean;
            let stay_time = aggregate.avg_stay_time.mean;
            let predicted = throughput * stay_time;
            let (relative_error, ok) = if predicted.abs() < 1e-9 && content_avg.abs() < 1e-9 {
                (0.0, true)
            } else if predicted.abs() < 1e-9 {
                (f64::INFINITY, false)
            } else {
                let error = (content_avg - predicted).abs() / predicted.abs();
                (error, error <= Self::TOLERANCE)
            };
            if !ok {
                mismatches.push(name.clone());
            }
            entries.insert(
                name.clone(),
                LittleEntry {
                    kind: aggregate.kind,
                    content_avg,
                    throughput,
                    stay_time,
                    predicted_content: predicted,
                    relative_error,
                    ok,
                },
            );
        }
        Self {
            tolerance: Self::TOLERANCE,
            objects: entries,
            mismatches,
        }
    }

    /// True when every checked object satisfies the identity.
    pub fn is_ok(&self) -> bool {
        self.mismatches.is_empty()
    }
}

/// Aggregated fluid statistics of one object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FluidObjectReport {
    pub name: String,
    pub kind: String,
    pub level_avg: f64,
    pub level_max: f64,
    pub throughput: f64,
    pub overflow: f64,
}

/// Aggregated fluid statistics of the whole run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FluidReport {
    pub objects: Vec<FluidObjectReport>,
    pub total_throughput: f64,
    pub total_overflow: f64,
    pub total_inflow: f64,
}

/// Aggregated Process Flow block statistics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowBlockReport {
    pub id: String,
    pub kind: String,
    pub input_count: f64,
    pub output_count: f64,
    pub content_avg: f64,
    pub avg_stay_time: f64,
}

/// Aggregated Process Flow statistics of the whole run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowReport {
    pub tokens_created: f64,
    pub tokens_completed: f64,
    pub blocks: Vec<FlowBlockReport>,
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
    /// Process Flow statistics; `None` when the model has no process flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<FlowReport>,
    /// Fluid statistics; `None` when the model has no fluid objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fluid: Option<FluidReport>,
    /// Little's law (`L = λW`) self-check; `None` only for reports built by
    /// hand that do not want the check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub little_check: Option<LittleCheck>,
}

impl SimReport {
    /// Build a report from the per-replication results.
    pub fn from_replications(
        model: &openflex_model::Model,
        replications: Vec<ReplicationResult>,
    ) -> Self {
        let objects = aggregate_objects(&replications);
        let created = Aggregate::from_samples(
            &replications
                .iter()
                .map(|r| r.items_created as f64)
                .collect::<Vec<_>>(),
        );
        let completed = Aggregate::from_samples(
            &replications
                .iter()
                .map(|r| r.items_completed as f64)
                .collect::<Vec<_>>(),
        );
        let little_check = Some(LittleCheck::from_objects(&objects));
        let process_flow = aggregate_flow(&replications);
        let fluid = aggregate_fluid(&replications);
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
            process_flow,
            fluid,
            little_check,
        }
    }
}

/// Average the Process Flow statistics over the replications.
fn aggregate_flow(replications: &[ReplicationResult]) -> Option<FlowReport> {
    let flows: Vec<&ReplicationFlow> = replications
        .iter()
        .filter_map(|replication| replication.process_flow.as_ref())
        .collect();
    if flows.is_empty() {
        return None;
    }
    let count = flows.len() as f64;
    let tokens_created = flows
        .iter()
        .map(|flow| flow.tokens_created as f64)
        .sum::<f64>()
        / count;
    let tokens_completed = flows
        .iter()
        .map(|flow| flow.tokens_completed as f64)
        .sum::<f64>()
        / count;

    let mut order: Vec<(String, String)> = Vec::new();
    for flow in &flows {
        for block in &flow.blocks {
            if !order.iter().any(|(id, _)| *id == block.id) {
                order.push((block.id.clone(), block.kind.clone()));
            }
        }
    }
    let blocks = order
        .into_iter()
        .map(|(id, kind)| {
            let samples: Vec<&ReplicationFlowBlock> = flows
                .iter()
                .flat_map(|flow| flow.blocks.iter().filter(|block| block.id == id))
                .collect();
            let mean = |pick: fn(&ReplicationFlowBlock) -> f64| {
                if samples.is_empty() {
                    0.0
                } else {
                    samples.iter().map(|block| pick(block)).sum::<f64>() / samples.len() as f64
                }
            };
            FlowBlockReport {
                id,
                kind,
                input_count: mean(|block| block.input_count as f64),
                output_count: mean(|block| block.output_count as f64),
                content_avg: mean(|block| block.content_avg),
                avg_stay_time: mean(|block| block.avg_stay_time),
            }
        })
        .collect();
    Some(FlowReport {
        tokens_created,
        tokens_completed,
        blocks,
    })
}

/// Average the fluid statistics over the replications.
fn aggregate_fluid(replications: &[ReplicationResult]) -> Option<FluidReport> {
    let runs: Vec<&ReplicationFluid> = replications
        .iter()
        .filter_map(|replication| replication.fluid.as_ref())
        .collect();
    if runs.is_empty() {
        return None;
    }
    let mean = |pick: fn(&ReplicationFluid) -> f64| {
        runs.iter().map(|run| pick(run)).sum::<f64>() / runs.len() as f64
    };
    let mut names: Vec<(String, String)> = Vec::new();
    for run in &runs {
        for object in &run.objects {
            if !names.iter().any(|(name, _)| *name == object.name) {
                names.push((object.name.clone(), object.kind.clone()));
            }
        }
    }
    let objects = names
        .into_iter()
        .map(|(name, kind)| {
            let samples: Vec<&ReplicationFluidObject> = runs
                .iter()
                .flat_map(|run| run.objects.iter().filter(|object| object.name == name))
                .collect();
            let sample_mean = |pick: fn(&ReplicationFluidObject) -> f64| {
                if samples.is_empty() {
                    0.0
                } else {
                    samples.iter().map(|object| pick(object)).sum::<f64>() / samples.len() as f64
                }
            };
            FluidObjectReport {
                name,
                kind,
                level_avg: sample_mean(|object| object.level_avg),
                level_max: sample_mean(|object| object.level_max),
                throughput: sample_mean(|object| object.throughput),
                overflow: sample_mean(|object| object.overflow),
            }
        })
        .collect();
    Some(FluidReport {
        objects,
        total_throughput: mean(|run| run.total_throughput),
        total_overflow: mean(|run| run.total_overflow),
        total_inflow: mean(|run| run.total_inflow),
    })
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
            let Some(stats) = replication.objects.get(name) else {
                continue;
            };
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
        let states = StateTimes {
            idle: 2.0,
            busy: 6.0,
            blocked: 2.0,
            down: 0.0,
            off_shift: 0.0,
            setup: 1.0,
            processing: 5.0,
            waiting_for_operator: 1.5,
            waiting_for_transport: 0.5,
        };
        let sum = states.fraction("idle")
            + states.fraction("busy")
            + states.fraction("blocked")
            + states.fraction("down")
            + states.fraction("off_shift");
        assert!((sum - 1.0).abs() < 1e-12);
    }

    #[test]
    fn state_details_are_backwards_compatible() {
        // A report written before the detail fields existed deserializes with
        // them set to zero.
        let json = r#"{"idle":1.0,"busy":2.0,"blocked":0.0,"down":0.0,"off_shift":0.0}"#;
        let states: StateTimes = serde_json::from_str(json).expect("legacy state times");
        assert_eq!(states.setup, 0.0);
        assert_eq!(states.processing, 0.0);
        assert_eq!(states.waiting_for_operator, 0.0);
        assert_eq!(states.waiting_for_transport, 0.0);
        assert_eq!(states.total(), 3.0);
    }

    #[test]
    fn state_details_stay_inside_their_parent_state() {
        let states = StateTimes {
            idle: 0.0,
            busy: 10.0,
            blocked: 6.0,
            down: 0.0,
            off_shift: 0.0,
            setup: 2.0,
            processing: 8.0,
            waiting_for_operator: 4.0,
            waiting_for_transport: 2.0,
        };
        assert!(states.setup + states.processing <= states.busy);
        assert!(states.waiting_for_operator + states.waiting_for_transport <= states.blocked);
        assert!((states.setup_share_of_busy() - 0.2).abs() < 1e-12);
        assert!((states.processing_share_of_busy() - 0.8).abs() < 1e-12);
        assert_eq!(states.get("waiting_for_operator"), 4.0);
        assert_eq!(states.get("unknown"), 0.0);
    }

    #[test]
    fn t_quantiles_match_the_table() {
        assert!((t_critical_95(1) - 12.706).abs() < 1e-9);
        assert!((t_critical_95(2) - 4.303).abs() < 1e-9);
        assert!((t_critical_95(9) - 2.262).abs() < 1e-9);
        assert!((t_critical_95(28) - 2.048).abs() < 1e-9);
        assert!((t_critical_95(29) - 2.045).abs() < 1e-9);
        // From 30 degrees of freedom on the normal quantile is used.
        assert!((t_critical_95(30) - 1.96).abs() < 1e-12);
        assert!((t_critical_95(200) - 1.96).abs() < 1e-12);
        assert!(t_critical_95(0).is_infinite());
    }

    #[test]
    fn small_samples_use_the_t_interval() {
        let samples = [1.0, 2.0, 3.0, 4.0];
        let aggregate = Aggregate::from_samples(&samples);
        // sd = sqrt(5/3) = 1.29099; se = sd / 2 = 0.645497; t(3) = 3.182.
        let expected = 3.182 * (5.0f64 / 3.0).sqrt() / 2.0;
        assert!((aggregate.ci95 - expected).abs() < 1e-9);
        // The t interval is wider than the normal approximation would be.
        assert!(aggregate.ci95 > 1.96 * (5.0f64 / 3.0).sqrt() / 2.0);
    }

    #[test]
    fn large_samples_use_the_normal_quantile() {
        let samples: Vec<f64> = (0..40).map(|i| i as f64).collect();
        let aggregate = Aggregate::from_samples(&samples);
        let mean = samples.iter().sum::<f64>() / 40.0;
        let variance = samples.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / 39.0;
        let expected = 1.96 * variance.sqrt() / (40.0f64).sqrt();
        assert!((aggregate.ci95 - expected).abs() < 1e-9);
    }

    fn aggregate_of(kind: ObjectKind, content: f64, throughput: f64, stay: f64) -> ObjectAggregate {
        let sample = |value: f64| Aggregate::from_samples(&[value]);
        ObjectAggregate {
            kind,
            input_count: sample(0.0),
            output_count: sample(0.0),
            avg_content: sample(content),
            max_content: sample(0.0),
            avg_stay_time: sample(stay),
            utilization: sample(0.0),
            throughput: sample(throughput),
            idle_fraction: sample(0.0),
            busy_fraction: sample(0.0),
            blocked_fraction: sample(0.0),
            down_fraction: sample(0.0),
        }
    }

    #[test]
    fn little_check_accepts_consistent_objects() {
        let mut objects = BTreeMap::new();
        // L = 4, λ = 0.5, W = 8 -> λW = 4.
        objects.insert(
            "Server".to_string(),
            aggregate_of(ObjectKind::Processor, 4.0, 0.5, 8.0),
        );
        objects.insert(
            "Empty".to_string(),
            aggregate_of(ObjectKind::Sink, 0.0, 0.0, 0.0),
        );
        // Sources stage items outside their counted content, so they are skipped.
        objects.insert(
            "Source1".to_string(),
            aggregate_of(ObjectKind::Source, 0.0, 0.5, 1.0),
        );
        let check = LittleCheck::from_objects(&objects);
        assert!(check.is_ok(), "{:?}", check.mismatches);
        assert!(!check.objects.contains_key("Source1"));
        let server = &check.objects["Server"];
        assert!((server.predicted_content - 4.0).abs() < 1e-12);
        assert!(server.ok);
    }

    #[test]
    fn little_check_flags_inconsistent_objects() {
        let mut objects = BTreeMap::new();
        // L = 10 but λW = 5: a 100% error.
        objects.insert(
            "Server".to_string(),
            aggregate_of(ObjectKind::Processor, 10.0, 0.5, 10.0),
        );
        let check = LittleCheck::from_objects(&objects);
        assert!(!check.is_ok());
        assert_eq!(check.mismatches, vec!["Server".to_string()]);
        let entry = &check.objects["Server"];
        assert!((entry.relative_error - 1.0).abs() < 1e-12);
        assert!(!entry.ok);
    }

    #[test]
    fn little_check_tolerates_ten_percent() {
        let mut objects = BTreeMap::new();
        // λW = 10, L = 10.9 -> 9% error, inside the tolerance.
        objects.insert(
            "Server".to_string(),
            aggregate_of(ObjectKind::Processor, 10.9, 1.0, 10.0),
        );
        // λW = 10, L = 11.5 -> 15% error, outside.
        objects.insert(
            "Queue".to_string(),
            aggregate_of(ObjectKind::Queue, 11.5, 1.0, 10.0),
        );
        let check = LittleCheck::from_objects(&objects);
        assert_eq!(check.mismatches, vec!["Queue".to_string()]);
        assert!(check.objects["Server"].ok);
    }
}
