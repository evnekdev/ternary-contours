# Ternary Cubic-Alpha Contouring Knowledge Base

This bundle is a self-contained bootstrap for implementing, reviewing, extending, and publishing a contour-construction method for scalar data sampled on a regular ternary composition grid.

The method combines:

1. deterministic regular triangular-grid topology;
2. conventional piecewise-linear contouring as a baseline;
3. compact one-dimensional cubic intervals represented by endpoint values plus two alpha coefficients;
4. lifting the three edge intervals into a local triangle field;
5. selectable Muggianu or Kohler interior extrapolation;
6. topology-aware contour extraction from the nonlinear local field;
7. approximate equal-arclength redistribution;
8. normal/Newton projection of redistributed points back to the requested isolevel;
9. a modular architecture suitable for later extraction and extension to N-component simplex grids.

## Start here

Read in this order:

1. `00_context/project_context.md`
2. `01_method/method_overview.md`
3. `02_mathematics/alpha_interval.md`
4. `02_mathematics/local_triangle_field.md`
5. `02_mathematics/muggianu_kohler.md`
6. `03_algorithms/contour_pipeline.md`
7. `04_architecture/modular_design.md`
8. `05_validation/validation_plan.md`
9. `06_nd_extension/kuhn_simplex_roadmap.md`
10. `08_bootstrap/new_thread_prompt.md`

## Current scope

The immediate implementation target is a regular two-dimensional ternary grid. Do not introduce Delaunay triangulation, irregular scattered meshes, or Kuhn simplices into the current `plotters-ternary` implementation.

Kuhn simplices belong in a future independent N-dimensional crate. This bundle nevertheless documents how the interpolation kernel should be kept modular enough to support that future extraction.

## Terminology

- **Linear mode**: no alpha excess terms; the field is affine in each elementary triangle.
- **Cubic-alpha mode**: vertex-linear field plus three pairwise alpha excess terms.
- **Muggianu extrapolation**: preserves the centered binary asymmetry coordinate and corresponds geometrically to perpendicular projection onto the relevant binary edge.
- **Kohler extrapolation**: preserves the binary pair ratio by normalizing within the pair.
- **Regularization**: redistributing a provisional contour approximately uniformly in arclength and projecting each new point back onto the implicit isolevel.

## Non-goals

This bundle does not claim that each ingredient is individually unprecedented. The likely engineering and publication contribution is the validated integration of these ingredients into a usable, modular algorithm and software library.
