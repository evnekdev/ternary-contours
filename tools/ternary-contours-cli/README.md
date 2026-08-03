# ternary-contours-cli

`ternary-contours-cli` is the repository companion application for manually
checking stable liquidus isotherms, stable univariants, binary and interior
invariants, and raw or regularized boundary paths from tabulated ternary data.
It is a repository tool and is not published to crates.io.

The input is a UTF-8 `.tct` (Ternary Contour Table) file. It combines small,
sectioned metadata with literal TSV tables, so a rectangular range can be
pasted directly from Excel without an `.xlsx` dependency.

## Run it

From this repository:

```text
cargo run -p ternary-contours-cli -- inspect tools/ternary-contours-cli/fixtures/minimal-regular.tct
cargo run -p ternary-contours-cli -- validate tools/ternary-contours-cli/fixtures/minimal-regular.tct
cargo run -p ternary-contours-cli -- plot tools/ternary-contours-cli/fixtures/minimal-regular.tct --output target/liquidus.svg
cargo run -p ternary-contours-cli -- plot tools/ternary-contours-cli/fixtures/minimal-regular.tct --output target/liquidus.png
```

`inspect` only parses and reports the structure. `validate` also constructs a
stable projection and its boundary graph; it exits non-zero for invalid input
or unmet calculation preconditions. `plot` runs that same calculation once and
writes SVG or PNG based on the output extension. SVG is the preferred static
format.

## Plot controls

```text
--levels 800,900,1000
--levels 800:1400:50
--sampling-subdivisions 32
--regularize | --no-regularize
--show-isotherms --show-univariants --show-invariants
--show-binary-invariants --show-grid --show-samples
--width 1200 --height 950 --title "My liquidus"
--format svg|png
```

With no `--show-*` layer selection, plots show isotherms, univariants, and both
classes of invariant by default. `--regularize` redraws the same graph with
path regularization; it never changes the parser or numerical pipeline.

## Interactive viewer

The native inspection window is optional. A normal `inspect`, `validate`, or
`plot` build does not compile GUI dependencies. Enable it only when launching a
viewer:

```text
cargo run -p ternary-contours-cli --features viewer -- \
    view tools/ternary-contours-cli/fixtures/interior-invariant.tct

cargo run -p ternary-contours-cli --features viewer -- \
    view data.tct --levels 800:1400:50 --sampling-subdivisions 40
```

`view` accepts the same calculation and initial layer options as `plot`
(`--levels`, `--sampling-subdivisions`, `--regularize`, the `--show-*` layer
flags, `--width`, `--height`, and `--title`). Without the feature it exits with
a concise command showing how to enable it.

The left panel owns the calculation and render configuration. Enter levels as a
comma list or `start:stop:step`, then press **Apply / recalculate**. Layer,
label, legend, marker, and path-display changes only redraw the shared Plotters
bitmap; levels, sampling, regularization, and reload create a worker request.
The current file is reparsed and validated by the same TCT pipeline as the
headless commands. A malformed reload retains the last valid projection and
texture while showing the diagnostic in the status area.

Use the toolbar to reload, export SVG/PNG beside the input as
`<input>.viewer.svg` or `<input>.viewer.png`, and fit/reset the bitmap view.
Scroll to zoom and drag to pan. Those operations crop and scale the shared
bitmap; they do not modify ternary coordinates. Raw, regularized, and overlay
modes draw the paths from the cached numerical projections with distinct styles.
Click an invariant, univariant, isotherm, or source point for its numerical and
topological details. The Diagnostics section keeps vertices, endpoints, IDs,
and phase-pair labels opt-in.

See [`docs/interactive-viewer.md`](../../../docs/interactive-viewer.md) for a
manual smoke-test checklist and platform notes.
## Complete example

```text
TCT 1.0

title = Corner liquidus fields
composition_units = fraction
default_missing = NA

[components]
A
B
C
[/components]

[phases]
alpha = 10
beta = 20
gamma = 30
[/phases]

[properties]
T required K
activity optional 1
[/properties]

[grid shared_regular]
type = regular
subdivisions = 1
order = canonical
composition_columns = none
properties = T activity
columns:
alpha.T	beta.T	gamma.T	alpha.activity
data:
100	100	120	0.10
100	120	100	0.40
120	100	100	NA
[/grid]
```

Phase identity is the declared integer ID, not a table-row position. The `T`
property is always the height field used to determine stability. Optional
properties are carried in the parsed data but do not affect phase stability.

## Excel workflow

1. Prepare one rectangular numerical table in Excel.
2. Copy the selected range.
3. Paste it immediately below `data:` in a `.tct` section.
4. Keep the tab characters and rectangular cell count intact.
5. Save as UTF-8 plain text with the `.tct` extension.
6. Run `ternary-contours-cli validate file.tct`.
7. Run `ternary-contours-cli plot file.tct --output projection.svg`.

Do not use a CSV exporter: a `.tct` file has several metadata and grid
sections, while each `columns:`/`data:` block is specifically TSV.

## Current limitations

Version 1 rejects ambiguous overlapping `(phase, property)` field definitions
rather than silently selecting one. Missing values remain undefined phase
regions; they are never converted to zero or negative infinity. Individual
irregular phase domains may be partial, but a later calculation that lacks
coverage reports a conversion/calculation error. There is no `.xlsx` parsing,
unit conversion, field merging, or direct nonlinear tracing in this milestone.
See [`docs/tct-format.md`](../../../docs/tct-format.md) for the full grammar and
grid semantics.