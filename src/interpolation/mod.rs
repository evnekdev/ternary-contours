//! Backend-independent interpolation primitives for regular ternary grids.

mod alpha;
mod cubic;
mod options;

pub use alpha::AlphaInterval;
pub use cubic::{
    BinaryExtrapolation, CubicAlphaTriangle, DirectedAlphaInterval, InterpolationError,
    PairEvaluation, evaluate_pair,
};
pub use options::{CubicAlphaBuildOptions, CubicAlphaMethod, CubicBoundaryPolicy};
