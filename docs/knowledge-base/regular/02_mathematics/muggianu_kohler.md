# Muggianu and Kohler Interior Extrapolation

## Common pair structure

For a directed pair `i -> j`, let `k` be the remaining component. Every policy uses

\[
E_{ij}=x_ix_j\left(\alpha_0+\alpha_1t_{ij}\right).
\]

The prefactor is always the raw multicomponent product

\[
\boxed{x_ix_j}.
\]

Never replace it by normalized `X_i X_j`.

## Muggianu

### Centered-coordinate derivation

On the binary edge `i-j`, define the centered composition coordinate

\[
u=t-\frac12.
\]

Since `t=x_j` and `x_i+x_j=1` on that edge,

\[
u=\frac{x_j-x_i}{2}.
\]

The Muggianu extension preserves this centered difference in the ternary interior:

\[
u_{ij}=\frac{x_j-x_i}{2}.
\]

Therefore

\[
t^{\mathrm M}_{ij}
=\frac12+\frac{x_j-x_i}{2}
=x_j+\frac{x_k}{2}.
\]

The pair contribution is

\[
\boxed{
E^{\mathrm M}_{ij}
=x_ix_j\left[\alpha_0+\alpha_1\left(x_j+\frac{x_k}{2}\right)\right].
}
\]

### Geometric interpretation

Muggianu corresponds to perpendicular projection of the ternary point onto the binary edge. For the `i-j` edge, the projection transfers half of `x_k` to each binary component:

\[
X_i=x_i+\frac{x_k}{2},
\qquad
X_j=x_j+\frac{x_k}{2}.
\]

The invariant along perpendicular lines is

\[
x_j-x_i=\text{constant}.
\]

Constant individual barycentric coordinates are not perpendicular to the binary edge; they meet it at 60 degrees in an equilateral diagram.

### Reversal

For the reversed edge `j -> i`,

\[
t^{\mathrm M}_{ji}
=\frac12+\frac{x_i-x_j}{2}
=1-t^{\mathrm M}_{ij}.
\]

Therefore ordinary alpha reversal preserves the full interior pair contribution.

## Kohler

Normalize within the binary pair:

\[
X_i=\frac{x_i}{x_i+x_j},
\qquad
X_j=\frac{x_j}{x_i+x_j}.
\]

For directed pair `i -> j`,

\[
t^{\mathrm K}_{ij}=X_j=\frac{x_j}{x_i+x_j}.
\]

The contribution is

\[
\boxed{
E^{\mathrm K}_{ij}
=x_ix_j\left(\alpha_0+\alpha_1\frac{x_j}{x_i+x_j}\right).
}
\]

This is not

\[
X_iX_j(\alpha_0+\alpha_1X_j).
\]

At the third vertex, where `x_i=x_j=0`, define the pair contribution as exactly zero without evaluating the ratio.

### Geometric interpretation

Kohler preserves the ratio of the two binary components. It is a radial projection from the third vertex onto the `i-j` edge.

### Reversal

\[
t^{\mathrm K}_{ji}
=\frac{x_i}{x_i+x_j}
=1-t^{\mathrm K}_{ij}.
\]

Therefore ordinary alpha reversal preserves the full interior pair contribution.

## Binary-edge invariant

On the actual binary edge `x_k=0`,

\[
x_i+x_j=1,
\]

so

\[
t^{\mathrm M}_{ij}=t^{\mathrm K}_{ij}=x_j.
\]

Both policies exactly reproduce the same `spline1d` edge interval.

## Dilution behavior

For Kohler, keep the normalized binary ratio fixed and let

\[
s=x_i+x_j\to0.
\]

Then

\[
x_ix_j=s^2X_iX_j,
\]

so the pair contribution decays quadratically toward the third-component vertex.

## Policy API

```rust
pub enum BinaryExtrapolation {
    Muggianu,
    Kohler,
}
```

Do not conflate this enum with interpolation order. Linear interpolation remains a separate top-level mode with no alpha terms.
