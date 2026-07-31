# Draft 0.1.0 release notes

`ternary-contours` 0.1.0 is a backend-independent numerical core for regular ternary scalar fields. It provides canonical simplex-grid generation, linear isolines, optional cubic-alpha line contours, deterministic path construction, regularization, projection, diagnostics, and piecewise-linear filled-band geometry.

The crate intentionally has no Plotters or rendering dependency. `plotters-ternary` consumes its final ternary coordinates for chart projection, viewport clipping, colours, labels, and drawing.

The optional `cubic-alpha` feature enables cubic-alpha line contours. Filled bands are piecewise-linear only. Irregular triangulations, N-component grids, rendering, and cubic-alpha filled bands are not included.

This is a draft for the GitHub release body only. No release, tag, or registry publication is created by this document.
