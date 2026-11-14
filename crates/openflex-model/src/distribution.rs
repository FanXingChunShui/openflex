//! Probability distributions used by the model format.
//!
//! FlexSim exposes a large family of statistical distributions for inter-arrival
//! times, processing times, failure times and so on. OpenFlex keeps the same
//! semantics for the distributions that are common in discrete-event models and
//! validates their parameters at model-load time (never at simulation time), so
//! a broken distribution is reported before the first event runs.

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

/// A probability distribution sampled by the simulation engine.
///
/// The JSON representation is tagged: `{"distribution": "exponential", "mean": 5.0}`.
///
/// The enum is internally tagged, and serde does not enforce
/// `deny_unknown_fields` for internally tagged enums; the hand-written
/// [`Deserialize`] implementation below rejects unknown keys explicitly (A1).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "distribution", rename_all = "snake_case")]
pub enum Distribution {
    /// Always returns the same value.
    Constant { value: f64 },
    /// Continuous uniform on `[min, max)`.
    Uniform { min: f64, max: f64 },
    /// Normal distribution; samples are not clamped.
    Normal { mean: f64, stddev: f64 },
    /// Exponential with the given mean (rate = 1 / mean).
    Exponential { mean: f64 },
    /// Triangular distribution with the usual three parameters.
    Triangular { min: f64, mode: f64, max: f64 },
    /// Gamma distribution parameterised by shape and scale.
    Gamma { shape: f64, scale: f64 },
    /// Weibull distribution parameterised by shape and scale.
    Weibull { shape: f64, scale: f64 },
    /// Lognormal distribution over the logarithm of the sample.
    Lognormal { meanlog: f64, sdlog: f64 },
    /// Erlang distribution: `shape` exponential phases with the given scale.
    Erlang { shape: u32, scale: f64 },
    /// Beta distribution on `[0, 1]`.
    Beta { alpha: f64, beta: f64 },
    /// Discrete uniform over the inclusive integer range.
    IntegerUniform { min: i64, max: i64 },
    /// Empirical distribution over fixed values, optionally weighted.
    Empirical {
        values: Vec<f64>,
        #[serde(default)]
        weights: Option<Vec<f64>>,
    },
    /// Pareto (type I) distribution: survival `(scale / x)^shape`.
    Pareto { shape: f64, scale: f64 },
    /// Log-logistic distribution with scale `alpha` and shape `beta`.
    LogLogistic { alpha: f64, beta: f64 },
    /// Rayleigh distribution with scale `sigma`.
    Rayleigh { sigma: f64 },
    /// Binomial distribution: `n` Bernoulli trials with success probability `p`.
    Binomial { n: u32, p: f64 },
    /// Poisson distribution with the given rate.
    Poisson { mean: f64 },
    /// Geometric distribution counting failures before the first success.
    Geometric { p: f64 },
    /// Negative binomial counting failures before the `r`-th success.
    NegativeBinomial { r: u32, p: f64 },
    /// Bernoulli distribution: 1 with probability `p`, 0 otherwise.
    Bernoulli { p: f64 },
}

/// Every distribution name, in the order used by `openflex distributions`.
pub const DISTRIBUTION_KINDS: &[&str] = &[
    "constant",
    "uniform",
    "normal",
    "exponential",
    "triangular",
    "gamma",
    "weibull",
    "lognormal",
    "erlang",
    "beta",
    "integer_uniform",
    "empirical",
    "pareto",
    "log_logistic",
    "rayleigh",
    "binomial",
    "poisson",
    "geometric",
    "negative_binomial",
    "bernoulli",
];

impl<'de> Deserialize<'de> for Distribution {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let mut map = value
            .as_object()
            .cloned()
            .ok_or_else(|| de::Error::custom("a distribution must be a JSON object"))?;
        let tag = map
            .remove("distribution")
            .ok_or_else(|| de::Error::custom("a distribution needs a \"distribution\" field"))?;
        let name = tag
            .as_str()
            .ok_or_else(|| de::Error::custom("\"distribution\" must be a string"))?
            .to_string();
        let distribution = match name.as_str() {
            "constant" => {
                check_keys(&map, &name, &["value"]).map_err(de::Error::custom)?;
                Distribution::Constant {
                    value: take(&mut map, "value", &name).map_err(de::Error::custom)?,
                }
            }
            "uniform" => {
                check_keys(&map, &name, &["min", "max"]).map_err(de::Error::custom)?;
                Distribution::Uniform {
                    min: take(&mut map, "min", &name).map_err(de::Error::custom)?,
                    max: take(&mut map, "max", &name).map_err(de::Error::custom)?,
                }
            }
            "normal" => {
                check_keys(&map, &name, &["mean", "stddev"]).map_err(de::Error::custom)?;
                Distribution::Normal {
                    mean: take(&mut map, "mean", &name).map_err(de::Error::custom)?,
                    stddev: take(&mut map, "stddev", &name).map_err(de::Error::custom)?,
                }
            }
            "exponential" => {
                check_keys(&map, &name, &["mean"]).map_err(de::Error::custom)?;
                Distribution::Exponential {
                    mean: take(&mut map, "mean", &name).map_err(de::Error::custom)?,
                }
            }
            "triangular" => {
                check_keys(&map, &name, &["min", "mode", "max"]).map_err(de::Error::custom)?;
                Distribution::Triangular {
                    min: take(&mut map, "min", &name).map_err(de::Error::custom)?,
                    mode: take(&mut map, "mode", &name).map_err(de::Error::custom)?,
                    max: take(&mut map, "max", &name).map_err(de::Error::custom)?,
                }
            }
            "gamma" => {
                check_keys(&map, &name, &["shape", "scale"]).map_err(de::Error::custom)?;
                Distribution::Gamma {
                    shape: take(&mut map, "shape", &name).map_err(de::Error::custom)?,
                    scale: take(&mut map, "scale", &name).map_err(de::Error::custom)?,
                }
            }
            "weibull" => {
                check_keys(&map, &name, &["shape", "scale"]).map_err(de::Error::custom)?;
                Distribution::Weibull {
                    shape: take(&mut map, "shape", &name).map_err(de::Error::custom)?,
                    scale: take(&mut map, "scale", &name).map_err(de::Error::custom)?,
                }
            }
            "lognormal" => {
                check_keys(&map, &name, &["meanlog", "sdlog"]).map_err(de::Error::custom)?;
                Distribution::Lognormal {
                    meanlog: take(&mut map, "meanlog", &name).map_err(de::Error::custom)?,
                    sdlog: take(&mut map, "sdlog", &name).map_err(de::Error::custom)?,
                }
            }
            "erlang" => {
                check_keys(&map, &name, &["shape", "scale"]).map_err(de::Error::custom)?;
                Distribution::Erlang {
                    shape: take(&mut map, "shape", &name).map_err(de::Error::custom)?,
                    scale: take(&mut map, "scale", &name).map_err(de::Error::custom)?,
                }
            }
            "beta" => {
                check_keys(&map, &name, &["alpha", "beta"]).map_err(de::Error::custom)?;
                Distribution::Beta {
                    alpha: take(&mut map, "alpha", &name).map_err(de::Error::custom)?,
                    beta: take(&mut map, "beta", &name).map_err(de::Error::custom)?,
                }
            }
            "integer_uniform" => {
                check_keys(&map, &name, &["min", "max"]).map_err(de::Error::custom)?;
                Distribution::IntegerUniform {
                    min: take(&mut map, "min", &name).map_err(de::Error::custom)?,
                    max: take(&mut map, "max", &name).map_err(de::Error::custom)?,
                }
            }
            "empirical" => {
                check_keys(&map, &name, &["values", "weights"]).map_err(de::Error::custom)?;
                Distribution::Empirical {
                    values: take(&mut map, "values", &name).map_err(de::Error::custom)?,
                    weights: take_optional(&mut map, "weights", &name)
                        .map_err(de::Error::custom)?,
                }
            }
            "pareto" => {
                check_keys(&map, &name, &["shape", "scale"]).map_err(de::Error::custom)?;
                Distribution::Pareto {
                    shape: take(&mut map, "shape", &name).map_err(de::Error::custom)?,
                    scale: take(&mut map, "scale", &name).map_err(de::Error::custom)?,
                }
            }
            "log_logistic" => {
                check_keys(&map, &name, &["alpha", "beta"]).map_err(de::Error::custom)?;
                Distribution::LogLogistic {
                    alpha: take(&mut map, "alpha", &name).map_err(de::Error::custom)?,
                    beta: take(&mut map, "beta", &name).map_err(de::Error::custom)?,
                }
            }
            "rayleigh" => {
                check_keys(&map, &name, &["sigma"]).map_err(de::Error::custom)?;
                Distribution::Rayleigh {
                    sigma: take(&mut map, "sigma", &name).map_err(de::Error::custom)?,
                }
            }
            "binomial" => {
                check_keys(&map, &name, &["n", "p"]).map_err(de::Error::custom)?;
                Distribution::Binomial {
                    n: take(&mut map, "n", &name).map_err(de::Error::custom)?,
                    p: take(&mut map, "p", &name).map_err(de::Error::custom)?,
                }
            }
            "poisson" => {
                check_keys(&map, &name, &["mean"]).map_err(de::Error::custom)?;
                Distribution::Poisson {
                    mean: take(&mut map, "mean", &name).map_err(de::Error::custom)?,
                }
            }
            "geometric" => {
                check_keys(&map, &name, &["p"]).map_err(de::Error::custom)?;
                Distribution::Geometric {
                    p: take(&mut map, "p", &name).map_err(de::Error::custom)?,
                }
            }
            "negative_binomial" => {
                check_keys(&map, &name, &["r", "p"]).map_err(de::Error::custom)?;
                Distribution::NegativeBinomial {
                    r: take(&mut map, "r", &name).map_err(de::Error::custom)?,
                    p: take(&mut map, "p", &name).map_err(de::Error::custom)?,
                }
            }
            "bernoulli" => {
                check_keys(&map, &name, &["p"]).map_err(de::Error::custom)?;
                Distribution::Bernoulli {
                    p: take(&mut map, "p", &name).map_err(de::Error::custom)?,
                }
            }
            other => {
                return Err(de::Error::custom(format!(
                    "unknown distribution '{other}' (expected one of: {})",
                    DISTRIBUTION_KINDS.join(", ")
                )));
            }
        };
        Ok(distribution)
    }
}

fn check_keys(
    map: &serde_json::Map<String, serde_json::Value>,
    name: &str,
    allowed: &[&str],
) -> Result<(), String> {
    for key in map.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!(
                "distribution '{name}' has unknown key '{key}' (expected one of: {})",
                allowed.join(", ")
            ));
        }
    }
    Ok(())
}

fn take<T: serde::de::DeserializeOwned>(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    name: &str,
) -> Result<T, String> {
    let value = map
        .remove(key)
        .ok_or_else(|| format!("distribution '{name}' requires '{key}'"))?;
    serde_json::from_value(value).map_err(|err| format!("distribution '{name}.{key}': {err}"))
}

fn take_optional<T: serde::de::DeserializeOwned>(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    name: &str,
) -> Result<Option<T>, String> {
    match map.remove(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => serde_json::from_value(value)
            .map(Some)
            .map_err(|err| format!("distribution '{name}.{key}': {err}")),
    }
}

impl Distribution {
    /// Validate every parameter; returns a human readable reason on failure.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Distribution::Constant { value } => finite(*value, "value"),
            Distribution::Uniform { min, max } => {
                finite(*min, "min")?;
                finite(*max, "max")?;
                if max > min {
                    Ok(())
                } else {
                    Err("uniform requires max > min".into())
                }
            }
            Distribution::Normal { mean, stddev } => {
                finite(*mean, "mean")?;
                finite(*stddev, "stddev")?;
                if *stddev >= 0.0 {
                    Ok(())
                } else {
                    Err("normal requires stddev >= 0".into())
                }
            }
            Distribution::Exponential { mean } => {
                finite(*mean, "mean")?;
                if *mean > 0.0 {
                    Ok(())
                } else {
                    Err("exponential requires mean > 0".into())
                }
            }
            Distribution::Triangular { min, mode, max } => {
                finite(*min, "min")?;
                finite(*mode, "mode")?;
                finite(*max, "max")?;
                if min <= mode && mode <= max && max > min {
                    Ok(())
                } else {
                    Err("triangular requires min <= mode <= max and max > min".into())
                }
            }
            Distribution::Gamma { shape, scale } => {
                finite(*shape, "shape")?;
                finite(*scale, "scale")?;
                if *shape > 0.0 && *scale > 0.0 {
                    Ok(())
                } else {
                    Err("gamma requires shape > 0 and scale > 0".into())
                }
            }
            Distribution::Weibull { shape, scale } => {
                finite(*shape, "shape")?;
                finite(*scale, "scale")?;
                if *shape > 0.0 && *scale > 0.0 {
                    Ok(())
                } else {
                    Err("weibull requires shape > 0 and scale > 0".into())
                }
            }
            Distribution::Lognormal { meanlog, sdlog } => {
                finite(*meanlog, "meanlog")?;
                finite(*sdlog, "sdlog")?;
                if *sdlog >= 0.0 {
                    Ok(())
                } else {
                    Err("lognormal requires sdlog >= 0".into())
                }
            }
            Distribution::Erlang { shape, scale } => {
                finite(*scale, "scale")?;
                if *shape > 0 && *scale > 0.0 {
                    Ok(())
                } else {
                    Err("erlang requires shape > 0 and scale > 0".into())
                }
            }
            Distribution::Beta { alpha, beta } => {
                finite(*alpha, "alpha")?;
                finite(*beta, "beta")?;
                if *alpha > 0.0 && *beta > 0.0 {
                    Ok(())
                } else {
                    Err("beta requires alpha > 0 and beta > 0".into())
                }
            }
            Distribution::IntegerUniform { min, max } => {
                if max >= min {
                    Ok(())
                } else {
                    Err("integer_uniform requires max >= min".into())
                }
            }
            Distribution::Empirical { values, weights } => {
                if values.is_empty() {
                    return Err("empirical requires at least one value".into());
                }
                for value in values {
                    finite(*value, "values")?;
                }
                if let Some(weights) = weights {
                    if weights.len() != values.len() {
                        return Err("empirical weights must match values".into());
                    }
                    let mut sum = 0.0;
                    for weight in weights {
                        finite(*weight, "weights")?;
                        if *weight < 0.0 {
                            return Err("empirical weights must be non-negative".into());
                        }
                        sum += weight;
                    }
                    if sum <= 0.0 {
                        return Err("empirical weights must sum to a positive value".into());
                    }
                }
                Ok(())
            }
            Distribution::Pareto { shape, scale } => {
                finite(*shape, "shape")?;
                finite(*scale, "scale")?;
                if *shape > 0.0 && *scale > 0.0 {
                    Ok(())
                } else {
                    Err("pareto requires shape > 0 and scale > 0".into())
                }
            }
            Distribution::LogLogistic { alpha, beta } => {
                finite(*alpha, "alpha")?;
                finite(*beta, "beta")?;
                if *alpha > 0.0 && *beta > 0.0 {
                    Ok(())
                } else {
                    Err("log_logistic requires alpha > 0 and beta > 0".into())
                }
            }
            Distribution::Rayleigh { sigma } => {
                finite(*sigma, "sigma")?;
                if *sigma > 0.0 {
                    Ok(())
                } else {
                    Err("rayleigh requires sigma > 0".into())
                }
            }
            Distribution::Binomial { n, p } => {
                finite(*p, "p")?;
                if *n >= 1 && (0.0..=1.0).contains(p) {
                    Ok(())
                } else {
                    Err("binomial requires n >= 1 and 0 <= p <= 1".into())
                }
            }
            Distribution::Poisson { mean } => {
                finite(*mean, "mean")?;
                if *mean > 0.0 {
                    Ok(())
                } else {
                    Err("poisson requires mean > 0".into())
                }
            }
            Distribution::Geometric { p } => {
                finite(*p, "p")?;
                if *p > 0.0 && *p <= 1.0 {
                    Ok(())
                } else {
                    Err("geometric requires 0 < p <= 1".into())
                }
            }
            Distribution::NegativeBinomial { r, p } => {
                finite(*p, "p")?;
                if *r >= 1 && *p > 0.0 && *p <= 1.0 {
                    Ok(())
                } else {
                    Err("negative_binomial requires r >= 1 and 0 < p <= 1".into())
                }
            }
            Distribution::Bernoulli { p } => {
                finite(*p, "p")?;
                if (0.0..=1.0).contains(p) {
                    Ok(())
                } else {
                    Err("bernoulli requires 0 <= p <= 1".into())
                }
            }
        }
    }

    /// Analytic mean, used by validation hints and by the CLI's summary output.
    /// Returns `None` for distributions without a meaningful closed form
    /// (for example a Pareto with `shape <= 1`, whose mean is infinite).
    pub fn mean(&self) -> Option<f64> {
        match self {
            Distribution::Constant { value } => Some(*value),
            Distribution::Uniform { min, max } => Some((min + max) / 2.0),
            Distribution::Normal { mean, .. } => Some(*mean),
            Distribution::Exponential { mean } => Some(*mean),
            Distribution::Triangular { min, mode, max } => Some((min + mode + max) / 3.0),
            Distribution::Gamma { shape, scale } => Some(shape * scale),
            Distribution::Weibull { shape, scale } => Some(scale * gamma_fn(1.0 + 1.0 / shape)),
            Distribution::Lognormal { meanlog, sdlog } => {
                Some((meanlog + sdlog * sdlog / 2.0).exp())
            }
            Distribution::Erlang { shape, scale } => Some(*scale * f64::from(*shape)),
            Distribution::Beta { alpha, beta } => Some(alpha / (alpha + beta)),
            Distribution::IntegerUniform { min, max } => Some((*min as f64 + *max as f64) / 2.0),
            Distribution::Empirical { values, weights } => {
                let weights = weights.as_ref()?;
                let sum: f64 = weights.iter().sum();
                if sum <= 0.0 {
                    return None;
                }
                Some(values.iter().zip(weights).map(|(v, w)| v * w).sum::<f64>() / sum)
            }
            Distribution::Pareto { shape, scale } => {
                if *shape > 1.0 {
                    Some(scale * shape / (shape - 1.0))
                } else {
                    None
                }
            }
            Distribution::LogLogistic { alpha, beta } => {
                if *beta > 1.0 {
                    let factor = std::f64::consts::PI / beta;
                    Some(alpha * factor / factor.sin())
                } else {
                    None
                }
            }
            Distribution::Rayleigh { sigma } => Some(sigma * (std::f64::consts::PI / 2.0).sqrt()),
            Distribution::Binomial { n, p } => Some(f64::from(*n) * p),
            Distribution::Poisson { mean } => Some(*mean),
            Distribution::Geometric { p } => Some((1.0 - p) / p),
            Distribution::NegativeBinomial { r, p } => Some(f64::from(*r) * (1.0 - p) / p),
            Distribution::Bernoulli { p } => Some(*p),
        }
    }

    /// Human readable name used in reports and error messages.
    pub fn kind(&self) -> &'static str {
        match self {
            Distribution::Constant { .. } => "constant",
            Distribution::Uniform { .. } => "uniform",
            Distribution::Normal { .. } => "normal",
            Distribution::Exponential { .. } => "exponential",
            Distribution::Triangular { .. } => "triangular",
            Distribution::Gamma { .. } => "gamma",
            Distribution::Weibull { .. } => "weibull",
            Distribution::Lognormal { .. } => "lognormal",
            Distribution::Erlang { .. } => "erlang",
            Distribution::Beta { .. } => "beta",
            Distribution::IntegerUniform { .. } => "integer_uniform",
            Distribution::Empirical { .. } => "empirical",
            Distribution::Pareto { .. } => "pareto",
            Distribution::LogLogistic { .. } => "log_logistic",
            Distribution::Rayleigh { .. } => "rayleigh",
            Distribution::Binomial { .. } => "binomial",
            Distribution::Poisson { .. } => "poisson",
            Distribution::Geometric { .. } => "geometric",
            Distribution::NegativeBinomial { .. } => "negative_binomial",
            Distribution::Bernoulli { .. } => "bernoulli",
        }
    }
}

/// Default distribution used when a model omits a timing parameter.
impl Default for Distribution {
    fn default() -> Self {
        Distribution::Constant { value: 0.0 }
    }
}

fn finite(value: f64, name: &str) -> Result<(), String> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(format!("{name} must be a finite number"))
    }
}

/// Lanczos approximation of the gamma function, used for the Weibull mean.
fn gamma_fn(x: f64) -> f64 {
    const COEFFS: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        return std::f64::consts::PI / ((std::f64::consts::PI * x).sin() * gamma_fn(1.0 - x));
    }
    let x = x - 1.0;
    let mut a = COEFFS[0];
    let t = x + 7.5;
    for (i, c) in COEFFS.iter().enumerate().skip(1) {
        a += c / (x + i as f64);
    }
    (2.0 * std::f64::consts::PI).sqrt() * t.powf(x + 0.5) * (-t).exp() * a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Result<Distribution, String> {
        serde_json::from_str(json).map_err(|err| err.to_string())
    }

    #[test]
    fn validates_parameters() {
        assert!(Distribution::Exponential { mean: 5.0 }.validate().is_ok());
        assert!(Distribution::Exponential { mean: 0.0 }.validate().is_err());
        assert!(
            Distribution::Triangular {
                min: 1.0,
                mode: 2.0,
                max: 3.0
            }
            .validate()
            .is_ok()
        );
        assert!(
            Distribution::Triangular {
                min: 3.0,
                mode: 2.0,
                max: 1.0
            }
            .validate()
            .is_err()
        );
        assert!(
            Distribution::Empirical {
                values: vec![],
                weights: None
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn computes_means() {
        assert_eq!(
            Distribution::Uniform { min: 2.0, max: 4.0 }.mean(),
            Some(3.0)
        );
        assert!((Distribution::Exponential { mean: 5.0 }.mean().unwrap() - 5.0).abs() < 1e-12);
        assert!(
            (Distribution::Erlang {
                shape: 3,
                scale: 2.0
            }
            .mean()
            .unwrap()
                - 6.0)
                .abs()
                < 1e-12
        );
    }

    #[test]
    fn validates_extended_distributions() {
        assert!(
            Distribution::Pareto {
                shape: 2.0,
                scale: 1.0
            }
            .validate()
            .is_ok()
        );
        assert!(
            Distribution::Pareto {
                shape: 0.0,
                scale: 1.0
            }
            .validate()
            .is_err()
        );
        assert!(
            Distribution::LogLogistic {
                alpha: 2.0,
                beta: 3.0
            }
            .validate()
            .is_ok()
        );
        assert!(
            Distribution::LogLogistic {
                alpha: 2.0,
                beta: 0.0
            }
            .validate()
            .is_err()
        );
        assert!(Distribution::Rayleigh { sigma: 2.0 }.validate().is_ok());
        assert!(Distribution::Rayleigh { sigma: -1.0 }.validate().is_err());
        assert!(Distribution::Binomial { n: 5, p: 0.5 }.validate().is_ok());
        assert!(Distribution::Binomial { n: 0, p: 0.5 }.validate().is_err());
        assert!(Distribution::Binomial { n: 5, p: 1.5 }.validate().is_err());
        assert!(Distribution::Poisson { mean: 2.0 }.validate().is_ok());
        assert!(Distribution::Poisson { mean: 0.0 }.validate().is_err());
        assert!(Distribution::Geometric { p: 0.25 }.validate().is_ok());
        assert!(Distribution::Geometric { p: 0.0 }.validate().is_err());
        assert!(
            Distribution::NegativeBinomial { r: 3, p: 0.5 }
                .validate()
                .is_ok()
        );
        assert!(
            Distribution::NegativeBinomial { r: 0, p: 0.5 }
                .validate()
                .is_err()
        );
        assert!(Distribution::Bernoulli { p: 0.0 }.validate().is_ok());
        assert!(Distribution::Bernoulli { p: 1.0 }.validate().is_ok());
        assert!(Distribution::Bernoulli { p: -0.1 }.validate().is_err());
    }

    #[test]
    fn computes_extended_means() {
        assert!(
            (Distribution::Pareto {
                shape: 2.0,
                scale: 3.0
            }
            .mean()
            .unwrap()
                - 6.0)
                .abs()
                < 1e-12
        );
        assert!(
            Distribution::Pareto {
                shape: 1.0,
                scale: 3.0
            }
            .mean()
            .is_none()
        );
        assert!(
            (Distribution::Rayleigh { sigma: 2.0 }.mean().unwrap()
                - 2.0 * (std::f64::consts::PI / 2.0).sqrt())
            .abs()
                < 1e-12
        );
        assert_eq!(Distribution::Binomial { n: 10, p: 0.3 }.mean(), Some(3.0));
        assert_eq!(Distribution::Poisson { mean: 4.5 }.mean(), Some(4.5));
        assert_eq!(Distribution::Geometric { p: 0.25 }.mean(), Some(3.0));
        assert_eq!(
            Distribution::NegativeBinomial { r: 4, p: 0.5 }.mean(),
            Some(4.0)
        );
        assert_eq!(Distribution::Bernoulli { p: 0.2 }.mean(), Some(0.2));
        // Log-logistic with beta=2: mean = alpha * (pi/2) / sin(pi/2).
        let log_logistic = Distribution::LogLogistic {
            alpha: 3.0,
            beta: 2.0,
        };
        assert!((log_logistic.mean().unwrap() - 3.0 * std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert!(
            Distribution::LogLogistic {
                alpha: 1.0,
                beta: 1.0
            }
            .mean()
            .is_none()
        );
    }

    #[test]
    fn kind_and_serde_roundtrip() {
        let cases = [
            (
                Distribution::Pareto {
                    shape: 2.0,
                    scale: 1.0,
                },
                "pareto",
            ),
            (
                Distribution::LogLogistic {
                    alpha: 2.0,
                    beta: 3.0,
                },
                "log_logistic",
            ),
            (Distribution::Rayleigh { sigma: 1.0 }, "rayleigh"),
            (Distribution::Binomial { n: 3, p: 0.5 }, "binomial"),
            (Distribution::Poisson { mean: 2.0 }, "poisson"),
            (Distribution::Geometric { p: 0.5 }, "geometric"),
            (
                Distribution::NegativeBinomial { r: 2, p: 0.5 },
                "negative_binomial",
            ),
            (Distribution::Bernoulli { p: 0.5 }, "bernoulli"),
        ];
        for (distribution, name) in cases {
            assert_eq!(distribution.kind(), name);
            let json = serde_json::to_string(&distribution).unwrap();
            let parsed: Distribution = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, distribution);
            assert!(json.contains(name), "{json}");
        }
    }

    #[test]
    fn unknown_parameters_are_rejected() {
        let error = parse(r#"{"distribution":"exponential","mean":5.0,"rate":0.2}"#)
            .expect_err("unknown key must be rejected");
        assert!(error.contains("unknown key 'rate'"), "{error}");
        let error = parse(r#"{"distribution":"pareto","shape":2.0,"scale":1.0,"x":1}"#)
            .expect_err("unknown key must be rejected");
        assert!(error.contains("unknown key 'x'"), "{error}");
        let error = parse(r#"{"distribution":"warp_drive","speed":1.0}"#)
            .expect_err("unknown distribution must be rejected");
        assert!(error.contains("unknown distribution"), "{error}");
    }
}
