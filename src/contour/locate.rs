use crate::{RegularTernaryScalarField, TernaryCoordinate, grid::GridTriangle};
#[cfg(feature = "cubic-alpha")]
use crate::{field::CubicGridField, interpolation::CubicAlphaBuildOptions};

use super::ContourError;
#[cfg(feature = "cubic-alpha")]
use super::{CubicAlphaOptions, CubicContourDiagnostics};

pub(crate) struct LocatedValue {
    pub value: f64,
    /// Gradient with respect to global semantic `(a,b)`, with `c=1-a-b`.
    pub gradient_ab: [f64; 2],
}

pub(crate) fn locate_linear(
    field: &RegularTernaryScalarField,
    point: TernaryCoordinate,
) -> Result<LocatedValue, ContourError> {
    locate_with(field, point, |triangle, barycentric| {
        let [v0, v1, v2] = triangle.vertices;
        let values = [field.value(v0)?, field.value(v1)?, field.value(v2)?];
        let value = dot(values, barycentric);
        Ok((value, local_linear_gradient(values)))
    })
}

fn locate_with(
    field: &RegularTernaryScalarField,
    point: TernaryCoordinate,
    mut evaluate: impl FnMut(GridTriangle, [f64; 3]) -> Result<(f64, [f64; 2]), ContourError>,
) -> Result<LocatedValue, ContourError> {
    let [a, b, c] = point.as_array();
    let location = field
        .grid()
        .locate([a, b, c])
        .map_err(|_| ContourError::PointOutsideGrid { a, b, c })?;
    let [v0, v1, v2] = location.triangle.vertices;
    let vertices = [
        field.composition(v0)?.into(),
        field.composition(v1)?.into(),
        field.composition(v2)?.into(),
    ];
    let (value, local_gradient) = evaluate(location.triangle, location.barycentric)?;
    let gradient_ab = local_to_global_gradient(local_gradient, vertices);
    Ok(LocatedValue { value, gradient_ab })
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn local_linear_gradient(values: [f64; 3]) -> [f64; 2] {
    [values[0] - values[2], values[1] - values[2]]
}

fn local_to_global_gradient(local: [f64; 2], vertices: [TernaryCoordinate; 3]) -> [f64; 2] {
    let [a0, b0, _] = vertices[0].as_array();
    let [a1, b1, _] = vertices[1].as_array();
    let [a2, b2, _] = vertices[2].as_array();
    let da0 = a0 - a2;
    let da1 = a1 - a2;
    let db0 = b0 - b2;
    let db1 = b1 - b2;
    let det = da0 * db1 - da1 * db0;
    [
        (local[0] * db1 - db0 * local[1]) / det,
        (da0 * local[1] - local[0] * da1) / det,
    ]
}

#[cfg(feature = "cubic-alpha")]
pub(crate) struct ContourCubicField<'a> {
    field: &'a RegularTernaryScalarField,
    core: CubicGridField<'a>,
    diagnostics: CubicContourDiagnostics,
}

#[cfg(feature = "cubic-alpha")]
impl<'a> ContourCubicField<'a> {
    pub fn new(
        field: &'a RegularTernaryScalarField,
        options: CubicAlphaOptions,
    ) -> Result<Self, ContourError> {
        let core = CubicGridField::new(
            field,
            CubicAlphaBuildOptions {
                method: options.method,
                boundary_policy: options.boundary_policy,
                partial_domain_policy: Default::default(),
                extrapolation: options.extrapolation,
            },
        )?;
        let diagnostics = CubicContourDiagnostics {
            cubic_edges: core.diagnostics().cubic_edges,
            linear_fallback_edges: core.diagnostics().linear_fallback_edges,
            ..CubicContourDiagnostics::default()
        };
        Ok(Self {
            field,
            core,
            diagnostics,
        })
    }

    pub fn diagnostics(&self) -> &CubicContourDiagnostics {
        &self.diagnostics
    }

    pub fn diagnostics_mut(&mut self) -> &mut CubicContourDiagnostics {
        &mut self.diagnostics
    }

    pub fn triangles(&self) -> &[GridTriangle] {
        self.core.elementary_triangles()
    }

    pub fn triangle_vertices(&self, index: usize) -> Result<[TernaryCoordinate; 3], ContourError> {
        Ok(self.core.triangle_vertices(index)?.map(Into::into))
    }

    pub fn value_in_triangle(&self, index: usize, barycentric: [f64; 3]) -> f64 {
        self.core
            .value_in_triangle(index, barycentric)
            .expect("topology triangle index comes from this core field")
    }

    pub fn locate(&self, point: TernaryCoordinate) -> Result<LocatedValue, ContourError> {
        locate_with(self.field, point, |triangle, barycentric| {
            let value = self
                .core
                .value_in_triangle(triangle.id, barycentric)
                .expect("located triangle belongs to this core field");
            let gradient = self
                .core
                .gradient_in_triangle(triangle.id, barycentric[0], barycentric[1])
                .expect("located triangle belongs to this core field");
            Ok((value, gradient))
        })
    }
}

