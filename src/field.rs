//! Construction of cubic-alpha fields on a regular ternary lattice.

#[cfg(feature = "cubic-alpha")]
use std::collections::BTreeMap;

use crate::{
    FieldError, RegularTernaryScalarField,
    grid::GridTriangle,
    interpolation::{CubicAlphaBuildOptions, CubicAlphaTriangle},
};
#[cfg(feature = "cubic-alpha")]
use crate::{
    GridVertexId, RegularTernaryPartialScalarField,
    grid::{GridEdgeKey, LatticeCoordinate},
    interpolation::{
        AlphaInterval, CubicAlphaMethod, CubicBoundaryPolicy, CubicPartialDomainPolicy,
        DirectedAlphaInterval, cubic_method_kind,
    },
};

/// Counts produced while deriving shared regular-grid cubic intervals.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CubicBuildDiagnostics {
    /// Number of edges assigned an interval from a three-or-more-sample line.
    pub cubic_edges: usize,
    /// Number of two-sample lines represented by a zero-alpha interval.
    pub linear_fallback_edges: usize,
    /// Number of valid partial-domain edges using a one-sided cubic stencil.
    pub one_sided_edges: usize,
    /// Number of complete cubic-alpha triangles.
    pub cubic_triangles: usize,
    /// Number of triangles with at least one one-sided cubic edge.
    pub one_sided_cubic_triangles: usize,
    /// Number of triangles downgraded to local linear interpolation.
    pub linear_fallback_triangles: usize,
    /// Number of triangles containing an unavailable scalar corner.
    pub undefined_triangles: usize,
}

/// Precomputed local cubic-alpha fields for every regular-grid elementary triangle.
///
/// This is an advanced numerical type. It has no rendering, viewport, or contour
/// path-extraction dependency.
pub struct CubicGridField<'a> {
    field: &'a RegularTernaryScalarField,
    triangles: Vec<GridTriangle>,
    models: Vec<CubicAlphaTriangle>,
    diagnostics: CubicBuildDiagnostics,
}

impl<'a> CubicGridField<'a> {
    /// Build one shared-edge cubic-alpha model for each elementary triangle.
    pub fn new(
        field: &'a RegularTernaryScalarField,
        options: CubicAlphaBuildOptions,
    ) -> Result<Self, FieldError> {
        #[cfg(not(feature = "cubic-alpha"))]
        {
            let _ = (field, options);
            Err(FieldError::CubicFeatureUnavailable)
        }
        #[cfg(feature = "cubic-alpha")]
        {
            let mut diagnostics = CubicBuildDiagnostics::default();
            let intervals = build_edge_intervals(field, options, &mut diagnostics)?;
            let triangles = field.elementary_triangles()?;
            let mut models = Vec::with_capacity(triangles.len());
            for triangle in &triangles {
                let [v0, v1, v2] = triangle.vertices;
                let values = [field.value(v0)?, field.value(v1)?, field.value(v2)?];
                let pairs = [(0, 1), (1, 2), (0, 2)];
                let mut directed = Vec::with_capacity(3);
                for (left, right) in pairs {
                    let key = GridEdgeKey::new(triangle.vertices[left], triangle.vertices[right]);
                    let interval = *intervals
                        .get(&key)
                        .ok_or(crate::interpolation::InterpolationError::MissingEdgePair)?;
                    let (start, end) = if key.start == triangle.vertices[left] {
                        (left, right)
                    } else {
                        (right, left)
                    };
                    directed.push(DirectedAlphaInterval::new(start, end, interval)?);
                }
                models.push(CubicAlphaTriangle::new(
                    values,
                    directed.try_into().expect("three local edge pairs"),
                    options.extrapolation,
                )?);
            }
            Ok(Self {
                field,
                triangles,
                models,
                diagnostics,
            })
        }
    }

    /// Construction diagnostics for the shared edge model.
    pub fn diagnostics(&self) -> &CubicBuildDiagnostics {
        &self.diagnostics
    }
    /// Elementary triangles in the canonical regular-grid ordering.
    pub fn elementary_triangles(&self) -> &[GridTriangle] {
        &self.triangles
    }
    /// Semantic compositions at one elementary triangle's vertices.
    pub fn triangle_vertices(&self, index: usize) -> Result<[[f64; 3]; 3], FieldError> {
        let [v0, v1, v2] = self
            .triangles
            .get(index)
            .ok_or(FieldError::InvalidVertexIndex {
                index,
                vertex_count: self.triangles.len(),
            })?
            .vertices;
        Ok([
            self.field.composition(v0)?,
            self.field.composition(v1)?,
            self.field.composition(v2)?,
        ])
    }
    /// Evaluate the selected local field at barycentric coordinates.
    pub fn value_in_triangle(&self, index: usize, barycentric: [f64; 3]) -> Option<f64> {
        self.models.get(index).map(|model| model.value(barycentric))
    }
    /// Evaluate the selected local field's reduced barycentric gradient.
    pub fn gradient_in_triangle(&self, index: usize, u: f64, v: f64) -> Option<[f64; 2]> {
        self.models
            .get(index)
            .map(|model| model.gradient_reduced(u, v))
    }
}

/// Local interpolation mode selected for one partial-domain triangle.
#[cfg(feature = "cubic-alpha")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CubicTriangleMode {
    Cubic,
    OneSidedCubic,
    LinearFallback,
    Undefined,
}

#[cfg(feature = "cubic-alpha")]
enum PartialTriangleModel {
    Cubic(CubicAlphaTriangle),
    Linear([f64; 3]),
}

/// A prepared cubic-alpha model for a regular field with unavailable vertices.
///
/// Every elementary triangle is either cubic, locally linear, or undefined.
/// Undefined corners are never converted into scalar sentinels and never enter
/// the spline helpers.
#[cfg(feature = "cubic-alpha")]
pub struct PartialCubicGridField {
    field: RegularTernaryPartialScalarField,
    models: Vec<Option<PartialTriangleModel>>,
    modes: Vec<CubicTriangleMode>,
    diagnostics: CubicBuildDiagnostics,
}

#[cfg(feature = "cubic-alpha")]
impl PartialCubicGridField {
    pub fn new(
        field: RegularTernaryPartialScalarField,
        options: CubicAlphaBuildOptions,
    ) -> Result<Self, FieldError> {
        if options.partial_domain_policy == CubicPartialDomainPolicy::Strict
            && field.values().iter().any(Option::is_none)
        {
            return Err(FieldError::InsufficientStencil { samples: 0 });
        }
        let triangles = field.elementary_triangles()?;
        let mut diagnostics = CubicBuildDiagnostics::default();
        let intervals = build_partial_edge_intervals(&field, options, &mut diagnostics)?;
        let mut models = Vec::with_capacity(triangles.len());
        let mut modes = Vec::with_capacity(triangles.len());
        for triangle in &triangles {
            let values = triangle
                .vertices
                .map(|vertex| field.value(vertex).expect("triangle vertices are valid"));
            let mode = if values.iter().any(Option::is_none) {
                diagnostics.undefined_triangles += 1;
                CubicTriangleMode::Undefined
            } else {
                let pairs = [(0, 1), (1, 2), (0, 2)];
                let mut has_one_sided = false;
                let mut has_linear = false;
                for (left, right) in pairs {
                    let key = partial_edge_key(triangle.vertices[left], triangle.vertices[right]);
                    let edge_mode = intervals
                        .get(&key)
                        .copied()
                        .ok_or(crate::interpolation::InterpolationError::MissingEdgePair)?
                        .mode;
                    has_one_sided |= edge_mode == PartialEdgeMode::OneSided;
                    has_linear |= edge_mode == PartialEdgeMode::LinearFallback;
                }
                if has_linear {
                    diagnostics.linear_fallback_triangles += 1;
                    CubicTriangleMode::LinearFallback
                } else if has_one_sided {
                    diagnostics.one_sided_cubic_triangles += 1;
                    CubicTriangleMode::OneSidedCubic
                } else {
                    diagnostics.cubic_triangles += 1;
                    CubicTriangleMode::Cubic
                }
            };
            let model = match mode {
                CubicTriangleMode::Undefined => None,
                CubicTriangleMode::LinearFallback => Some(PartialTriangleModel::Linear(
                    values.map(|value| value.expect("defined triangle")),
                )),
                CubicTriangleMode::Cubic | CubicTriangleMode::OneSidedCubic => {
                    let vertex_values = values.map(|value| value.expect("defined triangle"));
                    Some(PartialTriangleModel::Cubic(CubicAlphaTriangle::new(
                        vertex_values,
                        directed_edges(&intervals, *triangle)?,
                        options.extrapolation,
                    )?))
                }
            };
            modes.push(mode);
            models.push(model);
        }
        Ok(Self {
            field,
            models,
            modes,
            diagnostics,
        })
    }

    pub const fn field(&self) -> &RegularTernaryPartialScalarField {
        &self.field
    }

    pub const fn grid(&self) -> crate::RegularTernaryGrid {
        self.field.grid()
    }

    pub fn diagnostics(&self) -> &CubicBuildDiagnostics {
        &self.diagnostics
    }

    pub fn triangle_mode(&self, index: usize) -> Option<CubicTriangleMode> {
        self.modes.get(index).copied()
    }

    pub fn value_in_triangle(&self, index: usize, barycentric: [f64; 3]) -> Option<Option<f64>> {
        self.models.get(index).map(|model| {
            model.as_ref().map(|model| match model {
                PartialTriangleModel::Cubic(model) => model.value(barycentric),
                PartialTriangleModel::Linear(values) => dot(*values, barycentric),
            })
        })
    }

    pub fn gradient_in_triangle(&self, index: usize, u: f64, v: f64) -> Option<Option<[f64; 2]>> {
        self.models.get(index).map(|model| {
            model.as_ref().map(|model| match model {
                PartialTriangleModel::Cubic(model) => model.gradient_reduced(u, v),
                PartialTriangleModel::Linear(values) => {
                    [values[0] - values[2], values[1] - values[2]]
                }
            })
        })
    }
}

#[cfg(feature = "cubic-alpha")]
fn dot(values: [f64; 3], barycentric: [f64; 3]) -> f64 {
    values[0] * barycentric[0] + values[1] * barycentric[1] + values[2] * barycentric[2]
}

#[cfg(feature = "cubic-alpha")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PartialEdgeMode {
    Cubic,
    OneSided,
    LinearFallback,
}

#[cfg(feature = "cubic-alpha")]
#[derive(Clone, Copy)]
struct PartialEdgeInterval {
    interval: AlphaInterval,
    mode: PartialEdgeMode,
}

#[cfg(feature = "cubic-alpha")]
fn partial_edge_key(left: GridVertexId, right: GridVertexId) -> (GridVertexId, GridVertexId) {
    if left < right {
        (left, right)
    } else {
        (right, left)
    }
}

#[cfg(feature = "cubic-alpha")]
fn directed_edges(
    intervals: &BTreeMap<(GridVertexId, GridVertexId), PartialEdgeInterval>,
    triangle: GridTriangle,
) -> Result<[DirectedAlphaInterval; 3], FieldError> {
    let pairs = [(0, 1), (1, 2), (0, 2)];
    let mut result = Vec::with_capacity(3);
    for (left, right) in pairs {
        let key = partial_edge_key(triangle.vertices[left], triangle.vertices[right]);
        let interval = intervals
            .get(&key)
            .copied()
            .ok_or(crate::interpolation::InterpolationError::MissingEdgePair)?
            .interval;
        let (start, end) = if key.0 == triangle.vertices[left] {
            (left, right)
        } else {
            (right, left)
        };
        let interval = if key.0 == triangle.vertices[left] {
            interval
        } else {
            interval.reversed()
        };
        result.push(DirectedAlphaInterval::new(start, end, interval)?);
    }
    Ok(result.try_into().expect("three local edge pairs"))
}

#[cfg(feature = "cubic-alpha")]
fn build_partial_edge_intervals(
    field: &RegularTernaryPartialScalarField,
    options: CubicAlphaBuildOptions,
    diagnostics: &mut CubicBuildDiagnostics,
) -> Result<BTreeMap<(GridVertexId, GridVertexId), PartialEdgeInterval>, FieldError> {
    let n = field.subdivisions();
    let mut lines: Vec<Vec<GridVertexId>> = Vec::new();
    for fixed in 0..=n {
        lines.push(
            (0..=n - fixed)
                .map(|i| {
                    field.vertex_id(crate::LatticeCoordinate {
                        i,
                        j: n - fixed - i,
                        k: fixed,
                    })
                })
                .collect::<Result<_, _>>()?,
        );
        lines.push(
            (0..=n - fixed)
                .map(|i| {
                    field.vertex_id(crate::LatticeCoordinate {
                        i,
                        j: fixed,
                        k: n - fixed - i,
                    })
                })
                .collect::<Result<_, _>>()?,
        );
        lines.push(
            (0..=n - fixed)
                .map(|j| {
                    field.vertex_id(crate::LatticeCoordinate {
                        i: fixed,
                        j,
                        k: n - fixed - j,
                    })
                })
                .collect::<Result<_, _>>()?,
        );
    }
    let mut result = BTreeMap::new();
    for line in lines {
        for index in 0..line.len().saturating_sub(1) {
            let left = field.value(line[index])?;
            let right = field.value(line[index + 1])?;
            if left.is_none() || right.is_none() {
                continue;
            }
            let edge = partial_alpha_for_interval(
                options.method,
                &line,
                field,
                index,
                options.partial_domain_policy,
                options.boundary_policy,
            )?;
            match edge.mode {
                PartialEdgeMode::OneSided => diagnostics.one_sided_edges += 1,
                PartialEdgeMode::LinearFallback => diagnostics.linear_fallback_edges += 1,
                PartialEdgeMode::Cubic => diagnostics.cubic_edges += 1,
            }
            let key = partial_edge_key(line[index], line[index + 1]);
            let edge = if key.0 == line[index] {
                edge
            } else {
                PartialEdgeInterval {
                    interval: edge.interval.reversed(),
                    mode: edge.mode,
                }
            };
            result.insert(key, edge);
        }
    }
    Ok(result)
}

#[cfg(feature = "cubic-alpha")]
fn partial_alpha_for_interval(
    method: CubicAlphaMethod,
    line: &[GridVertexId],
    field: &RegularTernaryPartialScalarField,
    index: usize,
    policy: CubicPartialDomainPolicy,
    _boundary_policy: CubicBoundaryPolicy,
) -> Result<PartialEdgeInterval, FieldError> {
    use spline1d::{cubic_single_left_alpha, cubic_single_middle_alpha, cubic_single_right_alpha};
    let values = line
        .iter()
        .map(|vertex| field.value(*vertex))
        .collect::<Result<Vec<_>, _>>()?;
    let mut start = index;
    while start > 0 && values[start - 1].is_some() {
        start -= 1;
    }
    let mut end = index + 1;
    while end + 1 < values.len() && values[end + 1].is_some() {
        end += 1;
    }
    let run_len = end - start + 1;
    let linear = || {
        Ok(PartialEdgeInterval {
            interval: AlphaInterval::default(),
            mode: PartialEdgeMode::LinearFallback,
        })
    };
    if policy == CubicPartialDomainPolicy::Strict && (start > 0 || end + 1 < values.len()) {
        return Err(FieldError::InsufficientStencil { samples: run_len });
    }
    if policy == CubicPartialDomainPolicy::LinearNearDomain && (start > 0 || end + 1 < values.len())
    {
        return linear();
    }
    let kind = cubic_method_kind(method);
    let alpha = if index > start && index + 2 < end {
        cubic_single_middle_alpha(
            kind,
            (index - 1) as f64,
            values[index - 1].expect("valid middle stencil"),
            index as f64,
            values[index].expect("valid middle stencil"),
            (index + 1) as f64,
            values[index + 1].expect("valid middle stencil"),
            (index + 2) as f64,
            values[index + 2].expect("valid middle stencil"),
        )
    } else if index == start && index + 2 <= end {
        let at_domain_boundary = start == 0;
        let alpha = if at_domain_boundary {
            cubic_single_left_alpha(
                kind,
                index as f64,
                values[index].expect("valid left stencil"),
                (index + 1) as f64,
                values[index + 1].expect("valid left stencil"),
                (index + 2) as f64,
                values[index + 2].expect("valid left stencil"),
            )
        } else {
            cubic_single_right_alpha(
                kind,
                index as f64,
                values[index].expect("valid right stencil"),
                (index + 1) as f64,
                values[index + 1].expect("valid right stencil"),
                (index + 2) as f64,
                values[index + 2].expect("valid right stencil"),
            )
        };
        return partial_interval_from_alpha(
            alpha,
            if at_domain_boundary {
                PartialEdgeMode::Cubic
            } else {
                PartialEdgeMode::OneSided
            },
        );
    } else if index + 1 == end && index > start {
        let at_domain_boundary = end + 1 == values.len();
        let alpha = if at_domain_boundary {
            cubic_single_right_alpha(
                kind,
                (index - 1) as f64,
                values[index - 1].expect("valid right stencil"),
                index as f64,
                values[index].expect("valid right stencil"),
                (index + 1) as f64,
                values[index + 1].expect("valid right stencil"),
            )
        } else {
            cubic_single_left_alpha(
                kind,
                (index - 1) as f64,
                values[index - 1].expect("valid left stencil"),
                index as f64,
                values[index].expect("valid left stencil"),
                (index + 1) as f64,
                values[index + 1].expect("valid left stencil"),
            )
        };
        return partial_interval_from_alpha(
            alpha,
            if at_domain_boundary {
                PartialEdgeMode::Cubic
            } else {
                PartialEdgeMode::OneSided
            },
        );
    } else {
        match policy {
            CubicPartialDomainPolicy::OneSided | CubicPartialDomainPolicy::Strict => {
                return Err(FieldError::InsufficientStencil { samples: run_len });
            }
            CubicPartialDomainPolicy::OneSidedThenLinear
            | CubicPartialDomainPolicy::LinearNearDomain => return linear(),
        }
    };
    partial_interval_from_alpha(alpha, PartialEdgeMode::Cubic)
}

#[cfg(feature = "cubic-alpha")]
fn partial_interval_from_alpha(
    alpha: [f64; 2],
    mode: PartialEdgeMode,
) -> Result<PartialEdgeInterval, FieldError> {
    if !alpha.into_iter().all(f64::is_finite) {
        return Err(crate::interpolation::InterpolationError::NonFiniteAlpha {
            alpha0: alpha[0],
            alpha1: alpha[1],
        }
        .into());
    }
    Ok(PartialEdgeInterval {
        interval: AlphaInterval::new(alpha[0], alpha[1]),
        mode,
    })
}

#[cfg(feature = "cubic-alpha")]
fn build_edge_intervals(
    field: &RegularTernaryScalarField,
    options: CubicAlphaBuildOptions,
    diagnostics: &mut CubicBuildDiagnostics,
) -> Result<BTreeMap<GridEdgeKey, AlphaInterval>, FieldError> {
    let n = field.subdivisions();
    let mut lines: Vec<Vec<GridVertexId>> = Vec::new();
    for fixed in 0..=n {
        lines.push(
            (0..=n - fixed)
                .map(|i| {
                    field.vertex_id(LatticeCoordinate {
                        i,
                        j: n - fixed - i,
                        k: fixed,
                    })
                })
                .collect::<Result<_, _>>()?,
        );
        lines.push(
            (0..=n - fixed)
                .map(|i| {
                    field.vertex_id(LatticeCoordinate {
                        i,
                        j: fixed,
                        k: n - fixed - i,
                    })
                })
                .collect::<Result<_, _>>()?,
        );
        lines.push(
            (0..=n - fixed)
                .map(|j| {
                    field.vertex_id(LatticeCoordinate {
                        i: fixed,
                        j,
                        k: n - fixed - j,
                    })
                })
                .collect::<Result<_, _>>()?,
        );
    }
    let mut result = BTreeMap::new();
    for line in lines {
        if line.len() < 2 {
            continue;
        }
        let values = line
            .iter()
            .map(|id| field.value(*id))
            .collect::<Result<Vec<_>, _>>()?;
        for interval_index in 0..line.len() - 1 {
            let mut alpha = if line.len() < 3 {
                match options.boundary_policy {
                    CubicBoundaryPolicy::LinearFallback => {
                        diagnostics.linear_fallback_edges += 1;
                        AlphaInterval::default()
                    }
                    CubicBoundaryPolicy::Error => {
                        return Err(FieldError::InsufficientStencil {
                            samples: line.len(),
                        });
                    }
                }
            } else {
                diagnostics.cubic_edges += 1;
                alpha_for_interval(options.method, &values, interval_index)
            };
            let key = GridEdgeKey::new(line[interval_index], line[interval_index + 1]);
            if key.start != line[interval_index] {
                alpha = alpha.reversed();
            }
            result.insert(key, alpha);
        }
    }
    debug_assert_eq!(result.len(), field.edge_count()?);
    Ok(result)
}

#[cfg(feature = "cubic-alpha")]
fn alpha_for_interval(method: CubicAlphaMethod, values: &[f64], index: usize) -> AlphaInterval {
    use spline1d::{cubic_single_left_alpha, cubic_single_middle_alpha, cubic_single_right_alpha};
    let kind = cubic_method_kind(method);
    let alpha = if index == 0 {
        cubic_single_left_alpha(kind, 0.0, values[0], 1.0, values[1], 2.0, values[2])
    } else if index + 1 == values.len() - 1 {
        let base = index - 1;
        cubic_single_right_alpha(
            kind,
            base as f64,
            values[base],
            index as f64,
            values[index],
            (index + 1) as f64,
            values[index + 1],
        )
    } else {
        cubic_single_middle_alpha(
            kind,
            (index - 1) as f64,
            values[index - 1],
            index as f64,
            values[index],
            (index + 1) as f64,
            values[index + 1],
            (index + 2) as f64,
            values[index + 2],
        )
    };
    AlphaInterval::new(alpha[0], alpha[1])
}

#[cfg(all(test, feature = "cubic-alpha"))]
mod tests {
    use super::*;
    use crate::RegularTernaryGrid;
    #[test]
    fn shared_edges_are_built_once_with_expected_fallbacks() {
        let field = RegularTernaryScalarField::new(3, (0..10).map(|i| i as f64).collect()).unwrap();
        let model = CubicGridField::new(&field, CubicAlphaBuildOptions::default()).unwrap();
        assert_eq!(model.elementary_triangles().len(), 9);
        assert_eq!(
            model.diagnostics().cubic_edges + model.diagnostics().linear_fallback_edges,
            field.edge_count().unwrap()
        );
        assert_eq!(model.diagnostics().linear_fallback_edges, 3);
    }

    fn partial_values(n: usize, missing: &[usize]) -> RegularTernaryPartialScalarField {
        let grid = RegularTernaryGrid::new(n).unwrap();
        let values = (0..grid.vertex_count())
            .map(|index| (!missing.contains(&index)).then_some(index as f64 + 10.0))
            .collect();
        RegularTernaryPartialScalarField::new(n, values).unwrap()
    }

    #[test]
    fn partial_domain_uses_local_modes_without_nan_or_global_rejection() {
        let field = partial_values(10, &[0]);
        let model = PartialCubicGridField::new(field, CubicAlphaBuildOptions::default()).unwrap();
        let diagnostics = model.diagnostics();
        assert!(diagnostics.undefined_triangles > 0);
        assert!(diagnostics.cubic_triangles > 0);
        assert!(diagnostics.one_sided_edges > 0);
        for index in 0..diagnostics.cubic_triangles
            + diagnostics.one_sided_cubic_triangles
            + diagnostics.linear_fallback_triangles
            + diagnostics.undefined_triangles
        {
            match model.triangle_mode(index).unwrap() {
                CubicTriangleMode::Undefined => {
                    assert_eq!(model.value_in_triangle(index, [1.0, 0.0, 0.0]), Some(None));
                }
                _ => {
                    let value = model
                        .value_in_triangle(index, [0.2, 0.3, 0.5])
                        .flatten()
                        .unwrap();
                    let gradient = model
                        .gradient_in_triangle(index, 0.2, 0.3)
                        .flatten()
                        .unwrap();
                    assert!(value.is_finite());
                    assert!(gradient.into_iter().all(f64::is_finite));
                }
            }
        }
    }

    #[test]
    fn partial_stencil_selection_is_validity_aware() {
        let grid = RegularTernaryGrid::new(4).unwrap();
        let ids: Vec<_> = (0..=4)
            .map(|index| {
                grid.vertex_id(LatticeCoordinate {
                    i: index,
                    j: 0,
                    k: 4 - index,
                })
                .unwrap()
            })
            .collect();
        let make = |values: Vec<Option<f64>>| {
            RegularTernaryPartialScalarField::new(4, {
                let mut all = vec![Some(1.0); grid.vertex_count()];
                for (id, value) in ids.iter().copied().zip(values) {
                    all[id.0] = value;
                }
                all
            })
            .unwrap()
        };
        let all = make(vec![Some(1.0); 5]);
        assert_eq!(
            partial_alpha_for_interval(
                CubicAlphaMethod::Akima,
                &ids,
                &all,
                1,
                CubicPartialDomainPolicy::OneSidedThenLinear,
                CubicBoundaryPolicy::LinearFallback,
            )
            .unwrap()
            .mode,
            PartialEdgeMode::Cubic
        );
        let left_missing = make(vec![None, Some(1.0), Some(2.0), Some(3.0), Some(4.0)]);
        assert_eq!(
            partial_alpha_for_interval(
                CubicAlphaMethod::Akima,
                &ids,
                &left_missing,
                1,
                CubicPartialDomainPolicy::OneSidedThenLinear,
                CubicBoundaryPolicy::LinearFallback,
            )
            .unwrap()
            .mode,
            PartialEdgeMode::OneSided
        );
        let right_missing = make(vec![Some(1.0), Some(2.0), Some(3.0), Some(4.0), None]);
        assert_eq!(
            partial_alpha_for_interval(
                CubicAlphaMethod::Akima,
                &ids,
                &right_missing,
                2,
                CubicPartialDomainPolicy::OneSidedThenLinear,
                CubicBoundaryPolicy::LinearFallback,
            )
            .unwrap()
            .mode,
            PartialEdgeMode::OneSided
        );
        let two_sample_run = make(vec![None, Some(1.0), Some(2.0), None, Some(4.0)]);
        assert_eq!(
            partial_alpha_for_interval(
                CubicAlphaMethod::Akima,
                &ids,
                &two_sample_run,
                1,
                CubicPartialDomainPolicy::OneSidedThenLinear,
                CubicBoundaryPolicy::LinearFallback,
            )
            .unwrap()
            .mode,
            PartialEdgeMode::LinearFallback
        );
        assert!(matches!(
            partial_alpha_for_interval(
                CubicAlphaMethod::Akima,
                &ids,
                &two_sample_run,
                1,
                CubicPartialDomainPolicy::Strict,
                CubicBoundaryPolicy::LinearFallback,
            ),
            Err(FieldError::InsufficientStencil { .. })
        ));
    }
}
