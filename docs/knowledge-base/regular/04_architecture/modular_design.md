# Modular Architecture

## Immediate module boundaries

Suggested structure inside the current crate:

```text
src/contour/
    mod.rs
    regular_grid.rs
    linear.rs
    topology.rs
    paths.rs
    regularize.rs
    project.rs
    render.rs

src/interpolation/
    mod.rs
    alpha_interval.rs
    line_family.rs
    cubic_triangle.rs
    extrapolation.rs
```

Exact filenames may differ, but preserve these conceptual boundaries.

## Dependency direction

```text
regular grid/indexing
        ↓
1-D interval preparation
        ↓
local interpolation field
        ↓
contour topology extraction
        ↓
path regularization/projection
        ↓
Plotters rendering adapter
```

The numerical layers must not depend on:

- Plotters backends;
- drawing areas;
- chart orientation;
- screen coordinates;
- legends;
- SVG or PNG details.

## Suggested core traits

A future extraction can center around a small field interface:

```rust
pub trait LocalSimplexField<const D: usize> {
    fn value(&self, barycentric: &[f64; D + 1]) -> f64;
    fn gradient_reduced(&self, reduced: &[f64; D]) -> [f64; D];
}
```

Rust stable const-generics may require a different representation for `D+1`; do not force this exact syntax prematurely. The conceptual interface matters more than the initial type signature.

For the current 2-D implementation, concrete optimized types are acceptable.

## Top-level interpolation API

```rust
pub enum ContourInterpolation {
    Linear,
    CubicAlpha(CubicAlphaOptions),
}

pub enum CubicAlphaMethod {
    Akima,
    Makima,
    Pchip,
    Steffen,
}

pub enum BinaryExtrapolation {
    Muggianu,
    Kohler,
}
```

Keep interpolation order separate from extrapolation geometry.

## Future crate split

Likely eventual arrangement:

```text
simplex-field-core
    alpha intervals
    local simplex fields
    contour topology
    regularization/projection

ternary-contours
    regular ternary grid
    2-D triangular specialization

plotters-ternary
    plotting adapter and chart integration

simplex-contours-nd
    N-component/Kuhn-simplex grids
```

## Cross-language architecture

After the core API stabilizes:

```text
Rust numerical core
    ↓ stable C ABI
Python / MATLAB MEX / Fortran ISO_C_BINDING
```

Use flat result arrays across the ABI:

```text
levels
path_level_indices
path_offsets
path_closed
points_barycentric
```

Do not expose Rust `Vec`, Rust enums without explicit representation, or Rust-owned strings through the ABI.
