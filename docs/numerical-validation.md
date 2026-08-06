# Numerical validation

This report records the maintained deterministic validation matrix for
`ternary-contours`. It covers the numerical core only: it does not assess
rendering, chart projection, or viewport clipping.

## Scope and execution

The tests are implemented in `tests/numerical_validation.rs`, with shared,
test-only field and metric helpers in `tests/support/numerical.rs`. They run
without optional dependencies where possible and expand automatically under
`cubic-alpha`, `irregular-delaunay`, and `irregular-cubic-alpha`.

The suite is deterministic: it uses fixed-seed integer pseudo-random samples,
fixed analytic fields, stable mesh inputs, and canonical path comparisons. It
does not use wall-clock assertions.

## Coordinates and metrics

All input and output positions remain semantic compositions `(a,b,c)`, with
`c = 1-a-b`. Field gradients are assessed in the independent semantic
coordinates `(a,b)`. Length, spacing, and Hausdorff-style path comparisons use
the canonical logical equilateral plane

```text
A = (0, 0), B = (1, 0), C = (1/2, sqrt(3)/2).
```

The harness records maximum absolute and RMS scalar error, maximum absolute
error for both gradient components, contour-point level residual, logical path
length, chord-spacing coefficient of variation, and an approximate symmetric
Hausdorff distance obtained by uniformly sampling each path segment.

## Analytic catalogue

The maintained cases are deliberately small and interpretable:

- an affine semantic field, with exact value and gradient expectations;
- a pairwise quadratic field, with exact analytical gradient and expected
  piecewise-linear refinement trends;
- a saddle field for adaptive cubic-contour depth limiting;
- value-scale and offset transformations of the affine field;
- finite extreme values whose derived gradient overflows, to verify typed
  rejection rather than misreporting the condition as a malformed location.

Regular tests use subdivisions `1, 2, 7, 14, 28, 29`. Irregular tests use a
non-cocircular cloud spanning the simplex, the same cloud in reverse input
order, and a deliberately high-aspect-ratio but nondegenerate cell. Existing
unit tests additionally cover invalid compositions, duplicate samples,
cocircular backend policy, simplex boundaries, ownership, and outside-hull
queries.

## Acceptance envelopes and findings

| Area | Maintained check | Result / interpretation |
| --- | --- | --- |
| Regular linear, affine | 257 deterministic queries at each tested grid size | Value, `(a,b)` gradient, location reconstruction, and accepted near-normalization agree within `2e-10`. |
| Regular linear, quadratic | 1,001 fixed points at each of `n=7,14,28` | Maximum value error contracts by more than `0.35` per doubling; maximum gradient error contracts by more than `0.6`. These are conservative regression envelopes, not a public error bound. |
| Regular contours | affine levels, repeated computation | Debug representation and sampled geometry are identical; residual is below `2e-10`; logical lengths and spacing statistics are finite. |
| Regular cubic-alpha | all Akima, MAKIMA, PCHIP, and Steffen methods; RawBarycentric, Muggianu, and Kohler | All tested vertex values reproduce samples, shared-edge values agree to `2e-10`, repeated values are identical, and analytic gradients agree with centred differences within `2e-5`. Muggianu and Kohler are intentionally observed to differ in the interior. |
| Regular cubic contours | tight-depth saddle ablation | Repeated geometry is identical and maximum-depth diagnostics are nonzero rather than silently dropping unresolved detail. |
| Irregular linear | affine field, regular-vs-irregular queries | Both evaluators reproduce affine values and gradients to `2e-10`; nondegenerate topology is unchanged after reversing input order when compared by semantic triangle vertices. |
| Irregular topology stress | high-aspect-ratio cell | Every triangle-centroid value and gradient remains finite and retains affine accuracy with looser conditioning-aware envelopes. |
| Irregular cubic-alpha | PCHIP/Kohler prepared field and adaptive contour | Vertex reproduction, finite samples, preparation diagnostics, contour geometry, and adaptive accounting are deterministic. |
| Non-finite derived result | finite extreme vertex values | Regular and irregular evaluators return `NonFiniteEvaluation`; no non-finite value or gradient is returned as a successful sample. |
| Stable sampling-grid ensembles | affine height/secondary fields, mixed geometries, hidden interior phase, six-phase envelope, refinement, and forward-progress adversarial cases | Exact target and upper-envelope residual checks pass; metastable equalities are removed; location reuse and safe pruning are diagnostic; retracing, branches, cycles, and positive-length degeneracies are typed. |

The comparison between regular and irregular fields is exact only for affine
data. For nonlinear data the two meshes sample and interpolate different local
triangles, so their outputs must be compared against the analytical field and
quality metrics, not forced to equal each other.

## Tolerances and stabilization decisions

`POINT_LOCATION_TOLERANCE` remains `1e-10`. It accepts a narrowly
near-normalized finite composition, then normalizes and snaps it before
location. It is not a general projection tolerance. Contour value and geometry
tolerances remain explicit per contour option family; cubic adaptive controls
remain explicit and bounded.

The audit did not identify evidence that changing any default numerical
threshold would improve correctness. Defaults were therefore retained. The
only behavior hardening is an additive `NonFiniteEvaluation` error on both
regular and irregular prepared evaluators. This catches overflowed values or
analytic gradients derived from otherwise finite vertex samples.

`ContourSet::levels()` and `IrregularContourSet::levels()` were added as
additive borrowed accessors. The existing public `levels` fields remain intact
for 0.1.x compatibility.

## Determinism and performance smoke checks

Repeated construction and extraction compares the complete `Debug`
representation of result values, alongside numerical path metrics. This is a
strong regression check within a build configuration, rather than a promise of
stable text formatting across future Rust releases.

The pre-existing ignored timing smoke tests were run on the development
machine in a debug test build on 2026-08-01. They are observations only:

```text
n=96, 20,000 prepared queries: linear 91.9158 ms; cubic 95.3012 ms
n=192, 2,000 locations: direct 3.9713 ms; exhaustive reference 53.6214157 s
```

The exhaustive locator is retained only for tests. The direct regular locator
uses constant-sized local arithmetic, so its query cost does not scale with the
total number of elementary triangles. Timings vary by machine, build profile,
and compiler; they are not a benchmark claim or CI threshold.

## Stable sampling-grid validation

Stable tests compare one-phase height geometry against ordinary regular linear
contours, reproduce affine phase boundaries, validate two-phase univariants and
three/four-phase invariants, and confirm that a central stable polygon is found
when its phase wins no sampling-grid-triangle vertex. A six-phase secondary contour
crosses the upper-envelope order `A -> D -> E -> C -> F -> B` without recursive
pairwise repair. Dense checks evaluate every emitted point against its target
level and every competing sampling-grid height.

Verification tests start from a coarse sampling-grid over a finer nonlinear source,
observe decreasing centroid/midpoint residuals under deterministic global
doubling, and require explicit `allow_unresolved` at a configured limit.
Regular and irregular geometry groups verify point-location reuse, complete
simplex coverage, and exact irregular mesh-identity pairing. Optional-feature
tests prepare regular and irregular cubic-alpha sources once before sampling.

Forward-progress regressions reject extrapolated edge roots, sort corrected
events before emission, merge near-coincident events, cross exact shared-edge
and multi-cell vertex hits without using backward canonical ownership, and
return typed errors for immediate retracing, branching, and duplicate directed
states. Output points come only from sampling-grid stable-polygon intersections.

These checks validate the final affine sampling-grid model. Midpoint and centroid
verification does not certify that an original nonlinear source contains no
smaller feature between samples.

## Stable-topology audit workflow

The repository also provides `ternary-contours-cli audit-stable-topology` for
fixture-specific stability investigations. It repeatedly executes the
production projection pipeline, records canonical node/edge signatures, and
separates exact-repeatability, tolerance-aware geometry movement, and
phase-set/connectivity changes. It is intentionally a diagnostic report rather
than a claim of universal topology convergence.

For the persisted EX CaO–PbO–ZnO detailed fixture, use the existing file
unchanged and retain its hash with the audit output. The expected raw network
at the production Cubic alpha/Akima/Muggianu/one-sided-then-linear options is
three binary invariants, one interior invariant, and three complete
univariants. Regularization is evaluated after that raw graph; a per-path
`RawFallback` is reported separately and does not remove an accepted raw node
or edge. `runs.tsv` additionally records pair-driven triplets attempted,
continuous roots converged, and roots attached to all incident pair branches;
`invariants.tsv` records the independently verified maximum equality residual
and stability margin for each interior node. Failed calculations retain their
typed error in the final `error` column of `runs.tsv`.

The detailed persisted-EX CaO–PbO–ZnO audit uses its source file unchanged.
At the canonical Cubic alpha/Akima/Muggianu/one-sided-then-linear model it is
repeatable through sampling subdivisions 6–40 and has a 3-binary,
1-interior, 3-complete-univariant topology. The continuous interior solve is
approximately `(0.0948132375, 0.8568845056, 0.0483022569)` at
`815.9637563`, with a maximum equality residual below `6e-13` in the recorded
run. This is diagnostic fixture evidence, not a general physical accuracy
claim.

## Limits

These tests do not prove global interpolation error bounds, C1 continuity,
Delaunay uniqueness for cocircular input, or contour-topology invariance under
all threshold changes. Cubic-alpha fields are C0 but not guaranteed C1 across
elementary-triangle edges; an edge or vertex gradient remains that of the
deterministically selected owner triangle. Irregular support remains limited to
the convex hull of its samples, without holes or constrained edges.

Irregular cubic-alpha filled bands, constrained meshing, and parallel or C ABI
execution remain outside this validation milestone.
