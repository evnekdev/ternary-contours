//! Backend-independent interpolation primitives for regular ternary grids.

mod alpha;
mod build;
mod cubic;
mod options;

pub use alpha::AlphaInterval;
#[cfg(feature = "irregular-cubic-alpha")]
pub(crate) use build::alpha_from_uniform_four_values;
#[cfg(feature = "cubic-alpha")]
pub(crate) use build::cubic_method_kind;
pub use cubic::{
    BinaryExtrapolation, CubicAlphaTriangle, DirectedAlphaInterval, InterpolationError,
    PairEvaluation, evaluate_pair,
};
pub use options::{
    CubicAlphaBuildOptions, CubicAlphaMethod, CubicBoundaryPolicy, CubicPartialDomainPolicy,
};

