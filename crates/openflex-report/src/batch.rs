//! Batch-means confidence intervals (A12).
//!
//! With several replications, the ordinary replication interval treats each
//! replication as one independent sample. Batch means groups consecutive
//! replications into `k` batches, averages inside each batch, and then treats
//! the batch means as the independent sample. The resulting interval is useful
//! when the replication-level samples are noisy or when a long run has been
//! split into segments.
//!
//! The batch means here are computed over the per-replication system totals
//! (items created/completed/in system). Each batch mean is the mean of its
//! replications; the variance is the sample variance of the batch means and
//! the 95% interval uses the Student t quantile with `k - 1` degrees of freedom.

use openflex_core::{SimReport, t_critical_95};

use crate::ReportError;

/// Mean/variance/interval of one metric's batches.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchMeans {
    pub per_batch: Vec<f64>,
    pub mean: f64,
    pub variance: f64,
    pub ci95: f64,
    pub samples: u32,
    pub batches: u32,
}

/// Batch means for the three system-level totals shown in the summary.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchMeansSummary {
    pub batches: u32,
    pub samples: u32,
    pub items_created: BatchMeans,
    pub items_completed: BatchMeans,
    pub items_in_system: BatchMeans,
}

/// Compute batch means over the report's replications.
pub fn batch_means(report: &SimReport, batches: u32) -> Result<BatchMeansSummary, ReportError> {
    let samples = report.replications.len() as u32;
    if batches < 2 {
        return Err(ReportError::BatchMeans(
            "batch means need at least two batches".into(),
        ));
    }
    if samples < batches {
        return Err(ReportError::BatchMeans(format!(
            "batch means need at least as many replications ({samples}) as batches ({batches})"
        )));
    }
    let created: Vec<f64> = report
        .replications
        .iter()
        .map(|replication| replication.items_created as f64)
        .collect();
    let completed: Vec<f64> = report
        .replications
        .iter()
        .map(|replication| replication.items_completed as f64)
        .collect();
    let in_system: Vec<f64> = report
        .replications
        .iter()
        .map(|replication| replication.items_in_system as f64)
        .collect();
    Ok(BatchMeansSummary {
        batches,
        samples,
        items_created: compute(&created, batches),
        items_completed: compute(&completed, batches),
        items_in_system: compute(&in_system, batches),
    })
}

fn compute(samples: &[f64], batches: u32) -> BatchMeans {
    let count = batches as usize;
    let base = samples.len() / count;
    let remainder = samples.len() % count;
    let mut per_batch = Vec::with_capacity(count);
    let mut start = 0;
    for index in 0..count {
        let size = base + usize::from(index < remainder);
        let batch = &samples[start..start + size];
        per_batch.push(batch.iter().sum::<f64>() / size as f64);
        start += size;
    }
    let mean = per_batch.iter().sum::<f64>() / count as f64;
    let variance = if count > 1 {
        per_batch
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / (count - 1) as f64
    } else {
        0.0
    };
    let critical = t_critical_95(count.saturating_sub(1) as u32);
    let ci95 = critical * (variance / count as f64).sqrt();
    BatchMeans {
        per_batch,
        mean,
        variance,
        ci95,
        samples: samples.len() as u32,
        batches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_samples_into_equal_batches() {
        let values = [1.0, 3.0, 5.0, 7.0];
        let means = compute(&values, 2);
        assert_eq!(means.per_batch, vec![2.0, 6.0]);
        assert_eq!(means.mean, 4.0);
        assert_eq!(means.variance, 8.0);
        assert!(means.ci95 > 0.0);
    }

    #[test]
    fn handles_uneven_batches() {
        let means = compute(&[1.0, 3.0, 5.0], 2);
        assert_eq!(means.per_batch, vec![2.0, 5.0]);
        assert_eq!(means.mean, 3.5);
        assert_eq!(means.variance, 4.5);
    }
}
