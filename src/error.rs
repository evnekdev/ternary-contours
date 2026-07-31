use crate::interpolation::InterpolationError;
use core::fmt;

/// Error returned while validating a regular ternary scalar field or constructing
/// its cubic-alpha edge model.
#[derive(Clone, Debug, PartialEq)]
pub enum FieldError {
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
    InsufficientStencil {
        samples: usize,
    },
    CubicFeatureUnavailable,
    Interpolation(InterpolationError),
}
impl fmt::Display for FieldError {
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
            Self::InsufficientStencil { samples } => write!(
                f,
                "cubic-alpha line requires at least three samples; received {samples}"
            ),
            Self::CubicFeatureUnavailable => write!(
                f,
                "cubic-alpha construction requires the `cubic-alpha` feature"
            ),
            Self::Interpolation(error) => write!(f, "cubic interpolation error: {error}"),
        }
    }
}
impl std::error::Error for FieldError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Interpolation(error) => Some(error),
            _ => None,
        }
    }
}
impl From<InterpolationError> for FieldError {
    fn from(value: InterpolationError) -> Self {
        Self::Interpolation(value)
    }
}
