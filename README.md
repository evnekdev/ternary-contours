# ternary-contours

`ternary-contours` is a backend-independent numerical core for scalar data on a
regular two-dimensional ternary composition grid. It owns regular-lattice
indexing, scalar-field validation, shared directed cubic-alpha edge intervals,
and local cubic field evaluation with analytic gradients.

It intentionally does **not** contain Plotters integration, rendering, clipping,
contour path assembly, adaptive contour topology, regularization, or implicit
level projection. Those remain in `plotters-ternary` during extraction Phase 1.

## Grid-to-field workflow

Use RegularTernaryGrid when evaluating a scalar function at every regular
composition vertex. Its lazy composition iterator uses exactly the order
expected by RegularTernaryScalarField.

    use ternary_contours::{RegularTernaryGrid, RegularTernaryScalarField};

    let grid = RegularTernaryGrid::new(12)?;
    let values: Vec<f64> = grid
        .compositions()
        .map(|[a, b, c]| 2.0 * a - 3.0 * b + 5.0 * c)
        .collect();
    let field = RegularTernaryScalarField::new(grid.subdivisions(), values)?;

The workflow is:

    grid compositions
        -> user evaluates scalar values
        -> RegularTernaryScalarField
        -> contour extraction

RegularTernaryScalarField::from_fn is an equivalent convenience constructor
when scalar evaluation cannot fail.

## Alpha convention

For a directed interval, `t = 0` is its first endpoint and `t = 1` its second:

```text
y(t) = y0*(1-t) + y1*t + (1-t)*t*(alpha0 + alpha1*t)
```

Thus `alpha1` multiplies `t`. Reversal is
`(alpha0, alpha1) -> (alpha0 + alpha1, -alpha1)`.

## Interior policies

Every pair term keeps the raw `xi*xj` prefactor. `Muggianu` uses
`t = xj + xk/2`; `Kohler` uses `t = xj/(xi+xj)` and returns zero at the
opposite vertex without evaluating `0/0`. `RawBarycentric` is retained as an
experimental canonical-direction comparison policy.

## Features

`cubic-alpha` enables `spline1d`-derived interval construction. The crate is
currently `publish = false` while the extraction is validated.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.