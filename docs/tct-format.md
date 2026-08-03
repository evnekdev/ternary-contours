# TCT format 1.0

TCT means **Ternary Contour Table**. A TCT file is UTF-8, line-oriented, and
versioned. It has editable declarations around one or more literal TSV tables.
Blank lines and whole-line comments beginning with `#` are ignored. Inline
comments are deliberately not recognised in data rows.

## Header and declarations

The first non-comment content is exactly:

```text
TCT 1.0
```

The supported global declarations are `title = value`,
`composition_units = value`, `default_missing = token`, and
`missing_tokens = token ...`. `default_missing = NA` is conventional. A blank
TSV cell is an error; only declared missing tokens represent undefined optional
values.

The following sections occur once each:

```text
[components]
A
B
C
[/components]

[phases]
alpha = 10
"liquid + alpha" = 20
[/phases]

[properties]
T required K
activity optional 1
[/properties]
```

Exactly three unique, non-empty components are required, in semantic A/B/C
order. Phase names are unique and may be quoted; IDs are unique non-negative
integers and do not depend on declaration order. Property names are unique.
`T required <unit>` is mandatory; units are descriptive strings and are not
converted automatically.

## Grid grammar

A grid has this shape:

```text
[grid grid_name]
phase = alpha
type = regular
subdivisions = 20
order = canonical
composition_columns = guidance
properties = T activity
columns:
A	B	C	T	activity
data:
... literal TSV rows ...
[/grid]
```

The `columns:` header and every `data:` row use literal tab separators. For a
single-phase grid, unqualified `T` and `activity` belong to `phase = alpha`.
For a shared grid, qualified names identify ownership:

```text
columns:
A	B	C	alpha.T	beta.T	gamma.T	alpha.activity
```

All phases and properties in qualified names must be declared globally. A
single-phase declaration cannot conflict with qualified columns for another
phase. Version 1 permits one field definition for each `(phase, property)`
pair and rejects overlapping alternatives explicitly.

## Regular grids

Regular topology comes from `subdivisions = N`, not inferred from table length.
The expected row count is `(N + 1)(N + 2)/2`; canonical order increments `i`,
then `j`, with `k = N - i - j`.

`composition_columns = none` supplies no A/B/C columns and requires canonical
row order. `guidance` supplies A/B/C for reviewer convenience, but canonical
position remains authoritative; supplied values are compared at every row and
the diagnostic contains its line, expected composition, supplied composition,
residual, and tolerance. `authoritative` maps every supplied composition to its
integer lattice vertex, accepts shuffled rows, rejects off-lattice and duplicate
points, reports missing vertices, and reorders field values canonically.

`order = canonical` documents canonical input. `order = compositions` is useful
with authoritative shuffled compositions. It does not weaken the declared
composition-column mode.

## Irregular grids

An irregular grid uses `type = irregular` and must set:

```text
composition_columns = authoritative
```

A/B/C columns are mandatory. Rows may be in arbitrary order. The parser checks
finite, non-negative normalized compositions, duplicate samples, at least three
distinct points, and non-collinearity. Individual phase grids are allowed to
cover only part of the simplex. During calculation these remain explicit
`StablePhaseEvaluation::Undefined` regions; no extrapolation or invented zeros
are used.

## Missing data and diagnostics

A declared missing token represents an undefined optional value. Required fields
(including T) may not be missing. Other non-numeric tokens, non-finite values,
and blank cells are errors. Diagnostics carry source path, line, section/grid,
and column information where applicable, and include expected syntax for format
and table-shape failures. The default CLI output is concise; use `--verbose` to
request an error cause chain.

## Calculation and output

The CLI parses to a neutral `TabulatedTernaryDataset`, validates topology and
field ownership, creates explicit partial-domain evaluators, and passes those
to `ternary-contours` for stable liquidus sampling, contouring, invariant
identification, and boundary-connected univariant tracing. Rendering is a
separate `plotters-ternary` step. It uses deterministic phase colours and does
not smooth paths.

The optional `ternary-contours-cli view` command consumes this same parsed and
validated dataset. When enabled with the `viewer` Cargo feature it calculates a
`LiquidusProjection` on a worker thread and renders the existing Plotters scene
to an in-memory RGBA bitmap. It never uses a GUI-specific TCT reader or a
second numerical path. See [`interactive-viewer.md`](interactive-viewer.md) for
launch commands, reload semantics, and inspection controls.

The committed fixtures under `tools/ternary-contours-cli/fixtures` demonstrate
regular and irregular inputs, optional properties, partial domains, shuffled
authoritative rows, metastable equality suppression, interior/binary invariants,
and key malformed cases.
