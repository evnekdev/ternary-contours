# Stable-phase contours on a virtual regular sampling grid

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
    StableGridOptions,
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
    StableGridOptions::default(),
)?;
let contours = prepared.contours(&[0.3, 0.4])?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Preparation is deliberately separate from level extraction. Repeated calls to
`contours` reuse source interpolation models, sampling-grid samples, per-triangle
bounds, and stable polygons.

## Source interpolation and sampling-grid interpolation

A `StableScalarSource` selects the evaluator used while sampling:

| Source | Linear | Cubic-alpha |
| --- | --- | --- |
| Regular | Default build | `cubic-alpha` |
| Irregular Delaunay | `irregular-delaunay` | `irregular-cubic-alpha` |
| Explicit evaluator with defined/undefined results | Default build | Evaluator-defined |

Regular cubic-alpha sources use `CubicAlphaBuildOptions`. Irregular cubic-alpha
sources use `IrregularCubicAlphaOptions` and the already prepared synchronous
edge-alpha field. Muggianu, Kohler, and RawBarycentric remain continuation
policies inside those cubic-alpha models.

The compatibility `PreparedStablePhaseEnsemble::contours` entry point returns
the deterministic sampled-affine representation. Projection calculations use
`contours_with_stable_boundaries`: sampled cells provide finite deterministic
search regions, while transfer junctions and returned interior path points are
corrected through the prepared continuous source evaluator. Thus cubic-alpha
source choices affect the physical roots used by the Viewer and CLI projection;
they are not reduced to a second unrelated affine contour algorithm.

## One common regular topology

Preparation builds one `RegularTernaryGrid` with the requested sampling-grid
subdivision count. All phases are sampled at its vertices into dense
phase-major arrays:

```text
height[phase][sampling-grid vertex]
secondary[phase][sampling-grid vertex]  // secondary mode only
```

Regular layers with equal subdivisions form one geometry group. Irregular
layers form one group only when their mesh identities match. At each sampling-grid
or verification point, a group performs point location once and evaluates all
its scalar layers at that cached location. Irregular groups carry a deterministic
previous-triangle hint while points are visited in regular-grid row order.
Diagnostics expose group, location, reuse, and scalar-evaluation counts.

Regular sources cover the complete semantic simplex by construction. Explicit
evaluator sources may instead return `StablePhaseEvaluation::Undefined` with a
typed reason. Undefined phases are omitted from the local upper envelope; they
are never extrapolated. At least one phase must remain defined at every sampled
or queried point. A partially covered irregular convex hull still returns
`IncompleteSourceCoverage` during common-grid sampling because that source type
does not expose an explicit phase-domain policy.

## Exact affine stable regions

Inside one sampling-grid triangle all sampled heights are affine. The stable region
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
them. A phase can lose at all three sampling-grid-triangle vertices and still own an
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

Local segments retain private phase, sampling-grid triangle, endpoint source,
canonical sampling-grid edge, and tied-phase metadata. Global assembly joins only
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
target lines with stable polygons on the sampling grid.

## Univariants, invariants, and secondary contacts

At every stable-boundary endpoint, all triangle heights are evaluated and all
phases within `stability_tolerance` of the maximum are sorted.

- Two tied phases at a height contour produce `Univariant`.
- Exactly three tied liquidus phases produce `Invariant`. Four or more tied
  solids are an overdetermined fixed-pressure ternary event and return a typed
  error rather than becoming graph or contour topology.
- A secondary contour endpoint produces `StableBoundaryContact`.

For height contours, phase-specific paths end at one canonical shared coordinate
and reference the same junction ID. Paths remain separate across phase IDs.
Pairwise equalities below another phase are absent because the upper-envelope
clipping has already removed them.

For secondary contours, `q_A=L` and `q_B=L` generally reach the same height
boundary at different compositions. Those paths terminate independently and
retain the same tied height-phase set as metadata. They share a junction only
when their coordinates genuinely coincide within geometry tolerance.

## Level-free stable-boundary network

`PreparedStablePhaseEnsemble::stable_boundaries` is separate from level-specific
isotherm extraction. It first discovers stable transitions on the canonically
oriented AB, BC, and CA boundaries from the original prepared height evaluators.
Full phase sweeps are cached by boundary parameter; pair-only evaluations refine
a bracket only after the stable sequence identifies its two phases. A candidate
root is accepted only when the full upper-envelope evaluation confirms that both
phases are stable. Higher-order ties become one node with all sorted phase IDs.

The cached affine stable polygons then provide local phase-pair fragments. Dense
`RegularSamplingTopology` edge, triangle, and vertex identifiers canonicalize
shared events. Traversal starts from binary or interior invariant pending ends,
tracks directed states with dense epoch arrays, and constructs only complete
node-to-node paths. The resulting `StableBoundaryNetwork` exposes dense invariant
IDs, stable phase pairs, canonical node coordinates, path temperatures, incidence,
binary traces, and deterministic diagnostics. Unseeded isolated closed loops are
not silently emitted: their discovery is the explicitly deferred part of this
milestone.

Raw topology is constructed and validated before any cleanup. Set
`StableBoundaryOptions::regularization` to `Some(PathRegularizationOptions)` to
request the shared numerical path post-process. Ordinary isotherms and stable
univariants use the same finite-coordinate cleanup, canonical logical arclength,
chord redistribution, normalization, damped Newton correction, and backtracking.
The projection equations remain distinct:

```text
isotherm:      T_p(x) - L = 0
univariant:    T_p(x) - T_q(x) = 0
```

Univariant projection also requires both pair phases to be defined and no other
defined phase to exceed them beyond `stability_tolerance`. Each candidate is
relocated in the sampling grid and constrained to its raw segment neighbourhood.
Binary and interior invariant endpoints are graph-owned and never moved per
path. Connectivity, phase-pair ownership, direction, duplicate removal, and
self-intersection state are revalidated afterward. Per-path
`StableUnivariantRegularizationDiagnostics` reports raw/final size and length,
spacing variation, accepted/backtracked projections, undefined/unstable
rejections, triangle relocations, and maximum pair residual.

See [stable-boundary-network.md](stable-boundary-network.md) and run:

```text
cargo run --release --example stable_boundary_network
```
## Verification and global refinement

`StableGridVerification` optionally samples every sampling-grid triangle at its
centroid and/or edge midpoints. Original prepared source values are compared
with sampling-grid-affine predictions. A triangle is unresolved when a configured
height or secondary residual is exceeded, stable phase sets differ, or a direct
stable phase is absent from the affine candidate set.

When enabled, refinement doubles the global sampling-grid subdivisions (bounded by
`maximum_subdivisions`) and repeats sampling and verification. Source cubic
models are not reconstructed. If unresolved triangles remain at the limit,
preparation returns `SamplingResolutionInsufficient` unless
`allow_unresolved=true` was selected explicitly. Per-pass and final diagnostics
report residuals, ownership mismatches, hidden candidates, and the worst
triangle.

Verification is a practical finite sampling check. It is not interval
certification and cannot prove that no feature exists between verification
points. Results are exact for the final piecewise-linear sampling-grid
representation, not necessarily for original nonlinear source interpolants.

## Complexity and memory

For `P` phases, `S` sampled scalar layers, `V` sampling-grid vertices, and `T`
sampling-grid triangles:

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


## Simulated liquidus gallery

The companion `plotters-ternary` repository contains a reproducible SVG gallery
of synthetic stable-phase isotherms. It uses the public stable-contour API and
`LiquidusFieldSpec` analytical constructors; no image is hand-drawn and no
random source is used. These are numerical validation examples, not assessed
thermodynamic systems.

The executable is deterministic and writes one combined gallery plus individual
panels under `plotters-ternary/docs/images/`:

```text
cargo run --release --example stable_isotherm_gallery
```

The panels use these fixed specifications (all fields are sampled onto the
listed regular source grid before stable extraction):

| Panel | Phase maxima | Grid / levels | Feature |
| --- | --- | --- | --- |
| corner-symmetric | A, B, C: `100`, isotropic `80` | `n=24`, 25–90 | symmetric ownership breaks and junctions |
| corner-steepness | A/B/C: `100`, steepness 42/82/170 | `n=24`, 25–92 | unequal field widths |
| corner-maxima | A/B/C: 112/102/94 | `n=24`, 25–108 | unequal maximum temperatures |
| edge-maxima | AB(.35), AC(.55), BC(.45) | `n=28`, 35–96 | binary-edge congruent maxima |
| corner-edge | A, AB(.38), BC(.58) | `n=28`, 30–96 | mixed corner/edge topology |
| interior-maximum | interior `(0.34,0.36,0.30)` plus A/B | `n=30`, 55–105 | central stable region |
| interior-maxima | three interior centres | `n=30`, 30–96 | overlapping interior pockets |
| mixed-topology | A, BC(.44), two interiors | `n=30`, 35–98 | repeated ownership transitions |
| narrow coarse/refined | same narrow centre, steepness 900 | `n=8` / `n=32` | resolution-dependent detection |
| metastable-pair | A/B 100, central C 108 | `n=24`, 70–99 | raw A=B equality suppressed by C |
| asymmetric-fields | three explicit directional matrices | `n=30`, 40–100 | elongated and rotated paths |
| secondary-scalar | A/B/interior heights plus phase-specific `q` | `n=28`, 0.20–0.80 | height-gated independent contacts |

The coarse/refined pair is also covered by a numerical regression test: phase 2
has no stable path at `n=8` for level `100.2`, then becomes present at `n=32`.
The secondary panel traces phase-specific secondary fields only inside regions
owned by the height envelope; it does not change phase ownership.
## Limitations and roadmap

This milestone supports explicit partial-domain evaluators, but still excludes holes,
constrained Delaunay edges, direct mixed-mesh continuation, nonlinear source
envelope topology, local adaptive sampling-grid refinement, certified interval or
Bezier source bounds, stable filled regions, rendering, parallel/GPU execution,
and language ABIs.

Deferred work includes:

- exact clipping of partially overlapping source domains;
- hierarchical local sampling-grid refinement;
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

## Continuous level topology

Projection-facing contour extraction is level-specific but consumes the accepted
level-free `StableBoundaryNetwork`; it never rediscovers phase ownership from
sampled polygon edges.  For every stable univariant `(A,B)` and requested
height level `L`, a transfer candidate is isolated along the branch and
continuously corrected with:

```text
H_A - H_B = 0
H_A - L = 0
```

`H_B - L` is then verified.  A regular transfer has exactly one `A` and one
`B` phase-labelled contour half-edge.  The API records these incidences in
`StableContourLevel::half_edges`; it does not infer a phase switch merely from
two path endpoints that happen to be geometrically close.

Secondary scalar contours use the same stable height skeleton.  A phase switch
requires `S_A=L` *and* `S_B=L`; when only one phase satisfies its scalar level,
the result is an explicit `OneSidedSecondaryContact` rather than a fabricated
exit.  A requested height equal to a canonical three-solid interior invariant
is `InvariantLevelCoincidence`, not a generic two-phase transfer.  Four-solid
nodes are overdetermined in the fixed-pressure ternary model and are rejected.

Root isolation samples every accepted branch interval deterministically before
continuous correction.  Multiple roots keep their branch ID, phase pair, full
precision point, and verification evidence; triangle IDs and rounded display
coordinates are never their identity.  Degenerate or insufficiently assembled
incidence is retained as a typed junction diagnostic, so unrelated level paths
remain available.

`stable_contour_signature` and `compare_stable_contours` provide topology-only,
tolerance-aware, and exact-diagnostic comparisons for repeatability, level
cache tests, and raw/regularized audits.  Changing requested levels reuses the
accepted stable-boundary network; only level roots and phase-labelled contour
paths are rebuilt.
