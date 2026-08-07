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

- Levels accept `800,900,1000`, `800 1400 50`, or
  `800 1400 50; 815.96,900`; invalid text is reported before calculation and
  never crashes the window. Levels commit on Enter or when the control loses
  focus.
- Sampling subdivisions and regularization spacing likewise commit only on a
  valid Enter/focus change; the UI retains invalid text and explains the error.
  Committed numerical changes are debounced for a short interval and coalesced
  into one worker request. **Recalculate now** remains available as a manual
  recovery action.
- Sampling resolution refines envelope/path extraction. The canonical regular
  source default is Cubic alpha / Akima / Muggianu with one-sided-then-linear
  partial-domain fallback; selecting Linear retains a piecewise-planar source.
  Rust owns this interpolation snapshot and its revision for queries, projection,
  topology, regularization, and numerical tracing.
- **Source interpolation** selects Linear or Cubic alpha for regular `T` fields.
  Cubic alpha offers Akima, Makima, PCHIP, and Steffen edge-slope estimation,
  plus Raw barycentric, Muggianu, or Kohler continuation. The **Partial-domain
  cubic fallback** control defaults to *One-sided cubic, then linear*: missing,
  non-existing, and cut-off vertices remain local domain boundaries, one-sided
  finite stencils are used where possible, and only triangles containing an
  unavailable corner become undefined. Strict cubic is available for audits.
  Cubic remains disabled for irregular participating fields.
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


### Interpolation inspection

Grid inspection has **Vertex selection** and **Interpolation** modes. The
latter evaluates the selected `(grid, phase ID, property)` at arbitrary clicks
inside source triangles using a cached prepared numerical field, not a
GUI-specific formula. The right-side results pane records semantic A/B/C,
method, typed state, value, unit, optional local barycentric coordinates,
linear/excess contributions, triangle ID, and one-based source rows. The source
rows are displayed as `vertex_0_row`, `vertex_1_row`, and `vertex_2_row`; their
internal Rust indices are zero-based and remain aligned with local lambda0/1/2.
Changing interpolation settings, field selectors, or committed source values
recalculates every registered query without re-running the full liquidus
worker. Query IDs, A/B/C coordinates, row order, and canvas markers stay fixed;
only located triangles, local lambdas, values, and provenance are replaced.
Grid inspection uses the
same configured plot background as Plot.
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

**Export PNG** and **Export SVG** use the same accepted projection snapshot as
the Viewer. **Export CSV...** opens a modal export workflow instead of writing
immediately. It starts with the last successful CSV location for the current
session, otherwise `<document-stem>.csv` beside the TCT document, then a
sensible session default. The editable path can be changed through **Browse...**
without exporting. Select one or more independent sections: **Invariants**,
**Univariants**, and **Isotherms**; then confirm with **OK**. A missing extension
gets `.csv`; an explicitly supplied non-CSV extension is left untouched.
Existing files require a Replace/Cancel confirmation. Output is written through
a temporary sibling and atomically renamed where supported, so a failed write
leaves an existing destination intact.

The dialog pins the accepted projection revision visible when it opened. A
pending or failed recalculation therefore cannot change the exported geometry:
it exports the retained visible result. If a newer calculation is accepted
before confirmation, the dialog asks to be reopened rather than mixing
snapshots. Raw mode exports raw paths; regularized mode exports the selected
regularized geometry, including per-path raw fallbacks; overlay exports that
same regularized primary geometry once.

Projection CSV is UTF-8, RFC-compatible, CRLF-delimited, and intentionally a
data format rather than a display format. Its exact seven-column header is the
three declared component names, `T, <unit>`, `phase1`, `phase2`, and `phase3`.
Numbers use locale-independent shortest round-trip-safe `f64` text, not the
Viewer's fixed decimal labels. Invariants appear first (one row each), then
complete univariant paths, then phase-owned isotherm segments. Each selected
section and every separate path are divided by a physically blank line; no
internal IDs, rendering styles, point indices, or diagnostic columns are
exported.

The **Levels** edit is always editable concrete text. It accepts `minimum
maximum step`, a comma-separated manual list, or both forms separated by `;`.
When its origin is automatic, the accepted stable topology supplies actual
text: the minimum is `ceil(min invariant temperature / 100) * 100`, the maximum
is `floor(max invariant temperature / 100) * 100`, and the step is `100`.
**Reset to automatic range** returns to this `AutoDerived` behaviour. A valid
manual commit becomes `UserEdited` and is not overwritten by a later topology
calculation. If invariants cannot produce a finite whole-hundred range, the
last valid manual specification remains visible when one exists; otherwise the
field is visibly invalid or empty with an explanation. It never says
`automatic` and never falls back to sampled extrema. Level-only edits retain
stable topology and rebuild only level-dependent contours.

The level control has three lifecycle states: AwaitingTopology (a valid
first-run state while invariant temperatures are being discovered),
AutoDerived (concrete text derived from the accepted invariant range), and
UserEdited (the committed user specification). The placeholder text is never
parsed. The Viewer first accepts stable topology without levels, derives the
whole-hundred range, then runs the contour stage against that same topology.
Draft and requested text are kept separate from the accepted projection. If a
later request fails, the accepted plot, invariant/univariant data, concrete
level text, and generated-level preview remain visible; an invalid draft is
shown separately. No generated levels is reserved for a genuinely valid
zero-level result, not bootstrap or failure.

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

## Result tables

The lower Viewer pane contains two independently resizable, read-only tables. **Interpolation results** is session-only Viewer state: Copy exports selected rows as C-locale TSV with headers, Remove selected (including `Delete` while that table has focus) removes only the stable query IDs selected, and Clear all removes every query after confirmation when more than one exists. These commands remove canvas markers but never dirty a TCT document, change a dataset revision, create document undo, or schedule projection work.

**Invariant points** is calculated state, not a second numerical calculation. Its rows come directly from the accepted Rust stable-boundary graph, in stable invariant-node ID order, and include binary/interior type, composition, temperature, phase IDs and names, binary boundary metadata, and incident complete-univariant degree. During recalculation or failure the table keeps its last accepted rows and displays a stale status; a newly opened or created document clears it until a projection is accepted. Trace-only configuration is observational and does not alter invariant rows.

Both result tables sort all columns through typed sort values rather than formatted display text. Numeric values sort numerically, text is case-insensitive and locale-aware, and unavailable values remain last in either direction. Table sorting, selection, canvas query highlighting, deletion, and TSV export are keyed by stable query/invariant identity, so sorting cannot change an operation's target. The active sort column and direction persist with Viewer layout settings; Restore Layout resets each table to ascending ID order.


## Presentation contract

Viewer presentation uses fixed-point display formatting only: temperatures use two decimal places, compositions use five, and other properties use three. This is an interface convention, not a numerical precision limit; stored values, calculations, TCT serialization, cache keys, and traces retain full precision. Projection CSV is explicitly a data-serialization exception: it uses shortest round-trip-safe numbers rather than GUI display precision.

The compact isotherm editor accepts `minimum maximum step`, a comma-separated explicit list, or both separated by `;` (for example, `700 1200 50; 815.96, 925`). Duplicate levels are canonicalized using the numerical level tolerance. Invalid text remains visible and does not replace the last accepted projection.

Iso-line labels are optional and use the selected property metadata. Invariants share one red-diamond/black-outline style and show bold two-decimal temperature labels. Univariants are black, slightly thicker than iso-lines, and are labelled only by their stable phase pair. The legend is generated from visible, renderable semantic categories and omits hidden or empty categories.
