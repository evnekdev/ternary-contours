# Public state contracts

Generated from PUBLIC_STATES.

| State | Owner | Allowed values | Default | Meaning | Rationale | Hazard |
| --- | --- | --- | --- | --- | --- | --- |
| ActiveTab | TabBar | Data, Diagnostics, GridInspection, Plot | Data | Visible top-level workflow. | Separates data editing, diagnostics, grid inspection, and plotting. | A stale tab can hide controls needed to recover an error. |
| DocumentFreshness | GlobalToolbar | Untitled, LoadedClean, Dirty, Loading, Saving, SaveFailed, OpenFailed | Untitled | Whether replacing or closing can discard changes. | Classified point and declaration edits must be preserved. | A false clean state can silently discard edits. |
| DialogState | GlobalToolbar | Closed, Open, Save, UnsavedChanges | Closed | One active native file decision. | Prevents concurrent conflicting native dialogs. | Two dialogs can target the wrong path. |
| ProjectionFreshness | PlotCanvas | None, Stale, Calculating, Current, FailedWithPrevious, FailedWithoutProjection | None | Whether the plot matches numerical input revisions. | Exports and selection require honest numerical provenance. | Old geometry can be shown as current. |
| QueryFreshness | InterpolationResultsTable | Empty, Stale, Recalculating, Current, PartiallyFailed | Empty | Whether registered queries match field/settings. | The results pane supports Excel-copyable numerical inspection. | Copied values can disagree with the selected field. |
| DatasetRevision | DataPanel | monotonic revision | 0 | Dataset input provenance. | Async calculations must identify the dataset they used. | A stale worker can replace a newer document. |
| EditorRevision | DataPanel | monotonic revision | 0 | Draft-editor provenance. | Draft and active data are intentionally separated. | Dirty markers can fail to represent pending edits. |
| InterpolationSettingsRevision | PlotInterpolation | monotonic revision | 0 | Source interpolation provenance. | Plot and inspection share one interpolation configuration. | Query rows can use another interpolation method. |
| CalculationSettingsRevision | PlotSettings | monotonic revision | 0 | Projection calculation provenance. | Contour levels and sampling affect derived topology. | A stale calculation can be accepted. |
| ProjectionRevision | PlotCanvas | monotonic revision | 0 | Current accepted projection identity. | Render and hit geometry depend on it. | Selection can address a different projection. |
| RenderSettingsRevision | PlotSettings | monotonic revision | 0 | Render-only provenance. | Legend and labels should not rerun numerical tracing. | Texture can ignore visible layer choices. |
| ViewTransformRevision | PlotCanvas | monotonic revision | 0 | Pan/zoom provenance. | Hit geometry is screen/view dependent. | Clicks select the wrong feature. |
| TextureRevision | PlotCanvas | monotonic revision | 0 | Bitmap texture provenance. | Texture is derived from projection and render settings. | A prior document remains visible. |
| HitGeometryRevision | PlotCanvas | monotonic revision | 0 | Hit-test provenance. | Hover and click must use current geometry. | Selection describes stale lines or nodes. |
| RegisteredQueryRevision | InterpolationResultsTable | monotonic revision | 0 | Registered query collection identity. | Settings changes recalculate existing clicks without duplicates. | Rows can silently retain obsolete settings. |
| WindowLayoutRevision | MainWindow | monotonic revision | 0 | User layout provenance. | Panel changes and explicit restoration are persistent choices. | A stale layout can make controls inaccessible. |
| WindowLogicalGeometry | MainWindow | logical width, height, outer position, maximized | 1280 x 800 | DPI-independent native window geometry. | Logical size remains sensible across monitors. | Double scaling grows or shrinks the window. |
| WindowScaleFactor | MainWindow | positive milli-scale factor | 1000 | Physical rendering scale. | Physical size is logical size times scale only once. | Monitor transitions can repeatedly resize the window. |
| PanelVisibility | GridResults | visible, hidden | visible | Whether a panel is reachable. | Side panes may collapse at constrained sizes. | Hidden controls can retain focus. |
| PanelWidth | GridResults | bounded logical points | configured | Resizable panel width. | Canvas and results share finite window space. | A zero-sized visible panel is unreachable. |
