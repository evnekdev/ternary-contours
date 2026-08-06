//! Provenance-friendly extrapolation for canonical regular ternary meshes.

mod regular;

pub use regular::{
    DirectionalEstimate, ExtrapolationDirection, ExtrapolationRejection,
    RegularMeshExtrapolatedValue, RegularMeshExtrapolationDiagnostics,
    RegularMeshExtrapolationError, RegularMeshExtrapolationOptions, RegularMeshExtrapolationResult,
    RejectedExtrapolationVertex, extrapolate_regular_mesh, extrapolate_regular_mesh_with_trace,
};
