# Grid inspection and classified point editing

The feature-gated native viewer includes a **Grid inspection** tab for editing
one `(grid, phase, property)` field at a time. It uses the same editable draft,
TCT serializer, dataset validation, calculation worker, and renderer as the
Data and Plot tabs. Top-level navigation is consistently **Data**,
**Diagnostics**, **Grid inspection**, then **Plot**; editing a field remains in
the draft while switching modes.

## Open an existing document

Start an untitled document with either command:

```bash
cargo run -p ternary-contours-cli --features viewer --
cargo run -p ternary-contours-cli --features viewer -- view
```

Use **Open** in the persistent toolbar to choose a `Ternary Contour Table
(*.tct)`. The dialog first uses the directory selected by Open or Save As in
this viewer session, then the parent of the current document, then the current
working directory.

Open is transactional: it parses and structurally validates a temporary TCT
dataset before replacing the editor, selectors, projection cache, texture, or
selection. A malformed file therefore leaves the current document untouched.
When the current document has unapplied or unsaved edits, choose **Save**,
**Discard and open**, or **Cancel**. Save applies the valid draft before writing
so grid-inspection changes are not lost.

## Point states and markers

| State | TCT / TSV token | Marker | Calculation meaning |
| --- | --- | --- | --- |
| Calculated | finite number | green filled circle | defined scalar sample |
| Non-existing | `NE` | grey cross | phase is intentionally undefined |
| Cut-off | `CO`, `CO:<limit>` | orange triangle | explicitly limited high-temperature result |
| Missing | `NA` | hollow circle | no result or classification yet |

The optional suffix in `CO:3000` is retained as a cut-off limit/note. Notes
may likewise be retained with `NE:<note>` or `NA:<note>`. None of the three
undefined states is converted to zero, NaN, negative infinity, or a calculated
sample. The numerical adapter reports reason-specific undefined evaluations,
although tracing can treat all such vertices as outside the available source
domain.

## Inspect and edit a field

1. Select **Grid inspection**.
2. Choose a grid, phase, and property. A newly opened document prefers the
   first field containing `T`.
3. Click a marker. The editor shows its canonical row, A/B/C composition,
   selected phase/property, state, scalar value, and note.
4. For **Calculated**, enter a finite scalar value. For the other states, the
   scalar field is disabled and the value is cleared.
5. Press **Apply point** (or Enter) to modify the draft. Use Previous/Next
   field controls or the selectors without losing those changes.
6. Shift-click multiple markers to batch set Non-existing, Cut-off, Missing,
   or clear notes.
7. Use **Apply edits** to update the active document or **Apply and
   recalculate** to send the existing worker a new calculation request.

State filters and counts are visible in the control panel. Labels support
value, state, row index, or a value/state combination; labels default to only
selected points to keep large regular grids readable. Regular-grid edges are an
optional inspection overlay. Irregular grids display their source points; no
invented topology is shown when an edge mesh is unavailable.


## Interpolation queries

Choose **Interpolation** at the top of Grid inspection to evaluate the selected
`(grid, phase ID, property)` field at an arbitrary point inside a source
triangle. **Vertex selection** keeps the existing classified-point editing
workflow; it never changes a point merely by hovering. The inspection canvas
uses the same configured background as Plot.

Clicking inside the simplex keeps the semantic `A`, `B`, `C` composition (the
component names appear in the results header), locates the source triangle with
the prepared numerical evaluator, and appends a row in the independently
scrollable **Interpolated Results** pane. It does not snap to a vertex. Each
row records the field identity, interpolation family and partial-domain policy,
state, value, and property unit. Undefined triangles and classified boundaries
remain typed `Missing`, `Non-existing`, `Cut-off`, or unavailable results; no
result is represented as `NaN`.

The source interpolation, cubic slope, continuation, and partial-domain
fallback controls share the Plot calculation configuration. Changing a setting
rebuilds only the selected cached source evaluator and recalculates registered
queries; it does not start a full liquidus calculation. The optional local
lambda columns are explicitly triangle-local (`lambda0`, `lambda1`,
`lambda2`). For cubic alpha, **Linear part** plus **Excess part** equals the
reported value; linear interpolation reports zero excess.

Enable **Triangle index** and **Source rows** to inspect topology. Internally,
`triangle_vertex_indices` are zero-based canonical indices into the selected
grid composition/value arrays and are ordered exactly with the local lambdas:
`lambda0` belongs to vertex 0, `lambda1` to vertex 1, and `lambda2` to vertex
2. The UI and copied TSV intentionally display one-based source rows as
`vertex_0_row`, `vertex_1_row`, and `vertex_2_row`, matching Excel row usage.
Regular grids therefore use canonical regular-grid rows, while irregular grids
preserve the stable loaded source-point rows.

Use **Clear**, **Delete selected**, **Copy selected**, or **Copy all** to manage
queries. Basic copies begin with `A`, `B`, `C`, and `InterpolatedValue`; full
copies include the selected optional columns and a `State` column. Results stay
visible while changing zoom or returning to Vertex selection.
## Excel TSV

**Copy selected field TSV** emits `A`, `B`, `C`, and the selected qualified
field column with the same numeric/`NE`/`CO`/`NA` cell tokens. Paste through the
Data tab follows the same rules:

```text
1250.0  -> Calculated
NE      -> Non-existing
CO      -> Cut-off
CO:3000 -> Cut-off with note
NA      -> Missing
```

Blank cells are errors unless the explicit **Treat blank cells as missing**
policy is selected. The paste preview remains separate from the active dataset
until it is applied.

## Save and reload checklist

1. Open a populated regular or irregular TCT document.
2. Confirm the Grid inspection selector chooses a usable `T` field.
3. Edit one calculated value, one `NE`, and one `CO:3000` point.
4. Switch phase/property and return; confirm the draft edits remain.
5. Save, reopen the document, and confirm state, value, note, and field
   ownership round-trip.
6. Apply and recalculate. If coverage is insufficient, read the reason-specific
   diagnostic; the last valid plot is retained on calculation failure.
