# Grid data entry

Milestone 22 adds an Excel-friendly tabular authoring workflow to
ternary-contours-cli. It keeps numerical ownership unchanged:

~~~text
TSV preview -> TabulatedTernaryDataset -> existing TCT validation
            -> LiquidusProjection -> shared renderer/viewer
~~~

The editor has no .xlsx, COM, formula, or thermodynamic-calculation support.
It accepts literal tab-separated ranges from Excel or another spreadsheet.

## Classified scalar tokens

Paste and copy keep point classification rather than treating undefined cells as
numbers. Numeric cells become **Calculated**; legacy `NE` is accepted as **Missing**; `CO` or `CO:<limit>` becomes **Cut-off**; and `NA` (or a blank only when the explicit blank-as-missing option is enabled) becomes **Missing**. `EX[layer,method,support,spread]=value` is a persistent extrapolated estimate. Only calculated and EX finite values participate in liquidus interpolation.

Use the **Grid inspection** tab for point-by-point marker editing, batch state
changes, state counts, and selected-field TSV copy. See
[`grid-inspection.md`](grid-inspection.md) for the full workflow.


## Generate canonical regular compositions

Use the headless command whenever a viewer is not available:

~~~bash
cargo run -p ternary-contours-cli -- \
    compositions --subdivisions 20 --components A,B,C --header
~~~

The emitted order is exactly the numerical RegularTernaryGrid canonical order,
so it is authoritative for regular scalar data. --precision controls fixed
decimal places and --output writes TSV instead of stdout.

Generate a full regular TCT scaffold with:

~~~bash
cargo run -p ternary-contours-cli -- \
    template regular \
    --subdivisions 20 \
    --components A,B,C \
    --fields alpha.T,beta.T \
    --output regular-template.tct
~~~

The template intentionally contains NA scalar cells. Since T is required,
validating it correctly reports missing temperature values until every phase
field is filled. The declaration and canonical row structure are ready to paste.

For an irregular skeleton:

~~~bash
cargo run -p ternary-contours-cli -- \
    template irregular --components A,B,C --fields alpha.T
~~~

Use --style grid-section or --style tsv-header to emit only that content. No
irregular coordinates are invented.

## Regular grid in the viewer

Launch the optional native UI:

~~~bash
cargo run -p ternary-contours-cli --features viewer -- view data.tct
~~~

Open Data, select a regular grid, and use Copy compositions or Copy
compositions with header. Paste those into Excel, calculate scalar columns,
then copy either a values-only column or a rectangular range back.

Pasting never changes the active dataset. The preview reports row count, scalar
mappings, missing values, guidance residual, canonical reorder count, and
row/column errors. Select one of:

- Values only: data follows canonical row identity and must have exactly
  (n + 1)(n + 2)/2 rows.
- Coordinates guidance: coordinates are compared to canonical positions;
  values remain at their canonical row.
- Coordinates authoritative: shuffled rows map values into canonical order.
  Duplicate, off-lattice, missing, or extra points are rejected.

Headers matching A, B, C, and phase.property map automatically. Without a
header the selected destination field receives the first scalar column. Missing
tokens are recognised. Blank cells are errors unless Treat blank cells as
missing is selected.

Choose Add new field, Replace existing field, or Replace entire grid explicitly.
Apply valid data, then Apply and recalculate to send the validated neutral
dataset to the existing calculation worker. A failed calculation retains the
previous plot.

## Irregular grid in the viewer

Paste a rectangular range such as:

~~~text
A    B    C    alpha.T    alpha.activity
0.10 0.20 0.70 1120.5     0.42
0.22 0.31 0.47 1154.2     0.51
~~~

The Data tab supports header detection, component columns, scalar-field mapping,
multiple qualified scalar columns, missing tokens, and three normalization
policies: reject non-normalized, normalize within tolerance, and normalize all
positive rows. Every normalization is reported.

The preview rejects malformed widths, invalid or non-finite numbers, negative
values, grossly non-normalized points, duplicate/near-duplicate points, fewer
than three distinct points, and collinear sets. Errors retain pasted row and
column numbers.

## Save, undo, and diagnostics

The Data tab maintains separate active and draft datasets. Revert unapplied
edits restores the draft. Applying a draft makes a bounded dataset-level undo
snapshot; Undo and Redo cover accepted field/grid/declaration operations rather
than individual spreadsheet cells.

Save and Save As serialize deterministic TCT 1.0 with stable sections, field
ordering, canonical regular rows, configured missing token, and fixed numeric
formatting. Save writes a temporary sibling file before replacement and asks
for confirmation.

Copy actions use the native GUI clipboard for compositions, tables, diagnostics,
and selection text. Headless tests validate pure TSV generation/parsing.

## Manual smoke checklist

1. Generate subdivisions 4 compositions and paste regular-values-only.tsv.
2. Preview composition-bearing data and regular-authoritative-shuffled.tsv.
3. Confirm malformed data does not alter the active dataset.
4. Paste irregular-multiple-properties.tsv, apply, and recalculate.
5. Use undo/redo after a field replacement.
6. Save, reopen, and confirm the plot remains valid.
7. Attempt a malformed reload after a valid calculation; the previous plot
   must remain visible.

Manual launch smoke result (Windows desktop, 2026-08-04): the feature-enabled
viewer opened with the interior-invariant fixture and remained alive until the
smoke process was closed. Automated tests cover canonical generation, regular
paste mapping/reordering, irregular validation/normalization, dataset undo/redo,
serialization, and the headless commands. Clipboard and pointer-driven Data-tab
interactions remain in the desktop checklist above because the test suite does
not require a display server or native clipboard.