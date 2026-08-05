# GUI element contracts

Generated from the typed viewer contract registry.

| ID | Type | Location | Actions | Effects | Invalidation | Layout | Migration |
| --- | --- | --- | --- | --- | --- | --- | --- |
| MainWindow | Panel | MainWindow | [] | [] | [None] | Reflow/Reflow | Declared |
| GlobalToolbar | Toolbar | Toolbar | [] | [] | [None] | Wrap/Expand | Declared |
| TabBar | Toolbar | Toolbar | [] | [] | [None] | Wrap/Expand | Declared |
| Status | Status | Status | [] | [] | [None] | Wrap/Expand | Declared |
| TabData | Tab | TabBar | [TabSelected] | [] | [None] | Wrap/Expand | FullyContractDriven |
| TabDiagnostics | Tab | TabBar | [TabSelected] | [] | [None] | Wrap/Expand | FullyContractDriven |
| TabGridInspection | Tab | TabBar | [TabSelected] | [] | [None] | Wrap/Expand | FullyContractDriven |
| TabPlot | Tab | TabBar | [TabSelected] | [] | [None] | Wrap/Expand | FullyContractDriven |
| Open | Button | Toolbar | [OpenRequested] | [ShowOpenDialog] | [DatasetAndProjection] | Wrap/Expand | FullyContractDriven |
| Save | Button | Toolbar | [SaveRequested] | [ShowSaveDialog, SaveDataset] | [None] | Wrap/Expand | FullyContractDriven |
| SaveAs | Button | Toolbar | [SaveAsRequested] | [ShowSaveDialog, SaveDataset] | [None] | Wrap/Expand | FullyContractDriven |
| Reload | Button | Toolbar | [OpenRequested] | [ShowOpenDialog] | [DatasetAndProjection] | Wrap/Expand | Declared |
| Recalculate | Panel | Data | [RecalculateRequested] | [RecalculateProjection] | [ProjectionCalculation] | Wrap/Scroll | Declared |
| ExportSvg | Button | Toolbar | [ExportRequested] | [ShowSaveDialog, Export] | [None] | Wrap/Expand | FullyContractDriven |
| ExportPng | Button | Toolbar | [ExportRequested] | [ShowSaveDialog, Export] | [None] | Wrap/Expand | FullyContractDriven |
| ExportLinesCsv | Button | Toolbar | [ExportRequested] | [ShowSaveDialog, Export] | [None] | Wrap/Expand | FullyContractDriven |
| CopyLinesCsv | Panel | Data | [] | [] | [None] | Wrap/Scroll | Declared |
| Fit | Panel | Data | [ViewTransformChanged] | [RebuildHitGeometry] | [ViewOnly, HitGeometryOnly] | Wrap/Scroll | Declared |
| ResetView | Panel | Data | [ViewTransformChanged] | [RebuildHitGeometry] | [ViewOnly, HitGeometryOnly] | Wrap/Scroll | Declared |
| DataPanel | Panel | Data | [] | [] | [None] | Wrap/Scroll | Declared |
| DataDeclarations | Panel | Data | [DatasetEdited] | [] | [DatasetValidation, DatasetAndProjection] | Wrap/Scroll | Declared |
| DataComponents | Panel | Data | [DatasetEdited] | [] | [DatasetValidation, DatasetAndProjection] | Wrap/Scroll | Declared |
| DataPhases | Panel | Data | [DatasetEdited] | [] | [DatasetValidation, DatasetAndProjection] | Wrap/Scroll | Declared |
| DataPhaseMove | Panel | Data | [DatasetEdited] | [] | [DatasetValidation, DatasetAndProjection] | Wrap/Scroll | Declared |
| DataPhaseRemove | Panel | Data | [DatasetEdited] | [] | [DatasetValidation, DatasetAndProjection] | Wrap/Scroll | Declared |
| DataProperties | Panel | Data | [DatasetEdited] | [] | [DatasetValidation, DatasetAndProjection] | Wrap/Scroll | Declared |
| DataPropertyMove | Panel | Data | [DatasetEdited] | [] | [DatasetValidation, DatasetAndProjection] | Wrap/Scroll | Declared |
| DataPropertyRemove | Panel | Data | [DatasetEdited] | [] | [DatasetValidation, DatasetAndProjection] | Wrap/Scroll | Declared |
| DataGridList | Panel | Data | [DatasetEdited] | [] | [DatasetValidation, DatasetAndProjection] | Wrap/Scroll | Declared |
| DataGridEditor | Panel | Data | [DatasetEdited] | [] | [DatasetValidation, DatasetAndProjection] | Wrap/Scroll | Declared |
| DataResolution | Panel | Data | [NumericEditChanged, NumericEditCommitted, NumericEditCancelled] | [RecalculateProjection] | [ProjectionCalculation] | Wrap/Scroll | Declared |
| DataPastePreview | Panel | Data | [DatasetEdited] | [] | [DatasetValidation, DatasetAndProjection] | Wrap/Scroll | Declared |
| DataPasteApply | Panel | Data | [DatasetEdited] | [] | [DatasetValidation, DatasetAndProjection] | Wrap/Scroll | Declared |
| DataUndo | Panel | Data | [DatasetEdited] | [] | [DatasetValidation, DatasetAndProjection] | Wrap/Scroll | Declared |
| DataRedo | Panel | Data | [DatasetEdited] | [] | [DatasetValidation, DatasetAndProjection] | Wrap/Scroll | Declared |
| DataCopyCompositions | Panel | Data | [] | [] | [None] | Wrap/Scroll | Declared |
| DataCopyGrid | Panel | Data | [] | [] | [None] | Wrap/Scroll | Declared |
| DiagnosticsPanel | Panel | Diagnostics | [] | [] | [None] | Wrap/Scroll | Declared |
| DiagnosticsStateInspector | Panel | Diagnostics | [] | [] | [None] | Wrap/Scroll | Declared |
| DiagnosticsEventTrace | Panel | Diagnostics | [] | [] | [None] | Wrap/Scroll | Declared |
| DiagnosticsCopyState | Panel | Diagnostics | [] | [] | [None] | Wrap/Scroll | Declared |
| DiagnosticsCopyTrace | Panel | Diagnostics | [] | [] | [None] | Wrap/Scroll | Declared |
| DiagnosticsLayoutInspector | Panel | Diagnostics | [] | [] | [None] | Wrap/Scroll | Declared |
| GridSettings | ComboBox | GridSettings | [] | [] | [None] | Wrap/Scroll | Declared |
| GridCanvas | Canvas | GridCanvas | [ViewTransformChanged] | [RebuildHitGeometry] | [ViewOnly, HitGeometryOnly] | Reflow/Reflow | Declared |
| GridResults | Panel | GridResults | [] | [] | [None] | Scroll/Scroll | Declared |
| GridMode | ComboBox | GridSettings | [GridInspectionChanged] | [] | [LocalInterpolation] | Wrap/Scroll | Declared |
| GridSelector | ComboBox | GridSettings | [GridInspectionChanged] | [] | [LocalInterpolation] | Wrap/Scroll | Declared |
| GridPhaseSelector | ComboBox | GridSettings | [GridInspectionChanged] | [] | [LocalInterpolation] | Wrap/Scroll | Declared |
| GridPropertySelector | ComboBox | GridSettings | [GridInspectionChanged] | [] | [LocalInterpolation] | Wrap/Scroll | Declared |
| GridInterpolation | ComboBox | GridSettings | [CalculationSettingsCommitted] | [RecalculateRegisteredQueries, RecalculateProjection] | [RegisteredQueryBatch, ProjectionCalculation] | Wrap/Scroll | Declared |
| GridPointEditor | ComboBox | GridSettings | [DatasetEdited] | [] | [DatasetValidation, DatasetAndProjection] | Wrap/Scroll | Declared |
| GridPointApply | ComboBox | GridSettings | [DatasetEdited] | [] | [DatasetValidation, DatasetAndProjection] | Wrap/Scroll | Declared |
| GridStateFilter | ComboBox | GridSettings | [RenderSettingsChanged] | [RebuildPlotTexture] | [RenderOnly] | Wrap/Scroll | Declared |
| GridLabelMode | ComboBox | GridSettings | [RenderSettingsChanged] | [RebuildPlotTexture] | [RenderOnly] | Wrap/Scroll | Declared |
| InterpolationResultsTable | Table | GridResults | [] | [] | [None] | Scroll/Scroll | Declared |
| InterpolationResultsCopy | ComboBox | GridSettings | [] | [] | [None] | Wrap/Scroll | Declared |
| InterpolationResultsClear | ComboBox | GridSettings | [RegisteredQueriesCleared] | [] | [None] | Wrap/Scroll | Declared |
| PlotSettings | ComboBox | PlotSettings | [] | [] | [None] | Wrap/Scroll | Declared |
| PlotCanvas | Canvas | PlotCanvas | [ViewTransformChanged] | [RebuildHitGeometry] | [ViewOnly, HitGeometryOnly] | Reflow/Reflow | Declared |
| PlotInterpolation | ComboBox | PlotSettings | [CalculationSettingsCommitted] | [RecalculateRegisteredQueries, RecalculateProjection] | [RegisteredQueryBatch, ProjectionCalculation] | Wrap/Scroll | Declared |
| PlotSampling | ComboBox | PlotSettings | [NumericEditChanged, NumericEditCommitted, NumericEditCancelled] | [RecalculateProjection] | [ProjectionCalculation] | Wrap/Scroll | Declared |
| PlotLevels | ComboBox | PlotSettings | [NumericEditChanged, NumericEditCommitted, NumericEditCancelled] | [RecalculateProjection] | [ProjectionCalculation] | Wrap/Scroll | Declared |
| PlotRegularization | ComboBox | PlotSettings | [] | [] | [None] | Wrap/Scroll | Declared |
| PlotLegend | ComboBox | PlotSettings | [RenderSettingsChanged] | [RebuildPlotTexture] | [RenderOnly] | Wrap/Scroll | Declared |
| PlotAxisLabels | ComboBox | PlotSettings | [RenderSettingsChanged] | [RebuildPlotTexture] | [RenderOnly] | Wrap/Scroll | Declared |
| PlotPathVertices | ComboBox | PlotSettings | [RenderSettingsChanged] | [RebuildPlotTexture] | [RenderOnly] | Wrap/Scroll | Declared |
| PlotPathMode | ComboBox | PlotSettings | [RenderSettingsChanged] | [RebuildPlotTexture] | [RenderOnly] | Wrap/Scroll | Declared |
| OpenDialog | Dialog | NativeDialog | [] | [] | [None] | Wrap/Scroll | FullyContractDriven |
| SaveDialog | Dialog | NativeDialog | [] | [] | [None] | Wrap/Scroll | FullyContractDriven |
| ExportDialog | Dialog | NativeDialog | [] | [] | [None] | Wrap/Scroll | FullyContractDriven |
| UnsavedChangesDialog | Dialog | NativeDialog | [] | [] | [None] | Wrap/Scroll | FullyContractDriven |
