//! Cached source-field evaluation for native Grid inspection.
//!
//! This module owns no GUI projection formula. Regular queries use the
//! prepared numerical partial-field evaluator; irregular linear queries reuse
//! the tabulated evaluator used by stable projection construction.

use ternary_contours::{
    CubicPartialDomainPolicy, FieldInterpolation, InterpolatedPartialTernaryField,
    InterpolationInspectionState, IrregularTernaryMesh, LocalInterpolationMode,
    RegularTernaryPartialScalarField, StablePhaseId, StablePhaseUndefinedReason,
};

use crate::{
    ProjectionOptions, SourceInterpolation, TabulatedField, TabulatedGrid, TabulatedTernaryDataset,
    TabulatedValue, TabulatedValueState, projection::interpolate_tabulated,
};

/// Stable identity of the source field selected in Grid inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectionFieldIdentity {
    pub grid_index: usize,
    pub phase_id: StablePhaseId,
    pub property: String,
}

/// Typed result state retained by every interpolation query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InterpolatedResultState {
    Defined,
    UndefinedMissing,
    UndefinedNonExisting,
    UndefinedCutOff,
    TriangleUnavailable,
    OutsideDomain,
    Error(String),
}

impl InterpolatedResultState {
    pub fn label(&self) -> &str {
        match self {
            Self::Defined => "Defined",
            Self::UndefinedMissing => "Undefined: Missing domain",
            Self::UndefinedNonExisting => "Undefined: Non-existing",
            Self::UndefinedCutOff => "Undefined: Cut-off",
            Self::TriangleUnavailable => "Undefined: Triangle unavailable",
            Self::OutsideDomain => "Undefined: Outside source domain",
            Self::Error(_) => "Evaluation error",
        }
    }

    pub fn value_token(&self) -> &'static str {
        match self {
            Self::UndefinedNonExisting => "NE",
            Self::UndefinedCutOff => "CO",
            Self::Defined
            | Self::UndefinedMissing
            | Self::TriangleUnavailable
            | Self::OutsideDomain
            | Self::Error(_) => "NA",
        }
    }
}

/// One persisted, reproducible interpolation query.
#[derive(Clone, Debug)]
pub struct InterpolatedResult {
    pub index: usize,
    pub id: u64,
    pub field: InspectionFieldIdentity,
    pub grid_name: String,
    pub phase_name: String,
    pub component_names: [String; 3],
    pub unit: String,
    pub composition: [f64; 3],
    pub source_interpolation: SourceInterpolation,
    pub partial_domain_policy: CubicPartialDomainPolicy,
    pub state: InterpolatedResultState,
    pub value: Option<f64>,
    pub triangle_index: Option<usize>,
    pub local_barycentric: Option<[f64; 3]>,
    /// Zero-based canonical source-array indices in the same order as lambda.
    pub triangle_vertex_indices: Option<[usize; 3]>,
    pub linear_part: Option<f64>,
    pub excess_part: Option<f64>,
    pub local_mode: Option<LocalInterpolationMode>,
    pub stale: bool,
    pub stale_error: Option<String>,
}

impl InterpolatedResult {
    pub fn method_label(&self) -> String {
        match self.source_interpolation {
            SourceInterpolation::Linear => "Linear".into(),
            SourceInterpolation::CubicAlpha {
                method,
                continuation,
            } => format!(
                "Cubic alpha ({method:?}, {continuation:?}; {:?})",
                self.partial_domain_policy
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct CacheKey {
    field: InspectionFieldIdentity,
    grid: TabulatedGrid,
    values: Vec<TabulatedValue>,
    source_interpolation: SourceInterpolation,
    partial_domain_policy: CubicPartialDomainPolicy,
}

enum PreparedInspectionField {
    Regular(InterpolatedPartialTernaryField),
    Irregular {
        mesh: Box<IrregularTernaryMesh>,
        values: Vec<TabulatedValue>,
    },
    Unavailable(String),
}

/// One cached selected evaluator. It is rebuilt only when the selected source
/// field, its draft values, or the shared interpolation settings change.
#[derive(Default)]
pub struct FieldInspectionCache {
    key: Option<CacheKey>,
    prepared: Option<PreparedInspectionField>,
}

impl FieldInspectionCache {
    pub fn invalidate(&mut self) {
        self.key = None;
        self.prepared = None;
    }
    pub fn preparation_error(&self) -> Option<&str> {
        match self.prepared.as_ref() {
            Some(PreparedInspectionField::Unavailable(message)) => Some(message),
            Some(PreparedInspectionField::Regular(_))
            | Some(PreparedInspectionField::Irregular { .. })
            | None => None,
        }
    }

    pub fn evaluate(
        &mut self,
        dataset: &TabulatedTernaryDataset,
        identity: &InspectionFieldIdentity,
        options: &ProjectionOptions,
        composition: [f64; 3],
        index: usize,
        id: u64,
    ) -> InterpolatedResult {
        let Some((grid, field)) = dataset.grids.get(identity.grid_index).and_then(|grid| {
            grid.fields()
                .iter()
                .find(|field| {
                    field.phase_id == identity.phase_id && field.property == identity.property
                })
                .map(|field| (grid, field))
        }) else {
            return error_result(
                dataset,
                identity,
                composition,
                index,
                id,
                "selected field no longer exists",
            );
        };
        let key = CacheKey {
            field: identity.clone(),
            grid: grid.clone(),
            values: field.values.clone(),
            source_interpolation: options.source_interpolation,
            partial_domain_policy: options.partial_domain_policy,
        };
        if self.key.as_ref() != Some(&key) {
            self.prepared = Some(prepare(grid, field, options));
            self.key = Some(key);
        }
        let (grid_name, phase_name, unit, component_names) = metadata(dataset, grid, field);
        let mut result = InterpolatedResult {
            index,
            id,
            field: identity.clone(),
            grid_name,
            phase_name,
            component_names,
            unit,
            composition,
            source_interpolation: options.source_interpolation,
            partial_domain_policy: options.partial_domain_policy,
            state: InterpolatedResultState::TriangleUnavailable,
            value: None,
            triangle_index: None,
            local_barycentric: None,
            triangle_vertex_indices: None,
            linear_part: None,
            excess_part: None,
            stale_error: None,
            local_mode: None,
            stale: false,
        };
        match self.prepared.as_ref().expect("cache prepared with key") {
            PreparedInspectionField::Regular(evaluator) => match evaluator.inspect(composition) {
                Ok(inspection) => {
                    result.triangle_index = Some(inspection.triangle_index);
                    result.local_barycentric = Some(inspection.local_barycentric);
                    result.triangle_vertex_indices = inspection.triangle_vertex_indices;
                    result.local_mode = Some(inspection.local_mode);
                    result.linear_part = inspection.linear_part;
                    result.excess_part = inspection.excess_part;
                    result.value = inspection.value;
                    result.state = match inspection.state {
                        InterpolationInspectionState::Defined => InterpolatedResultState::Defined,
                        InterpolationInspectionState::UndefinedTriangle => {
                            state_for_triangle(&field.values, inspection.triangle_vertex_indices)
                        }
                    };
                }
                Err(error) => result.state = InterpolatedResultState::Error(error.to_string()),
            },
            PreparedInspectionField::Irregular { mesh, values } => match mesh.locate(composition) {
                Ok(location) => {
                    result.triangle_index = Some(location.triangle.id.0);
                    result.local_barycentric = Some(location.barycentric);
                    result.triangle_vertex_indices =
                        Some(location.triangle.vertices.map(|vertex| vertex.0));
                    result.local_mode = Some(LocalInterpolationMode::Linear);
                    match interpolate_tabulated(
                        values,
                        location
                            .triangle
                            .vertices
                            .into_iter()
                            .zip(location.barycentric)
                            .map(|(vertex, weight)| (vertex.0, weight)),
                    ) {
                        Ok(value) if value.is_finite() => {
                            result.state = InterpolatedResultState::Defined;
                            result.value = Some(value);
                            result.linear_part = Some(value);
                            result.excess_part = Some(0.0);
                        }
                        Ok(_) => {
                            result.state = InterpolatedResultState::Error(
                                "interpolation produced a non-finite scalar".into(),
                            )
                        }
                        Err(reason) => result.state = state_from_reason(reason),
                    }
                }
                Err(_) => result.state = InterpolatedResultState::OutsideDomain,
            },
            PreparedInspectionField::Unavailable(message) => {
                result.state = InterpolatedResultState::Error(message.clone());
            }
        }
        result
    }
}

fn prepare(
    grid: &TabulatedGrid,
    field: &TabulatedField,
    options: &ProjectionOptions,
) -> PreparedInspectionField {
    match grid {
        TabulatedGrid::Regular(grid) => {
            let values = field
                .values
                .iter()
                .map(TabulatedValue::calculated_value)
                .collect::<Vec<_>>();
            let source = match RegularTernaryPartialScalarField::new(grid.subdivisions, values) {
                Ok(source) => source,
                Err(error) => return PreparedInspectionField::Unavailable(error.to_string()),
            };
            let interpolation = match options.source_interpolation {
                SourceInterpolation::Linear => FieldInterpolation::Linear,
                source @ SourceInterpolation::CubicAlpha { .. } => {
                    let mut cubic = source
                        .cubic_options()
                        .expect("cubic interpolation has options");
                    cubic.partial_domain_policy = options.partial_domain_policy;
                    FieldInterpolation::CubicAlpha(cubic)
                }
            };
            match InterpolatedPartialTernaryField::new(source, interpolation) {
                Ok(evaluator) => PreparedInspectionField::Regular(evaluator),
                Err(error) => PreparedInspectionField::Unavailable(error.to_string()),
            }
        }
        TabulatedGrid::Irregular(grid) => {
            if !matches!(options.source_interpolation, SourceInterpolation::Linear) {
                return PreparedInspectionField::Unavailable(
                    "cubic alpha is not available for irregular source fields in this viewer build"
                        .into(),
                );
            }
            match IrregularTernaryMesh::new(grid.compositions.iter().copied()) {
                Ok(mesh) => PreparedInspectionField::Irregular {
                    mesh: Box::new(mesh),
                    values: field.values.clone(),
                },
                Err(error) => PreparedInspectionField::Unavailable(error.to_string()),
            }
        }
    }
}

fn state_for_triangle(
    values: &[TabulatedValue],
    vertices: Option<[usize; 3]>,
) -> InterpolatedResultState {
    let states = vertices
        .into_iter()
        .flatten()
        .filter_map(|index| values.get(index).map(|value| value.state))
        .collect::<Vec<_>>();
    if states.contains(&TabulatedValueState::Missing) {
        InterpolatedResultState::UndefinedMissing
    } else if states.contains(&TabulatedValueState::CutOff) {
        InterpolatedResultState::UndefinedCutOff
    } else {
        InterpolatedResultState::TriangleUnavailable
    }
}

fn state_from_reason(reason: StablePhaseUndefinedReason) -> InterpolatedResultState {
    match reason {
        StablePhaseUndefinedReason::ClassifiedNonExisting => {
            InterpolatedResultState::UndefinedNonExisting
        }
        StablePhaseUndefinedReason::ClassifiedCutOff => InterpolatedResultState::UndefinedCutOff,
        StablePhaseUndefinedReason::MissingTabulatedInput => {
            InterpolatedResultState::UndefinedMissing
        }
        StablePhaseUndefinedReason::OutsidePhaseDomain => InterpolatedResultState::OutsideDomain,
        _ => InterpolatedResultState::TriangleUnavailable,
    }
}

fn metadata(
    dataset: &TabulatedTernaryDataset,
    grid: &TabulatedGrid,
    field: &TabulatedField,
) -> (String, String, String, [String; 3]) {
    let phase = dataset
        .phases
        .iter()
        .find(|phase| phase.id == field.phase_id)
        .map(|phase| phase.name.clone())
        .unwrap_or_else(|| format!("Phase {}", field.phase_id.0));
    let unit = dataset
        .properties
        .iter()
        .find(|property| property.name == field.property)
        .map(|property| property.unit.clone())
        .unwrap_or_default();
    (
        grid.name().to_owned(),
        phase,
        unit,
        dataset.components.clone().map(|component| component.name),
    )
}

fn error_result(
    dataset: &TabulatedTernaryDataset,
    identity: &InspectionFieldIdentity,
    composition: [f64; 3],
    index: usize,
    id: u64,
    message: &str,
) -> InterpolatedResult {
    InterpolatedResult {
        id,
        index,
        field: identity.clone(),
        grid_name: dataset
            .grids
            .get(identity.grid_index)
            .map(|grid| grid.name().to_owned())
            .unwrap_or_else(|| "unknown".into()),
        phase_name: format!("Phase {}", identity.phase_id.0),
        component_names: dataset.components.clone().map(|component| component.name),
        unit: String::new(),
        composition,
        stale_error: None,
        source_interpolation: SourceInterpolation::Linear,
        partial_domain_policy: CubicPartialDomainPolicy::OneSidedThenLinear,
        state: InterpolatedResultState::Error(message.into()),
        value: None,
        triangle_index: None,
        local_barycentric: None,
        triangle_vertex_indices: None,
        linear_part: None,
        excess_part: None,
        local_mode: None,
        stale: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn populated_regular_dataset() -> TabulatedTernaryDataset {
        let mut dataset = crate::default_regular_dataset();
        let TabulatedGrid::Regular(grid) = &mut dataset.grids[0] else {
            unreachable!("default dataset uses a regular grid");
        };
        for (index, value) in grid.fields[0].values.iter_mut().enumerate() {
            *value = TabulatedValue::calculated(1_000.0 + index as f64).unwrap();
        }
        dataset
    }

    #[test]
    fn regular_queries_retain_canonical_triangle_rows_in_lambda_order() {
        let dataset = populated_regular_dataset();
        let identity = InspectionFieldIdentity {
            grid_index: 0,
            phase_id: StablePhaseId(1),
            property: "T".into(),
        };
        let point = [0.63, 0.17, 0.20];
        let mut cache = FieldInspectionCache::default();
        let first = cache.evaluate(
            &dataset,
            &identity,
            &ProjectionOptions::default(),
            point,
            1,
            1,
        );
        let second = cache.evaluate(
            &dataset,
            &identity,
            &ProjectionOptions::default(),
            point,
            1,
            1,
        );
        let grid = ternary_contours::RegularTernaryGrid::new(10).unwrap();
        let location = grid.locate(point).unwrap();
        let expected = location.triangle.vertices.map(|vertex| vertex.0);
        assert_eq!(first.triangle_vertex_indices, Some(expected));
        assert_eq!(second.triangle_vertex_indices, Some(expected));
        assert_eq!(first.local_barycentric, Some(location.barycentric));
        assert!(
            first
                .triangle_vertex_indices
                .unwrap()
                .into_iter()
                .all(|index| index < grid.vertex_count())
        );
        assert!((first.local_barycentric.unwrap().into_iter().sum::<f64>() - 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn irregular_queries_keep_loaded_source_indices_stable() {
        let dataset =
            crate::parse_str(include_str!("../../fixtures/irregular-phase-grids.tct")).unwrap();
        let identity = InspectionFieldIdentity {
            grid_index: 0,
            phase_id: StablePhaseId(10),
            property: "T".into(),
        };
        let point = [0.20, 0.30, 0.50];
        let mut cache = FieldInspectionCache::default();
        let first = cache.evaluate(
            &dataset,
            &identity,
            &ProjectionOptions::default(),
            point,
            1,
            1,
        );
        let second = cache.evaluate(
            &dataset,
            &identity,
            &ProjectionOptions::default(),
            point,
            1,
            1,
        );
        assert_eq!(first.state, InterpolatedResultState::Defined);
        assert_eq!(
            first.triangle_vertex_indices,
            second.triangle_vertex_indices
        );
        assert_eq!(first.local_barycentric, second.local_barycentric);
        let vertices = first.triangle_vertex_indices.unwrap();
        let grid = &dataset.grids[0];
        assert!(
            vertices
                .into_iter()
                .all(|index| index < grid.compositions().len())
        );
    }
}
