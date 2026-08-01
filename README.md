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

## Metrics and derived analysis

The `metrics` module is a numerical analysis layer; it does not add rendering,
histogram plotting, maps, contours of derived values, or interpolation families.
It uses the same canonical equilateral logical plane as irregular Delaunay
construction and contour lengths.

Triangulation quality is **irregular-only**. `IrregularTernaryMesh::metrics()`
reports Delaunay triangle, edge, vertex, hull, and topology records. By
contrast, `TernaryGradient`, derived prepared fields, local quadratic sampled-
field estimates, interior-edge gradient jumps, and final contour response
metrics apply to both regular and irregular fields.

| Metric family | Regular grid | Irregular mesh |
| --- | --- | --- |
| Triangle-quality distribution | Not needed / analytically uniform | Yes |
| Delaunay topology and valence | No | Yes |
| Gradient and gradient norm | Yes | Yes |
| Gradient jumps | Yes | Yes |
| Local Hessian estimate | Yes | Yes |
| Curvature anisotropy | Yes | Yes |
| Derived-field evaluation | Yes | Yes |
| Mesh–field alignment | Controlled lattice form | Full irregular form |
| Alpha-response metrics | Regular cubic continuity | Irregular cubic response |
| Contour-response metrics | Yes | Yes |

`TernaryGradient` converts the established reduced `[df/da, df/db]` result to
logical `[gx, gy]` with `gx = gb-ga` and `gy = -(ga+gb)/sqrt(3)`. Its norm is
`sqrt((4/3) * (ga^2 - ga*gb + gb^2))`, so regular and irregular gradients have
the same physical units and comparison basis. `FieldSample::gradient()` and
`IrregularFieldSample::gradient()` retain the existing public `gradient_ab`
field while adding this invariant representation.

`DerivedRegularTernaryField` and `DerivedIrregularTernaryField` reuse prepared
evaluators for values, reduced/logical gradient components, and gradient norm.
`LocalQuadraticEstimate` is a shared interpolation-independent, stable QR fit
to sampled values: regular fields expand integer lattice rings while irregular
fields expand stable graph rings. It is not an analytic interpolant Hessian.

For an irregular analysis walkthrough, run:

~~~text
cargo run --example irregular_metrics --features irregular-delaunay
~~~

See [irregular-mesh-metrics.md](docs/irregular-mesh-metrics.md) for formulas,
edge ownership, weighting semantics, feature gates, and limitations.

## Stable phase ensembles

`PreparedStablePhaseEnsemble` traces phase-labelled contours over the stable
upper envelope `max_i h_i(a,b,c)`. Each phase supplies a required height source
and may supply a topology-matched secondary scalar. Height mode traces `h_i=L`
where phase `i` is stable; secondary mode traces `q_i=L` in the same
height-defined region. Secondary values never choose phase ownership.

Sources may be regular linear fields, optional regular cubic-alpha fields,
irregular linear fields, or optional irregular cubic-alpha fields. Preparation
groups sources sharing geometry, locates once per group and point, and samples
every layer onto one virtual `RegularTernaryGrid`. Cubic source interpolation
changes sampled vertex values; all final sampling-grid fields and stable contours
remain affine inside each sampling-grid triangle.

~~~rust
use ternary_contours::{
    FieldInterpolation, PreparedStablePhaseEnsemble, RegularTernaryScalarField,
    StableContourQuantity, StablePhaseId, StablePhaseSource, StableScalarSource,
    StableGridOptions,
};

let alpha = RegularTernaryScalarField::from_fn(12, |[a, _, _]| a)?;
let beta = RegularTernaryScalarField::from_fn(12, |[_, b, _]| b)?;
let prepared = PreparedStablePhaseEnsemble::new(
    [
        StablePhaseSource::new(
            StablePhaseId(1),
            StableScalarSource::regular(&alpha, FieldInterpolation::Linear),
        ),
        StablePhaseSource::new(
            StablePhaseId(2),
            StableScalarSource::regular(&beta, FieldInterpolation::Linear),
        ),
    ],
    StableContourQuantity::Height,
    StableGridOptions::default(),
)?;
let result = prepared.contours(&[0.3, 0.4])?;
# Ok::<(), Box<dyn std::error::Error>>(())
~~~

`StableScalarSource::evaluator` may return `StablePhaseEvaluation::Undefined`
for a phase outside its physical domain. Undefined competitors are omitted from
the local upper envelope, but every sampled or queried composition must still
have at least one defined phase. No phase is extrapolated through an undefined
region.

`PreparedStablePhaseEnsemble::stable_boundaries` constructs a level-free graph
whose dense nodes are stable binary or interior invariants and whose edges are
phase-pair `StableUnivariantPath` values. Binary discovery evaluates the original
prepared sources on canonical AB, BC, and CA parameterizations. Raw interior
tracing then follows the cached affine stable polygons and dense regular
sampling-grid adjacency. Canonical invariant coordinates are shared by every
incident path. Isolated closed univariant loops without an invariant seed are
explicitly deferred.

Optional `PathRegularizationOptions` is a post-topology operation. It shares
cleanup, equilateral-plane arclength redistribution, normalization, safeguarded
projection, and backtracking with ordinary regular and irregular contours, while
using the univariant equation `T_p-T_q=0`. Binary and interior invariant endpoints
remain fixed, every accepted point is rechecked against all defined competitors,
and graph connectivity cannot change. `ContourRegularization` remains a
backward-compatible name for the same shared option type. Run:

~~~text
cargo run --release --example stable_boundary_network
~~~
Stable regions are exact convex half-plane intersections for the final affine
sampling-grid model. They are not inferred from phase labels at triangle vertices,
so an interior stable polygon can be recovered even when its phase wins no
triangle vertex. Exact affine min/max bounds safely prune phases and competitor
comparisons before clipping.

Height paths retain their phase IDs and end at shared canonical univariant or
invariant junctions without joining across phases. Secondary paths terminate
independently at stable height boundaries unless their coordinates genuinely
coincide. Local intersections and global assembly enforce strict forward
progress and return typed errors for retracing, branching, directed cycles,
positive-area height ties, or target lines coincident with stable boundaries.

Optional centroid/edge-midpoint verification compares original prepared source
fields with sampling-grid-affine predictions and can double the global subdivisions
until configured tolerances pass. This is a practical resolution check, not a
proof that no smaller feature exists. Stable results are exact for the final
piecewise-linear sampling-grid representation, not necessarily for the original
source interpolants.

Synthetic `LiquidusFieldSpec` constructors provide deterministic corner, edge, and interior liquidus-like fields for reproducible gallery cases. See [stable-phase-contours.md](docs/stable-phase-contours.md) for the full model,
secondary semantics, diagnostics, complexity, feature gates, degeneracies, and
roadmap. Run the mixed numerical example with:

~~~text
cargo run --example stable_phase_contours --features irregular-delaunay
~~~

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

- Default: regular-grid fields, linear contours, linear filled bands, and stable
  height/secondary ensembles sampled from regular linear sources.
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
- Stable evaluator sources may have partial domains, but their union must cover every
  sampled/query point; local adaptive sampling-grid refinement remains deferred.
- Stable contours are piecewise linear on the final sampling grid; direct nonlinear
  upper-envelope topology and stable filled regions are deferred.
- No rendering, pixels, chart clipping, or labels.
- No C1 global surface guarantee for cubic-alpha fields.

Detailed formulas and validation material live in the
[knowledge base](docs/knowledge-base/README.md), the
[permanent numerical-validation report](docs/numerical-validation.md), and the
[filled-band note](docs/filled-contours.md).

Contributions are welcome through the
[project repository](https://github.com/evnekdev/ternary-contours). The crate
is licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
