# ternary-contours

`ternary-contours` is a backend-independent numerical core for scalar data on a
regular two-dimensional ternary composition grid. It owns the complete line-
contour pipeline:

```text
regular scalar grid
  -> linear or cubic-alpha interpolation
  -> local iso-level intersections
  -> deterministic global path assembly
  -> optional arc-length redistribution
  -> implicit-level projection
  -> semantic ternary contour paths
```

It intentionally has no dependency on Plotters, rendering backends, screen
coordinates, image dimensions, visual viewports, or legends. A rendering crate
such as `plotters-ternary` consumes the final semantic `(a, b, c)` paths and may
clip them only for display.

Changing a renderer, output dimensions, viewport, line style, or supersampling
cannot change the numerical contours returned by this crate.

## Regular grid

For subdivision count `n`, samples correspond to integer triples `i + j + k = n`
and compositions `[i/n, j/n, k/n]`. Values use row-major `(i, j)` ordering: `i`
increases from zero through `n`; within each row, `j` increases from zero
through `n-i`, with `k = n-i-j`.

## Alpha convention

For a directed interval, `t = 0` is the first endpoint and `t = 1` the second:

```text
y(t) = y0*(1-t) + y1*t + (1-t)*t*(alpha0 + alpha1*t)
```

Thus `alpha1` multiplies `t`. Reversal is
`(alpha0, alpha1) -> (alpha0 + alpha1, -alpha1)`.

## Interior policies

Every binary-pair term retains the raw `xi*xj` prefactor. `Muggianu` uses
`t = xj + xk/2`; `Kohler` uses `t = xj/(xi+xj)` and returns zero at the
opposite vertex without evaluating `0/0`. `RawBarycentric` is an experimental,
canonical-direction comparison policy rather than conventional Muggianu.

## Features and exclusions

`cubic-alpha` enables `spline1d`-derived shared-edge interval construction.
Piecewise-linear contours are always available. The crate remains
`publish = false` while extraction compatibility is reviewed.

Irregular triangulations, the proposed iterative irregular-edge alpha method,
Kuhn simplices, arbitrary-dimensional grids, filled contours, isosurfaces,
manifold extraction, viewport clipping, and rendering are intentionally out of
scope.

Editable numerical design notes live under [`docs/knowledge-base`](docs/knowledge-base/README.md).
The committed ZIP files are archival convenience copies and are excluded from
Cargo packages.

## License

Licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the
[MIT license](LICENSE-MIT), at your option.
