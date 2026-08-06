//! Deterministic layered one-dimensional extrapolation on regular ternary grids.
//!
//! A target is considered only when it is explicitly eligible.  Every layer is
//! evaluated from an immutable snapshot of previous layers, which makes the
//! result independent of vertex traversal order.  Each directional estimate
//! walks a single canonical lattice ray and uses the shared `spline1d` cubic
//! endpoint construction; no missing sample is ever passed to a cubic kernel.

use core::fmt;

use crate::{
    CubicAlphaMethod, GridVertexId, LatticeCoordinate, NumericalTraceEventKind,
    NumericalTraceLevel, NumericalTraceSession, NumericalTraceSink, RegularTernaryGrid,
    TraceDecision, decision,
};

/// One directed ray of a regular ternary lattice line.
///
/// The numeric discriminant is stable and deliberately independent from
/// canonical vertex ordering.  Opposite directions belong to the same line
/// family, but remain separate estimates at a target vertex.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum ExtrapolationDirection {
    /// Constant C, moving toward increasing A and decreasing B.
    ConstantCPositive = 0,
    /// Constant C, moving toward decreasing A and increasing B.
    ConstantCNegative = 1,
    /// Constant A, moving toward increasing B and decreasing C.
    ConstantAPositive = 2,
    /// Constant A, moving toward decreasing B and increasing C.
    ConstantANegative = 3,
    /// Constant B, moving toward increasing A and decreasing C.
    ConstantBPositive = 4,
    /// Constant B, moving toward decreasing A and increasing C.
    ConstantBNegative = 5,
}

impl ExtrapolationDirection {
    const ALL: [Self; 6] = [
        Self::ConstantCPositive,
        Self::ConstantCNegative,
        Self::ConstantAPositive,
        Self::ConstantANegative,
        Self::ConstantBPositive,
        Self::ConstantBNegative,
    ];

    const fn delta(self) -> [isize; 3] {
        match self {
            Self::ConstantCPositive => [1, -1, 0],
            Self::ConstantCNegative => [-1, 1, 0],
            Self::ConstantAPositive => [0, 1, -1],
            Self::ConstantANegative => [0, -1, 1],
            Self::ConstantBPositive => [1, 0, -1],
            Self::ConstantBNegative => [-1, 0, 1],
        }
    }
}

/// Guard rails for [`extrapolate_regular_mesh`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegularMeshExtrapolationOptions {
    /// Shared one-dimensional cubic method used at every accepted ray.
    pub method: CubicAlphaMethod,
    /// Number of synchronous propagation layers to attempt.
    pub maximum_layers: u16,
    /// Required contiguous samples on a ray. Cubic endpoint construction also
    /// requires three samples, so values below three do not lower that floor.
    pub minimum_directional_support: usize,
    /// Reject a vertex when accepted ray estimates disagree by more than this.
    pub maximum_directional_spread: Option<f64>,
    /// Optional inclusive lower scalar bound.
    pub minimum_value: Option<f64>,
    /// Optional inclusive upper scalar bound.
    pub maximum_value: Option<f64>,
    /// Optional maximum ratio between endpoint derivative and observed secants.
    pub maximum_endpoint_slope_growth: Option<f64>,
    /// Optional maximum ratio between endpoint curvature and secant curvature.
    pub maximum_endpoint_curvature_growth: Option<f64>,
}

impl Default for RegularMeshExtrapolationOptions {
    fn default() -> Self {
        Self {
            method: CubicAlphaMethod::Steffen,
            maximum_layers: 1,
            minimum_directional_support: 3,
            maximum_directional_spread: None,
            minimum_value: None,
            maximum_value: None,
            maximum_endpoint_slope_growth: None,
            maximum_endpoint_curvature_growth: None,
        }
    }
}

/// One accepted one-dimensional continuation into a target vertex.
#[derive(Clone, Debug, PartialEq)]
pub struct DirectionalEstimate {
    /// Stable directed lattice ray identifier.
    pub direction: ExtrapolationDirection,
    /// Canonical vertex rows used, nearest endpoint first.
    pub support_vertex_indices: Vec<usize>,
    /// Finite values corresponding to [`Self::support_vertex_indices`].
    pub support_values: Vec<f64>,
    /// The one-step extrapolated scalar.
    pub value: f64,
}

/// Why a ray or target could not safely be materialized.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExtrapolationRejection {
    /// A ray hit a mesh boundary or unavailable cell before enough support.
    InsufficientContiguousSupport { found: usize, required: usize },
    /// The selected cubic construction produced a non-finite value.
    NonFiniteEstimate,
    /// A scalar range guard rejected the estimate.
    ValueOutsideBounds,
    /// An endpoint slope guard rejected the estimate.
    EndpointSlopeGrowth,
    /// An endpoint curvature guard rejected the estimate.
    EndpointCurvatureGrowth,
    /// Multiple accepted rays disagreed beyond the configured spread.
    DirectionalSpreadExceeded,
    /// No ray produced a safe estimate.
    NoSafeDirection,
}

impl fmt::Display for ExtrapolationRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsufficientContiguousSupport { found, required } => {
                write!(
                    formatter,
                    "only {found} contiguous finite supports were available; {required} required"
                )
            }
            Self::NonFiniteEstimate => {
                formatter.write_str("cubic endpoint estimate was not finite")
            }
            Self::ValueOutsideBounds => formatter.write_str("estimate violates scalar bounds"),
            Self::EndpointSlopeGrowth => {
                formatter.write_str("endpoint slope growth guard rejected estimate")
            }
            Self::EndpointCurvatureGrowth => {
                formatter.write_str("endpoint curvature growth guard rejected estimate")
            }
            Self::DirectionalSpreadExceeded => {
                formatter.write_str("directional estimates disagree")
            }
            Self::NoSafeDirection => formatter.write_str("no safe extrapolation direction"),
        }
    }
}

/// One target rejected while a layer was evaluated.
#[derive(Clone, Debug, PartialEq)]
pub struct RejectedExtrapolationVertex {
    /// Canonical target row.
    pub vertex_index: usize,
    /// Synchronous propagation layer considered.
    pub layer: u16,
    /// Rejection explanations in stable ray order.
    pub reasons: Vec<(ExtrapolationDirection, ExtrapolationRejection)>,
}

/// One accepted materialized regular-mesh extrapolation.
#[derive(Clone, Debug, PartialEq)]
pub struct RegularMeshExtrapolatedValue {
    /// Canonical target row.
    pub vertex_index: usize,
    /// Combined scalar estimate.
    pub value: f64,
    /// First synchronous iteration in which the target became fillable.
    pub layer: u16,
    /// Method used by every accepted direction.
    pub method: CubicAlphaMethod,
    /// Number of accepted independent directional rays.
    pub directional_support_count: usize,
    /// Accepted ray estimates in stable direction order.
    pub directional_estimates: Vec<DirectionalEstimate>,
    /// Maximum minus minimum accepted directional estimate.
    pub spread: f64,
}

/// Aggregate result counters, including candidates left unavailable.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RegularMeshExtrapolationDiagnostics {
    /// Number of original finite samples.
    pub original_finite_values: usize,
    /// Number of eligible missing input cells.
    pub eligible_missing_values: usize,
    /// Number of materialized values.
    pub values_created: usize,
    /// Number of layers which created at least one value.
    pub layers_completed: u16,
    /// Number of remaining eligible cells.
    pub remaining_eligible_missing_values: usize,
    /// Ray rejections due to insufficient contiguous support.
    pub rejected_insufficient_support: usize,
    /// Candidate rejections due to directional disagreement.
    pub rejected_directional_spread: usize,
    /// Candidates that had no usable ray.
    pub rejected_no_safe_direction: usize,
}

/// Deterministic result of a layered mesh extrapolation preview.
#[derive(Clone, Debug, PartialEq)]
pub struct RegularMeshExtrapolationResult {
    /// Accepted values in ascending layer then canonical vertex order.
    pub values: Vec<RegularMeshExtrapolatedValue>,
    /// Candidates left missing and their typed reasons.
    pub rejections: Vec<RejectedExtrapolationVertex>,
    /// Aggregate diagnostics.
    pub diagnostics: RegularMeshExtrapolationDiagnostics,
}

/// Input validation failure for the focused regular-grid algorithm.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegularMeshExtrapolationError {
    /// Scalar and candidate arrays must align with the canonical grid.
    LengthMismatch {
        expected: usize,
        values: usize,
        eligible: usize,
    },
    /// Only finite source scalars are permitted.
    NonFiniteSourceValue { vertex_index: usize },
    /// Guard configuration is inconsistent or non-finite.
    InvalidOptions { message: &'static str },
}

impl fmt::Display for RegularMeshExtrapolationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthMismatch {
                expected,
                values,
                eligible,
            } => write!(
                formatter,
                "regular mesh has {expected} vertices but received {values} values and {eligible} eligibility flags"
            ),
            Self::NonFiniteSourceValue { vertex_index } => {
                write!(
                    formatter,
                    "source value at regular-grid vertex {vertex_index} is not finite"
                )
            }
            Self::InvalidOptions { message } => formatter.write_str(message),
        }
    }
}

impl std::error::Error for RegularMeshExtrapolationError {}

/// Materialize eligible missing cells from finite values on a regular lattice.
///
/// `values` represents finite calculated or previously accepted extrapolated
/// samples as `Some`. `eligible` distinguishes `NA` from cells such as `CO`
/// that must remain an unavailable barrier. The returned preview does not
/// mutate either input.
pub fn extrapolate_regular_mesh(
    grid: RegularTernaryGrid,
    values: &[Option<f64>],
    eligible: &[bool],
    options: RegularMeshExtrapolationOptions,
) -> Result<RegularMeshExtrapolationResult, RegularMeshExtrapolationError> {
    let mut sink = crate::NoopTraceSink;
    let mut trace = crate::NumericalTraceSession::new(&mut sink);
    extrapolate_regular_mesh_impl(grid, values, eligible, options, &mut trace)
}

/// Traced variant of [`extrapolate_regular_mesh`].
pub fn extrapolate_regular_mesh_with_trace(
    grid: RegularTernaryGrid,
    values: &[Option<f64>],
    eligible: &[bool],
    options: RegularMeshExtrapolationOptions,
    sink: &mut dyn NumericalTraceSink,
) -> Result<RegularMeshExtrapolationResult, RegularMeshExtrapolationError> {
    let mut trace = NumericalTraceSession::new(sink);
    extrapolate_regular_mesh_impl(grid, values, eligible, options, &mut trace)
}

fn extrapolate_regular_mesh_impl(
    grid: RegularTernaryGrid,
    values: &[Option<f64>],
    eligible: &[bool],
    options: RegularMeshExtrapolationOptions,
    trace: &mut NumericalTraceSession<'_>,
) -> Result<RegularMeshExtrapolationResult, RegularMeshExtrapolationError> {
    validate_options(options)?;
    let expected = grid.vertex_count();
    if values.len() != expected || eligible.len() != expected {
        return Err(RegularMeshExtrapolationError::LengthMismatch {
            expected,
            values: values.len(),
            eligible: eligible.len(),
        });
    }
    if let Some((vertex_index, _)) = values
        .iter()
        .enumerate()
        .find(|(_, value)| value.is_some_and(|value| !value.is_finite()))
    {
        return Err(RegularMeshExtrapolationError::NonFiniteSourceValue { vertex_index });
    }

    let mut diagnostics = RegularMeshExtrapolationDiagnostics {
        original_finite_values: values.iter().filter(|value| value.is_some()).count(),
        eligible_missing_values: values
            .iter()
            .zip(eligible)
            .filter(|(value, eligible)| value.is_none() && **eligible)
            .count(),
        ..RegularMeshExtrapolationDiagnostics::default()
    };
    emit(
        trace,
        NumericalTraceEventKind::MeshExtrapolationStarted,
        TraceDecision {
            counts: Some(crate::TraceCounts {
                calculated: diagnostics.original_finite_values,
                missing: diagnostics.eligible_missing_values,
                ..crate::TraceCounts::default()
            }),
            ..TraceDecision::default()
        },
    );

    let mut working = values.to_vec();
    let mut accepted = Vec::new();
    let mut rejections = Vec::new();
    for layer in 1..=options.maximum_layers {
        emit(
            trace,
            NumericalTraceEventKind::ExtrapolationLayerStarted,
            TraceDecision {
                iteration: Some(layer as usize),
                ..TraceDecision::default()
            },
        );
        let previous = working.clone();
        let mut layer_values = Vec::new();
        let mut layer_rejections = Vec::new();
        for vertex_index in 0..expected {
            if previous[vertex_index].is_some() || !eligible[vertex_index] {
                continue;
            }
            match estimate_vertex(grid, &previous, vertex_index, layer, options, trace) {
                Ok(value) => layer_values.push(value),
                Err(rejection) => layer_rejections.push(rejection),
            }
        }
        if layer_values.is_empty() {
            diagnostics.rejected_insufficient_support += layer_rejections
                .iter()
                .flat_map(|entry| entry.reasons.iter())
                .filter(|(_, reason)| {
                    matches!(
                        reason,
                        ExtrapolationRejection::InsufficientContiguousSupport { .. }
                    )
                })
                .count();
            diagnostics.rejected_directional_spread += layer_rejections
                .iter()
                .filter(|entry| {
                    entry.reasons.iter().any(|(_, reason)| {
                        matches!(reason, ExtrapolationRejection::DirectionalSpreadExceeded)
                    })
                })
                .count();
            diagnostics.rejected_no_safe_direction += layer_rejections
                .iter()
                .filter(|entry| {
                    entry.reasons.iter().any(|(_, reason)| {
                        matches!(reason, ExtrapolationRejection::NoSafeDirection)
                    })
                })
                .count();
            rejections.extend(layer_rejections);
            emit(
                trace,
                NumericalTraceEventKind::ExtrapolationLayerCompleted,
                TraceDecision {
                    iteration: Some(layer as usize),
                    counts: Some(crate::TraceCounts::default()),
                    ..TraceDecision::default()
                },
            );
            break;
        }
        for value in &layer_values {
            working[value.vertex_index] = Some(value.value);
        }
        diagnostics.layers_completed = layer;
        diagnostics.values_created += layer_values.len();
        emit(
            trace,
            NumericalTraceEventKind::ExtrapolationLayerCompleted,
            TraceDecision {
                iteration: Some(layer as usize),
                counts: Some(crate::TraceCounts {
                    calculated: layer_values.len(),
                    ..crate::TraceCounts::default()
                }),
                ..TraceDecision::default()
            },
        );
        accepted.extend(layer_values);
        rejections = layer_rejections;
    }
    diagnostics.remaining_eligible_missing_values = working
        .iter()
        .zip(eligible)
        .filter(|(value, eligible)| value.is_none() && **eligible)
        .count();
    emit(
        trace,
        NumericalTraceEventKind::MeshExtrapolationCompleted,
        TraceDecision {
            counts: Some(crate::TraceCounts {
                calculated: diagnostics.values_created,
                missing: diagnostics.remaining_eligible_missing_values,
                ..crate::TraceCounts::default()
            }),
            ..TraceDecision::default()
        },
    );
    Ok(RegularMeshExtrapolationResult {
        values: accepted,
        rejections,
        diagnostics,
    })
}

fn estimate_vertex(
    grid: RegularTernaryGrid,
    values: &[Option<f64>],
    vertex_index: usize,
    layer: u16,
    options: RegularMeshExtrapolationOptions,
    trace: &mut NumericalTraceSession<'_>,
) -> Result<RegularMeshExtrapolatedValue, RejectedExtrapolationVertex> {
    let coordinate = match grid.lattice_coordinate(GridVertexId(vertex_index)) {
        Ok(coordinate) => coordinate,
        Err(_) => unreachable!("vertex index came from grid length"),
    };
    let mut accepted = Vec::new();
    let mut reasons = Vec::new();
    for direction in ExtrapolationDirection::ALL {
        emit(
            trace,
            NumericalTraceEventKind::ExtrapolationDirectionStarted,
            TraceDecision {
                composition: grid.composition(GridVertexId(vertex_index)).ok(),
                iteration: Some(layer as usize),
                ..TraceDecision::default()
            },
        );
        match estimate_direction(grid, values, coordinate, direction, options) {
            Ok(estimate) => {
                emit(
                    trace,
                    NumericalTraceEventKind::ExtrapolationDirectionAccepted,
                    TraceDecision {
                        composition: grid.composition(GridVertexId(vertex_index)).ok(),
                        value: Some(estimate.value),
                        iteration: Some(layer as usize),
                        ..TraceDecision::default()
                    },
                );
                accepted.push(estimate);
            }
            Err(reason) => {
                emit(
                    trace,
                    NumericalTraceEventKind::ExtrapolationDirectionRejected,
                    TraceDecision {
                        composition: grid.composition(GridVertexId(vertex_index)).ok(),
                        iteration: Some(layer as usize),
                        reason: Some(reason.to_string()),
                        ..TraceDecision::default()
                    },
                );
                reasons.push((direction, reason));
            }
        }
    }
    if accepted.is_empty() {
        reasons.push((
            ExtrapolationDirection::ConstantCPositive,
            ExtrapolationRejection::NoSafeDirection,
        ));
        emit(
            trace,
            NumericalTraceEventKind::ExtrapolationVertexRejected,
            TraceDecision {
                composition: grid.composition(GridVertexId(vertex_index)).ok(),
                iteration: Some(layer as usize),
                reason: Some(ExtrapolationRejection::NoSafeDirection.to_string()),
                ..TraceDecision::default()
            },
        );
        return Err(RejectedExtrapolationVertex {
            vertex_index,
            layer,
            reasons,
        });
    }
    accepted.sort_by_key(|estimate| estimate.direction);
    let minimum = accepted
        .iter()
        .map(|estimate| estimate.value)
        .fold(f64::INFINITY, f64::min);
    let maximum = accepted
        .iter()
        .map(|estimate| estimate.value)
        .fold(f64::NEG_INFINITY, f64::max);
    let spread = maximum - minimum;
    if options
        .maximum_directional_spread
        .is_some_and(|maximum| spread > maximum)
    {
        reasons.push((
            ExtrapolationDirection::ConstantCPositive,
            ExtrapolationRejection::DirectionalSpreadExceeded,
        ));
        emit(
            trace,
            NumericalTraceEventKind::ExtrapolationVertexRejected,
            TraceDecision {
                composition: grid.composition(GridVertexId(vertex_index)).ok(),
                iteration: Some(layer as usize),
                reason: Some(ExtrapolationRejection::DirectionalSpreadExceeded.to_string()),
                ..TraceDecision::default()
            },
        );
        return Err(RejectedExtrapolationVertex {
            vertex_index,
            layer,
            reasons,
        });
    }
    let value = combine_estimates(&accepted);
    emit(
        trace,
        NumericalTraceEventKind::ExtrapolationVertexAccepted,
        TraceDecision {
            composition: grid.composition(GridVertexId(vertex_index)).ok(),
            value: Some(value),
            iteration: Some(layer as usize),
            ..TraceDecision::default()
        },
    );
    Ok(RegularMeshExtrapolatedValue {
        vertex_index,
        value,
        layer,
        method: options.method,
        directional_support_count: accepted.len(),
        directional_estimates: accepted,
        spread,
    })
}

fn estimate_direction(
    grid: RegularTernaryGrid,
    values: &[Option<f64>],
    target: LatticeCoordinate,
    direction: ExtrapolationDirection,
    options: RegularMeshExtrapolationOptions,
) -> Result<DirectionalEstimate, ExtrapolationRejection> {
    let required = options.minimum_directional_support.max(3);
    let mut coordinate = target;
    let delta = direction.delta();
    let mut support_vertex_indices = Vec::new();
    let mut support_values = Vec::new();
    while let Some(next) = translated(coordinate, delta, grid.subdivisions()) {
        coordinate = next;
        let index = grid
            .vertex_id(coordinate)
            .expect("translated coordinate stays on this regular grid")
            .0;
        let Some(value) = values[index] else {
            break;
        };
        support_vertex_indices.push(index);
        support_values.push(value);
    }
    if support_values.len() < required {
        return Err(ExtrapolationRejection::InsufficientContiguousSupport {
            found: support_values.len(),
            required,
        });
    }
    let coefficients = cubic_left_coefficients(
        options.method,
        support_values[0],
        support_values[1],
        support_values[2],
    );
    // The cubic interval begins at the nearest support at x=1.  Evaluate it at
    // x=0, exactly one regular step beyond that endpoint.
    let value = evaluate_cubic(coefficients, -1.0);
    if !value.is_finite() {
        return Err(ExtrapolationRejection::NonFiniteEstimate);
    }
    if options.minimum_value.is_some_and(|minimum| value < minimum)
        || options.maximum_value.is_some_and(|maximum| value > maximum)
    {
        return Err(ExtrapolationRejection::ValueOutsideBounds);
    }
    let endpoint_slope = derivative_cubic(coefficients, -1.0).abs();
    let secant_one = (support_values[1] - support_values[0]).abs();
    let secant_two = (support_values[2] - support_values[1]).abs();
    let slope_scale = secant_one.max(secant_two).max(f64::MIN_POSITIVE);
    if options
        .maximum_endpoint_slope_growth
        .is_some_and(|maximum| endpoint_slope > maximum * slope_scale)
    {
        return Err(ExtrapolationRejection::EndpointSlopeGrowth);
    }
    let endpoint_curvature = second_derivative_cubic(coefficients, -1.0).abs();
    let curvature_scale = (secant_two - secant_one).abs().max(f64::MIN_POSITIVE);
    if options
        .maximum_endpoint_curvature_growth
        .is_some_and(|maximum| endpoint_curvature > maximum * curvature_scale)
    {
        return Err(ExtrapolationRejection::EndpointCurvatureGrowth);
    }
    Ok(DirectionalEstimate {
        direction,
        support_vertex_indices,
        support_values,
        value,
    })
}

fn translated(
    coordinate: LatticeCoordinate,
    delta: [isize; 3],
    subdivisions: usize,
) -> Option<LatticeCoordinate> {
    let i = coordinate.i.checked_add_signed(delta[0])?;
    let j = coordinate.j.checked_add_signed(delta[1])?;
    let k = coordinate.k.checked_add_signed(delta[2])?;
    (i + j + k == subdivisions).then_some(LatticeCoordinate { i, j, k })
}

fn cubic_left_coefficients(
    method: CubicAlphaMethod,
    first: f64,
    second: f64,
    third: f64,
) -> [f64; 4] {
    let interpolation = crate::interpolation::cubic_method_kind(method);
    spline1d::cubic_single_left(interpolation, 1.0, first, 2.0, second, 3.0, third)
}

fn evaluate_cubic([a, b, c, d]: [f64; 4], dx: f64) -> f64 {
    ((a * dx + b) * dx + c) * dx + d
}

fn derivative_cubic([a, b, c, _]: [f64; 4], dx: f64) -> f64 {
    (3.0 * a * dx + 2.0 * b) * dx + c
}

fn second_derivative_cubic([a, b, _, _]: [f64; 4], dx: f64) -> f64 {
    6.0 * a * dx + 2.0 * b
}

fn combine_estimates(estimates: &[DirectionalEstimate]) -> f64 {
    match estimates {
        [one] => one.value,
        [first, second] => (first.value + second.value) / 2.0,
        many => {
            // Values have already been sorted by stable ray ID. Sort a small
            // copy by value for a deterministic robust trimmed mean.
            let mut sorted = many
                .iter()
                .map(|estimate| estimate.value)
                .collect::<Vec<_>>();
            sorted.sort_by(f64::total_cmp);
            let trimmed = &sorted[1..sorted.len() - 1];
            trimmed.iter().copied().sum::<f64>() / trimmed.len() as f64
        }
    }
}

fn validate_options(
    options: RegularMeshExtrapolationOptions,
) -> Result<(), RegularMeshExtrapolationError> {
    if options.maximum_layers == 0 {
        return Err(RegularMeshExtrapolationError::InvalidOptions {
            message: "maximum_layers must be at least one",
        });
    }
    if options.minimum_directional_support == 0 {
        return Err(RegularMeshExtrapolationError::InvalidOptions {
            message: "minimum_directional_support must be at least one",
        });
    }
    let finite_nonnegative =
        |value: Option<f64>| value.is_none_or(|value| value.is_finite() && value >= 0.0);
    if !finite_nonnegative(options.maximum_directional_spread)
        || !finite_nonnegative(options.maximum_endpoint_slope_growth)
        || !finite_nonnegative(options.maximum_endpoint_curvature_growth)
    {
        return Err(RegularMeshExtrapolationError::InvalidOptions {
            message: "nonnegative extrapolation guards must be finite",
        });
    }
    if options
        .minimum_value
        .is_some_and(|value| !value.is_finite())
        || options
            .maximum_value
            .is_some_and(|value| !value.is_finite())
        || options
            .minimum_value
            .zip(options.maximum_value)
            .is_some_and(|(minimum, maximum)| minimum > maximum)
    {
        return Err(RegularMeshExtrapolationError::InvalidOptions {
            message: "scalar bounds must be finite and ordered",
        });
    }
    Ok(())
}

fn emit(
    trace: &mut NumericalTraceSession<'_>,
    kind: NumericalTraceEventKind,
    detail: TraceDecision,
) {
    trace.emit(
        NumericalTraceLevel::Decisions,
        crate::NumericalTraceStage::Extrapolation,
        decision(kind, detail),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> RegularTernaryGrid {
        RegularTernaryGrid::new(4).unwrap()
    }

    #[test]
    fn pure_corner_has_exactly_two_inward_rays() {
        let grid = grid();
        let pb = grid
            .vertex_id(LatticeCoordinate { i: 0, j: 4, k: 0 })
            .unwrap()
            .0;
        let values = vec![Some(1.0); grid.vertex_count()];
        let rays = ExtrapolationDirection::ALL
            .into_iter()
            .filter(|direction| {
                estimate_direction(
                    grid,
                    &values,
                    grid.lattice_coordinate(GridVertexId(pb)).unwrap(),
                    *direction,
                    RegularMeshExtrapolationOptions::default(),
                )
                .is_ok()
            })
            .count();
        assert_eq!(rays, 2);
    }

    #[test]
    fn cubic_extrapolation_is_synchronous_and_deterministic() {
        let grid = grid();
        let mut values = grid
            .compositions()
            .map(|[a, b, c]| Some(800.0 + 100.0 * a + 30.0 * b + 10.0 * c))
            .collect::<Vec<_>>();
        let target = grid
            .vertex_id(LatticeCoordinate { i: 0, j: 4, k: 0 })
            .unwrap()
            .0;
        values[target] = None;
        let eligible = values.iter().map(Option::is_none).collect::<Vec<_>>();
        let first = extrapolate_regular_mesh(
            grid,
            &values,
            &eligible,
            RegularMeshExtrapolationOptions::default(),
        )
        .unwrap();
        let second = extrapolate_regular_mesh(
            grid,
            &values,
            &eligible,
            RegularMeshExtrapolationOptions::default(),
        )
        .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.values.len(), 1);
        assert!((first.values[0].value - 830.0).abs() < 1.0e-9);
        assert_eq!(first.values[0].layer, 1);
    }

    #[test]
    fn ineligible_missing_cells_are_barriers() {
        let grid = grid();
        let mut values = vec![Some(1.0); grid.vertex_count()];
        let target = grid
            .vertex_id(LatticeCoordinate { i: 0, j: 4, k: 0 })
            .unwrap()
            .0;
        values[target] = None;
        let eligible = vec![false; grid.vertex_count()];
        let result = extrapolate_regular_mesh(
            grid,
            &values,
            &eligible,
            RegularMeshExtrapolationOptions::default(),
        )
        .unwrap();
        assert!(result.values.is_empty());
    }
}
