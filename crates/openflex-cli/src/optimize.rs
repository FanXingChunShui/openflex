//! Parameter search for `openflex optimize` (D4).
//!
//! The command evaluates a grid (or a deterministic sample) of parameter
//! combinations and ranks them by a system-level objective:
//!
//! - `throughput` — completed items per time unit (higher is better)
//! - `wip` — average work in process across the model, or one object (lower)
//! - `utilization` — average utilisation, or one object (higher)
//!
//! Each combination is a full `openflex run` of the model, so the CLI reuses
//! the same replication, seed and parallel machinery as the normal run path.
//! Rendering is deliberately self-contained: a table for humans, CSV for
//! spreadsheets and JSON for scripts.

use openflex_core::SimReport;
use openflex_model::Model;
use openflex_report::Format;
use serde_json::json;

/// What the search maximises or minimises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub enum Objective {
    /// Completed items per time unit (maximise).
    Throughput,
    /// Average work in process (minimise).
    Wip,
    /// Average utilisation (maximise).
    Utilization,
}

impl std::fmt::Display for Objective {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

impl Objective {
    pub fn label(self) -> &'static str {
        match self {
            Objective::Throughput => "throughput",
            Objective::Wip => "wip",
            Objective::Utilization => "utilization",
        }
    }

    pub fn maximize(self) -> bool {
        !matches!(self, Objective::Wip)
    }
}

/// Search strategy for `openflex optimize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Algorithm {
    /// Full grid (or deterministic subset with `--samples`).
    Grid,
    /// Uniform random sample within each parameter range.
    Random,
    /// Deterministic genetic search (population, crossover, mutation, elites).
    Genetic,
}

/// One `--param OBJECT.PARAM=...` specification.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamRange {
    pub object: String,
    pub param: String,
    /// The values that appear in the `key` column of reports.
    pub values: Vec<f64>,
}

impl ParamRange {
    pub fn key(&self) -> String {
        format!("{}.{}", self.object, self.param)
    }
}

/// Parse `OBJECT.PARAM=VALUE`, `=START..END` or `=V1,V2,V3`.
///
/// For a `START..END` range, `steps` equally spaced values are produced
/// (inclusive); an explicit list ignores `--steps`.
pub fn parse_param(spec: &str, steps: u32) -> Result<ParamRange, String> {
    let (target, range) = spec
        .split_once('=')
        .ok_or_else(|| format!("--param expects OBJECT.PARAM=START..END, found '{spec}'"))?;
    let (object, param) = target
        .split_once('.')
        .ok_or_else(|| format!("--param expects OBJECT.PARAM=VALUE, found '{spec}'"))?;
    if object.trim().is_empty() || param.trim().is_empty() {
        return Err(format!(
            "--param expects a non-empty object and parameter, found '{spec}'"
        ));
    }
    let range = range.trim();
    if range.is_empty() {
        return Err(format!("--param '{spec}' has an empty value range"));
    }

    let values = if let Some((start, end)) = range.split_once("..") {
        let start = parse_number(start, spec)?;
        let end = parse_number(end, spec)?;
        if end < start {
            return Err(format!(
                "--param '{spec}': end value {end} is smaller than start value {start}"
            ));
        }
        let steps = steps.max(1);
        if steps == 1 {
            vec![start]
        } else {
            (0..steps)
                .map(|index| {
                    let t = index as f64 / (steps - 1) as f64;
                    start + (end - start) * t
                })
                .collect()
        }
    } else if range.contains(',') {
        range
            .split(',')
            .map(|value| parse_number(value, spec))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![parse_number(range, spec)?]
    };
    if values.is_empty() {
        return Err(format!("--param '{spec}' produced no values"));
    }
    Ok(ParamRange {
        object: object.trim().to_string(),
        param: param.trim().to_string(),
        values,
    })
}

fn parse_number(value: &str, spec: &str) -> Result<f64, String> {
    let value = value.trim();
    value
        .parse::<f64>()
        .map_err(|error| format!("--param '{spec}': invalid number '{value}': {error}"))
        .and_then(|number| {
            if number.is_finite() {
                Ok(number)
            } else {
                Err(format!("--param '{spec}': '{value}' is not finite"))
            }
        })
}

/// Enumerate the parameter grid, or a deterministic sample of it.
///
/// Sampling uses SplitMix64 over the mixed-radix index so repeated runs with
/// the same `--seed` select the same candidates; the selected indices are
/// sorted, which keeps the output table stable.
pub fn combinations(
    ranges: &[ParamRange],
    samples: Option<usize>,
    seed: u64,
) -> Result<Vec<Vec<f64>>, String> {
    if ranges.is_empty() {
        return Err("at least one --param is required".into());
    }
    let mut total = 1u64;
    for range in ranges {
        total = total
            .checked_mul(range.values.len() as u64)
            .ok_or_else(|| "the parameter grid is too large; pass --samples".to_string())?;
    }
    let wanted = match samples {
        Some(0) => return Err("--samples must be at least 1".into()),
        Some(count) => (count as u64).min(total),
        None => total,
    };
    const MAX_GRID: u64 = 100_000;
    if wanted > MAX_GRID {
        return Err(format!(
            "the parameter grid has {total} combinations; pass --samples to evaluate a subset"
        ));
    }

    let indices: Vec<u64> = if wanted < total {
        sample_indices(total, wanted, seed)
    } else {
        (0..total).collect()
    };
    let combinations = indices
        .into_iter()
        .map(|mut index| {
            let mut values = vec![0.0; ranges.len()];
            for (position, range) in ranges.iter().enumerate().rev() {
                let radix = range.values.len() as u64;
                values[position] = range.values[(index % radix) as usize];
                index /= radix;
            }
            values
        })
        .collect();
    Ok(combinations)
}

fn sample_indices(total: u64, count: u64, seed: u64) -> Vec<u64> {
    let mut chosen = std::collections::BTreeSet::new();
    let mut state = seed;
    while (chosen.len() as u64) < count {
        state = splitmix64(state);
        chosen.insert(state % total);
    }
    chosen.into_iter().collect()
}

fn splitmix64(state: u64) -> u64 {
    let mut z = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Apply one combination to a copy of the base model.
pub fn apply(base: &Model, ranges: &[ParamRange], values: &[f64]) -> Result<Model, String> {
    if ranges.len() != values.len() {
        return Err("internal error: parameter/value count mismatch".into());
    }
    let mut model = base.clone();
    for (range, value) in ranges.iter().zip(values) {
        // Integer parameters (queue capacities, counts, …) reject `5.0`, so
        // integral values are sent as integers and fractional ones as floats.
        let payload = if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
            json!(*value as i64)
        } else {
            json!(value)
        };
        model = model
            .with_override(&range.object, &range.param, payload)
            .map_err(|error| {
                format!(
                    "could not apply {}.{}={}: {error}",
                    range.object, range.param, value
                )
            })?;
    }
    Ok(model)
}

/// One evaluated parameter combination.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub index: usize,
    pub assignments: Vec<(String, f64)>,
    /// Raw value of the primary objective, or the weighted score for several.
    pub metric: f64,
    /// Weighted score (higher is always better).
    pub score: f64,
    /// Every objective value, in the order given on the command line.
    pub objective_values: Vec<(Objective, f64)>,
    pub created: f64,
    pub completed: f64,
    pub wip: f64,
    pub utilization: f64,
}

/// Weighted score of one candidate: maximise objectives, minimise `wip`.
pub fn weighted_score(
    objectives: &[Objective],
    values: &[(Objective, f64)],
    weights: &std::collections::BTreeMap<Objective, f64>,
) -> f64 {
    let mut score = 0.0;
    for objective in objectives {
        if let Some((_, value)) = values.iter().find(|(kind, _)| kind == objective) {
            let weight = weights.get(objective).copied().unwrap_or(1.0);
            score += weight
                * if objective.maximize() {
                    *value
                } else {
                    -*value
                };
        }
    }
    score
}

/// Parse `--weight objective=value` specifications.
pub fn parse_weights(
    specs: &[String],
    objectives: &[Objective],
) -> Result<std::collections::BTreeMap<Objective, f64>, String> {
    let mut weights = std::collections::BTreeMap::new();
    if specs.is_empty() {
        for objective in objectives {
            weights.insert(*objective, 1.0);
        }
        return Ok(weights);
    }
    for spec in specs {
        let (name, value) = spec
            .split_once('=')
            .ok_or_else(|| format!("--weight expects OBJECTIVE=WEIGHT, found '{spec}'"))?;
        let objective = objectives
            .iter()
            .find(|objective| objective.label() == name.trim())
            .copied()
            .ok_or_else(|| {
                format!(
                    "--weight '{name}' is not one of the selected objectives ({})",
                    objectives
                        .iter()
                        .map(|objective| objective.label())
                        .collect::<Vec<_>>()
                        .join(",")
                )
            })?;
        let weight = value
            .trim()
            .parse::<f64>()
            .map_err(|error| format!("--weight '{spec}': invalid number: {error}"))?;
        if !weight.is_finite() || weight < 0.0 {
            return Err(format!("--weight '{spec}': weight must be non-negative"));
        }
        weights.insert(objective, weight);
    }
    for objective in objectives {
        if !weights.contains_key(objective) {
            return Err(format!(
                "--weight is missing a value for '{}'",
                objective.label()
            ));
        }
    }
    Ok(weights)
}

/// Extract the objective value from one run report.
pub fn metric(
    report: &SimReport,
    objective: Objective,
    object: Option<&str>,
) -> Result<f64, String> {
    if let Some(name) = object
        && !report.objects.contains_key(name)
    {
        return Err(format!("--object '{name}' does not exist in the report"));
    }
    let duration = (report.stop_time - report.warmup).max(f64::EPSILON);
    Ok(match objective {
        Objective::Throughput => match object {
            Some(name) => report.objects[name].throughput.mean,
            None => report.completed.mean / duration,
        },
        Objective::Wip => match object {
            Some(name) => report.objects[name].avg_content.mean,
            None => report
                .objects
                .values()
                .map(|stats| stats.avg_content.mean)
                .sum(),
        },
        Objective::Utilization => match object {
            Some(name) => report.objects[name].utilization.mean,
            None => {
                let values: Vec<f64> = report
                    .objects
                    .values()
                    .map(|stats| stats.utilization.mean)
                    .collect();
                if values.is_empty() {
                    0.0
                } else {
                    values.iter().sum::<f64>() / values.len() as f64
                }
            }
        },
    })
}

/// Build a report row from one run.
pub fn candidate(
    index: usize,
    ranges: &[ParamRange],
    values: &[f64],
    report: &SimReport,
    objectives: &[Objective],
    weights: &std::collections::BTreeMap<Objective, f64>,
    object: Option<&str>,
) -> Result<Candidate, String> {
    let assignments = ranges
        .iter()
        .zip(values)
        .map(|(range, value)| (range.key(), *value))
        .collect();
    let objective_values: Vec<(Objective, f64)> = objectives
        .iter()
        .map(|objective| Ok((*objective, metric(report, *objective, object)?)))
        .collect::<Result<_, String>>()?;
    let score = weighted_score(objectives, &objective_values, weights);
    let primary = objective_values
        .first()
        .map(|(_, value)| *value)
        .unwrap_or(score);
    Ok(Candidate {
        index,
        assignments,
        metric: if objectives.len() == 1 {
            primary
        } else {
            score
        },
        score,
        objective_values,
        created: report.created.mean,
        completed: report.completed.mean,
        wip: report
            .objects
            .values()
            .map(|stats| stats.avg_content.mean)
            .sum(),
        utilization: {
            let values: Vec<f64> = report
                .objects
                .values()
                .map(|stats| stats.utilization.mean)
                .collect();
            if values.is_empty() {
                0.0
            } else {
                values.iter().sum::<f64>() / values.len() as f64
            }
        },
    })
}

/// Index of the best candidate (ties keep the earliest grid position).
pub fn best_index(candidates: &[Candidate]) -> usize {
    let mut best = 0;
    for (index, candidate) in candidates.iter().enumerate().skip(1) {
        if candidate.score > candidates[best].score {
            best = index;
        }
    }
    best
}

/// Indices of candidates that are not dominated by any other candidate.
pub fn pareto_indices(candidates: &[Candidate], objectives: &[Objective]) -> Vec<usize> {
    let mut front = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let dominated = candidates.iter().enumerate().any(|(other_index, other)| {
            if other_index == index {
                return false;
            }
            let mut better_in_one = false;
            for objective in objectives {
                let value = candidate
                    .objective_values
                    .iter()
                    .find(|(kind, _)| kind == objective)
                    .map(|(_, value)| *value)
                    .unwrap_or(0.0);
                let other_value = other
                    .objective_values
                    .iter()
                    .find(|(kind, _)| kind == objective)
                    .map(|(_, value)| *value)
                    .unwrap_or(0.0);
                let (left, right) = if objective.maximize() {
                    (other_value, value)
                } else {
                    (-other_value, -value)
                };
                if left < right - 1e-12 {
                    return false;
                }
                if left > right + 1e-12 {
                    better_in_one = true;
                }
            }
            better_in_one
        });
        if !dominated {
            front.push(index);
        }
    }
    front
}

/// Render the search result.
#[allow(clippy::too_many_arguments)]
pub fn render(
    model: &str,
    objectives: &[Objective],
    weights: &std::collections::BTreeMap<Objective, f64>,
    object: Option<&str>,
    ranges: &[ParamRange],
    candidates: &[Candidate],
    best: usize,
    pareto: bool,
    format: Format,
) -> Result<String, String> {
    Ok(match format {
        Format::Json => to_json(model, objectives, weights, object, candidates, best, pareto)?,
        Format::Csv => to_csv(objectives, ranges, candidates, best, pareto),
        Format::Table => to_table(model, objectives, object, candidates, best, pareto),
        Format::Markdown => to_markdown(model, objectives, object, candidates, best, pareto),
    })
}

fn candidate_json(candidate: &Candidate, pareto: bool) -> serde_json::Value {
    let parameters: serde_json::Map<String, serde_json::Value> = candidate
        .assignments
        .iter()
        .map(|(key, value)| (key.clone(), json!(value)))
        .collect();
    let objective_values: serde_json::Map<String, serde_json::Value> = candidate
        .objective_values
        .iter()
        .map(|(objective, value)| (objective.label().to_string(), json!(value)))
        .collect();
    json!({
        "index": candidate.index,
        "parameters": parameters,
        "metric": candidate.metric,
        "score": candidate.score,
        "objective_values": objective_values,
        "pareto": pareto,
        "created": candidate.created,
        "completed": candidate.completed,
        "avg_wip": candidate.wip,
        "utilization": candidate.utilization,
    })
}

fn to_json(
    model: &str,
    objectives: &[Objective],
    weights: &std::collections::BTreeMap<Objective, f64>,
    object: Option<&str>,
    candidates: &[Candidate],
    best: usize,
    pareto: bool,
) -> Result<String, String> {
    let weight_json: serde_json::Map<String, serde_json::Value> = objectives
        .iter()
        .map(|objective| {
            (
                objective.label().to_string(),
                json!(weights.get(objective).copied().unwrap_or(1.0)),
            )
        })
        .collect();
    let front: Vec<serde_json::Value> = if pareto {
        pareto_indices(candidates, objectives)
            .into_iter()
            .filter_map(|index| candidates.get(index))
            .map(|candidate| candidate_json(candidate, true))
            .collect()
    } else {
        Vec::new()
    };
    let values: Vec<serde_json::Value> = candidates
        .iter()
        .map(|candidate| {
            candidate_json(
                candidate,
                front.iter().any(|value| value["index"] == candidate.index),
            )
        })
        .collect();
    let objective_label = objectives
        .iter()
        .map(|objective| objective.label())
        .collect::<Vec<_>>()
        .join(",");
    let direction = if objectives.len() == 1 && !objectives[0].maximize() {
        "minimize"
    } else {
        "maximize"
    };
    let document = json!({
        "model": model,
        "objective": objective_label,
        "objectives": objectives.iter().map(|objective| json!({
            "name": objective.label(),
            "direction": if objective.maximize() { "maximize" } else { "minimize" },
        })).collect::<Vec<_>>(),
        "direction": direction,
        "object": object,
        "weights": weight_json,
        "pareto_enabled": pareto,
        "pareto_front": front,
        "best": &values[best],
        "candidates": values,
    });
    serde_json::to_string_pretty(&document).map_err(|error| error.to_string())
}

fn to_csv(
    objectives: &[Objective],
    ranges: &[ParamRange],
    candidates: &[Candidate],
    best: usize,
    pareto: bool,
) -> String {
    let mut out = String::new();
    out.push_str("rank");
    for range in ranges {
        out.push(',');
        out.push_str(&range.key());
    }
    if objectives.len() == 1 {
        out.push_str(&format!(",{}", objectives[0].label()));
    } else {
        for objective in objectives {
            out.push(',');
            out.push_str(objective.label());
        }
        out.push_str(",score");
    }
    out.push_str(",best");
    if pareto {
        out.push_str(",pareto");
    }
    out.push_str(",created,completed,avg_wip,utilization\n");
    let front = pareto_indices(candidates, objectives);
    let mut ranked: Vec<&Candidate> = candidates.iter().collect();
    sort_candidates(&mut ranked);
    for (rank, candidate) in ranked.iter().enumerate() {
        out.push_str(&format!("{}", rank + 1));
        for (_, value) in &candidate.assignments {
            out.push_str(&format!(",{value}"));
        }
        if objectives.len() == 1 {
            out.push_str(&format!(",{}", candidate.metric));
        } else {
            for objective in objectives {
                let value = candidate
                    .objective_values
                    .iter()
                    .find(|(kind, _)| kind == objective)
                    .map(|(_, value)| *value)
                    .unwrap_or_default();
                out.push_str(&format!(",{value}"));
            }
            out.push_str(&format!(",{}", candidate.score));
        }
        out.push_str(&format!(",{}", candidate.index == best));
        if pareto {
            out.push_str(&format!(",{}", front.contains(&candidate.index)));
        }
        out.push_str(&format!(
            ",{:.3},{:.3},{:.3},{:.4}\n",
            candidate.created, candidate.completed, candidate.wip, candidate.utilization
        ));
    }
    out
}

fn to_table(
    model: &str,
    objectives: &[Objective],
    object: Option<&str>,
    candidates: &[Candidate],
    best: usize,
    pareto: bool,
) -> String {
    let mut out = String::new();
    let objective_label = objectives
        .iter()
        .map(|objective| objective.label())
        .collect::<Vec<_>>()
        .join("+");
    let direction = if objectives.len() == 1 && !objectives[0].maximize() {
        "minimize"
    } else {
        "maximize weighted score"
    };
    out.push_str(&format!(
        "OpenFlex optimize: {model} (objective {} {}{}, {} candidates)\n",
        objective_label,
        direction,
        object.map(|name| format!(" on {name}")).unwrap_or_default(),
        candidates.len()
    ));
    out.push_str(&format!(
        "{:<28} {:>10} {:>10} {:>10} {:>10} {:>10}\n",
        "parameters", "metric", "score", "created", "completed", "avg_wip"
    ));
    out.push_str(&"-".repeat(92));
    out.push('\n');
    let front = pareto_indices(candidates, objectives);
    let mut ranked: Vec<&Candidate> = candidates.iter().collect();
    sort_candidates(&mut ranked);
    for (rank, candidate) in ranked.iter().enumerate() {
        let parameters = candidate
            .assignments
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(" ");
        let marker = if candidate.index == best { "*" } else { " " };
        let front_marker = if pareto && front.contains(&candidate.index) {
            "P"
        } else {
            " "
        };
        out.push_str(&format!(
            "{marker}{front_marker}{rank:<2} {:<22} {:>10.4} {:>10.4} {:>10.2} {:>10.2} {:>10.2}\n",
            parameters,
            candidate.metric,
            candidate.score,
            candidate.created,
            candidate.completed,
            candidate.wip
        ));
    }
    if let Some(best) = candidates.get(best) {
        let parameters = best
            .assignments
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "\nbest: {parameters} -> {} {:.4} (score {:.4})\n",
            objective_label, best.metric, best.score
        ));
    }
    if pareto {
        out.push_str(&format!("pareto front: {}\n", front.len()));
    }
    out
}

fn to_markdown(
    model: &str,
    objectives: &[Objective],
    object: Option<&str>,
    candidates: &[Candidate],
    best: usize,
    pareto: bool,
) -> String {
    let mut out = String::new();
    let objective_label = objectives
        .iter()
        .map(|objective| objective.label())
        .collect::<Vec<_>>()
        .join("+");
    out.push_str(&format!(
        "# OpenFlex optimize: {model}\n\nObjective: {objective_label}"
    ));
    if let Some(name) = object {
        out.push_str(&format!(" on `{name}`"));
    }
    out.push_str("\n\n| Rank | Parameters | Metric | Score | Created | Completed | Avg WIP | Utilisation | Pareto |\n");
    out.push_str("|---|---|---:|---:|---:|---:|---:|---:|:---:|\n");
    let front = pareto_indices(candidates, objectives);
    let mut ranked: Vec<&Candidate> = candidates.iter().collect();
    sort_candidates(&mut ranked);
    for (rank, candidate) in ranked.iter().enumerate() {
        let parameters = candidate
            .assignments
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("<br>");
        let marker = if candidate.index == best {
            " **best**"
        } else {
            ""
        };
        out.push_str(&format!(
            "| {} | {parameters} | {:.4} | {:.4} | {:.2} | {:.2} | {:.2} | {:.1}% | {}{} |\n",
            rank + 1,
            candidate.metric,
            candidate.score,
            candidate.created,
            candidate.completed,
            candidate.wip,
            candidate.utilization * 100.0,
            if pareto && front.contains(&candidate.index) {
                "P"
            } else {
                ""
            },
            marker
        ));
    }
    out
}

fn sort_candidates(candidates: &mut Vec<&Candidate>) {
    candidates.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then(left.index.cmp(&right.index))
    });
}

/// Lower/upper bounds of one parameter range (explicit lists included).
fn range_bounds(range: &ParamRange) -> (f64, f64) {
    let min = range.values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = range
        .values
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    (min, max)
}

fn integral_range(range: &ParamRange) -> bool {
    range.values.iter().all(|value| value.fract() == 0.0)
}

fn clamp_value(range: &ParamRange, value: f64) -> f64 {
    let (min, max) = range_bounds(range);
    let value = value.max(min).min(max);
    if integral_range(range) {
        value.round()
    } else {
        value
    }
}

/// Uniform random combinations within each parameter range.
pub fn random_combinations(
    ranges: &[ParamRange],
    count: usize,
    seed: u64,
) -> Result<Vec<Vec<f64>>, String> {
    if count == 0 {
        return Err("--samples must be at least 1".into());
    }
    let mut state = seed;
    let mut combinations = Vec::with_capacity(count);
    for _ in 0..count {
        let mut values = Vec::with_capacity(ranges.len());
        for range in ranges {
            let (min, max) = range_bounds(range);
            state = splitmix64(state);
            let fraction = (state >> 11) as f64 / (1u64 << 53) as f64;
            values.push(clamp_value(range, min + (max - min) * fraction));
        }
        combinations.push(values);
    }
    Ok(combinations)
}

/// Genetic search parameters.
#[derive(Debug, Clone, Copy)]
pub struct GeneticConfig {
    pub population: usize,
    pub generations: usize,
    pub mutation_rate: f64,
}

/// Deterministic genetic search using a caller-supplied evaluator.
///
/// The evaluator receives a batch of parameter vectors and must return one
/// [`Candidate`] per vector in the same order. Selection uses the weighted
/// score; elites survive unchanged, children use blend crossover and Gaussian
/// mutation. The RNG is a local SplitMix64 seeded by `seed`, so repeated runs
/// with the same seed explore exactly the same population.
pub fn genetic_search<F, E>(
    ranges: &[ParamRange],
    config: &GeneticConfig,
    seed: u64,
    mut evaluate: F,
) -> Result<Vec<Candidate>, E>
where
    F: FnMut(&[Vec<f64>]) -> Result<Vec<Candidate>, E>,
    E: From<String>,
{
    if config.population < 2 {
        return Err(E::from("--population must be at least 2".to_string()));
    }
    if config.generations == 0 {
        return Err(E::from("--generations must be at least 1".to_string()));
    }
    if !config.mutation_rate.is_finite() || !(0.0..=1.0).contains(&config.mutation_rate) {
        return Err(E::from(
            "--mutation-rate must be between 0 and 1".to_string(),
        ));
    }
    let mut state = seed ^ 0xA5A5_5A5A_1234_5678;
    let mut population: Vec<Vec<f64>> = Vec::with_capacity(config.population);
    for _ in 0..config.population {
        let mut values = Vec::with_capacity(ranges.len());
        for range in ranges {
            let (min, max) = range_bounds(range);
            state = splitmix64(state);
            let fraction = (state >> 11) as f64 / (1u64 << 53) as f64;
            values.push(clamp_value(range, min + (max - min) * fraction));
        }
        population.push(values);
    }
    let mut all = evaluate(&population)?;
    for _ in 1..config.generations {
        let mut ranked: Vec<usize> = (0..population.len()).collect();
        ranked.sort_by(|left, right| {
            all[*right]
                .score
                .total_cmp(&all[*left].score)
                .then(left.cmp(right))
        });
        let elite_count = (config.population / 5).max(1).min(population.len());
        let mut next: Vec<Vec<f64>> = ranked[..elite_count]
            .iter()
            .map(|index| population[*index].clone())
            .collect();
        while next.len() < config.population {
            let parent_a = tournament(&ranked, &all, &mut state, config.population);
            let parent_b = tournament(&ranked, &all, &mut state, config.population);
            let mut child = Vec::with_capacity(ranges.len());
            for (index, range) in ranges.iter().enumerate() {
                state = splitmix64(state);
                let fraction = (state >> 11) as f64 / (1u64 << 53) as f64;
                let mut value = population[parent_a][index]
                    + fraction * (population[parent_b][index] - population[parent_a][index]);
                state = splitmix64(state);
                let roll = (state >> 11) as f64 / (1u64 << 53) as f64;
                if roll < config.mutation_rate {
                    state = splitmix64(state);
                    let noise = (state >> 11) as f64 / (1u64 << 53) as f64;
                    let (min, max) = range_bounds(range);
                    value += (noise - 0.5) * (max - min) * 0.5;
                }
                child.push(clamp_value(range, value));
            }
            next.push(child);
        }
        population = next;
        all.extend(evaluate(&population)?);
    }
    Ok(all)
}

fn tournament(
    ranked: &[usize],
    candidates: &[Candidate],
    state: &mut u64,
    population: usize,
) -> usize {
    let mut best: Option<usize> = None;
    for _ in 0..3 {
        *state = splitmix64(*state);
        let pick = (*state as usize) % population;
        let index = ranked[pick];
        if best.is_none_or(|best| candidates[index].score > candidates[best].score) {
            best = Some(index);
        }
    }
    best.unwrap_or(ranked[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ranges_lists_and_single_values() {
        let range = parse_param("P.process_time.mean=2..6", 5).expect("range");
        assert_eq!(range.object, "P");
        assert_eq!(range.param, "process_time.mean");
        assert_eq!(range.values, vec![2.0, 3.0, 4.0, 5.0, 6.0]);

        let list = parse_param("P.x=1,4,9", 5).expect("list");
        assert_eq!(list.values, vec![1.0, 4.0, 9.0]);

        let single = parse_param("P.x=3", 5).expect("single");
        assert_eq!(single.values, vec![3.0]);
    }

    #[test]
    fn rejects_malformed_specs() {
        for spec in ["P.x", "P=2..6", "=2..6", "P.x=6..2", "P.x=a..b", "P.x=1,2,"] {
            assert!(parse_param(spec, 5).is_err(), "{spec} should be rejected");
        }
    }

    #[test]
    fn builds_the_cartesian_product_and_deterministic_samples() {
        let x = parse_param("P.x=1..3", 3).expect("x");
        let y = parse_param("P.y=10,20", 3).expect("y");
        let grid = combinations(&[x.clone(), y.clone()], None, 1).expect("grid");
        assert_eq!(grid.len(), 6);
        assert_eq!(grid[0], vec![1.0, 10.0]);
        assert_eq!(grid[1], vec![1.0, 20.0]);
        assert_eq!(grid[5], vec![3.0, 20.0]);

        let samples = combinations(&[x, y], Some(3), 7).expect("samples");
        assert_eq!(samples.len(), 3);
        assert_eq!(
            samples,
            combinations(
                &[
                    parse_param("P.x=1..3", 3).expect("x"),
                    parse_param("P.y=10,20", 3).expect("y")
                ],
                Some(3),
                7
            )
            .expect("samples")
        );
    }

    fn candidate(index: usize, score: f64, objective: Objective, value: f64) -> Candidate {
        Candidate {
            index,
            assignments: Vec::new(),
            metric: value,
            score,
            objective_values: vec![(objective, value)],
            created: 0.0,
            completed: 0.0,
            wip: 0.0,
            utilization: 0.0,
        }
    }

    #[test]
    fn selects_the_highest_weighted_score() {
        let candidates = vec![
            candidate(0, 2.0, Objective::Throughput, 2.0),
            candidate(1, 6.0, Objective::Throughput, 6.0),
        ];
        assert_eq!(best_index(&candidates), 1);

        let minimized = vec![
            candidate(0, -2.0, Objective::Wip, 2.0),
            candidate(1, -6.0, Objective::Wip, 6.0),
        ];
        assert_eq!(best_index(&minimized), 0);
    }

    #[test]
    fn pareto_front_keeps_the_trade_off_candidates() {
        let candidates = vec![
            Candidate {
                index: 0,
                assignments: Vec::new(),
                metric: 1.0,
                score: 0.0,
                objective_values: vec![(Objective::Throughput, 10.0), (Objective::Wip, 5.0)],
                created: 0.0,
                completed: 0.0,
                wip: 5.0,
                utilization: 0.0,
            },
            Candidate {
                index: 1,
                assignments: Vec::new(),
                metric: 1.0,
                score: 0.0,
                objective_values: vec![(Objective::Throughput, 5.0), (Objective::Wip, 1.0)],
                created: 0.0,
                completed: 0.0,
                wip: 1.0,
                utilization: 0.0,
            },
            Candidate {
                index: 2,
                assignments: Vec::new(),
                metric: 1.0,
                score: 0.0,
                objective_values: vec![(Objective::Throughput, 1.0), (Objective::Wip, 9.0)],
                created: 0.0,
                completed: 0.0,
                wip: 9.0,
                utilization: 0.0,
            },
        ];
        let front = pareto_indices(&candidates, &[Objective::Throughput, Objective::Wip]);
        assert_eq!(front, vec![0, 1]);
    }
}
