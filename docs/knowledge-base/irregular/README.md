# Irregular implementation status

`ternary-contours` implements the irregular two-dimensional numerical foundation
behind `irregular-delaunay`: a private `delaunay` backend, crate-owned dense mesh
IDs and topology, robust backend-assisted location in the convex hull, semantic
barycentric coordinates, and prepared piecewise-linear values with analytic
global `(a,b)` gradients.

The documented edge-alpha construction is now implemented for pointwise scalar
fields behind `irregular-cubic-alpha`. It stores one alpha interval per
canonical undirected mesh edge; maps the two canonical collinear virtual points
into cached containing-triangle/barycentric locations once; uses a linear
bootstrap; and performs damped synchronous Jacobi sweeps to convergence. Hull
or simplex-exiting virtual stencils obey `LinearFallback` (zero alpha) or return
a typed `Error`. Diagnostics retain the full options, stencil/fallback counts,
residual history, worst edge, and convergence state. The public evaluator
returns values, analytic global gradients, locations, and allocation-friendly
batch results.

The alpha-estimation note below remains authoritative for the model. It is not
a proposal for a different smooth interpolation family. Its dependency-graph
and optional local-propagation optimizations are deferred; this implementation
uses only deterministic global Jacobi sweeps.

Irregular isolines are now implemented. Linear fields use deterministic
mesh-edge-owned marching triangles. Converged cubic-alpha fields use adaptive
local barycentric microtriangles, canonical roots of each shared cubic edge
interval, deterministic global path assembly, and optional equal-arclength
redistribution followed by global implicit-field projection. The physical
microtriangle threshold is measured in the canonical equilateral plane used by
the Delaunay embedding. Projection relocates after every accepted step and
backs out candidates outside the convex hull.

Milestone 16 adds deterministic metrics. `IrregularTernaryMesh::metrics()` now
reports Delaunay-only quality and topology records; shared gradient,
derived-field, local-quadratic curvature, edge-jump, and contour-response
analysis applies to regular and irregular fields alike. The mesh retains stable
incident-edge adjacency and compact per-edge cubic stencil-completion flags.
This changes implementation status only; it does not alter the authoritative
edge-alpha algorithm note below.

Milestone 17 can now consume irregular linear or cubic-alpha fields as sources
for stable-phase ensembles. Sources sharing one immutable mesh reuse locations
while being sampled onto a common regular umbrella. Irregular convex hulls must
cover the complete simplex; source triangulation edges are never inserted into
final stable contour geometry. Partial-domain phases and seeded random
irregular-mesh ensemble generation remain deferred.
Not implemented: irregular bands (including cubic-alpha filled bands), virtual-
stencil persistence APIs, or parallel/local solver execution.

# Supplement to the ternary cubic contour knowledge base

This archive adds one technical note:

- `irregular-triangulation-alpha-estimation.md`

It covers irregular triangular meshes, canonical collinear virtual edge stencils, containing-triangle evaluation, linear bootstrap, self-consistent alpha refinement, synchronous global sweeps, dependency-driven local updates, convergence diagnostics, boundary handling, validation, and the intended contour-core/plotting-layer separation.

Excluded by design:

- alpha-bound estimates or admissible alpha intervals;
- compressed floating-point or integer alpha representations;
- fixed-point arithmetic;
- AI training or quantized neural-network arithmetic.
