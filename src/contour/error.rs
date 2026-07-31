use std::fmt;

use crate::{FieldError, interpolation::InterpolationError};

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ContourError {
    ZeroSubdivisions,
    AllocationOverflow,
    IncorrectValueCount {
        expected: usize,
        actual: usize,
    },
    NonFiniteValue {
        index: usize,
        value: f64,
    },
    InvalidLatticeCoordinate {
        i: usize,
        j: usize,
        k: usize,
        subdivisions: usize,
    },
    InvalidVertexIndex {
        index: usize,
        vertex_count: usize,
    },
    NonFiniteLevel {
        index: usize,
        value: f64,
    },
    DuplicateLevel {
        first: usize,
        second: usize,
        value: f64,
    },
    NonFiniteBandBreak {
        index: usize,
        value: f64,
    },
    DuplicateBandBreak {
        first: usize,
        second: usize,
        value: f64,
    },
    UnorderedBandBreak {
        previous_index: usize,
        index: usize,
        previous: f64,
        value: f64,
    },
    UnsupportedFilledInterpolation,
    UnclosedBandBoundary,
    InvalidTolerance {
        value_tolerance: f64,
        geometry_tolerance: f64,
    },
    InvalidAdaptiveOptions {
        max_depth: u8,
        flatness_tolerance: f64,
    },
    InvalidRegularizationSpacing {
        spacing: f64,
    },
    InvalidProjectionOptions {
        tolerance: f64,
        iterations: usize,
        max_step: f64,
    },
    CubicFeatureUnavailable,
    InsufficientStencil {
        samples: usize,
    },
    FlatTriangle {
        triangle: usize,
        level: f64,
    },
    BranchingTopology {
        degree: usize,
    },
    ZeroLengthPath,
    InvalidClosedLoop,
    ProjectionZeroGradient {
        residual: f64,
    },
    ProjectionNonConvergence {
        residual: f64,
        iterations: usize,
    },
    PointOutsideGrid {
        a: f64,
        b: f64,
        c: f64,
    },
    Interpolation(InterpolationError),
}

impl fmt::Display for ContourError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroSubdivisions => {
                write!(f, "regular ternary field subdivisions must be positive")
            }
            Self::AllocationOverflow => {
                write!(f, "regular-grid allocation or index arithmetic overflowed")
            }
            Self::IncorrectValueCount { expected, actual } => write!(
                f,
                "regular ternary field requires {expected} values; received {actual}"
            ),
            Self::NonFiniteValue { index, value } => {
                write!(f, "field value {index} is not finite: {value:?}")
            }
            Self::InvalidLatticeCoordinate {
                i,
                j,
                k,
                subdivisions,
            } => write!(
                f,
                "lattice coordinate ({i},{j},{k}) does not sum to subdivision count {subdivisions}"
            ),
            Self::InvalidVertexIndex {
                index,
                vertex_count,
            } => write!(f, "grid vertex index {index} is outside 0..{vertex_count}"),
            Self::NonFiniteLevel { index, value } => {
                write!(f, "contour level {index} is not finite: {value:?}")
            }
            Self::DuplicateLevel {
                first,
                second,
                value,
            } => write!(f, "contour levels {first} and {second} duplicate {value:?}"),
            Self::NonFiniteBandBreak { index, value } => {
                write!(f, "contour band break {index} is not finite: {value:?}")
            }
            Self::DuplicateBandBreak {
                first,
                second,
                value,
            } => write!(
                f,
                "contour band breaks {first} and {second} duplicate {value:?}"
            ),
            Self::UnorderedBandBreak {
                previous_index,
                index,
                previous,
                value,
            } => write!(
                f,
                "band break {index} ({value}) must be greater than break {previous_index} ({previous})"
            ),
            Self::UnsupportedFilledInterpolation => write!(
                f,
                "filled contour bands currently support only piecewise-linear interpolation"
            ),
            Self::UnclosedBandBoundary => write!(
                f,
                "filled-band fragments did not assemble into a closed boundary"
            ),
            Self::InvalidTolerance {
                value_tolerance,
                geometry_tolerance,
            } => write!(
                f,
                "contour tolerances must be finite and positive: value={value_tolerance:?}, geometry={geometry_tolerance:?}"
            ),
            Self::InvalidAdaptiveOptions {
                max_depth,
                flatness_tolerance,
            } => write!(
                f,
                "invalid adaptive options: max_depth={max_depth}, flatness={flatness_tolerance:?}"
            ),
            Self::InvalidRegularizationSpacing { spacing } => write!(
                f,
                "contour regularization spacing must be finite and positive: {spacing:?}"
            ),
            Self::InvalidProjectionOptions {
                tolerance,
                iterations,
                max_step,
            } => write!(
                f,
                "invalid projection options: tolerance={tolerance:?}, iterations={iterations}, max_step={max_step:?}"
            ),
            Self::CubicFeatureUnavailable => write!(
                f,
                "cubic-alpha contour interpolation requires the `cubic-alpha` feature"
            ),
            Self::InsufficientStencil { samples } => write!(
                f,
                "cubic-alpha line requires at least three samples; received {samples}"
            ),
            Self::FlatTriangle { triangle, level } => write!(
                f,
                "triangle {triangle} is entirely coincident with contour level {level:?}"
            ),
            Self::BranchingTopology { degree } => {
                write!(f, "contour endpoint graph has non-manifold degree {degree}")
            }
            Self::ZeroLengthPath => write!(f, "contour extraction produced a zero-length path"),
            Self::InvalidClosedLoop => write!(
                f,
                "closed contour does not contain at least three distinct points"
            ),
            Self::ProjectionZeroGradient { residual } => write!(
                f,
                "implicit contour projection encountered a zero gradient at residual {residual:?}"
            ),
            Self::ProjectionNonConvergence {
                residual,
                iterations,
            } => write!(
                f,
                "implicit contour projection did not converge after {iterations} iterations; residual={residual:?}"
            ),
            Self::PointOutsideGrid { a, b, c } => write!(
                f,
                "composition ({a:?},{b:?},{c:?}) lies outside the regular ternary grid"
            ),
            Self::Interpolation(error) => write!(f, "cubic interpolation error: {error}"),
        }
    }
}
impl std::error::Error for ContourError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Interpolation(error) => Some(error),
            _ => None,
        }
    }
}
impl From<InterpolationError> for ContourError {
    fn from(value: InterpolationError) -> Self {
        Self::Interpolation(value)
    }
}
impl From<FieldError> for ContourError {
    fn from(value: FieldError) -> Self {
        match value {
            FieldError::ZeroSubdivisions => Self::ZeroSubdivisions,
            FieldError::AllocationOverflow => Self::AllocationOverflow,
            FieldError::IncorrectValueCount { expected, actual } => {
                Self::IncorrectValueCount { expected, actual }
            }
            FieldError::NonFiniteValue { index, value } => Self::NonFiniteValue { index, value },
            FieldError::InvalidLatticeCoordinate {
                i,
                j,
                k,
                subdivisions,
            } => Self::InvalidLatticeCoordinate {
                i,
                j,
                k,
                subdivisions,
            },
            FieldError::InvalidVertexIndex {
                index,
                vertex_count,
            } => Self::InvalidVertexIndex {
                index,
                vertex_count,
            },
            FieldError::InsufficientStencil { samples } => Self::InsufficientStencil { samples },
            FieldError::CubicFeatureUnavailable => Self::CubicFeatureUnavailable,
            FieldError::Interpolation(error) => Self::Interpolation(error),
        }
    }
}
