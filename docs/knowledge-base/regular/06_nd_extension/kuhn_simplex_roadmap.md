# Roadmap to N-Component Simplex and Kuhn-Simplex Grids

## Purpose

The current implementation is intentionally specialized to a regular 2-D ternary triangular lattice. A future crate may generalize the field construction to an N-dimensional composition simplex and a parameter hypercube decomposed into Kuhn simplices.

Do not import Kuhn-simplex machinery into the current ternary crate.

## What should remain reusable

The following concepts should be designed independently of Plotters and 2-D rendering:

- compact directed alpha interval;
- interval reversal;
- edge-family coefficient preparation;
- local simplex field interface;
- pairwise excess contribution;
- extrapolation policy;
- value and reduced-gradient evaluation;
- implicit level projection;
- approximately uniform path/surface sampling concepts;
- diagnostics and error handling.

## General simplex coordinates

For an N-component composition simplex:

\[
\sum_{i=0}^{N-1}x_i=1,
\qquad x_i\ge0.
\]

A pairwise model can retain the form

\[
E_{ij}=x_ix_j\left(\alpha_{ij,0}+\alpha_{ij,1}t_{ij}\right).
\]

## Multicomponent Muggianu extension

For pair `i-j`, distribute the total fraction of all remaining components equally between the pair:

\[
r=1-x_i-x_j.
\]

Then

\[
X_i^{\mathrm M}=x_i+\frac{r}{2},
\qquad
X_j^{\mathrm M}=x_j+\frac{r}{2}.
\]

For directed `i -> j`:

\[
t_{ij}^{\mathrm M}=x_j+\frac{1-x_i-x_j}{2}
=\frac{1-x_i+x_j}{2}.
\]

This depends only on the pair difference and remains naturally centered.

## Multicomponent Kohler extension

\[
t_{ij}^{\mathrm K}=\frac{x_j}{x_i+x_j},
\]

with contribution zero when `x_i=x_j=0`.

The prefactor remains

\[
x_ix_j.
\]

## Local field on a simplex

A direct generalization is

\[
f(x)=\sum_i f_ix_i+\sum_{i<j}E_{ij}.
\]

However, several questions must be studied before claiming a general interpolation method:

- which one-dimensional grid lines provide each pair's alpha interval inside a high-dimensional grid;
- whether all simplex edges have sufficient neighboring samples;
- continuity across adjacent Kuhn simplices;
- permutation invariance under different dependent-coordinate choices;
- whether pairwise terms alone adequately reproduce target cross-coupling;
- topology extraction for hypersurfaces.

## Why Kuhn simplices later

Advantages:

- universal decomposition of an N-dimensional hypercube;
- simple generation from coordinate permutations;
- deterministic adjacency;
- direct connection to regular product grids.

Disadvantages:

- elongated simplex shapes;
- orientation anisotropy;
- more complicated conditioning;
- unnecessary loss of symmetry in the 2-D ternary case.

Therefore:

- regular equilateral-like triangular lattice now;
- Kuhn-simplex ND crate later.

## Future contour generalization

In D-dimensional reduced composition space, an isolevel is a `(D-1)`-dimensional manifold.

The current polyline regularization generalizes only conceptually. Future work needs:

- simplex marching for hypersurface facets;
- adaptive subdivision;
- manifold stitching;
- remeshing or point redistribution on surfaces;
- projection onto the implicit field using
  \[
  \Delta x=-F\nabla F/\|\nabla F\|^2;
  \]
- constraints that keep points inside the global composition simplex.

## Recommended future crate boundary

```text
simplex-alpha-field
    N-component field construction and extrapolation

kuhn-grid
    regular ND grid and Kuhn-simplex connectivity

simplex-isosurfaces
    topology extraction and manifold projection
```
