# ternary-contours

ternary-contours is a backend-independent numerical crate for scalar data on a
regular ternary composition grid or, with an optional feature, an irregular Delaunay mesh of scattered compositions. It answers questions such as where in a
three-component composition space temperature is 900 degrees C, Gibbs energy is
zero, or a phase fraction is 0.5.

A composition is written as (a, b, c) with a + b + c = 1. Those three numbers
live on a two-dimensional simplex, so a ternary diagram is naturally a
triangle. This crate computes numerical isolines and filled scalar intervals in
that triangle; it deliberately does not know about pixels, colours, fonts,
viewports, or drawing backends.

## From compositions to a scalar field

A regular grid with subdivision count n contains the integer lattice points:

~~~text
i + j + k = n
(a, b, c) = (i/n, j/n, k/n)
~~~

RegularTernaryGrid yields compositions lazily in the canonical order expected
by RegularTernaryScalarField. Evaluate a finite scalar property at each grid
vertex, then construct a field from those values. The scalar can be
temperature, activity, concentration, Gibbs energy, phase fraction, or any
other property defined on a three-component mixture.

## Pointwise field evaluation

Prepare an interpolation evaluator once when a field will be queried at many
arbitrary normalized compositions. Point location belongs to
`RegularTernaryGrid`, independently of scalar values; it uses scaled lattice
coordinates and a constant-sized local candidate set rather than scanning the
grid.

~~~rust
use ternary_contours::{
    FieldInterpolation, InterpolatedTernaryField, RegularTernaryScalarField,
};

let field = RegularTernaryScalarField::from_fn(12, |[a, b, c]| {
    2.0 * a - 3.0 * b + 5.0 * c
})?;
let evaluator = InterpolatedTernaryField::new(&field, FieldInterpolation::Linear)?;

let sample = evaluator.evaluate([0.23, 0.31, 0.46])?;
assert!((sample.value - (2.0 * 0.23 - 3.0 * 0.31 + 5.0 * 0.46)).abs() < 1e-12);
println!("triangle={:?}, bary={:?}", sample.location.triangle, sample.location.barycentric);
println!("df/da, df/db = {:?}", sample.gradient_ab);
# Ok::<(), Box<dyn std::error::Error>>(())
~~~

Accepted compositions must be finite and sum to one within
`POINT_LOCATION_TOLERANCE` (`1e-10`). They are then normalized and snapped near
simplex and regular-lattice boundaries. A point on a shared edge or vertex is
owned by the lowest canonical elementary-triangle identifier; its reported
gradient comes from that owning triangle and is never averaged.

Field interpolation has this hierarchy:

~~~text
Linear
CubicAlpha
    Akima | MAKIMA | PCHIP | Steffen
    boundary: LinearFallback | Error
    interior continuation: Muggianu | Kohler | RawBarycentric
~~~

`Muggianu`, `Kohler`, and `RawBarycentric` are interior continuation policies
inside the cubic-alpha model, not separate interpolation families. With the
optional `cubic-alpha` feature, select them through `CubicAlphaBuildOptions`.
The cubic local model is prepared once and shares the exact directed interval
model used by cubic contours. Both interpolation families are C0; piecewise
linear gradients are constant within a triangle, while cubic-alpha gradients
vary inside a triangle and neither model promises C1 continuity across a grid
edge.

Use `InterpolatedTernaryField::values` for lazy batch results or `values_into`
to reuse caller-owned output storage. See `examples/interpolate_field.rs`.

## Irregular Delaunay fields

Enable `irregular-delaunay` to construct an immutable
`IrregularTernaryMesh` from scattered finite A/B/C samples. The private
backend is `delaunay`; its handles never appear in this crate's public API.
Samples are embedded symmetrically in the logical equilateral plane
`A=(0,0)`, `B=(1,0)`, `C=(1/2,sqrt(3)/2)`, while all public coordinates remain
semantic `(a,b,c)` values.

The mesh has dense, stable vertex/edge/triangle IDs. Canonical edge orientation,
incident triangles, hull edges, and triangle edge IDs provide the topology for
both interpolation models. `PreparedIrregularTernaryField` remains the compact
linear-only evaluator. The broader `InterpolatedIrregularTernaryField` chooses
one of:

~~~text
Irregular field interpolation:
    Linear
    CubicAlpha (`irregular-cubic-alpha`)
        Akima | MAKIMA | PCHIP | Steffen
        boundary: LinearFallback | Error
        interior continuation: Muggianu | Kohler | RawBarycentric
~~~

The cubic-alpha option stores one directed interval per canonical mesh edge.
For edge endpoints `x0, x1`, it locates the collinear virtual points
`2*x0-x1` and `2*x1-x0` once, starts from their linear field values, then
performs damped synchronous Jacobi alpha updates. `LinearFallback` pins an edge
whose virtual point leaves the simplex or convex hull to zero alpha; `Error`
returns the canonical edge, endpoint side, and failure class. Muggianu and
Kohler are interior continuation policies within this one cubic-alpha model,
not alternative interpolation families.

~~~rust
# #[cfg(feature = "irregular-cubic-alpha")]
# fn main() -> Result<(), Box<dyn std::error::Error>> {
use ternary_contours::{
    BinaryExtrapolation, CubicAlphaMethod, InterpolatedIrregularTernaryField,
    IrregularCubicAlphaOptions, IrregularFieldInterpolation, IrregularTernaryMesh,
    IrregularTernaryScalarField,
};
let mesh = IrregularTernaryMesh::new([
    [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0],
    [0.57, 0.28, 0.15], [0.18, 0.61, 0.21], [0.23, 0.16, 0.61],
    [0.31, 0.42, 0.27],
])?;
let field = IrregularTernaryScalarField::from_fn(mesh, |[a, b, c]| a*a + b*b - c*c)?;
let evaluator = InterpolatedIrregularTernaryField::new(
    &field,
    IrregularFieldInterpolation::CubicAlpha(IrregularCubicAlphaOptions {
        method: CubicAlphaMethod::Pchip,
        extrapolation: BinaryExtrapolation::Kohler,
        ..IrregularCubicAlphaOptions::default()
    }),
)?;
let sample = evaluator.evaluate([0.23, 0.31, 0.46])?;
println!("triangle={:?}, bary={:?}", sample.location.triangle, sample.location.barycentric);
println!("df/da, df/db = {:?}", sample.gradient_ab);
# Ok(()) }
# #[cfg(not(feature = "irregular-cubic-alpha"))]
# fn main() {}
~~~

Both irregular evaluators have lazy `values` and allocation-reusing
`values_into` batch methods. Linear gradients are constant in a triangle;
cubic gradients vary locally. Both fields are C0 but not generally C1 across a
triangle edge, and edge/vertex gradients are those of the mesh's deterministic
owning triangle rather than an average. A query outside the samples' convex
hull returns a typed error. There are no holes, constrained edges, or
non-convex domains. Run `cargo run --example interpolate_irregular_field --features irregular-delaunay`
for linear evaluation, or `cargo run --example interpolate_irregular_cubic_field --features irregular-cubic-alpha`
for cubic preparation, diagnostics, and alpha inspection.

### Irregular isolines

`IrregularContourSet` adds backend-independent isolines over the mesh convex
hull. `IrregularContourOptions::linear()` uses one exact affine segment per
Delaunay triangle. `IrregularContourOptions::cubic_alpha(...)` prepares the
edge-alpha field once, then adaptively subdivides each source triangle in local
barycentric coordinates. The cubic extractor calculates canonical roots of each
shared edge interval once per level, so adjacent triangles use the same
semantic endpoint even when their interior refinement differs.

Use `IrregularContourSet::compute_prepared` when one
`InterpolatedIrregularTernaryField` should serve point queries, alpha
inspection, and multiple contour sets without repeating the Jacobi solve. The
optional `ContourRegularization` redistributes chord lengths in the same
canonical equilateral plane used for the Delaunay embedding, then projects
interior points using the prepared global field. Each accepted projection step
relocates in the mesh; candidates leaving its convex hull are backtracked.
Open-path endpoints remain fixed and closed paths have no duplicate final point.

Irregular cubic contours are C0, not C1, across mesh edges. Their reported
gradient is the deterministic owner-triangle gradient at a shared edge or
vertex; it is never averaged. Run
`cargo run --example irregular_contours --features irregular-cubic-alpha` for
a complete numerical example.

## Extracting isolines

An isoline is the set of compositions satisfying f(a, b, c) = level. The crate
inspects every elementary grid triangle locally, finds its intersections with
the requested level, and joins those local pieces into deterministic open or
closed ContourPath values. Output remains in semantic A/B/C coordinates, ready
for a renderer or a scientific calculation.

| Need | Method |
| --- | --- |
| Robust baseline or exactly affine data | Linear contours |
| Smoother edge behaviour on a regular grid | Cubic-alpha contours |
| Coloured scalar intervals | Linear contour bands |
| Charting, labels, colours, or clipping | plotters-ternary |

### Linear

ContourInterpolation::Linear is the default baseline. The field is
piecewise-affine inside each elementary triangle, so contours are straight
segments there and exact for linearly interpolated vertex values. Choose it
when predictable topology and a direct interpretation of samples matter most.

### Cubic-alpha

The optional cubic-alpha feature constructs directed one-dimensional spline
intervals along the regular-grid line families and extends them through each
triangle. Muggianu and Kohler policies control that interior binary
continuation; both reproduce the source interval exactly on its binary edge.
Adaptive topology extraction handles curved level sets, and optional
regularization redistributes points while projecting them back to the requested
level.

Cubic-alpha is smoother along grid edges, but the global field is C0 rather
than C1 across elementary-triangle boundaries. Choose it when the
edge-derived model fits the sampled property; it does not make every coarse
data set more accurate.

## Filled contour bands

ContourBandSet turns ordered scalar breaks into regions between those values.
For l0 < l1 < ... < lm, scalar ownership is exact and half-open:

- lower extreme: f < l0;
- intermediate band: li <= f < li+1;
- upper extreme: f >= lm.

Adjacent polygons may share a zero-area threshold boundary, but their
positive-area interiors do not overlap. Bands can have disconnected regions and
holes. The current band implementation is linear only; requesting cubic-alpha
bands returns a typed unsupported-mode error.

## A complete numerical workflow

~~~rust
use ternary_contours::{
    ContourBandOptions, ContourBandSet, ContourOptions, ContourSet,
    RegularTernaryScalarField,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let field = RegularTernaryScalarField::from_fn(12, |[a, b, c]| {
        2.0 * a - 3.0 * b + 5.0 * c
    })?;

    let contours = ContourSet::compute(&field, &[0.0, 1.0], ContourOptions::linear())?;
    let bands = ContourBandSet::compute(
        &field,
        &[-1.0, 0.0, 1.0],
        ContourBandOptions::linear(),
    )?;

    if let Some(path) = contours.levels[0].paths.first() {
        println!("{:?}", path.points[0].as_array());
    }
    println!("{} scalar bands", bands.bands.len());
    Ok(())
}
~~~

For fallible field evaluation, RegularTernaryScalarField::try_from_fn preserves
the callback error together with the canonical vertex id and composition that
failed.

## Numerical core versus rendering

This crate owns grids, scalar fields, interpolation, contour paths, band
regions, holes, and diagnostics. It does not own colours, screen coordinates,
viewport clipping, labels, legends, or image backends. Changing a renderer,
output size, line style, or supersampling setting therefore cannot change the
numerical contours returned by this crate.

Use [plotters-ternary](https://crates.io/crates/plotters-ternary) when those
paths and regions should become PNG or SVG ternary diagrams.

## Features

- Default: regular-grid fields, linear contours, and linear filled bands.
- cubic-alpha: edge-derived cubic-alpha contour construction, adaptive topology,
  and optional regularization. It enables the optional spline1d dependency.
- irregular-delaunay: irregular 2-D Delaunay meshes, backend-assisted point
  location, prepared piecewise-linear scalar evaluation, and linear isolines.
  It enables the optional `delaunay` dependency.
- irregular-cubic-alpha: self-consistent cubic-alpha point evaluation and
  adaptive cubic isolines on an irregular mesh. It enables both
  `irregular-delaunay` and `cubic-alpha`.

The current minimum supported Rust version is 1.97.1, selected explicitly for
maintained `delaunay` 0.8 support.

## Limits

- Irregular meshes support linear and optional cubic-alpha isolines, values,
  and gradients only inside their samples' convex hull.
- No irregular filled bands.
- No constrained Delaunay meshing, holes, or non-convex domains.
- No cubic-alpha filled bands.
- No rendering, pixels, chart clipping, or labels.
- No C1 global surface guarantee for cubic-alpha fields.

Detailed formulas and validation material live in the
[knowledge base](docs/knowledge-base/README.md), the
[permanent numerical-validation report](docs/numerical-validation.md), and the
[filled-band note](docs/filled-contours.md).

Contributions are welcome through the
[project repository](https://github.com/evnekdev/ternary-contours). The crate
is licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
