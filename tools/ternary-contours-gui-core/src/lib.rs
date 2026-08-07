//! Framework-neutral GUI state, actions, effects, and inventory.
//!
//! Rendering emits typed actions. The reducer performs no native I/O, filesystem
//! access, worker spawning, GUI calls, or wall-clock reads.

include!(concat!(env!("OUT_DIR"), "/qt_ui_ids.rs"));
include!(concat!(env!("OUT_DIR"), "/qt_ui_inventory.rs"));
include!(concat!(env!("OUT_DIR"), "/qt_ui_hierarchy.rs"));
include!(concat!(env!("OUT_DIR"), "/qt_ui_actions.rs"));
include!(concat!(env!("OUT_DIR"), "/qt_ui_tab_order.rs"));

use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Revision(pub u64);
impl Revision {
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct RequestId(pub u64);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ViewerTab {
    #[default]
    Data,
    Diagnostics,
    GridInspection,
    Plot,
}
impl ViewerTab {
    pub const ORDERED: [Self; 4] = [
        Self::Data,
        Self::Diagnostics,
        Self::GridInspection,
        Self::Plot,
    ];
    pub const fn next(self, backwards: bool) -> Self {
        let current = match self {
            Self::Data => 0,
            Self::Diagnostics => 1,
            Self::GridInspection => 2,
            Self::Plot => 3,
        };
        let next = if backwards {
            (current + 3) % 4
        } else {
            (current + 1) % 4
        };
        Self::ORDERED[next]
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::Data => "Data",
            Self::Diagnostics => "Diagnostics",
            Self::GridInspection => "Grid inspection",
            Self::Plot => "Plot",
        }
    }
}

macro_rules! ui_ids {
    ($($id:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum UiElementId { $($id),+ }
        impl UiElementId {
            pub const ALL: &[Self] = &[$(Self::$id),+];
            pub const fn name(self) -> &'static str {
                match self { $(Self::$id => stringify!($id)),+ }
            }
        }
    };
}
ui_ids!(
    MainWindow,
    GlobalToolbar,
    TabBar,
    Status,
    TabData,
    TabDiagnostics,
    TabGridInspection,
    TabPlot,
    Open,
    Save,
    SaveAs,
    Reload,
    Recalculate,
    ExportSvg,
    ExportPng,
    ExportLinesCsv,
    CopyLinesCsv,
    Fit,
    ResetView,
    DataPanel,
    DataDeclarations,
    DataComponents,
    DataPhases,
    DataPhaseMove,
    DataPhaseRemove,
    DataProperties,
    DataPropertyMove,
    DataPropertyRemove,
    DataGridList,
    DataGridEditor,
    DataResolution,
    DataPastePreview,
    DataPasteApply,
    DataUndo,
    DataRedo,
    DataCopyCompositions,
    DataCopyGrid,
    DiagnosticsPanel,
    DiagnosticsStateInspector,
    DiagnosticsEventTrace,
    DiagnosticsCopyState,
    DiagnosticsCopyTrace,
    DiagnosticsLayoutInspector,
    GridSettings,
    GridCanvas,
    GridResults,
    GridMode,
    GridSelector,
    GridPhaseSelector,
    GridPropertySelector,
    GridInterpolation,
    GridPointEditor,
    GridPointApply,
    GridStateFilter,
    GridLabelMode,
    InterpolationResultsTable,
    InterpolationResultsCopy,
    InterpolationResultsClear,
    PlotSettings,
    PlotCanvas,
    PlotInterpolation,
    PlotSampling,
    PlotLevels,
    PlotRegularization,
    PlotLegend,
    PlotAxisLabels,
    PlotPathVertices,
    PlotPathMode,
    OpenDialog,
    SaveDialog,
    ExportDialog,
    UnsavedChangesDialog,
);

/// Map a Qt Designer public object to its toolkit-neutral semantic contract.
/// `QtUiElementId` is generated directly from `.ui` XML; this mapping owns
/// behaviour rather than XML.
pub fn qt_ui_contract_id(object_name: &str) -> Option<UiElementId> {
    let id = match object_name {
        "mainWindow" => UiElementId::MainWindow,
        "menuBarMain" => UiElementId::GlobalToolbar,
        "menuFile" | "menuGrid" | "menuView" | "menuAbout" | "menuExport" | "menuAddGrid" => {
            UiElementId::GlobalToolbar
        }
        "primaryTabs" => UiElementId::TabBar,
        "tabData" => UiElementId::TabData,
        "tabViewer" => UiElementId::TabPlot,
        "treeProject" => UiElementId::DataGridList,
        "tableGridValues" => UiElementId::DataGridEditor,
        "editProjectTitle" => UiElementId::DataDeclarations,
        "editCornerA" | "editCornerB" | "editCornerC" => UiElementId::DataComponents,
        "buttonAddPhase" | "buttonRemovePhase" => UiElementId::DataPhases,
        "buttonAddProperty" => UiElementId::DataProperties,
        "buttonAddIrregularRow" => UiElementId::DataGridEditor,
        "splitterData" => UiElementId::DataPanel,
        "splitterViewerOuter" => UiElementId::PlotSettings,
        "splitterViewerControls" => UiElementId::PlotSettings,
        "splitterViewerRight"
        | "resultTablesPane"
        | "splitterViewerResultTables"
        | "groupInterpolationResults"
        | "groupInvariantPoints"
        | "tableInvariantPoints"
        | "labelInterpolationResultsStatus"
        | "labelInvariantPointsStatus"
        | "buttonInvariantCopy" => UiElementId::GridResults,
        "canvasTernary" => UiElementId::PlotCanvas,
        "tableInterpolationResults" => UiElementId::InterpolationResultsTable,
        "buttonInterpolationCopy" => UiElementId::InterpolationResultsCopy,
        "buttonInterpolationRemoveSelected" | "buttonInterpolationClearAll" => {
            UiElementId::InterpolationResultsClear
        }
        "statusMain" => UiElementId::Status,
        "buttonViewerResetAutomaticRange" => UiElementId::PlotLevels,
        "buttonViewerExtrapolatePhase" => UiElementId::GridPointEditor,
        "comboViewerGrid" => UiElementId::GridSelector,
        "comboViewerPhase" => UiElementId::GridPhaseSelector,
        "comboViewerProperty" => UiElementId::GridPropertySelector,
        "comboViewerMode" => UiElementId::GridMode,
        "comboViewerLabelMode" => UiElementId::GridLabelMode,
        "comboViewerSourceInterpolation"
        | "comboViewerCubicMethod"
        | "comboViewerPartialDomain"
        | "comboViewerContinuation" => UiElementId::PlotInterpolation,
        "comboViewerPathDisplay" => UiElementId::PlotPathMode,
        "editViewerIsoLevelSpec" => UiElementId::PlotLevels,
        "editViewerRegularizationSpacing" => UiElementId::PlotRegularization,
        "spinViewerMarkerSize" | "spinViewerLabelDecimals" => UiElementId::GridPointEditor,
        "spinViewerSamplingSubdivisions" => UiElementId::PlotSampling,
        "spinViewerLineWidth" | "spinViewerPlotMarkerSize" => UiElementId::PlotSettings,
        "checkViewerCalculated"
        | "checkViewerExtrapolated"
        | "checkViewerCutOff"
        | "checkViewerMissing"
        | "checkViewerRegularGridEdges" => UiElementId::GridStateFilter,
        "checkViewerLabelsSelectedOnly" => UiElementId::GridLabelMode,
        "checkViewerRegularizePaths" => UiElementId::PlotRegularization,
        "checkViewerStableIsotherms"
        | "checkViewerStableUnivariants"
        | "checkViewerBinaryInvariants"
        | "checkViewerInteriorInvariants"
        | "checkViewerSamplingGrid"
        | "checkViewerSourceVertices"
        | "checkViewerQueryPoints"
        | "checkViewerAxisLabels"
        | "checkViewerCornerNames"
        | "checkViewerLegend"
        | "checkViewerPathVertices"
        | "checkViewerContourEndpoints"
        | "checkViewerUnivariantEndpoints"
        | "checkViewerInvariantIds"
        | "checkViewerUnivariantIds"
        | "checkViewerPhasePairLabels"
        | "checkViewerIsoLineLabels"
        | "checkViewerContainingTriangle" => UiElementId::PlotSettings,

        "actionFileOpen" => UiElementId::Open,
        "actionFileSave" => UiElementId::Save,
        "actionFileSaveAs" => UiElementId::SaveAs,
        "actionExportPng" => UiElementId::ExportPng,
        "actionExportSvg" => UiElementId::ExportSvg,
        "actionExportLinesCsv" => UiElementId::ExportLinesCsv,
        "actionViewPlot" => UiElementId::PlotLegend,
        "actionViewGrid" => UiElementId::GridStateFilter,
        "actionViewFit" => UiElementId::Fit,
        "actionViewReset" | "actionViewRestoreLayout" => UiElementId::ResetView,
        "actionGridCopy" => UiElementId::DataCopyGrid,
        "actionGridPaste" => UiElementId::DataPasteApply,
        "actionGridRecalculate" => UiElementId::Recalculate,
        "actionViewerCopyQueries" => UiElementId::InterpolationResultsCopy,
        "actionViewerClearSelectedQuery" | "actionViewerClearAllQueries" => {
            UiElementId::InterpolationResultsClear
        }
        "actionViewerCopyInvariantPoints" => UiElementId::GridResults,
        "actionGridValidate" => UiElementId::DataGridEditor,
        name if name.starts_with("actionGrid") => UiElementId::DataGridEditor,
        name if name.starts_with("actionFile")
            || name == "actionSettings"
            || name == "actionQuit" =>
        {
            UiElementId::DataDeclarations
        }
        name if name.starts_with("actionAbout") => UiElementId::DiagnosticsPanel,
        name if name.starts_with("actionView") => UiElementId::PlotSettings,
        "settingsDialog" | "settingsTabs" | "settingsButtonBox" => UiElementId::DataDeclarations,
        "interpolationPointDialog"
        | "buttonBoxInterpolationPoint"
        | "editGlobalA"
        | "editGlobalB"
        | "editGlobalC"
        | "editLocal0"
        | "editLocal1"
        | "editLocal2" => UiElementId::GridInterpolation,
        "addGridDialog"
        | "addGridButtonBox"
        | "editAddGridName"
        | "radioAddRegularGrid"
        | "radioAddIrregularGrid"
        | "spinAddGridSubdivisions" => UiElementId::DataGridEditor,
        "phaseEditorDialog" | "phaseEditorButtonBox" | "editPhaseName" | "spinPhaseIdentifier" => {
            UiElementId::DataPhases
        }
        "propertyEditorDialog"
        | "propertyEditorButtonBox"
        | "editPropertyName"
        | "editPropertyUnit"
        | "checkPropertyRequired" => UiElementId::DataProperties,
        "aboutDialog" | "aboutButtonBox" => UiElementId::DiagnosticsPanel,
        _ => return None,
    };
    Some(id)
}

/// Typed native command declared by a Qt Designer `QAction`.
///
/// This describes an input event only. The Rust controller remains the owner
/// of parsing, validation, numerical calculation, and state changes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QtUiAction {
    NewDocument,
    OpenDocument,
    SaveDocument,
    SaveDocumentAs,
    ExportPng,
    ExportSvg,
    ExportLinesCsv,
    ShowSettings,
    Quit,
    AddRegularGrid,
    AddIrregularGrid,
    DuplicateGrid,
    RenameGrid,
    RemoveGrid,
    AddPhaseField,
    ModifyPhaseField,
    RemovePhaseField,
    ValidateGrid,
    RecalculateGrid,
    CopyGrid,
    PasteGrid,
    ExtrapolateMesh,
    TogglePlotLayer,
    ToggleGridLayer,
    ToggleSourceVertices,
    ToggleQueryPoints,
    ToggleResultsTable,
    FitView,
    ResetView,
    RestoreDefaultLayout,
    ToggleStableIsotherms,
    ToggleStableUnivariants,
    ToggleBinaryInvariants,
    ToggleInteriorInvariants,
    ToggleAxisLabels,
    ToggleCornerNames,
    ToggleLegend,
    CopyInterpolationResults,
    ClearSelectedQuery,
    ClearAllQueries,
    CopyInvariantPoints,
    ResetAutomaticIsoRange,
    ShowApplicationAbout,
    ShowDocumentation,
    ShowLicences,
    ShowQtAbout,
}

/// Bind every generated QAction identity to an explicit native command.
/// Returning `None` for a generated QAction is a contract-test failure.
pub const fn qt_ui_action(id: QtUiElementId) -> Option<QtUiAction> {
    let action = match id {
        QtUiElementId::ActionFileNew => QtUiAction::NewDocument,
        QtUiElementId::ActionFileOpen => QtUiAction::OpenDocument,
        QtUiElementId::ActionFileSave => QtUiAction::SaveDocument,
        QtUiElementId::ActionFileSaveAs => QtUiAction::SaveDocumentAs,
        QtUiElementId::ActionExportPng => QtUiAction::ExportPng,
        QtUiElementId::ActionExportSvg => QtUiAction::ExportSvg,
        QtUiElementId::ActionExportLinesCsv => QtUiAction::ExportLinesCsv,
        QtUiElementId::ActionSettings => QtUiAction::ShowSettings,
        QtUiElementId::ActionQuit => QtUiAction::Quit,
        QtUiElementId::ActionGridAddRegular => QtUiAction::AddRegularGrid,
        QtUiElementId::ActionGridAddIrregular => QtUiAction::AddIrregularGrid,
        QtUiElementId::ActionGridDuplicate => QtUiAction::DuplicateGrid,
        QtUiElementId::ActionGridRename => QtUiAction::RenameGrid,
        QtUiElementId::ActionGridRemove => QtUiAction::RemoveGrid,
        QtUiElementId::ActionGridAddPhaseField => QtUiAction::AddPhaseField,
        QtUiElementId::ActionGridModifyPhaseField => QtUiAction::ModifyPhaseField,
        QtUiElementId::ActionGridRemovePhaseField => QtUiAction::RemovePhaseField,
        QtUiElementId::ActionGridValidate => QtUiAction::ValidateGrid,
        QtUiElementId::ActionGridRecalculate => QtUiAction::RecalculateGrid,
        QtUiElementId::ActionGridCopy => QtUiAction::CopyGrid,
        QtUiElementId::ActionGridPaste => QtUiAction::PasteGrid,
        QtUiElementId::ActionGridExtrapolate => QtUiAction::ExtrapolateMesh,
        QtUiElementId::ActionViewPlot => QtUiAction::TogglePlotLayer,
        QtUiElementId::ActionViewGrid => QtUiAction::ToggleGridLayer,
        QtUiElementId::ActionViewSourceVertices => QtUiAction::ToggleSourceVertices,
        QtUiElementId::ActionViewQueryPoints => QtUiAction::ToggleQueryPoints,
        QtUiElementId::ActionViewResultsTable => QtUiAction::ToggleResultsTable,
        QtUiElementId::ActionViewFit => QtUiAction::FitView,
        QtUiElementId::ActionViewReset => QtUiAction::ResetView,
        QtUiElementId::ActionViewRestoreLayout => QtUiAction::RestoreDefaultLayout,
        QtUiElementId::ActionViewStableIsotherms => QtUiAction::ToggleStableIsotherms,
        QtUiElementId::ActionViewStableUnivariants => QtUiAction::ToggleStableUnivariants,
        QtUiElementId::ActionViewBinaryInvariants => QtUiAction::ToggleBinaryInvariants,
        QtUiElementId::ActionViewInteriorInvariants => QtUiAction::ToggleInteriorInvariants,
        QtUiElementId::ActionViewAxisLabels => QtUiAction::ToggleAxisLabels,
        QtUiElementId::ActionViewCornerNames => QtUiAction::ToggleCornerNames,
        QtUiElementId::ActionViewLegend => QtUiAction::ToggleLegend,
        QtUiElementId::ActionViewerCopyQueries => QtUiAction::CopyInterpolationResults,
        QtUiElementId::ActionViewerClearSelectedQuery => QtUiAction::ClearSelectedQuery,
        QtUiElementId::ActionViewerClearAllQueries => QtUiAction::ClearAllQueries,
        QtUiElementId::ActionViewerCopyInvariantPoints => QtUiAction::CopyInvariantPoints,
        QtUiElementId::ActionViewerResetAutomaticRange => QtUiAction::ResetAutomaticIsoRange,
        QtUiElementId::ActionAboutApplication => QtUiAction::ShowApplicationAbout,
        QtUiElementId::ActionAboutDocumentation => QtUiAction::ShowDocumentation,
        QtUiElementId::ActionAboutLicenses => QtUiAction::ShowLicences,
        QtUiElementId::ActionAboutQt => QtUiAction::ShowQtAbout,
        _ => return None,
    };
    Some(action)
}
/// Generated Designer objects, including source and parent metadata for
/// documentation and QtTest discovery.
pub fn qt_ui_inventory_markdown() -> String {
    let mut output = String::from(
        "# Qt Designer object inventory\n\n[Qt product and architecture contract](product-and-architecture-contract.md) is normative for this inventory.\n\nGenerated at Rust build time from `apps/ternary-contours-qt/ui/*.ui`.\n\n| Rust Qt ID | Qt objectName | Class | .ui source | Parent | Core contract | Purpose | Visible when | Enabled when | Layout policy | Typed action or model role |\n| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |\n",
    );
    let registry = ui_element_registry();
    for element in QT_UI_ELEMENTS {
        if element.is_public {
            let contract_id = qt_ui_contract_id(element.object_name);
            let contract =
                contract_id.and_then(|id| registry.iter().find(|definition| definition.id == id));
            let contract_name = contract_id.map(UiElementId::name).unwrap_or("MISSING");
            let purpose = contract
                .map(|definition| definition.purpose)
                .unwrap_or("MISSING");
            let visible_when = contract
                .map(|definition| definition.visible_when)
                .unwrap_or("MISSING");
            let enabled_when = contract
                .map(|definition| definition.enabled_when)
                .unwrap_or("MISSING");
            let layout = contract
                .map(|definition| {
                    format!(
                        "{:?}/{:?}",
                        definition.layout.horizontal, definition.layout.vertical
                    )
                })
                .unwrap_or_else(|| "MISSING".to_owned());
            let action = qt_ui_action(element.id)
                .map(|action| format!("{action:?}"))
                .unwrap_or_else(|| "Static or model host".to_owned());
            output.push_str(&format!(
                "| {:?} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
                element.id,
                element.object_name,
                element.qt_class,
                element.source_file,
                element.parent_object_name,
                contract_name,
                purpose,
                visible_when,
                enabled_when,
                layout,
                action,
            ));
        }
    }
    output
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiElementKind {
    Button,
    Checkbox,
    ComboBox,
    TextEdit,
    NumericEdit,
    Tab,
    Table,
    Canvas,
    Toolbar,
    Panel,
    Dialog,
    Status,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiLocation {
    MainWindow,
    Toolbar,
    TabBar,
    Data,
    Diagnostics,
    GridSettings,
    GridCanvas,
    GridResults,
    PlotSettings,
    PlotCanvas,
    NativeDialog,
    Status,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandardGuiBehavior {
    ImmediateAction,
    ImmediateSelectionChange,
    CommitOnEnterOrFocusLoss,
    NativeOpenDialog,
    NativeSaveDialog,
    ConfirmationBeforeDestructiveAction,
    ToggleWithoutCalculation,
    CalculationWithDebounce,
    LocalEvaluation,
    CopySelectedRows,
    AppendCanvasSelection,
    ResizePanel,
    NativeWindowMove,
    NativeWindowResize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidationClass {
    None,
    ViewOnly,
    RenderOnly,
    HitGeometryOnly,
    LocalInterpolation,
    RegisteredQueryBatch,
    ProjectionCalculation,
    DatasetValidation,
    DatasetAndProjection,
    WindowLayout,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverflowPolicy {
    Expand,
    Scroll,
    Wrap,
    Reflow,
    ExplicitCollapse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayoutContract {
    pub horizontal: OverflowPolicy,
    pub vertical: OverflowPolicy,
    pub min_width: u16,
    pub min_height: u16,
    pub resizable: bool,
    pub collapsible: bool,
}
const TOOLBAR: LayoutContract = LayoutContract {
    horizontal: OverflowPolicy::Wrap,
    vertical: OverflowPolicy::Expand,
    min_width: 320,
    min_height: 28,
    resizable: false,
    collapsible: false,
};
const FORM: LayoutContract = LayoutContract {
    horizontal: OverflowPolicy::Wrap,
    vertical: OverflowPolicy::Scroll,
    min_width: 250,
    min_height: 180,
    resizable: true,
    collapsible: false,
};
const CANVAS: LayoutContract = LayoutContract {
    horizontal: OverflowPolicy::Reflow,
    vertical: OverflowPolicy::Reflow,
    min_width: 160,
    min_height: 160,
    resizable: false,
    collapsible: false,
};
const RESULTS: LayoutContract = LayoutContract {
    horizontal: OverflowPolicy::Scroll,
    vertical: OverflowPolicy::Scroll,
    min_width: 220,
    min_height: 180,
    resizable: true,
    collapsible: true,
};
const DIALOG: LayoutContract = LayoutContract {
    horizontal: OverflowPolicy::Wrap,
    vertical: OverflowPolicy::Scroll,
    min_width: 300,
    min_height: 160,
    resizable: false,
    collapsible: false,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UiActionKind {
    TabSelected,
    OpenRequested,
    OpenDialogCompleted,
    SaveRequested,
    SaveAsRequested,
    SaveDialogCompleted,
    UnsavedDecisionSelected,
    ExportRequested,
    ExportDialogCompleted,
    RecalculateRequested,
    CalculationSettingsCommitted,
    NumericEditChanged,
    NumericEditCommitted,
    NumericEditCancelled,
    DatasetEdited,
    GridInspectionChanged,
    RegisteredQueryAdded,
    RegisteredQueriesCleared,
    RenderSettingsChanged,
    ViewTransformChanged,
    PanelChanged,
    WindowGeometryChanged,
    WindowScaleFactorChanged,
    RestoreDefaultLayoutRequested,
    DatasetLoaded,
    DatasetSaved,
    ProjectionCalculated,
    RegisteredQueriesRecalculated,
    ExportCompleted,
    EffectFailed,
}
impl UiActionKind {
    pub const ALL: &[Self] = &[
        Self::TabSelected,
        Self::OpenRequested,
        Self::OpenDialogCompleted,
        Self::SaveRequested,
        Self::SaveAsRequested,
        Self::SaveDialogCompleted,
        Self::UnsavedDecisionSelected,
        Self::ExportRequested,
        Self::ExportDialogCompleted,
        Self::RecalculateRequested,
        Self::CalculationSettingsCommitted,
        Self::NumericEditChanged,
        Self::NumericEditCommitted,
        Self::NumericEditCancelled,
        Self::DatasetEdited,
        Self::GridInspectionChanged,
        Self::RegisteredQueryAdded,
        Self::RegisteredQueriesCleared,
        Self::RenderSettingsChanged,
        Self::ViewTransformChanged,
        Self::PanelChanged,
        Self::WindowGeometryChanged,
        Self::WindowScaleFactorChanged,
        Self::RestoreDefaultLayoutRequested,
        Self::DatasetLoaded,
        Self::DatasetSaved,
        Self::ProjectionCalculated,
        Self::RegisteredQueriesRecalculated,
        Self::ExportCompleted,
        Self::EffectFailed,
    ];
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UiEffectKind {
    ShowOpenDialog,
    ShowSaveDialog,
    ShowUnsavedChangesDialog,
    LoadDataset,
    SaveDataset,
    Export,
    RecalculateProjection,
    RecalculateRegisteredQueries,
    RebuildPlotTexture,
    RebuildHitGeometry,
    CopyToClipboard,
    PersistWindowLayout,
}
impl UiEffectKind {
    pub const ALL: &[Self] = &[
        Self::ShowOpenDialog,
        Self::ShowSaveDialog,
        Self::ShowUnsavedChangesDialog,
        Self::LoadDataset,
        Self::SaveDataset,
        Self::Export,
        Self::RecalculateProjection,
        Self::RecalculateRegisteredQueries,
        Self::RebuildPlotTexture,
        Self::RebuildHitGeometry,
        Self::CopyToClipboard,
        Self::PersistWindowLayout,
    ];
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportKind {
    Svg,
    Png,
    LinesCsv,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnsavedDecision {
    Save,
    Discard,
    Cancel,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumericFieldId {
    SamplingSubdivisions,
    ContourLevels,
    RegularizationSpacing,
    GridResolution,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PanelId {
    Data,
    Diagnostics,
    GridSettings,
    GridResults,
    PlotSettings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowGeometry {
    pub logical_width: u16,
    pub logical_height: u16,
    pub outer_x: i32,
    pub outer_y: i32,
    pub maximized: bool,
}
impl Default for WindowGeometry {
    fn default() -> Self {
        Self {
            logical_width: 1280,
            logical_height: 800,
            outer_x: 0,
            outer_y: 0,
            maximized: false,
        }
    }
}

/// Monitor work area in physical desktop pixels. Coordinates may be negative.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MonitorWorkArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor_milli: u32,
}

pub fn physical_size_from_logical(geometry: WindowGeometry, scale_factor_milli: u32) -> (u32, u32) {
    let scale = u64::from(scale_factor_milli.max(1));
    (
        ((u64::from(geometry.logical_width) * scale + 500) / 1000) as u32,
        ((u64::from(geometry.logical_height) * scale + 500) / 1000) as u32,
    )
}

/// Preserve a valid secondary-monitor position. Clamp only when the title bar
/// is inaccessible, retaining the logical size across monitor/DPI transitions.
pub fn restore_accessible_geometry(
    saved: WindowGeometry,
    monitors: &[MonitorWorkArea],
) -> WindowGeometry {
    let Some(primary) = monitors.first().copied() else {
        return saved;
    };
    let title_y = saved.outer_y;
    let title_x = saved.outer_x;
    let title_reachable = monitors.iter().any(|monitor| {
        title_x >= monitor.x
            && title_x < monitor.x.saturating_add_unsigned(monitor.width)
            && title_y >= monitor.y
            && title_y < monitor.y.saturating_add_unsigned(monitor.height)
    });
    if title_reachable {
        return saved;
    }
    let max_x = primary
        .x
        .saturating_add_unsigned(primary.width.saturating_sub(80));
    let max_y = primary
        .y
        .saturating_add_unsigned(primary.height.saturating_sub(32));
    WindowGeometry {
        outer_x: saved.outer_x.clamp(primary.x, max_x),
        outer_y: saved.outer_y.clamp(primary.y, max_y),
        ..saved
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiError(pub String);
impl fmt::Display for UiError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(&self.0)
    }
}
impl std::error::Error for UiError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiAction {
    TabSelected(ViewerTab),
    OpenRequested,
    OpenDialogCompleted(Option<PathBuf>),
    SaveRequested,
    SaveAsRequested,
    SaveDialogCompleted(Option<PathBuf>),
    UnsavedDecisionSelected(UnsavedDecision),
    ExportRequested(ExportKind),
    ExportDialogCompleted {
        kind: ExportKind,
        path: Option<PathBuf>,
    },
    RecalculateRequested,
    CalculationSettingsCommitted,
    NumericEditChanged {
        field: NumericFieldId,
        text: String,
    },
    NumericEditCommitted {
        field: NumericFieldId,
        text: String,
    },
    NumericEditCancelled {
        field: NumericFieldId,
    },
    DatasetEdited,
    GridInspectionChanged,
    RegisteredQueryAdded,
    RegisteredQueriesCleared,
    RenderSettingsChanged,
    ViewTransformChanged,
    PanelChanged {
        panel: PanelId,
        visible: bool,
        width: u16,
    },
    WindowGeometryChanged(WindowGeometry),
    WindowScaleFactorChanged(u32),
    RestoreDefaultLayoutRequested,
    DatasetLoaded {
        request: RequestId,
        result: Result<(), UiError>,
    },
    DatasetSaved {
        request: RequestId,
        result: Result<(), UiError>,
    },
    ProjectionCalculated {
        request: RequestId,
        dataset_revision: Revision,
        settings_revision: Revision,
        result: Result<(), UiError>,
    },
    RegisteredQueriesRecalculated {
        request: RequestId,
        result: Result<(), UiError>,
    },
    ExportCompleted {
        request: RequestId,
        result: Result<(), UiError>,
    },
    EffectFailed {
        request: RequestId,
        error: UiError,
    },
}
impl UiAction {
    pub const fn kind(&self) -> UiActionKind {
        match self {
            Self::TabSelected(_) => UiActionKind::TabSelected,
            Self::OpenRequested => UiActionKind::OpenRequested,
            Self::OpenDialogCompleted(_) => UiActionKind::OpenDialogCompleted,
            Self::SaveRequested => UiActionKind::SaveRequested,
            Self::SaveAsRequested => UiActionKind::SaveAsRequested,
            Self::SaveDialogCompleted(_) => UiActionKind::SaveDialogCompleted,
            Self::UnsavedDecisionSelected(_) => UiActionKind::UnsavedDecisionSelected,
            Self::ExportRequested(_) => UiActionKind::ExportRequested,
            Self::ExportDialogCompleted { .. } => UiActionKind::ExportDialogCompleted,
            Self::RecalculateRequested => UiActionKind::RecalculateRequested,
            Self::CalculationSettingsCommitted => UiActionKind::CalculationSettingsCommitted,
            Self::NumericEditChanged { .. } => UiActionKind::NumericEditChanged,
            Self::NumericEditCommitted { .. } => UiActionKind::NumericEditCommitted,
            Self::NumericEditCancelled { .. } => UiActionKind::NumericEditCancelled,
            Self::DatasetEdited => UiActionKind::DatasetEdited,
            Self::GridInspectionChanged => UiActionKind::GridInspectionChanged,
            Self::RegisteredQueryAdded => UiActionKind::RegisteredQueryAdded,
            Self::RegisteredQueriesCleared => UiActionKind::RegisteredQueriesCleared,
            Self::RenderSettingsChanged => UiActionKind::RenderSettingsChanged,
            Self::ViewTransformChanged => UiActionKind::ViewTransformChanged,
            Self::PanelChanged { .. } => UiActionKind::PanelChanged,
            Self::WindowGeometryChanged(_) => UiActionKind::WindowGeometryChanged,
            Self::WindowScaleFactorChanged(_) => UiActionKind::WindowScaleFactorChanged,
            Self::RestoreDefaultLayoutRequested => UiActionKind::RestoreDefaultLayoutRequested,
            Self::DatasetLoaded { .. } => UiActionKind::DatasetLoaded,
            Self::DatasetSaved { .. } => UiActionKind::DatasetSaved,
            Self::ProjectionCalculated { .. } => UiActionKind::ProjectionCalculated,
            Self::RegisteredQueriesRecalculated { .. } => {
                UiActionKind::RegisteredQueriesRecalculated
            }
            Self::ExportCompleted { .. } => UiActionKind::ExportCompleted,
            Self::EffectFailed { .. } => UiActionKind::EffectFailed,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiEffect {
    ShowOpenDialog,
    ShowSaveDialog,
    ShowUnsavedChangesDialog,
    LoadDataset {
        request: RequestId,
        path: PathBuf,
    },
    SaveDataset {
        request: RequestId,
        path: PathBuf,
    },
    Export {
        request: RequestId,
        kind: ExportKind,
        path: PathBuf,
    },
    RecalculateProjection {
        request: RequestId,
        dataset_revision: Revision,
        settings_revision: Revision,
    },
    RecalculateRegisteredQueries {
        request: RequestId,
        dataset_revision: Revision,
        settings_revision: Revision,
    },
    RebuildPlotTexture {
        projection_revision: Revision,
        render_revision: Revision,
    },
    RebuildHitGeometry {
        projection_revision: Revision,
        transform_revision: Revision,
    },
    CopyToClipboard(String),
    PersistWindowLayout(WindowGeometry),
}
impl UiEffect {
    pub const fn kind(&self) -> UiEffectKind {
        match self {
            Self::ShowOpenDialog => UiEffectKind::ShowOpenDialog,
            Self::ShowSaveDialog => UiEffectKind::ShowSaveDialog,
            Self::ShowUnsavedChangesDialog => UiEffectKind::ShowUnsavedChangesDialog,
            Self::LoadDataset { .. } => UiEffectKind::LoadDataset,
            Self::SaveDataset { .. } => UiEffectKind::SaveDataset,
            Self::Export { .. } => UiEffectKind::Export,
            Self::RecalculateProjection { .. } => UiEffectKind::RecalculateProjection,
            Self::RecalculateRegisteredQueries { .. } => UiEffectKind::RecalculateRegisteredQueries,
            Self::RebuildPlotTexture { .. } => UiEffectKind::RebuildPlotTexture,
            Self::RebuildHitGeometry { .. } => UiEffectKind::RebuildHitGeometry,
            Self::CopyToClipboard(_) => UiEffectKind::CopyToClipboard,
            Self::PersistWindowLayout(_) => UiEffectKind::PersistWindowLayout,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentFreshness {
    Untitled,
    LoadedClean,
    Dirty,
    Loading,
    Saving,
    SaveFailed,
    OpenFailed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionFreshness {
    None,
    Stale,
    Calculating,
    Current,
    FailedWithPrevious,
    FailedWithoutProjection,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryFreshness {
    Empty,
    Stale,
    Recalculating,
    Current,
    PartiallyFailed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogState {
    Closed,
    Open,
    Save,
    UnsavedChanges,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ViewerRevisions {
    pub dataset: Revision,
    pub editor: Revision,
    pub interpolation_settings: Revision,
    pub calculation_settings: Revision,
    pub projection: Revision,
    pub render_settings: Revision,
    pub view_transform: Revision,
    pub texture: Revision,
    pub hit_geometry: Revision,
    pub registered_queries: Revision,
    pub window_layout: Revision,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingCalculation {
    request: RequestId,
    dataset_revision: Revision,
    settings_revision: Revision,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PanelState {
    pub visible: bool,
    pub width: u16,
}

#[derive(Clone, Debug)]
pub struct GuiContractState {
    pub active_tab: ViewerTab,
    pub document: DocumentFreshness,
    pub dialog: DialogState,
    pub projection: ProjectionFreshness,
    pub queries: QueryFreshness,
    pub revisions: ViewerRevisions,
    pub window: WindowGeometry,
    pub scale_factor_milli: u32,
    pub has_projection: bool,
    pub draft_matches_active: bool,
    pub panels: BTreeMap<PanelId, PanelState>,
    pending_load: Option<RequestId>,
    pending_save: Option<RequestId>,
    pending_projection: Option<PendingCalculation>,
    pending_queries: Option<RequestId>,
    pending_export: Option<RequestId>,
    previous_document: Option<DocumentFreshness>,
    request_counter: u64,
}
impl Default for GuiContractState {
    fn default() -> Self {
        let mut panels = BTreeMap::new();
        for (panel, width) in [
            (PanelId::Data, 320),
            (PanelId::Diagnostics, 320),
            (PanelId::GridSettings, 310),
            (PanelId::GridResults, 360),
            (PanelId::PlotSettings, 320),
        ] {
            panels.insert(
                panel,
                PanelState {
                    visible: true,
                    width,
                },
            );
        }
        Self {
            active_tab: ViewerTab::Data,
            document: DocumentFreshness::Untitled,
            dialog: DialogState::Closed,
            projection: ProjectionFreshness::None,
            queries: QueryFreshness::Empty,
            revisions: ViewerRevisions::default(),
            window: WindowGeometry::default(),
            scale_factor_milli: 1000,
            has_projection: false,
            draft_matches_active: true,
            panels,
            pending_load: None,
            pending_save: None,
            pending_projection: None,
            pending_queries: None,
            pending_export: None,
            previous_document: None,
            request_counter: 0,
        }
    }
}
impl GuiContractState {
    fn next_request(&mut self) -> RequestId {
        self.request_counter = self.request_counter.saturating_add(1);
        RequestId(self.request_counter)
    }
    pub const fn is_dirty(&self) -> bool {
        matches!(self.document, DocumentFreshness::Dirty)
    }
    pub fn invariant_failures(&self) -> Vec<InvariantFailure> {
        let mut failures = Vec::new();
        if !self.is_dirty() && !self.draft_matches_active {
            failures.push(InvariantFailure::CleanDocumentHasDraftEdits);
        }
        if self.projection == ProjectionFreshness::Current
            && (!self.has_projection || self.pending_projection.is_some())
        {
            failures.push(InvariantFailure::CurrentProjectionInvalid);
        }
        if self.queries == QueryFreshness::Current && self.pending_queries.is_some() {
            failures.push(InvariantFailure::CurrentQueriesStillPending);
        }
        if self.dialog != DialogState::Closed
            && (self.pending_load.is_some() || self.pending_save.is_some())
        {
            failures.push(InvariantFailure::ConflictingDialogAndIo);
        }
        if self
            .panels
            .values()
            .any(|panel| panel.visible && panel.width == 0)
        {
            failures.push(InvariantFailure::VisiblePanelHasZeroWidth);
        }
        failures
    }
    pub fn state_report(&self) -> String {
        let mut report = format!(
            "Window\n  logical size: {} x {}\n  scale factor: {:.3}\n\nDocument\n  state: {:?}\n  dataset revision: {}\n\nCalculation\n  state: {:?}\n  settings revision: {}\n\nGrid interpolation\n  state: {:?}\n  query revision: {}\n",
            self.window.logical_width,
            self.window.logical_height,
            f64::from(self.scale_factor_milli) / 1000.0,
            self.document,
            self.revisions.dataset.0,
            self.projection,
            self.revisions.calculation_settings.0,
            self.queries,
            self.revisions.registered_queries.0,
        );
        for failure in self.invariant_failures() {
            report.push_str(&format!("Invariant failure: {failure:?}\n"));
        }
        report
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvariantFailure {
    CleanDocumentHasDraftEdits,
    CurrentProjectionInvalid,
    CurrentQueriesStillPending,
    ConflictingDialogAndIo,
    VisiblePanelHasZeroWidth,
}

/// Pure deterministic reducer. Effects are executed by the application layer.
pub fn update(state: &mut GuiContractState, action: UiAction) -> Vec<UiEffect> {
    let mut effects = Vec::new();
    match action {
        UiAction::TabSelected(tab) => state.active_tab = tab,
        UiAction::OpenRequested => {
            if state.dialog != DialogState::Closed {
                return effects;
            }
            if state.is_dirty() {
                state.dialog = DialogState::UnsavedChanges;
                effects.push(UiEffect::ShowUnsavedChangesDialog);
            } else {
                state.dialog = DialogState::Open;
                effects.push(UiEffect::ShowOpenDialog);
            }
        }
        UiAction::UnsavedDecisionSelected(UnsavedDecision::Cancel) => {
            state.dialog = DialogState::Closed
        }
        UiAction::UnsavedDecisionSelected(UnsavedDecision::Discard)
            if state.dialog == DialogState::UnsavedChanges =>
        {
            state.dialog = DialogState::Open;
            effects.push(UiEffect::ShowOpenDialog);
        }
        UiAction::UnsavedDecisionSelected(UnsavedDecision::Save)
            if state.dialog == DialogState::UnsavedChanges =>
        {
            state.dialog = DialogState::Save;
            effects.push(UiEffect::ShowSaveDialog);
        }
        UiAction::OpenDialogCompleted(path) => {
            state.dialog = DialogState::Closed;
            if let Some(path) = path {
                let request = state.next_request();
                state.pending_load = Some(request);
                state.previous_document = Some(state.document);
                state.document = DocumentFreshness::Loading;
                effects.push(UiEffect::LoadDataset { request, path });
            }
        }
        UiAction::SaveRequested | UiAction::SaveAsRequested => {
            if state.dialog == DialogState::Closed {
                state.dialog = DialogState::Save;
                effects.push(UiEffect::ShowSaveDialog);
            }
        }
        UiAction::SaveDialogCompleted(path) => {
            if state.dialog != DialogState::Save {
                return effects;
            }
            state.dialog = DialogState::Closed;
            if let Some(path) = path {
                let request = state.next_request();
                state.pending_save = Some(request);
                state.document = DocumentFreshness::Saving;
                effects.push(UiEffect::SaveDataset { request, path });
            }
        }
        UiAction::ExportRequested(_) => {
            if state.dialog == DialogState::Closed && state.has_projection {
                state.dialog = DialogState::Save;
                effects.push(UiEffect::ShowSaveDialog);
            }
        }
        UiAction::ExportDialogCompleted { kind, path } => {
            state.dialog = DialogState::Closed;
            if let Some(path) = path {
                let request = state.next_request();
                state.pending_export = Some(request);
                effects.push(UiEffect::Export {
                    request,
                    kind,
                    path,
                });
            }
        }
        UiAction::RecalculateRequested => schedule_projection(state, &mut effects),
        UiAction::CalculationSettingsCommitted => {
            state.revisions.interpolation_settings = state.revisions.interpolation_settings.next();
            state.revisions.calculation_settings = state.revisions.calculation_settings.next();
            if state.queries != QueryFreshness::Empty {
                state.queries = QueryFreshness::Recalculating;
                let request = state.next_request();
                state.pending_queries = Some(request);
                effects.push(UiEffect::RecalculateRegisteredQueries {
                    request,
                    dataset_revision: state.revisions.dataset,
                    settings_revision: state.revisions.interpolation_settings,
                });
            }
            schedule_projection(state, &mut effects);
        }
        UiAction::NumericEditChanged { .. } | UiAction::NumericEditCancelled { .. } => {}
        UiAction::NumericEditCommitted { .. } => {
            state.revisions.calculation_settings = state.revisions.calculation_settings.next();
            schedule_projection(state, &mut effects);
        }
        UiAction::DatasetEdited => {
            state.document = DocumentFreshness::Dirty;
            state.draft_matches_active = false;
            state.revisions.editor = state.revisions.editor.next();
            state.projection = if state.has_projection {
                ProjectionFreshness::Stale
            } else {
                ProjectionFreshness::None
            };
        }
        UiAction::GridInspectionChanged => {}
        UiAction::RegisteredQueryAdded => {
            state.queries = QueryFreshness::Stale;
            state.revisions.registered_queries = state.revisions.registered_queries.next();
        }
        UiAction::RegisteredQueriesCleared => {
            state.queries = QueryFreshness::Empty;
            state.revisions.registered_queries = state.revisions.registered_queries.next();
        }
        UiAction::RenderSettingsChanged => {
            state.revisions.render_settings = state.revisions.render_settings.next();
            if state.has_projection {
                effects.push(UiEffect::RebuildPlotTexture {
                    projection_revision: state.revisions.projection,
                    render_revision: state.revisions.render_settings,
                });
            }
        }
        UiAction::ViewTransformChanged => {
            state.revisions.view_transform = state.revisions.view_transform.next();
            if state.has_projection {
                effects.push(UiEffect::RebuildHitGeometry {
                    projection_revision: state.revisions.projection,
                    transform_revision: state.revisions.view_transform,
                });
            }
        }
        UiAction::PanelChanged {
            panel,
            visible,
            width,
        } => {
            state.panels.insert(
                panel,
                PanelState {
                    visible,
                    width: width.max(120),
                },
            );
            state.revisions.window_layout = state.revisions.window_layout.next();
        }
        UiAction::WindowGeometryChanged(geometry) => {
            state.window = geometry;
            state.revisions.window_layout = state.revisions.window_layout.next();
            effects.push(UiEffect::PersistWindowLayout(geometry));
        }
        UiAction::WindowScaleFactorChanged(factor) => state.scale_factor_milli = factor.max(1),
        UiAction::RestoreDefaultLayoutRequested => {
            state.window = WindowGeometry::default();
            state.revisions.window_layout = state.revisions.window_layout.next();
            effects.push(UiEffect::PersistWindowLayout(state.window));
        }
        UiAction::DatasetLoaded { request, result } => {
            if state.pending_load != Some(request) {
                return effects;
            }
            state.pending_load = None;
            match result {
                Ok(()) => {
                    state.document = DocumentFreshness::LoadedClean;
                    state.previous_document = None;
                    state.draft_matches_active = true;
                    state.revisions.dataset = state.revisions.dataset.next();
                    state.revisions.editor = state.revisions.editor.next();
                    state.has_projection = false;
                    schedule_projection(state, &mut effects);
                }
                Err(_) => {
                    state.document = state
                        .previous_document
                        .take()
                        .unwrap_or(DocumentFreshness::OpenFailed)
                }
            }
        }
        UiAction::DatasetSaved { request, result } => {
            if state.pending_save != Some(request) {
                return effects;
            }
            state.pending_save = None;
            match result {
                Ok(()) => {
                    state.document = DocumentFreshness::LoadedClean;
                    state.previous_document = None;
                    state.draft_matches_active = true;
                    state.revisions.dataset = state.revisions.dataset.next();
                }
                Err(_) => state.document = DocumentFreshness::SaveFailed,
            }
        }
        UiAction::ProjectionCalculated {
            request,
            dataset_revision,
            settings_revision,
            result,
        } => {
            let expected = PendingCalculation {
                request,
                dataset_revision,
                settings_revision,
            };
            if state.pending_projection != Some(expected)
                || state.revisions.dataset != dataset_revision
                || state.revisions.calculation_settings != settings_revision
            {
                return effects;
            }
            state.pending_projection = None;
            match result {
                Ok(()) => {
                    state.has_projection = true;
                    state.projection = ProjectionFreshness::Current;
                    state.revisions.projection = state.revisions.projection.next();
                    effects.push(UiEffect::RebuildPlotTexture {
                        projection_revision: state.revisions.projection,
                        render_revision: state.revisions.render_settings,
                    });
                    effects.push(UiEffect::RebuildHitGeometry {
                        projection_revision: state.revisions.projection,
                        transform_revision: state.revisions.view_transform,
                    });
                }
                Err(_) => {
                    state.projection = if state.has_projection {
                        ProjectionFreshness::FailedWithPrevious
                    } else {
                        ProjectionFreshness::FailedWithoutProjection
                    }
                }
            }
        }
        UiAction::RegisteredQueriesRecalculated { request, result } => {
            if state.pending_queries != Some(request) {
                return effects;
            }
            state.pending_queries = None;
            state.queries = if result.is_ok() {
                QueryFreshness::Current
            } else {
                QueryFreshness::PartiallyFailed
            };
        }
        UiAction::ExportCompleted { request, .. } if state.pending_export == Some(request) => {
            state.pending_export = None
        }
        UiAction::ExportCompleted { .. } | UiAction::EffectFailed { .. } => {}
        UiAction::UnsavedDecisionSelected(_) => {}
    }
    debug_assert!(state.invariant_failures().is_empty());
    effects
}
fn schedule_projection(state: &mut GuiContractState, effects: &mut Vec<UiEffect>) {
    let request = state.next_request();
    let pending = PendingCalculation {
        request,
        dataset_revision: state.revisions.dataset,
        settings_revision: state.revisions.calculation_settings,
    };
    state.pending_projection = Some(pending);
    state.projection = ProjectionFreshness::Calculating;
    effects.push(UiEffect::RecalculateProjection {
        request,
        dataset_revision: pending.dataset_revision,
        settings_revision: pending.settings_revision,
    });
}

pub trait FileDialogService {
    fn open_file(&mut self, title: &str) -> Result<Option<PathBuf>, UiError>;
    fn save_file(&mut self, title: &str) -> Result<Option<PathBuf>, UiError>;
}
pub trait FileSystemService {
    fn read(&mut self, path: &Path) -> Result<Vec<u8>, UiError>;
    fn write(&mut self, path: &Path, data: &[u8]) -> Result<(), UiError>;
}
pub trait ClipboardService {
    fn set_text(&mut self, text: String) -> Result<(), UiError>;
}
pub trait CalculationService {
    fn submit_projection(
        &mut self,
        request: RequestId,
        dataset_revision: Revision,
        settings_revision: Revision,
    ) -> Result<(), UiError>;
    fn submit_query_batch(
        &mut self,
        request: RequestId,
        dataset_revision: Revision,
        settings_revision: Revision,
    ) -> Result<(), UiError>;
}
pub trait WindowService {
    fn persist_layout(&mut self, layout: WindowGeometry) -> Result<(), UiError>;
}

#[derive(Clone, Debug)]
pub struct UiElementContract {
    pub id: UiElementId,
    pub label: &'static str,
    pub kind: UiElementKind,
    pub location: UiLocation,
    pub purpose: &'static str,
    pub visible_when: &'static str,
    pub enabled_when: &'static str,
    pub emitted_actions: Vec<UiActionKind>,
    pub expected_effects: Vec<UiEffectKind>,
    pub invalidation: Vec<InvalidationClass>,
    pub standard_behavior: StandardGuiBehavior,
    pub layout: LayoutContract,
    pub migration: ContractMigrationStatus,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractMigrationStatus {
    Declared,
    PartiallyMigrated,
    FullyContractDriven,
    BehaviorallyTested,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PublicStateId {
    ActiveTab,
    DocumentFreshness,
    DialogState,
    ProjectionFreshness,
    QueryFreshness,
    DatasetRevision,
    EditorRevision,
    InterpolationSettingsRevision,
    CalculationSettingsRevision,
    ProjectionRevision,
    RenderSettingsRevision,
    ViewTransformRevision,
    TextureRevision,
    HitGeometryRevision,
    RegisteredQueryRevision,
    WindowLayoutRevision,
    WindowLogicalGeometry,
    WindowScaleFactor,
    PanelVisibility,
    PanelWidth,
}

#[derive(Clone, Copy, Debug)]
pub struct PublicStateContract {
    pub id: PublicStateId,
    pub owner: UiElementId,
    pub allowed_values: &'static str,
    pub default_value: &'static str,
    pub meaning: &'static str,
    pub rationale: &'static str,
    pub hazard: &'static str,
    pub entered_by: &'static [UiActionKind],
    pub exited_by: &'static [UiActionKind],
}

const ENTER_TAB: &[UiActionKind] = &[UiActionKind::TabSelected];
const ENTER_DOCUMENT: &[UiActionKind] = &[
    UiActionKind::DatasetEdited,
    UiActionKind::DatasetLoaded,
    UiActionKind::DatasetSaved,
    UiActionKind::OpenRequested,
];
const ENTER_PROJECTION: &[UiActionKind] = &[
    UiActionKind::RecalculateRequested,
    UiActionKind::CalculationSettingsCommitted,
    UiActionKind::ProjectionCalculated,
];
const ENTER_WINDOW: &[UiActionKind] = &[
    UiActionKind::WindowGeometryChanged,
    UiActionKind::WindowScaleFactorChanged,
    UiActionKind::PanelChanged,
];
const ENTER_QUERIES: &[UiActionKind] = &[
    UiActionKind::RegisteredQueryAdded,
    UiActionKind::RegisteredQueriesCleared,
    UiActionKind::RegisteredQueriesRecalculated,
];

pub const PUBLIC_STATES: &[PublicStateContract] = &[
    PublicStateContract {
        id: PublicStateId::ActiveTab,
        owner: UiElementId::TabBar,
        allowed_values: "Data, Diagnostics, GridInspection, Plot",
        default_value: "Data",
        meaning: "Visible top-level workflow.",
        rationale: "Separates data editing, diagnostics, grid inspection, and plotting.",
        hazard: "A stale tab can hide controls needed to recover an error.",
        entered_by: ENTER_TAB,
        exited_by: ENTER_TAB,
    },
    PublicStateContract {
        id: PublicStateId::DocumentFreshness,
        owner: UiElementId::GlobalToolbar,
        allowed_values: "Untitled, LoadedClean, Dirty, Loading, Saving, SaveFailed, OpenFailed",
        default_value: "Untitled",
        meaning: "Whether replacing or closing can discard changes.",
        rationale: "Classified point and declaration edits must be preserved.",
        hazard: "A false clean state can silently discard edits.",
        entered_by: ENTER_DOCUMENT,
        exited_by: ENTER_DOCUMENT,
    },
    PublicStateContract {
        id: PublicStateId::DialogState,
        owner: UiElementId::GlobalToolbar,
        allowed_values: "Closed, Open, Save, UnsavedChanges",
        default_value: "Closed",
        meaning: "One active native file decision.",
        rationale: "Prevents concurrent conflicting native dialogs.",
        hazard: "Two dialogs can target the wrong path.",
        entered_by: ENTER_DOCUMENT,
        exited_by: ENTER_DOCUMENT,
    },
    PublicStateContract {
        id: PublicStateId::ProjectionFreshness,
        owner: UiElementId::PlotCanvas,
        allowed_values: "None, Stale, Calculating, Current, FailedWithPrevious, FailedWithoutProjection",
        default_value: "None",
        meaning: "Whether the plot matches numerical input revisions.",
        rationale: "Exports and selection require honest numerical provenance.",
        hazard: "Old geometry can be shown as current.",
        entered_by: ENTER_PROJECTION,
        exited_by: ENTER_PROJECTION,
    },
    PublicStateContract {
        id: PublicStateId::QueryFreshness,
        owner: UiElementId::InterpolationResultsTable,
        allowed_values: "Empty, Stale, Recalculating, Current, PartiallyFailed",
        default_value: "Empty",
        meaning: "Whether registered queries match field/settings.",
        rationale: "The results pane supports Excel-copyable numerical inspection.",
        hazard: "Copied values can disagree with the selected field.",
        entered_by: ENTER_QUERIES,
        exited_by: ENTER_QUERIES,
    },
    PublicStateContract {
        id: PublicStateId::DatasetRevision,
        owner: UiElementId::DataPanel,
        allowed_values: "monotonic revision",
        default_value: "0",
        meaning: "Dataset input provenance.",
        rationale: "Async calculations must identify the dataset they used.",
        hazard: "A stale worker can replace a newer document.",
        entered_by: ENTER_DOCUMENT,
        exited_by: ENTER_DOCUMENT,
    },
    PublicStateContract {
        id: PublicStateId::EditorRevision,
        owner: UiElementId::DataPanel,
        allowed_values: "monotonic revision",
        default_value: "0",
        meaning: "Draft-editor provenance.",
        rationale: "Draft and active data are intentionally separated.",
        hazard: "Dirty markers can fail to represent pending edits.",
        entered_by: ENTER_DOCUMENT,
        exited_by: ENTER_DOCUMENT,
    },
    PublicStateContract {
        id: PublicStateId::InterpolationSettingsRevision,
        owner: UiElementId::PlotInterpolation,
        allowed_values: "monotonic revision",
        default_value: "0",
        meaning: "Source interpolation provenance.",
        rationale: "Plot and inspection share one interpolation configuration.",
        hazard: "Query rows can use another interpolation method.",
        entered_by: ENTER_PROJECTION,
        exited_by: ENTER_PROJECTION,
    },
    PublicStateContract {
        id: PublicStateId::CalculationSettingsRevision,
        owner: UiElementId::PlotSettings,
        allowed_values: "monotonic revision",
        default_value: "0",
        meaning: "Projection calculation provenance.",
        rationale: "Contour levels and sampling affect derived topology.",
        hazard: "A stale calculation can be accepted.",
        entered_by: ENTER_PROJECTION,
        exited_by: ENTER_PROJECTION,
    },
    PublicStateContract {
        id: PublicStateId::ProjectionRevision,
        owner: UiElementId::PlotCanvas,
        allowed_values: "monotonic revision",
        default_value: "0",
        meaning: "Current accepted projection identity.",
        rationale: "Render and hit geometry depend on it.",
        hazard: "Selection can address a different projection.",
        entered_by: ENTER_PROJECTION,
        exited_by: ENTER_PROJECTION,
    },
    PublicStateContract {
        id: PublicStateId::RenderSettingsRevision,
        owner: UiElementId::PlotSettings,
        allowed_values: "monotonic revision",
        default_value: "0",
        meaning: "Render-only provenance.",
        rationale: "Legend and labels should not rerun numerical tracing.",
        hazard: "Texture can ignore visible layer choices.",
        entered_by: ENTER_PROJECTION,
        exited_by: ENTER_PROJECTION,
    },
    PublicStateContract {
        id: PublicStateId::ViewTransformRevision,
        owner: UiElementId::PlotCanvas,
        allowed_values: "monotonic revision",
        default_value: "0",
        meaning: "Pan/zoom provenance.",
        rationale: "Hit geometry is screen/view dependent.",
        hazard: "Clicks select the wrong feature.",
        entered_by: ENTER_PROJECTION,
        exited_by: ENTER_PROJECTION,
    },
    PublicStateContract {
        id: PublicStateId::TextureRevision,
        owner: UiElementId::PlotCanvas,
        allowed_values: "monotonic revision",
        default_value: "0",
        meaning: "Bitmap texture provenance.",
        rationale: "Texture is derived from projection and render settings.",
        hazard: "A prior document remains visible.",
        entered_by: ENTER_PROJECTION,
        exited_by: ENTER_PROJECTION,
    },
    PublicStateContract {
        id: PublicStateId::HitGeometryRevision,
        owner: UiElementId::PlotCanvas,
        allowed_values: "monotonic revision",
        default_value: "0",
        meaning: "Hit-test provenance.",
        rationale: "Hover and click must use current geometry.",
        hazard: "Selection describes stale lines or nodes.",
        entered_by: ENTER_PROJECTION,
        exited_by: ENTER_PROJECTION,
    },
    PublicStateContract {
        id: PublicStateId::RegisteredQueryRevision,
        owner: UiElementId::InterpolationResultsTable,
        allowed_values: "monotonic revision",
        default_value: "0",
        meaning: "Registered query collection identity.",
        rationale: "Settings changes recalculate existing clicks without duplicates.",
        hazard: "Rows can silently retain obsolete settings.",
        entered_by: ENTER_QUERIES,
        exited_by: ENTER_QUERIES,
    },
    PublicStateContract {
        id: PublicStateId::WindowLayoutRevision,
        owner: UiElementId::MainWindow,
        allowed_values: "monotonic revision",
        default_value: "0",
        meaning: "User layout provenance.",
        rationale: "Panel changes and explicit restoration are persistent choices.",
        hazard: "A stale layout can make controls inaccessible.",
        entered_by: ENTER_WINDOW,
        exited_by: ENTER_WINDOW,
    },
    PublicStateContract {
        id: PublicStateId::WindowLogicalGeometry,
        owner: UiElementId::MainWindow,
        allowed_values: "logical width, height, outer position, maximized",
        default_value: "1280 x 800",
        meaning: "DPI-independent native window geometry.",
        rationale: "Logical size remains sensible across monitors.",
        hazard: "Double scaling grows or shrinks the window.",
        entered_by: ENTER_WINDOW,
        exited_by: ENTER_WINDOW,
    },
    PublicStateContract {
        id: PublicStateId::WindowScaleFactor,
        owner: UiElementId::MainWindow,
        allowed_values: "positive milli-scale factor",
        default_value: "1000",
        meaning: "Physical rendering scale.",
        rationale: "Physical size is logical size times scale only once.",
        hazard: "Monitor transitions can repeatedly resize the window.",
        entered_by: ENTER_WINDOW,
        exited_by: ENTER_WINDOW,
    },
    PublicStateContract {
        id: PublicStateId::PanelVisibility,
        owner: UiElementId::GridResults,
        allowed_values: "visible, hidden",
        default_value: "visible",
        meaning: "Whether a panel is reachable.",
        rationale: "Side panes may collapse at constrained sizes.",
        hazard: "Hidden controls can retain focus.",
        entered_by: ENTER_WINDOW,
        exited_by: ENTER_WINDOW,
    },
    PublicStateContract {
        id: PublicStateId::PanelWidth,
        owner: UiElementId::GridResults,
        allowed_values: "bounded logical points",
        default_value: "configured",
        meaning: "Resizable panel width.",
        rationale: "Canvas and results share finite window space.",
        hazard: "A zero-sized visible panel is unreachable.",
        entered_by: ENTER_WINDOW,
        exited_by: ENTER_WINDOW,
    },
];

/// The authoritative typed registry. Every enum ID appears exactly once.
pub fn ui_element_registry() -> Vec<UiElementContract> {
    UiElementId::ALL
        .iter()
        .copied()
        .map(element_contract)
        .collect()
}
pub fn element_contract(id: UiElementId) -> UiElementContract {
    let (label, kind, location, purpose, behavior, layout) = match id {
        UiElementId::MainWindow => (
            "Ternary contours",
            UiElementKind::Panel,
            UiLocation::MainWindow,
            "Native viewer window",
            StandardGuiBehavior::NativeWindowMove,
            CANVAS,
        ),
        UiElementId::GlobalToolbar | UiElementId::TabBar => (
            "Viewer navigation",
            UiElementKind::Toolbar,
            UiLocation::Toolbar,
            "Persistent document actions and navigation",
            StandardGuiBehavior::ImmediateAction,
            TOOLBAR,
        ),
        UiElementId::Status => (
            "Status",
            UiElementKind::Status,
            UiLocation::Status,
            "Visible operation feedback",
            StandardGuiBehavior::ImmediateAction,
            TOOLBAR,
        ),
        UiElementId::TabData
        | UiElementId::TabDiagnostics
        | UiElementId::TabGridInspection
        | UiElementId::TabPlot => (
            id.name(),
            UiElementKind::Tab,
            UiLocation::TabBar,
            "Select top-level workflow",
            StandardGuiBehavior::ImmediateSelectionChange,
            TOOLBAR,
        ),
        UiElementId::Open | UiElementId::Reload => (
            id.name(),
            UiElementKind::Button,
            UiLocation::Toolbar,
            "Transactionally replace document",
            StandardGuiBehavior::NativeOpenDialog,
            TOOLBAR,
        ),
        UiElementId::Save | UiElementId::SaveAs => (
            id.name(),
            UiElementKind::Button,
            UiLocation::Toolbar,
            "Persist active TCT document",
            StandardGuiBehavior::NativeSaveDialog,
            TOOLBAR,
        ),
        UiElementId::ExportSvg | UiElementId::ExportPng | UiElementId::ExportLinesCsv => (
            id.name(),
            UiElementKind::Button,
            UiLocation::Toolbar,
            "Select native export destination",
            StandardGuiBehavior::NativeSaveDialog,
            TOOLBAR,
        ),
        UiElementId::GridCanvas | UiElementId::PlotCanvas => (
            id.name(),
            UiElementKind::Canvas,
            if id == UiElementId::GridCanvas {
                UiLocation::GridCanvas
            } else {
                UiLocation::PlotCanvas
            },
            "Interactive ternary canvas",
            StandardGuiBehavior::AppendCanvasSelection,
            CANVAS,
        ),
        UiElementId::GridResults | UiElementId::InterpolationResultsTable => (
            id.name(),
            if id == UiElementId::InterpolationResultsTable {
                UiElementKind::Table
            } else {
                UiElementKind::Panel
            },
            UiLocation::GridResults,
            "Scroll-safe interpolation results",
            StandardGuiBehavior::ResizePanel,
            RESULTS,
        ),
        UiElementId::OpenDialog
        | UiElementId::SaveDialog
        | UiElementId::ExportDialog
        | UiElementId::UnsavedChangesDialog => (
            id.name(),
            UiElementKind::Dialog,
            UiLocation::NativeDialog,
            "Native dialog or confirmation",
            StandardGuiBehavior::ConfirmationBeforeDestructiveAction,
            DIALOG,
        ),
        id if id.name().starts_with("Plot") => (
            id.name(),
            UiElementKind::ComboBox,
            UiLocation::PlotSettings,
            "Plot configuration control",
            StandardGuiBehavior::CommitOnEnterOrFocusLoss,
            FORM,
        ),
        id if id.name().starts_with("Grid") || id.name().starts_with("Interpolation") => (
            id.name(),
            UiElementKind::ComboBox,
            UiLocation::GridSettings,
            "Grid inspection control",
            StandardGuiBehavior::ImmediateSelectionChange,
            FORM,
        ),
        id if id.name().starts_with("Diagnostics") => (
            id.name(),
            UiElementKind::Panel,
            UiLocation::Diagnostics,
            "Development diagnostics",
            StandardGuiBehavior::ImmediateAction,
            FORM,
        ),
        _ => (
            id.name(),
            UiElementKind::Panel,
            UiLocation::Data,
            "Dataset authoring control",
            StandardGuiBehavior::CommitOnEnterOrFocusLoss,
            FORM,
        ),
    };
    let (emitted_actions, expected_effects, invalidation) = contract_effects(id);
    UiElementContract {
        id,
        label,
        kind,
        location,
        purpose,
        visible_when: "the owning tab or panel is visible",
        enabled_when: "the documented action preconditions are satisfied",
        emitted_actions,
        expected_effects,
        invalidation,
        standard_behavior: behavior,
        layout,
        migration: migration_status(id),
    }
}
fn contract_effects(
    id: UiElementId,
) -> (Vec<UiActionKind>, Vec<UiEffectKind>, Vec<InvalidationClass>) {
    match id {
        UiElementId::Open | UiElementId::Reload => (
            vec![UiActionKind::OpenRequested],
            vec![UiEffectKind::ShowOpenDialog],
            vec![InvalidationClass::DatasetAndProjection],
        ),
        UiElementId::Save => (
            vec![UiActionKind::SaveRequested],
            vec![UiEffectKind::ShowSaveDialog, UiEffectKind::SaveDataset],
            vec![InvalidationClass::None],
        ),
        UiElementId::SaveAs => (
            vec![UiActionKind::SaveAsRequested],
            vec![UiEffectKind::ShowSaveDialog, UiEffectKind::SaveDataset],
            vec![InvalidationClass::None],
        ),
        UiElementId::ExportSvg | UiElementId::ExportPng | UiElementId::ExportLinesCsv => (
            vec![UiActionKind::ExportRequested],
            vec![UiEffectKind::ShowSaveDialog, UiEffectKind::Export],
            vec![InvalidationClass::None],
        ),
        UiElementId::Recalculate => (
            vec![UiActionKind::RecalculateRequested],
            vec![UiEffectKind::RecalculateProjection],
            vec![InvalidationClass::ProjectionCalculation],
        ),
        UiElementId::Fit
        | UiElementId::ResetView
        | UiElementId::GridCanvas
        | UiElementId::PlotCanvas => (
            vec![UiActionKind::ViewTransformChanged],
            vec![UiEffectKind::RebuildHitGeometry],
            vec![
                InvalidationClass::ViewOnly,
                InvalidationClass::HitGeometryOnly,
            ],
        ),
        UiElementId::PlotInterpolation | UiElementId::GridInterpolation => (
            vec![UiActionKind::CalculationSettingsCommitted],
            vec![
                UiEffectKind::RecalculateRegisteredQueries,
                UiEffectKind::RecalculateProjection,
            ],
            vec![
                InvalidationClass::RegisteredQueryBatch,
                InvalidationClass::ProjectionCalculation,
            ],
        ),
        UiElementId::PlotSampling | UiElementId::PlotLevels | UiElementId::DataResolution => (
            vec![
                UiActionKind::NumericEditChanged,
                UiActionKind::NumericEditCommitted,
                UiActionKind::NumericEditCancelled,
            ],
            vec![UiEffectKind::RecalculateProjection],
            vec![InvalidationClass::ProjectionCalculation],
        ),
        UiElementId::PlotLegend
        | UiElementId::PlotAxisLabels
        | UiElementId::PlotPathVertices
        | UiElementId::PlotPathMode
        | UiElementId::GridStateFilter
        | UiElementId::GridLabelMode => (
            vec![UiActionKind::RenderSettingsChanged],
            vec![UiEffectKind::RebuildPlotTexture],
            vec![InvalidationClass::RenderOnly],
        ),
        UiElementId::DataDeclarations
        | UiElementId::DataComponents
        | UiElementId::DataPhases
        | UiElementId::DataPhaseMove
        | UiElementId::DataPhaseRemove
        | UiElementId::DataProperties
        | UiElementId::DataPropertyMove
        | UiElementId::DataPropertyRemove
        | UiElementId::DataGridList
        | UiElementId::DataGridEditor
        | UiElementId::DataPastePreview
        | UiElementId::DataPasteApply
        | UiElementId::DataUndo
        | UiElementId::DataRedo
        | UiElementId::GridPointEditor
        | UiElementId::GridPointApply => (
            vec![UiActionKind::DatasetEdited],
            Vec::new(),
            vec![
                InvalidationClass::DatasetValidation,
                InvalidationClass::DatasetAndProjection,
            ],
        ),
        UiElementId::InterpolationResultsClear => (
            vec![UiActionKind::RegisteredQueriesCleared],
            Vec::new(),
            vec![InvalidationClass::None],
        ),
        UiElementId::GridMode
        | UiElementId::GridSelector
        | UiElementId::GridPhaseSelector
        | UiElementId::GridPropertySelector => (
            vec![UiActionKind::GridInspectionChanged],
            Vec::new(),
            vec![InvalidationClass::LocalInterpolation],
        ),
        UiElementId::TabData
        | UiElementId::TabDiagnostics
        | UiElementId::TabGridInspection
        | UiElementId::TabPlot => (
            vec![UiActionKind::TabSelected],
            Vec::new(),
            vec![InvalidationClass::None],
        ),
        _ => (Vec::new(), Vec::new(), vec![InvalidationClass::None]),
    }
}
fn migration_status(id: UiElementId) -> ContractMigrationStatus {
    match id {
        UiElementId::Open
        | UiElementId::Save
        | UiElementId::SaveAs
        | UiElementId::ExportSvg
        | UiElementId::ExportPng
        | UiElementId::ExportLinesCsv
        | UiElementId::TabData
        | UiElementId::TabDiagnostics
        | UiElementId::TabGridInspection
        | UiElementId::TabPlot
        | UiElementId::OpenDialog
        | UiElementId::SaveDialog
        | UiElementId::ExportDialog
        | UiElementId::UnsavedChangesDialog => ContractMigrationStatus::FullyContractDriven,
        _ => ContractMigrationStatus::Declared,
    }
}

#[derive(Clone, Debug)]
pub struct EventTraceEntry {
    pub element: Option<UiElementId>,
    pub action: UiActionKind,
    pub effects: Vec<UiEffectKind>,
    pub dataset_before: Revision,
    pub dataset_after: Revision,
    pub settings_before: Revision,
    pub settings_after: Revision,
    pub invariant_failures: Vec<InvariantFailure>,
}
#[derive(Clone, Debug, Default)]
pub struct EventTrace {
    entries: Vec<EventTraceEntry>,
}
impl EventTrace {
    pub fn entries(&self) -> &[EventTraceEntry] {
        &self.entries
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    pub fn as_text(&self) -> String {
        self.entries.iter().enumerate().map(|(index, entry)| format!(
            "{}: {:?} {:?}; effects={:?}; dataset {} to {}; settings {} to {}; invariant failures={:?}",
            index + 1, entry.element, entry.action, entry.effects, entry.dataset_before.0,
            entry.dataset_after.0, entry.settings_before.0, entry.settings_after.0, entry.invariant_failures,
        )).collect::<Vec<_>>().join("\n")
    }
    fn push(
        &mut self,
        element: Option<UiElementId>,
        before: &GuiContractState,
        action: &UiAction,
        effects: &[UiEffect],
        after: &GuiContractState,
    ) {
        self.entries.push(EventTraceEntry {
            element,
            action: action.kind(),
            effects: effects.iter().map(UiEffect::kind).collect(),
            dataset_before: before.revisions.dataset,
            dataset_after: after.revisions.dataset,
            settings_before: before.revisions.calculation_settings,
            settings_after: after.revisions.calculation_settings,
            invariant_failures: after.invariant_failures(),
        });
        if self.entries.len() > 200 {
            self.entries.remove(0);
        }
    }
}
pub fn dispatch(
    state: &mut GuiContractState,
    trace: &mut EventTrace,
    element: Option<UiElementId>,
    action: UiAction,
) -> Vec<UiEffect> {
    let before = state.clone();
    let effects = update(state, action.clone());
    trace.push(element, &before, &action, &effects, state);
    effects
}

/// Generated files come from the same registry and reducer used at runtime.
pub fn generated_documentation() -> BTreeMap<&'static str, String> {
    let mut files = BTreeMap::new();
    files.insert("gui-elements.md", gui_elements_markdown());
    files.insert("gui-elements.json", gui_elements_json());
    files.insert("public-states.md", public_states_markdown());
    files.insert("public-states.json", public_states_json());
    files.insert("state-invariants.md", state_invariants_markdown());
    files.insert("state-hazards.md", state_hazards_markdown());
    files.insert("state-dependencies.md", state_dependencies_markdown());
    files.insert("transition-matrix.md", transition_matrix_markdown());
    files.insert("testing.md", testing_markdown());
    files.insert("layout-contracts.md", layout_contracts_markdown());
    files.insert("window-behavior.md", window_behavior_markdown());
    files.insert("contributing-gui-elements.md", contributing_markdown());
    files
}
fn gui_elements_markdown() -> String {
    let mut output = String::from(
        "# GUI element contracts\n\nGenerated from the typed viewer contract registry.\n\n| ID | Type | Location | Actions | Effects | Invalidation | Layout | Migration |\n| --- | --- | --- | --- | --- | --- | --- | --- |\n",
    );
    for contract in ui_element_registry() {
        output.push_str(&format!(
            "| {} | {:?} | {:?} | {:?} | {:?} | {:?} | {:?}/{:?} | {:?} |\n",
            contract.id.name(),
            contract.kind,
            contract.location,
            contract.emitted_actions,
            contract.expected_effects,
            contract.invalidation,
            contract.layout.horizontal,
            contract.layout.vertical,
            contract.migration,
        ));
    }
    output.push_str("\n## Qt Designer public objects\n\nThe Qt object identity and source file are generated from `.ui` XML; the semantic behaviour remains in the core registry.\n\n| Qt objectName | .ui source | Qt class | Core contract |\n| --- | --- | --- | --- |\n");
    for element in QT_UI_ELEMENTS.iter().filter(|element| element.is_public) {
        output.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            element.object_name,
            element.source_file,
            element.qt_class,
            qt_ui_contract_id(element.object_name)
                .map(UiElementId::name)
                .unwrap_or("MISSING"),
        ));
    }
    output
}
fn gui_elements_json() -> String {
    let values = ui_element_registry().iter().map(|contract| format!(
        "  {{\"id\":\"{}\",\"kind\":\"{:?}\",\"location\":\"{:?}\",\"migration\":\"{:?}\"}}",
        contract.id.name(), contract.kind, contract.location, contract.migration,
    )).collect::<Vec<_>>();
    format!("[\n{}\n]\n", values.join(",\n"))
}
fn public_states_markdown() -> String {
    let mut output = String::from(
        "# Public state contracts\n\nGenerated from PUBLIC_STATES.\n\n| State | Owner | Allowed values | Default | Meaning | Rationale | Hazard |\n| --- | --- | --- | --- | --- | --- | --- |\n",
    );
    for state in PUBLIC_STATES {
        output.push_str(&format!(
            "| {:?} | {} | {} | {} | {} | {} | {} |\n",
            state.id,
            state.owner.name(),
            state.allowed_values,
            state.default_value,
            state.meaning,
            state.rationale,
            state.hazard,
        ));
    }
    output
}
fn public_states_json() -> String {
    let values = PUBLIC_STATES
        .iter()
        .map(|state| format!("  {{\"id\":\"{:?}\",\"owner\":\"{}\",\"default\":\"{}\",\"rationale\":\"{}\",\"hazard\":\"{}\"}}", state.id, state.owner.name(), state.default_value, state.rationale, state.hazard))
        .collect::<Vec<_>>();
    format!("[\n{}\n]\n", values.join(",\n"))
}
fn state_invariants_markdown() -> String {
    "# State invariants\n\n- A clean document has no outstanding draft edits.\n- A current projection has a projection and no pending calculation.\n- Current queries have no pending query batch.\n- A dialog cannot overlap conflicting document I/O.\n- A visible panel has non-zero reachable width.\n- Async completion requires matching request and input revisions.\n".into()
}
fn state_hazards_markdown() -> String {
    "# State hazards\n\nDirty, stale, calculating, current, and failed states are explicit. This prevents old geometry and interpolation values being presented as current, protects unsaved classified-point changes, and blocks conflicting file dialogs.\n".into()
}
fn state_dependencies_markdown() -> String {
    "# State dependencies\n\nDataset edit -> dataset revision -> interpolator/query stale -> projection stale -> texture and hit geometry stale.\n\nInterpolation settings -> settings revision -> registered queries and projection recalculation.\n\nView transform -> hit geometry rebuild only.\n".into()
}
fn transition_matrix_markdown() -> String {
    let mut output = String::from(
        "# Transition matrix\n\n| Action | Effects | Stale policy |\n| --- | --- | --- |\n",
    );
    for action in UiActionKind::ALL {
        output.push_str(&format!("| {:?} | typed reducer effect where needed | request and revision checked for async completion |\n", action));
    }
    output
}
fn testing_markdown() -> String {
    let registry = ui_element_registry();
    let fully_migrated = registry
        .iter()
        .filter(|contract| contract.migration == ContractMigrationStatus::FullyContractDriven)
        .count();
    format!(
        "# GUI contract testing\n\nReducer tests run without GUI, native dialogs, filesystem, or workers. Effect interfaces are small enough for fakes. Generated documentation is compared with checked-in files. Geometry tests exercise DPI transitions without a physical multi-monitor test runner.\n\n## Coverage inventory\n\n```text\nInteractive elements:                 {elements}\nRegistered contracts:                 {elements} / {elements}\nFully contract-driven elements:       {fully_migrated} / {elements}\nPublic state categories:              {states} / {states}\nAction kinds with reducer coverage:   {actions} / {actions}\nEffect kinds with executor coverage:  {effects} / {effects}\nLayout contracts:                     {elements} / {elements}\n```\n\nThe migration count is deliberately reported separately: declared entries retain their contracts and documentation while their legacy rendering paths are incrementally moved behind contract-aware wrappers.\n",
        elements = registry.len(),
        states = PUBLIC_STATES.len(),
        actions = UiActionKind::ALL.len(),
        effects = UiEffectKind::ALL.len(),
    )
}
fn layout_contracts_markdown() -> String {
    "# Layout contracts\n\nToolbar wraps. Form panels scroll vertically. The canvas consumes remaining space and never requests native-window growth. Interpolation results scroll both directions and may explicitly collapse. Dialog bodies scroll while action buttons remain reachable. Silent clipping is not permitted.\n".into()
}
fn window_behavior_markdown() -> String {
    "# Window behavior\n\nThe native window receives an initial logical size only at startup. Frame updates never issue inner-size or outer-position commands. DPI changes update physical-rendering provenance only; they do not resize or reposition the native window.\n".into()
}
fn contributing_markdown() -> String {
    "# Adding a GUI element\n\n- [ ] Assign a stable UiElementId.\n- [ ] Add typed action, effect, transition, invalidation, and layout policy.\n- [ ] Add public-state rationale and hazard coverage.\n- [ ] Use a contract-aware widget wrapper.\n- [ ] Add reducer/effect/behavior tests.\n- [ ] Regenerate this directory with the GUI documentation generator.\n".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_public_designer_object_maps_to_a_core_contract() {
        let registry = ui_element_registry();
        for element in QT_UI_ELEMENTS.iter().filter(|element| element.is_public) {
            let contract = qt_ui_contract_id(element.object_name).unwrap_or_else(|| {
                panic!(
                    "missing core contract for Qt object {}",
                    element.object_name
                )
            });
            assert!(registry.iter().any(|entry| entry.id == contract));
        }
    }

    #[test]
    fn every_designer_menu_action_has_a_typed_native_action() {
        for action_id in QT_UI_ACTIONS {
            assert!(
                qt_ui_action(*action_id).is_some(),
                "missing typed action for Qt action {action_id:?}"
            );
        }
        assert_eq!(
            qt_ui_action(QtUiElementId::ActionFileOpen),
            Some(QtUiAction::OpenDocument)
        );
    }
    #[test]
    fn visible_qt_viewer_controls_use_the_thin_adapter_and_rust_bridge() {
        let source = std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../apps/ternary-contours-qt/src/main_window.cpp"),
        )
        .unwrap();
        for object_name in [
            "actionViewPlot",
            "actionViewGrid",
            "actionViewSourceVertices",
            "actionViewQueryPoints",
            "actionViewResultsTable",
            "actionViewFit",
            "actionViewReset",
            "actionViewRestoreLayout",
            "actionViewStableIsotherms",
            "actionViewStableUnivariants",
            "actionViewBinaryInvariants",
            "actionViewInteriorInvariants",
            "actionViewAxisLabels",
            "actionViewCornerNames",
            "actionViewLegend",
            "actionViewerClearSelectedQuery",
            "actionViewerClearAllQueries",
            "actionViewerResetAutomaticRange",
            "comboViewerGrid",
            "comboViewerPhase",
            "comboViewerProperty",
            "comboViewerMode",
            "buttonViewerExtrapolatePhase",
            "checkViewerCalculated",
            "checkViewerExtrapolated",
            "checkViewerCutOff",
            "checkViewerMissing",
            "spinViewerSamplingSubdivisions",
            "comboViewerSourceInterpolation",
            "comboViewerCubicMethod",
            "comboViewerPartialDomain",
            "comboViewerContinuation",
            "checkViewerRegularizePaths",
            "editViewerRegularizationSpacing",
            "comboViewerPathDisplay",
        ] {
            assert!(
                source.contains(object_name),
                "missing Qt receiver for {object_name}"
            );
        }
        assert!(source.contains("dispatchViewerWidgetCommand"));
        assert!(source.contains("updateViewerActionState"));
        assert!(source.contains("tcqt_set_viewer_calculation_options"));
        assert!(source.contains("tcqt_viewer_calculation_state"));
        assert!(source.contains("tcqt_calculate_viewer"));
        assert!(!source.contains("tcqt_calculate_current"));
        assert!(source.contains("pending_recalculation"));
        assert!(source.contains("result.request_id != generation"));
        assert!(source.contains("TcqtViewerCalculationState"));
        assert!(source.contains("CollapsibleSection"));
        for section in [
            "toggleViewerVertexVisibilitySection",
            "toggleViewerLabelsAppearanceSection",
            "toggleViewerSelectedVertexSection",
            "toggleViewerIsoRangeSection",
            "toggleViewerSourceCalculationSection",
            "toggleViewerPathsSection",
            "toggleViewerLayersSection",
            "toggleViewerDiagnosticsSection",
        ] {
            assert!(
                source.contains(section),
                "missing collapsible section wiring for {section}"
            );
        }
        for command in [
            "SetStableIsothermsVisible",
            "SetStableUnivariantsVisible",
            "SetBinaryInvariantsVisible",
            "SetInteriorInvariantsVisible",
            "SetAxisLabelsVisible",
            "SetCornerNamesVisible",
            "SetLegendVisible",
        ] {
            assert!(
                source.contains(command),
                "distinct Viewer command {command} is not wired"
            );
        }
    }

    #[test]
    fn interpolation_coordinate_dialog_has_a_non_committing_preview_path() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/ternary-contours-qt");
        let dialog =
            std::fs::read_to_string(root.join("src/interpolation_point_dialog.cpp")).unwrap();
        let canvas = std::fs::read_to_string(root.join("src/ternary_canvas.cpp")).unwrap();
        let window = std::fs::read_to_string(root.join("src/main_window.cpp")).unwrap();

        for object_name in [
            "interpolationPointDialog",
            "editGlobalA",
            "editGlobalB",
            "editGlobalC",
            "editLocal0",
            "editLocal1",
            "editLocal2",
            "buttonBoxInterpolationPoint",
        ] {
            assert!(
                QT_UI_ELEMENTS
                    .iter()
                    .any(|element| element.object_name == object_name),
                "missing coordinate-dialog object {object_name}"
            );
        }
        for required in [
            "validateGlobalOnFocusLoss",
            "validateLocalOnFocusLoss",
            "normalizeGlobalFromEditors",
            "normalizeLocalFromEditors",
            "previewLocationChanged",
            "handleOk",
            "tcqt_locate_grid_point",
            "tcqt_locate_grid_local_point",
        ] {
            assert!(
                dialog.contains(required),
                "missing dialog state transition {required}"
            );
        }
        assert!(canvas.contains("interpolation_preview_"));
        assert!(canvas.contains("setInterpolationPreview"));
        assert!(window.contains("setInterpolationPreview(initial)"));
        assert!(window.contains("clearInterpolationPreview();"));
        assert!(window.contains("dialog.exec() == QDialog::Accepted"));
        assert!(window.contains("viewer_.queries.append(query);"));
    }
    #[test]
    fn coordinate_dialog_uses_fixed_display_precision_and_consumes_enter() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/ternary-contours-qt");
        let dialog =
            std::fs::read_to_string(root.join("src/interpolation_point_dialog.cpp")).unwrap();
        let header =
            std::fs::read_to_string(root.join("src/interpolation_point_dialog.hpp")).unwrap();
        let bridge = std::fs::read_to_string(root.join("rust-bridge/src/lib.rs")).unwrap();
        let window = std::fs::read_to_string(root.join("src/main_window.cpp")).unwrap();

        assert!(dialog.contains("displayNumber(value, DisplayNumberKind::Composition)"));
        assert!(dialog.contains("displayedNormalizedTriplet"));
        assert!(dialog.contains("Qt::Key_Return || key_event->key() == Qt::Key_Enter"));
        assert!(dialog.contains("editor->installEventFilter(this)"));
        assert!(!dialog.contains("&QLineEdit::returnPressed"));
        assert!(header.contains("bool eventFilter(QObject* watched, QEvent* event) override"));
        assert!(bridge.contains("tcqt_evaluate_field_current"));
        assert!(window.contains("tcqt_evaluate_field_current"));
    }

    fn button_placeholder_is_not_a_value(ui: &str) -> bool {
        ui.contains("placeholderText")
            && !ui.contains(r#"<property name="text"><string>minimum maximum step; extra1, extra2</string></property>"#)
    }

    #[test]
    fn viewer_level_field_uses_concrete_auto_text_and_retains_manual_fallback() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/ternary-contours-qt");
        let ui = std::fs::read_to_string(root.join("ui/main_window.ui")).unwrap();
        let window = std::fs::read_to_string(root.join("src/main_window.cpp")).unwrap();
        let header = std::fs::read_to_string(root.join("src/main_window.hpp")).unwrap();

        assert!(ui.contains("editViewerIsoLevelSpec"));
        assert!(ui.contains("placeholderText"));
        assert!(ui.contains("minimum maximum step; extra1, extra2"));
        assert!(button_placeholder_is_not_a_value(&ui));
        assert!(ui.contains("buttonViewerResetAutomaticRange"));
        assert!(!ui.contains("checkViewerAutomaticRange"));
        assert!(header.contains("IsoLevelSpecOrigin"));
        assert!(header.contains("AwaitingTopology"));
        assert!(header.contains("AutoDerived"));
        assert!(header.contains("UserEdited"));
        assert!(header.contains("accepted_iso_level_spec"));
        assert!(header.contains("requested_iso_level_spec"));
        assert!(window.contains("Waiting for stable topology to derive invariant-based levels."));
        assert!(window.contains("accepted_iso_level_spec"));
        assert!(window.contains("Derived from accepted invariant temperatures."));
        assert!(!window.contains("QStringLiteral(\"automatic\")"));
    }

    #[test]
    fn designer_viewer_uses_the_required_split_pane_hierarchy() {
        for object_name in [
            "splitterViewerOuter",
            "splitterViewerControls",
            "scrollVertexInspection",
            "groupVertexInspection",
            "scrollIsoPlots",
            "groupIsoPlots",
            "splitterViewerRight",
            "canvasTernary",
            "resultTablesPane",
            "splitterViewerResultTables",
            "groupInterpolationResults",
            "tableInterpolationResults",
            "groupInvariantPoints",
            "tableInvariantPoints",
            "buttonInterpolationCopy",
            "buttonInterpolationRemoveSelected",
            "buttonInterpolationClearAll",
            "buttonInvariantCopy",
        ] {
            assert!(
                QT_UI_ELEMENTS
                    .iter()
                    .any(|element| element.object_name == object_name)
            );
        }
        let parent = |name| {
            QT_UI_ELEMENTS
                .iter()
                .find(|element| element.object_name == name)
                .unwrap()
                .parent_object_name
        };
        assert_eq!(parent("splitterViewerControls"), "splitterViewerOuter");
        assert_eq!(parent("splitterViewerRight"), "splitterViewerOuter");
        assert_eq!(parent("scrollVertexInspection"), "splitterViewerControls");
        assert_eq!(parent("scrollIsoPlots"), "splitterViewerControls");
        assert_eq!(parent("canvasTernary"), "splitterViewerRight");
        assert_eq!(parent("resultTablesPane"), "splitterViewerRight");
        assert_eq!(
            parent("splitterViewerResultTables"),
            "layoutViewerResultTablesPane"
        );
        assert_eq!(
            parent("groupInterpolationResults"),
            "splitterViewerResultTables"
        );
        assert_eq!(parent("groupInvariantPoints"), "splitterViewerResultTables");
        assert_eq!(
            parent("tableInterpolationResults"),
            "layoutInterpolationResults"
        );
        assert_eq!(parent("tableInvariantPoints"), "layoutInvariantPoints");
        assert!(!QT_UI_ELEMENTS.iter().any(|element| matches!(
            element.object_name,
            "viewerControls" | "groupViewerPresentation" | "buttonRunRustCalculation"
        )));
        let ui = std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../apps/ternary-contours-qt/ui/main_window.ui"),
        )
        .unwrap();
        assert!(ui.contains("<string>Vertex</string>"));
        assert!(ui.contains("<string>Interpolate</string>"));
        for section in [
            "toggleViewerVertexVisibilitySection",
            "toggleViewerLabelsAppearanceSection",
            "toggleViewerSelectedVertexSection",
            "toggleViewerIsoRangeSection",
            "toggleViewerSourceCalculationSection",
            "toggleViewerPathsSection",
            "toggleViewerLayersSection",
            "toggleViewerDiagnosticsSection",
        ] {
            assert!(
                ui.contains(section),
                "missing Designer-defined collapsible header {section}"
            );
        }
        assert!(!ui.contains("<string>Inspect</string>"));
        assert!(!ui.contains("<string>Edit</string>"));
    }

    #[test]
    fn viewer_result_tables_are_sortable_and_use_stable_ids() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/ternary-contours-qt");
        let ui = std::fs::read_to_string(root.join("ui/main_window.ui")).unwrap();
        let window = std::fs::read_to_string(root.join("src/main_window.cpp")).unwrap();
        for required in [
            "resultTablesPane",
            "splitterViewerResultTables",
            "tableInvariantPoints",
            "buttonInterpolationCopy",
            "buttonInterpolationRemoveSelected",
            "buttonInterpolationClearAll",
            "actionViewerCopyQueries",
            "actionViewerCopyInvariantPoints",
        ] {
            assert!(
                ui.contains(required),
                "missing Viewer result-table object {required}"
            );
        }
        for required in [
            "TypedResultSortProxyModel",
            "query_id_role",
            "invariant_id_role",
            "selectedRowsAsTsv",
            "removeSelectedInterpolationQueries",
            "clearAllInterpolationQueries",
            "tcqt_invariant_point_count",
            "tcqt_invariant_point_at",
            "splitter/viewer-result-tables",
        ] {
            assert!(
                window.contains(required),
                "missing result-table wiring {required}"
            );
        }
    }

    #[test]
    fn designer_tab_order_and_menu_hierarchy_are_explicit() {
        let tab_order = QT_UI_TAB_ORDER
            .iter()
            .map(|id| {
                QT_UI_ELEMENTS
                    .iter()
                    .find(|element| element.id == *id)
                    .unwrap()
                    .object_name
            })
            .collect::<Vec<_>>();
        assert_eq!(
            tab_order,
            [
                "primaryTabs",
                "treeProject",
                "editProjectTitle",
                "editCornerA",
                "editCornerB",
                "editCornerC",
                "buttonAddPhase",
                "buttonRemovePhase",
                "buttonAddProperty",
                "buttonAddIrregularRow",
                "tableGridValues",
                "comboViewerGrid",
                "comboViewerPhase",
                "comboViewerProperty",
                "comboViewerMode",
                "buttonViewerExtrapolatePhase",
                "canvasTernary",
                "buttonInterpolationCopy",
                "buttonInterpolationRemoveSelected",
                "buttonInterpolationClearAll",
                "tableInterpolationResults",
                "buttonInvariantCopy",
                "tableInvariantPoints"
            ]
        );
        for (child, parent) in [("menuExport", "menuFile"), ("menuAddGrid", "menuGrid")] {
            let element = QT_UI_ELEMENTS
                .iter()
                .find(|element| element.object_name == child)
                .unwrap();
            assert_eq!(element.parent_object_name, parent);
        }
    }
    #[test]
    fn qt_designer_inventory_documents_sources_and_contracts() {
        let inventory = qt_ui_inventory_markdown();
        assert!(inventory.contains("main_window.ui"));
        assert!(inventory.contains("actionFileOpen"));
        assert!(!inventory.contains("MISSING"));
    }
    #[test]
    fn every_ui_element_has_a_contract() {
        let registry = ui_element_registry();
        assert_eq!(registry.len(), UiElementId::ALL.len());
        for id in UiElementId::ALL {
            assert_eq!(registry.iter().filter(|entry| entry.id == *id).count(), 1);
        }
    }
    #[test]
    fn every_contract_has_invalidation_and_layout() {
        for contract in ui_element_registry() {
            assert!(!contract.invalidation.is_empty(), "{}", contract.id.name());
            assert!(contract.layout.min_width > 0, "{}", contract.id.name());
            assert!(contract.layout.min_height > 0, "{}", contract.id.name());
        }
    }
    #[test]
    fn every_public_state_has_meaning_rationale_and_hazard() {
        assert!(!PUBLIC_STATES.is_empty());
        for state in PUBLIC_STATES {
            assert!(!state.allowed_values.is_empty());
            assert!(!state.meaning.is_empty());
            assert!(!state.rationale.is_empty());
            assert!(!state.hazard.is_empty());
            assert!(!state.entered_by.is_empty());
            assert!(!state.exited_by.is_empty());
        }
    }
    #[test]
    fn tab_order_is_contract_order() {
        assert_eq!(
            ViewerTab::ORDERED,
            [
                ViewerTab::Data,
                ViewerTab::Diagnostics,
                ViewerTab::GridInspection,
                ViewerTab::Plot
            ]
        );
    }
    #[test]
    fn dirty_open_requires_confirmation_and_cancel_preserves_edits() {
        let mut state = GuiContractState {
            document: DocumentFreshness::Dirty,
            draft_matches_active: false,
            ..GuiContractState::default()
        };
        assert_eq!(
            update(&mut state, UiAction::OpenRequested),
            vec![UiEffect::ShowUnsavedChangesDialog]
        );
        update(
            &mut state,
            UiAction::UnsavedDecisionSelected(UnsavedDecision::Cancel),
        );
        assert_eq!(state.document, DocumentFreshness::Dirty);
        assert_eq!(state.dialog, DialogState::Closed);
    }
    #[test]
    fn interpolation_change_recalculates_queries_and_projection() {
        let mut state = GuiContractState {
            queries: QueryFreshness::Current,
            has_projection: true,
            ..GuiContractState::default()
        };
        let effects = update(&mut state, UiAction::CalculationSettingsCommitted);
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, UiEffect::RecalculateRegisteredQueries { .. }))
        );
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, UiEffect::RecalculateProjection { .. }))
        );
    }
    #[test]
    fn stale_completion_is_rejected() {
        let mut state = GuiContractState::default();
        let effects = update(&mut state, UiAction::RecalculateRequested);
        let UiEffect::RecalculateProjection {
            request,
            dataset_revision,
            settings_revision,
        } = effects[0]
        else {
            panic!("expected projection effect");
        };
        update(&mut state, UiAction::CalculationSettingsCommitted);
        assert!(
            update(
                &mut state,
                UiAction::ProjectionCalculated {
                    request,
                    dataset_revision,
                    settings_revision,
                    result: Ok(())
                }
            )
            .is_empty()
        );
        assert_ne!(state.projection, ProjectionFreshness::Current);
    }
    #[test]
    fn dpi_transition_never_requests_native_resize() {
        let mut state = GuiContractState::default();
        assert!(update(&mut state, UiAction::WindowScaleFactorChanged(1500)).is_empty());
        assert_eq!(state.window, WindowGeometry::default());
        assert_eq!(state.scale_factor_milli, 1500);
    }
    #[test]
    fn documentation_inventory_is_complete() {
        let files = generated_documentation();
        assert_eq!(files.len(), 12);
        assert!(files["gui-elements.md"].contains("Open"));
    }
    #[test]
    fn generated_gui_docs_are_current() {
        let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/gui");
        for (name, generated) in generated_documentation() {
            let checked_in = std::fs::read_to_string(directory.join(name)).unwrap();
            assert_eq!(
                checked_in, generated,
                "generated GUI document {name} is stale"
            );
        }
    }

    #[test]
    fn logical_geometry_scales_once_per_dpi_transition() {
        let geometry = WindowGeometry::default();
        assert_eq!(physical_size_from_logical(geometry, 1000), (1280, 800));
        assert_eq!(physical_size_from_logical(geometry, 1500), (1920, 1200));
        assert_eq!(physical_size_from_logical(geometry, 2000), (2560, 1600));
    }

    #[test]
    fn restore_keeps_reachable_negative_secondary_position() {
        let saved = WindowGeometry {
            outer_x: -1400,
            outer_y: 120,
            ..WindowGeometry::default()
        };
        let monitors = [
            MonitorWorkArea {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                scale_factor_milli: 1000,
            },
            MonitorWorkArea {
                x: -1600,
                y: 0,
                width: 1600,
                height: 900,
                scale_factor_milli: 1500,
            },
        ];
        assert_eq!(restore_accessible_geometry(saved, &monitors), saved);
    }

    #[test]
    fn restore_clamps_only_inaccessible_title_bar() {
        let saved = WindowGeometry {
            outer_x: -1400,
            outer_y: 120,
            ..WindowGeometry::default()
        };
        let monitors = [MonitorWorkArea {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            scale_factor_milli: 1000,
        }];
        let restored = restore_accessible_geometry(saved, &monitors);
        assert_eq!(
            (restored.logical_width, restored.logical_height),
            (1280, 800)
        );
        assert!(restored.outer_x >= 0);
        assert!(restored.outer_y >= 0);
    }
}
