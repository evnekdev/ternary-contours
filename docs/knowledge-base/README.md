# Numerical knowledge base

This directory contains the editable Markdown source for the regular-grid
ternary contour design and the separate irregular-triangulation research note.

- [`regular/README.md`](regular/README.md) describes the implemented regular-grid
  scalar field, cubic-alpha interpolation, contour topology, regularization, and
  projection pipeline.
- [`irregular/README.md`](irregular/README.md) records the irregular roadmap. The
  Delaunay-backed mesh and linear field foundation are implemented behind
  `irregular-delaunay`; the iterative irregular-edge alpha proposal is not.

The repository-level ZIP bundles are retained as archival convenience copies.
They are excluded from Cargo packages; these Markdown files are the maintained,
reviewable source.