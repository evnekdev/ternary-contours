use crate::{
    FieldEvaluationError, GridVertexId, InterpolatedTernaryField, LocatedTriangle,
    RegularTernaryGrid,
};
#[cfg(feature = "irregular-delaunay")]
use crate::{
    InterpolatedIrregularTernaryField, IrregularFieldEvaluationError, IrregularTriangleId,
    LocatedIrregularTriangle,
};

use super::{
    StableContourDiagnostics, StableContourError, StableContourQuantity, StablePhaseId,
    StablePhaseSource, StableSourceEvaluationError,
    source::{ScalarRole, SourceGeometryKey},
};

pub(crate) enum PreparedSourceEvaluator<'a> {
    Regular(InterpolatedTernaryField<'a>),
    #[cfg(feature = "irregular-delaunay")]
    Irregular(InterpolatedIrregularTernaryField<'a>),
}

pub(crate) struct PreparedSourceLayer<'a> {
    pub phase_index: usize,
    pub phase: StablePhaseId,
    pub role: ScalarRole,
    pub geometry: SourceGeometryKey<'a>,
    pub evaluator: PreparedSourceEvaluator<'a>,
}

pub(crate) struct SourceGeometryGroup<'a> {
    pub geometry: SourceGeometryKey<'a>,
    pub layers: Vec<usize>,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct SourceLocationHint {
    #[cfg(feature = "irregular-delaunay")]
    irregular: Option<IrregularTriangleId>,
}

pub(crate) struct UmbrellaSamples {
    pub grid: RegularTernaryGrid,
    pub phase_count: usize,
    heights: Vec<f64>,
    secondary: Option<Vec<f64>>,
}

impl UmbrellaSamples {
    pub fn vertex_count(&self) -> usize {
        self.grid.vertex_count()
    }

    pub fn height(&self, phase: usize, vertex: GridVertexId) -> f64 {
        self.heights[phase * self.vertex_count() + vertex.0]
    }

    pub fn secondary(&self, phase: usize, vertex: GridVertexId) -> Option<f64> {
        self.secondary
            .as_ref()
            .map(|values| values[phase * self.vertex_count() + vertex.0])
    }

    pub fn triangle_height_values(&self, phase: usize, vertices: [GridVertexId; 3]) -> [f64; 3] {
        vertices.map(|vertex| self.height(phase, vertex))
    }

    pub fn triangle_secondary_values(
        &self,
        phase: usize,
        vertices: [GridVertexId; 3],
    ) -> Option<[f64; 3]> {
        self.secondary
            .as_ref()
            .map(|_| vertices.map(|vertex| self.secondary(phase, vertex).unwrap()))
    }

    pub fn affine_heights(&self, vertices: [GridVertexId; 3], barycentric: [f64; 3]) -> Vec<f64> {
        (0..self.phase_count)
            .map(|phase| dot(self.triangle_height_values(phase, vertices), barycentric))
            .collect()
    }

    pub fn affine_secondary(
        &self,
        vertices: [GridVertexId; 3],
        barycentric: [f64; 3],
    ) -> Option<Vec<f64>> {
        self.secondary.as_ref().map(|_| {
            (0..self.phase_count)
                .map(|phase| {
                    dot(
                        self.triangle_secondary_values(phase, vertices).unwrap(),
                        barycentric,
                    )
                })
                .collect()
        })
    }
}

pub(crate) fn prepare_sources<'a>(
    phases: &[StablePhaseSource<'a>],
    quantity: StableContourQuantity,
    diagnostics: &mut StableContourDiagnostics,
) -> Result<(Vec<PreparedSourceLayer<'a>>, Vec<SourceGeometryGroup<'a>>), StableContourError> {
    let mut layers = Vec::with_capacity(match quantity {
        StableContourQuantity::Height => phases.len(),
        StableContourQuantity::Secondary => phases.len().saturating_mul(2),
    });
    for (phase_index, phase) in phases.iter().enumerate() {
        layers.push(prepare_layer(
            phase_index,
            phase.phase(),
            ScalarRole::Height,
            phase.height(),
        )?);
        if quantity == StableContourQuantity::Secondary {
            let secondary =
                phase
                    .secondary()
                    .ok_or(StableContourError::MissingSecondaryScalar {
                        phase: phase.phase(),
                    })?;
            layers.push(prepare_layer(
                phase_index,
                phase.phase(),
                ScalarRole::Secondary,
                secondary,
            )?);
        }
    }

    let mut groups: Vec<SourceGeometryGroup<'a>> = Vec::new();
    for (layer_index, layer) in layers.iter().enumerate() {
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group.geometry.matches(layer.geometry))
        {
            group.layers.push(layer_index);
        } else {
            groups.push(SourceGeometryGroup {
                geometry: layer.geometry,
                layers: vec![layer_index],
            });
        }
    }
    diagnostics.source_scalar_layer_count = layers.len();
    diagnostics.geometry_group_count = groups.len();
    diagnostics.regular_geometry_group_count = groups
        .iter()
        .filter(|group| group.geometry.is_regular())
        .count();
    diagnostics.irregular_geometry_group_count =
        groups.len() - diagnostics.regular_geometry_group_count;
    Ok((layers, groups))
}

fn prepare_layer<'a>(
    phase_index: usize,
    phase: StablePhaseId,
    role: ScalarRole,
    source: super::StableScalarSource<'a>,
) -> Result<PreparedSourceLayer<'a>, StableContourError> {
    let geometry = source.geometry_key();
    let evaluator = match source {
        super::StableScalarSource::Regular {
            field,
            interpolation,
        } => InterpolatedTernaryField::new(field, interpolation)
            .map(PreparedSourceEvaluator::Regular)
            .map_err(|error| {
                preparation_error(phase, role, StableSourceEvaluationError::Regular(error))
            })?,
        #[cfg(feature = "irregular-delaunay")]
        super::StableScalarSource::Irregular {
            field,
            interpolation,
        } => InterpolatedIrregularTernaryField::new(field, interpolation)
            .map(PreparedSourceEvaluator::Irregular)
            .map_err(|error| {
                preparation_error(phase, role, StableSourceEvaluationError::Irregular(error))
            })?,
    };
    Ok(PreparedSourceLayer {
        phase_index,
        phase,
        role,
        geometry,
        evaluator,
    })
}

fn preparation_error(
    phase: StablePhaseId,
    role: ScalarRole,
    source: StableSourceEvaluationError,
) -> StableContourError {
    let quantity = role_quantity(role);
    match &source {
        StableSourceEvaluationError::Regular(FieldEvaluationError::CubicFeatureUnavailable) => {
            StableContourError::UnsupportedSourceFeature {
                phase,
                quantity,
                feature: "cubic-alpha",
            }
        }
        #[cfg(feature = "irregular-delaunay")]
        StableSourceEvaluationError::Irregular(
            IrregularFieldEvaluationError::CubicFeatureUnavailable,
        ) => StableContourError::UnsupportedSourceFeature {
            phase,
            quantity,
            feature: "irregular-cubic-alpha",
        },
        _ => StableContourError::SourcePreparation {
            phase,
            quantity,
            source: Box::new(source),
        },
    }
}

pub(crate) fn sample_umbrella(
    grid: RegularTernaryGrid,
    phase_count: usize,
    quantity: StableContourQuantity,
    layers: &[PreparedSourceLayer<'_>],
    groups: &[SourceGeometryGroup<'_>],
    diagnostics: &mut StableContourDiagnostics,
) -> Result<UmbrellaSamples, StableContourError> {
    let sample_count = phase_count
        .checked_mul(grid.vertex_count())
        .ok_or(StableContourError::UmbrellaSubdivisionOverflow)?;
    let mut heights = vec![0.0; sample_count];
    let mut secondary =
        (quantity == StableContourQuantity::Secondary).then(|| vec![0.0; sample_count]);
    let mut scratch_heights = vec![0.0; phase_count];
    let mut scratch_secondary = secondary.as_ref().map(|_| vec![0.0; phase_count]);
    let mut hints = vec![SourceLocationHint::default(); groups.len()];

    for (vertex, composition) in grid.indexed_compositions() {
        evaluate_sources_at_point(
            layers,
            groups,
            composition,
            &mut hints,
            &mut scratch_heights,
            scratch_secondary.as_deref_mut(),
            diagnostics,
            Some(vertex),
        )?;
        for phase in 0..phase_count {
            heights[phase * grid.vertex_count() + vertex.0] = scratch_heights[phase];
            if let (Some(values), Some(scratch)) = (&mut secondary, &scratch_secondary) {
                values[phase * grid.vertex_count() + vertex.0] = scratch[phase];
            }
        }
    }
    diagnostics.sampled_scalar_values = sample_count
        .checked_mul(if secondary.is_some() { 2 } else { 1 })
        .ok_or(StableContourError::UmbrellaSubdivisionOverflow)?;
    Ok(UmbrellaSamples {
        grid,
        phase_count,
        heights,
        secondary,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn evaluate_sources_at_point(
    layers: &[PreparedSourceLayer<'_>],
    groups: &[SourceGeometryGroup<'_>],
    composition: [f64; 3],
    hints: &mut [SourceLocationHint],
    heights: &mut [f64],
    mut secondary: Option<&mut [f64]>,
    diagnostics: &mut StableContourDiagnostics,
    coverage_vertex: Option<GridVertexId>,
) -> Result<(), StableContourError> {
    #[cfg(not(feature = "irregular-delaunay"))]
    let _ = coverage_vertex;
    for (group_index, group) in groups.iter().enumerate() {
        let _hint = &mut hints[group_index];
        diagnostics.source_point_location_count += 1;
        diagnostics.source_scalar_evaluation_count += group.layers.len();
        diagnostics.reused_source_locations += group.layers.len().saturating_sub(1);
        match group.geometry {
            SourceGeometryKey::Regular(_, _) => {
                let first = &layers[group.layers[0]];
                #[cfg(feature = "irregular-delaunay")]
                let grid = match &first.evaluator {
                    PreparedSourceEvaluator::Regular(evaluator) => evaluator.field().grid(),
                    PreparedSourceEvaluator::Irregular(_) => unreachable!(),
                };
                #[cfg(not(feature = "irregular-delaunay"))]
                let grid = {
                    let PreparedSourceEvaluator::Regular(evaluator) = &first.evaluator;
                    evaluator.field().grid()
                };
                let location = grid.locate(composition).map_err(|error| {
                    evaluation_error(
                        first,
                        composition,
                        StableSourceEvaluationError::Regular(error.into()),
                    )
                })?;
                evaluate_regular_group(
                    layers,
                    group,
                    &location,
                    composition,
                    heights,
                    secondary.as_deref_mut(),
                )?;
            }
            #[cfg(feature = "irregular-delaunay")]
            SourceGeometryKey::Irregular(mesh) => {
                let location = match mesh.locate_with_hint(composition, _hint.irregular) {
                    Ok(location) => location,
                    Err(error) => {
                        let first = &layers[group.layers[0]];
                        if let Some(umbrella_vertex) = coverage_vertex {
                            return Err(StableContourError::IncompleteSourceCoverage {
                                phase: first.phase,
                                umbrella_vertex,
                                composition,
                            });
                        }
                        return Err(evaluation_error(
                            first,
                            composition,
                            StableSourceEvaluationError::Irregular(error.into()),
                        ));
                    }
                };
                _hint.irregular = Some(location.triangle.id);
                evaluate_irregular_group(
                    layers,
                    group,
                    &location,
                    composition,
                    heights,
                    secondary.as_deref_mut(),
                )?;
            }
        }
    }
    Ok(())
}

fn evaluate_regular_group(
    layers: &[PreparedSourceLayer<'_>],
    group: &SourceGeometryGroup<'_>,
    location: &LocatedTriangle,
    composition: [f64; 3],
    heights: &mut [f64],
    mut secondary: Option<&mut [f64]>,
) -> Result<(), StableContourError> {
    for &layer_index in &group.layers {
        let layer = &layers[layer_index];
        #[cfg(feature = "irregular-delaunay")]
        let evaluator = match &layer.evaluator {
            PreparedSourceEvaluator::Regular(evaluator) => evaluator,
            PreparedSourceEvaluator::Irregular(_) => unreachable!(),
        };
        #[cfg(not(feature = "irregular-delaunay"))]
        let evaluator = {
            let PreparedSourceEvaluator::Regular(evaluator) = &layer.evaluator;
            evaluator
        };
        let value = evaluator.value_at_location(location).map_err(|error| {
            evaluation_error(
                layer,
                composition,
                StableSourceEvaluationError::Regular(error),
            )
        })?;
        store_value(layer, value, composition, heights, secondary.as_deref_mut())?;
    }
    Ok(())
}

#[cfg(feature = "irregular-delaunay")]
fn evaluate_irregular_group(
    layers: &[PreparedSourceLayer<'_>],
    group: &SourceGeometryGroup<'_>,
    location: &LocatedIrregularTriangle,
    composition: [f64; 3],
    heights: &mut [f64],
    mut secondary: Option<&mut [f64]>,
) -> Result<(), StableContourError> {
    for &layer_index in &group.layers {
        let layer = &layers[layer_index];
        let evaluator = match &layer.evaluator {
            PreparedSourceEvaluator::Irregular(evaluator) => evaluator,
            PreparedSourceEvaluator::Regular(_) => unreachable!(),
        };
        let value = evaluator.value_at_location(location).map_err(|error| {
            evaluation_error(
                layer,
                composition,
                StableSourceEvaluationError::Irregular(error),
            )
        })?;
        store_value(layer, value, composition, heights, secondary.as_deref_mut())?;
    }
    Ok(())
}

fn store_value(
    layer: &PreparedSourceLayer<'_>,
    value: f64,
    composition: [f64; 3],
    heights: &mut [f64],
    secondary: Option<&mut [f64]>,
) -> Result<(), StableContourError> {
    if !value.is_finite() {
        return Err(StableContourError::NonFiniteSourceEvaluation {
            phase: layer.phase,
            quantity: role_quantity(layer.role),
            composition,
        });
    }
    match layer.role {
        ScalarRole::Height => heights[layer.phase_index] = value,
        ScalarRole::Secondary => {
            secondary.expect("secondary storage exists for a secondary layer")[layer.phase_index] =
                value;
        }
    }
    Ok(())
}

fn evaluation_error(
    layer: &PreparedSourceLayer<'_>,
    composition: [f64; 3],
    source: StableSourceEvaluationError,
) -> StableContourError {
    StableContourError::SourceEvaluation {
        phase: layer.phase,
        quantity: role_quantity(layer.role),
        composition,
        source: Box::new(source),
    }
}

pub(crate) const fn role_quantity(role: ScalarRole) -> StableContourQuantity {
    match role {
        ScalarRole::Height => StableContourQuantity::Height,
        ScalarRole::Secondary => StableContourQuantity::Secondary,
    }
}

pub(crate) fn dot(values: [f64; 3], barycentric: [f64; 3]) -> f64 {
    values[0] * barycentric[0] + values[1] * barycentric[1] + values[2] * barycentric[2]
}
