# Qt product and architecture contract

This is the normative contract for `apps/ternary-contours-qt`.  It records the
settled product decisions and architectural invariants for the Qt application.
A coding prompt does not override this contract unless it explicitly requests a
contract change and updates this contract, tests, affected documentation, and
the implementation.

The terms **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, and **MAY** are
normative.  Sections labelled *Deferred* are intentional product exclusions;
implementation details not stated as MUST/MUST NOT MAY change.

## Ownership and document model

Rust MUST own TCT parsing and serialization; the dataset; validation; draft-valid
versus calculation-ready state; dirty state; saved baseline and revisions;
undo/redo; phase/property/grid identities; classified values; bulk edits;
interpolation; field-independent ternary-coordinate normalization and deterministic triangle location; stable boundaries; invariants; univariants; isotherms; typed
projection records; calculation options; calculation generations; and
stale-result rejection.

Qt MUST own widget hierarchy, menus, native dialogs, clipboard interaction,
keyboard focus, dock/splitter layout, accessibility, canvas painting, window
geometry, and presentation-only preferences. Qt MUST NOT own a duplicate TCT
model, numerical validation, dirty-state logic, calculation options, or an
independently mutable canvas dataset.

There is exactly one active TCT document per process, with no document tabs.
New and Open MUST replace that document only after Save/Discard/Cancel handling.
There are exactly two primary tabs: **Data** and **Viewer**. The legacy egui
viewer MAY remain only until Qt feature parity is established.

## Static UI and Viewer layout

Qt Designer `.ui` XML is authoritative for the static widget hierarchy. Public
widgets MUST have stable `objectName` values and accessibility metadata. Runtime
code MUST NOT rebuild the normal hierarchy.

The Viewer MUST use a narrow left control area and a wide right display area.
The left area contains **Vertex inspection** above **Iso-plots**. The right area
contains the ternary canvas above the interpolation-results table. Controls MUST
NOT be placed in a horizontal toolbar above the Viewer; both the triangle and
results table belong in the wide right pane.

Viewer modes are exactly **Vertex** and **Interpolate**. In Vertex mode, a
single click inspects, Shift-click manages multi-selection, double-click opens
the vertex editor, and right-click opens selection actions. In Interpolate mode,
a double-click opens the coordinate-entry dialog rather than creating a query
directly. Rust owns global/local normalization, triangle location, boundary
tie-breaking, source-row identity, and conversion. The dialog maintains global
A/B/C and local triangle barycentric triplets independently while typing.
Enter synchronizes the authoritative triplet through Rust; focus loss only
validates and highlights. Edited coordinates use two-stage OK: the first OK
normalizes and updates a temporary canvas preview, the second creates exactly
one query. Cancel removes the preview and preserves prior selection. There is
no separate Inspect mode.

## Edit and synchronization lifecycle

The product MUST NOT expose Apply, Apply-and-recalculate, or ordinary Calculate
projection buttons. Valid edits commit through Enter or valid
`editingFinished`/focus loss. A commit MUST update authoritative Rust state,
refresh Data and Viewer, update dirty state, create undo where applicable, and
schedule affected calculation automatically. Invalid text MUST remain visible,
MUST NOT mutate Rust or start a calculation, and MUST show a useful error.

Data and Viewer are two views of one document. A Viewer vertex edit MUST appear
in Data immediately, mark the root/window title dirty, be undoable and savable,
and invalidate affected calculations. A Data edit MUST refresh Viewer vertices
and affected calculations.

A Vertex-mode double-click opens a compact transient editor near the pointer.
It shows row, composition, phase, property, state, value, and note; Enter commits,
Esc cancels, invalid input keeps it open, and a successful commit is one Rust
transaction.

## Validity, save, and document identity

Documents are **Invalid**, **Draft-valid**, or **Calculation-ready**. A
draft-valid document MAY have zero phases, zero grids, or incomplete calculation
inputs, and MUST be saveable. Calculation readiness MUST be separate from
saveability. Modal save errors are reserved for invalid data, serialization or
filesystem failure, and invalid active editors; zero phases or grids alone are
not save errors.

A save transaction MUST validate, serialize fully, write a temporary file, flush
and close it, atomically replace the destination, and only then update path,
source path, saved baseline, and dirty state. On cancellation or failure, path,
baseline, dirty state, root filename, and window title MUST remain unchanged.
Dirty means the authoritative document differs from the last successfully loaded
or saved document.

Document identity surfaces MUST derive from one authoritative summary and show
`Untitled`, `Untitled *`, `example.tct`, or `example.tct *`. The project-tree
root is the attached filename; the logical project title is a child.

## Numerical calculation contract

Automatic calculation order MUST be:

```text
prepare participating temperature fields
calculate stable binary and interior invariants
calculate stable univariants
derive automatic isotherm range
generate levels
calculate stable isotherms
generate typed projection records
refresh the canvas
```

Automatic defaults are Tmin = the lowest finite binary or interior invariant,
Tmax = the highest finite calculated participating-grid temperature, and Step =
100 C. If no finite invariant exists, Tmin falls back to the lowest finite grid
temperature. `NA`, `NE`, `CO`, notes, and non-finite values are excluded.

Editing Tmin, Tmax, or Step MUST switch to manual range and commit on Enter.
Manual values MUST remain committed if subsequent calculation fails. Reset to
automatic range restores automatic behavior. Range validation requires finite
Tmin/Tmax, Tmax >= Tmin, a finite positive step, and a safe level count.

The following MUST be passed to Rust for every calculation: automatic/manual
range, Tmin, Tmax, step, sampling subdivisions, source interpolation, cubic
method, partial-domain policy, continuation, regularization enabled, and
regularization spacing. Changes to them MUST schedule a projection calculation.

Last-change-wins scheduling is mandatory. Every request carries dataset revision,
numerical-options revision, generation, and a complete option snapshot. A setting
change during a worker run MUST mark recalculation pending; the stale completion
MUST be rejected and the newest request started immediately. A failed calculation
MUST retain the last valid plot.

Render-only settings (layer visibility, line/marker size, labels, diagnostics,
and Raw/Regularized/Overlay when geometry is cached) MUST redraw without
numerical recalculation. Projection geometry MUST preserve stable isotherms,
stable univariants, binary invariants, interior invariants, and raw/regularized
provenance through Rust projection, records, C ABI, Qt grouping, and canvas
objects.

For regular Viewer fields, the default source model is Cubic alpha / Akima /
Muggianu with one-sided-cubic-then-linear partial-domain fallback. This default
is defined once by Rust `InterpolationOptions`; Qt, the bridge, inspection,
projection, stable-boundary discovery, univariant tracing, regularization, and
numerical tracing consume one immutable Rust-owned snapshot and its option
revision. Inspection requests must use that current snapshot, never a
widget-reconstructed interpolation model. Irregular fields use effective Linear
interpolation; cubic-only controls are disabled and the visible selector is
returned to Linear rather than displaying an inapplicable Cubic alpha choice.
The selected raw or regularized projection is calculated first.
A failed optional sibling variant must be reported as unavailable without
discarding the selected projection or its canvas geometry.

## Actions, layers, sections, and clipboard

Menus, panels, canvas, and state MUST share one layer state. Every visible Viewer
control MUST dispatch a `QAction` or typed Viewer action; none may change only
local widget state. Stable isotherms, univariants, binary invariants, interior
invariants, sampling grid, source vertices, query points, axis labels, corner
names, legend, and diagnostic overlays have independent state. A master Plot
action MAY hide the group but MUST preserve sublayer preferences.

Collapsible sections use a keyboard-accessible header with `>` collapsed and
`v` expanded; collapsed content MUST consume no layout height. Vertex defaults:
Field and mode expanded; Vertex visibility, Labels and appearance, Selected
vertex, and Interpolation options collapsed. Iso-plots keeps status visible,
with Isotherm range expanded and Source calculation, Paths, Layers, and
Appearance and diagnostics collapsed. Splitters, collapsible state, results-table
visibility, and geometry MAY be persisted; Restore Layout restores defaults.

Excel paste is TSV and MUST be one Rust bulk transaction with no partial paste
and one undo entry. Regular-grid A/B/C stays read-only; irregular composition
editing is validated.

## Acceptance and enforcement

The CaO-PbO-ZnO regression case targets 3 binary invariants, 1 ternary/interior
invariant, and 3 univariants. These counts MUST be asserted for direct projection,
bridge summary, record grouping, Qt canvas grouping, and visible Viewer summary.
It is a numerical regression fixture, not merely a visual test.

Every visible control needs an inventory entry recording object name, signal,
action/handler, authoritative state, and whether it mutates a document,
calculates, redraws, or changes layout. Tests MUST reject unconnected controls,
unbound checkable actions, local-only state, unrelated duplicate action mapping,
and calculation options absent from the Rust ABI. Tests SHOULD also enforce the
static hierarchy, two tabs, mode list, stale-result handling, pending
recalculation, typed geometry, golden counts, and collapsible defaults.

## Deferred

This contract does not require document tabs, spreadsheet formulas, multi-document
workflows, an independently editable canvas dataset, or replacement of Qt native
painting with a different GUI toolkit.
## Developer numerical tracing

The numerical core owns deterministic typed trace events and their observation
levels. Qt only submits a Rust-owned per-calculation trace request from
**Settings → Developer diagnostics**; it does not rebuild numerical events,
log algorithm decisions in C++, or store a trace choice in TCT. Trace state is
not part of `ProjectionOptions`, option equality, cache keys, document dirty
state, or calculation-option revisions. A trace-output failure is surfaced
separately and never replaces the last valid projection.
