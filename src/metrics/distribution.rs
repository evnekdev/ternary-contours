//! Deterministic finite-value summaries and configurable histograms.

use core::fmt;

/// Quantiles calculated with inclusive linear interpolation at `p*(n-1)`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DistributionQuantiles {
    pub p0: Option<f64>,
    pub p5: Option<f64>,
    pub p25: Option<f64>,
    pub p50: Option<f64>,
    pub p75: Option<f64>,
    pub p95: Option<f64>,
    pub p100: Option<f64>,
}

/// Deterministic summary of a finite scalar distribution.
///
/// Empty input has `count == 0` and `None` for all scalar statistics. A
/// constant distribution has zero standard deviation and no coefficient of
/// variation because its mean-relative spread is undefined at zero mean.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DistributionSummary {
    pub count: usize,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub mean: Option<f64>,
    pub standard_deviation: Option<f64>,
    pub coefficient_of_variation: Option<f64>,
    pub quantiles: DistributionQuantiles,
}

impl DistributionSummary {
    /// Summarize finite values without retaining caller data.
    pub fn from_values(values: &[f64]) -> Result<Self, DistributionError> {
        if let Some((index, value)) = values
            .iter()
            .copied()
            .enumerate()
            .find(|(_, value)| !value.is_finite())
        {
            return Err(DistributionError::NonFiniteValue { index, value });
        }
        if values.is_empty() {
            return Ok(Self::default());
        }
        let mut ordered = values.to_vec();
        ordered.sort_by(f64::total_cmp);
        let count = ordered.len();
        let minimum = ordered[0];
        let maximum = ordered[count - 1];
        let mean = ordered.iter().sum::<f64>() / count as f64;
        let variance = ordered
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / count as f64;
        let standard_deviation = variance.sqrt();
        let coefficient_of_variation = (mean != 0.0).then_some(standard_deviation / mean.abs());
        Ok(Self {
            count,
            minimum: Some(minimum),
            maximum: Some(maximum),
            mean: Some(mean),
            standard_deviation: Some(standard_deviation),
            coefficient_of_variation,
            quantiles: DistributionQuantiles {
                p0: Some(minimum),
                p5: Some(quantile(&ordered, 0.05)),
                p25: Some(quantile(&ordered, 0.25)),
                p50: Some(quantile(&ordered, 0.5)),
                p75: Some(quantile(&ordered, 0.75)),
                p95: Some(quantile(&ordered, 0.95)),
                p100: Some(maximum),
            },
        })
    }
}

/// Histogram boundary selection independent from summary construction.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum HistogramBinning {
    EqualWidth { bins: usize },
    Logarithmic { bins: usize },
    Quantile { bins: usize },
    Explicit(Vec<f64>),
}

/// A histogram with explicit underflow and overflow counts.
///
/// Bins are half-open `[edge[i], edge[i+1])`, except that the final upper edge
/// belongs to the final bin. Explicit edges therefore make out-of-range input
/// visible rather than silently discarding it.
#[derive(Clone, Debug, PartialEq)]
pub struct Histogram {
    pub edges: Vec<f64>,
    pub counts: Vec<usize>,
    pub underflow_count: usize,
    pub overflow_count: usize,
}

impl Histogram {
    /// Construct a histogram from finite observations.
    pub fn from_values(values: &[f64], binning: HistogramBinning) -> Result<Self, HistogramError> {
        if let Some((index, value)) = values
            .iter()
            .copied()
            .enumerate()
            .find(|(_, value)| !value.is_finite())
        {
            return Err(HistogramError::NonFiniteValue { index, value });
        }
        if values.is_empty() {
            return Err(HistogramError::EmptyInput);
        }
        let mut ordered = values.to_vec();
        ordered.sort_by(f64::total_cmp);
        let edges = match binning {
            HistogramBinning::EqualWidth { bins } => equal_width_edges(&ordered, bins)?,
            HistogramBinning::Logarithmic { bins } => logarithmic_edges(&ordered, bins)?,
            HistogramBinning::Quantile { bins } => quantile_edges(&ordered, bins)?,
            HistogramBinning::Explicit(edges) => {
                validate_edges(&edges)?;
                edges
            }
        };
        let mut histogram = Self {
            counts: vec![0; edges.len() - 1],
            edges,
            underflow_count: 0,
            overflow_count: 0,
        };
        for value in values {
            if *value < histogram.edges[0] {
                histogram.underflow_count += 1;
            } else if *value > *histogram.edges.last().expect("validated edges") {
                histogram.overflow_count += 1;
            } else {
                let index = histogram
                    .edges
                    .partition_point(|edge| *edge <= *value)
                    .saturating_sub(1)
                    .min(histogram.counts.len() - 1);
                histogram.counts[index] += 1;
            }
        }
        Ok(histogram)
    }
}

/// Failure while summarizing finite values.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum DistributionError {
    NonFiniteValue { index: usize, value: f64 },
}
impl fmt::Display for DistributionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteValue { index, value } => {
                write!(f, "distribution value {index} is not finite: {value:?}")
            }
        }
    }
}
impl std::error::Error for DistributionError {}

/// Failure while constructing a histogram.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum HistogramError {
    EmptyInput,
    NonFiniteValue { index: usize, value: f64 },
    InvalidBinCount { bins: usize },
    InvalidEdges,
    NonPositiveLogarithmicValue { index: usize, value: f64 },
    RepeatedQuantileBoundary,
}
impl fmt::Display for HistogramError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => f.write_str("histogram construction requires at least one value"),
            Self::NonFiniteValue { index, value } => {
                write!(f, "histogram value {index} is not finite: {value:?}")
            }
            Self::InvalidBinCount { bins } => write!(f, "histogram bin count must be positive: {bins}"),
            Self::InvalidEdges => f.write_str("histogram edges must be finite and strictly increasing"),
            Self::NonPositiveLogarithmicValue { index, value } => write!(
                f,
                "logarithmic histogram value {index} must be positive: {value:?}"
            ),
            Self::RepeatedQuantileBoundary => f.write_str(
                "requested quantile histogram has repeated boundaries; use fewer bins or explicit edges",
            ),
        }
    }
}
impl std::error::Error for HistogramError {}

fn quantile(ordered: &[f64], probability: f64) -> f64 {
    let position = probability * (ordered.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower as f64)
}

fn equal_width_edges(ordered: &[f64], bins: usize) -> Result<Vec<f64>, HistogramError> {
    if bins == 0 {
        return Err(HistogramError::InvalidBinCount { bins });
    }
    let minimum = ordered[0];
    let maximum = ordered[ordered.len() - 1];
    if minimum == maximum {
        return Ok(constant_edges(minimum, bins));
    }
    let width = (maximum - minimum) / bins as f64;
    let edges = (0..=bins)
        .map(|index| minimum + width * index as f64)
        .collect::<Vec<_>>();
    validate_edges(&edges)?;
    Ok(edges)
}

fn logarithmic_edges(ordered: &[f64], bins: usize) -> Result<Vec<f64>, HistogramError> {
    if bins == 0 {
        return Err(HistogramError::InvalidBinCount { bins });
    }
    if let Some((index, value)) = ordered
        .iter()
        .copied()
        .enumerate()
        .find(|(_, value)| *value <= 0.0)
    {
        return Err(HistogramError::NonPositiveLogarithmicValue { index, value });
    }
    let minimum = ordered[0];
    let maximum = ordered[ordered.len() - 1];
    if minimum == maximum {
        return Ok(constant_edges(minimum, bins));
    }
    let log_minimum = minimum.ln();
    let step = (maximum.ln() - log_minimum) / bins as f64;
    let edges = (0..=bins)
        .map(|index| (log_minimum + step * index as f64).exp())
        .collect::<Vec<_>>();
    validate_edges(&edges)?;
    Ok(edges)
}

fn quantile_edges(ordered: &[f64], bins: usize) -> Result<Vec<f64>, HistogramError> {
    if bins == 0 {
        return Err(HistogramError::InvalidBinCount { bins });
    }
    if ordered[0] == ordered[ordered.len() - 1] {
        return Ok(constant_edges(ordered[0], bins));
    }
    let edges = (0..=bins)
        .map(|index| quantile(ordered, index as f64 / bins as f64))
        .collect::<Vec<_>>();
    if edges.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(HistogramError::RepeatedQuantileBoundary);
    }
    Ok(edges)
}

fn constant_edges(value: f64, bins: usize) -> Vec<f64> {
    let half_width = value.abs().max(1.0) * 0.5;
    let minimum = value - half_width;
    let width = (2.0 * half_width) / bins as f64;
    (0..=bins)
        .map(|index| minimum + width * index as f64)
        .collect()
}

fn validate_edges(edges: &[f64]) -> Result<(), HistogramError> {
    if edges.len() < 2
        || edges.iter().any(|edge| !edge.is_finite())
        || edges.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(HistogramError::InvalidEdges);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries_define_empty_constant_and_linear_quantiles() {
        assert_eq!(
            DistributionSummary::from_values(&[]).unwrap(),
            DistributionSummary::default()
        );
        let constant = DistributionSummary::from_values(&[2.0, 2.0, 2.0]).unwrap();
        assert_eq!(constant.standard_deviation, Some(0.0));
        assert_eq!(constant.quantiles.p50, Some(2.0));
        let values = DistributionSummary::from_values(&[0.0, 10.0]).unwrap();
        assert_eq!(values.quantiles.p50, Some(5.0));
    }

    #[test]
    fn histograms_report_explicit_range_and_validate_bins() {
        let histogram = Histogram::from_values(
            &[-1.0, 0.0, 1.0, 2.0, 3.0],
            HistogramBinning::Explicit(vec![0.0, 1.0, 2.0]),
        )
        .unwrap();
        assert_eq!(histogram.counts, vec![1, 2]);
        assert_eq!(histogram.underflow_count, 1);
        assert_eq!(histogram.overflow_count, 1);
        assert!(matches!(
            Histogram::from_values(&[1.0], HistogramBinning::Logarithmic { bins: 0 }),
            Err(HistogramError::InvalidBinCount { .. })
        ));
        assert!(matches!(
            Histogram::from_values(&[0.0, 1.0], HistogramBinning::Logarithmic { bins: 2 }),
            Err(HistogramError::NonPositiveLogarithmicValue { .. })
        ));
    }
}
