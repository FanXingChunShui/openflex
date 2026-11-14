//! Probability distributions used by the model format.
//!
//! FlexSim exposes a large family of statistical distributions for inter-arrival
//! times, processing times, failure times and so on. OpenFlex keeps the same
//! semantics for the distributions that are common in discrete-event models and
//! validates their parameters at model-load time (never at simulation time), so
//! a broken distribution is reported before the first event runs.

use serde::{Deserialize, Serialize};

/// A probability distribution sampled by the simulation engine.
///
/// The JSON representation is tagged: `{"distribution": "exponential", "mean": 5.0}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
        }
    }

    /// Analytic mean, used by validation hints and by the CLI's summary output.
    /// Returns `None` for distributions without a meaningful closed form.
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
}
