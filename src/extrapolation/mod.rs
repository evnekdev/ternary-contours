//! Provenance-friendly extrapolation for canonical regular ternary meshes.

mod regular;

pub use regular::{
    DirectionalEstimate, ExtrapolationDirection, ExtrapolationRejection,
    RegularMeshExtrapolatedValue, RegularMeshExtrapolationDiagnostics,
    RegularMeshExtrapolationError, RegularMeshExtrapolationOptions, RegularMeshExtrapolationResult,
    RegularMeshExtrapolationScope, RegularMeshExtrapolationTarget, RejectedExtrapolationVertex,
    TargetedExtrapolationResult, extrapolate_regular_mesh, extrapolate_regular_mesh_scoped,
    extrapolate_regular_mesh_with_trace,
};
