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
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
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
    pub metric: f64,
    pub created: f64,
    pub completed: f64,
    pub wip: f64,
    pub utilization: f64,
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
    objective: Objective,
    object: Option<&str>,
) -> Result<Candidate, String> {
    let assignments = ranges
        .iter()
        .zip(values)
        .map(|(range, value)| (range.key(), *value))
        .collect();
    Ok(Candidate {
        index,
        assignments,
        metric: metric(report, objective, object)?,
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
pub fn best_index(candidates: &[Candidate], objective: Objective) -> usize {
    let mut best = 0;
    for (index, candidate) in candidates.iter().enumerate().skip(1) {
        let better = if objective.maximize() {
            candidate.metric > candidates[best].metric
        } else {
            candidate.metric < candidates[best].metric
        };
        if better {
            best = index;
        }
    }
    best
}

/// Render the search result.
pub fn render(
    model: &str,
    objective: Objective,
    object: Option<&str>,
    ranges: &[ParamRange],
    candidates: &[Candidate],
    best: usize,
    format: Format,
) -> Result<String, String> {
    Ok(match format {
        Format::Json => to_json(model, objective, object, candidates, best)?,
        Format::Csv => to_csv(objective, ranges, candidates, best),
        Format::Table => to_table(model, objective, object, ranges, candidates, best),
        Format::Markdown => to_markdown(model, objective, object, ranges, candidates, best),
    })
}

fn to_json(
    model: &str,
    objective: Objective,
    object: Option<&str>,
    candidates: &[Candidate],
    best: usize,
) -> Result<String, String> {
    let values: Vec<serde_json::Value> = candidates
        .iter()
        .map(|candidate| {
            let parameters: serde_json::Map<String, serde_json::Value> = candidate
                .assignments
                .iter()
                .map(|(key, value)| (key.clone(), json!(value)))
                .collect();
            json!({
                "index": candidate.index,
                "parameters": parameters,
                "metric": candidate.metric,
                "created": candidate.created,
                "completed": candidate.completed,
                "avg_wip": candidate.wip,
                "utilization": candidate.utilization,
            })
        })
        .collect();
    let document = json!({
        "model": model,
        "objective": objective.label(),
        "direction": if objective.maximize() { "maximize" } else { "minimize" },
        "object": object,
        "best": &values[best],
        "candidates": values,
    });
    serde_json::to_string_pretty(&document).map_err(|error| error.to_string())
}

fn to_csv(
    objective: Objective,
    ranges: &[ParamRange],
    candidates: &[Candidate],
    best: usize,
) -> String {
    let mut out = String::new();
    out.push_str("rank");
    for range in ranges {
        out.push(',');
        out.push_str(&range.key());
    }
    out.push_str(&format!(
        ",{},best,created,completed,avg_wip,utilization\n",
        objective.label()
    ));
    let mut ranked: Vec<&Candidate> = candidates.iter().collect();
    sort_candidates(&mut ranked, objective);
    for (rank, candidate) in ranked.iter().enumerate() {
        out.push_str(&format!("{}", rank + 1));
        for (_, value) in &candidate.assignments {
            out.push_str(&format!(",{value}"));
        }
        out.push_str(&format!(
            ",{},{},{:.3},{:.3},{:.3},{:.4}\n",
            candidate.metric,
            candidate.index == best,
            candidate.created,
            candidate.completed,
            candidate.wip,
            candidate.utilization
        ));
    }
    out
}

fn to_table(
    model: &str,
    objective: Objective,
    object: Option<&str>,
    _ranges: &[ParamRange],
    candidates: &[Candidate],
    best: usize,
) -> String {
    let mut out = String::new();
    let direction = if objective.maximize() {
        "maximize"
    } else {
        "minimize"
    };
    out.push_str(&format!(
        "OpenFlex optimize: {model} (objective {} {}{}, {} candidates)\n",
        objective.label(),
        direction,
        object.map(|name| format!(" on {name}")).unwrap_or_default(),
        candidates.len()
    ));
    out.push_str(&format!(
        "{:<28} {:>10} {:>10} {:>10} {:>10} {:>10}\n",
        "parameters", "metric", "created", "completed", "avg_wip", "util"
    ));
    out.push_str(&"-".repeat(82));
    out.push('\n');
    let mut ranked: Vec<&Candidate> = candidates.iter().collect();
    sort_candidates(&mut ranked, objective);
    for (rank, candidate) in ranked.iter().enumerate() {
        let parameters = candidate
            .assignments
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(" ");
        let marker = if candidate.index == best { "*" } else { " " };
        out.push_str(&format!(
            "{marker}{rank:<2} {:<24} {:>10.4} {:>10.2} {:>10.2} {:>10.2} {:>9.1}%\n",
            parameters,
            candidate.metric,
            candidate.created,
            candidate.completed,
            candidate.wip,
            candidate.utilization * 100.0
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
            "\nbest: {parameters} -> {} {:.4}\n",
            objective.label(),
            best.metric
        ));
    }
    out
}

fn to_markdown(
    model: &str,
    objective: Objective,
    object: Option<&str>,
    _ranges: &[ParamRange],
    candidates: &[Candidate],
    best: usize,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# OpenFlex optimize: {model}\n\nObjective: {} ({})",
        objective.label(),
        if objective.maximize() {
            "maximize"
        } else {
            "minimize"
        }
    ));
    if let Some(name) = object {
        out.push_str(&format!(" on `{name}`"));
    }
    out.push_str(
        "\n\n| Rank | Parameters | Metric | Created | Completed | Avg WIP | Utilisation |\n",
    );
    out.push_str("|---|---|---:|---:|---:|---:|---:|\n");
    let mut ranked: Vec<&Candidate> = candidates.iter().collect();
    sort_candidates(&mut ranked, objective);
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
            "| {} | {parameters} | {:.4} | {:.2} | {:.2} | {:.2} | {:.1}%{} |\n",
            rank + 1,
            candidate.metric,
            candidate.created,
            candidate.completed,
            candidate.wip,
            candidate.utilization * 100.0,
            marker
        ));
    }
    out
}

fn sort_candidates(candidates: &mut Vec<&Candidate>, objective: Objective) {
    candidates.sort_by(|left, right| {
        let order = left.metric.total_cmp(&right.metric);
        let order = if objective.maximize() {
            order.reverse()
        } else {
            order
        };
        order.then(left.index.cmp(&right.index))
    });
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

    #[test]
    fn selects_the_better_end_of_the_objective() {
        let candidates = vec![
            Candidate {
                index: 0,
                assignments: Vec::new(),
                metric: 2.0,
                created: 0.0,
                completed: 0.0,
                wip: 0.0,
                utilization: 0.0,
            },
            Candidate {
                index: 1,
                assignments: Vec::new(),
                metric: 6.0,
                created: 0.0,
                completed: 0.0,
                wip: 0.0,
                utilization: 0.0,
            },
        ];
        assert_eq!(best_index(&candidates, Objective::Throughput), 1);
        assert_eq!(best_index(&candidates, Objective::Wip), 0);
    }
}
