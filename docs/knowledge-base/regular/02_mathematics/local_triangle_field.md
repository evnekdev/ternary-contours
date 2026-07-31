# Local Cubic-Alpha Field on an Elementary Triangle

## Barycentric coordinates

For one elementary triangle:

\[
x_1+x_2+x_3=1,
\qquad x_i\ge0.
\]

Corner values are `f1,f2,f3`.

## Linear part

\[
f_{\mathrm{lin}}=f_1x_1+f_2x_2+f_3x_3.
\]

## Pair contributions

For every canonically directed edge `i -> j`, define an alpha interval with coefficients `alpha0_ij, alpha1_ij`.

The contribution is always

\[
E_{ij}=x_ix_j\left(\alpha_{ij,0}+\alpha_{ij,1}t_{ij}\right).
\]

The extrapolation policy determines `t_ij`; it never changes the prefactor `x_i x_j`.

## Complete field

\[
f(x_1,x_2,x_3)
=f_{\mathrm{lin}}+E_{12}+E_{23}+E_{13}.
\]

The pair labels above are conceptual; the implementation must retain each edge's canonical direction and transform local coordinates accordingly.

## Vertex reproduction

At a vertex, at least one factor of every pair product `x_i x_j` is zero. Therefore all excess terms vanish and

\[
f(V_i)=f_i.
\]

## Edge reproduction

On edge `i-j`, the third barycentric coordinate is zero. The only surviving excess term is `E_ij`; the other two pair products vanish. Both Muggianu and Kohler reduce to the same directed binary parameter `t=x_j` on the edge. Therefore the local field exactly reproduces the source one-dimensional alpha interval.

## Linear reduction

If all alpha coefficients are zero,

\[
f=f_{\mathrm{lin}}.
\]

## Constant-alpha reduction

If every `alpha1` is zero,

\[
E_{ij}=\alpha_{ij,0}x_ix_j,
\]

which is a regular-solution-like quadratic excess term. Muggianu and Kohler coincide in this case.

## Continuity

Adjacent elementary triangles share the same edge interval. Consequently the field is exactly `C0` across shared edges, assuming both triangles reference the same directed edge model.

Full `C1` continuity is not guaranteed. Normal derivatives may differ across a shared edge because the third pairwise contributions differ on the two sides. The contour projection algorithm must tolerate piecewise-smooth gradients.

## Reduced coordinates

For analytic evaluation and gradients, use

\[
u=x_1,\qquad v=x_2,\qquad x_3=1-u-v.
\]

Implement both

```rust
fn value(&self, bary: [f64; 3]) -> f64;
fn gradient_reduced(&self, u: f64, v: f64) -> [f64; 2];
```

Analytic gradients are required. Finite differences are for validation only.
