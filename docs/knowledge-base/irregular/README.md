# Irregular implementation status

`ternary-contours` now implements the first irregular numerical foundation behind
its `irregular-delaunay` feature: a private `delaunay` backend, crate-owned
dense mesh IDs and topology, robust backend-assisted location in the convex
hull, semantic barycentric coordinates, prepared piecewise-linear values and
analytic global `(a,b)` gradients. Canonical edges, incident triangles, hull
edges, and dense triangle/edge arrays are available for the planned edge-alpha
work.

The alpha estimation note below remains authoritative for the next milestone.
Virtual stencils, synchronous fixed-point alpha sweeps, irregular cubic-alpha,
and irregular contour/band construction are not implemented yet.
# Supplement to the ternary cubic contour knowledge base

This archive adds one technical note:

- `irregular-triangulation-alpha-estimation.md`

It covers irregular triangular meshes, canonical collinear virtual edge stencils, containing-triangle evaluation, linear bootstrap, self-consistent alpha refinement, synchronous global sweeps, dependency-driven local updates, convergence diagnostics, boundary handling, validation, and the intended contour-core/plotting-layer separation.

Excluded by design:

- alpha-bound estimates or admissible alpha intervals;
- compressed floating-point or integer alpha representations;
- fixed-point arithmetic;
- AI training or quantized neural-network arithmetic.
