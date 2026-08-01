use ternary_contours::ContourPath;

pub const ANALYTIC_TOLERANCE: f64 = 2.0e-10;

/// Deterministic analytic fields used by validation tests.
#[derive(Clone, Copy, Debug)]
pub enum AnalyticField {
    Affine,
    PairwiseQuadratic,
    #[cfg(feature = "cubic-alpha")]
    Saddle,
}

impl AnalyticField {
    pub fn value(self, [a, b, c]: [f64; 3]) -> f64 {
        match self {
            Self::Affine => 2.25 * a - 3.5 * b + 0.75 * c + 1.125,
            Self::PairwiseQuadratic => {
                a * a + 0.75 * b * b + 1.25 * c * c + 0.4 * a * b - 0.3 * b * c + 0.2 * c * a
            }
            #[cfg(feature = "cubic-alpha")]
            Self::Saddle => (a - b) * (b - c) + 0.2 * a * c,
        }
    }

    /// Derivatives in independent semantic `(a,b)` coordinates, `c=1-a-b`.
    pub fn gradient_ab(self, [a, b, c]: [f64; 3]) -> [f64; 2] {
        match self {
            Self::Affine => [1.5, -4.25],
            Self::PairwiseQuadratic => [1.8 * a + 0.7 * b - 2.3 * c, 0.2 * a + 1.8 * b - 2.8 * c],
            #[cfg(feature = "cubic-alpha")]
            Self::Saddle => [1.6 * a + 0.8 * b - 1.0, 0.8 * a - 4.0 * b + 1.0],
        }
    }
}

/// Small deterministic generator: reproducibility matters more than a random distribution here.
#[derive(Clone, Copy, Debug)]
pub struct Lcg(u64);

impl Lcg {
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn unit(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (u64::MAX >> 11) as f64
    }

    pub fn simplex(&mut self) -> [f64; 3] {
        let a = self.unit();
        let b = (1.0 - a) * self.unit();
        [a, b, 1.0 - a - b]
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ErrorMetrics {
    count: usize,
    sum_squares: f64,
    max_abs: f64,
}

impl ErrorMetrics {
    pub fn record(&mut self, actual: f64, expected: f64) {
        let error = (actual - expected).abs();
        self.count += 1;
        self.sum_squares += error * error;
        self.max_abs = self.max_abs.max(error);
    }

    pub fn max_abs(self) -> f64 {
        self.max_abs
    }

    pub fn rms(self) -> f64 {
        (self.sum_squares / self.count.max(1) as f64).sqrt()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FieldMetrics {
    pub value: ErrorMetrics,
    pub gradient_a: ErrorMetrics,
    pub gradient_b: ErrorMetrics,
}

impl FieldMetrics {
    pub fn record(
        &mut self,
        value: f64,
        gradient_ab: [f64; 2],
        point: [f64; 3],
        field: AnalyticField,
    ) {
        self.value.record(value, field.value(point));
        let expected = field.gradient_ab(point);
        self.gradient_a.record(gradient_ab[0], expected[0]);
        self.gradient_b.record(gradient_ab[1], expected[1]);
    }
}

pub fn assert_within(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected:.16e}, got {actual:.16e}, tolerance {tolerance:.3e}"
    );
}

pub fn contour_residual(
    paths: &[ContourPath],
    level: f64,
    value: impl Fn([f64; 3]) -> f64,
) -> ErrorMetrics {
    let mut metrics = ErrorMetrics::default();
    for path in paths {
        for point in &path.points {
            metrics.record(value(point.as_array()), level);
        }
    }
    metrics
}

pub fn path_length(path: &ContourPath) -> f64 {
    if path.points.len() < 2 {
        return 0.0;
    }
    let edge_count = if path.closed {
        path.points.len()
    } else {
        path.points.len() - 1
    };
    (0..edge_count)
        .map(|index| {
            logical_distance(
                path.points[index].as_array(),
                path.points[(index + 1) % path.points.len()].as_array(),
            )
        })
        .sum()
}

pub fn spacing_cv(path: &ContourPath) -> Option<f64> {
    if path.points.len() < 3 {
        return None;
    }
    let edge_count = if path.closed {
        path.points.len()
    } else {
        path.points.len() - 1
    };
    let lengths = (0..edge_count)
        .map(|index| {
            logical_distance(
                path.points[index].as_array(),
                path.points[(index + 1) % path.points.len()].as_array(),
            )
        })
        .collect::<Vec<_>>();
    let mean = lengths.iter().sum::<f64>() / lengths.len() as f64;
    (mean > 0.0).then(|| {
        let variance = lengths
            .iter()
            .map(|length| (length - mean).powi(2))
            .sum::<f64>()
            / lengths.len() as f64;
        variance.sqrt() / mean
    })
}

pub fn approximate_symmetric_hausdorff(left: &[ContourPath], right: &[ContourPath]) -> f64 {
    let left = sampled_points(left);
    let right = sampled_points(right);
    if left.is_empty() && right.is_empty() {
        return 0.0;
    }
    if left.is_empty() || right.is_empty() {
        return f64::INFINITY;
    }
    directed_hausdorff(&left, &right).max(directed_hausdorff(&right, &left))
}

fn sampled_points(paths: &[ContourPath]) -> Vec<[f64; 3]> {
    let mut points = Vec::new();
    for path in paths {
        if path.points.len() < 2 {
            continue;
        }
        let edge_count = if path.closed {
            path.points.len()
        } else {
            path.points.len() - 1
        };
        for edge in 0..edge_count {
            let start = path.points[edge].as_array();
            let end = path.points[(edge + 1) % path.points.len()].as_array();
            for step in 0..=4 {
                let t = step as f64 / 4.0;
                points.push([
                    start[0] * (1.0 - t) + end[0] * t,
                    start[1] * (1.0 - t) + end[1] * t,
                    start[2] * (1.0 - t) + end[2] * t,
                ]);
            }
        }
    }
    points
}

fn directed_hausdorff(left: &[[f64; 3]], right: &[[f64; 3]]) -> f64 {
    left.iter()
        .map(|point| {
            right
                .iter()
                .map(|candidate| logical_distance(*point, *candidate))
                .fold(f64::INFINITY, f64::min)
        })
        .fold(0.0, f64::max)
}

fn logical_distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    let logical = |[_a, b, c]: [f64; 3]| [b + 0.5 * c, 0.866_025_403_784_438_6 * c];
    let left = logical(left);
    let right = logical(right);
    (left[0] - right[0]).hypot(left[1] - right[1])
}
