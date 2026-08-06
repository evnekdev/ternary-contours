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

## Qt Designer public objects

The Qt object identity and source file are generated from `.ui` XML; the semantic behaviour remains in the core registry.

| Qt objectName | .ui source | Qt class | Core contract |
| --- | --- | --- | --- |
| aboutButtonBox | about_dialog.ui | QDialogButtonBox | DiagnosticsPanel |
| aboutDialog | about_dialog.ui | QDialog | DiagnosticsPanel |
| actionAboutApplication | main_window.ui | QAction | DiagnosticsPanel |
| actionAboutDocumentation | main_window.ui | QAction | DiagnosticsPanel |
| actionAboutLicenses | main_window.ui | QAction | DiagnosticsPanel |
| actionAboutQt | main_window.ui | QAction | DiagnosticsPanel |
| actionExportLinesCsv | main_window.ui | QAction | ExportLinesCsv |
| actionExportPng | main_window.ui | QAction | ExportPng |
| actionExportSvg | main_window.ui | QAction | ExportSvg |
| actionFileNew | main_window.ui | QAction | DataDeclarations |
| actionFileOpen | main_window.ui | QAction | Open |
| actionFileSave | main_window.ui | QAction | Save |
| actionFileSaveAs | main_window.ui | QAction | SaveAs |
| actionGridAddIrregular | main_window.ui | QAction | DataGridEditor |
| actionGridAddPhaseField | main_window.ui | QAction | DataGridEditor |
| actionGridAddRegular | main_window.ui | QAction | DataGridEditor |
| actionGridCopy | main_window.ui | QAction | DataCopyGrid |
| actionGridDuplicate | main_window.ui | QAction | DataGridEditor |
| actionGridExtrapolate | main_window.ui | QAction | DataGridEditor |
| actionGridModifyPhaseField | main_window.ui | QAction | DataGridEditor |
| actionGridPaste | main_window.ui | QAction | DataPasteApply |
| actionGridRecalculate | main_window.ui | QAction | Recalculate |
| actionGridRemove | main_window.ui | QAction | DataGridEditor |
| actionGridRemovePhaseField | main_window.ui | QAction | DataGridEditor |
| actionGridRename | main_window.ui | QAction | DataGridEditor |
| actionGridValidate | main_window.ui | QAction | DataGridEditor |
| actionQuit | main_window.ui | QAction | DataDeclarations |
| actionSettings | main_window.ui | QAction | DataDeclarations |
| actionViewAxisLabels | main_window.ui | QAction | PlotSettings |
| actionViewBinaryInvariants | main_window.ui | QAction | PlotSettings |
| actionViewCornerNames | main_window.ui | QAction | PlotSettings |
| actionViewFit | main_window.ui | QAction | Fit |
| actionViewGrid | main_window.ui | QAction | GridStateFilter |
| actionViewInteriorInvariants | main_window.ui | QAction | PlotSettings |
| actionViewLegend | main_window.ui | QAction | PlotSettings |
| actionViewPlot | main_window.ui | QAction | PlotLegend |
| actionViewQueryPoints | main_window.ui | QAction | PlotSettings |
| actionViewReset | main_window.ui | QAction | ResetView |
| actionViewRestoreLayout | main_window.ui | QAction | ResetView |
| actionViewResultsTable | main_window.ui | QAction | PlotSettings |
| actionViewSourceVertices | main_window.ui | QAction | PlotSettings |
| actionViewStableIsotherms | main_window.ui | QAction | PlotSettings |
| actionViewStableUnivariants | main_window.ui | QAction | PlotSettings |
| actionViewerClearAllQueries | main_window.ui | QAction | PlotSettings |
| actionViewerClearSelectedQuery | main_window.ui | QAction | PlotSettings |
| actionViewerResetAutomaticRange | main_window.ui | QAction | PlotSettings |
| addGridButtonBox | add_grid_dialog.ui | QDialogButtonBox | DataGridEditor |
| addGridDialog | add_grid_dialog.ui | QDialog | DataGridEditor |
| buttonAddIrregularRow | main_window.ui | QPushButton | DataGridEditor |
| buttonAddPhase | main_window.ui | QPushButton | DataPhases |
| buttonAddProperty | main_window.ui | QPushButton | DataProperties |
| buttonRemovePhase | main_window.ui | QPushButton | DataPhases |
| buttonViewerExtrapolatePhase | main_window.ui | QPushButton | GridPointEditor |
| buttonViewerResetAutomaticRange | main_window.ui | QPushButton | PlotLevels |
| canvasTernary | main_window.ui | TernaryCanvas | PlotCanvas |
| checkPropertyRequired | property_editor_dialog.ui | QCheckBox | DataProperties |
| checkViewerAutomaticRange | main_window.ui | QCheckBox | PlotLevels |
| checkViewerAxisLabels | main_window.ui | QCheckBox | PlotSettings |
| checkViewerBinaryInvariants | main_window.ui | QCheckBox | PlotSettings |
| checkViewerCalculated | main_window.ui | QCheckBox | GridStateFilter |
| checkViewerContainingTriangle | main_window.ui | QCheckBox | PlotSettings |
| checkViewerContourEndpoints | main_window.ui | QCheckBox | PlotSettings |
| checkViewerCornerNames | main_window.ui | QCheckBox | PlotSettings |
| checkViewerCutOff | main_window.ui | QCheckBox | GridStateFilter |
| checkViewerExtrapolated | main_window.ui | QCheckBox | GridStateFilter |
| checkViewerInteriorInvariants | main_window.ui | QCheckBox | PlotSettings |
| checkViewerInvariantIds | main_window.ui | QCheckBox | PlotSettings |
| checkViewerLabelsSelectedOnly | main_window.ui | QCheckBox | GridLabelMode |
| checkViewerLegend | main_window.ui | QCheckBox | PlotSettings |
| checkViewerMissing | main_window.ui | QCheckBox | GridStateFilter |
| checkViewerPathVertices | main_window.ui | QCheckBox | PlotSettings |
| checkViewerPhasePairLabels | main_window.ui | QCheckBox | PlotSettings |
| checkViewerQueryPoints | main_window.ui | QCheckBox | PlotSettings |
| checkViewerRegularGridEdges | main_window.ui | QCheckBox | GridStateFilter |
| checkViewerRegularizePaths | main_window.ui | QCheckBox | PlotRegularization |
| checkViewerSamplingGrid | main_window.ui | QCheckBox | PlotSettings |
| checkViewerSourceVertices | main_window.ui | QCheckBox | PlotSettings |
| checkViewerStableIsotherms | main_window.ui | QCheckBox | PlotSettings |
| checkViewerStableUnivariants | main_window.ui | QCheckBox | PlotSettings |
| checkViewerUnivariantEndpoints | main_window.ui | QCheckBox | PlotSettings |
| checkViewerUnivariantIds | main_window.ui | QCheckBox | PlotSettings |
| editAddGridName | add_grid_dialog.ui | QLineEdit | DataGridEditor |
| editCornerA | main_window.ui | QLineEdit | DataComponents |
| editCornerB | main_window.ui | QLineEdit | DataComponents |
| editCornerC | main_window.ui | QLineEdit | DataComponents |
| editPhaseName | phase_editor_dialog.ui | QLineEdit | DataPhases |
| editProjectTitle | main_window.ui | QLineEdit | DataDeclarations |
| editPropertyName | property_editor_dialog.ui | QLineEdit | DataProperties |
| editPropertyUnit | property_editor_dialog.ui | QLineEdit | DataProperties |
| editViewerRegularizationSpacing | main_window.ui | QLineEdit | PlotRegularization |
| editViewerStep | main_window.ui | QLineEdit | PlotLevels |
| editViewerTmax | main_window.ui | QLineEdit | PlotLevels |
| editViewerTmin | main_window.ui | QLineEdit | PlotLevels |
| mainWindow | main_window.ui | QMainWindow | MainWindow |
| menuAbout | main_window.ui | QMenu | GlobalToolbar |
| menuAddGrid | main_window.ui | QMenu | GlobalToolbar |
| menuBarMain | main_window.ui | QMenuBar | GlobalToolbar |
| menuExport | main_window.ui | QMenu | GlobalToolbar |
| menuFile | main_window.ui | QMenu | GlobalToolbar |
| menuGrid | main_window.ui | QMenu | GlobalToolbar |
| menuView | main_window.ui | QMenu | GlobalToolbar |
| phaseEditorButtonBox | phase_editor_dialog.ui | QDialogButtonBox | DataPhases |
| phaseEditorDialog | phase_editor_dialog.ui | QDialog | DataPhases |
| primaryTabs | main_window.ui | QTabWidget | TabBar |
| propertyEditorButtonBox | property_editor_dialog.ui | QDialogButtonBox | DataProperties |
| propertyEditorDialog | property_editor_dialog.ui | QDialog | DataProperties |
| radioAddIrregularGrid | add_grid_dialog.ui | QRadioButton | DataGridEditor |
| radioAddRegularGrid | add_grid_dialog.ui | QRadioButton | DataGridEditor |
| settingsButtonBox | settings_dialog.ui | QDialogButtonBox | DataDeclarations |
| settingsDialog | settings_dialog.ui | QDialog | DataDeclarations |
| settingsTabs | settings_dialog.ui | QTabWidget | DataDeclarations |
| spinAddGridSubdivisions | add_grid_dialog.ui | QSpinBox | DataGridEditor |
| spinPhaseIdentifier | phase_editor_dialog.ui | QSpinBox | DataPhases |
| spinViewerLabelDecimals | main_window.ui | QSpinBox | GridPointEditor |
| spinViewerLineWidth | main_window.ui | QSpinBox | PlotSettings |
| spinViewerMarkerSize | main_window.ui | QSpinBox | GridPointEditor |
| spinViewerPlotMarkerSize | main_window.ui | QSpinBox | PlotSettings |
| spinViewerSamplingSubdivisions | main_window.ui | QSpinBox | PlotSampling |
| splitterData | main_window.ui | QSplitter | DataPanel |
| splitterViewerControls | main_window.ui | QSplitter | PlotSettings |
| splitterViewerOuter | main_window.ui | QSplitter | PlotSettings |
| splitterViewerRight | main_window.ui | QSplitter | GridResults |
| statusMain | main_window.ui | QStatusBar | Status |
| tabData | main_window.ui | QWidget | TabData |
| tabViewer | main_window.ui | QWidget | TabPlot |
| tableGridValues | main_window.ui | QTableView | DataGridEditor |
| tableInterpolationResults | main_window.ui | QTableView | InterpolationResultsTable |
| treeProject | main_window.ui | QTreeView | DataGridList |
