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

Not implemented: irregular isolines, irregular bands (including cubic-alpha
filled bands), virtual-stencil persistence APIs, or parallel/local solver
execution.

# Supplement to the ternary cubic contour knowledge base

This archive adds one technical note:

- `irregular-triangulation-alpha-estimation.md`

It covers irregular triangular meshes, canonical collinear virtual edge stencils, containing-triangle evaluation, linear bootstrap, self-consistent alpha refinement, synchronous global sweeps, dependency-driven local updates, convergence diagnostics, boundary handling, validation, and the intended contour-core/plotting-layer separation.

Excluded by design:

- alpha-bound estimates or admissible alpha intervals;
- compressed floating-point or integer alpha representations;
- fixed-point arithmetic;
- AI training or quantized neural-network arithmetic.
