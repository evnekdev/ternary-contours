/// One source-to-umbrella verification pass.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StableVerificationPassDiagnostics {
    /// Umbrella subdivisions used in this pass.
    pub subdivisions: usize,
    /// Number of direct source verification points.
    pub verification_points: usize,
    /// Largest absolute height residual.
    pub maximum_height_approximation_error: f64,
    /// Root-mean-square height residual across phases and points.
    pub rms_height_approximation_error: f64,
    /// Largest absolute secondary residual, or zero in height mode.
    pub maximum_secondary_approximation_error: f64,
    /// Points whose direct and umbrella stable phase sets differ.
    pub ownership_mismatches: usize,
    /// Directly stable phases absent from the umbrella-predicted stable set.
    pub hidden_candidate_discoveries: usize,
    /// Umbrella triangles failing at least one configured check.
    pub unresolved_umbrella_triangles: usize,
    /// Triangle with the largest configured residual, when unresolved.
    pub worst_unresolved_triangle: Option<usize>,
}

/// Preparation, partition, verification, and contour extraction counters.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StableContourDiagnostics {
    // Preparation.
    pub phase_count: usize,
    pub source_scalar_layer_count: usize,
    pub geometry_group_count: usize,
    pub regular_geometry_group_count: usize,
    pub irregular_geometry_group_count: usize,
    pub umbrella_subdivisions: usize,
    pub umbrella_vertex_count: usize,
    pub umbrella_triangle_count: usize,
    pub source_point_location_count: usize,
    pub reused_source_locations: usize,
    pub source_scalar_evaluation_count: usize,
    pub refinement_passes: usize,
    pub final_subdivisions: usize,
    pub sampled_scalar_values: usize,

    // Verification.
    pub verification_point_count: usize,
    pub maximum_height_approximation_error: f64,
    pub rms_height_approximation_error: f64,
    pub maximum_secondary_approximation_error: f64,
    pub ownership_mismatches: usize,
    pub hidden_candidate_discoveries: usize,
    pub unresolved_umbrella_triangles: usize,
    pub worst_unresolved_triangle: Option<usize>,
    pub verification_passes: Vec<StableVerificationPassDiagnostics>,

    // Stable upper-envelope partition.
    pub total_phase_triangle_candidates: usize,
    pub phases_removed_by_envelope_floor: usize,
    pub pair_comparisons_skipped_by_range: usize,
    pub polygon_clipping_operations: usize,
    pub empty_stable_polygons: usize,
    pub nonempty_stable_polygons: usize,
    pub interior_stable_polygons_without_vertex_winner: usize,
    pub univariant_edges: usize,
    pub invariant_vertices: usize,
    pub co_stable_regions: usize,
    pub coincident_tie_segments: usize,

    // Level extraction and path assembly.
    pub requested_levels: usize,
    pub local_stable_segments: usize,
    pub phase_labelled_paths: usize,
    pub closed_paths: usize,
    pub open_paths: usize,
    pub stable_boundary_contacts: usize,
    pub univariant_junctions: usize,
    pub invariant_junctions: usize,
    pub isolated_target_points: usize,
    pub path_assembly_ambiguities: usize,
}
