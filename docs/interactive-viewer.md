# Interactive liquidus inspection viewer

The optional native viewer is a manual numerical-inspection surface for a TCT
file. It reuses the exact command-line pipeline:

```text
TCT parser -> dataset validation -> LiquidusProjection -> Plotters / plotters-ternary
```

It does not implement another parser, field evaluator, or stable-topology
algorithm.

## Launch

The viewer is intentionally excluded from default CLI builds. Enable it when it
is needed:

```text
cargo run -p ternary-contours-cli --features viewer -- \
    view tools/ternary-contours-cli/fixtures/interior-invariant.tct

cargo run -p ternary-contours-cli --features viewer -- \
    view data.tct --levels 800:1400:50 --sampling-subdivisions 40
```

Without `--features viewer`, `view` exits with the enabling command while
`inspect`, `validate`, and `plot` remain headless.

## Controls and calculation policy

The persistent toolbar provides **Open**, **Save**, **Save As**, the current
file/unsaved marker, reload, recalculation, export, and fit/reset from every
tab. Open uses the native `.tct` picker and remembers the last directory used
by Open or Save As, then the current document directory, then the process
working directory. It never defaults to a repository fixture directory. If a
draft or file-backed document has changes, Open asks to **Save**, **Discard and
open**, or **Cancel**. The selected file is parsed and validated before it
replaces the current editor; a load failure leaves the document and plot intact.

The top-level tabs always appear and navigate in this order: **Data**,
**Diagnostics**, **Grid inspection**, then **Plot**. A new Untitled dataset opens
on Data. A successfully loaded populated document transitions to Plot after its
first calculation succeeds.

The Plot-side panel owns all current options:

- Levels accept `800,900,1000` or `800:1400:50`; invalid text is reported
  before calculation and never crashes the window. Levels commit on Enter or
  when the control loses focus.
- Sampling subdivisions and regularization spacing likewise commit only on a
  valid Enter/focus change; the UI retains invalid text and explains the error.
  Committed numerical changes are debounced for a short interval and coalesced
  into one worker request. **Recalculate now** remains available as a manual
  recovery action.
- Sampling resolution refines envelope/path extraction. With the default Linear
  source it does not make the source field higher order: that source remains
  piecewise planar between its tabulated vertices.
- **Source interpolation** selects Linear or, for complete regular `T` fields,
  Cubic alpha. Cubic alpha offers Akima, Makima, PCHIP, and Steffen edge-slope
  estimation, plus Raw barycentric, Muggianu, or Kohler continuation. It is
  disabled for irregular participating fields and reports classified undefined
  source cells rather than interpolating across `NA`, `NE`, or `CO` gaps.
- Layer visibility, labels, legend, line width, marker size, and raw versus
  regularized display redraw the shared bitmap without stable-boundary tracing.
- Raw, regularized, and overlay modes cache the raw projection and, when
  enabled, the regularized projection under the same parsed dataset.

Parsing, validation, and calculation run on an owned worker request. Results
carry a generation number; stale results are ignored. A failed reload leaves
the last valid dataset, projection, and texture intact and reports the parser
or numerical diagnostic in the status area.

## Grid inspection

The **Grid inspection** tab selects one grid, phase, and property at a time and
draws every source vertex on a ternary diagram. Marker shape and colour are
both meaningful: green filled circles are calculated finite values, grey crosses
are non-existing, orange triangles are cut-off, and hollow circles are missing.
Filters and compact labels avoid overloading large grids.

Click a marker to inspect its canonical row, composition, current state, value,
and optional note. Shift-click selects several points for batch non-existing,
cut-off, missing, or note-clearing actions. Edits remain in the dataset draft
while moving between grid fields; **Apply edits** and **Apply and recalculate**
make their transition explicit. Save serializes the current draft and the first
open after an unsaved edit will prompt before discarding it. A loaded file
initializes the first grid field, preferring its `T` field.

`NE`, `CO`, `CO:<limit>`, and `NA` round-trip through TCT and TSV. Classified
undefined values do not participate in interpolation; numerical diagnostics
preserve whether the source was non-existing, cut off, or missing.

## Inspection and diagnostics

Scroll over the plot to zoom; drag to pan; fit/reset only crop and scale the
rendered bitmap. Ternary compositions remain unchanged. The viewer centralizes
composition to canonical equilateral logical coordinates, bitmap coordinates,
and screen coordinates for hit testing.

Click priority is invariant node, univariant, isotherm, then source sample. The
selection panel reports available numerical data: phase names and IDs,
composition, temperature, incident paths, point counts, endpoints, path length,
pair residuals, and regularization diagnostics. Stable-contour field residuals
are not currently retained by the numerical result API and are labelled as
such.

The collapsible Diagnostics panel enables path vertices, contour/univariant
endpoints, invariant and univariant IDs, and phase-pair labels. Raw vertices
are yellow circles and regularized vertices are cyan squares, and their overlay
uses the same Plotters-derived component-to-bitmap transform as the drawn path
and the hit geometry. Sampling/source points are regular render layers.
Sampling-grid edge diagnostics are not shown for irregular source grids because
no common edge topology exists there. Diagnostics also shows the active source
interpolation, sampling-grid vertex count, classified `T`-field coverage, and
the semantic `A`/`B`/`C` corner mapping.

Normal plots and SVG/PNG exports label pure corners with component names only;
they never append barycentric vectors such as `[1, 0, 0]`. The semantic mapping
is deliberately kept in Diagnostics instead of the export.

## Export and limits

Export uses the same static Plotters configuration as the view and writes
`<input>.viewer.svg` or `<input>.viewer.png` beside the source TCT file.

Zoom/pan is intentionally a viewer-only bitmap transform rather than an
arbitrary Plotters viewport. It preserves the renderer and numerical pipeline,
but vector export always uses the full configured static plot. Diagnostic text
is a viewer overlay; core layers, visibility, styling, and raw/regularized paths
are shared by image and export.

The viewer does not edit TCT tables, read `.xlsx`, invoke external
thermodynamic software, discover isolated closed univariants, trace nonlinear
paths directly, draw stable filled regions or 3D surfaces, or provide web/FFI
integration.

## Manual smoke-test checklist

Use the committed fixtures below on the intended desktop platform:

1. Load `minimal-regular.tct`; confirm initial calculation and layer toggles.
2. Use `different-subdivisions.tct`; change sampling subdivisions, commit the
   field, and confirm one debounced recalculation starts.
3. Use `interior-invariant.tct`; select an interior invariant and a univariant.
4. Use `binary-invariants.tct`; select a binary invariant.
5. Use `hidden-metastable-equality.tct`; check raw, regularized, and overlay
   modes after enabling regularization.
6. Use `partial-phase-domain.tct` and `irregular-phase-grids.tct`; enable source
   and sampling-point diagnostics.
7. Edit a valid input, reload it, then make it malformed and reload again;
   confirm the last valid plot remains visible.
8. Export both SVG and PNG and inspect the files.

Manual launch smoke result (Windows desktop, 2026-08-03): passed. The
interior-invariant fixture opened through the `eframe` glow backend and ran
without a startup error; the test process was then closed after launch. Full
visual interaction remains a desktop checklist because the automated suite
intentionally requires no display server.
## Data entry

The document-first navigation order is **Data**, **Diagnostics**, **Grid
inspection**, then **Plot**. The Data tab keeps active data, an editable draft,
and a paste preview separate from the last valid projection. Regular grids copy exact canonical compositions and
accept values-only, guidance, or authoritative composition TSV. Irregular grids
accept mapped composition/property columns with explicit normalization policy.
Preview diagnostics are copyable and include source row/column errors; apply,
apply-and-recalculate, revert, undo/redo, save, and Save As are explicit.
Calculation still uses the existing worker and retains the last valid plot on
failure. See [`grid-data-entry.md`](grid-data-entry.md).
## Starting without a file

With no subcommand, or with view and no input path, the feature-enabled
viewer opens on the Data tab with an Untitled ternary system regular grid:
10 subdivisions, 66 canonical rows, and undefined Phase1.T, Phase2.T, and
Phase3.T values. Canonical compositions can be copied immediately. The
first Save action is Save As.

cargo run -p ternary-contours-cli --features viewer --
cargo run -p ternary-contours-cli --features viewer -- view