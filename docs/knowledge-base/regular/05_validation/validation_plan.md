# Validation and Test Plan

## A. Alpha convention

1. Known asymmetric alpha interval values.
2. Incorrect `alpha1*(1-t)` convention must fail.
3. Direct-cubic to alpha round trip.
4. Non-unit interval width.
5. Direct and alpha APIs agree for Akima, Makima, PCHIP, and Steffen.
6. Left, middle, and right interval APIs all checked.
7. Reversal identity:
   \[
   (\alpha_0,\alpha_1)\to(\alpha_0+\alpha_1,-\alpha_1).
   \]

## B. Regular grid

1. Vertex-count formula.
2. Index-coordinate round trips.
3. Exact elementary-triangle count `n^2`.
4. Every geometric edge generated once.
5. Correct line-family assignment.
6. Correct left/middle/right stencil assignment.
7. Boundary fallback diagnostics.

## C. Local field invariants

1. Exact vertex reproduction.
2. Exact edge spline reproduction.
3. Adjacent triangles agree on shared-edge values.
4. Zero alpha gives linear field.
5. `alpha1=0` gives quadratic regular-solution-like form.
6. Muggianu and Kohler agree on binary edges.
7. Muggianu and Kohler agree when `alpha1=0`.
8. Raw prefactor remains `x_i*x_j`.
9. Kohler dilution scales quadratically at fixed pair ratio.
10. Finite third-vertex behavior.
11. Analytic gradients agree with centered finite differences.
12. Local vertex permutation invariance with preserved edge metadata.

## D. Muggianu geometry

1. In an equilateral triangle, `x_i=const` is parallel to the edge opposite vertex `i`.
2. Toward edge `i-j`, the perpendicular invariant is `x_j-x_i=const`.
3. Perpendicular projection gives
   \[
   X_i=x_i+x_k/2,\quad X_j=x_j+x_k/2.
   \]
4. Centered parameter
   \[
   u=(x_j-x_i)/2
   \]
   is preserved.
5. Reversal changes `u` to `-u` and `t` to `1-t`.

## E. Kohler geometry

1. Pair ratio is preserved.
2. `t=x_j/(x_i+x_j)`.
3. Third-vertex singular ratio is never evaluated.
4. Reversal gives `1-t`.

## F. Contour topology

1. Exact linear isolines.
2. Open paths.
3. Closed loops.
4. Vertex-level degeneracy.
5. Coincident edge ownership.
6. Tangency.
7. Multiple edge roots.
8. Interior loop in a nonlinear cell.
9. Adaptive refinement convergence.
10. Maximum-depth unresolved diagnostic.
11. Deterministic path ordering.
12. Degree-greater-than-two detection.

## G. Regularization and projection

1. Spacing variance decreases.
2. Every projected point satisfies `|f-L|` tolerance.
3. Open endpoints preserved.
4. Closed paths remain periodic with no duplicated endpoint.
5. Orientation preserved.
6. Projection can cross elementary-triangle boundaries.
7. Backtracking reduces residual.
8. Zero-gradient condition handled.
9. Maximum step enforced.
10. Repeated passes improve or preserve spacing quality.
11. Linear and cubic modes use their own correct field during projection.

## H. Analytic benchmarks

Use fields with known behavior:

### Linear plane

\[
f(a,b,c)=2a-3b+5c.
\]

Linear contouring should recover exact straight lines.

### Pairwise quadratic

\[
f=f_1a+f_2b+f_3c+A_{12}ab+A_{23}bc+A_{13}ac.
\]

Cubic-alpha with `alpha1=0` should reproduce this local form.

### Synthetic cubic-alpha field

Construct known asymmetric edge intervals and verify level residuals, orientation, and edge recovery.

### Smooth reference functions

Sample smooth analytic functions at increasing grid resolution and compare against a dense reference.

Metrics:

- maximum scalar error;
- RMS scalar error;
- Hausdorff-type contour error;
- contour-length error;
- maximum level residual;
- spacing coefficient of variation;
- runtime and refinement count.

## I. Ablation study

Compare:

1. linear contours;
2. cubic edge roots only;
3. complete cubic field;
4. cubic field plus adaptive topology;
5. cubic field plus redistribution;
6. cubic field plus redistribution and projection.

This separates interpolation gains from simple point densification.
