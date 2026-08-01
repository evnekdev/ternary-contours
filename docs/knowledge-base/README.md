# Numerical knowledge base

This directory contains the editable Markdown source for the regular-grid
ternary contour design and the separate irregular-triangulation research note.

- [`regular/README.md`](regular/README.md) describes the implemented regular-grid
  scalar field, cubic-alpha interpolation, contour topology, regularization, and
  projection pipeline.
- [`irregular/README.md`](irregular/README.md) records the irregular roadmap. The
  Delaunay-backed linear fields are implemented behind `irregular-delaunay`;
  self-consistent irregular cubic-alpha point evaluation and irregular isolines
  are implemented behind `irregular-cubic-alpha`. Milestone 16 metrics distinguish
  Delaunay-only mesh quality from shared regular/irregular field analysis; irregular bands remain deferred.
- [`../numerical-validation.md`](../numerical-validation.md) records the
  maintained deterministic numerical audit, acceptance envelopes, and limits.

The repository-level ZIP bundles are retained as archival convenience copies.
They are excluded from Cargo packages; these Markdown files are the maintained,
reviewable source.
- [`../stable-phase-contours.md`](../stable-phase-contours.md) documents the
  implemented virtual regular umbrella, exact affine upper-envelope clipping,
  phase-labelled height and secondary contours, verification/refinement, and
  deferred stable-atlas and partial-domain work.
