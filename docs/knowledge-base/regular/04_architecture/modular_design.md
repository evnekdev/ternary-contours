# Modular architecture

## Implemented ownership boundary

The numerical/rendering extraction is complete and remains the governing
dependency direction:

```text
ternary-contours
    semantic ternary coordinates and equilateral logical geometry
    regular grids and irregular Delaunay meshes
    prepared interpolation and gradients
    ordinary contours and regular linear bands
    stable upper-envelope preparation and contour geometry
    numerical diagnostics and metrics

plotters-ternary
    chart projection and viewport clipping
    styling, labels, legends, and colour bars
    PNG/SVG/backend rendering
```

The numerical crate has no Plotters dependency. Rendering adapters consume
final semantic A/B/C coordinates and must not rebuild interpolation, alter
stable ownership, or change numerical topology.

## Current module flow

```text
regular/irregular topology and point location
        -> prepared scalar evaluators
        -> optional cubic-alpha field construction
        -> ordinary contour or metric consumers

heterogeneous prepared source evaluators
        -> geometry-grouped sampling-grid sampling
        -> optional verification and global refinement
        -> exact affine upper-envelope polygon clipping
        -> phase-labelled target segments
        -> canonical junction and path assembly
```

`src/simplex.rs` owns the canonical equilateral logical plane used by Delaunay
construction, lengths, metrics, and scale-aware geometry. Public coordinates
remain semantic `(a,b,c)`.

## Stable-phase module boundaries

`src/stable/` separates durable public inputs/results from private numerical
machinery:

```text
source/options/error/diagnostics
    public phase, source, mode, control, result, and failure concepts
sample/verify
    prepared evaluator grouping, dense sampling, and refinement checks
clip/partition
    exact affine pruning and cached stable polygons
segments/paths
    target intersection, forward progress, junctions, and assembly
```

The source interpolation family is independent from the sampling-grid model.
Muggianu, Kohler, and RawBarycentric remain policies inside cubic-alpha source
interpolation. Stable ownership is always the sampled height upper envelope;
secondary fields cannot influence it.

## Concrete 2-D APIs before generic abstraction

Regular grids retain integer-lattice direct location and deterministic triangle
ordering. Irregular meshes retain Delaunay-backed robust location and dense
crate-owned IDs. The stable sampling-grid intentionally does not rewrite either
around a superficial shared public enum or expose backend handles.

A future generic simplex-field extraction may share local value/gradient and
contour helpers, but Rust const-generic layout and ABI decisions should not be
frozen prematurely. Current optimized 2-D types remain appropriate.

## Future crate and ABI direction

Possible future separation remains:

```text
simplex-field-core
    local simplex fields and generic topology helpers
ternary-contours
    semantic ternary specialization and stable ensembles
plotters-ternary
    rendering adapter
simplex-contours-nd
    future N-component/Kuhn-simplex work
```

A future C ABI should flatten levels, paths, phase IDs, junctions, offsets, and
A/B/C point arrays. It must not expose Rust `Vec`, borrowed evaluator lifetimes,
backend Delaunay handles, or Rust enum layout directly. Partial-domain and
stable-atlas designs should be settled before an ABI promises their result
shape.
