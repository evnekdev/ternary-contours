# Stable-phase contours on a virtual umbrella grid

## Model and scope

A stable phase ensemble contains phases `i = 1..N`. Every phase has a required
height field `h_i(a,b,c)`, where `a+b+c=1`. A phase is stable where it belongs
to the upper envelope:

```text
H(x) = max_i h_i(x)
R_i = { x | h_i(x) >= h_j(x) for every j }
```

`PreparedStablePhaseEnsemble` supports two quantities:

- `StableContourQuantity::Height` traces `h_i=L` inside `R_i`;
- `StableContourQuantity::Secondary` traces a phase-specific `q_i=L` inside
  the same height-defined `R_i`.

The secondary scalar never participates in phase ownership. Every phase in
secondary mode must supply both fields, and the pair must have exactly the same
source topology: equal regular subdivisions or the same immutable irregular
mesh identity. A regular/irregular pair is rejected.

Stable contours are numerical geometry only. They do not contain Plotters
projection, clipping, colours, labels, or rendering behavior.

## Public workflow

```rust
use ternary_contours::{
    FieldInterpolation, PreparedStablePhaseEnsemble, RegularTernaryScalarField,
    StableContourQuantity, StablePhaseId, StablePhaseSource, StableScalarSource,
    StableUmbrellaOptions,
};

let alpha = RegularTernaryScalarField::from_fn(12, |[a, _, _]| a)?;
let beta = RegularTernaryScalarField::from_fn(12, |[_, b, _]| b)?;
let phases = [
    StablePhaseSource::new(
        StablePhaseId(1),
        StableScalarSource::regular(&alpha, FieldInterpolation::Linear),
    ),
    StablePhaseSource::new(
        StablePhaseId(2),
        StableScalarSource::regular(&beta, FieldInterpolation::Linear),
    ),
];
let prepared = PreparedStablePhaseEnsemble::new(
    phases,
    StableContourQuantity::Height,
    StableUmbrellaOptions::default(),
)?;
let contours = prepared.contours(&[0.3, 0.4])?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Preparation is deliberately separate from level extraction. Repeated calls to
`contours` reuse source interpolation models, umbrella samples, per-triangle
bounds, and stable polygons.

## Source interpolation and umbrella interpolation

A `StableScalarSource` selects the evaluator used while sampling:

| Source | Linear | Cubic-alpha |
| --- | --- | --- |
| Regular | Default build | `cubic-alpha` |
| Irregular Delaunay | `irregular-delaunay` | `irregular-cubic-alpha` |

Regular cubic-alpha sources use `CubicAlphaBuildOptions`. Irregular cubic-alpha
sources use `IrregularCubicAlphaOptions` and the already prepared synchronous
edge-alpha field. Muggianu, Kohler, and RawBarycentric remain continuation
policies inside those cubic-alpha models.

After sampling, every umbrella scalar is piecewise affine. A cubic source does
not produce cubic stable contours. It only changes the values sampled at
umbrella vertices.

## One common regular topology

Preparation builds one `RegularTernaryGrid` with the requested umbrella
subdivision count. All phases are sampled at its vertices into dense
phase-major arrays:

```text
height[phase][umbrella vertex]
secondary[phase][umbrella vertex]  // secondary mode only
```

Regular layers with equal subdivisions form one geometry group. Irregular
layers form one group only when their mesh identities match. At each umbrella
or verification point, a group performs point location once and evaluates all
its scalar layers at that cached location. Irregular groups carry a deterministic
previous-triangle hint while points are visited in regular-grid row order.
Diagnostics expose group, location, reuse, and scalar-evaluation counts.

All sources must cover the complete semantic simplex. Regular sources do so by
construction. An irregular source whose convex hull misses any umbrella vertex
returns `IncompleteSourceCoverage`; it is never extrapolated or silently
omitted.

## Exact affine stable regions

Inside one umbrella triangle all sampled heights are affine. The stable region
for phase `i` is therefore the convex polygon

```text
P_i = triangle intersect all half-planes (h_i - h_j >= -stability_tolerance).
```

The implementation constructs `P_i` with deterministic Sutherland-Hodgman
clipping in triangle barycentric coordinates and converts final results to
semantic A/B/C coordinates. Stable polygons depend only on heights, so they are
cached once and reused for every requested level and for either target
quantity.

This construction does not label triangle vertices and infer regions between
them. A phase can lose at all three umbrella-triangle vertices and still own an
interior polygon. Such a phase remains eligible whenever exact affine bounds
permit it, and the clipping inequalities find its region. A maintained
regression case constructs this central narrow polygon explicitly.

### Safe pruning

For phase `i`, let `min_i` and `max_i` be the extrema of its three triangle
vertex heights. The upper envelope is at least

```text
envelope_floor = max_i min_i.
```

A phase with `max_i < envelope_floor - stability_tolerance` cannot be stable
and is removed. When clipping retained phase `i`, competitor `j` is skipped if
`max_j < min_i - stability_tolerance`; `i` is discarded immediately if
`min_j > max_i + stability_tolerance`. These are affine bounds, not winner or
distance heuristics. Candidate traversal uses descending `max_i` and then
canonical phase ID, while output is invariant to input phase order.

## Target intersection and path topology

For each cached stable polygon, extraction intersects one affine line:

```text
height mode:    h_i = L
secondary mode: q_i = L
```

Triangle target ranges reject impossible phase/level pairs before intersection.
A tangential one-point contact is diagnostic and never becomes a zero-length
path. A positive-length target line coincident with a stable boundary returns
`CoincidentTargetSegment`; a positive-area top-height tie returns
`PositiveAreaHeightTie`. Neither degeneracy is collapsed to a point or assigned
silently by phase ID.

Local segments retain private phase, umbrella triangle, endpoint source,
canonical umbrella edge, and tied-phase metadata. Global assembly joins only
equal phase IDs at the same level and compatible canonical coordinates. It
never joins different phases.

### Forward progress and anti-zigzag behavior

Every local line has a canonical unit tangent in semantic `(a,b)` coordinates.
All exact intersection events are projected onto that line, sorted by parameter,
and accepted only when the next parameter exceeds the previous by more than
`parameter_tolerance`. Near-coincident parameter and geometry events merge.
An edge root outside `[0,1]` (with tolerance) is rejected. There are no
provisional solver points in output.

During path assembly, a continuation is oriented away from its shared endpoint.
Non-junction continuations require positive normalized tangent alignment.
Immediate retracing, a duplicate directed state, branching, or non-monotone
local events return typed errors. Cumulative path arclength therefore increases
strictly between output points. Exact source-mesh edge-piercing positions are
used only for source evaluation; final points are exclusively intersections of
target lines with stable polygons on the umbrella grid.

## Univariants, invariants, and secondary contacts

At every stable-boundary endpoint, all triangle heights are evaluated and all
phases within `stability_tolerance` of the maximum are sorted.

- Two tied phases at a height contour produce `Univariant`.
- Three or more tied phases produce `Invariant`; the implementation does not
  assume exactly three.
- A secondary contour endpoint produces `StableBoundaryContact`.

For height contours, phase-specific paths end at one canonical shared coordinate
and reference the same junction ID. Paths remain separate across phase IDs.
Pairwise equalities below another phase are absent because the upper-envelope
clipping has already removed them.

For secondary contours, `q_A=L` and `q_B=L` generally reach the same height
boundary at different compositions. Those paths terminate independently and
retain the same tied height-phase set as metadata. They share a junction only
when their coordinates genuinely coincide within geometry tolerance.

## Verification and global refinement

`StableUmbrellaVerification` optionally samples every umbrella triangle at its
centroid and/or edge midpoints. Original prepared source values are compared
with umbrella-affine predictions. A triangle is unresolved when a configured
height or secondary residual is exceeded, stable phase sets differ, or a direct
stable phase is absent from the affine candidate set.

When enabled, refinement doubles the global umbrella subdivisions (bounded by
`maximum_subdivisions`) and repeats sampling and verification. Source cubic
models are not reconstructed. If unresolved triangles remain at the limit,
preparation returns `UmbrellaResolutionInsufficient` unless
`allow_unresolved=true` was selected explicitly. Per-pass and final diagnostics
report residuals, ownership mismatches, hidden candidates, and the worst
triangle.

Verification is a practical finite sampling check. It is not interval
certification and cannot prove that no feature exists between verification
points. Results are exact for the final piecewise-linear umbrella
representation, not necessarily for original nonlinear source interpolants.

## Complexity and memory

For `P` phases, `S` sampled scalar layers, `V` umbrella vertices, and `T`
umbrella triangles:

- source sampling is `O(S*V)`, with point location reduced to one operation per
  geometry group and point;
- stable partitioning is worst-case `O(T*P^2)`, reduced by exact range pruning
  and early empty-polygon termination;
- level extraction visits cached nonempty phase polygons and performs no source
  evaluation or stable re-clipping;
- sampled memory is `O(P*V)` in height mode and `O(2*P*V)` in secondary mode,
  plus compact cached convex polygons.

Diagnostics quantify actual pruning and reuse. No public wall-clock claim is
made.

## Limitations and roadmap

This milestone intentionally excludes partial-domain sources, holes,
constrained Delaunay edges, direct mixed-mesh continuation, nonlinear source
envelope topology, local adaptive umbrella refinement, certified interval or
Bezier source bounds, stable filled regions, rendering, parallel/GPU execution,
and language ABIs.

Deferred work includes:

- exact clipping of partially overlapping source domains;
- hierarchical local umbrella refinement;
- certified nonlinear source bounds;
- direct cubic stable boundaries and invariant solves;
- a level-independent stable phase atlas and filled regions;
- seeded random irregular-mesh ensembles with target area, aspect-ratio, and
  anisotropic metric distributions;
- a neutral C ABI after the Rust ownership, error, and result layout is stable.

Run the complete numerical example with:

```text
cargo run --example stable_phase_contours --features irregular-delaunay
```