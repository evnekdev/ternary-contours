# Method Overview

## Input

A scalar value is attached to every vertex of a regular ternary composition grid with subdivision count `n`.

Grid vertices correspond to integer triples

\[
i+j+k=n
\]

and compositions

\[
(a,b,c)=\left(\frac{i}{n},\frac{j}{n},\frac{k}{n}\right).
\]

The number of vertices is

\[
N_v=\frac{(n+1)(n+2)}{2}.
\]

The regular lattice is subdivided deterministically into elementary upward and downward triangles.

## Two interpolation modes

### Linear

For a triangle with barycentric coordinates `x1,x2,x3` and corner values `f1,f2,f3`:

\[
f_{\mathrm{lin}}=f_1x_1+f_2x_2+f_3x_3.
\]

Every local isolevel is a straight segment, except for standard degeneracies.

### Cubic-alpha

Each of the triangle's three edges has a one-dimensional cubic interval represented by:

- two endpoint values;
- `alpha0`;
- `alpha1`.

The local field is

\[
f=f_{\mathrm{lin}}+E_{12}+E_{23}+E_{13}.
\]

Every pair contribution keeps the raw barycentric prefactor

\[
x_ix_j,
\]

while the parameter used inside the interaction polynomial is selected by an extrapolation policy:

\[
E_{ij}=x_ix_j\left(\alpha_{ij,0}+\alpha_{ij,1}t_{ij}\right).
\]

Required policies:

- Muggianu;
- Kohler.

## Edge interval source

Along every regular-grid line, use `spline1d` to calculate interval alpha coefficients. Supported methods initially include:

- Akima;
- Makima;
- PCHIP;
- Steffen.

Use left, middle, or right interval functions according to the interval's position on its one-dimensional grid line.

One unique directed grid edge must have one alpha interval. Adjacent triangles share that exact edge model.

## Nonlinear contour extraction

A cubic local field may have more complicated topology than a linearly interpolated triangle. A robust implementation should not assume exactly two edge crossings.

Preferred strategy:

1. evaluate the cubic local field;
2. adaptively subdivide the elementary triangle in barycentric space;
3. apply linear marching triangles to the refined microtriangles;
4. join local segments deterministically;
5. report unresolved topology if the refinement limit is reached.

## Contour regularization

A provisional contour has nonuniform point spacing inherited from triangle-edge crossings and adaptive subdivision.

Regularization:

1. estimate cumulative chord length;
2. place new provisional points at uniform arclength intervals;
3. discard the old nonuniform interior points;
4. project each new point onto `f = level` by moving in the local normal direction;
5. optionally repeat redistribution and projection.

The projection must use the same global piecewise interpolation field that generated the contour.

## Result

Return backend-independent contour levels and paths:

```rust
pub struct ContourSet {
    pub levels: Vec<ContourLevel>,
}

pub struct ContourLevel {
    pub value: f64,
    pub paths: Vec<ContourPath>,
}

pub struct ContourPath {
    pub points: Vec<TernaryPoint>,
    pub closed: bool,
}
```

Rendering and viewport clipping are downstream operations.
