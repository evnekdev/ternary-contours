//! Shared local quadratic sampled-field estimates in logical ternary geometry.

use core::fmt;

use super::TernaryGradient;

/// Controls the deterministic local quadratic sampled-field estimator.
///
/// Backend adapters expand their graph or lattice neighbourhood from one ring
/// through `max_ring` until the six-parameter fit is supported.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalQuadraticOptions {
    /// Largest deterministic neighbourhood ring to consider.
    pub max_ring: usize,
    /// Largest accepted diagonal QR condition estimate.
    pub max_condition_estimate: f64,
}

impl Default for LocalQuadraticOptions {
    fn default() -> Self {
        Self {
            max_ring: 2,
            max_condition_estimate: 1.0e10,
        }
    }
}

/// A local quadratic estimate in canonical logical `(x, y)` coordinates.
///
/// The Hessian is the interpolation-independent fit to sampled vertex values,
/// not an analytic second derivative of a selected interpolant. Its gradient is
/// represented by the same [`TernaryGradient`] type used by both evaluator
/// families.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalQuadraticEstimate {
    /// Fitted first derivative at the centre, in shared ternary coordinates.
    pub gradient: TernaryGradient,
    /// Symmetric logical-plane Hessian `[[dxx, dxy], [dxy, dyy]]`.
    pub hessian_logical_xy: [[f64; 2]; 2],
    /// Frobenius norm of the logical Hessian.
    pub hessian_norm: f64,
    /// Logical-plane Laplacian `dxx + dyy`.
    pub laplacian: f64,
    /// Logical-plane Hessian determinant.
    pub determinant: f64,
    /// Ascending Hessian eigenvalues.
    pub eigenvalues: [f64; 2],
    /// Deterministically signed principal direction for the greater-magnitude eigenvalue.
    pub principal_direction: Option<[f64; 2]>,
    /// Ratio `max(abs(eigenvalue))/min(abs(eigenvalue))`, absent for zero curvature.
    pub anisotropy: Option<f64>,
    /// Root-mean-square residual in source-field units.
    pub residual_root_mean_square: f64,
    /// Diagonal QR condition estimate after local coordinate scaling.
    pub condition_estimate: f64,
    /// Number of sampled vertices included in the fit.
    pub sample_count: usize,
    /// Neighbourhood ring used by the backend adapter.
    pub ring: usize,
}

/// Typed failure for a local quadratic estimate.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum LocalQuadraticError {
    /// Options do not describe a stable finite fit request.
    InvalidOptions,
    /// Too few finite samples are available for the six fitted coefficients.
    InsufficientSamples { actual: usize, required: usize },
    /// The local observation coordinates do not span the quadratic basis.
    RankDeficient,
    /// The QR diagonal condition estimate exceeded the configured bound.
    IllConditioned { estimate: f64, maximum: f64 },
    /// Input samples or derived coefficients were non-finite.
    NonFinite,
}

impl fmt::Display for LocalQuadraticError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptions => formatter.write_str("invalid local quadratic fit options"),
            Self::InsufficientSamples { actual, required } => write!(
                formatter,
                "local quadratic fit needs at least {required} samples; received {actual}"
            ),
            Self::RankDeficient => formatter.write_str("local quadratic fit is rank deficient"),
            Self::IllConditioned { estimate, maximum } => write!(
                formatter,
                "local quadratic fit condition estimate {estimate:e} exceeds {maximum:e}"
            ),
            Self::NonFinite => formatter.write_str("local quadratic fit contains non-finite data"),
        }
    }
}
impl std::error::Error for LocalQuadraticError {}

/// Fit a locally centred quadratic using a scaled modified-Gram-Schmidt QR solve.
///
/// `observations` contain logical coordinates and values, including the centre.
/// This crate-private mathematical core is deliberately independent of either
/// mesh topology; regular and irregular adapters differ only in how they make
/// deterministic neighbourhoods.
pub(crate) fn fit_local_quadratic(
    centre: [f64; 2],
    observations: &[([f64; 2], f64)],
    options: LocalQuadraticOptions,
    ring: usize,
) -> Result<LocalQuadraticEstimate, LocalQuadraticError> {
    const PARAMETERS: usize = 6;
    if options.max_ring == 0
        || !options.max_condition_estimate.is_finite()
        || options.max_condition_estimate < 1.0
    {
        return Err(LocalQuadraticError::InvalidOptions);
    }
    if observations.len() < PARAMETERS {
        return Err(LocalQuadraticError::InsufficientSamples {
            actual: observations.len(),
            required: PARAMETERS,
        });
    }
    if !centre.into_iter().all(f64::is_finite)
        || observations
            .iter()
            .any(|(point, value)| !point.iter().copied().all(f64::is_finite) || !value.is_finite())
    {
        return Err(LocalQuadraticError::NonFinite);
    }

    let scale = observations
        .iter()
        .map(|(point, _)| (point[0] - centre[0]).hypot(point[1] - centre[1]))
        .fold(0.0_f64, f64::max);
    if !scale.is_finite() || scale == 0.0 {
        return Err(LocalQuadraticError::RankDeficient);
    }

    let mut columns = (0..PARAMETERS)
        .map(|_| Vec::with_capacity(observations.len()))
        .collect::<Vec<_>>();
    let values = observations
        .iter()
        .map(|(point, value)| {
            let x = (point[0] - centre[0]) / scale;
            let y = (point[1] - centre[1]) / scale;
            columns[0].push(1.0);
            columns[1].push(x);
            columns[2].push(y);
            columns[3].push(0.5 * x * x);
            columns[4].push(x * y);
            columns[5].push(0.5 * y * y);
            *value
        })
        .collect::<Vec<_>>();

    let mut q = (0..PARAMETERS)
        .map(|_| vec![0.0; observations.len()])
        .collect::<Vec<_>>();
    let mut r = [[0.0; PARAMETERS]; PARAMETERS];
    for column in 0..PARAMETERS {
        let mut vector = columns[column].clone();
        for prior in 0..column {
            let projection = dot(&q[prior], &vector);
            r[prior][column] = projection;
            for (value, basis) in vector.iter_mut().zip(&q[prior]) {
                *value -= projection * basis;
            }
        }
        let norm = dot(&vector, &vector).sqrt();
        if !norm.is_finite() || norm <= 1.0e-13 {
            return Err(LocalQuadraticError::RankDeficient);
        }
        r[column][column] = norm;
        for (basis, value) in q[column].iter_mut().zip(vector) {
            *basis = value / norm;
        }
    }
    let diagonal = (0..PARAMETERS)
        .map(|index| r[index][index])
        .collect::<Vec<_>>();
    let minimum = diagonal.iter().copied().fold(f64::INFINITY, f64::min);
    let maximum = diagonal.iter().copied().fold(0.0_f64, f64::max);
    let condition_estimate = maximum / minimum;
    if !condition_estimate.is_finite() {
        return Err(LocalQuadraticError::NonFinite);
    }
    if condition_estimate > options.max_condition_estimate {
        return Err(LocalQuadraticError::IllConditioned {
            estimate: condition_estimate,
            maximum: options.max_condition_estimate,
        });
    }

    let mut coefficients = [0.0; PARAMETERS];
    for row in (0..PARAMETERS).rev() {
        let rhs = dot(&q[row], &values)
            - ((row + 1)..PARAMETERS)
                .map(|column| r[row][column] * coefficients[column])
                .sum::<f64>();
        coefficients[row] = rhs / r[row][row];
    }
    if !coefficients.into_iter().all(f64::is_finite) {
        return Err(LocalQuadraticError::NonFinite);
    }
    let residual_sum = observations
        .iter()
        .zip(&values)
        .map(|((point, _), value)| {
            let x = (point[0] - centre[0]) / scale;
            let y = (point[1] - centre[1]) / scale;
            let fitted = coefficients[0]
                + coefficients[1] * x
                + coefficients[2] * y
                + 0.5 * coefficients[3] * x * x
                + coefficients[4] * x * y
                + 0.5 * coefficients[5] * y * y;
            (fitted - value).powi(2)
        })
        .sum::<f64>();
    let hessian = [
        [
            coefficients[3] / (scale * scale),
            coefficients[4] / (scale * scale),
        ],
        [
            coefficients[4] / (scale * scale),
            coefficients[5] / (scale * scale),
        ],
    ];
    let gradient =
        TernaryGradient::from_logical_xy([coefficients[1] / scale, coefficients[2] / scale]);
    let hessian_norm =
        (hessian[0][0].powi(2) + 2.0 * hessian[0][1].powi(2) + hessian[1][1].powi(2)).sqrt();
    let laplacian = hessian[0][0] + hessian[1][1];
    let determinant = hessian[0][0].mul_add(hessian[1][1], -hessian[0][1].powi(2));
    let half_difference = 0.5 * (hessian[0][0] - hessian[1][1]);
    let radius = half_difference.hypot(hessian[0][1]);
    let midpoint = 0.5 * laplacian;
    let eigenvalues = [midpoint - radius, midpoint + radius];
    let selected = if eigenvalues[0].abs() > eigenvalues[1].abs() {
        eigenvalues[0]
    } else {
        eigenvalues[1]
    };
    let mut direction = [hessian[0][1], selected - hessian[0][0]];
    if direction[0].hypot(direction[1]) <= 1.0e-14 {
        direction = [selected - hessian[1][1], hessian[0][1]];
    }
    let direction_norm = direction[0].hypot(direction[1]);
    let principal_direction = (direction_norm > 1.0e-14).then(|| {
        let mut unit = [direction[0] / direction_norm, direction[1] / direction_norm];
        if unit[0] < 0.0 || (unit[0] == 0.0 && unit[1] < 0.0) {
            unit = [-unit[0], -unit[1]];
        }
        unit
    });
    let absolute = eigenvalues.map(f64::abs);
    let least = absolute[0].min(absolute[1]);
    let anisotropy = (least > 1.0e-14).then_some(absolute[0].max(absolute[1]) / least);
    let estimate = LocalQuadraticEstimate {
        gradient,
        hessian_logical_xy: hessian,
        hessian_norm,
        laplacian,
        determinant,
        eigenvalues,
        principal_direction,
        anisotropy,
        residual_root_mean_square: (residual_sum / observations.len() as f64).sqrt(),
        condition_estimate,
        sample_count: observations.len(),
        ring,
    };
    if !estimate.gradient.is_finite()
        || !estimate
            .hessian_logical_xy
            .into_iter()
            .flatten()
            .all(f64::is_finite)
        || !estimate.hessian_norm.is_finite()
        || !estimate.laplacian.is_finite()
        || !estimate.determinant.is_finite()
        || !estimate.eigenvalues.into_iter().all(f64::is_finite)
        || !estimate.residual_root_mean_square.is_finite()
    {
        return Err(LocalQuadraticError::NonFinite);
    }
    Ok(estimate)
}

fn dot(left: &[f64], right: &[f64]) -> f64 {
    left.iter().zip(right).map(|(a, b)| a * b).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_fit_recovers_a_logical_quadratic() {
        let centre: [f64; 2] = [0.5, 0.3];
        let observations = [-1.0, 0.0, 1.0]
            .into_iter()
            .flat_map(|x| [-1.0, 0.0, 1.0].into_iter().map(move |y| (x, y)))
            .map(|(x, y)| {
                let point = [centre[0] + x * 0.1, centre[1] + y * 0.1];
                let value = 1.2 + 2.0 * point[0] - 3.0 * point[1]
                    + 0.5 * 4.0 * point[0].powi(2)
                    + 1.5 * point[0] * point[1]
                    + 0.5 * -2.0 * point[1].powi(2);
                (point, value)
            })
            .collect::<Vec<_>>();
        let estimate =
            fit_local_quadratic(centre, &observations, LocalQuadraticOptions::default(), 1)
                .unwrap();
        assert!((estimate.hessian_logical_xy[0][0] - 4.0).abs() < 1.0e-11);
        assert!((estimate.hessian_logical_xy[0][1] - 1.5).abs() < 1.0e-11);
        assert!((estimate.hessian_logical_xy[1][1] + 2.0).abs() < 1.0e-11);
        assert!(estimate.residual_root_mean_square < 1.0e-12);
    }
}
