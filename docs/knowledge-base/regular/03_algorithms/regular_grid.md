# Regular Ternary Grid and Edge Preparation

## Vertex indexing

Choose and document one canonical value ordering for all integer triples satisfying

\[
i+j+k=n.
\]

Required operations:

```text
linear index <-> (i,j,k) <-> ternary composition
```

All conversions must be checked for overflow and invalid coordinates.

## Elementary triangles

Generate deterministic upward and downward elementary triangles from neighboring lattice vertices.

For subdivision count `n`, the total number of elementary triangles is

\[
n^2.
\]

Verify this for several small grids by enumeration.

## Unique edges

Every elementary triangle uses three grid edges. Build canonical undirected/directed edge keys so each geometric edge is generated once.

A suitable directed convention is:

```text
smaller canonical GridVertexId -> larger canonical GridVertexId
```

Store one alpha interval per canonical directed edge.

## Three line families

Every regular-grid edge belongs to one of three line families corresponding to holding one lattice coordinate constant.

For each line:

1. enumerate vertices in a deterministic direction;
2. collect scalar values;
3. calculate interval coefficients with the selected `spline1d` method;
4. choose left/middle/right interval routines appropriately;
5. map every interval back to its canonical grid edge;
6. reverse alpha coefficients if line traversal opposes canonical edge direction.

## Boundary stencils

Some methods require wider stencils than short boundary lines provide.

Suggested policy:

```rust
pub enum CubicBoundaryPolicy {
    LinearFallback,
    ReducedStencil,
    Error,
}
```

Use `LinearFallback` as the practical default unless `spline1d` already provides a mathematically documented reduced stencil for the available samples.

Record diagnostic counts of fallback edges.

## No triangulation dependencies

The grid topology is known analytically. Do not add Delaunay or general triangulation crates.
