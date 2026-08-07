//! Rust-owned document state for the Qt Widgets application.
//!
//! Qt receives copies of typed snapshots and submits small mutations. TCT
//! parsing, serialization, validation, classified values, and calculations stay
//! in Rust; no second document parser or NaN-based missing representation exists
//! in the C++ layer.

#![allow(clippy::missing_safety_doc)]

use std::{
    ffi::CStr,
    fs,
    io::Write,
    os::raw::c_char,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use ternary_contours::{
    BinaryBoundary, BinaryExtrapolation, CubicAlphaMethod, CubicPartialDomainPolicy,
    IrregularTernaryMesh, IrregularTriangleId, NumericalTraceConfig, NumericalTraceLevel,
    RegularMeshExtrapolationOptions, RegularTernaryGrid, StableBoundaryNetwork,
    StableInvariantNode, StablePhaseId, composition_from_local_barycentric,
    normalize_ternary_triplet,
};
use ternary_contours_cli::{
    CompositionColumns, GridType, HeaderMode, InterpolationOptions, IrregularTabulatedGrid,
    JsonLinesTraceSink, LiquidusProjection, MeshExtrapolationField, MeshExtrapolationPreview,
    MeshExtrapolationRequest, NumericalTraceRunContext, OutputFormat, ParsedTable, PhaseDefinition,
    ProjectionCsvLayerFilter, ProjectionCsvOptions, ProjectionCsvRecord,
    ProjectionGeometryCsvSelection, ProjectionOptions, ProjectionPathSource, PropertyDefinition,
    RegularTabulatedGrid, RenderOptions, RenderPathMode, RowOrder, SourceRange, TabulatedField,
    TabulatedGrid, TabulatedTernaryDataset, TabulatedValue, TabulatedValueState,
    TctSerializeOptions, apply_mesh_extrapolation, automatic_iso_levels, automatic_iso_range,
    calculate_projection, calculate_projection_reusing_stable_topology,
    calculate_projection_with_automatic_bootstrap_with_trace_context,
    calculate_projection_with_trace_context_reusing_stable_topology, calculate_stable_topology,
    calculate_stable_topology_projection, empty_project_dataset, extrapolate_regular_grid_fields,
    interpolation_inspection::{
        FieldInspectionCache, InspectionFieldIdentity, InterpolatedResultState,
    },
    parse_path, parse_tabulated_value_token, projection_csv_records, render_to_path,
    save_tct_atomic, serialize_projection_geometry_csv, serialize_tct,
    validate_new_regular_grid_subdivisions,
};
use ternary_contours_gui_core::{GuiContractState, Revision, UiAction, UiEffect, update};

const NAME: usize = 128;
const PATH: usize = 512;
const MESSAGE: usize = 512;
// C ABI enum values.  Keep these named instead of coupling Rust semantics to
// Designer combo-box ordering.
const ABI_SOURCE_LINEAR: u32 = 0;
const ABI_SOURCE_CUBIC_ALPHA: u32 = 1;
const ABI_CUBIC_AKIMA: u32 = 0;
const ABI_CUBIC_MAKIMA: u32 = 1;
const ABI_CUBIC_PCHIP: u32 = 2;
const ABI_CUBIC_STEFFEN: u32 = 3;
const ABI_MESH_SCOPE_FIELD: u32 = 0;
const ABI_MESH_SCOPE_PHASE: u32 = 1;
const ABI_MESH_SCOPE_TARGETS: u32 = 2;
const ABI_PARTIAL_STRICT: u32 = 0;
const ABI_PARTIAL_ONE_SIDED: u32 = 1;
const ABI_PARTIAL_ONE_SIDED_THEN_LINEAR: u32 = 2;
const ABI_PARTIAL_LINEAR_NEAR_BOUNDARIES: u32 = 3;
const ABI_CONTINUATION_RAW_BARYCENTRIC: u32 = 0;
const ABI_CONTINUATION_MUGGIANU: u32 = 1;
const ABI_CONTINUATION_KOHLER: u32 = 2;
#[repr(C)]
pub struct TcqtStatus {
    pub success: bool,
    pub message: [u8; MESSAGE],
}
#[repr(C)]
pub struct TcqtCalculationResult {
    pub success: bool,
    pub request_id: u64,
    pub dataset_revision: u64,
    pub options_revision: u64,
    pub vertex_count: u32,
    pub message: [u8; 128],
}
/// Numerical configuration shared by the Qt Viewer and the Rust projection.
/// Enum values are deliberately explicit across the C ABI; invalid values are
/// rejected by the bridge instead of falling back silently.
pub const TCQT_MAX_EXPLICIT_LEVELS: usize = 256;

#[repr(C)]
#[derive(Clone, Copy, PartialEq)]
pub struct TcqtViewerCalculationOptions {
    pub automatic_range: bool,
    pub minimum: f64,
    pub maximum: f64,
    pub level_step: f64,
    pub sampling_subdivisions: u32,
    pub regularize: bool,
    pub regularization_spacing: f64,
    /// 0 = linear, 1 = cubic alpha.
    pub source_interpolation: u32,
    /// 0 = Akima, 1 = Makima, 2 = PCHIP, 3 = Steffen.
    pub cubic_method: u32,
    /// 0 = strict, 1 = one-sided, 2 = one-sided then linear,
    /// 3 = linear near boundaries.
    pub partial_domain_policy: u32,
    /// 0 = raw barycentric, 1 = Muggianu, 2 = Kohler.
    pub continuation: u32,
    /// Explicit level list supplied by the Qt presentation parser.
    pub explicit_level_count: u32,
    pub explicit_levels: [f64; TCQT_MAX_EXPLICIT_LEVELS],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ViewerTopologyKey {
    sampling_subdivisions: u32,
    regularize: bool,
    regularization_spacing_bits: u64,
    interpolation: InterpolationOptions,
}

#[derive(Clone, Debug, PartialEq)]
struct ViewerCalculationOptions {
    automatic_range: bool,
    minimum: f64,
    maximum: f64,
    level_step: f64,
    explicit_levels: Vec<f64>,
    sampling_subdivisions: u32,
    regularize: bool,
    regularization_spacing: f64,
    interpolation: InterpolationOptions,
}

impl Default for ViewerCalculationOptions {
    fn default() -> Self {
        Self {
            automatic_range: true,
            minimum: 0.0,
            maximum: 0.0,
            level_step: 100.0,
            explicit_levels: Vec::new(),
            sampling_subdivisions: 20,
            regularize: true,
            regularization_spacing: 0.0,
            interpolation: InterpolationOptions::default(),
        }
    }
}

impl ViewerCalculationOptions {
    fn topology_key(&self) -> ViewerTopologyKey {
        ViewerTopologyKey {
            sampling_subdivisions: self.sampling_subdivisions,
            regularize: self.regularize,
            regularization_spacing_bits: self.regularization_spacing.to_bits(),
            interpolation: self.interpolation,
        }
    }

    fn from_abi(raw: &TcqtViewerCalculationOptions) -> Result<Self, String> {
        let source = match raw.source_interpolation {
            ABI_SOURCE_LINEAR => ternary_contours_cli::SourceInterpolation::Linear,
            ABI_SOURCE_CUBIC_ALPHA => ternary_contours_cli::SourceInterpolation::CubicAlpha {
                method: match raw.cubic_method {
                    ABI_CUBIC_AKIMA => CubicAlphaMethod::Akima,
                    ABI_CUBIC_MAKIMA => CubicAlphaMethod::Makima,
                    ABI_CUBIC_PCHIP => CubicAlphaMethod::Pchip,
                    ABI_CUBIC_STEFFEN => CubicAlphaMethod::Steffen,
                    _ => return Err("unsupported cubic slope method".into()),
                },
                continuation: match raw.continuation {
                    ABI_CONTINUATION_RAW_BARYCENTRIC => BinaryExtrapolation::RawBarycentric,
                    ABI_CONTINUATION_MUGGIANU => BinaryExtrapolation::Muggianu,
                    ABI_CONTINUATION_KOHLER => BinaryExtrapolation::Kohler,
                    _ => return Err("unsupported ternary continuation".into()),
                },
            },
            _ => return Err("unsupported source interpolation".into()),
        };
        let partial_domain_policy = match raw.partial_domain_policy {
            ABI_PARTIAL_STRICT => CubicPartialDomainPolicy::Strict,
            ABI_PARTIAL_ONE_SIDED => CubicPartialDomainPolicy::OneSided,
            ABI_PARTIAL_ONE_SIDED_THEN_LINEAR => CubicPartialDomainPolicy::OneSidedThenLinear,
            ABI_PARTIAL_LINEAR_NEAR_BOUNDARIES => CubicPartialDomainPolicy::LinearNearDomain,
            _ => return Err("unsupported partial-domain policy".into()),
        };
        if !raw.level_step.is_finite() || raw.level_step <= 0.0 {
            return Err("isotherm step must be finite and positive".into());
        }
        if raw.regularization_spacing.is_finite() && raw.regularization_spacing < 0.0 {
            return Err("regularization spacing must be positive when supplied".into());
        }
        if raw.explicit_level_count as usize > TCQT_MAX_EXPLICIT_LEVELS {
            return Err(format!(
                "too many explicit isotherm levels (maximum {})",
                TCQT_MAX_EXPLICIT_LEVELS
            ));
        }
        let mut explicit_levels = raw.explicit_levels[..raw.explicit_level_count as usize].to_vec();
        if explicit_levels.iter().any(|level| !level.is_finite()) {
            return Err("explicit isotherm levels must be finite".into());
        }
        explicit_levels.sort_by(|a, b| a.total_cmp(b));
        explicit_levels.dedup_by(|left, right| {
            (*left - *right).abs() <= 1.0e-12 * left.abs().max(right.abs()).max(1.0)
        });
        if raw.automatic_range && !explicit_levels.is_empty() {
            return Err("automatic range cannot be combined with explicit levels".into());
        }
        if !raw.automatic_range && explicit_levels.is_empty() {
            automatic_iso_levels(raw.minimum, raw.maximum, raw.level_step)
                .map_err(|error| format!("invalid manual isotherm range: {error}"))?;
        } else if !explicit_levels.is_empty() {
            validate_explicit_levels(&explicit_levels)?;
        }
        Ok(Self {
            automatic_range: raw.automatic_range,
            minimum: raw.minimum,
            maximum: raw.maximum,
            level_step: raw.level_step,
            explicit_levels,
            sampling_subdivisions: raw.sampling_subdivisions,
            regularize: raw.regularize,
            regularization_spacing: raw.regularization_spacing,
            interpolation: InterpolationOptions {
                source,
                partial_domain_policy,
            },
        })
    }

    fn to_abi(&self) -> TcqtViewerCalculationOptions {
        let (source_interpolation, cubic_method, continuation) = match self.interpolation.source {
            ternary_contours_cli::SourceInterpolation::Linear => (
                ABI_SOURCE_LINEAR,
                ABI_CUBIC_AKIMA,
                ABI_CONTINUATION_MUGGIANU,
            ),
            ternary_contours_cli::SourceInterpolation::CubicAlpha {
                method,
                continuation,
            } => {
                let cubic_method = match method {
                    CubicAlphaMethod::Akima => ABI_CUBIC_AKIMA,
                    CubicAlphaMethod::Makima => ABI_CUBIC_MAKIMA,
                    CubicAlphaMethod::Pchip => ABI_CUBIC_PCHIP,
                    CubicAlphaMethod::Steffen => ABI_CUBIC_STEFFEN,
                    _ => unreachable!("unsupported cubic method cannot cross the Qt ABI"),
                };
                let continuation = match continuation {
                    BinaryExtrapolation::RawBarycentric => ABI_CONTINUATION_RAW_BARYCENTRIC,
                    BinaryExtrapolation::Muggianu => ABI_CONTINUATION_MUGGIANU,
                    BinaryExtrapolation::Kohler => ABI_CONTINUATION_KOHLER,
                    _ => unreachable!("unsupported continuation cannot cross the Qt ABI"),
                };
                (ABI_SOURCE_CUBIC_ALPHA, cubic_method, continuation)
            }
        };
        let partial_domain_policy = match self.interpolation.partial_domain_policy {
            CubicPartialDomainPolicy::Strict => ABI_PARTIAL_STRICT,
            CubicPartialDomainPolicy::OneSided => ABI_PARTIAL_ONE_SIDED,
            CubicPartialDomainPolicy::OneSidedThenLinear => ABI_PARTIAL_ONE_SIDED_THEN_LINEAR,
            CubicPartialDomainPolicy::LinearNearDomain => ABI_PARTIAL_LINEAR_NEAR_BOUNDARIES,
            _ => unreachable!("unsupported partial-domain policy cannot cross the Qt ABI"),
        };
        TcqtViewerCalculationOptions {
            automatic_range: self.automatic_range,
            minimum: self.minimum,
            maximum: self.maximum,
            level_step: self.level_step,
            sampling_subdivisions: self.sampling_subdivisions,
            regularize: self.regularize,
            regularization_spacing: self.regularization_spacing,
            source_interpolation,
            cubic_method,
            partial_domain_policy,
            continuation,
            explicit_level_count: self.explicit_levels.len() as u32,
            explicit_levels: {
                let mut levels = [0.0; TCQT_MAX_EXPLICIT_LEVELS];
                for (index, level) in self.explicit_levels.iter().enumerate() {
                    levels[index] = *level;
                }
                levels
            },
        }
    }

    fn projection_options(&self) -> ProjectionOptions {
        ProjectionOptions {
            levels: if !self.explicit_levels.is_empty() {
                self.explicit_levels.clone()
            } else if self.automatic_range {
                Vec::new()
            } else {
                automatic_iso_levels(self.minimum, self.maximum, self.level_step)
                    .expect("validated Viewer range")
            },
            automatic_level_step: self.automatic_range.then_some(self.level_step),
            sampling_subdivisions: (self.sampling_subdivisions != 0)
                .then_some(self.sampling_subdivisions as usize),
            regularize: self.regularize,
            regularization_spacing: (self.regularization_spacing > 0.0)
                .then_some(self.regularization_spacing),
            interpolation: self.interpolation,
        }
    }
}

fn validate_explicit_levels(levels: &[f64]) -> Result<(), String> {
    if levels.is_empty() || levels.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(
            "explicit isotherm levels must be strictly ascending without duplicates".into(),
        );
    }
    Ok(())
}

/// Rust-authoritative Viewer numerical configuration and its monotonic revision.
#[repr(C)]
pub struct TcqtViewerCalculationState {
    pub options: TcqtViewerCalculationOptions,
    pub options_revision: u64,
}

#[repr(C)]
pub struct TcqtProjectionCsvExportOptions {
    pub invariants: bool,
    pub univariants: bool,
    pub isotherms: bool,
    /// 0 = raw, 1 = regularized, 2 = overlay (regularized primary).
    pub path_display_mode: u32,
    pub expected_dataset_revision: u64,
    pub expected_options_revision: u64,
    pub expected_request_id: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TcqtProjectionSummary {
    pub available: bool,
    pub source_minimum: f64,
    pub source_maximum: f64,
    pub automatic_minimum: f64,
    pub automatic_used_invariant: bool,
    pub level_count: u32,
    pub invariant_count: u32,
    pub binary_invariant_count: u32,
    pub interior_invariant_count: u32,
    pub univariant_count: u32,
    pub contour_path_count: u32,
    pub contour_transfer_junction_count: u32,
    pub contour_one_sided_contact_count: u32,
    pub contour_invariant_level_coincidence_count: u32,
    pub contour_degenerate_event_count: u32,
    pub contour_levels_attempted: u32,
    pub contour_levels_completed: u32,
    pub contour_levels_failed: u32,
    pub maximum_contour_level_residual: f64,
    pub effective_automatic_range: bool,
    pub effective_minimum: f64,
    pub effective_maximum: f64,
    pub effective_level_step: f64,
    pub effective_sampling_subdivisions: u32,
    pub effective_regularize: bool,
    pub effective_regularization_spacing: f64,
    pub effective_source_interpolation: u32,
    pub effective_cubic_method: u32,
    pub effective_partial_domain_policy: u32,
    pub effective_continuation: u32,
    pub dataset_revision: u64,
    pub options_revision: u64,
    pub request_id: u64,
    pub raw_projection_available: bool,
    pub regularized_projection_available: bool,
    pub selected_projection_regularized: bool,
    pub domain_truncated_univariant_count: u32,
    pub regularization_failure_count: u32,
    /// Session-local projection lifecycle diagnostics.
    pub stable_topology_build_count: u64,
    pub stable_topology_reuse_count: u64,
    pub isotherm_rebuild_count: u64,
    pub stable_topology_reused: bool,
    pub message: [u8; MESSAGE],
}

/// One invariant node from the currently accepted projection graph.
#[repr(C)]
pub struct TcqtInvariantPoint {
    pub id: u32,
    /// 0 = binary, 1 = interior.
    pub kind: u32,
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub temperature: f64,
    /// 0 = AB, 1 = BC, 2 = CA; the value is ignored for interior nodes.
    pub boundary: u32,
    pub boundary_parameter: f64,
    pub incident_univariant_count: u32,
    pub phases: [u8; 256],
    pub boundary_name: [u8; 32],
    pub dataset_revision: u64,
    pub options_revision: u64,
    pub request_id: u64,
}

/// Result of a single authoritative source-field interpolation query. Source
/// rows are zero-based at the ABI and are presented as one-based rows by Qt.
#[repr(C)]
pub struct TcqtInspectionResult {
    pub success: bool,
    /// 0 defined, 1 missing, 2 non-existing, 3 cut-off, 4 triangle
    /// unavailable, 5 outside domain, 6 evaluation error.
    pub state: u32,
    pub has_value: bool,
    pub value: f64,
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub triangle_index: u32,
    pub has_local_barycentric: bool,
    pub lambda0: f64,
    pub lambda1: f64,
    pub lambda2: f64,
    pub has_contributions: bool,
    pub linear_part: f64,
    pub excess_part: f64,
    pub has_source_rows: bool,
    pub source_row0: u32,
    pub source_row1: u32,
    pub source_row2: u32,
    /// 0 cubic, 1 one-sided cubic, 2 linear fallback, 3 undefined.
    pub local_mode: u32,
    /// Finite EX inputs are accepted by the same field evaluator and remain attributable.
    pub uses_extrapolated_sources: bool,
    /// Zero means no EX source was used; values are otherwise one-based EX layers.
    pub maximum_extrapolation_layer: u32,
    pub extrapolation_methods: [u8; NAME],
    pub extrapolated_source_row_count: u32,
    pub options_revision: u64,
    pub effective_source_interpolation: u32,
    pub effective_cubic_method: u32,
    pub effective_partial_domain_policy: u32,
    pub effective_continuation: u32,
    pub unit: [u8; NAME],
    pub message: [u8; MESSAGE],
}
/// Field-independent point location returned in canonical source-row order.
/// Rows are zero-based at the ABI; the Qt dialog presents them as one-based.
#[repr(C)]
pub struct TcqtLocatedPoint {
    pub success: bool,
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub triangle_index: u32,
    pub source_row0: u32,
    pub source_row1: u32,
    pub source_row2: u32,
    pub lambda0: f64,
    pub lambda1: f64,
    pub lambda2: f64,
    pub message: [u8; MESSAGE],
}
#[repr(C)]
pub struct TcqtProjectSummary {
    pub title: [u8; NAME],
    pub path: [u8; PATH],
    pub component_a: [u8; NAME],
    pub component_b: [u8; NAME],
    pub component_c: [u8; NAME],
    pub phase_count: u32,
    pub property_count: u32,
    pub grid_count: u32,
    pub dirty: bool,
    pub revision: u64,
    pub saved_revision: u64,
    /// 0 invalid, 1 draft-valid, 2 calculation-ready.
    pub validity: u32,
    pub saveable: bool,
    pub calculation_available: bool,
    pub blocking_reason: [u8; MESSAGE],
}
#[repr(C)]
pub struct TcqtSaveResult {
    /// 0 saved, 1 invalid document, 2 serialization failure, 3 write failure.
    pub outcome: u32,
    pub message: [u8; MESSAGE],
    pub path: [u8; PATH],
}
#[repr(C)]
pub struct TcqtPasteResult {
    pub success: bool,
    pub rows_pasted: u32,
    pub columns_pasted: u32,
    pub rows_appended: u32,
    pub header_skipped: bool,
    /// 1-based clipboard source row/column for an error, or zero on success.
    pub clipboard_row: u32,
    pub clipboard_column: u32,
    /// 1-based destination table row/column for an error, or zero on success.
    pub target_row: u32,
    pub target_column: u32,
    pub message: [u8; MESSAGE],
}
#[repr(C)]
pub struct TcqtPhase {
    pub id: u32,
    pub name: [u8; NAME],
}
#[repr(C)]
pub struct TcqtProperty {
    pub ordinal: u32,
    pub required: bool,
    pub name: [u8; NAME],
    pub unit: [u8; NAME],
}
#[repr(C)]
pub struct TcqtGrid {
    pub index: u32,
    pub kind: u32,
    pub subdivisions: u32,
    pub row_count: u32,
    pub field_count: u32,
    pub name: [u8; NAME],
}
#[repr(C)]
pub struct TcqtField {
    pub index: u32,
    pub phase_id: u32,
    pub property: [u8; NAME],
    pub column_name: [u8; NAME],
}
#[repr(C)]
pub struct TcqtRow {
    pub a: f64,
    pub b: f64,
    pub c: f64,
}
/// Rust-owned regular mesh extrapolation request.
/// Scope 0 = field, 1 = phase, and 2 = canonical target rows.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TcqtMeshExtrapolationOptions {
    pub grid_index: u32,
    pub field_index: u32,
    pub phase_id: u32,
    pub scope: u32,
    pub all_phase_properties: bool,
    pub target_rows: *const u32,
    pub target_row_count: u32,
    pub method: u32,
    pub maximum_layers: u32,
    pub minimum_directional_support: u32,
    pub has_maximum_directional_spread: bool,
    pub maximum_directional_spread: f64,
    pub has_minimum_value: bool,
    pub minimum_value: f64,
    pub has_maximum_value: bool,
    pub maximum_value: f64,
}
#[repr(C)]
pub struct TcqtMeshExtrapolationSummary {
    pub success: bool,
    pub fields_processed: u32,
    pub values_proposed: u32,
    pub values_remaining: u32,
    pub maximum_layer: u32,
    pub message: [u8; MESSAGE],
}
/// One proposed or rejected row in the stored mesh extrapolation preview.
#[repr(C)]
pub struct TcqtMeshExtrapolationPreviewRow {
    pub field_index: u32,
    pub phase_id: u32,
    pub row_index: u32,
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub old_state: u32,
    /// 0 requested, 1 dependency, 2 field candidate, 3 rejected.
    pub status: u32,
    pub has_value: bool,
    pub value: f64,
    pub layer: u32,
    pub method: u32,
    pub support_count: u32,
    pub spread: f64,
    pub property: [u8; NAME],
    pub reason: [u8; MESSAGE],
    pub directional_estimates: [u8; MESSAGE],
}
#[repr(C)]
pub struct TcqtCell {
    /// 0 calculated, 2 cut-off, 3 missing, 4 extrapolated. Legacy 1 is
    /// normalized to missing. Undefined cells use has_value=false and value=0.0.
    pub state: u32,
    pub has_value: bool,
    pub value: f64,
    pub extrapolation_layer: u32,
    pub extrapolation_method: u32,
    pub extrapolation_support_count: u32,
    pub extrapolation_spread: f64,
    pub note: [u8; NAME],
}

/// A projected polyline point exported by the Rust calculation pipeline.
///
/// `point_index` is the position within `line_id`; it lets the Qt canvas retain
/// the exact path boundaries rather than inferring them from coordinates.
#[repr(C)]
pub struct TcqtProjectionRecord {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub point_index: u32,
    pub line_type: u32,
    /// Rust-owned ARGB style consumed by the thin Qt QPainter adapter.
    pub rgba: u32,
    pub stroke_width: f64,
    /// 0 = path/circle, 1 = square invariant marker.
    pub marker_kind: u32,
    /// 0 = raw, 1 = regularized. Preserves geometry provenance through Qt.
    pub path_source: u32,
    pub has_level: bool,
    pub level: f64,
    pub unit: [u8; 32],
    pub phase_1: [u8; NAME],
    pub phase_2: [u8; NAME],
    pub line_id: [u8; NAME],
}

#[derive(Clone, Debug, Default)]
struct NumericalTraceRequest {
    level: NumericalTraceLevel,
    destination: Option<PathBuf>,
}

impl NumericalTraceRequest {
    fn config(&self) -> NumericalTraceConfig {
        NumericalTraceConfig {
            level: self.level,
            maximum_events: 500_000,
            ..NumericalTraceConfig::default()
        }
    }
}
struct ProjectDocument {
    dataset: TabulatedTernaryDataset,
    saved_dataset: TabulatedTernaryDataset,
    path: Option<PathBuf>,
    dirty: bool,
    revision: u64,
    saved_revision: u64,
    undo: Vec<TabulatedTernaryDataset>,
    redo: Vec<TabulatedTernaryDataset>,
    contract: GuiContractState,
    projection: Option<LiquidusProjection>,
    raw_projection: Option<LiquidusProjection>,
    regularized_projection: Option<LiquidusProjection>,
    projection_records: Vec<ProjectionCsvRecord>,
    /// Metadata and phase names belonging to the accepted visible projection.
    /// This snapshot keeps export/render ownership coherent while a newer
    /// request is pending or fails.
    accepted_projection_dataset: Option<TabulatedTernaryDataset>,
    accepted_projection_dataset_revision: u64,
    accepted_viewer_options: Option<ViewerCalculationOptions>,
    inspection_cache: FieldInspectionCache,
    calculation_generation: u64,
    options_revision: u64,
    projection_options_revision: u64,
    projection_request_id: u64,
    accepted_topology_key: Option<ViewerTopologyKey>,
    stable_topology_build_count: u64,
    stable_topology_reuse_count: u64,
    isotherm_rebuild_count: u64,
    last_stable_topology_reused: bool,
    // Numerical Viewer settings are validated and retained in Rust. Qt keeps
    // only a presentation mirror for its controls.
    viewer_options: ViewerCalculationOptions,
    trace_request: NumericalTraceRequest,
    mesh_extrapolation_preview: Option<MeshExtrapolationPreview>,
}
impl ProjectDocument {
    fn new() -> Self {
        Self {
            dataset: empty_project_dataset(),
            saved_dataset: empty_project_dataset(),
            path: None,
            dirty: false,
            revision: 1,
            saved_revision: 1,
            undo: Vec::new(),
            redo: Vec::new(),
            contract: GuiContractState::default(),
            projection: None,
            raw_projection: None,
            regularized_projection: None,
            projection_records: Vec::new(),
            accepted_projection_dataset: None,
            accepted_projection_dataset_revision: 0,
            accepted_viewer_options: None,
            inspection_cache: FieldInspectionCache::default(),
            calculation_generation: 0,
            options_revision: 1,
            projection_options_revision: 0,
            projection_request_id: 0,
            accepted_topology_key: None,
            stable_topology_build_count: 0,
            stable_topology_reuse_count: 0,
            isotherm_rebuild_count: 0,
            last_stable_topology_reused: false,
            viewer_options: ViewerCalculationOptions::default(),
            trace_request: NumericalTraceRequest::default(),
            mesh_extrapolation_preview: None,
        }
    }
    /// Invalidate a future calculation without discarding the last accepted
    /// scene.  The Viewer deliberately keeps that immutable snapshot visible
    /// while the newest calculation is pending or has failed.
    fn invalidate_calculation(&mut self) {
        self.accepted_topology_key = None;
        self.last_stable_topology_reused = false;
        self.inspection_cache.invalidate();
        self.calculation_generation = self.calculation_generation.saturating_add(1);
    }

    fn clear_accepted_projection(&mut self) {
        self.projection = None;
        self.raw_projection = None;
        self.regularized_projection = None;
        self.projection_records.clear();
        self.accepted_projection_dataset = None;
        self.accepted_projection_dataset_revision = 0;
        self.accepted_viewer_options = None;
        self.projection_options_revision = 0;
        self.projection_request_id = 0;
    }
    /// Store a validated numerical Viewer configuration.  Options are not TCT
    /// document data, so they advance their own revision and invalidate only
    /// numerical caches rather than dirtying the document.
    fn set_viewer_options(
        &mut self,
        raw_options: TcqtViewerCalculationOptions,
    ) -> Result<bool, String> {
        let options = ViewerCalculationOptions::from_abi(&raw_options)?;
        if self.viewer_options == options {
            return Ok(false);
        }
        let topology_changed = self
            .accepted_topology_key
            .is_none_or(|accepted| accepted != options.topology_key());
        self.viewer_options = options;
        self.options_revision = self.options_revision.saturating_add(1);
        if topology_changed {
            self.invalidate_calculation();
        } else {
            // A level-only request supersedes any running worker but preserves
            // the accepted graph and invariant table for isotherm reuse.
            self.calculation_generation = self.calculation_generation.saturating_add(1);
        }
        Ok(true)
    }
    fn mark_revision_changed(&mut self) {
        self.revision = self.revision.saturating_add(1);
        self.dirty = self.dataset != self.saved_dataset;
        self.contract.revisions.dataset = Revision(self.revision);
    }

    fn mutate(
        &mut self,
        edit: impl FnOnce(&mut TabulatedTernaryDataset) -> Result<(), String>,
    ) -> Result<(), String> {
        let prior = self.dataset.clone();
        if let Err(error) = edit(&mut self.dataset) {
            self.dataset = prior;
            return Err(error);
        }
        if let Err(error) = self.dataset.validate_document_structure() {
            self.dataset = prior;
            return Err(error);
        }
        self.mesh_extrapolation_preview = None;
        self.undo.push(prior);
        if self.undo.len() > 50 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.invalidate_calculation();
        self.mark_revision_changed();
        let _ = update(&mut self.contract, UiAction::DatasetEdited);
        Ok(())
    }
    fn replace_loaded(
        &mut self,
        mut dataset: TabulatedTernaryDataset,
        path: PathBuf,
    ) -> Result<(), String> {
        dataset.validate_saveable_document()?;
        dataset.source_path = Some(path.clone());
        self.dataset = dataset;
        self.saved_dataset = self.dataset.clone();
        self.path = Some(path);
        self.revision = self.revision.saturating_add(1);
        self.saved_revision = self.revision;
        self.dirty = false;
        self.undo.clear();
        self.redo.clear();
        self.mesh_extrapolation_preview = None;
        self.contract = GuiContractState::default();
        self.clear_accepted_projection();
        self.invalidate_calculation();
        // Calculation counters describe the loaded document's accepted
        // projection lifecycle. They must never leak from a replaced project.
        self.stable_topology_build_count = 0;
        self.stable_topology_reuse_count = 0;
        self.isotherm_rebuild_count = 0;
        self.last_stable_topology_reused = false;
        self.contract.revisions.dataset = Revision(self.revision);
        Ok(())
    }
}
fn document() -> &'static Mutex<ProjectDocument> {
    static INSTANCE: OnceLock<Mutex<ProjectDocument>> = OnceLock::new();
    INSTANCE.get_or_init(|| Mutex::new(ProjectDocument::new()))
}
fn bytes<const N: usize>(value: &str) -> [u8; N] {
    let mut output = [0; N];
    let length = value.len().min(N.saturating_sub(1));
    output[..length].copy_from_slice(&value.as_bytes()[..length]);
    output
}
fn status(result: Result<(), String>) -> TcqtStatus {
    match result {
        Ok(()) => TcqtStatus {
            success: true,
            message: bytes("OK"),
        },
        Err(error) => TcqtStatus {
            success: false,
            message: bytes(&error),
        },
    }
}
fn locate_tabulated_grid_point(
    grid: &TabulatedGrid,
    composition: [f64; 3],
) -> Result<TcqtLocatedPoint, String> {
    let composition = normalize_ternary_triplet(composition).map_err(|error| error.to_string())?;
    let (triangle_index, source_rows, local_barycentric) = match grid {
        TabulatedGrid::Regular(grid) => {
            let numerical = RegularTernaryGrid::new(grid.subdivisions)
                .map_err(|error| format!("cannot prepare regular grid location: {error}"))?;
            let location = numerical.locate(composition).map_err(|error| {
                format!("global composition is outside the regular grid: {error}")
            })?;
            let rows = location.triangle.vertices.map(|vertex| vertex.0);
            if rows.iter().any(|row| *row >= grid.compositions.len()) {
                return Err("regular-grid triangle refers to an unavailable source row".into());
            }
            (location.triangle.id, rows, location.barycentric)
        }
        TabulatedGrid::Irregular(grid) => {
            let mesh = IrregularTernaryMesh::new(grid.compositions.iter().copied())
                .map_err(|error| format!("cannot prepare irregular grid location: {error}"))?;
            let location = mesh.locate(composition).map_err(|error| {
                format!("global composition is outside the irregular grid: {error}")
            })?;
            (
                location.triangle.id.0,
                location.triangle.vertices.map(|vertex| vertex.0),
                location.barycentric,
            )
        }
    };
    Ok(TcqtLocatedPoint {
        success: true,
        a: composition[0],
        b: composition[1],
        c: composition[2],
        triangle_index: triangle_index as u32,
        source_row0: source_rows[0] as u32,
        source_row1: source_rows[1] as u32,
        source_row2: source_rows[2] as u32,
        lambda0: local_barycentric[0],
        lambda1: local_barycentric[1],
        lambda2: local_barycentric[2],
        message: bytes("OK"),
    })
}

fn locate_tabulated_grid_local_point(
    grid: &TabulatedGrid,
    triangle_index: u32,
    local: [f64; 3],
) -> Result<TcqtLocatedPoint, String> {
    let vertices = match grid {
        TabulatedGrid::Regular(grid) => {
            let numerical = RegularTernaryGrid::new(grid.subdivisions)
                .map_err(|error| format!("cannot prepare regular grid location: {error}"))?;
            let triangle = numerical
                .triangle(triangle_index as usize)
                .map_err(|error| {
                    format!("regular triangle {triangle_index} is unavailable: {error}")
                })?;
            triangle.vertices.map(|vertex| {
                grid.compositions.get(vertex.0).copied().ok_or_else(|| {
                    "regular-grid triangle refers to an unavailable source row".to_owned()
                })
            })
        }
        TabulatedGrid::Irregular(grid) => {
            let mesh = IrregularTernaryMesh::new(grid.compositions.iter().copied())
                .map_err(|error| format!("cannot prepare irregular grid location: {error}"))?;
            let triangle = mesh
                .triangle(IrregularTriangleId(triangle_index as usize))
                .map_err(|error| {
                    format!("irregular triangle {triangle_index} is unavailable: {error}")
                })?;
            triangle.vertices.map(|vertex| {
                grid.compositions.get(vertex.0).copied().ok_or_else(|| {
                    "irregular triangle refers to an unavailable source row".to_owned()
                })
            })
        }
    };
    let [first, second, third] = vertices;
    let vertices = [first?, second?, third?];
    let composition =
        composition_from_local_barycentric(vertices, local).map_err(|error| error.to_string())?;
    // Always re-locate after the transform. This applies the exact same
    // canonical shared-edge and vertex ownership as field evaluation.
    locate_tabulated_grid_point(grid, composition)
}
/// Store a validated numerical Viewer configuration without changing the TCT
/// document revision or dirty state. Rendering preferences remain Qt-owned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_set_viewer_calculation_options(
    raw_options: *const TcqtViewerCalculationOptions,
) -> TcqtStatus {
    status((|| {
        let raw_options = *unsafe { out(raw_options.cast_mut(), "viewer calculation options") }?;
        let mut state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        state.set_viewer_options(raw_options)?;
        Ok(())
    })())
}

/// Configure a developer-only numerical trace for the next Viewer calculation.
/// This observation request never changes TCT dirty state, calculation options,
/// or their revision.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_set_numerical_trace(
    level: u32,
    destination: *const c_char,
) -> TcqtStatus {
    status((|| {
        let level = match level {
            0 => NumericalTraceLevel::Off,
            1 => NumericalTraceLevel::Summary,
            2 => NumericalTraceLevel::Decisions,
            3 => NumericalTraceLevel::Iterations,
            _ => return Err("unsupported numerical trace level".into()),
        };
        let destination = if level == NumericalTraceLevel::Off {
            None
        } else {
            let destination = unsafe { input(destination, "trace destination") }?;
            if destination.trim().is_empty() {
                return Err("a trace destination is required when tracing is enabled".into());
            }
            Some(PathBuf::from(destination))
        };
        let mut state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        state.trace_request = NumericalTraceRequest { level, destination };
        Ok(())
    })())
}
/// Return the Rust-authoritative numerical Viewer configuration. This lets Qt
/// refresh controls after validation instead of treating widget values as the
/// source of truth.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_viewer_calculation_options(
    output_value: *mut TcqtViewerCalculationOptions,
) -> TcqtStatus {
    status((|| {
        let output_value = unsafe { out(output_value, "viewer calculation options") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        *output_value = state.viewer_options.to_abi();
        Ok(())
    })())
}

/// Return the complete Rust-authoritative Viewer option snapshot used for
/// revision-checked worker submission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_viewer_calculation_state(
    output_value: *mut TcqtViewerCalculationState,
) -> TcqtStatus {
    status((|| {
        let output_value = unsafe { out(output_value, "viewer calculation state") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        *output_value = TcqtViewerCalculationState {
            options: state.viewer_options.to_abi(),
            options_revision: state.options_revision,
        };
        Ok(())
    })())
}
fn inspection_state(state: &InterpolatedResultState) -> u32 {
    match state {
        InterpolatedResultState::Defined => 0,
        InterpolatedResultState::UndefinedMissing => 1,
        InterpolatedResultState::UndefinedNonExisting => 2,
        InterpolatedResultState::UndefinedCutOff => 3,
        InterpolatedResultState::TriangleUnavailable => 4,
        InterpolatedResultState::OutsideDomain => 5,
        InterpolatedResultState::Error(_) => 6,
    }
}

fn local_mode(mode: Option<ternary_contours::LocalInterpolationMode>) -> u32 {
    use ternary_contours::LocalInterpolationMode;
    match mode {
        Some(LocalInterpolationMode::Linear) => 2,
        Some(LocalInterpolationMode::Cubic) => 0,
        Some(LocalInterpolationMode::OneSidedCubic) => 1,
        Some(LocalInterpolationMode::LinearFallback) => 2,
        Some(LocalInterpolationMode::Undefined) | None => 3,
    }
}
fn save_result(outcome: u32, message: impl AsRef<str>, path: impl AsRef<str>) -> TcqtSaveResult {
    TcqtSaveResult {
        outcome,
        message: bytes(message.as_ref()),
        path: bytes(path.as_ref()),
    }
}
#[derive(Debug)]
struct PasteFailure {
    message: String,
    clipboard_row: usize,
    clipboard_column: usize,
    target_row: usize,
    target_column: usize,
}

impl PasteFailure {
    fn new(
        message: impl Into<String>,
        clipboard_row: usize,
        clipboard_column: usize,
        target_row: usize,
        target_column: usize,
    ) -> Self {
        Self {
            message: message.into(),
            clipboard_row,
            clipboard_column,
            target_row,
            target_column,
        }
    }

    fn result(self) -> TcqtPasteResult {
        TcqtPasteResult {
            success: false,
            rows_pasted: 0,
            columns_pasted: 0,
            rows_appended: 0,
            header_skipped: false,
            clipboard_row: self.clipboard_row as u32,
            clipboard_column: self.clipboard_column as u32,
            target_row: self.target_row as u32,
            target_column: self.target_column as u32,
            message: bytes(&self.message),
        }
    }
}

fn paste_success(
    rows_pasted: usize,
    columns_pasted: usize,
    rows_appended: usize,
    header_skipped: bool,
    grid_name: &str,
) -> TcqtPasteResult {
    let message = if rows_appended > 0 {
        format!(
            "Pasted {} rows x {} columns and added {} irregular-grid rows to grid \"{}\".",
            rows_pasted, columns_pasted, rows_appended, grid_name
        )
    } else {
        format!(
            "Pasted {} rows x {} columns into grid \"{}\".",
            rows_pasted, columns_pasted, grid_name
        )
    };
    TcqtPasteResult {
        success: true,
        rows_pasted: rows_pasted as u32,
        columns_pasted: columns_pasted as u32,
        rows_appended: rows_appended as u32,
        header_skipped,
        clipboard_row: 0,
        clipboard_column: 0,
        target_row: 0,
        target_column: 0,
        message: bytes(&message),
    }
}

fn paste_headers(
    dataset: &TabulatedTernaryDataset,
    grid_index: usize,
) -> Result<Vec<String>, PasteFailure> {
    let grid = dataset
        .grids
        .get(grid_index)
        .ok_or_else(|| PasteFailure::new("selected grid is unavailable", 0, 0, 0, 0))?;
    let mut headers = dataset
        .components
        .iter()
        .map(|component| component.name.clone())
        .collect::<Vec<_>>();
    headers.extend(grid.fields().iter().map(|field| field.column_name.clone()));
    Ok(headers)
}

fn parse_pasted_value(token: &str, missing_tokens: &[String]) -> Result<TabulatedValue, String> {
    if token.trim().is_empty() {
        return Err("Blank cells are not accepted. Use NA to represent a missing value.".into());
    }
    parse_tabulated_value_token(token, missing_tokens, false)
        .map_err(|_| "Unsupported value. Enter a finite number, NA, or CO.".to_owned())
}

fn parse_pasted_composition(token: &str) -> Result<f64, String> {
    let value = token
        .trim()
        .parse::<f64>()
        .map_err(|_| "Composition must be a finite non-negative number.".to_owned())?;
    if !value.is_finite() || value < 0.0 {
        return Err("Composition must be a finite non-negative number.".into());
    }
    Ok(value)
}

fn validate_pasted_irregular_compositions(
    compositions: &[[f64; 3]],
) -> Result<(), (usize, String)> {
    for (row, point) in compositions.iter().enumerate() {
        if point.iter().any(|value| !value.is_finite() || *value < 0.0)
            || (point.iter().sum::<f64>() - 1.0).abs() > 1.0e-8
        {
            return Err((
                row + 1,
                "irregular compositions must be finite, non-negative, and sum to one".into(),
            ));
        }
        for (previous, other) in compositions[..row].iter().enumerate() {
            if point
                .iter()
                .zip(other)
                .map(|(left, right)| (left - right).abs())
                .fold(0.0, f64::max)
                <= 1.0e-10
            {
                return Err((
                    row + 1,
                    format!(
                        "duplicate or near-duplicate composition matches row {}",
                        previous + 1
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn prepare_grid_paste(
    dataset: &TabulatedTernaryDataset,
    grid_index: usize,
    start_row: usize,
    start_column: usize,
    clipboard: &str,
) -> Result<(TabulatedTernaryDataset, usize, usize, usize, bool, String), PasteFailure> {
    let headers = paste_headers(dataset, grid_index)?;
    if start_column >= headers.len() {
        return Err(PasteFailure::new(
            "destination column is outside the selected grid",
            0,
            0,
            start_row + 1,
            start_column + 1,
        ));
    }
    let table = ParsedTable::parse_tsv(clipboard, HeaderMode::Absent).map_err(|error| {
        let (row, column) = error
            .location
            .map(|location| (location.row, location.column))
            .unwrap_or((0, 0));
        PasteFailure::new(
            error.message,
            row,
            column,
            start_row + row,
            start_column + column,
        )
    })?;
    let width = table.width();
    if width == 0 || start_column + width > headers.len() {
        return Err(PasteFailure::new(
            format!(
                "The clipboard contains {} columns, but only {} columns are available from the selected destination.",
                width,
                headers.len().saturating_sub(start_column)
            ),
            0,
            0,
            start_row + 1,
            start_column + 1,
        ));
    }
    let mut rows = table.rows;
    let header_skipped = rows.first().is_some_and(|row| {
        row.cells.len() == width
            && row
                .cells
                .iter()
                .enumerate()
                .all(|(offset, cell)| cell.text == headers[start_column + offset])
    });
    if header_skipped {
        rows.remove(0);
    }
    if rows.is_empty() {
        return Err(PasteFailure::new(
            "Clipboard contains a header but no data rows.",
            1,
            1,
            start_row + 1,
            start_column + 1,
        ));
    }
    let grid = dataset
        .grids
        .get(grid_index)
        .ok_or_else(|| PasteFailure::new("selected grid is unavailable", 0, 0, 0, 0))?;
    let existing_rows = grid.compositions().len();
    let mut candidate = dataset.clone();
    let mut rows_appended = 0;
    if matches!(grid, TabulatedGrid::Regular(_)) {
        if start_column < 3 {
            return Err(PasteFailure::new(
                "Paste cannot modify the composition columns of a regular grid. Select the first phase/property column and try again.",
                0,
                0,
                start_row + 1,
                start_column + 1,
            ));
        }
        if start_row + rows.len() > existing_rows {
            return Err(PasteFailure::new(
                format!(
                    "The clipboard contains {} rows, but only {} rows are available from the selected destination.",
                    rows.len(),
                    existing_rows.saturating_sub(start_row)
                ),
                0,
                0,
                start_row + 1,
                start_column + 1,
            ));
        }
        let mut assignments = Vec::with_capacity(rows.len() * width);
        for (offset, row) in rows.iter().enumerate() {
            for (column, cell) in row.cells.iter().enumerate() {
                let target_column = start_column + column;
                let value =
                    parse_pasted_value(&cell.text, &dataset.missing_tokens).map_err(|message| {
                        PasteFailure::new(
                            message,
                            row.source_row,
                            cell.location.column,
                            start_row + offset + 1,
                            target_column + 1,
                        )
                    })?;
                assignments.push((start_row + offset, target_column - 3, value));
            }
        }
        let grid = candidate
            .grids
            .get_mut(grid_index)
            .expect("checked grid index");
        let mut touched_fields = assignments
            .iter()
            .map(|(_, field_index, _)| *field_index)
            .collect::<Vec<_>>();
        touched_fields.sort_unstable();
        touched_fields.dedup();
        for field_index in touched_fields {
            for existing in &mut fields_mut(grid)[field_index].values {
                existing.clear_if_extrapolated();
            }
        }
        for (row, field_index, value) in assignments {
            fields_mut(grid)
                .get_mut(field_index)
                .expect("checked field index")
                .values[row] = value;
        }
    } else {
        if start_row > existing_rows {
            return Err(PasteFailure::new(
                "Paste cannot leave a gap before appended irregular-grid rows.",
                0,
                0,
                start_row + 1,
                start_column + 1,
            ));
        }
        rows_appended = (start_row + rows.len()).saturating_sub(existing_rows);
        if rows_appended > 0 && (start_column != 0 || width < 3) {
            return Err(PasteFailure::new(
                "New irregular-grid rows require A, B, and C values. No cells were changed.",
                0,
                0,
                existing_rows + 1,
                start_column + 1,
            ));
        }
        let grid = candidate
            .grids
            .get_mut(grid_index)
            .expect("checked grid index");
        let TabulatedGrid::Irregular(grid) = grid else {
            unreachable!()
        };
        for _ in 0..rows_appended {
            grid.compositions.push([0.0; 3]);
            for field in &mut grid.fields {
                field.values.push(TabulatedValue::missing());
                field.row_lines.push(0);
            }
        }
        let mut compositions = Vec::new();
        let mut assignments = Vec::new();
        for (offset, row) in rows.iter().enumerate() {
            let target_row = start_row + offset;
            for (column, cell) in row.cells.iter().enumerate() {
                let target_column = start_column + column;
                if target_column < 3 {
                    let value = parse_pasted_composition(&cell.text).map_err(|message| {
                        PasteFailure::new(
                            message,
                            row.source_row,
                            cell.location.column,
                            target_row + 1,
                            target_column + 1,
                        )
                    })?;
                    compositions.push((target_row, target_column, value));
                } else {
                    let value = parse_pasted_value(&cell.text, &dataset.missing_tokens).map_err(
                        |message| {
                            PasteFailure::new(
                                message,
                                row.source_row,
                                cell.location.column,
                                target_row + 1,
                                target_column + 1,
                            )
                        },
                    )?;
                    assignments.push((target_row, target_column - 3, value));
                }
            }
        }
        for (row, component, value) in compositions {
            grid.compositions[row][component] = value;
        }
        if let Err((row, message)) = validate_pasted_irregular_compositions(&grid.compositions) {
            return Err(PasteFailure::new(message, row, 0, row, 1));
        }
        let mut touched_fields = assignments
            .iter()
            .map(|(_, field_index, _)| *field_index)
            .collect::<Vec<_>>();
        touched_fields.sort_unstable();
        touched_fields.dedup();
        for field_index in touched_fields {
            for existing in &mut grid.fields[field_index].values {
                existing.clear_if_extrapolated();
            }
        }
        for (row, field_index, value) in assignments {
            grid.fields
                .get_mut(field_index)
                .ok_or_else(|| {
                    PasteFailure::new(
                        "destination field is unavailable",
                        0,
                        0,
                        row + 1,
                        field_index + 4,
                    )
                })?
                .values[row] = value;
        }
    }
    candidate
        .validate_document_structure()
        .map_err(|message| PasteFailure::new(message, 0, 0, 0, 0))?;
    Ok((
        candidate,
        rows.len(),
        width,
        rows_appended,
        header_skipped,
        grid.name().to_owned(),
    ))
}

unsafe fn input(value: *const c_char, label: &str) -> Result<String, String> {
    if value.is_null() {
        return Err(format!("{label} is required"));
    }
    // SAFETY: a non-null Qt UTF-8 string is NUL terminated.
    unsafe { CStr::from_ptr(value) }
        .to_str()
        .map(str::to_owned)
        .map_err(|_| format!("{label} must be valid UTF-8"))
}
unsafe fn out<'a, T>(value: *mut T, label: &str) -> Result<&'a mut T, String> {
    if value.is_null() {
        return Err(format!("{label} output is required"));
    }
    // SAFETY: the ABI requires caller-provided writable output storage.
    Ok(unsafe { &mut *value })
}
fn fields_mut(grid: &mut TabulatedGrid) -> &mut Vec<TabulatedField> {
    match grid {
        TabulatedGrid::Regular(grid) => &mut grid.fields,
        TabulatedGrid::Irregular(grid) => &mut grid.fields,
    }
}
fn add_field(
    fields: &mut Vec<TabulatedField>,
    row_count: usize,
    phase: &PhaseDefinition,
    property: &PropertyDefinition,
) {
    fields.push(TabulatedField {
        phase_id: phase.id,
        property: property.name.clone(),
        column_name: format!("{}.{}", phase.name, property.name),
        values: vec![TabulatedValue::missing(); row_count],
        row_lines: vec![0; row_count],
    });
}
fn initialise_fields(dataset: &TabulatedTernaryDataset, grid: &mut TabulatedGrid) {
    let row_count = grid.compositions().len();
    let fields = fields_mut(grid);
    for phase in &dataset.phases {
        for property in &dataset.properties {
            add_field(fields, row_count, phase, property);
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_new_document() -> TcqtStatus {
    status((|| {
        *document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())? = ProjectDocument::new();
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_open_document(path: *const c_char) -> TcqtStatus {
    status((|| {
        // SAFETY: Qt passes a NUL-terminated UTF-8 filesystem path.
        let path = PathBuf::from(unsafe { input(path, "document path") }?);
        let dataset = parse_path(&path).map_err(|error| error.to_string())?;
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .replace_loaded(dataset, path)
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_save_document(path: *const c_char) -> TcqtSaveResult {
    let path = match unsafe { input(path, "document path") } {
        Ok(path) => PathBuf::from(path),
        Err(error) => return save_result(3, error, ""),
    };
    let attempted = path.to_string_lossy().into_owned();
    let mut state = match document().lock() {
        Ok(state) => state,
        Err(_) => return save_result(3, "project lock is unavailable", &attempted),
    };
    if let Err(error) = state.dataset.validate_saveable_document() {
        return save_result(1, format!("Cannot save project: {error}"), &attempted);
    }
    let text = match serialize_tct(&state.dataset, &TctSerializeOptions::default()) {
        Ok(text) => text,
        Err(error) => {
            return save_result(
                2,
                format!("Could not serialize project: {error}"),
                &attempted,
            );
        }
    };
    if let Err(error) = save_tct_atomic(&path, &text) {
        return save_result(3, format!("Could not save project: {error}"), &attempted);
    }
    state.dataset.source_path = Some(path.clone());
    state.path = Some(path);
    state.saved_revision = state.revision;
    state.saved_dataset = state.dataset.clone();
    state.dirty = false;
    let (validity, _, _, _) = document_status(&state.dataset);
    let message = if validity == 1 {
        format!("Saved {} - draft document", attempted)
    } else {
        format!("Saved {}", attempted)
    };
    save_result(0, message, &attempted)
}
fn document_status(dataset: &TabulatedTernaryDataset) -> (u32, bool, bool, String) {
    match dataset.validate_saveable_document() {
        Err(error) => (0, false, false, error),
        Ok(()) => match dataset.validate_calculation_readiness() {
            Ok(()) => (2, true, true, String::new()),
            Err(error) => (1, true, false, error),
        },
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_project_summary(output_value: *mut TcqtProjectSummary) -> TcqtStatus {
    status((|| {
        // SAFETY: caller supplies writable summary storage.
        let output_value = unsafe { out(output_value, "project summary") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let (validity, saveable, calculation_available, blocking_reason) =
            document_status(&state.dataset);
        *output_value = TcqtProjectSummary {
            title: bytes(state.dataset.title.as_deref().unwrap_or("Untitled")),
            path: bytes(
                state
                    .path
                    .as_ref()
                    .and_then(|path| path.to_str())
                    .unwrap_or(""),
            ),
            component_a: bytes(&state.dataset.components[0].name),
            component_b: bytes(&state.dataset.components[1].name),
            component_c: bytes(&state.dataset.components[2].name),
            phase_count: state.dataset.phases.len() as u32,
            property_count: state.dataset.properties.len() as u32,
            grid_count: state.dataset.grids.len() as u32,
            dirty: state.dirty,
            revision: state.revision,
            saved_revision: state.saved_revision,
            validity,
            saveable,
            calculation_available,
            blocking_reason: bytes(&blocking_reason),
        };
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_phase_at(index: u32, output_value: *mut TcqtPhase) -> TcqtStatus {
    status((|| {
        // SAFETY: caller supplies writable phase storage.
        let output_value = unsafe { out(output_value, "phase") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let phase = state
            .dataset
            .phases
            .get(index as usize)
            .ok_or("phase index is out of range")?;
        *output_value = TcqtPhase {
            id: phase.id.0,
            name: bytes(&phase.name),
        };
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_property_at(
    index: u32,
    output_value: *mut TcqtProperty,
) -> TcqtStatus {
    status((|| {
        // SAFETY: caller supplies writable property storage.
        let output_value = unsafe { out(output_value, "property") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let property = state
            .dataset
            .properties
            .get(index as usize)
            .ok_or("property index is out of range")?;
        *output_value = TcqtProperty {
            ordinal: index,
            required: property.required,
            name: bytes(&property.name),
            unit: bytes(&property.unit),
        };
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_grid_at(index: u32, output_value: *mut TcqtGrid) -> TcqtStatus {
    status((|| {
        // SAFETY: caller supplies writable grid storage.
        let output_value = unsafe { out(output_value, "grid") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let grid = state
            .dataset
            .grids
            .get(index as usize)
            .ok_or("grid index is out of range")?;
        *output_value = TcqtGrid {
            index,
            kind: match grid.grid_type() {
                GridType::Regular => 0,
                GridType::Irregular => 1,
            },
            subdivisions: match grid {
                TabulatedGrid::Regular(grid) => grid.subdivisions as u32,
                TabulatedGrid::Irregular(_) => 0,
            },
            row_count: grid.compositions().len() as u32,
            field_count: grid.fields().len() as u32,
            name: bytes(grid.name()),
        };
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_grid_field_at(
    grid_index: u32,
    field_index: u32,
    output_value: *mut TcqtField,
) -> TcqtStatus {
    status((|| {
        // SAFETY: caller supplies writable field storage.
        let output_value = unsafe { out(output_value, "field") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let grid = state
            .dataset
            .grids
            .get(grid_index as usize)
            .ok_or("grid index is out of range")?;
        let field = grid
            .fields()
            .get(field_index as usize)
            .ok_or("field index is out of range")?;
        *output_value = TcqtField {
            index: field_index,
            phase_id: field.phase_id.0,
            property: bytes(&field.property),
            column_name: bytes(&field.column_name),
        };
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_grid_row_at(
    grid_index: u32,
    row_index: u32,
    output_value: *mut TcqtRow,
) -> TcqtStatus {
    status((|| {
        // SAFETY: caller supplies writable row storage.
        let output_value = unsafe { out(output_value, "row") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let row = state
            .dataset
            .grids
            .get(grid_index as usize)
            .ok_or("grid index is out of range")?
            .compositions()
            .get(row_index as usize)
            .ok_or("row index is out of range")?;
        *output_value = TcqtRow {
            a: row[0],
            b: row[1],
            c: row[2],
        };
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_preview_regular_mesh_extrapolation(
    options: *const TcqtMeshExtrapolationOptions,
    output_value: *mut TcqtMeshExtrapolationSummary,
) -> TcqtStatus {
    status((|| {
        // SAFETY: caller supplies readable options and writable summary storage.
        let options = unsafe { options.as_ref() }.ok_or("mesh extrapolation options are null")?;
        let output_value = unsafe { out(output_value, "mesh extrapolation summary") }?;
        let mut state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let grid = state
            .dataset
            .grids
            .get(options.grid_index as usize)
            .ok_or("grid index is out of range")?;
        let TabulatedGrid::Regular(regular) = grid else {
            return Err(
                "Automatic mesh extrapolation is currently available for regular grids only."
                    .into(),
            );
        };
        let target_rows = if options.target_row_count == 0 {
            Vec::new()
        } else {
            if options.target_rows.is_null() {
                return Err("mesh extrapolation target rows are null".into());
            }
            // SAFETY: the ABI requires target_row_count readable rows for this call.
            unsafe {
                std::slice::from_raw_parts(options.target_rows, options.target_row_count as usize)
            }
            .iter()
            .map(|row| *row as usize)
            .collect::<Vec<_>>()
        };
        let requested_field = |field_index: u32| -> Result<MeshExtrapolationField, String> {
            let field = regular
                .fields
                .get(field_index as usize)
                .ok_or("field index is out of range")?;
            let phase = state
                .dataset
                .phases
                .iter()
                .find(|phase| phase.id == field.phase_id)
                .ok_or("field references an unknown phase")?;
            Ok(MeshExtrapolationField {
                phase: phase.name.clone(),
                property: field.property.clone(),
            })
        };
        let (fields, all_fields) = match options.scope {
            ABI_MESH_SCOPE_FIELD => {
                if options.field_index == u32::MAX {
                    (Vec::new(), true)
                } else {
                    (vec![requested_field(options.field_index)?], false)
                }
            }
            ABI_MESH_SCOPE_PHASE => {
                let phase = state
                    .dataset
                    .phases
                    .iter()
                    .find(|phase| phase.id.0 == options.phase_id)
                    .ok_or("selected phase is unavailable")?;
                if options.all_phase_properties {
                    let fields = regular
                        .fields
                        .iter()
                        .filter(|field| field.phase_id == phase.id)
                        .map(|field| MeshExtrapolationField {
                            phase: phase.name.clone(),
                            property: field.property.clone(),
                        })
                        .collect::<Vec<_>>();
                    if fields.is_empty() {
                        return Err("selected phase has no fields in this grid".into());
                    }
                    (fields, false)
                } else {
                    let field = requested_field(options.field_index)?;
                    if field.phase != phase.name {
                        return Err(
                            "selected property does not belong to the selected phase".into()
                        );
                    }
                    (vec![field], false)
                }
            }
            ABI_MESH_SCOPE_TARGETS => {
                if target_rows.is_empty() {
                    return Err("select at least one missing vertex to extrapolate".into());
                }
                (vec![requested_field(options.field_index)?], false)
            }
            _ => return Err("unsupported mesh extrapolation scope".into()),
        };
        let method = match options.method {
            ABI_CUBIC_AKIMA => CubicAlphaMethod::Akima,
            ABI_CUBIC_MAKIMA => CubicAlphaMethod::Makima,
            ABI_CUBIC_PCHIP => CubicAlphaMethod::Pchip,
            ABI_CUBIC_STEFFEN => CubicAlphaMethod::Steffen,
            _ => return Err("unsupported cubic extrapolation method".into()),
        };
        let preview = extrapolate_regular_grid_fields(
            &state.dataset,
            &MeshExtrapolationRequest {
                grid: regular.name.clone(),
                fields,
                all_fields,
                target_rows,
                options: RegularMeshExtrapolationOptions {
                    method,
                    maximum_layers: u16::try_from(options.maximum_layers)
                        .map_err(|_| "maximum extrapolation layers exceed u16")?,
                    minimum_directional_support: options.minimum_directional_support as usize,
                    maximum_directional_spread: options
                        .has_maximum_directional_spread
                        .then_some(options.maximum_directional_spread),
                    minimum_value: options.has_minimum_value.then_some(options.minimum_value),
                    maximum_value: options.has_maximum_value.then_some(options.maximum_value),
                    ..RegularMeshExtrapolationOptions::default()
                },
            },
        )
        .map_err(|error| error.to_string())?;
        let fields_processed = u32::try_from(preview.fields.len()).unwrap_or(u32::MAX);
        let values_proposed = preview
            .fields
            .iter()
            .map(|field| field.values.len())
            .sum::<usize>();
        let values_remaining = preview
            .fields
            .iter()
            .map(|field| field.diagnostics.remaining_eligible_missing_values)
            .sum::<usize>();
        let maximum_layer = preview
            .fields
            .iter()
            .flat_map(|field| field.values.iter().map(|value| value.layer))
            .max()
            .unwrap_or(0);
        *output_value = TcqtMeshExtrapolationSummary {
            success: true,
            fields_processed,
            values_proposed: u32::try_from(values_proposed).unwrap_or(u32::MAX),
            values_remaining: u32::try_from(values_remaining).unwrap_or(u32::MAX),
            maximum_layer: u32::from(maximum_layer),
            message: bytes(&format!(
                "Preview: {values_proposed} EX values across {fields_processed} fields; {values_remaining} eligible NA cells remain"
            )),
        };
        state.mesh_extrapolation_preview = Some(preview);
        Ok(())
    })())
}

fn state_code(value: TabulatedValueState) -> u32 {
    match value {
        TabulatedValueState::Calculated => 0,
        TabulatedValueState::CutOff => 2,
        TabulatedValueState::Missing => 3,
        TabulatedValueState::Extrapolated => 4,
    }
}

fn method_code(value: CubicAlphaMethod) -> u32 {
    match value {
        CubicAlphaMethod::Akima => ABI_CUBIC_AKIMA,
        CubicAlphaMethod::Makima => ABI_CUBIC_MAKIMA,
        CubicAlphaMethod::Pchip => ABI_CUBIC_PCHIP,
        CubicAlphaMethod::Steffen => ABI_CUBIC_STEFFEN,
        _ => u32::MAX,
    }
}

fn preview_row_count(preview: &MeshExtrapolationPreview) -> usize {
    preview
        .fields
        .iter()
        .map(|field| field.values.len() + field.rejections.len())
        .sum()
}

fn preview_row(
    state: &ProjectDocument,
    index: usize,
) -> Result<TcqtMeshExtrapolationPreviewRow, String> {
    let preview = state
        .mesh_extrapolation_preview
        .as_ref()
        .ok_or("create and review a mesh extrapolation preview first")?;
    let TabulatedGrid::Regular(grid) = state
        .dataset
        .grids
        .get(preview.grid_index)
        .ok_or("preview grid is unavailable")?
    else {
        return Err(
            "Automatic mesh extrapolation is currently available for regular grids only.".into(),
        );
    };
    let mut current = 0usize;
    for preview_field in &preview.fields {
        let field_index = grid
            .fields
            .iter()
            .position(|field| {
                field.phase_id == preview_field.phase_id && field.property == preview_field.property
            })
            .ok_or("preview field is unavailable")?;
        let field = &grid.fields[field_index];
        for value in &preview_field.values {
            if current == index {
                let composition = grid
                    .compositions
                    .get(value.vertex_index)
                    .ok_or("preview row is unavailable")?;
                let prior = field
                    .values
                    .get(value.vertex_index)
                    .ok_or("preview source is unavailable")?;
                let status = if preview_field
                    .requested_rows
                    .binary_search(&value.vertex_index)
                    .is_ok()
                {
                    0
                } else if preview_field
                    .dependency_rows
                    .binary_search(&value.vertex_index)
                    .is_ok()
                {
                    1
                } else {
                    2
                };
                return Ok(TcqtMeshExtrapolationPreviewRow {
                    field_index: field_index as u32,
                    phase_id: preview_field.phase_id.0,
                    row_index: value.vertex_index as u32,
                    a: composition[0],
                    b: composition[1],
                    c: composition[2],
                    old_state: state_code(prior.state),
                    status,
                    has_value: true,
                    value: value.value,
                    layer: value.layer as u32,
                    method: method_code(value.method),
                    support_count: value.directional_support_count as u32,
                    spread: value.spread,
                    property: bytes(&preview_field.property),
                    reason: [0; MESSAGE],
                    directional_estimates: bytes(
                        &value
                            .directional_estimates
                            .iter()
                            .map(|estimate| {
                                let rows = estimate
                                    .support_vertex_indices
                                    .iter()
                                    .map(|row| (row + 1).to_string())
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                let samples = estimate
                                    .support_values
                                    .iter()
                                    .map(|sample| format!("{sample:.8}"))
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                format!(
                                    "{:?}: rows [{rows}], values [{samples}] -> {:.8}",
                                    estimate.direction, estimate.value
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n"),
                    ),
                });
            }
            current += 1;
        }
        for rejection in &preview_field.rejections {
            if current == index {
                let composition = grid
                    .compositions
                    .get(rejection.vertex_index)
                    .ok_or("preview rejection row is unavailable")?;
                let prior = field
                    .values
                    .get(rejection.vertex_index)
                    .ok_or("preview source is unavailable")?;
                return Ok(TcqtMeshExtrapolationPreviewRow {
                    field_index: field_index as u32,
                    phase_id: preview_field.phase_id.0,
                    row_index: rejection.vertex_index as u32,
                    a: composition[0],
                    b: composition[1],
                    c: composition[2],
                    old_state: state_code(prior.state),
                    status: 3,
                    has_value: false,
                    value: 0.0,
                    layer: rejection.layer as u32,
                    method: u32::MAX,
                    support_count: 0,
                    spread: 0.0,
                    property: bytes(&preview_field.property),
                    reason: bytes(
                        &rejection
                            .reasons
                            .iter()
                            .map(|(_, reason)| reason.to_string())
                            .collect::<Vec<_>>()
                            .join("; "),
                    ),
                    directional_estimates: [0; MESSAGE],
                });
            }
            current += 1;
        }
    }
    Err("preview row index is out of range".into())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_mesh_extrapolation_preview_row_count(
    output_value: *mut u32,
) -> TcqtStatus {
    status((|| {
        let output = unsafe { out(output_value, "mesh extrapolation preview row count") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let preview = state
            .mesh_extrapolation_preview
            .as_ref()
            .ok_or("create and review a mesh extrapolation preview first")?;
        *output = u32::try_from(preview_row_count(preview))
            .map_err(|_| "mesh extrapolation preview has too many rows")?;
        Ok(())
    })())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_mesh_extrapolation_preview_row_at(
    index: u32,
    output_value: *mut TcqtMeshExtrapolationPreviewRow,
) -> TcqtStatus {
    status((|| {
        let output = unsafe { out(output_value, "mesh extrapolation preview row") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        *output = preview_row(&state, index as usize)?;
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_materialize_regular_mesh_extrapolation(
    output_value: *mut TcqtMeshExtrapolationSummary,
) -> TcqtStatus {
    status((|| {
        // SAFETY: caller supplies writable summary storage.
        let output_value = unsafe { out(output_value, "mesh extrapolation summary") }?;
        let mut state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let preview = state
            .mesh_extrapolation_preview
            .clone()
            .ok_or("create and review a mesh extrapolation preview first")?;
        let fields_processed = preview.fields.len();
        let values_remaining = preview
            .fields
            .iter()
            .map(|field| field.diagnostics.remaining_eligible_missing_values)
            .sum::<usize>();
        let summary = apply_mesh_extrapolation(&mut state.dataset.clone(), preview.clone())
            .map_err(|error| error.to_string())?;
        state.mutate(|dataset| {
            apply_mesh_extrapolation(dataset, preview)
                .map(|_| ())
                .map_err(|error| error.to_string())
        })?;
        *output_value = TcqtMeshExtrapolationSummary {
            success: true,
            fields_processed: u32::try_from(fields_processed).unwrap_or(u32::MAX),
            values_proposed: u32::try_from(summary.values_created).unwrap_or(u32::MAX),
            values_remaining: u32::try_from(values_remaining).unwrap_or(u32::MAX),
            maximum_layer: u32::from(summary.maximum_layer),
            message: bytes(&format!(
                "Materialized {} EX values; projection is stale and will recalculate",
                summary.values_created
            )),
        };
        Ok(())
    })())
}

#[unsafe(no_mangle)]
pub extern "C" fn tcqt_clear_extrapolated_grid_values(
    grid_index: u32,
    field_index: u32,
) -> TcqtStatus {
    status((|| {
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                let grid = dataset
                    .grids
                    .get_mut(grid_index as usize)
                    .ok_or("grid index is out of range")?;
                let fields = fields_mut(grid);
                let selected = if field_index == u32::MAX {
                    (0..fields.len()).collect::<Vec<_>>()
                } else {
                    (field_index as usize..=field_index as usize).collect::<Vec<_>>()
                };
                for index in selected {
                    let field = fields.get_mut(index).ok_or("field index is out of range")?;
                    for value in &mut field.values {
                        if matches!(value.state, TabulatedValueState::Extrapolated) {
                            *value = TabulatedValue::missing();
                        }
                    }
                }
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_clear_extrapolated_phase_values(
    grid_index: u32,
    phase_id: u32,
) -> TcqtStatus {
    status((|| {
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                let grid = dataset
                    .grids
                    .get_mut(grid_index as usize)
                    .ok_or("grid index is out of range")?;
                let mut found_phase = false;
                for field in fields_mut(grid)
                    .iter_mut()
                    .filter(|field| field.phase_id.0 == phase_id)
                {
                    found_phase = true;
                    for value in &mut field.values {
                        if matches!(value.state, TabulatedValueState::Extrapolated) {
                            *value = TabulatedValue::missing();
                        }
                    }
                }
                if !found_phase {
                    return Err("phase has no fields in the selected grid".into());
                }
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_grid_cell_at(
    grid_index: u32,
    field_index: u32,
    row_index: u32,
    output_value: *mut TcqtCell,
) -> TcqtStatus {
    status((|| {
        // SAFETY: caller supplies writable cell storage.
        let output_value = unsafe { out(output_value, "cell") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let value = state
            .dataset
            .grids
            .get(grid_index as usize)
            .ok_or("grid index is out of range")?
            .fields()
            .get(field_index as usize)
            .ok_or("field index is out of range")?
            .values
            .get(row_index as usize)
            .ok_or("row index is out of range")?;
        *output_value = TcqtCell {
            state: match value.state {
                TabulatedValueState::Calculated => 0,
                TabulatedValueState::CutOff => 2,
                TabulatedValueState::Missing => 3,
                TabulatedValueState::Extrapolated => 4,
            },
            has_value: value.value.is_some(),
            value: value.value.unwrap_or_default(),
            extrapolation_layer: value
                .extrapolation
                .as_ref()
                .map_or(0, |metadata| u32::from(metadata.layer)),
            extrapolation_method: value
                .extrapolation
                .as_ref()
                .map_or(0, |metadata| match metadata.method {
                    CubicAlphaMethod::Akima => 0,
                    CubicAlphaMethod::Makima => 1,
                    CubicAlphaMethod::Pchip => 2,
                    CubicAlphaMethod::Steffen => 3,
                    _ => u32::MAX,
                }),
            extrapolation_support_count: value
                .extrapolation
                .as_ref()
                .map_or(0, |metadata| u32::from(metadata.support_count)),
            extrapolation_spread: value
                .extrapolation
                .as_ref()
                .map_or(0.0, |metadata| metadata.spread),
            note: bytes(value.note.as_deref().unwrap_or("")),
        };
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_paste_grid_tsv(
    grid_index: u32,
    start_row: u32,
    start_column: u32,
    clipboard: *const c_char,
) -> TcqtPasteResult {
    let clipboard = match unsafe { input(clipboard, "clipboard text") } {
        Ok(value) => value,
        Err(error) => return PasteFailure::new(error, 0, 0, 0, 0).result(),
    };
    let mut state = match document().lock() {
        Ok(state) => state,
        Err(_) => return PasteFailure::new("project lock is unavailable", 0, 0, 0, 0).result(),
    };
    let prior = state.dataset.clone();
    let (candidate, rows, columns, appended, header_skipped, grid_name) = match prepare_grid_paste(
        &state.dataset,
        grid_index as usize,
        start_row as usize,
        start_column as usize,
        &clipboard,
    ) {
        Ok(plan) => plan,
        Err(error) => return error.result(),
    };
    state.dataset = candidate;
    state.undo.push(prior);
    if state.undo.len() > 50 {
        state.undo.remove(0);
    }
    state.redo.clear();
    state.invalidate_calculation();
    state.mark_revision_changed();
    let _ = update(&mut state.contract, UiAction::DatasetEdited);
    paste_success(rows, columns, appended, header_skipped, &grid_name)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_set_title(value: *const c_char) -> TcqtStatus {
    status((|| {
        let value = unsafe { input(value, "title") }?;
        if value.trim().is_empty() {
            return Err("title cannot be empty".into());
        }
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                dataset.title = Some(value);
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_set_component(index: u32, value: *const c_char) -> TcqtStatus {
    status((|| {
        let value = unsafe { input(value, "component name") }?;
        if value.trim().is_empty() {
            return Err("component names cannot be empty".into());
        }
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                dataset
                    .components
                    .get_mut(index as usize)
                    .ok_or("component index is out of range")?
                    .name = value;
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_add_phase(name: *const c_char) -> TcqtStatus {
    status((|| {
        let name = unsafe { input(name, "phase name") }?;
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                if name.trim().is_empty() || dataset.phases.iter().any(|phase| phase.name == name) {
                    return Err("phase names must be non-empty and unique".into());
                }
                let mut id = 1;
                while dataset.phases.iter().any(|phase| phase.id.0 == id) {
                    id += 1;
                }
                let phase = PhaseDefinition {
                    name,
                    id: StablePhaseId(id),
                    line: 0,
                };
                for grid in &mut dataset.grids {
                    let count = grid.compositions().len();
                    let fields = fields_mut(grid);
                    for property in &dataset.properties {
                        add_field(fields, count, &phase, property);
                    }
                }
                dataset.phases.push(phase);
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_remove_phase(id: u32) -> TcqtStatus {
    status((|| {
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                if !dataset.phases.iter().any(|phase| phase.id.0 == id) {
                    return Err("phase ID is out of range".into());
                }
                dataset.phases.retain(|phase| phase.id.0 != id);
                for grid in &mut dataset.grids {
                    fields_mut(grid).retain(|field| field.phase_id.0 != id);
                }
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_add_property(
    name: *const c_char,
    unit: *const c_char,
    required: bool,
) -> TcqtStatus {
    status((|| {
        let name = unsafe { input(name, "property name") }?;
        let unit = unsafe { input(unit, "property unit") }?;
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                if name.trim().is_empty()
                    || dataset
                        .properties
                        .iter()
                        .any(|property| property.name == name)
                {
                    return Err("property names must be non-empty and unique".into());
                }
                let property = PropertyDefinition {
                    name,
                    required,
                    unit,
                    line: 0,
                };
                for grid in &mut dataset.grids {
                    let count = grid.compositions().len();
                    let fields = fields_mut(grid);
                    for phase in &dataset.phases {
                        add_field(fields, count, phase, &property);
                    }
                }
                dataset.properties.push(property);
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_remove_property(ordinal: u32) -> TcqtStatus {
    status((|| {
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                let property = dataset
                    .properties
                    .get(ordinal as usize)
                    .cloned()
                    .ok_or("property index is out of range")?;
                if property.name == "T" {
                    return Err("required property T cannot be removed".into());
                }
                dataset.properties.remove(ordinal as usize);
                for grid in &mut dataset.grids {
                    fields_mut(grid).retain(|field| field.property != property.name);
                }
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_add_regular_grid(
    name: *const c_char,
    subdivisions: u32,
) -> TcqtStatus {
    status((|| {
        let name = unsafe { input(name, "grid name") }?;
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                validate_new_regular_grid_subdivisions(subdivisions as usize)?;
                if name.trim().is_empty() || dataset.grids.iter().any(|grid| grid.name() == name) {
                    return Err("grid names must be non-empty and unique".into());
                }
                let compositions = RegularTernaryGrid::new(subdivisions as usize)
                    .map_err(|error| error.to_string())?
                    .compositions()
                    .collect();
                let mut grid = TabulatedGrid::Regular(RegularTabulatedGrid {
                    name,
                    source: SourceRange {
                        first_line: 0,
                        last_line: 0,
                    },
                    subdivisions: subdivisions as usize,
                    order: RowOrder::Canonical,
                    composition_columns: CompositionColumns::None,
                    compositions,
                    fields: Vec::new(),
                });
                initialise_fields(dataset, &mut grid);
                dataset.grids.push(grid);
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_add_irregular_grid(name: *const c_char) -> TcqtStatus {
    status((|| {
        let name = unsafe { input(name, "grid name") }?;
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                if name.trim().is_empty() || dataset.grids.iter().any(|grid| grid.name() == name) {
                    return Err("grid names must be non-empty and unique".into());
                }
                let mut grid = TabulatedGrid::Irregular(IrregularTabulatedGrid {
                    name,
                    source: SourceRange {
                        first_line: 0,
                        last_line: 0,
                    },
                    compositions: Vec::new(),
                    fields: Vec::new(),
                });
                initialise_fields(dataset, &mut grid);
                dataset.grids.push(grid);
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_remove_grid(index: u32) -> TcqtStatus {
    status((|| {
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                if index as usize >= dataset.grids.len() {
                    return Err("grid index is out of range".into());
                }
                dataset.grids.remove(index as usize);
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_set_grid_cell(
    grid_index: u32,
    field_index: u32,
    row_index: u32,
    token: *const c_char,
) -> TcqtStatus {
    status((|| {
        let token = unsafe { input(token, "cell value") }?;
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                let value = parse_tabulated_value_token(&token, &dataset.missing_tokens, true)?;
                let grid = dataset
                    .grids
                    .get_mut(grid_index as usize)
                    .ok_or("grid index is out of range")?;
                let field = fields_mut(grid)
                    .get_mut(field_index as usize)
                    .ok_or("field index is out of range")?;
                for existing in &mut field.values {
                    existing.clear_if_extrapolated();
                }
                *field
                    .values
                    .get_mut(row_index as usize)
                    .ok_or("row index is out of range")? = value;
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_add_irregular_row(grid_index: u32) -> TcqtStatus {
    status((|| {
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                let grid = dataset
                    .grids
                    .get_mut(grid_index as usize)
                    .ok_or("grid index is out of range")?;
                let TabulatedGrid::Irregular(grid) = grid else {
                    return Err(
                        "regular-grid compositions are canonical and cannot be edited".into(),
                    );
                };
                grid.compositions.push([1.0, 0.0, 0.0]);
                for field in &mut grid.fields {
                    field.values.push(TabulatedValue::missing());
                    field.row_lines.push(0);
                }
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_undo() -> TcqtStatus {
    status((|| {
        let mut state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let prior = state.undo.pop().ok_or("nothing to undo")?;
        let current = std::mem::replace(&mut state.dataset, prior);
        state.redo.push(current);
        state.invalidate_calculation();
        state.mark_revision_changed();
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_redo() -> TcqtStatus {
    status((|| {
        let mut state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let next = state.redo.pop().ok_or("nothing to redo")?;
        let current = std::mem::replace(&mut state.dataset, next);
        state.undo.push(current);
        state.invalidate_calculation();
        state.mark_revision_changed();
        Ok(())
    })())
}
/// Calculate the exact projection selected by the Viewer. Trace output is
/// observation-only, but when configured it is attached to this accepted
/// calculation rather than a diagnostic-only repeat of it.
fn calculate_viewer_projection(
    dataset: &TabulatedTernaryDataset,
    options: &ProjectionOptions,
    trace_request: &NumericalTraceRequest,
    context: &NumericalTraceRunContext,
    reused_topology: Option<&StableBoundaryNetwork>,
    bootstrap_topology: bool,
) -> (Result<LiquidusProjection, String>, Option<String>) {
    let Some(destination) = trace_request.destination.as_ref() else {
        let result = if bootstrap_topology {
            calculate_stable_topology(dataset, options).and_then(|topology| {
                calculate_projection_reusing_stable_topology(dataset, options, &topology)
            })
        } else {
            match reused_topology {
                Some(topology) => {
                    calculate_projection_reusing_stable_topology(dataset, options, topology)
                }
                None => calculate_projection(dataset, options),
            }
        };
        return (result.map_err(|error| error.to_string()), None);
    };
    let mut sink = match JsonLinesTraceSink::create(destination, trace_request.config()) {
        Ok(sink) => sink,
        Err(error) => {
            let projection = if bootstrap_topology {
                calculate_stable_topology(dataset, options).and_then(|topology| {
                    calculate_projection_reusing_stable_topology(dataset, options, &topology)
                })
            } else {
                match reused_topology {
                    Some(topology) => {
                        calculate_projection_reusing_stable_topology(dataset, options, topology)
                    }
                    None => calculate_projection(dataset, options),
                }
            };
            return (
                projection.map_err(|error| error.to_string()),
                Some(format!("Numerical trace could not be created: {error}")),
            );
        }
    };
    let result = if bootstrap_topology {
        calculate_projection_with_automatic_bootstrap_with_trace_context(
            dataset, options, &mut sink, context,
        )
    } else {
        calculate_projection_with_trace_context_reusing_stable_topology(
            dataset,
            options,
            &mut sink,
            context,
            reused_topology,
        )
    }
    .map_err(|error| error.to_string());
    let output = sink.finish();
    let status = match (&result, output.first_error) {
        (Ok(_), None) => format!(
            "Numerical trace saved to {} ({} events)",
            output.path.display(),
            output.events_written
        ),
        (Ok(_), Some(error)) => {
            format!("Projection remains valid; numerical trace output failed: {error}")
        }
        (Err(error), _) => {
            format!(
                "Projection failed; numerical trace recorded the same calculation failure: {error}"
            )
        }
    };
    (result, Some(status))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_calculate_viewer(
    raw_options: *const TcqtViewerCalculationOptions,
    expected_revision: u64,
    expected_options_revision: u64,
    request_id: u64,
) -> TcqtCalculationResult {
    let failure =
        |message: String, dataset_revision: u64, options_revision: u64| TcqtCalculationResult {
            success: false,
            request_id,
            dataset_revision,
            options_revision,
            vertex_count: 0,
            message: bytes(&message),
        };
    let abi_options = match unsafe { out(raw_options.cast_mut(), "viewer calculation options") } {
        Ok(value) => *value,
        Err(error) => return failure(error, 0, 0),
    };
    let requested_options = match ViewerCalculationOptions::from_abi(&abi_options) {
        Ok(options) => options,
        Err(error) => return failure(error, 0, 0),
    };
    let projection_options = requested_options.projection_options();
    let (
        dataset,
        options_revision,
        trace_request,
        reuse_raw,
        reuse_regularized,
        can_reuse_topology,
    ) = match document().lock() {
        Ok(mut state) => {
            if state.revision != expected_revision {
                return failure(
                    "project changed before calculation started".to_owned(),
                    state.revision,
                    state.options_revision,
                );
            }
            if state.options_revision != expected_options_revision
                || state.viewer_options != requested_options
            {
                return failure(
                    "viewer settings changed before calculation started".to_owned(),
                    state.revision,
                    state.options_revision,
                );
            }
            if let Err(error) = state.dataset.validate_calculation_readiness() {
                return failure(
                    format!("Calculation unavailable: {error}"),
                    state.revision,
                    state.options_revision,
                );
            }
            state.calculation_generation = request_id;
            let can_reuse_topology =
                state.accepted_topology_key == Some(requested_options.topology_key());
            (
                state.dataset.clone(),
                state.options_revision,
                state.trace_request.clone(),
                can_reuse_topology
                    .then(|| state.raw_projection.clone())
                    .flatten(),
                can_reuse_topology
                    .then(|| state.regularized_projection.clone())
                    .flatten(),
                can_reuse_topology,
            )
        }
        Err(_) => return failure("project lock is unavailable".to_owned(), 0, 0),
    };

    // Calculate the selected display variant first. The request trace belongs
    // to that accepted projection; an optional sibling variant is useful for
    // Overlay but must never invalidate the selected result.
    let trace_context = NumericalTraceRunContext {
        input_identifier: dataset
            .source_path
            .as_ref()
            .map(|path| path.display().to_string()),
        dataset_revision: Some(expected_revision),
        options_revision: Some(options_revision),
        request_id: Some(request_id),
        ..NumericalTraceRunContext::default()
    };
    let selected_regularized = requested_options.regularize;
    let mut selected_options = projection_options.clone();
    selected_options.regularize = selected_regularized;
    let reusable_topology = if can_reuse_topology {
        reuse_regularized
            .as_ref()
            .map(|projection| &projection.stable_boundaries)
            .or_else(|| {
                reuse_raw
                    .as_ref()
                    .map(|projection| &projection.stable_boundaries)
            })
    } else {
        None
    };

    // Stage A is an independently accepted artifact. Build and commit it
    // before Stage B so a contour failure can never make an accepted stable
    // boundary network, its invariant temperatures, or its univariants vanish.
    let mut staged_topology = if reusable_topology.is_none() {
        match calculate_stable_topology_projection(&dataset, &selected_options) {
            Ok(projection) => Some(projection),
            Err(error) => {
                return failure(
                    format!("Stable topology calculation failed: {error}"),
                    expected_revision,
                    options_revision,
                );
            }
        }
    } else {
        None
    };
    let mut automatic_level_failure = None;
    if let Some(topology) = staged_topology.as_mut() {
        if requested_options.automatic_range && requested_options.explicit_levels.is_empty() {
            let step = selected_options.automatic_level_step.unwrap_or(100.0);
            match automatic_iso_range(
                &topology.stable_boundaries,
                topology.input_summary.temperature_range[0],
                topology.input_summary.temperature_range[1],
                step,
            ) {
                Ok(range) => match automatic_iso_levels(range.minimum, range.maximum, step) {
                    Ok(levels) => {
                        topology.levels = levels;
                        topology.automatic_iso_range = Some(range);
                    }
                    Err(error) => {
                        automatic_level_failure = Some(format!(
                            "automatic levels could not be materialized: {error}"
                        ));
                    }
                },
                Err(error) => {
                    automatic_level_failure =
                        Some(format!("automatic levels could not be derived: {error}"));
                }
            }
        } else {
            topology.levels = selected_options.levels.clone();
        }
        let topology_records = match projection_csv_records(
            &dataset,
            Some(topology),
            None,
            &RenderOptions::default(),
            ProjectionCsvOptions {
                layers: ProjectionCsvLayerFilter::AllCalculatedLayers,
                path_mode: RenderPathMode::Raw,
            },
        ) {
            Ok(records) => records,
            Err(error) => {
                return failure(
                    format!("Stable topology could not be transferred to the Viewer: {error}"),
                    expected_revision,
                    options_revision,
                );
            }
        };
        match document().lock() {
            Ok(mut state)
                if state.revision == expected_revision
                    && state.options_revision == expected_options_revision
                    && state.calculation_generation == request_id =>
            {
                state.projection = Some(topology.clone());
                state.raw_projection = Some(topology.clone());
                state.regularized_projection = None;
                state.projection_records = topology_records;
                state.accepted_projection_dataset = Some(dataset.clone());
                state.accepted_projection_dataset_revision = expected_revision;
                state.accepted_viewer_options = Some(requested_options.clone());
                state.projection_options_revision = expected_options_revision;
                state.projection_request_id = request_id;
                state.accepted_topology_key = Some(requested_options.topology_key());
                state.stable_topology_build_count =
                    state.stable_topology_build_count.saturating_add(1);
                state.last_stable_topology_reused = false;
            }
            Ok(state) => {
                return failure(
                    "stable topology result became stale and was discarded".to_owned(),
                    state.revision,
                    state.options_revision,
                );
            }
            Err(_) => return failure("project lock is unavailable".to_owned(), 0, 0),
        }
    }
    if let Some(error) = automatic_level_failure {
        return TcqtCalculationResult {
            success: true,
            request_id,
            dataset_revision: expected_revision,
            options_revision: expected_options_revision,
            vertex_count: dataset
                .grids
                .iter()
                .map(|grid| grid.compositions().len())
                .sum::<usize>() as u32,
            message: bytes(&format!(
                "Stable topology calculated; automatic isotherm range is unavailable: {error}"
            )),
        };
    }
    let selected_reuse = staged_topology
        .as_ref()
        .map(|projection| &projection.stable_boundaries)
        .or(reusable_topology);
    let topology_built_for_request = staged_topology.is_some();
    let reused_stable_topology = !topology_built_for_request && selected_reuse.is_some();
    let (selected_result, trace_status) = calculate_viewer_projection(
        &dataset,
        &selected_options,
        &trace_request,
        &trace_context,
        selected_reuse,
        false,
    );
    let mut selected_projection = match selected_result {
        Ok(projection) => projection,
        Err(error) if topology_built_for_request => {
            return TcqtCalculationResult {
                success: true,
                request_id,
                dataset_revision: expected_revision,
                options_revision: expected_options_revision,
                vertex_count: dataset
                    .grids
                    .iter()
                    .map(|grid| grid.compositions().len())
                    .sum::<usize>() as u32,
                message: bytes(&format!(
                    "Stable topology calculated; isotherm calculation incomplete: {error}"
                )),
            };
        }
        Err(error) => {
            return failure(
                format!(
                    "Calculation failed while preparing the selected {} paths: {error}",
                    if selected_regularized {
                        "regularized"
                    } else {
                        "raw"
                    }
                ),
                expected_revision,
                options_revision,
            );
        }
    };
    if topology_built_for_request {
        // The selected contour pass reused the topology produced by Stage A;
        // it is a build for lifecycle counters, not an isotherm-only reuse.
        selected_projection.diagnostics.stable_topology_reused = false;
    }

    let mut raw_options = projection_options.clone();
    raw_options.regularize = false;
    let mut regularized_options = projection_options.clone();
    regularized_options.regularize = true;
    let (raw_projection, regularized_projection, sibling_warning) = if selected_regularized {
        let raw = match selected_reuse {
            Some(topology) => {
                calculate_projection_reusing_stable_topology(&dataset, &raw_options, topology)
            }
            None => calculate_projection(&dataset, &raw_options),
        }
        .map_err(|error| error.to_string());
        let warning = raw
            .as_ref()
            .err()
            .map(|error| format!("Raw path variant unavailable: {error}"));
        (raw.ok(), Some(selected_projection.clone()), warning)
    } else {
        let regularized = match selected_reuse {
            Some(topology) => calculate_projection_reusing_stable_topology(
                &dataset,
                &regularized_options,
                topology,
            ),
            None => calculate_projection(&dataset, &regularized_options),
        }
        .map_err(|error| error.to_string());
        let warning = regularized
            .as_ref()
            .err()
            .map(|error| format!("Regularized path variant unavailable: {error}"));
        (Some(selected_projection.clone()), regularized.ok(), warning)
    };
    let records = match projection_csv_records(
        &dataset,
        regularized_projection.as_ref(),
        raw_projection.as_ref(),
        &RenderOptions::default(),
        ProjectionCsvOptions {
            layers: ProjectionCsvLayerFilter::AllCalculatedLayers,
            path_mode: RenderPathMode::Overlay,
        },
    ) {
        Ok(records) => records,
        Err(error) => {
            return failure(
                format!(
                    "Calculation produced projection geometry that could not be transferred: {error}"
                ),
                expected_revision,
                options_revision,
            );
        }
    };
    let vertex_count = dataset
        .grids
        .iter()
        .map(|grid| grid.compositions().len())
        .sum::<usize>() as u32;
    let binary_invariants = selected_projection
        .stable_boundaries
        .nodes
        .iter()
        .filter(|node| matches!(node, StableInvariantNode::Binary(_)))
        .count();
    let interior_invariants = selected_projection
        .stable_boundaries
        .nodes
        .iter()
        .filter(|node| matches!(node, StableInvariantNode::Interior(_)))
        .count();
    let mut summary = format!(
        "Calculated {binary_invariants} binary invariants, {interior_invariants} ternary invariants, {} univariant paths, and {} isotherm paths",
        selected_projection.diagnostics.univariant_count,
        selected_projection.diagnostics.contour_path_count,
    );
    if selected_projection
        .diagnostics
        .domain_truncated_univariant_count
        != 0
    {
        summary.push_str(&format!(
            "; {} domain-truncated branch{} retained as diagnostics",
            selected_projection
                .diagnostics
                .domain_truncated_univariant_count,
            if selected_projection
                .diagnostics
                .domain_truncated_univariant_count
                == 1
            {
                ""
            } else {
                "es"
            }
        ));
    }
    if let Some(warning) = sibling_warning {
        summary.push_str(". ");
        summary.push_str(&warning);
    }
    if let Some(trace_status) = trace_status {
        summary.push_str(". ");
        summary.push_str(&trace_status);
    }
    match document().lock() {
        Ok(mut state)
            if state.revision == expected_revision
                && state.options_revision == expected_options_revision
                && state.calculation_generation == request_id =>
        {
            state.projection = Some(selected_projection);
            state.raw_projection = raw_projection;
            state.regularized_projection = regularized_projection;
            state.projection_records = records;
            state.accepted_projection_dataset = Some(dataset.clone());
            state.accepted_projection_dataset_revision = expected_revision;
            state.accepted_viewer_options = Some(requested_options.clone());
            state.projection_options_revision = expected_options_revision;
            state.projection_request_id = request_id;
            state.accepted_topology_key = Some(requested_options.topology_key());
            state.isotherm_rebuild_count = state.isotherm_rebuild_count.saturating_add(1);
            if reused_stable_topology {
                state.stable_topology_reuse_count =
                    state.stable_topology_reuse_count.saturating_add(1);
            } else if !topology_built_for_request {
                state.stable_topology_build_count =
                    state.stable_topology_build_count.saturating_add(1);
            }
            state.last_stable_topology_reused = reused_stable_topology;
            TcqtCalculationResult {
                success: true,
                request_id,
                dataset_revision: expected_revision,
                options_revision: expected_options_revision,
                vertex_count,
                message: bytes(&summary),
            }
        }
        Ok(state) => failure(
            "calculation result became stale and was discarded".to_owned(),
            state.revision,
            state.options_revision,
        ),
        Err(_) => failure("project lock is unavailable".to_owned(), 0, 0),
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_projection_summary(
    output_value: *mut TcqtProjectionSummary,
) -> TcqtStatus {
    status((|| {
        let output_value = unsafe { out(output_value, "projection summary") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let effective = state.viewer_options.to_abi();
        let Some(projection) = state.projection.as_ref() else {
            *output_value = TcqtProjectionSummary {
                available: false,
                source_minimum: 0.0,
                source_maximum: 0.0,
                automatic_minimum: 0.0,
                automatic_used_invariant: false,
                level_count: 0,
                invariant_count: 0,
                binary_invariant_count: 0,
                interior_invariant_count: 0,
                univariant_count: 0,
                contour_path_count: 0,
                contour_transfer_junction_count: 0,
                contour_one_sided_contact_count: 0,
                contour_invariant_level_coincidence_count: 0,
                contour_degenerate_event_count: 0,
                contour_levels_attempted: 0,
                contour_levels_completed: 0,
                contour_levels_failed: 0,
                maximum_contour_level_residual: 0.0,
                effective_automatic_range: effective.automatic_range,
                effective_minimum: effective.minimum,
                effective_maximum: effective.maximum,
                effective_level_step: effective.level_step,
                effective_sampling_subdivisions: effective.sampling_subdivisions,
                effective_regularize: effective.regularize,
                effective_regularization_spacing: effective.regularization_spacing,
                effective_source_interpolation: effective.source_interpolation,
                effective_cubic_method: effective.cubic_method,
                effective_partial_domain_policy: effective.partial_domain_policy,
                effective_continuation: effective.continuation,
                dataset_revision: state.revision,
                options_revision: state.options_revision,
                request_id: state.projection_request_id,
                raw_projection_available: state.raw_projection.is_some(),
                regularized_projection_available: state.regularized_projection.is_some(),
                selected_projection_regularized: effective.regularize,
                domain_truncated_univariant_count: 0,
                regularization_failure_count: 0,
                stable_topology_build_count: state.stable_topology_build_count,
                stable_topology_reuse_count: state.stable_topology_reuse_count,
                isotherm_rebuild_count: state.isotherm_rebuild_count,
                stable_topology_reused: state.last_stable_topology_reused,
                message: bytes("No projection has been calculated."),
            };
            return Ok(());
        };
        let effective = state
            .accepted_viewer_options
            .as_ref()
            .unwrap_or(&state.viewer_options)
            .to_abi();
        let automatic = projection.automatic_iso_range;
        let binary_invariant_count = projection
            .stable_boundaries
            .nodes
            .iter()
            .filter(|node| matches!(node, StableInvariantNode::Binary(_)))
            .count() as u32;
        let interior_invariant_count = projection
            .stable_boundaries
            .nodes
            .iter()
            .filter(|node| matches!(node, StableInvariantNode::Interior(_)))
            .count() as u32;
        let (effective_minimum, effective_maximum) = automatic
            .map_or((effective.minimum, effective.maximum), |range| {
                (range.minimum, range.maximum)
            });
        *output_value = TcqtProjectionSummary {
            available: true,
            source_minimum: projection.input_summary.temperature_range[0],
            source_maximum: projection.input_summary.temperature_range[1],
            automatic_minimum: automatic
                .map_or(projection.input_summary.temperature_range[0], |range| {
                    range.minimum
                }),
            automatic_used_invariant: automatic.is_some_and(|range| range.used_invariant_minimum),
            level_count: projection.levels.len() as u32,
            invariant_count: projection.diagnostics.invariant_count as u32,
            binary_invariant_count,
            interior_invariant_count,
            univariant_count: projection.diagnostics.univariant_count as u32,
            contour_path_count: projection.diagnostics.contour_path_count as u32,
            contour_transfer_junction_count: projection.diagnostics.contour_transfer_junction_count
                as u32,
            contour_one_sided_contact_count: projection.diagnostics.contour_one_sided_contact_count
                as u32,
            contour_invariant_level_coincidence_count: projection
                .diagnostics
                .contour_invariant_level_coincidence_count
                as u32,
            contour_degenerate_event_count: projection.diagnostics.contour_degenerate_event_count
                as u32,
            contour_levels_attempted: projection.diagnostics.contour_levels_attempted as u32,
            contour_levels_completed: projection.diagnostics.contour_levels_completed as u32,
            contour_levels_failed: projection.diagnostics.contour_levels_failed as u32,
            maximum_contour_level_residual: projection.diagnostics.maximum_contour_level_residual,
            effective_automatic_range: automatic.is_some(),
            effective_minimum,
            effective_maximum,
            effective_level_step: effective.level_step,
            effective_sampling_subdivisions: projection.diagnostics.sampling_subdivisions as u32,
            effective_regularize: effective.regularize,
            effective_regularization_spacing: effective.regularization_spacing,
            effective_source_interpolation: effective.source_interpolation,
            effective_cubic_method: effective.cubic_method,
            effective_partial_domain_policy: effective.partial_domain_policy,
            effective_continuation: effective.continuation,
            dataset_revision: state.accepted_projection_dataset_revision,
            options_revision: state.projection_options_revision,
            request_id: state.projection_request_id,
            raw_projection_available: state.raw_projection.is_some(),
            regularized_projection_available: state.regularized_projection.is_some(),
            selected_projection_regularized: effective.regularize,
            domain_truncated_univariant_count: projection
                .diagnostics
                .domain_truncated_univariant_count
                as u32,
            regularization_failure_count: projection.diagnostics.regularization_failure_count
                as u32,
            stable_topology_build_count: state.stable_topology_build_count,
            stable_topology_reuse_count: state.stable_topology_reuse_count,
            isotherm_rebuild_count: state.isotherm_rebuild_count,
            stable_topology_reused: state.last_stable_topology_reused,
            message: bytes(
                if projection.diagnostics.regularization_failure_count == 0 {
                    "Projection is current."
                } else {
                    "Raw topology is current; one or more univariants could not be regularized."
                },
            ),
        };
        Ok(())
    })())
}
fn format_invariant_phases(dataset: &TabulatedTernaryDataset, phases: &[StablePhaseId]) -> String {
    let mut ordered = phases.to_vec();
    ordered.sort_unstable();
    ordered
        .iter()
        .map(|id| {
            let name = dataset
                .phases
                .iter()
                .find(|phase| phase.id == *id)
                .map(|phase| phase.name.as_str())
                .unwrap_or("unknown");
            format!("[{}] {}", id.0, name)
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

fn invariant_point(
    dataset: &TabulatedTernaryDataset,
    projection: &LiquidusProjection,
    node: &StableInvariantNode,
    dataset_revision: u64,
    options_revision: u64,
    request_id: u64,
) -> TcqtInvariantPoint {
    let [a, b, c] = node.point().as_array();
    let degree = projection
        .stable_boundaries
        .incident_univariants(node.id())
        .map_or(0, |paths| paths.len()) as u32;
    let (kind, boundary, boundary_parameter, boundary_name, phases) = match node {
        StableInvariantNode::Binary(node) => {
            let (boundary, name) = match node.boundary {
                BinaryBoundary::Ab => (0, "AB"),
                BinaryBoundary::Bc => (1, "BC"),
                BinaryBoundary::Ca => (2, "CA"),
            };
            (
                0,
                boundary,
                node.boundary_parameter,
                name,
                node.phases.as_slice(),
            )
        }
        StableInvariantNode::Interior(node) => (1, 0, 0.0, "", node.phases.as_slice()),
    };
    TcqtInvariantPoint {
        id: node.id().0 as u32,
        kind,
        a,
        b,
        c,
        temperature: node.temperature(),
        boundary,
        boundary_parameter,
        incident_univariant_count: degree,
        phases: bytes(&format_invariant_phases(dataset, phases)),
        boundary_name: bytes(boundary_name),
        dataset_revision,
        options_revision,
        request_id,
    }
}

fn accepted_invariant_nodes(projection: &LiquidusProjection) -> Vec<&StableInvariantNode> {
    let mut nodes = projection
        .stable_boundaries
        .nodes
        .iter()
        .collect::<Vec<_>>();
    nodes.sort_by_key(|node| node.id());
    nodes
}

/// Return the number of invariant nodes in the currently accepted projection.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_invariant_point_count(output_value: *mut u32) -> TcqtStatus {
    status((|| {
        let output_value = unsafe { out(output_value, "invariant point count") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        *output_value = state.projection.as_ref().map_or(0, |projection| {
            projection.stable_boundaries.nodes.len() as u32
        });
        Ok(())
    })())
}

/// Return one invariant node from the currently accepted projection in stable ID order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_invariant_point_at(
    index: u32,
    output_value: *mut TcqtInvariantPoint,
) -> TcqtStatus {
    status((|| {
        let output_value = unsafe { out(output_value, "invariant point") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let projection = state
            .projection
            .as_ref()
            .ok_or_else(|| "no accepted projection is available".to_owned())?;
        let nodes = accepted_invariant_nodes(projection);
        let node = nodes
            .get(index as usize)
            .ok_or_else(|| "invariant point index is outside the accepted projection".to_owned())?;
        *output_value = invariant_point(
            state
                .accepted_projection_dataset
                .as_ref()
                .unwrap_or(&state.dataset),
            projection,
            node,
            state.accepted_projection_dataset_revision,
            state.projection_options_revision,
            state.projection_request_id,
        );
        Ok(())
    })())
}

/// Validate a finite, non-negative coordinate triplet without normalizing or
/// changing a document. The dialog uses this on ordinary focus loss.
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_validate_coordinate_triplet(a: f64, b: f64, c: f64) -> TcqtStatus {
    status(
        normalize_ternary_triplet([a, b, c])
            .map(|_| ())
            .map_err(|error| error.to_string()),
    )
}
/// Normalize a semantic global triplet and locate its deterministic source triangle.
/// This operation is field-independent and never changes document state.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_locate_grid_point(
    grid_index: u32,
    a: f64,
    b: f64,
    c: f64,
    output_value: *mut TcqtLocatedPoint,
) -> TcqtStatus {
    status((|| {
        let output_value = unsafe { out(output_value, "located point") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let grid = state
            .dataset
            .grids
            .get(grid_index as usize)
            .ok_or_else(|| "selected grid is unavailable".to_owned())?;
        *output_value = locate_tabulated_grid_point(grid, [a, b, c])?;
        Ok(())
    })())
}

/// Normalize triangle-local barycentric coordinates, convert to global A/B/C,
/// then return the deterministic triangle owner for the resulting composition.
/// This operation is field-independent and never changes document state.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_locate_grid_local_point(
    grid_index: u32,
    triangle_index: u32,
    lambda0: f64,
    lambda1: f64,
    lambda2: f64,
    output_value: *mut TcqtLocatedPoint,
) -> TcqtStatus {
    status((|| {
        let output_value = unsafe { out(output_value, "located point") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let grid = state
            .dataset
            .grids
            .get(grid_index as usize)
            .ok_or_else(|| "selected grid is unavailable".to_owned())?;
        *output_value =
            locate_tabulated_grid_local_point(grid, triangle_index, [lambda0, lambda1, lambda2])?;
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_evaluate_field_current(
    grid_index: u32,
    phase_id: u32,
    property: *const c_char,
    expected_options_revision: u64,
    a: f64,
    b: f64,
    c: f64,
    query_index: u64,
    output_value: *mut TcqtInspectionResult,
) -> TcqtStatus {
    status((|| {
        let property = unsafe { input(property, "field property") }?;
        if [a, b, c]
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
            || (a + b + c - 1.0).abs() > 1.0e-8
        {
            return Err("query composition must be finite, non-negative, and sum to one".into());
        }
        let output_value = unsafe { out(output_value, "inspection result") }?;
        let mut state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        if state.options_revision != expected_options_revision {
            return Err("viewer interpolation settings changed; refresh the query snapshot".into());
        }
        let dataset = state.dataset.clone();
        let options_revision = state.options_revision;
        let effective_options = state.viewer_options.to_abi();
        let options = state.viewer_options.projection_options();
        let identity = InspectionFieldIdentity {
            grid_index: grid_index as usize,
            phase_id: StablePhaseId(phase_id),
            property,
        };
        let result = state.inspection_cache.evaluate(
            &dataset,
            &identity,
            &options,
            [a, b, c],
            query_index as usize,
            query_index,
        );
        let lambdas = result.local_barycentric.unwrap_or([0.0; 3]);
        let rows = result.triangle_vertex_indices.unwrap_or([0; 3]);
        *output_value = TcqtInspectionResult {
            success: !matches!(result.state, InterpolatedResultState::Error(_)),
            state: inspection_state(&result.state),
            has_value: result.value.is_some(),
            value: result.value.unwrap_or(0.0),
            a,
            b,
            c,
            triangle_index: result.triangle_index.map_or(u32::MAX, |index| index as u32),
            has_local_barycentric: result.local_barycentric.is_some(),
            lambda0: lambdas[0],
            lambda1: lambdas[1],
            lambda2: lambdas[2],
            has_contributions: result.linear_part.is_some() && result.excess_part.is_some(),
            linear_part: result.linear_part.unwrap_or(0.0),
            excess_part: result.excess_part.unwrap_or(0.0),
            has_source_rows: result.triangle_vertex_indices.is_some(),
            source_row0: rows[0] as u32,
            source_row1: rows[1] as u32,
            source_row2: rows[2] as u32,
            local_mode: local_mode(result.local_mode),
            uses_extrapolated_sources: result.source_provenance.uses_extrapolated_values,
            maximum_extrapolation_layer: result
                .source_provenance
                .maximum_extrapolation_layer
                .map_or(0, u32::from),
            extrapolation_methods: bytes(
                &result
                    .source_provenance
                    .extrapolation_methods
                    .iter()
                    .map(|method| format!("{method:?}"))
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            extrapolated_source_row_count: result.source_provenance.extrapolated_source_rows.len()
                as u32,
            options_revision,
            effective_source_interpolation: effective_options.source_interpolation,
            effective_cubic_method: effective_options.cubic_method,
            effective_partial_domain_policy: effective_options.partial_domain_policy,
            effective_continuation: effective_options.continuation,
            unit: bytes(&result.unit),
            message: bytes(match &result.state {
                InterpolatedResultState::Error(error) => error,
                state => state.label(),
            }),
        };
        Ok(())
    })())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_set_field_vertex(
    grid_index: u32,
    phase_id: u32,
    property: *const c_char,
    row_index: u32,
    token: *const c_char,
) -> TcqtStatus {
    status((|| {
        let property = unsafe { input(property, "field property") }?;
        let token = unsafe { input(token, "vertex value") }?;
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                let value = parse_tabulated_value_token(&token, &dataset.missing_tokens, false)?;
                let grid = dataset
                    .grids
                    .get_mut(grid_index as usize)
                    .ok_or("grid index is out of range")?;
                let field = fields_mut(grid)
                    .iter_mut()
                    .find(|field| field.phase_id.0 == phase_id && field.property == property)
                    .ok_or("selected phase/property field no longer exists")?;
                for existing in &mut field.values {
                    existing.clear_if_extrapolated();
                }
                *field
                    .values
                    .get_mut(row_index as usize)
                    .ok_or("vertex row is out of range")? = value;
                Ok(())
            })
    })())
}

fn state_value(code: u32) -> Result<TabulatedValueState, String> {
    match code {
        1 => Ok(TabulatedValueState::Missing),
        2 => Ok(TabulatedValueState::CutOff),
        3 => Ok(TabulatedValueState::Missing),
        _ => Err("bulk edits may set only Missing or Cut-off".into()),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_bulk_set_field_state(
    grid_index: u32,
    phase_id: u32,
    property: *const c_char,
    rows: *const u32,
    row_count: u32,
    state_code: u32,
) -> TcqtStatus {
    status((|| {
        let property = unsafe { input(property, "field property") }?;
        let state_value = state_value(state_code)?;
        if rows.is_null() && row_count != 0 {
            return Err("selected vertex rows are unavailable".into());
        }
        // SAFETY: the C++ caller supplies row_count contiguous u32 entries.
        let rows = unsafe { std::slice::from_raw_parts(rows, row_count as usize) };
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                let grid = dataset
                    .grids
                    .get_mut(grid_index as usize)
                    .ok_or("grid index is out of range")?;
                let field = fields_mut(grid)
                    .iter_mut()
                    .find(|field| field.phase_id.0 == phase_id && field.property == property)
                    .ok_or("selected phase/property field no longer exists")?;
                if rows.iter().any(|row| *row as usize >= field.values.len()) {
                    return Err("a selected vertex row is out of range".into());
                }
                for existing in &mut field.values {
                    existing.clear_if_extrapolated();
                }
                for row in rows {
                    field.values[*row as usize] = TabulatedValue {
                        state: state_value,
                        value: None,
                        extrapolation: None,
                        note: None,
                    };
                }
                Ok(())
            })
    })())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_clear_field_notes(
    grid_index: u32,
    phase_id: u32,
    property: *const c_char,
    rows: *const u32,
    row_count: u32,
) -> TcqtStatus {
    status((|| {
        let property = unsafe { input(property, "field property") }?;
        if rows.is_null() && row_count != 0 {
            return Err("selected vertex rows are unavailable".into());
        }
        // SAFETY: the C++ caller supplies row_count contiguous u32 entries.
        let rows = unsafe { std::slice::from_raw_parts(rows, row_count as usize) };
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                let grid = dataset
                    .grids
                    .get_mut(grid_index as usize)
                    .ok_or("grid index is out of range")?;
                let field = fields_mut(grid)
                    .iter_mut()
                    .find(|field| field.phase_id.0 == phase_id && field.property == property)
                    .ok_or("selected phase/property field no longer exists")?;
                if rows.iter().any(|row| *row as usize >= field.values.len()) {
                    return Err("a selected vertex row is out of range".into());
                }
                for row in rows {
                    field.values[*row as usize].note = None;
                }
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_projection_record_count(output_value: *mut u32) -> TcqtStatus {
    status((|| {
        let output_value = unsafe { out(output_value, "projection record count") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        *output_value = state.projection_records.len() as u32;
        Ok(())
    })())
}

fn projection_paint_style(line_type: u32, path_source: u32) -> (u32, f64, u32) {
    // AARRGGBB values. Scene styling and raw/regularized provenance stay in
    // Rust; Qt executes the supplied scene instructions only.
    let alpha = if path_source == 0 {
        0xa0_00_00_00
    } else {
        0xff_00_00_00
    };
    let (rgb, width, marker_kind) = match line_type {
        0 => (0x002D_6EB4, 1.0, 0),
        1 => (0x0000_0000, 1.5, 0),
        2 | 3 => (0x00D0_2020, 1.4, 2),
        _ => (0x006A_6A6A, 1.2, 0),
    };
    (
        alpha | rgb,
        if path_source == 0 { width * 0.8 } else { width },
        marker_kind,
    )
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_projection_record_at(
    index: u32,
    output_value: *mut TcqtProjectionRecord,
) -> TcqtStatus {
    status((|| {
        let output_value = unsafe { out(output_value, "projection record") }?;
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let record = state
            .projection_records
            .get(index as usize)
            .ok_or("projection record index is out of range")?;
        let line_type = match record.line_type {
            ternary_contours_cli::ProjectionLineType::StableIsotherm => 0,
            ternary_contours_cli::ProjectionLineType::StableUnivariant => 1,
            ternary_contours_cli::ProjectionLineType::BinaryInvariant => 2,
            ternary_contours_cli::ProjectionLineType::InteriorInvariant => 3,
            ternary_contours_cli::ProjectionLineType::StableBoundaryContact => 4,
        };
        let path_source = match record.path_source {
            ProjectionPathSource::Raw => 0,
            ProjectionPathSource::Regularized => 1,
        };
        let (rgba, stroke_width, marker_kind) = projection_paint_style(line_type, path_source);
        *output_value = TcqtProjectionRecord {
            a: record.composition[0],
            b: record.composition[1],
            c: record.composition[2],
            point_index: record.point_index as u32,
            line_type,
            rgba,
            stroke_width,
            marker_kind,
            path_source,
            has_level: record.temperature.is_some(),
            level: record.temperature.unwrap_or_default(),
            unit: bytes(if line_type == 0 { "\u{00B0}C" } else { "" }),
            phase_1: bytes(record.phase_1.as_deref().unwrap_or_default()),
            phase_2: bytes(record.phase_2.as_deref().unwrap_or_default()),
            line_id: bytes(&record.line_id),
        };
        Ok(())
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_export_plot(path: *const c_char, format: u32) -> TcqtStatus {
    status((|| {
        let path = PathBuf::from(unsafe { input(path, "export path") }?);
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let projection = state
            .projection
            .as_ref()
            .ok_or("calculate a projection before exporting it")?;
        let format = match format {
            0 => OutputFormat::Png,
            1 => OutputFormat::Svg,
            _ => return Err("unsupported plot export format".into()),
        };
        let dataset = state
            .accepted_projection_dataset
            .as_ref()
            .ok_or("calculate a projection before exporting it")?;
        render_to_path(
            &path,
            dataset,
            projection,
            &RenderOptions::default(),
            Some(format),
        )
        .map_err(|error| error.to_string())
    })())
}

fn write_projection_csv_atomically(path: &std::path::Path, contents: &str) -> Result<(), String> {
    let directory = path
        .parent()
        .filter(|directory| !directory.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or("export path must name a file")?;
    let temporary = directory.join(format!(
        ".{name}.ternary-contours-export-{}.tmp",
        std::process::id()
    ));
    let result = (|| -> Result<(), String> {
        let mut file = fs::File::create(&temporary)
            .map_err(|error| format!("could not create temporary CSV: {error}"))?;
        file.write_all(contents.as_bytes())
            .map_err(|error| format!("could not write temporary CSV: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("could not finalize temporary CSV: {error}"))?;
        fs::rename(&temporary, path)
            .map_err(|error| format!("could not replace CSV atomically: {error}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn selected_export_projection(
    state: &ProjectDocument,
    path_display_mode: u32,
) -> Result<&LiquidusProjection, String> {
    match path_display_mode {
        // The currently selected primary geometry is regularized when it is
        // available; paths which could not be regularized are already present
        // as RawFallback in that selected projection.
        0 => state.raw_projection.as_ref().or(state.projection.as_ref()),
        1 | 2 => state.projection.as_ref(),
        _ => return Err("unsupported projection display mode for CSV export".into()),
    }
    .ok_or_else(|| "calculate a projection before exporting it".into())
}

/// Export one immutable, accepted Viewer projection snapshot as seven-column
/// thermodynamic geometry.  Qt supplies only dialog choices; this bridge owns
/// component metadata, stable topology, ordering, and data precision.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_export_projection_csv(
    path: *const c_char,
    options: *const TcqtProjectionCsvExportOptions,
) -> TcqtStatus {
    status((|| {
        let path = PathBuf::from(unsafe { input(path, "export path") }?);
        let options = unsafe { options.as_ref().ok_or("CSV export options are required")? };
        let selection = ProjectionGeometryCsvSelection {
            invariants: options.invariants,
            univariants: options.univariants,
            isotherms: options.isotherms,
        };
        if !selection.any() {
            return Err("select at least one projection geometry category".into());
        }
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        if state.accepted_projection_dataset_revision != options.expected_dataset_revision
            || state.projection_options_revision != options.expected_options_revision
            || state.projection_request_id != options.expected_request_id
        {
            return Err("the accepted projection changed; reopen the export dialog".into());
        }
        let dataset = state
            .accepted_projection_dataset
            .as_ref()
            .ok_or("calculate a projection before exporting it")?;
        let projection = selected_export_projection(&state, options.path_display_mode)?;
        let contents = serialize_projection_geometry_csv(dataset, projection, selection)
            .map_err(|error| error.to_string())?;
        write_projection_csv_atomically(&path, &contents)
    })())
}

/// Legacy ABI retained for older clients.  New Qt code always captures a
/// revision-pinned selection through [`tcqt_export_projection_csv`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_export_lines_csv(path: *const c_char) -> TcqtStatus {
    status((|| {
        let path = PathBuf::from(unsafe { input(path, "export path") }?);
        let state = document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?;
        let dataset = state
            .accepted_projection_dataset
            .as_ref()
            .ok_or("calculate a projection before exporting it")?;
        let projection = state
            .projection
            .as_ref()
            .ok_or("calculate a projection before exporting it")?;
        let contents = serialize_projection_geometry_csv(
            dataset,
            projection,
            ProjectionGeometryCsvSelection::default(),
        )
        .map_err(|error| error.to_string())?;
        write_projection_csv_atomically(&path, &contents)
    })())
}
/// Original feasibility entrypoint retained for existing callers. The application
/// uses the configurable `tcqt_calculate_viewer` entrypoint against the active dataset.
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_run_feasibility_calculation(
    subdivisions: u32,
    dataset_revision: u64,
) -> TcqtCalculationResult {
    let mut state = GuiContractState::default();
    state.revisions.dataset = Revision(dataset_revision);
    let request_id = update(&mut state, UiAction::RecalculateRequested)
        .iter()
        .find_map(|effect| match effect {
            UiEffect::RecalculateProjection { request, .. } => Some(request.0),
            _ => None,
        })
        .unwrap_or_default();
    match RegularTernaryGrid::new(subdivisions as usize) {
        Ok(grid) => TcqtCalculationResult {
            success: true,
            request_id,
            dataset_revision,
            options_revision: 0,
            vertex_count: grid.vertex_count() as u32,
            message: bytes(&format!(
                "Rust grid ready: {} canonical vertices",
                grid.vertex_count()
            )),
        },
        Err(error) => TcqtCalculationResult {
            success: false,
            request_id,
            dataset_revision,
            options_revision: 0,
            vertex_count: 0,
            message: bytes(&format!("Rust grid rejected: {error}")),
        },
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn tcqt_set_irregular_composition(
    grid_index: u32,
    row_index: u32,
    a: f64,
    b: f64,
    c: f64,
) -> TcqtStatus {
    status((|| {
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                if ![a, b, c].into_iter().all(f64::is_finite)
                    || [a, b, c].into_iter().any(|value| value < 0.0)
                    || (a + b + c - 1.0).abs() > 1e-8
                {
                    return Err(
                        "irregular compositions must be finite, non-negative, and sum to one"
                            .into(),
                    );
                }
                let grid = dataset
                    .grids
                    .get_mut(grid_index as usize)
                    .ok_or("grid index is out of range")?;
                let TabulatedGrid::Irregular(grid) = grid else {
                    return Err(
                        "regular-grid compositions are canonical and cannot be edited".into(),
                    );
                };
                if row_index as usize >= grid.compositions.len() {
                    return Err("row index is out of range".into());
                }
                if grid.compositions.iter().enumerate().any(|(index, point)| {
                    index != row_index as usize
                        && point
                            .iter()
                            .zip([a, b, c])
                            .map(|(left, right)| (left - right).abs())
                            .fold(0.0, f64::max)
                            <= 1e-10
                }) {
                    return Err("irregular grid has a duplicate composition".into());
                }
                grid.compositions[row_index as usize] = [a, b, c];
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcqt_rename_grid(index: u32, name: *const c_char) -> TcqtStatus {
    status((|| {
        let name = unsafe { input(name, "grid name") }?;
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                if name.trim().is_empty()
                    || dataset
                        .grids
                        .iter()
                        .enumerate()
                        .any(|(other, grid)| other != index as usize && grid.name() == name)
                {
                    return Err("grid names must be non-empty and unique".into());
                }
                match dataset
                    .grids
                    .get_mut(index as usize)
                    .ok_or("grid index is out of range")?
                {
                    TabulatedGrid::Regular(grid) => grid.name = name,
                    TabulatedGrid::Irregular(grid) => grid.name = name,
                }
                Ok(())
            })
    })())
}
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_duplicate_grid(index: u32) -> TcqtStatus {
    status((|| {
        document()
            .lock()
            .map_err(|_| "project lock is unavailable".to_owned())?
            .mutate(|dataset| {
                let mut duplicate = dataset
                    .grids
                    .get(index as usize)
                    .cloned()
                    .ok_or("grid index is out of range")?;
                let base = duplicate.name().to_owned();
                let mut number = 2;
                let mut name = format!("{base} {number}");
                while dataset.grids.iter().any(|grid| grid.name() == name) {
                    number += 1;
                    name = format!("{base} {number}");
                }
                match &mut duplicate {
                    TabulatedGrid::Regular(grid) => grid.name = name,
                    TabulatedGrid::Irregular(grid) => grid.name = name,
                }
                dataset.grids.push(duplicate);
                Ok(())
            })
    })())
}
#[cfg(test)]
mod tests {
    use super::*;

    fn test_document_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    #[test]
    fn new_document_has_empty_phase_and_grid_collections() {
        let state = ProjectDocument::new();
        assert_eq!(
            state.dataset.components.map(|component| component.name),
            ["A", "B", "C"]
        );
        assert!(state.dataset.phases.is_empty());
        assert!(state.dataset.grids.is_empty());
        assert_eq!(state.dataset.properties.len(), 1);
        assert_eq!(state.dataset.properties[0].name, "T");
        assert!(state.dataset.properties[0].required);
        assert_eq!(state.dataset.properties[0].unit, "C");
    }

    #[test]
    fn draft_save_and_reopen_round_trips_transactionally() {
        let _guard = test_document_lock().lock().unwrap();
        let path =
            std::env::temp_dir().join(format!("ternary-contours-draft-{}.tct", std::process::id()));
        let encoded = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap();
        unsafe {
            assert!(tcqt_new_document().success);
            let saved = tcqt_save_document(encoded.as_ptr());
            assert_eq!(saved.outcome, 0);
            assert!(path.exists());
            let mut summary = std::mem::zeroed();
            assert!(tcqt_project_summary(&mut summary).success);
            assert_eq!(summary.phase_count, 0);
            assert_eq!(summary.grid_count, 0);
            assert!(!summary.dirty);
            assert!(tcqt_open_document(encoded.as_ptr()).success);
            let mut reopened = std::mem::zeroed();
            assert!(tcqt_project_summary(&mut reopened).success);
            assert_eq!(reopened.phase_count, 0);
            assert_eq!(reopened.grid_count, 0);
            assert!(!reopened.dirty);
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn bulk_paste_handles_headers_states_and_is_transactional() {
        let dataset = ternary_contours_cli::default_regular_dataset();
        let pasted = prepare_grid_paste(
            &dataset,
            0,
            0,
            3,
            "Phase1.T\tPhase2.T\r\n1200\t1180\r\n1210\tNE\r\n",
        )
        .unwrap();
        assert_eq!(pasted.1, 2);
        assert_eq!(pasted.2, 2);
        assert!(pasted.4);
        let fields = pasted.0.grids[0].fields();
        assert_eq!(fields[0].values[0].state, TabulatedValueState::Calculated);
        assert_eq!(fields[1].values[1].state, TabulatedValueState::Missing);

        let failure = prepare_grid_paste(&dataset, 0, 0, 3, "1200\tNO").unwrap_err();
        assert!(failure.message.contains("finite number, NA, or CO"));
        assert_eq!(
            dataset.grids[0].fields()[0].values[0].state,
            TabulatedValueState::Missing
        );
        assert!(
            prepare_grid_paste(&dataset, 0, 0, 3, "1200\n1210\t1220")
                .unwrap_err()
                .message
                .contains("wrong row width")
        );
    }

    #[test]
    fn bulk_paste_rejects_regular_compositions_and_appends_irregular_rows() {
        let dataset = ternary_contours_cli::default_regular_dataset();
        let regular_error = prepare_grid_paste(&dataset, 0, 0, 0, "A\tB\n0\t1").unwrap_err();
        assert!(regular_error.message.contains("composition columns"));

        let mut irregular = empty_project_dataset();
        irregular.phases.push(PhaseDefinition {
            name: "Phase1".into(),
            id: StablePhaseId(1),
            line: 0,
        });
        irregular
            .grids
            .push(TabulatedGrid::Irregular(IrregularTabulatedGrid {
                name: "irregular".into(),
                source: SourceRange {
                    first_line: 0,
                    last_line: 0,
                },
                compositions: vec![[1.0, 0.0, 0.0]],
                fields: vec![TabulatedField {
                    phase_id: StablePhaseId(1),
                    property: "T".into(),
                    column_name: "Phase1.T".into(),
                    values: vec![TabulatedValue::missing()],
                    row_lines: vec![0],
                }],
            }));
        let appended = prepare_grid_paste(
            &irregular,
            0,
            1,
            0,
            "A\tB\tC\tPhase1.T\n0\t0.5\t0.5\t1250\n0.2\t0.3\t0.5\tCO:3000",
        )
        .unwrap();
        assert_eq!(appended.3, 2);
        assert_eq!(appended.0.grids[0].compositions().len(), 3);
        assert_eq!(
            appended.0.grids[0].fields()[0].values[2].state,
            TabulatedValueState::CutOff
        );

        let missing_compositions = prepare_grid_paste(&irregular, 0, 1, 3, "1250").unwrap_err();
        assert!(missing_compositions.message.contains("A, B, and C"));
    }

    #[test]
    fn starter_cells_are_typed_missing() {
        let state = ProjectDocument {
            dataset: ternary_contours_cli::default_regular_dataset(),
            ..ProjectDocument::new()
        };
        assert_eq!(state.dataset.grids[0].compositions().len(), 66);
        assert!(
            state.dataset.grids[0]
                .fields()
                .iter()
                .flat_map(|field| &field.values)
                .all(|value| value.state == TabulatedValueState::Missing && value.value.is_none())
        );
    }

    #[test]
    fn target_preview_and_apply_are_one_scoped_bridge_transaction() {
        let _guard = test_document_lock().lock().unwrap();
        {
            let mut state = document().lock().unwrap();
            state.dataset = ternary_contours_cli::default_regular_dataset();
            state.revision = 0;
            state.saved_revision = 0;
            state.dirty = false;
            state.undo.clear();
            state.redo.clear();
            let field = &mut fields_mut(&mut state.dataset.grids[0])[0];
            for (row, value) in field.values.iter_mut().enumerate() {
                *value = TabulatedValue::calculated(900.0 + row as f64).unwrap();
            }
            field.values[0] = TabulatedValue::missing();
            field.values[1] = TabulatedValue::missing();
        }
        let target_rows = [0_u32];
        let options = TcqtMeshExtrapolationOptions {
            grid_index: 0,
            field_index: 0,
            phase_id: 1,
            scope: ABI_MESH_SCOPE_TARGETS,
            all_phase_properties: false,
            target_rows: target_rows.as_ptr(),
            target_row_count: target_rows.len() as u32,
            method: ABI_CUBIC_STEFFEN,
            maximum_layers: 1,
            minimum_directional_support: 1,
            has_maximum_directional_spread: false,
            maximum_directional_spread: 0.0,
            has_minimum_value: false,
            minimum_value: 0.0,
            has_maximum_value: false,
            maximum_value: 0.0,
        };
        let mut preview = unsafe { std::mem::zeroed::<TcqtMeshExtrapolationSummary>() };
        unsafe {
            assert!(tcqt_preview_regular_mesh_extrapolation(&options, &mut preview).success);
        }
        assert_eq!(preview.values_proposed, 1);
        let mut rows = 0;
        assert!(unsafe { tcqt_mesh_extrapolation_preview_row_count(&mut rows) }.success);
        assert_eq!(rows, 1);
        let mut preview_row = unsafe { std::mem::zeroed::<TcqtMeshExtrapolationPreviewRow>() };
        unsafe {
            assert!(tcqt_mesh_extrapolation_preview_row_at(0, &mut preview_row).success);
        }
        assert_eq!(preview_row.row_index, 0);
        assert_eq!(preview_row.status, 0);
        let mut applied = unsafe { std::mem::zeroed::<TcqtMeshExtrapolationSummary>() };
        unsafe {
            assert!(tcqt_materialize_regular_mesh_extrapolation(&mut applied).success);
        }
        assert_eq!(applied.values_proposed, 1);
        let mut document_summary = unsafe { std::mem::zeroed::<TcqtProjectSummary>() };
        assert!(unsafe { tcqt_project_summary(&mut document_summary) }.success);
        assert_eq!(document_summary.revision, 1);
        assert!(document_summary.dirty);
        let mut target = unsafe { std::mem::zeroed::<TcqtCell>() };
        let mut unrelated = unsafe { std::mem::zeroed::<TcqtCell>() };
        unsafe {
            assert!(tcqt_grid_cell_at(0, 0, 0, &mut target).success);
            assert!(tcqt_grid_cell_at(0, 0, 1, &mut unrelated).success);
        }
        assert_eq!(target.state, 4);
        assert_eq!(unrelated.state, 3);
    }
    #[test]
    fn viewer_option_snapshot_is_validated_and_preserves_non_default_settings() {
        let raw = TcqtViewerCalculationOptions {
            automatic_range: false,
            minimum: 900.0,
            maximum: 1_200.0,
            level_step: 25.0,
            sampling_subdivisions: 37,
            regularize: false,
            regularization_spacing: 0.0,
            source_interpolation: 1,
            cubic_method: 1,
            partial_domain_policy: 2,
            continuation: 2,
            explicit_level_count: 0,
            explicit_levels: [0.0; TCQT_MAX_EXPLICIT_LEVELS],
        };
        let options = ViewerCalculationOptions::from_abi(&raw)
            .expect("configured Viewer options are valid")
            .projection_options();
        assert_eq!(options.sampling_subdivisions, Some(37));
        assert!(!options.regularize);
        assert_eq!(
            options.levels,
            vec![
                900.0, 925.0, 950.0, 975.0, 1_000.0, 1_025.0, 1_050.0, 1_075.0, 1_100.0, 1_125.0,
                1_150.0, 1_175.0, 1_200.0
            ]
        );
        assert!(matches!(
            options.interpolation.source,
            ternary_contours_cli::SourceInterpolation::CubicAlpha { .. }
        ));
    }

    #[test]
    fn field_queries_reject_stale_authoritative_option_revisions() {
        let _guard = test_document_lock().lock().unwrap();
        let (stale_revision, changed) = {
            let mut state = document().lock().unwrap();
            state.dataset = ternary_contours_cli::default_regular_dataset();
            let stale_revision = state.options_revision;
            let mut changed = state.viewer_options.clone();
            changed.sampling_subdivisions = changed.sampling_subdivisions.saturating_add(1);
            (stale_revision, changed.to_abi())
        };
        assert!(unsafe { tcqt_set_viewer_calculation_options(&changed) }.success);
        let property = std::ffi::CString::new("T").unwrap();
        let mut output = unsafe { std::mem::zeroed::<TcqtInspectionResult>() };
        let status = unsafe {
            tcqt_evaluate_field_current(
                0,
                1,
                property.as_ptr(),
                stale_revision,
                0.2,
                0.3,
                0.5,
                1,
                &mut output,
            )
        };
        assert!(!status.success);
        assert!(String::from_utf8_lossy(&status.message).contains("settings changed"));
    }

    #[test]
    fn coordinate_bridge_locations_are_field_independent_and_non_mutating() {
        let _guard = test_document_lock().lock().unwrap();
        {
            let mut state = document().lock().unwrap();
            state.dataset = ternary_contours_cli::default_regular_dataset();
            state.dirty = false;
            state.revision = 41;
            state.saved_revision = 41;
            state.undo.clear();
            state.redo.clear();
            for field in fields_mut(&mut state.dataset.grids[0]) {
                for (row, value) in field.values.iter_mut().enumerate() {
                    *value = TabulatedValue::calculated(800.0 + row as f64).unwrap();
                }
            }
        }
        let mut global = unsafe { std::mem::zeroed::<TcqtLocatedPoint>() };
        unsafe {
            assert!(tcqt_locate_grid_point(0, 2.0, 3.0, 5.0, &mut global).success);
        }
        assert!((global.a - 0.2).abs() < 1.0e-12);
        assert!((global.b - 0.3).abs() < 1.0e-12);
        assert!((global.c - 0.5).abs() < 1.0e-12);
        assert!(global.source_row0 < 66 && global.source_row1 < 66 && global.source_row2 < 66);
        let property = std::ffi::CString::new("T").unwrap();
        let mut option_state = unsafe { std::mem::zeroed::<TcqtViewerCalculationState>() };
        unsafe {
            assert!(tcqt_viewer_calculation_state(&mut option_state).success);
        }
        let mut evaluated = unsafe { std::mem::zeroed::<TcqtInspectionResult>() };
        unsafe {
            assert!(
                tcqt_evaluate_field_current(
                    0,
                    1,
                    property.as_ptr(),
                    option_state.options_revision,
                    global.a,
                    global.b,
                    global.c,
                    0,
                    &mut evaluated,
                )
                .success
            );
        }
        assert_eq!(evaluated.options_revision, option_state.options_revision);
        assert_eq!(
            evaluated.effective_source_interpolation,
            option_state.options.source_interpolation
        );
        assert_eq!(
            evaluated.effective_cubic_method,
            option_state.options.cubic_method
        );
        assert_eq!(
            evaluated.effective_partial_domain_policy,
            option_state.options.partial_domain_policy
        );
        assert_eq!(
            evaluated.effective_continuation,
            option_state.options.continuation
        );
        assert_eq!(evaluated.triangle_index, global.triangle_index);
        assert_eq!(
            (
                evaluated.source_row0,
                evaluated.source_row1,
                evaluated.source_row2
            ),
            (global.source_row0, global.source_row1, global.source_row2)
        );
        let mut local = unsafe { std::mem::zeroed::<TcqtLocatedPoint>() };
        unsafe {
            assert!(
                tcqt_locate_grid_local_point(
                    0,
                    global.triangle_index,
                    global.lambda0 * 2.0,
                    global.lambda1 * 2.0,
                    global.lambda2 * 2.0,
                    &mut local,
                )
                .success
            );
        }
        assert!((local.a - global.a).abs() < 1.0e-12);
        assert!((local.b - global.b).abs() < 1.0e-12);
        assert!((local.c - global.c).abs() < 1.0e-12);
        let mut summary = unsafe { std::mem::zeroed::<TcqtProjectSummary>() };
        unsafe {
            assert!(tcqt_project_summary(&mut summary).success);
        }
        assert_eq!(summary.revision, 41);
        assert!(!summary.dirty);

        {
            let mut state = document().lock().unwrap();
            let values = &mut fields_mut(&mut state.dataset.grids[0])[0].values;
            values[global.source_row0 as usize] = TabulatedValue::missing();
            values[global.source_row1 as usize] = TabulatedValue::cut_off();
        }
        let mut unchanged = unsafe { std::mem::zeroed::<TcqtLocatedPoint>() };
        unsafe {
            assert!(tcqt_locate_grid_point(0, 0.2, 0.3, 0.5, &mut unchanged).success);
        }
        assert_eq!(
            (
                unchanged.triangle_index,
                unchanged.source_row0,
                unchanged.source_row1,
                unchanged.source_row2
            ),
            (
                global.triangle_index,
                global.source_row0,
                global.source_row1,
                global.source_row2
            )
        );
    }

    #[test]
    fn coordinate_bridge_rejects_invalid_triplets_and_supports_irregular_meshes() {
        assert!(!tcqt_validate_coordinate_triplet(0.0, 0.0, 0.0).success);
        assert!(!tcqt_validate_coordinate_triplet(-1.0, 1.0, 1.0).success);
        assert!(!tcqt_validate_coordinate_triplet(f64::INFINITY, 0.0, 1.0).success);
        let grid = TabulatedGrid::Irregular(IrregularTabulatedGrid {
            name: "irregular".into(),
            source: SourceRange {
                first_line: 0,
                last_line: 0,
            },
            compositions: vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            fields: Vec::new(),
        });
        let global = locate_tabulated_grid_point(&grid, [1.0, 2.0, 3.0]).unwrap();
        let local = locate_tabulated_grid_local_point(
            &grid,
            global.triangle_index,
            [global.lambda0, global.lambda1, global.lambda2],
        )
        .unwrap();
        assert!((local.a - 1.0 / 6.0).abs() < 1.0e-12);
        assert!((local.b - 2.0 / 6.0).abs() < 1.0e-12);
        assert!((local.c - 3.0 / 6.0).abs() < 1.0e-12);
    }
    #[test]
    fn default_viewer_options_prefer_regular_cubic_alpha_akima_muggianu() {
        let options = ViewerCalculationOptions::default().to_abi();
        assert_eq!(options.source_interpolation, ABI_SOURCE_CUBIC_ALPHA);
        assert_eq!(options.cubic_method, ABI_CUBIC_AKIMA);
        assert_eq!(
            options.partial_domain_policy,
            ABI_PARTIAL_ONE_SIDED_THEN_LINEAR
        );
        assert_eq!(options.continuation, ABI_CONTINUATION_MUGGIANU);
        assert_eq!(
            ViewerCalculationOptions::default().interpolation,
            ProjectionOptions::default().interpolation
        );
    }
    #[test]
    fn viewer_setting_revision_invalidates_only_numerical_caches() {
        let mut state = ProjectDocument::new();
        state.projection_records.push(ProjectionCsvRecord {
            line_id: "cached".into(),
            point_index: 0,
            composition: [1.0, 0.0, 0.0],
            temperature: None,
            line_type: ternary_contours_cli::ProjectionLineType::StableIsotherm,
            phase: None,
            phase_1: None,
            phase_2: None,
            level: None,
            path_source: ProjectionPathSource::Raw,
            closed: false,
        });
        let document_revision = state.revision;
        let options_revision = state.options_revision;
        let mut options = state.viewer_options.clone();
        options.sampling_subdivisions = 37;
        assert!(state.set_viewer_options(options.to_abi()).unwrap());
        assert_eq!(state.revision, document_revision);
        assert_eq!(state.options_revision, options_revision + 1);
        assert!(
            !state.projection_records.is_empty(),
            "last accepted scene remains visible while recalculating"
        );
        assert!(!state.dirty);
        assert!(!state.set_viewer_options(options.to_abi()).unwrap());
        assert_eq!(state.options_revision, options_revision + 1);
    }

    #[test]
    fn invariant_point_reads_are_empty_without_an_accepted_projection_and_do_not_mutate() {
        let _guard = test_document_lock().lock().unwrap();
        let revision = {
            let mut state = document().lock().unwrap();
            state.projection = None;
            state.revision = 77;
            state.options_revision = 23;
            state.projection_options_revision = 0;
            state.projection_request_id = 0;
            state.revision
        };
        let mut count = u32::MAX;
        unsafe {
            assert!(tcqt_invariant_point_count(&mut count).success);
            assert_eq!(count, 0);
            let mut point = std::mem::zeroed::<TcqtInvariantPoint>();
            assert!(!tcqt_invariant_point_at(0, &mut point).success);
        }
        let state = document().lock().unwrap();
        assert_eq!(state.revision, revision);
        assert!(state.projection.is_none());
    }

    #[test]
    fn bridge_calculation_uses_the_complete_authoritative_option_snapshot() {
        let _guard = test_document_lock().lock().unwrap();
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tools/ternary-contours-cli/fixtures/interior-invariant.tct");
        let fixture_text = std::fs::read_to_string(&fixture).unwrap();
        let path = std::ffi::CString::new(fixture.to_string_lossy().as_bytes()).unwrap();
        unsafe {
            assert!(tcqt_open_document(path.as_ptr()).success);
            let mut document_summary = std::mem::zeroed::<TcqtProjectSummary>();
            assert!(tcqt_project_summary(&mut document_summary).success);
            let configured = TcqtViewerCalculationOptions {
                automatic_range: true,
                minimum: 0.0,
                maximum: 0.0,
                level_step: 5.0,
                sampling_subdivisions: 17,
                regularize: true,
                regularization_spacing: 0.01,
                source_interpolation: ABI_SOURCE_LINEAR,
                cubic_method: ABI_CUBIC_STEFFEN,
                partial_domain_policy: ABI_PARTIAL_ONE_SIDED_THEN_LINEAR,
                continuation: ABI_CONTINUATION_KOHLER,
                explicit_level_count: 0,
                explicit_levels: [0.0; TCQT_MAX_EXPLICIT_LEVELS],
            };
            assert!(tcqt_set_viewer_calculation_options(&configured).success);
            let direct = calculate_projection(
                &ternary_contours_cli::parse_str(&fixture_text).unwrap(),
                &ViewerCalculationOptions::from_abi(&configured)
                    .unwrap()
                    .projection_options(),
            )
            .unwrap();
            let direct_binary = direct
                .stable_boundaries
                .nodes
                .iter()
                .filter(|node| matches!(node, StableInvariantNode::Binary(_)))
                .count() as u32;
            let direct_interior = direct
                .stable_boundaries
                .nodes
                .iter()
                .filter(|node| matches!(node, StableInvariantNode::Interior(_)))
                .count() as u32;
            assert_eq!(
                (
                    direct_binary,
                    direct_interior,
                    direct.stable_boundaries.univariants.len()
                ),
                (3, 1, 3)
            );
            let mut state = std::mem::zeroed::<TcqtViewerCalculationState>();
            assert!(tcqt_viewer_calculation_state(&mut state).success);
            let result = tcqt_calculate_viewer(
                &state.options,
                document_summary.revision,
                state.options_revision,
                91,
            );
            assert!(
                result.success,
                "{}",
                String::from_utf8_lossy(&result.message)
            );
            let mut projection = std::mem::zeroed::<TcqtProjectionSummary>();
            assert!(tcqt_projection_summary(&mut projection).success);
            assert!(projection.available);
            assert!(
                projection.level_count > 0,
                "automatic bootstrap must derive concrete levels"
            );
            assert_eq!(projection.stable_topology_build_count, 1);
            assert_eq!(projection.stable_topology_reuse_count, 0);
            assert_eq!(
                (
                    projection.binary_invariant_count,
                    projection.interior_invariant_count,
                    projection.univariant_count
                ),
                (
                    direct_binary,
                    direct_interior,
                    direct.stable_boundaries.univariants.len() as u32
                )
            );
            assert_eq!(projection.effective_level_step, 5.0);
            assert_eq!(projection.effective_sampling_subdivisions, 17);
            assert_eq!(projection.effective_source_interpolation, ABI_SOURCE_LINEAR);
            assert_eq!(projection.effective_continuation, ABI_CONTINUATION_MUGGIANU);
            assert_eq!(projection.options_revision, state.options_revision);
            assert_eq!(projection.request_id, 91);
            let mut invariant_count = 0;
            assert!(tcqt_invariant_point_count(&mut invariant_count).success);
            assert_eq!(invariant_count, direct.stable_boundaries.nodes.len() as u32);
            let mut prior_id = None;
            for index in 0..invariant_count {
                let mut invariant = std::mem::zeroed::<TcqtInvariantPoint>();
                assert!(tcqt_invariant_point_at(index, &mut invariant).success);
                assert!(prior_id.is_none_or(|prior| invariant.id > prior));
                prior_id = Some(invariant.id);
                assert_eq!(invariant.dataset_revision, projection.dataset_revision);
                assert_eq!(invariant.options_revision, projection.options_revision);
                assert_eq!(invariant.request_id, projection.request_id);
                assert!(invariant.temperature.is_finite());
                assert!(invariant.phases.iter().any(|byte| *byte != 0));
                if invariant.kind == 0 {
                    assert!(invariant.boundary_name.iter().any(|byte| *byte != 0));
                } else {
                    assert!(invariant.boundary_name.iter().all(|byte| *byte == 0));
                }
            }
            let mut record_count = 0;
            assert!(tcqt_projection_record_count(&mut record_count).success);
            assert!(record_count > 0);
            let mut line_type_counts = [0_u32; 4];
            let mut saw_raw = false;
            let mut saw_regularized = false;
            let mut saw_phase_pair = false;
            for index in 0..record_count {
                let mut record = std::mem::zeroed::<TcqtProjectionRecord>();
                assert!(tcqt_projection_record_at(index, &mut record).success);
                if (record.line_type as usize) < line_type_counts.len() {
                    line_type_counts[record.line_type as usize] += 1;
                }
                saw_raw |= record.path_source == 0;
                saw_regularized |= record.path_source == 1;
                saw_phase_pair |= record.phase_1.iter().any(|byte| *byte != 0)
                    && record.phase_2.iter().any(|byte| *byte != 0);
            }
            assert!(line_type_counts.iter().all(|count| *count > 0));
            assert!(saw_raw && saw_regularized);
            assert!(saw_phase_pair);
        }
    }
    #[test]
    fn isotherm_only_viewer_update_reuses_the_accepted_stable_topology() {
        let _guard = test_document_lock().lock().unwrap();
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tools/ternary-contours-cli/fixtures/interior-invariant.tct");
        let path = std::ffi::CString::new(fixture.to_string_lossy().as_bytes()).unwrap();
        unsafe {
            assert!(tcqt_open_document(path.as_ptr()).success);
            let initial = TcqtViewerCalculationOptions {
                automatic_range: false,
                minimum: 100.0,
                maximum: 120.0,
                level_step: 10.0,
                sampling_subdivisions: 17,
                regularize: false,
                regularization_spacing: 0.01,
                source_interpolation: ABI_SOURCE_LINEAR,
                cubic_method: ABI_CUBIC_AKIMA,
                partial_domain_policy: ABI_PARTIAL_ONE_SIDED_THEN_LINEAR,
                continuation: ABI_CONTINUATION_MUGGIANU,
                explicit_level_count: 0,
                explicit_levels: [0.0; TCQT_MAX_EXPLICIT_LEVELS],
            };
            assert!(tcqt_set_viewer_calculation_options(&initial).success);
            let mut project = std::mem::zeroed::<TcqtProjectSummary>();
            let mut options = std::mem::zeroed::<TcqtViewerCalculationState>();
            assert!(tcqt_project_summary(&mut project).success);
            assert!(tcqt_viewer_calculation_state(&mut options).success);
            assert!(
                tcqt_calculate_viewer(
                    &options.options,
                    project.revision,
                    options.options_revision,
                    301,
                )
                .success
            );
            let mut first = std::mem::zeroed::<TcqtProjectionSummary>();
            assert!(tcqt_projection_summary(&mut first).success);
            assert_eq!(first.stable_topology_build_count, 1);
            assert_eq!(first.stable_topology_reuse_count, 0);
            let node_count = first.invariant_count;

            let mut levels_only = initial;
            levels_only.level_step = 5.0;
            assert!(tcqt_set_viewer_calculation_options(&levels_only).success);
            let mut retained = std::mem::zeroed::<TcqtProjectionSummary>();
            assert!(tcqt_projection_summary(&mut retained).success);
            assert!(
                retained.available,
                "level-only update retains the accepted graph"
            );
            assert_eq!(retained.invariant_count, node_count);
            assert!(tcqt_viewer_calculation_state(&mut options).success);
            assert!(
                tcqt_calculate_viewer(
                    &options.options,
                    project.revision,
                    options.options_revision,
                    302,
                )
                .success
            );
            let mut second = std::mem::zeroed::<TcqtProjectionSummary>();
            assert!(tcqt_projection_summary(&mut second).success);
            assert!(second.stable_topology_reused);
            assert_eq!(second.stable_topology_build_count, 1);
            assert_eq!(second.stable_topology_reuse_count, 1);
            assert_eq!(second.isotherm_rebuild_count, 2);
            assert_eq!(second.invariant_count, node_count);

            let mut topology_change = levels_only;
            topology_change.sampling_subdivisions = 18;
            assert!(tcqt_set_viewer_calculation_options(&topology_change).success);
            assert!(tcqt_viewer_calculation_state(&mut options).success);
            assert!(
                tcqt_calculate_viewer(
                    &options.options,
                    project.revision,
                    options.options_revision,
                    303,
                )
                .success
            );
            let mut third = std::mem::zeroed::<TcqtProjectionSummary>();
            assert!(tcqt_projection_summary(&mut third).success);
            assert!(!third.stable_topology_reused);
            assert_eq!(third.stable_topology_build_count, 2);
        }
    }

    #[test]
    fn contour_failure_retains_the_independently_accepted_stable_topology() {
        let _guard = test_document_lock().lock().unwrap();
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tools/ternary-contours-cli/fixtures/interior-invariant.tct");
        let path = std::ffi::CString::new(fixture.to_string_lossy().as_bytes()).unwrap();
        unsafe {
            assert!(tcqt_open_document(path.as_ptr()).success);
            let options = TcqtViewerCalculationOptions {
                automatic_range: false,
                minimum: 1_000_000.0,
                maximum: 1_000_100.0,
                level_step: 100.0,
                sampling_subdivisions: 17,
                regularize: false,
                regularization_spacing: 0.01,
                source_interpolation: ABI_SOURCE_LINEAR,
                cubic_method: ABI_CUBIC_AKIMA,
                partial_domain_policy: ABI_PARTIAL_ONE_SIDED_THEN_LINEAR,
                continuation: ABI_CONTINUATION_MUGGIANU,
                explicit_level_count: 0,
                explicit_levels: [0.0; TCQT_MAX_EXPLICIT_LEVELS],
            };
            assert!(tcqt_set_viewer_calculation_options(&options).success);
            let mut project = std::mem::zeroed::<TcqtProjectSummary>();
            let mut state = std::mem::zeroed::<TcqtViewerCalculationState>();
            assert!(tcqt_project_summary(&mut project).success);
            assert!(tcqt_viewer_calculation_state(&mut state).success);
            let result = tcqt_calculate_viewer(
                &state.options,
                project.revision,
                state.options_revision,
                390,
            );
            assert!(result.success);
            assert!(
                String::from_utf8_lossy(&result.message)
                    .contains("Stable topology calculated; isotherm calculation incomplete")
            );
            let mut summary = std::mem::zeroed::<TcqtProjectionSummary>();
            assert!(tcqt_projection_summary(&mut summary).success);
            assert!(summary.available);
            assert_eq!(summary.binary_invariant_count, 3);
            assert_eq!(summary.interior_invariant_count, 1);
            assert_eq!(summary.univariant_count, 3);
            assert_eq!(summary.level_count, 2);
            assert_eq!(summary.contour_path_count, 0);
            assert_eq!(summary.stable_topology_build_count, 1);
            let mut record_count = 0;
            assert!(tcqt_projection_record_count(&mut record_count).success);
            assert!(record_count > 0, "topology records remain renderable");
        }
    }

    #[test]
    fn viewer_trace_observes_the_accepted_projection_request() {
        let _guard = test_document_lock().lock().unwrap();
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tools/ternary-contours-cli/fixtures/interior-invariant.tct");
        let path = std::ffi::CString::new(fixture.to_string_lossy().as_bytes()).unwrap();
        let trace_path = std::env::temp_dir().join(format!(
            "ternary-contours-viewer-request-{}.jsonl",
            std::process::id()
        ));
        let trace_path_c = std::ffi::CString::new(trace_path.to_string_lossy().as_bytes()).unwrap();
        unsafe {
            assert!(tcqt_open_document(path.as_ptr()).success);
            let options = TcqtViewerCalculationOptions {
                automatic_range: false,
                minimum: 100.0,
                maximum: 120.0,
                level_step: 10.0,
                sampling_subdivisions: 17,
                regularize: false,
                regularization_spacing: 0.01,
                source_interpolation: ABI_SOURCE_LINEAR,
                cubic_method: ABI_CUBIC_STEFFEN,
                partial_domain_policy: ABI_PARTIAL_ONE_SIDED_THEN_LINEAR,
                continuation: ABI_CONTINUATION_KOHLER,
                explicit_level_count: 0,
                explicit_levels: [0.0; TCQT_MAX_EXPLICIT_LEVELS],
            };
            assert!(tcqt_set_viewer_calculation_options(&options).success);
            assert!(tcqt_set_numerical_trace(2, trace_path_c.as_ptr()).success);
            let mut document_summary = std::mem::zeroed::<TcqtProjectSummary>();
            let mut state = std::mem::zeroed::<TcqtViewerCalculationState>();
            assert!(tcqt_project_summary(&mut document_summary).success);
            assert!(tcqt_viewer_calculation_state(&mut state).success);
            assert!(
                tcqt_calculate_viewer(
                    &state.options,
                    document_summary.revision,
                    state.options_revision,
                    92,
                )
                .success
            );
            assert!(tcqt_set_numerical_trace(0, std::ptr::null()).success);
        }
        let trace = std::fs::read_to_string(&trace_path).unwrap();
        assert!(trace.contains("\"request_id\":92"));
        assert!(trace.contains("\"sampling_subdivisions\":17"));
        assert!(trace.contains("\"regularization\":false"));
        std::fs::remove_file(trace_path).unwrap();
    }

    #[test]
    fn projection_csv_export_uses_the_revision_pinned_accepted_snapshot() {
        let _guard = test_document_lock().lock().unwrap();
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tools/ternary-contours-cli/fixtures/interior-invariant.tct");
        let fixture_c = std::ffi::CString::new(fixture.to_string_lossy().as_bytes()).unwrap();
        let output = std::env::temp_dir().join(format!(
            "ternary-contours-projection-export-{}.csv",
            std::process::id()
        ));
        let output_c = std::ffi::CString::new(output.to_string_lossy().as_bytes()).unwrap();
        unsafe {
            assert!(tcqt_open_document(fixture_c.as_ptr()).success);
            let options = TcqtViewerCalculationOptions {
                automatic_range: false,
                minimum: 100.0,
                maximum: 120.0,
                level_step: 10.0,
                sampling_subdivisions: 17,
                regularize: false,
                regularization_spacing: 0.01,
                source_interpolation: ABI_SOURCE_LINEAR,
                cubic_method: ABI_CUBIC_AKIMA,
                partial_domain_policy: ABI_PARTIAL_ONE_SIDED_THEN_LINEAR,
                continuation: ABI_CONTINUATION_MUGGIANU,
                explicit_level_count: 0,
                explicit_levels: [0.0; TCQT_MAX_EXPLICIT_LEVELS],
            };
            assert!(tcqt_set_viewer_calculation_options(&options).success);
            let mut project = std::mem::zeroed::<TcqtProjectSummary>();
            let mut state = std::mem::zeroed::<TcqtViewerCalculationState>();
            assert!(tcqt_project_summary(&mut project).success);
            assert!(tcqt_viewer_calculation_state(&mut state).success);
            assert!(
                tcqt_calculate_viewer(
                    &state.options,
                    project.revision,
                    state.options_revision,
                    701
                )
                .success
            );
            let mut accepted = std::mem::zeroed::<TcqtProjectionSummary>();
            assert!(tcqt_projection_summary(&mut accepted).success && accepted.available);
            let selection = TcqtProjectionCsvExportOptions {
                invariants: true,
                univariants: true,
                isotherms: true,
                path_display_mode: 1,
                expected_dataset_revision: accepted.dataset_revision,
                expected_options_revision: accepted.options_revision,
                expected_request_id: accepted.request_id,
            };
            assert!(tcqt_export_projection_csv(output_c.as_ptr(), &selection).success);
            let first = std::fs::read_to_string(&output).unwrap();
            assert!(first.starts_with("A,B,C,\"T, K\",phase1,phase2,phase3\r\n"));
            assert!(!first.contains("line_id"));
            assert!(!first.contains(",,,,,,"));

            // A newer data revision leaves the old accepted scene exportable;
            // the bridge uses its retained immutable dataset/projection pair.
            let token = std::ffi::CString::new("101").unwrap();
            assert!(tcqt_set_grid_cell(0, 0, 0, token.as_ptr()).success);
            assert!(tcqt_export_projection_csv(output_c.as_ptr(), &selection).success);
            assert_eq!(std::fs::read_to_string(&output).unwrap(), first);

            let none = TcqtProjectionCsvExportOptions {
                invariants: false,
                univariants: false,
                isotherms: false,
                ..selection
            };
            assert!(!tcqt_export_projection_csv(output_c.as_ptr(), &none).success);
        }
        std::fs::remove_file(output).unwrap();
    }

    #[test]
    fn rust_projection_scene_style_uses_shared_invariant_style() {
        assert_ne!(
            projection_paint_style(0, 1).0,
            projection_paint_style(1, 1).0
        );
        assert_eq!(
            projection_paint_style(2, 1).2,
            projection_paint_style(3, 1).2
        );
        assert_eq!(projection_paint_style(2, 1).2, 2);
        assert!(projection_paint_style(0, 1).1.is_finite());
    }
    #[test]
    fn viewer_vertex_mutation_is_one_revision_one_undo_and_invalidates_projection() {
        let mut state = ProjectDocument {
            dataset: ternary_contours_cli::default_regular_dataset(),
            ..ProjectDocument::new()
        };
        state.projection_records.push(ProjectionCsvRecord {
            line_id: "previous".into(),
            point_index: 0,
            composition: [1.0, 0.0, 0.0],
            temperature: None,
            line_type: ternary_contours_cli::ProjectionLineType::StableIsotherm,
            phase: None,
            phase_1: None,
            phase_2: None,
            level: None,
            path_source: ternary_contours_cli::ProjectionPathSource::Raw,
            closed: false,
        });
        let revision = state.revision;
        state
            .mutate(|dataset| {
                let field = &mut fields_mut(&mut dataset.grids[0])[0];
                field.values[0] =
                    TabulatedValue::calculated(1_250.0).map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
        assert_eq!(state.revision, revision + 1);
        assert_eq!(state.undo.len(), 1);
        assert!(state.dirty);
        assert!(
            !state.projection_records.is_empty(),
            "editing retains the last visible projection snapshot"
        );
        assert_eq!(
            state.dataset.grids[0].fields()[0].values[0].value,
            Some(1_250.0)
        );
    }
    #[test]
    fn a_regular_grid_has_canonical_rows_and_fields() {
        let state = ProjectDocument {
            dataset: ternary_contours_cli::default_regular_dataset(),
            ..ProjectDocument::new()
        };
        let mut grid = TabulatedGrid::Regular(RegularTabulatedGrid {
            name: "next".into(),
            source: SourceRange {
                first_line: 0,
                last_line: 0,
            },
            subdivisions: 4,
            order: RowOrder::Canonical,
            composition_columns: CompositionColumns::None,
            compositions: RegularTernaryGrid::new(4).unwrap().compositions().collect(),
            fields: Vec::new(),
        });
        initialise_fields(&state.dataset, &mut grid);
        assert_eq!(grid.compositions().len(), 15);
        assert_eq!(grid.fields().len(), 3);
    }
}
