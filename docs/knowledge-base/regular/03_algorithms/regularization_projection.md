# Equal-Arclength Regularization and Level Projection

## Motivation

Initial contour vertices arise from cell boundaries and adaptive subdivision. Their spacing is irregular and can produce visually uneven polylines.

Generic geometric smoothing is unsuitable because it may move the curve away from the requested isovalue.

The proposed method changes parameterization, then re-enforces the field constraint.

## Redistribution

Given provisional points

\[
p_0,p_1,\ldots,p_m,
\]

compute cumulative chord lengths

\[
s_0=0,
\qquad
s_i=s_{i-1}+\|p_i-p_{i-1}\|.
\]

Choose spacing `h` and target positions

\[
0,h,2h,\ldots.
\]

Interpolate provisional guesses at those cumulative lengths.

For open curves, preserve endpoint locations unless an explicit boundary policy says otherwise.

For closed curves, distribute points periodically and do not duplicate the first point at the end.

## Projection onto the implicit level

Define

\[
F(u,v)=f(u,v)-L,
\]

with reduced barycentric coordinates

\[
x_1=u,\quad x_2=v,\quad x_3=1-u-v.
\]

The normal/Newton correction is

\[
\Delta q=-\frac{F(q)}{\|\nabla F(q)\|^2}\nabla F(q).
\]

Apply:

- maximum step length;
- finite-value checks;
- gradient-norm guard;
- residual-decreasing backtracking;
- iteration limit;
- level-residual and geometric stopping tolerances.

## Crossing triangle boundaries

A projection step may move the point into a neighboring elementary triangle.

After every accepted move:

1. locate the containing grid triangle;
2. evaluate that triangle's interpolation model;
3. continue with its analytic gradient.

The projection acts on the global piecewise field, not on a permanently fixed starting triangle.

`C0` continuity is required; `C1` continuity is not assumed.

## Repeated passes

A practical sequence is

```text
redistribute -> project -> recompute length -> redistribute -> project
```

Two or three passes may improve spacing uniformity.

## Suggested options

```rust
pub struct ContourRegularization {
    pub spacing: f64,
    pub redistribution_passes: usize,
    pub projection_tolerance: f64,
    pub max_projection_iterations: usize,
    pub max_normal_step: f64,
}
```

## Diagnostics

Record:

- projection iterations;
- nonconverged points;
- zero-gradient encounters;
- backtracking counts;
- triangle-boundary crossings;
- spacing variance before and after;
- maximum level residual.
