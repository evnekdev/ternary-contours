#include "main_window.hpp"
#include "collapsible_section.hpp"
#include "interpolation_point_dialog.hpp"

#include "grid_table_model.hpp"
#include "rust_bridge.hpp"
#include "ternary_canvas.hpp"
#include "ui_add_grid_dialog.h"
#include <algorithm>
#include <array>
#include <functional>
#include "ui_main_window.h"

#include <QAbstractItemView>
#include <limits>
#include <QAction>
#include <QApplication>
#include <QCloseEvent>
#include <QCheckBox>
#include <QComboBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QGuiApplication>
#include <QGroupBox>
#include <QLabel>
#include <QScreen>
#include <QSignalBlocker>
#include <QSpinBox>
#include <QClipboard>
#include <QMenu>
#include <QFileDialog>
#include <QFileInfo>
#include <QFutureWatcher>
#include <QInputDialog>
#include <QItemSelectionModel>
#include <QLineEdit>
#include <QTextEdit>
#include <QTableWidget>
#include <QHeaderView>
#include <QVBoxLayout>
#include <QMessageBox>
#include <QSettings>
#include <QTimer>

#include <QLocale>
#include <QPushButton>
#include <QStandardItem>
#include <QStandardItemModel>
#include <QStyle>
#include <QtConcurrentRun>

namespace {
constexpr int node_kind_role = Qt::UserRole;
constexpr int grid_id_role = Qt::UserRole + 1;
constexpr int phase_id_role = Qt::UserRole + 2;
constexpr int property_id_role = Qt::UserRole + 3;
constexpr int field_id_role = Qt::UserRole + 4;
enum class NodeKind {
    Project,
    Title,
    Corner,
    PhaseCollection,
    Phase,
    PropertyCollection,
    Property,
    GridCollection,
    Grid,
    GridPhase,
    GridField
};
constexpr int min_regular_subdivisions = 1;
constexpr int max_regular_subdivisions = 50;
constexpr int default_regular_subdivisions = 10;
constexpr std::uint32_t invalid_viewer_abi = std::numeric_limits<std::uint32_t>::max();
constexpr std::uint32_t mesh_scope_field = 0;
constexpr std::uint32_t mesh_scope_phase = 1;
constexpr std::uint32_t mesh_scope_targets = 2;
constexpr std::uint32_t abi_source_linear = 0;
constexpr std::uint32_t abi_source_cubic_alpha = 1;
constexpr std::uint32_t abi_cubic_akima = 0;
constexpr std::uint32_t abi_cubic_makima = 1;
constexpr std::uint32_t abi_cubic_pchip = 2;
constexpr std::uint32_t abi_cubic_steffen = 3;
constexpr std::uint32_t abi_partial_strict = 0;
constexpr std::uint32_t abi_partial_one_sided = 1;
constexpr std::uint32_t abi_partial_one_sided_then_linear = 2;
constexpr std::uint32_t abi_partial_linear_near_boundaries = 3;
constexpr std::uint32_t abi_continuation_raw_barycentric = 0;
constexpr std::uint32_t abi_continuation_muggianu = 1;
constexpr std::uint32_t abi_continuation_kohler = 2;
std::uint32_t sourceInterpolationAbi(int index) {
    switch (index) { case 0: return abi_source_linear; case 1: return abi_source_cubic_alpha; default: return invalid_viewer_abi; }
}
int sourceInterpolationIndex(std::uint32_t value) {
    switch (value) { case abi_source_linear: return 0; case abi_source_cubic_alpha: return 1; default: return -1; }
}
std::uint32_t cubicMethodAbi(int index) {
    switch (index) { case 0: return abi_cubic_akima; case 1: return abi_cubic_makima; case 2: return abi_cubic_pchip; case 3: return abi_cubic_steffen; default: return invalid_viewer_abi; }
}
int cubicMethodIndex(std::uint32_t value) {
    switch (value) { case abi_cubic_akima: return 0; case abi_cubic_makima: return 1; case abi_cubic_pchip: return 2; case abi_cubic_steffen: return 3; default: return -1; }
}
std::uint32_t partialDomainAbi(int index) {
    switch (index) { case 0: return abi_partial_strict; case 1: return abi_partial_one_sided; case 2: return abi_partial_one_sided_then_linear; case 3: return abi_partial_linear_near_boundaries; default: return invalid_viewer_abi; }
}
int partialDomainIndex(std::uint32_t value) {
    switch (value) { case abi_partial_strict: return 0; case abi_partial_one_sided: return 1; case abi_partial_one_sided_then_linear: return 2; case abi_partial_linear_near_boundaries: return 3; default: return -1; }
}
std::uint32_t continuationAbi(int index) {
    switch (index) { case 0: return abi_continuation_raw_barycentric; case 1: return abi_continuation_muggianu; case 2: return abi_continuation_kohler; default: return invalid_viewer_abi; }
}
int continuationIndex(std::uint32_t value) {
    switch (value) { case abi_continuation_raw_barycentric: return 0; case abi_continuation_muggianu: return 1; case abi_continuation_kohler: return 2; default: return -1; }
}QString text(const char* value) { return QString::fromUtf8(value); }
QString statusText(const TcqtStatus& status) { return text(status.message); }
QString calculationText(const TcqtCalculationResult& result) { return text(result.message); }
QString documentLabel(const TcqtProjectSummary& summary) {
    const auto path = text(summary.path);
    const auto name = path.isEmpty() ? QStringLiteral("Untitled") : QFileInfo(path).fileName();
    return name + (summary.dirty ? QStringLiteral(" *") : QString());
}
QString documentStatusLabel(const TcqtProjectSummary& summary) {
    return summary.validity == 2 ? QStringLiteral("Calculation-ready") : summary.validity == 1 ? QStringLiteral("Draft document") : QStringLiteral("Invalid document");
}
QString documentTooltip(const TcqtProjectSummary& summary) {
    const auto path = text(summary.path).isEmpty() ? QStringLiteral("Not yet saved") : text(summary.path);
    return path + QStringLiteral("\n") + documentStatusLabel(summary) + QStringLiteral("\n") + (summary.dirty ? QStringLiteral("Modified") : QStringLiteral("Saved"));
}
QStandardItem* node(const QString& label, NodeKind kind, std::uint32_t grid_id = 0,
                    std::uint32_t phase_id = 0, std::uint32_t property_id = 0,
                    std::uint32_t field_id = 0) {
    auto* item = new QStandardItem(label);
    item->setEditable(false);
    item->setData(static_cast<int>(kind), node_kind_role);
    item->setData(grid_id, grid_id_role);
    item->setData(phase_id, phase_id_role);
    item->setData(property_id, property_id_role);
    item->setData(field_id, field_id_role);
    if (kind == NodeKind::Grid) item->setIcon(qApp->style()->standardIcon(QStyle::SP_DirIcon));
    if (kind == NodeKind::GridPhase) item->setIcon(qApp->style()->standardIcon(QStyle::SP_ComputerIcon));
    if (kind == NodeKind::GridField) item->setIcon(qApp->style()->standardIcon(QStyle::SP_FileIcon));
    return item;
}
bool isGridNode(NodeKind kind) {
    return kind == NodeKind::Grid || kind == NodeKind::GridPhase || kind == NodeKind::GridField;
}
QString normalizeTctPath(QString path) {
    if (!path.endsWith(QStringLiteral(".tct"), Qt::CaseInsensitive)) path += QStringLiteral(".tct");
    return path;
}
QString fieldProperty(const TcqtField& field) { return text(field.property); }
}

MainWindow::MainWindow(QWidget* parent) : QMainWindow(parent), ui_(std::make_unique<Ui::MainWindow>()) {
    ui_->setupUi(this);
    tree_model_ = new QStandardItemModel(ui_->treeProject);
    tree_model_->setHorizontalHeaderLabels({tr("Project")});
    ui_->treeProject->setModel(tree_model_);
    ui_->treeProject->setEditTriggers(QAbstractItemView::NoEditTriggers);
    grid_model_ = new GridTableModel(ui_->tableGridValues);
    ui_->tableGridValues->setModel(grid_model_);
    ui_->tableGridValues->setContextMenuPolicy(Qt::CustomContextMenu);
    ui_->tableGridValues->addAction(ui_->actionGridCopy);
    ui_->tableGridValues->addAction(ui_->actionGridPaste);
    auto* results_model = new QStandardItemModel(0, 11, ui_->tableInterpolationResults);
    results_model->setHorizontalHeaderLabels({tr("Index"), tr("A"), tr("B"), tr("C"), tr("Value"), tr("State"), tr("Grid"), tr("Phase"), tr("Property"), tr("Method"), tr("Source provenance")});
    ui_->tableInterpolationResults->setModel(results_model);
    connect(ui_->tableInterpolationResults->selectionModel(), &QItemSelectionModel::currentChanged, this,
            [this](const QModelIndex& current, const QModelIndex&) {
        QVector<CanvasQuery> queries;
        queries.reserve(viewer_.queries.size());
        for (qsizetype index = 0; index < viewer_.queries.size(); ++index) {
            const auto& query = viewer_.queries.at(index);
            queries.append({query.id, QPointF(query.a, query.b), query.result.state,
                            current.isValid() && current.row() == index, {}});
        }
        ui_->canvasTernary->setQueries(queries);
        updateViewerActionState();
    });
    ui_->actionViewSourceVertices->setChecked(true);
    ui_->actionViewQueryPoints->setChecked(true);

    const auto add_viewer_section = [this](QToolButton* header, QWidget* content,
                                           bool expanded_by_default,
                                           const QString& settings_key) {
        auto section = std::make_unique<CollapsibleSection>(
            header, content, expanded_by_default, settings_key, this);
        section->restore();
        viewer_sections_.push_back(std::move(section));
    };
    add_viewer_section(ui_->toggleViewerVertexVisibilitySection, ui_->groupViewerVertexVisibility, false,
                       QStringLiteral("viewer/section/vertex-visibility"));
    add_viewer_section(ui_->toggleViewerLabelsAppearanceSection, ui_->groupViewerLabelsAppearance, false,
                       QStringLiteral("viewer/section/labels-appearance"));
    add_viewer_section(ui_->toggleViewerSelectedVertexSection, ui_->groupViewerSelectedVertex, false,
                       QStringLiteral("viewer/section/selected-vertex"));
    add_viewer_section(ui_->toggleViewerIsoRangeSection, ui_->groupViewerIsoRange, true,
                       QStringLiteral("viewer/section/isotherm-range"));
    add_viewer_section(ui_->toggleViewerSourceCalculationSection, ui_->groupViewerSourceCalculation, false,
                       QStringLiteral("viewer/section/source-calculation"));
    add_viewer_section(ui_->toggleViewerPathsSection, ui_->groupViewerPaths, false,
                       QStringLiteral("viewer/section/paths"));
    add_viewer_section(ui_->toggleViewerLayersSection, ui_->groupViewerLayers, false,
                       QStringLiteral("viewer/section/layers"));
    add_viewer_section(ui_->toggleViewerDiagnosticsSection, ui_->groupViewerDiagnostics, false,
                       QStringLiteral("viewer/section/diagnostics"));

    connect(ui_->actionFileNew, &QAction::triggered, this, &MainWindow::newDocument);
    connect(ui_->actionFileOpen, &QAction::triggered, this, &MainWindow::openDocument);
    connect(ui_->actionFileSave, &QAction::triggered, this, &MainWindow::saveDocument);
    connect(ui_->actionFileSaveAs, &QAction::triggered, this, &MainWindow::saveDocumentAs);
    connect(ui_->actionExportPng, &QAction::triggered, this, &MainWindow::exportPng);
    connect(ui_->actionExportSvg, &QAction::triggered, this, &MainWindow::exportSvg);
    connect(ui_->actionExportLinesCsv, &QAction::triggered, this, &MainWindow::exportLinesCsv);
    connect(ui_->actionQuit, &QAction::triggered, this, &QWidget::close);
    connect(ui_->actionAboutQt, &QAction::triggered, qApp, &QApplication::aboutQt);
    connect(ui_->actionSettings, &QAction::triggered, this, [this] {
        QDialog dialog(this);
        dialog.setWindowTitle(tr("Developer diagnostics"));
        auto* layout = new QFormLayout(&dialog);
        auto* level = new QComboBox(&dialog);
        level->addItem(tr("Off"), 0);
        level->addItem(tr("Summary"), 1);
        level->addItem(tr("Decisions"), 2);
        level->addItem(tr("Iterations"), 3);
        auto* destination = new QLineEdit(&dialog);
        destination->setPlaceholderText(tr("Trace destination (.jsonl)"));
        destination->setAccessibleName(tr("Numerical trace destination"));
        destination->setAccessibleDescription(tr("Path for the Rust-owned deterministic numerical trace output."));
        level->setAccessibleName(tr("Numerical trace level"));
        level->setAccessibleDescription(tr("Developer-only trace detail. It does not alter numerical options or document state."));
        layout->addRow(tr("Numerical trace"), level);
        layout->addRow(tr("Trace destination"), destination);
        auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, &dialog);
        layout->addRow(buttons);
        connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
        connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
        if (dialog.exec() != QDialog::Accepted) return;
        const auto encoded = destination->text().toUtf8();
        const auto result = tcqt_set_numerical_trace(level->currentData().toUInt(), encoded.constData());
        reportBridgeStatus(statusText(result), result.success);
    });    connect(ui_->actionGridAddRegular, &QAction::triggered, this, [this] { addGrid(true); });
    connect(ui_->actionGridAddIrregular, &QAction::triggered, this, [this] { addGrid(false); });
    connect(ui_->actionGridRemove, &QAction::triggered, this, &MainWindow::removeSelectedGrid);
    connect(ui_->actionGridDuplicate, &QAction::triggered, this, &MainWindow::duplicateSelectedGrid);
    connect(ui_->actionGridRename, &QAction::triggered, this, &MainWindow::renameSelectedGrid);
    connect(ui_->actionGridCopy, &QAction::triggered, this, &MainWindow::copyGridSelection);
    connect(ui_->actionGridPaste, &QAction::triggered, this, &MainWindow::pasteGridClipboard);
    connect(ui_->actionGridExtrapolate, &QAction::triggered, this, &MainWindow::extrapolateSelectedRegularField);
    connect(ui_->buttonViewerExtrapolatePhase, &QPushButton::clicked, this, &MainWindow::extrapolateViewerPhase);
    connect(ui_->tableGridValues, &QTableView::customContextMenuRequested, this, &MainWindow::showGridContextMenu);
    connect(ui_->tableGridValues->selectionModel(), &QItemSelectionModel::selectionChanged, this, [this] { updateActionState(); });
    connect(QApplication::clipboard(), &QClipboard::dataChanged, this, [this] { updateActionState(); });
    connect(ui_->buttonAddPhase, &QPushButton::clicked, this, &MainWindow::addPhase);
    connect(ui_->buttonRemovePhase, &QPushButton::clicked, this, &MainWindow::removeSelectedPhase);
    connect(ui_->buttonAddProperty, &QPushButton::clicked, this, &MainWindow::addProperty);
    connect(ui_->buttonAddIrregularRow, &QPushButton::clicked, this, &MainWindow::addIrregularRow);

    const auto bind_toggle = [this](QAction* action, QCheckBox* panel, ViewerWidgetCommand viewer_action) {
        connect(action, &QAction::toggled, this, [this, viewer_action] { dispatchViewerWidgetCommand(viewer_action); });
        if (panel) connect(panel, &QCheckBox::toggled, this, [this, action, viewer_action](bool checked) {
            QSignalBlocker blocker(action); action->setChecked(checked); dispatchViewerWidgetCommand(viewer_action);
        });
    };
    bind_toggle(ui_->actionViewPlot, nullptr, ViewerWidgetCommand::SetMasterPlotVisible);
    bind_toggle(ui_->actionViewGrid, ui_->checkViewerSamplingGrid, ViewerWidgetCommand::SetSamplingGridVisible);
    bind_toggle(ui_->actionViewSourceVertices, ui_->checkViewerSourceVertices, ViewerWidgetCommand::SetSourceVerticesVisible);
    bind_toggle(ui_->actionViewQueryPoints, ui_->checkViewerQueryPoints, ViewerWidgetCommand::SetQueryPointsVisible);
    bind_toggle(ui_->actionViewResultsTable, nullptr, ViewerWidgetCommand::SetResultsTableVisible);
    bind_toggle(ui_->actionViewStableIsotherms, ui_->checkViewerStableIsotherms, ViewerWidgetCommand::SetStableIsothermsVisible);
    bind_toggle(ui_->actionViewStableUnivariants, ui_->checkViewerStableUnivariants, ViewerWidgetCommand::SetStableUnivariantsVisible);
    bind_toggle(ui_->actionViewBinaryInvariants, ui_->checkViewerBinaryInvariants, ViewerWidgetCommand::SetBinaryInvariantsVisible);
    bind_toggle(ui_->actionViewInteriorInvariants, ui_->checkViewerInteriorInvariants, ViewerWidgetCommand::SetInteriorInvariantsVisible);
    bind_toggle(ui_->actionViewAxisLabels, ui_->checkViewerAxisLabels, ViewerWidgetCommand::SetAxisLabelsVisible);
    bind_toggle(ui_->actionViewCornerNames, ui_->checkViewerCornerNames, ViewerWidgetCommand::SetCornerNamesVisible);
    bind_toggle(ui_->actionViewLegend, ui_->checkViewerLegend, ViewerWidgetCommand::SetLegendVisible);
    connect(ui_->actionViewFit, &QAction::triggered, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::Fit); });
    connect(ui_->actionViewReset, &QAction::triggered, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::Reset); });
    connect(ui_->actionViewRestoreLayout, &QAction::triggered, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::RestoreLayout); });
    connect(ui_->actionViewerClearSelectedQuery, &QAction::triggered, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::RemoveSelectedQuery); });
    connect(ui_->actionViewerClearAllQueries, &QAction::triggered, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::RemoveAllQueries); });
    connect(ui_->actionViewerResetAutomaticRange, &QAction::triggered, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::ResetAutomaticRange); });
    connect(ui_->buttonViewerResetAutomaticRange, &QPushButton::clicked, ui_->actionViewerResetAutomaticRange, &QAction::trigger);

    connect(ui_->comboViewerGrid, qOverload<int>(&QComboBox::currentIndexChanged), this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SelectGrid); });
    connect(ui_->comboViewerPhase, qOverload<int>(&QComboBox::currentIndexChanged), this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SelectPhase); });
    connect(ui_->comboViewerProperty, qOverload<int>(&QComboBox::currentIndexChanged), this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SelectProperty); });
    connect(ui_->comboViewerMode, qOverload<int>(&QComboBox::currentIndexChanged), this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetInteractionMode); });
    for (auto* toggle : {ui_->checkViewerCalculated, ui_->checkViewerExtrapolated, ui_->checkViewerCutOff, ui_->checkViewerMissing}) connect(toggle, &QCheckBox::toggled, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetVertexVisibility); });
    connect(ui_->checkViewerRegularGridEdges, &QCheckBox::toggled, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetRegularGridEdges); });
    connect(ui_->spinViewerMarkerSize, qOverload<int>(&QSpinBox::valueChanged), this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetMarkerSize); });
    connect(ui_->comboViewerLabelMode, qOverload<int>(&QComboBox::currentIndexChanged), this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetLabelMode); });
    connect(ui_->spinViewerLabelDecimals, qOverload<int>(&QSpinBox::valueChanged), this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetLabelDecimals); });
    connect(ui_->checkViewerLabelsSelectedOnly, &QCheckBox::toggled, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetLabelsSelectedOnly); });
    connect(ui_->checkViewerAutomaticRange, &QCheckBox::toggled, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetAutomaticRange); });
    connect(ui_->editViewerTmin, &QLineEdit::editingFinished, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::CommitIsoMinimum); });
    connect(ui_->editViewerTmax, &QLineEdit::editingFinished, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::CommitIsoMaximum); });
    connect(ui_->editViewerStep, &QLineEdit::editingFinished, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::CommitIsoStep); });
    connect(ui_->spinViewerSamplingSubdivisions, qOverload<int>(&QSpinBox::valueChanged), this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetSamplingSubdivisions); });
    connect(ui_->comboViewerSourceInterpolation, qOverload<int>(&QComboBox::currentIndexChanged), this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetSourceInterpolation); });
    connect(ui_->comboViewerCubicMethod, qOverload<int>(&QComboBox::currentIndexChanged), this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetCubicMethod); });
    connect(ui_->comboViewerPartialDomain, qOverload<int>(&QComboBox::currentIndexChanged), this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetPartialDomainPolicy); });
    connect(ui_->comboViewerContinuation, qOverload<int>(&QComboBox::currentIndexChanged), this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetContinuation); });
    connect(ui_->checkViewerRegularizePaths, &QCheckBox::toggled, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetRegularizationEnabled); });
    connect(ui_->editViewerRegularizationSpacing, &QLineEdit::editingFinished, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetRegularizationSpacing); });
    connect(ui_->comboViewerPathDisplay, qOverload<int>(&QComboBox::currentIndexChanged), this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetPathDisplayMode); });
    connect(ui_->spinViewerLineWidth, qOverload<int>(&QSpinBox::valueChanged), this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetLineWidth); });
    connect(ui_->spinViewerPlotMarkerSize, qOverload<int>(&QSpinBox::valueChanged), this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetPlotMarkerSize); });
    connect(ui_->checkViewerPathVertices, &QCheckBox::toggled, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetPathVerticesVisible); });
    connect(ui_->checkViewerContourEndpoints, &QCheckBox::toggled, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetContourEndpointsVisible); });
    connect(ui_->checkViewerUnivariantEndpoints, &QCheckBox::toggled, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetUnivariantEndpointsVisible); });
    connect(ui_->checkViewerInvariantIds, &QCheckBox::toggled, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetInvariantIdsVisible); });
    connect(ui_->checkViewerUnivariantIds, &QCheckBox::toggled, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetUnivariantIdsVisible); });
    connect(ui_->checkViewerPhasePairLabels, &QCheckBox::toggled, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetPhasePairLabelsVisible); });
    connect(ui_->checkViewerContainingTriangle, &QCheckBox::toggled, this, [this] { dispatchViewerWidgetCommand(ViewerWidgetCommand::SetContainingTriangleVisible); });
    connect(ui_->canvasTernary, &TernaryCanvas::compositionSelected, this, &MainWindow::updateComposition);
    connect(ui_->canvasTernary, &TernaryCanvas::vertexSelected, this, &MainWindow::selectViewerVertex);
    connect(ui_->canvasTernary, &TernaryCanvas::vertexDoubleClicked, this, &MainWindow::editViewerVertex);
    connect(ui_->canvasTernary, &TernaryCanvas::vertexContextRequested, this, &MainWindow::showViewerVertexContextMenu);
    connect(ui_->canvasTernary, &TernaryCanvas::interpolationRequested, this, &MainWindow::addInterpolationQuery);
    connect(ui_->treeProject->selectionModel(), &QItemSelectionModel::currentChanged, this, &MainWindow::selectProjectNode);
    connect(grid_model_, &GridTableModel::bridgeStatus, this, [this](const QString& message, bool success) { reportBridgeStatus(message, success); if (!success) editor_commit_failed_ = true; });
    connect(grid_model_, &GridTableModel::documentMutated, this, [this] { rebuildFromRust(selected_grid_); });
    connect(ui_->editProjectTitle, &QLineEdit::editingFinished, this, &MainWindow::commitTitle);
    connect(ui_->editCornerA, &QLineEdit::editingFinished, this, &MainWindow::commitComponentA);
    connect(ui_->editCornerB, &QLineEdit::editingFinished, this, &MainWindow::commitComponentB);
    connect(ui_->editCornerC, &QLineEdit::editingFinished, this, &MainWindow::commitComponentC);
    restoreWindowLayout();
    rebuildFromRust();
}

MainWindow::~MainWindow() { saveWindowLayout(); }

void MainWindow::reportBridgeStatus(const QString& message, bool success) {
    ui_->statusMain->showMessage(success ? message : tr("Error: %1").arg(message), 7000);
}

void MainWindow::rebuildFromRust(std::uint32_t preferred_grid) {
    TcqtProjectSummary summary{}; const auto result = tcqt_project_summary(&summary);
    if (!result.success) { reportBridgeStatus(statusText(result), false); return; }
    synchronizing_ = true;
    ui_->editProjectTitle->setText(text(summary.title)); ui_->editCornerA->setText(text(summary.component_a));
    ui_->editCornerB->setText(text(summary.component_b)); ui_->editCornerC->setText(text(summary.component_c));
    selected_grid_ = summary.grid_count == 0 ? 0 : qMin(preferred_grid, summary.grid_count - 1);
    const QStringList component_names{text(summary.component_a), text(summary.component_b), text(summary.component_c)};
    if (summary.grid_count == 0) {
        grid_model_->clear();
    } else {
        grid_model_->load(selected_grid_, component_names);
    }
    synchronizing_ = false;
    rebuildTree(); updateDocumentPresentation(); updateActionState();
    const QStringList components{text(summary.component_a), text(summary.component_b), text(summary.component_c)};
    ui_->canvasTernary->setComponentNames(components);
    QVector<QPointF> vertices;
    TcqtGrid selected{};
    if (summary.grid_count > 0 && tcqt_grid_at(selected_grid_, &selected).success) {
        vertices.reserve(static_cast<qsizetype>(selected.row_count));
        for (std::uint32_t row = 0; row < selected.row_count; ++row) {
            TcqtRow composition{};
            if (tcqt_grid_row_at(selected_grid_, row, &composition).success) vertices.append(QPointF(composition.a, composition.b));
        }
    }
    ui_->canvasTernary->setSourceVertices(vertices);
    refreshViewerFieldSelectors();
    refreshViewerVertices();
    refreshViewerQueries();
    if (viewer_.has_last_valid_projection) viewer_.projection_is_stale = true;
    refreshProjectionCanvas();
    ui_->canvasTernary->setInteractionMode(viewer_.interaction_mode);
    syncViewerPanelControls();
    updateViewerActionState();
    scheduleViewerCalculation();
}

void MainWindow::rebuildTree() {
    const auto previous = ui_->treeProject->currentIndex();
    const auto previous_kind = previous.data(node_kind_role).toInt();
    const auto previous_grid = previous.data(grid_id_role).toUInt();
    const auto previous_phase = previous.data(phase_id_role).toUInt();
    const auto previous_property = previous.data(property_id_role).toUInt();
    const auto previous_field = previous.data(field_id_role).toUInt();

    TcqtProjectSummary summary{};
    if (!tcqt_project_summary(&summary).success) return;


    QVector<TcqtPhase> phases;
    for (std::uint32_t index = 0; index < summary.phase_count; ++index) {
        TcqtPhase phase{};
        if (tcqt_phase_at(index, &phase).success) phases.append(phase);
    }
    QVector<TcqtProperty> properties;
    for (std::uint32_t index = 0; index < summary.property_count; ++index) {
        TcqtProperty property{};
        if (tcqt_property_at(index, &property).success) properties.append(property);
    }

    tree_model_->clear();
    tree_model_->setHorizontalHeaderLabels({tr("Project")});
    auto* project = node(documentLabel(summary), NodeKind::Project);
    project->setToolTip(documentTooltip(summary));
    project->appendRow(node(tr("Title: %1").arg(text(summary.title)), NodeKind::Title));

    auto* corners = node(tr("Corners"), NodeKind::Corner);
    corners->appendRow(node(tr("A: %1").arg(text(summary.component_a)), NodeKind::Corner, 0));
    corners->appendRow(node(tr("B: %1").arg(text(summary.component_b)), NodeKind::Corner));
    corners->appendRow(node(tr("C: %1").arg(text(summary.component_c)), NodeKind::Corner));
    project->appendRow(corners);

    auto* phase_collection = node(tr("Phases"), NodeKind::PhaseCollection);
    for (const auto& phase : phases) {
        phase_collection->appendRow(node(tr("[%1] %2").arg(phase.id).arg(text(phase.name)), NodeKind::Phase, 0, phase.id));
    }
    project->appendRow(phase_collection);

    auto* property_collection = node(tr("Properties"), NodeKind::PropertyCollection);
    for (int index = 0; index < properties.size(); ++index) {
        const auto& property = properties.at(index);
        property_collection->appendRow(node(
            tr("%1%2 (%3)").arg(text(property.name), property.required ? tr(" required") : QString(), text(property.unit)),
            NodeKind::Property, 0, 0, static_cast<std::uint32_t>(property.ordinal)));
    }
    project->appendRow(property_collection);

    auto* grid_collection = node(tr("Grids"), NodeKind::GridCollection);
    for (std::uint32_t grid_index = 0; grid_index < summary.grid_count; ++grid_index) {
        TcqtGrid grid{};
        if (!tcqt_grid_at(grid_index, &grid).success) continue;
        auto* grid_node = node(
            tr("%1 - %2 (%3 rows)").arg(grid.kind == 0 ? tr("Regular") : tr("Irregular"), text(grid.name)).arg(grid.row_count),
            NodeKind::Grid, grid_index);
        grid_node->setToolTip(tr("%1 grid \"%2\"\n%3 rows\n%4 phase/property fields")
                                  .arg(grid.kind == 0 ? tr("Regular") : tr("Irregular"), text(grid.name))
                                  .arg(grid.row_count)
                                  .arg(grid.field_count));

        QVector<TcqtField> fields;
        for (std::uint32_t field_index = 0; field_index < grid.field_count; ++field_index) {
            TcqtField field{};
            if (tcqt_grid_field_at(grid_index, field_index, &field).success) fields.append(field);
        }
        QVector<std::uint32_t> phase_ids;
        for (const auto& field : fields) {
            if (!phase_ids.contains(field.phase_id)) phase_ids.append(field.phase_id);
        }
        std::stable_sort(phase_ids.begin(), phase_ids.end(), [&](std::uint32_t left, std::uint32_t right) {
            auto phase_order = [&](std::uint32_t id) {
                for (int i = 0; i < phases.size(); ++i) if (phases.at(i).id == id) return i;
                return static_cast<int>(phases.size());
            };
            return phase_order(left) < phase_order(right);
        });

        for (const auto phase_id : phase_ids) {
            QString phase_name = tr("Unknown phase");
            for (const auto& phase : phases) if (phase.id == phase_id) { phase_name = text(phase.name); break; }
            QVector<TcqtField> phase_fields;
            for (const auto& field : fields) if (field.phase_id == phase_id) phase_fields.append(field);
            auto* phase_node = node(tr("[%1] %2").arg(phase_id).arg(phase_name), NodeKind::GridPhase, grid_index, phase_id);
            phase_node->setToolTip(tr("Phase %1 [%2]\nIncluded in grid \"%3\"\n%4 property fields")
                                       .arg(phase_name).arg(phase_id).arg(text(grid.name)).arg(phase_fields.size()));
            QVector<std::uint32_t> added_fields;
            auto append_field = [&](const TcqtField& field, std::uint32_t property_id) {
                QString property_name = text(field.property);
                QString property_unit;
                for (const auto& property : properties) {
                    if (property.ordinal == property_id || QString::fromUtf8(property.name) == property_name) {
                        property_name = text(property.name);
                        property_unit = text(property.unit);
                        property_id = property.ordinal;
                        break;
                    }
                }
                auto* field_node = node(
                    tr("%1 (%2)").arg(property_name, property_unit), NodeKind::GridField,
                    grid_index, phase_id, property_id, field.index);
                field_node->setToolTip(tr("%1 (%2)\nGrid: %3\nPhase: %4 [%5]")
                                           .arg(property_name, property_unit, text(grid.name), phase_name).arg(phase_id));
                phase_node->appendRow(field_node);
                added_fields.append(field.index);
            };
            for (const auto& property : properties) {
                for (const auto& field : phase_fields) {
                    if (added_fields.contains(field.index)) continue;
                    if (QString::fromUtf8(field.property) == text(property.name)) {
                        append_field(field, property.ordinal);
                    }
                }
            }
            for (const auto& field : phase_fields) {
                if (!added_fields.contains(field.index)) append_field(field, 0);
            }
            grid_node->appendRow(phase_node);
        }
        grid_collection->appendRow(grid_node);
    }
    project->appendRow(grid_collection);
    tree_model_->appendRow(project);
    ui_->treeProject->expandAll();

    QModelIndex restored;
    std::function<void(const QModelIndex&)> find_node = [&](const QModelIndex& parent) {
        if (restored.isValid()) return;
        for (int row = 0; row < tree_model_->rowCount(parent); ++row) {
            const auto candidate = tree_model_->index(row, 0, parent);
            if (candidate.data(node_kind_role).toInt() == previous_kind
                && candidate.data(grid_id_role).toUInt() == previous_grid
                && candidate.data(phase_id_role).toUInt() == previous_phase
                && candidate.data(property_id_role).toUInt() == previous_property
                && candidate.data(field_id_role).toUInt() == previous_field) {
                restored = candidate;
                return;
            }
            find_node(candidate);
            if (restored.isValid()) return;
        }
    };
    if (previous.isValid()) find_node({});
    if (restored.isValid()) {
        const auto old_synchronizing = synchronizing_;
        synchronizing_ = true;
        ui_->treeProject->setCurrentIndex(restored);
        synchronizing_ = old_synchronizing;
    }
}
void MainWindow::updateActionState() {
    TcqtProjectSummary summary{};
    if (!tcqt_project_summary(&summary).success) return;
    const auto selected = ui_->treeProject->currentIndex();
    const auto kind = selected.data(node_kind_role).toInt();
    const bool grid_selected = isGridNode(static_cast<NodeKind>(kind));
    const bool phase_selected = kind == static_cast<int>(NodeKind::Phase);
    ui_->actionFileSave->setEnabled(true);
    ui_->actionFileSaveAs->setEnabled(true);
    ui_->actionGridRemove->setEnabled(grid_selected);
    ui_->actionGridDuplicate->setEnabled(grid_selected);
    ui_->actionGridRename->setEnabled(grid_selected);
    ui_->buttonRemovePhase->setEnabled(phase_selected);
    ui_->buttonAddIrregularRow->setEnabled(grid_selected && !grid_model_->isRegular());
    ui_->actionGridValidate->setEnabled(grid_selected);
    ui_->actionGridRecalculate->setEnabled(grid_selected && summary.calculation_available);
    const auto selection = ui_->tableGridValues->selectionModel()->selection();
    const bool has_table_selection = !selection.isEmpty() || ui_->tableGridValues->currentIndex().isValid();
    const bool table_loaded = grid_model_->rowCount() > 0 && grid_model_->columnCount() > 0;
    const bool has_clipboard = !QApplication::clipboard()->text().isEmpty();
    ui_->actionGridCopy->setEnabled(grid_selected && has_table_selection);
    ui_->actionGridPaste->setEnabled(grid_selected && table_loaded && has_clipboard && has_table_selection);
    ui_->actionGridExtrapolate->setEnabled(grid_selected && table_loaded && grid_model_->isRegular());
    // Projection calculation is automatic; document saveability remains independent of readiness.

    updateViewerActionState();
}
void MainWindow::updateDocumentPresentation() {
    TcqtProjectSummary summary{};
    if (!tcqt_project_summary(&summary).success) return;
    const auto root = tree_model_->index(0, 0);
    if (root.isValid()) {
        tree_model_->setData(root, documentLabel(summary), Qt::DisplayRole);
        tree_model_->setData(root, documentTooltip(summary), Qt::ToolTipRole);
    }
    updateWindowTitle();
}
void MainWindow::updateWindowTitle() {
    TcqtProjectSummary summary{};
    if (!tcqt_project_summary(&summary).success) return;
    setWindowTitle(tr("Ternary Contours \u2014 %1").arg(documentLabel(summary)));
}

void MainWindow::commitPendingEditors() {
    editor_commit_failed_ = false;
    if (auto* focus = QApplication::focusWidget()) focus->clearFocus();
    ui_->tableGridValues->setFocus(Qt::OtherFocusReason);
}

bool MainWindow::confirmDocumentReplacement(const QString& action) {
    TcqtProjectSummary summary{};
    if (!tcqt_project_summary(&summary).success || !summary.dirty) return true;
    const auto choice = QMessageBox::warning(
        this,
        tr("Unsaved changes"),
        tr("The current document has unsaved changes.\n\nSave before %1?").arg(action),
        QMessageBox::Save | QMessageBox::Discard | QMessageBox::Cancel,
        QMessageBox::Save);
    if (choice == QMessageBox::Cancel) return false;
    if (choice == QMessageBox::Save) return performSave(false);
    return true;
}

void MainWindow::newDocument() {
    if (!confirmDocumentReplacement(tr("creating a new project"))) return;
    const auto result = tcqt_new_document();
    reportBridgeStatus(statusText(result), result.success);
    if (result.success) {
        viewer_.has_last_valid_projection = false;
        viewer_.projection_is_stale = false;
        viewer_.selected_rows.clear();
        viewer_.queries.clear();
        ui_->canvasTernary->setProjectionPaths({});
        ui_->canvasTernary->setQueries({});
        rebuildFromRust();
    }
}

void MainWindow::openDocument() {
    if (!confirmDocumentReplacement(tr("opening another project"))) return;
    TcqtProjectSummary summary{};
    tcqt_project_summary(&summary);
    const auto initial = text(summary.path);
    const auto path = QFileDialog::getOpenFileName(
        this,
        tr("Open Ternary Contour Table"),
        initial.isEmpty() ? QString() : QFileInfo(initial).absolutePath(),
        tr("Ternary Contour Table (*.tct)"));
    if (path.isEmpty()) return;
    const auto encoded = path.toUtf8();
    const auto result = tcqt_open_document(encoded.constData());
    if (!result.success) {
        QMessageBox::critical(this, tr("Could not open project"),
                              tr("Path:\n%1\n\nReason:\n%2\n\nThe current document was not changed.")
                                  .arg(path, statusText(result)));
        return;
    }
    viewer_.has_last_valid_projection = false;
    viewer_.projection_is_stale = false;
    viewer_.selected_rows.clear();
    viewer_.queries.clear();
    ui_->canvasTernary->setProjectionPaths({});
    ui_->canvasTernary->setQueries({});
    rebuildFromRust();
    TcqtProjectSummary opened{};
    tcqt_project_summary(&opened);
    reportBridgeStatus(
        tr("Opened %1 - %2").arg(QFileInfo(path).fileName(), documentStatusLabel(opened)),
        true);
}

bool MainWindow::saveToPath(const QString& path) {
    if (path.isEmpty()) return false;
    const auto encoded = path.toUtf8();
    const auto result = tcqt_save_document(encoded.constData());
    if (result.outcome != 0) {
        const auto title = result.outcome == 1
            ? tr("Cannot save project")
            : result.outcome == 2 ? tr("Could not serialize project") : tr("Could not save project");
        auto details = text(result.message);
        if (result.outcome == 3) {
            details += tr("\n\nPath:\n%1").arg(path);
        }
        details += tr("\n\nNo file was written.\nThe project remains open and modified.");
        QMessageBox::critical(this, title, details);
        ui_->statusMain->showMessage(title, 10000);
        return false;
    }
    rebuildFromRust(selected_grid_);
    reportBridgeStatus(text(result.message), true);
    return true;
}

bool MainWindow::performSave(bool save_as) {
    commitPendingEditors();
    TcqtProjectSummary summary{};
    if (!tcqt_project_summary(&summary).success) return false;
    if (editor_commit_failed_) {
        QMessageBox::critical(this, tr("Cannot save project"), tr("An active editor contains invalid input.\n\nNo file was written."));
        return false;
    }
    if (!summary.saveable) {
        QMessageBox::critical(
            this,
            tr("Cannot save project"),
            tr("The project contains invalid data:\n\n%1\n\nNo file was written.\nThe project remains open and modified.")
                .arg(text(summary.blocking_reason)));
        return false;
    }
    QString path = text(summary.path);
    if (save_as || path.isEmpty()) {
        path = QFileDialog::getSaveFileName(
            this,
            tr("Save Ternary Contour Table"),
            path,
            tr("Ternary Contour Table (*.tct)"));
        if (path.isEmpty()) return false;
        path = normalizeTctPath(path);
    }
    return saveToPath(path);
}

void MainWindow::saveDocument() {
    performSave(false);
}
void MainWindow::saveDocumentAs() {
    performSave(true);
}
void MainWindow::exportPng() {
    const auto path = QFileDialog::getSaveFileName(this, tr("Export PNG"), {}, tr("PNG image (*.png)"));
    if (path.isEmpty()) return; const auto encoded = path.toUtf8(); const auto result = tcqt_export_plot(encoded.constData(), 0); reportBridgeStatus(statusText(result), result.success);
}
void MainWindow::exportSvg() {
    const auto path = QFileDialog::getSaveFileName(this, tr("Export SVG"), {}, tr("SVG image (*.svg)"));
    if (path.isEmpty()) return; const auto encoded = path.toUtf8(); const auto result = tcqt_export_plot(encoded.constData(), 1); reportBridgeStatus(statusText(result), result.success);
}
void MainWindow::exportLinesCsv() {
    const auto path = QFileDialog::getSaveFileName(this, tr("Export contour lines CSV"), {}, tr("CSV files (*.csv)"));
    if (path.isEmpty()) return; const auto encoded = path.toUtf8(); const auto result = tcqt_export_lines_csv(encoded.constData()); reportBridgeStatus(statusText(result), result.success);
}void MainWindow::addGrid(bool regular) {
    QDialog dialog(this);
    Ui::AddGridDialog form;
    form.setupUi(&dialog);
    form.radioAddRegularGrid->setChecked(regular);
    form.spinAddGridSubdivisions->setRange(min_regular_subdivisions, max_regular_subdivisions);
    form.spinAddGridSubdivisions->setValue(default_regular_subdivisions);
    form.spinAddGridSubdivisions->setToolTip(
        tr("Regular-grid subdivisions range from %1 to %2.\nHigher resolutions create quadratically more grid points.")
            .arg(min_regular_subdivisions)
            .arg(max_regular_subdivisions));
    form.spinAddGridSubdivisions->setEnabled(regular);
    auto* subdivision_editor = form.spinAddGridSubdivisions->findChild<QLineEdit*>();

    const auto updateRegularHint = [&]() {
        const auto regular_selected = form.radioAddRegularGrid->isChecked();
        bool valid = false;
        std::uint64_t point_count = 0;
        double step = 0.0;
        if (regular_selected) {
            bool parsed = false;
            const auto text_value = subdivision_editor->text().trimmed();
            const auto subdivisions = text_value.toUInt(&parsed);
            const auto max_subdivisions = static_cast<std::uint64_t>(form.spinAddGridSubdivisions->maximum());
            const auto n = static_cast<std::uint64_t>(subdivisions);
            if (parsed && n > 0 && n <= max_subdivisions && n < std::numeric_limits<std::uint64_t>::max() - 2) {
                const auto left = n + 1;
                const auto right = n + 2;
                if (left <= std::numeric_limits<std::uint64_t>::max() / right) {
                    point_count = (left * right) / 2;
                    step = 1.0 / static_cast<double>(n);
                    valid = true;
                }
            }
        }
        if (valid) {
            const auto step_text = QString::number(step, 'g', 8);
            const auto percent_text = QString::number(step * 100.0, 'g', 8);
            form.labelAddGridStepValue->setText(tr("Step size: %1 (%2%)").arg(step_text, percent_text));
            const auto point_text = QLocale(QLocale::English, QLocale::UnitedStates).toString(point_count);
            form.labelAddGridPointsValue->setText(
                tr("Grid points: %1\nAllowed subdivisions: %2-%3")
                    .arg(point_text)
                    .arg(min_regular_subdivisions)
                    .arg(max_regular_subdivisions));
        } else {
            form.labelAddGridStepValue->setText(tr("Step size: -"));
            form.labelAddGridPointsValue->setText(tr("Grid points: -\nAllowed subdivisions: %1-%2").arg(min_regular_subdivisions).arg(max_regular_subdivisions));
        }
        if (auto* ok = form.addGridButtonBox->button(QDialogButtonBox::Ok)) {
            ok->setEnabled(!regular_selected || valid);
        }
        return valid;
    };

    connect(form.spinAddGridSubdivisions, qOverload<int>(&QSpinBox::valueChanged), &dialog, [&](int) { updateRegularHint(); });
    connect(subdivision_editor, &QLineEdit::textChanged, &dialog, [&](const QString&) { updateRegularHint(); });
    connect(form.radioAddRegularGrid, &QRadioButton::toggled, &dialog, [&](bool checked) {
        form.spinAddGridSubdivisions->setEnabled(checked);
        updateRegularHint();
    });
    connect(form.addGridButtonBox, &QDialogButtonBox::accepted, &dialog, [&]() {
        if (!form.radioAddRegularGrid->isChecked() || updateRegularHint()) dialog.accept();
    });
    connect(form.addGridButtonBox, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
    updateRegularHint();
    if (dialog.exec() != QDialog::Accepted) return;
    const auto name = form.editAddGridName->text().toUtf8();
    const auto result = form.radioAddRegularGrid->isChecked() ? tcqt_add_regular_grid(name.constData(), static_cast<std::uint32_t>(form.spinAddGridSubdivisions->value())) : tcqt_add_irregular_grid(name.constData());
    reportBridgeStatus(statusText(result), result.success);
    if (result.success) {
        viewer_.has_last_valid_projection = false;
        viewer_.projection_is_stale = false;
        viewer_.selected_rows.clear();
        viewer_.queries.clear();
        ui_->canvasTernary->setProjectionPaths({});
        ui_->canvasTernary->setQueries({});
        rebuildFromRust();
    }
}void MainWindow::removeSelectedGrid() {
    const auto index = ui_->treeProject->currentIndex(); if (!isGridNode(static_cast<NodeKind>(index.data(node_kind_role).toInt()))) return;
    if (QMessageBox::question(this, tr("Remove grid"), tr("Remove the selected grid and its values?")) != QMessageBox::Yes) return;
    const auto result = tcqt_remove_grid(index.data(grid_id_role).toUInt()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust();
}
void MainWindow::duplicateSelectedGrid() {
    const auto index = ui_->treeProject->currentIndex(); if (!isGridNode(static_cast<NodeKind>(index.data(node_kind_role).toInt()))) return;
    const auto result = tcqt_duplicate_grid(index.data(grid_id_role).toUInt()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_);
}
void MainWindow::renameSelectedGrid() {
    const auto index = ui_->treeProject->currentIndex(); if (!isGridNode(static_cast<NodeKind>(index.data(node_kind_role).toInt()))) return;
    TcqtGrid grid{}; tcqt_grid_at(index.data(grid_id_role).toUInt(), &grid);
    bool accepted = false; const auto name = QInputDialog::getText(this, tr("Rename grid"), tr("Grid name:"), QLineEdit::Normal, text(grid.name), &accepted); if (!accepted) return;
    const auto encoded = name.toUtf8(); const auto result = tcqt_rename_grid(index.data(grid_id_role).toUInt(), encoded.constData()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_);
}void MainWindow::addPhase() {
    bool accepted = false; const auto name = QInputDialog::getText(this, tr("Add phase"), tr("Phase name:"), QLineEdit::Normal, {}, &accepted); if (!accepted) return;
    const auto encoded = name.toUtf8(); const auto result = tcqt_add_phase(encoded.constData()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_);
}
void MainWindow::removeSelectedPhase() {
    const auto index = ui_->treeProject->currentIndex(); if (index.data(node_kind_role).toInt() != static_cast<int>(NodeKind::Phase)) return;
    if (QMessageBox::question(this, tr("Remove phase"), tr("Removing this phase also removes its grid fields and values. Continue?")) != QMessageBox::Yes) return;
    const auto result = tcqt_remove_phase(index.data(phase_id_role).toUInt()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_);
}
void MainWindow::addProperty() {
    bool accepted = false; const auto name = QInputDialog::getText(this, tr("Add property"), tr("Property name:"), QLineEdit::Normal, {}, &accepted); if (!accepted) return;
    const auto unit = QInputDialog::getText(this, tr("Property unit"), tr("Unit:"), QLineEdit::Normal, tr("1"), &accepted); if (!accepted) return;
    const auto name_encoded = name.toUtf8(); const auto unit_encoded = unit.toUtf8(); const auto result = tcqt_add_property(name_encoded.constData(), unit_encoded.constData(), false); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_);
}
void MainWindow::addIrregularRow() { const auto result = tcqt_add_irregular_row(selected_grid_); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_); }

void MainWindow::selectProjectNode(const QModelIndex& index) {
    if (!index.isValid() || synchronizing_) return;
    const auto kind = static_cast<NodeKind>(index.data(node_kind_role).toInt());
    if (isGridNode(kind)) {
        selected_grid_ = index.data(grid_id_role).toUInt();
        selected_phase_id_ = index.data(phase_id_role).toUInt();
        const auto requested_field = index.data(field_id_role).toUInt();
        rebuildFromRust(selected_grid_);

        std::uint32_t field_index = requested_field;
        bool found = false;
        TcqtGrid grid{};
        if (tcqt_grid_at(selected_grid_, &grid).success) {
            for (std::uint32_t i = 0; i < grid.field_count; ++i) {
                TcqtField field{};
                if (!tcqt_grid_field_at(selected_grid_, i, &field).success) continue;
                if (kind == NodeKind::GridField && field.index == requested_field) {
                    field_index = i;
                    found = true;
                    break;
                }
                if (kind == NodeKind::GridPhase && field.phase_id == selected_phase_id_ && !found) {
                    field_index = i;
                    found = true;
                }
            }
        }
        if (found && grid_model_->rowCount() > 0) {
            const auto cell = grid_model_->index(0, static_cast<int>(field_index) + 3);
            ui_->tableGridValues->setCurrentIndex(cell);
            ui_->tableGridValues->scrollTo(cell, QAbstractItemView::PositionAtCenter);
        }
    } else if (kind == NodeKind::Phase) {
        selected_phase_id_ = index.data(phase_id_role).toUInt();
    }
    updateActionState();
}

void MainWindow::commitTitle() { if (synchronizing_) return; const auto value = ui_->editProjectTitle->text().toUtf8(); const auto result = tcqt_set_title(value.constData()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_); }
void MainWindow::commitComponentA() { if (synchronizing_) return; const auto value = ui_->editCornerA->text().toUtf8(); const auto result = tcqt_set_component(0, value.constData()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_); }
void MainWindow::commitComponentB() { if (synchronizing_) return; const auto value = ui_->editCornerB->text().toUtf8(); const auto result = tcqt_set_component(1, value.constData()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_); }
void MainWindow::commitComponentC() { if (synchronizing_) return; const auto value = ui_->editCornerC->text().toUtf8(); const auto result = tcqt_set_component(2, value.constData()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_); }
void MainWindow::copyGridSelection() {
    const auto ranges = ui_->tableGridValues->selectionModel()->selection();
    QItemSelectionRange range;
    if (ranges.size() > 1) {
        reportBridgeStatus(tr("Copy requires one contiguous table selection."), false);
        return;
    }
    if (ranges.size() == 1) {
        range = ranges.first();
    } else if (ui_->tableGridValues->currentIndex().isValid()) {
        range = QItemSelectionRange(ui_->tableGridValues->currentIndex());
    } else {
        reportBridgeStatus(tr("Select a table range before copying."), false);
        return;
    }
    QStringList lines;
    for (int row = range.top(); row <= range.bottom(); ++row) {
        QStringList cells;
        for (int column = range.left(); column <= range.right(); ++column) {
            cells.append(grid_model_->data(grid_model_->index(row, column), Qt::DisplayRole).toString());
        }
        lines.append(cells.join(QChar('\t')));
    }
    QApplication::clipboard()->setText(lines.join(QChar('\n')));
    reportBridgeStatus(tr("Copied %1 rows x %2 columns.").arg(range.height()).arg(range.width()), true);
}

void MainWindow::pasteGridClipboard() {
    auto* focus = QApplication::focusWidget();
    if (focus && (qobject_cast<QLineEdit*>(focus) || qobject_cast<QTextEdit*>(focus))) {
        return;
    }
    if (grid_model_->rowCount() == 0 || grid_model_->columnCount() == 0) {
        reportBridgeStatus(tr("Select a loaded grid before pasting."), false);
        return;
    }
    const auto ranges = ui_->tableGridValues->selectionModel()->selection();
    if (ranges.size() > 1) {
        QMessageBox::critical(this, tr("Could not paste grid data"),
                              tr("The destination selection is discontiguous. Select one contiguous range.\n\nNo cells were changed."));
        return;
    }
    QModelIndex anchor;
    if (ranges.size() == 1) {
        anchor = ranges.first().topLeft();
    } else {
        anchor = ui_->tableGridValues->currentIndex();
    }
    if (!anchor.isValid()) {
        reportBridgeStatus(tr("Select a destination cell before pasting."), false);
        return;
    }
    const auto clipboard = QApplication::clipboard()->text();
    if (clipboard.isEmpty()) {
        reportBridgeStatus(tr("Clipboard is empty."), false);
        return;
    }
    const auto encoded = clipboard.toUtf8();
    const auto result = tcqt_paste_grid_tsv(
        grid_model_->gridIndex(),
        static_cast<std::uint32_t>(anchor.row()),
        static_cast<std::uint32_t>(anchor.column()),
        encoded.constData());
    if (!result.success) {
        QString details;
        if (result.clipboard_row != 0) {
            details += tr("Clipboard cell:\nRow %1, column %2\n\n")
                           .arg(result.clipboard_row)
                           .arg(result.clipboard_column);
        }
        if (result.target_row != 0) {
            const auto header = result.target_column == 0
                ? QString()
                : grid_model_->headerData(static_cast<int>(result.target_column - 1), Qt::Horizontal).toString();
            details += tr("Destination:\nGrid row %1, column %2%3\n\n")
                           .arg(result.target_row)
                           .arg(result.target_column)
                           .arg(header.isEmpty() ? QString() : tr(" (%1)").arg(header));
        }
        details += tr("Reason:\n%1\n\nNo cells were changed.").arg(text(result.message));
        QMessageBox::critical(this, tr("Could not paste grid data"), details);
        reportBridgeStatus(text(result.message), false);
        return;
    }
    const auto start_row = anchor.row();
    const auto start_column = anchor.column();
    rebuildFromRust(selected_grid_);
    if (result.rows_pasted > 0 && result.columns_pasted > 0) {
        const auto top = grid_model_->index(start_row, start_column);
        const auto bottom = grid_model_->index(
            start_row + static_cast<int>(result.rows_pasted) - 1,
            start_column + static_cast<int>(result.columns_pasted) - 1);
        ui_->tableGridValues->selectionModel()->select(
            QItemSelection(top, bottom), QItemSelectionModel::ClearAndSelect);
        ui_->tableGridValues->setCurrentIndex(top);
        ui_->tableGridValues->scrollTo(top, QAbstractItemView::PositionAtCenter);
    }
    auto message = text(result.message);
    if (result.header_skipped) message += tr(" Header row skipped.");
    reportBridgeStatus(message, true);
}

void MainWindow::extrapolateSelectedRegularField() {
    if (!grid_model_->isRegular()) {
        QMessageBox::information(this, tr("Extrapolate missing values"),
            tr("Automatic mesh extrapolation is currently available for regular grids only."));
        return;
    }
    TcqtGrid grid{};
    if (!tcqt_grid_at(selected_grid_, &grid).success) {
        reportBridgeStatus(tr("Select a regular grid before extrapolating."), false);
        return;
    }

    QDialog dialog(this);
    dialog.setWindowTitle(tr("Extrapolate Missing Values"));
    auto* layout = new QFormLayout(&dialog);
    auto* field = new QComboBox(&dialog);
    const auto current = ui_->tableGridValues->currentIndex();
    if (current.isValid() && current.column() >= 3) {
        field->addItem(grid_model_->headerData(current.column(), Qt::Horizontal, Qt::DisplayRole).toString(), current.column() - 3);
    }
    field->addItem(tr("All fields"), invalid_viewer_abi);
    auto* method = new QComboBox(&dialog);
    method->addItem(tr("Akima"), 0U);
    method->addItem(tr("Makima"), 1U);
    method->addItem(tr("PCHIP"), 2U);
    method->addItem(tr("Steffen"), 3U);
    method->setCurrentIndex(3);
    auto* layers = new QSpinBox(&dialog); layers->setRange(1, 100); layers->setValue(1);
    auto* support = new QSpinBox(&dialog); support->setRange(1, 32); support->setValue(3);
    auto* spread = new QLineEdit(&dialog);
    spread->setPlaceholderText(tr("Optional"));
    auto* result = new QLabel(tr("Preview does not modify the document."), &dialog);
    result->setWordWrap(true);
    layout->addRow(tr("Grid"), new QLabel(text(grid.name), &dialog));
    layout->addRow(tr("Fields"), field);
    layout->addRow(tr("Method"), method);
    layout->addRow(tr("Maximum layers"), layers);
    layout->addRow(tr("Minimum directional support"), support);
    layout->addRow(tr("Maximum directional spread"), spread);
    layout->addRow(result);
    auto* buttons = new QDialogButtonBox(QDialogButtonBox::Cancel, &dialog);
    auto* preview = buttons->addButton(tr("Preview"), QDialogButtonBox::ActionRole);
    auto* materialize = buttons->addButton(tr("Materialize"), QDialogButtonBox::AcceptRole);
    materialize->setEnabled(false);
    layout->addRow(buttons);
    auto make_options = [&]() -> std::optional<TcqtMeshExtrapolationOptions> {
        TcqtMeshExtrapolationOptions options{};
        options.grid_index = selected_grid_;
        options.field_index = field->currentData().toUInt();
        options.method = method->currentData().toUInt();
        options.maximum_layers = layers->value();
        options.minimum_directional_support = support->value();
        if (!spread->text().trimmed().isEmpty()) {
            bool ok = false;
            const auto value = QLocale::c().toDouble(spread->text(), &ok);
            if (!ok || !std::isfinite(value) || value < 0.0) {
                result->setText(tr("Maximum directional spread must be a finite non-negative number."));
                return std::nullopt;
            }
            options.has_maximum_directional_spread = true;
            options.maximum_directional_spread = value;
        }
        return options;
    };
    connect(preview, &QPushButton::clicked, &dialog, [&]() {
        const auto options = make_options();
        if (!options) return;
        TcqtMeshExtrapolationSummary summary{};
        const auto status = tcqt_preview_regular_mesh_extrapolation(&*options, &summary);
        if (!status.success) {
            result->setText(statusText(status));
            materialize->setEnabled(false);
            return;
        }
        result->setText(text(summary.message));
        materialize->setEnabled(summary.values_proposed > 0);
    });
    connect(materialize, &QPushButton::clicked, &dialog, [&]() {
        TcqtMeshExtrapolationSummary summary{};
        const auto status = tcqt_materialize_regular_mesh_extrapolation(&summary);
        if (!status.success) {
            result->setText(statusText(status));
            return;
        }
        rebuildFromRust(selected_grid_);
        scheduleViewerCalculation();
        reportBridgeStatus(text(summary.message), true);
        dialog.accept();
    });
    connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
    dialog.exec();
}
void MainWindow::extrapolateViewerPhase() {
    showViewerMeshExtrapolationDialog(mesh_scope_phase);
}

void MainWindow::extrapolateViewerTargets(const QVector<std::uint32_t>& rows) {
    showViewerMeshExtrapolationDialog(mesh_scope_targets, rows);
}

void MainWindow::showViewerMeshExtrapolationDialog(
    std::uint32_t scope,
    const QVector<std::uint32_t>& rows)
{
    if (viewer_.property.isEmpty()) {
        reportBridgeStatus(tr("Select a grid, phase, and property before extrapolating."), false);
        return;
    }
    TcqtGrid grid{};
    if (!tcqt_grid_at(viewer_.grid_index, &grid).success || grid.kind != 0) {
        QMessageBox::information(this, tr("Extrapolate missing values"),
            tr("Automatic mesh extrapolation is currently available for regular grids only."));
        return;
    }

    QDialog dialog(this);
    dialog.setWindowTitle(scope == mesh_scope_targets
        ? tr("Extrapolate Selected Vertex")
        : tr("Extrapolate Selected Phase"));
    auto* layout = new QVBoxLayout(&dialog);
    auto* form = new QFormLayout();
    layout->addLayout(form);
    form->addRow(tr("Grid"), new QLabel(text(grid.name), &dialog));
    form->addRow(tr("Phase"), new QLabel(ui_->comboViewerPhase->currentText(), &dialog));
    form->addRow(tr("Property"), new QLabel(viewer_.property, &dialog));
    if (scope == mesh_scope_targets) {
        form->addRow(tr("Requested rows"), new QLabel([&rows] {
            QStringList labels;
            for (const auto row : rows) labels << QString::number(row + 1);
            return labels.join(QStringLiteral(", "));
        }(), &dialog));
    }
    auto* phase_scope = new QComboBox(&dialog);
    if (scope == mesh_scope_phase) {
        phase_scope->addItem(tr("Current property"), false);
        phase_scope->addItem(tr("All properties for selected phase"), true);
        form->addRow(tr("Scope"), phase_scope);
    }
    auto* method = new QComboBox(&dialog);
    method->addItem(tr("Akima"), 0U);
    method->addItem(tr("Makima"), 1U);
    method->addItem(tr("PCHIP"), 2U);
    method->addItem(tr("Steffen"), 3U);
    method->setCurrentIndex(3);
    auto* layers = new QSpinBox(&dialog); layers->setRange(1, 100); layers->setValue(1);
    auto* support = new QSpinBox(&dialog); support->setRange(1, 32); support->setValue(3);
    auto* spread = new QLineEdit(&dialog); spread->setPlaceholderText(tr("Optional"));
    auto* minimum = new QLineEdit(&dialog); minimum->setPlaceholderText(tr("Optional"));
    auto* maximum = new QLineEdit(&dialog); maximum->setPlaceholderText(tr("Optional"));
    form->addRow(tr("Method"), method);
    form->addRow(tr("Maximum layers"), layers);
    form->addRow(tr("Minimum directional support"), support);
    form->addRow(tr("Maximum directional spread"), spread);
    form->addRow(tr("Minimum value"), minimum);
    form->addRow(tr("Maximum value"), maximum);

    auto* result = new QLabel(tr("Preview does not modify the document."), &dialog);
    result->setWordWrap(true);
    layout->addWidget(result);
    auto* preview_table = new QTableWidget(&dialog);
    preview_table->setObjectName(QStringLiteral("tableViewerExtrapolationPreview"));
    preview_table->setColumnCount(13);
    preview_table->setHorizontalHeaderLabels({tr("Field"), tr("Row"), tr("A"), tr("B"), tr("C"), tr("Old state"), tr("Proposed value"), tr("EX layer"), tr("Method"), tr("Support"), tr("Spread"), tr("Status"), tr("Directional estimates")});
    preview_table->setEditTriggers(QAbstractItemView::NoEditTriggers);
    preview_table->setSelectionBehavior(QAbstractItemView::SelectRows);
    preview_table->horizontalHeader()->setStretchLastSection(true);
    preview_table->setMinimumHeight(230);
    layout->addWidget(preview_table);
    auto* buttons = new QDialogButtonBox(QDialogButtonBox::Cancel, &dialog);
    auto* preview = buttons->addButton(tr("Preview"), QDialogButtonBox::ActionRole);
    auto* materialize = buttons->addButton(tr("Materialize"), QDialogButtonBox::AcceptRole);
    materialize->setEnabled(false);
    layout->addWidget(buttons);

    const auto method_name = [](std::uint32_t code) {
        switch (code) { case 0: return QStringLiteral("Akima"); case 1: return QStringLiteral("Makima"); case 2: return QStringLiteral("PCHIP"); case 3: return QStringLiteral("Steffen"); default: return QStringLiteral("-"); }
    };
    const auto state_name = [](std::uint32_t code) {
        switch (code) { case 0: return QObject::tr("Calculated"); case 2: return QObject::tr("Cut-off"); case 4: return QObject::tr("Extrapolated"); default: return QObject::tr("Missing"); }
    };
    const auto read_optional = [result](QLineEdit* editor, bool* present, double* value, const QString& label) {
        const auto input = editor->text().trimmed();
        *present = !input.isEmpty();
        if (!*present) return true;
        bool ok = false;
        *value = QLocale::c().toDouble(input, &ok);
        if (!ok || !std::isfinite(*value)) { result->setText(QObject::tr("%1 must be a finite number.").arg(label)); return false; }
        return true;
    };
    auto make_options = [&]() -> std::optional<TcqtMeshExtrapolationOptions> {
        TcqtMeshExtrapolationOptions options{};
        options.grid_index = viewer_.grid_index;
        options.field_index = viewer_.field_index;
        options.phase_id = viewer_.phase_id;
        options.scope = scope;
        options.all_phase_properties = scope == mesh_scope_phase && phase_scope->currentData().toBool();
        options.target_rows = rows.isEmpty() ? nullptr : rows.constData();
        options.target_row_count = static_cast<std::uint32_t>(rows.size());
        options.method = method->currentData().toUInt();
        options.maximum_layers = layers->value();
        options.minimum_directional_support = support->value();
        if (!read_optional(spread, &options.has_maximum_directional_spread, &options.maximum_directional_spread, tr("Maximum directional spread"))) return std::nullopt;
        if (options.has_maximum_directional_spread && options.maximum_directional_spread < 0.0) { result->setText(tr("Maximum directional spread must be non-negative.")); return std::nullopt; }
        if (!read_optional(minimum, &options.has_minimum_value, &options.minimum_value, tr("Minimum value"))) return std::nullopt;
        if (!read_optional(maximum, &options.has_maximum_value, &options.maximum_value, tr("Maximum value"))) return std::nullopt;
        if (options.has_minimum_value && options.has_maximum_value && options.minimum_value > options.maximum_value) { result->setText(tr("Minimum value must not exceed maximum value.")); return std::nullopt; }
        return options;
    };
    connect(preview, &QPushButton::clicked, &dialog, [&] {
        const auto options = make_options();
        if (!options) return;
        TcqtMeshExtrapolationSummary summary{};
        const auto status = tcqt_preview_regular_mesh_extrapolation(&*options, &summary);
        if (!status.success) { result->setText(statusText(status)); materialize->setEnabled(false); return; }
        result->setText(text(summary.message));
        std::uint32_t count = 0;
        const auto count_status = tcqt_mesh_extrapolation_preview_row_count(&count);
        if (!count_status.success) { result->setText(statusText(count_status)); materialize->setEnabled(false); return; }
        preview_table->setRowCount(static_cast<int>(count));
        for (std::uint32_t index = 0; index < count; ++index) {
            TcqtMeshExtrapolationPreviewRow row{};
            const auto row_status = tcqt_mesh_extrapolation_preview_row_at(index, &row);
            if (!row_status.success) { result->setText(statusText(row_status)); materialize->setEnabled(false); return; }
            const auto set = [preview_table, index](int column, const QString& value) { preview_table->setItem(static_cast<int>(index), column, new QTableWidgetItem(value)); };
            set(0, text(row.property)); set(1, QString::number(row.row_index + 1));
            set(2, QLocale::c().toString(row.a, 'g', 8)); set(3, QLocale::c().toString(row.b, 'g', 8)); set(4, QLocale::c().toString(row.c, 'g', 8));
            set(5, state_name(row.old_state)); set(6, row.has_value ? QLocale::c().toString(row.value, 'g', 12) : QStringLiteral("-"));
            set(7, row.has_value ? QStringLiteral("EX%1").arg(row.layer) : QStringLiteral("-")); set(8, row.has_value ? method_name(row.method) : QStringLiteral("-"));
            set(9, row.has_value ? QString::number(row.support_count) : QStringLiteral("-")); set(10, row.has_value ? QLocale::c().toString(row.spread, 'g', 8) : QStringLiteral("-"));
            set(11, row.status == 0 ? tr("Requested") : row.status == 1 ? tr("Dependency") : row.status == 2 ? tr("Proposed") : tr("Rejected: %1").arg(text(row.reason)));
            set(12, row.has_value ? text(row.directional_estimates) : QString());
        }
        materialize->setEnabled(summary.values_proposed > 0);
    });
    connect(materialize, &QPushButton::clicked, &dialog, [&] {
        TcqtMeshExtrapolationSummary summary{};
        const auto status = tcqt_materialize_regular_mesh_extrapolation(&summary);
        if (!status.success) { result->setText(statusText(status)); return; }
        rebuildFromRust(selected_grid_);
        scheduleViewerCalculation();
        reportBridgeStatus(text(summary.message), true);
        dialog.accept();
    });
    connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
    dialog.resize(1120, 650);
    dialog.exec();
}
void MainWindow::showGridContextMenu(const QPoint& position) {
    updateActionState();
    QMenu menu(this);
    menu.addAction(ui_->actionGridCopy);
    menu.addAction(ui_->actionGridPaste);
    menu.exec(ui_->tableGridValues->mapToGlobal(position));
}

void MainWindow::runRustCalculation() {
    TcqtProjectSummary summary{};
    if (!tcqt_project_summary(&summary).success || !summary.calculation_available) return;
    if (viewer_.calculation_running) {
        viewer_.pending_recalculation = true;
        viewer_.projection_is_stale = viewer_.has_last_valid_projection;
        setViewerCalculationStatus(tr("Settings changed - recalculation pending"));
        return;
    }
    TcqtViewerCalculationState state{};
    const auto state_status = tcqt_viewer_calculation_state(&state);
    if (!state_status.success) {
        reportBridgeStatus(statusText(state_status), false);
        return;
    }
    const auto revision = summary.revision;
    const auto options_revision = state.options_revision;
    const auto generation = ++viewer_.calculation_generation;
    viewer_.options = state.options;
    viewer_.options_revision = options_revision;
    viewer_.active_dataset_revision = revision;
    viewer_.active_options_revision = options_revision;
    viewer_.active_request_generation = generation;
    viewer_.calculation_running = true;
    viewer_.pending_recalculation = false;
    viewer_.projection_is_stale = viewer_.has_last_valid_projection;
    setViewerCalculationStatus(tr("Calculating with sampling %1, %2, step %3...")
        .arg(viewer_.options.sampling_subdivisions)
        .arg(viewer_.options.source_interpolation == abi_source_linear ? tr("Linear") : tr("Cubic alpha"))
        .arg(QLocale::c().toString(viewer_.options.level_step, 'g', 10)));
    updateViewerActionState();
    ui_->statusMain->showMessage(tr("Calculating topology and iso-plots on the Rust worker..."));
    auto* watcher = new QFutureWatcher<TcqtCalculationResult>(this);
    connect(watcher, &QFutureWatcher<TcqtCalculationResult>::finished, this,
            [this, watcher, revision, options_revision, generation] {
        TcqtProjectSummary latest{};
        TcqtViewerCalculationState latest_options{};
        const auto project_status = tcqt_project_summary(&latest);
        const auto options_status = tcqt_viewer_calculation_state(&latest_options);
        const auto result = watcher->result();
        viewer_.calculation_running = false;
        const bool stale = !project_status.success || !options_status.success
            || latest.revision != revision
            || latest_options.options_revision != options_revision
            || generation != viewer_.active_request_generation
            || result.dataset_revision != revision
            || result.options_revision != options_revision
            || result.request_id != generation;
        if (stale) {
            viewer_.projection_is_stale = viewer_.has_last_valid_projection;
            viewer_.pending_recalculation = false;
            setViewerCalculationStatus(tr("A newer setting was committed; restarting calculation..."));
            syncViewerPanelControls();
            updateViewerActionState();
            watcher->deleteLater();
            scheduleViewerCalculation();
            return;
        }
        if (result.success) {
            TcqtProjectionSummary projection{};
            const bool summary_ready = tcqt_projection_summary(&projection).success && projection.available;
            const bool transferred = summary_ready && refreshProjectionCanvas(true);
            if (!transferred) {
                viewer_.projection_is_stale = viewer_.has_last_valid_projection;
                setViewerCalculationStatus(tr("Calculation completed, but projection records could not be transferred to the canvas."), true);
                reportBridgeStatus(tr("Projection records could not be transferred; previous plot remains visible."), false);
            } else {
                viewer_.has_last_valid_projection = true;
                viewer_.projection_is_stale = false;
                const auto source = QLocale::c();
                if (projection.effective_automatic_range) {
                    const QSignalBlocker min_blocker(ui_->editViewerTmin);
                    const QSignalBlocker max_blocker(ui_->editViewerTmax);
                    ui_->editViewerTmin->setText(source.toString(projection.effective_minimum, 'g', 10));
                    ui_->editViewerTmax->setText(source.toString(projection.effective_maximum, 'g', 10));
                }
                auto projection_summary = tr("%1 levels, %2 binary invariants, %3 ternary invariant%4, %5 complete univariants, %6 isotherm paths")
                    .arg(projection.level_count)
                    .arg(projection.binary_invariant_count)
                    .arg(projection.interior_invariant_count)
                    .arg(projection.interior_invariant_count == 1 ? QString() : QStringLiteral("s"))
                    .arg(projection.univariant_count)
                    .arg(projection.contour_path_count);
                if (projection.domain_truncated_univariant_count != 0) {
                    projection_summary += tr("; %1 domain-truncated branch%2 retained as diagnostics")
                        .arg(projection.domain_truncated_univariant_count)
                        .arg(projection.domain_truncated_univariant_count == 1 ? QString() : QStringLiteral("es"));
                }
                ui_->labelViewerLevelPreview->setText(projection_summary);
                setViewerCalculationStatus(projection_summary);
                ui_->labelViewerCalculationStatus->setToolTip(
                    tr("Effective settings\nRange: %1 (%2 to %3), step %4\nSampling: %5\nInterpolation: %6, cubic method %7, partial-domain policy %8, continuation %9\nRegularization: %10, spacing %11\nDataset revision %12, options revision %13, request %14")
                        .arg(projection.effective_automatic_range ? tr("automatic") : tr("manual"))
                        .arg(source.toString(projection.effective_minimum, 'g', 10))
                        .arg(source.toString(projection.effective_maximum, 'g', 10))
                        .arg(source.toString(projection.effective_level_step, 'g', 10))
                        .arg(projection.effective_sampling_subdivisions)
                        .arg(projection.effective_source_interpolation)
                        .arg(projection.effective_cubic_method)
                        .arg(projection.effective_partial_domain_policy)
                        .arg(projection.effective_continuation)
                        .arg(projection.effective_regularize ? tr("enabled") : tr("disabled"))
                        .arg(source.toString(projection.effective_regularization_spacing, 'g', 10))
                        .arg(projection.dataset_revision)
                        .arg(projection.options_revision)
                        .arg(projection.request_id));
                refreshViewerQueries();
                reportBridgeStatus(text(result.message), true);
            }
        } else {
            viewer_.projection_is_stale = viewer_.has_last_valid_projection;
            const auto detail = calculationText(result);
            setViewerCalculationStatus(
                viewer_.has_last_valid_projection
                    ? tr("Calculation failed - %1. The previous result remains visible.").arg(detail)
                    : tr("Calculation failed - %1").arg(detail),
                true);
            reportBridgeStatus(detail, false);
        }
        const bool restart = viewer_.pending_recalculation;
        viewer_.pending_recalculation = false;
        syncViewerPanelControls();
        updateViewerActionState();
        watcher->deleteLater();
        if (restart) {
            setViewerCalculationStatus(tr("A newer setting was committed; restarting calculation..."));
            scheduleViewerCalculation();
        }
    });
    watcher->setFuture(QtConcurrent::run([options = state.options, revision, options_revision, generation] {
        return tcqt_calculate_viewer(&options, revision, options_revision, generation);
    }));
}void MainWindow::updateComposition(double a, double b, double c) { ui_->statusMain->showMessage(tr("A=%1  B=%2  C=%3").arg(a, 0, 'f', 4).arg(b, 0, 'f', 4).arg(c, 0, 'f', 4)); }

void MainWindow::refreshViewerFieldSelectors() {
    TcqtProjectSummary summary{};
    if (!tcqt_project_summary(&summary).success) return;
    const QSignalBlocker grid_blocker(ui_->comboViewerGrid);
    const QSignalBlocker phase_blocker(ui_->comboViewerPhase);
    const QSignalBlocker property_blocker(ui_->comboViewerProperty);
    ui_->comboViewerGrid->clear();
    for (std::uint32_t index = 0; index < summary.grid_count; ++index) {
        TcqtGrid grid{};
        if (tcqt_grid_at(index, &grid).success) ui_->comboViewerGrid->addItem(text(grid.name), index);
    }
    if (summary.grid_count == 0) {
        viewer_.grid_index = 0; viewer_.phase_id = 0; viewer_.field_index = 0; viewer_.property.clear();
        ui_->comboViewerPhase->clear(); ui_->comboViewerProperty->clear();
        return;
    }
    viewer_.grid_index = qMin(viewer_.grid_index, summary.grid_count - 1);
    ui_->comboViewerGrid->setCurrentIndex(ui_->comboViewerGrid->findData(viewer_.grid_index));
    TcqtGrid grid{};
    if (!tcqt_grid_at(viewer_.grid_index, &grid).success) return;
    QVector<TcqtField> fields;
    for (std::uint32_t field_index = 0; field_index < grid.field_count; ++field_index) {
        TcqtField field{};
        if (tcqt_grid_field_at(viewer_.grid_index, field_index, &field).success) fields.append(field);
    }
    ui_->comboViewerPhase->clear();
    QSet<std::uint32_t> known_phases;
    for (const auto& field : fields) if (!known_phases.contains(field.phase_id)) {
        known_phases.insert(field.phase_id);
        QString name = tr("Phase %1").arg(field.phase_id);
        for (std::uint32_t phase_index = 0; phase_index < summary.phase_count; ++phase_index) {
            TcqtPhase phase{}; if (tcqt_phase_at(phase_index, &phase).success && phase.id == field.phase_id) { name = text(phase.name); break; }
        }
        ui_->comboViewerPhase->addItem(tr("[%1] %2").arg(field.phase_id).arg(name), field.phase_id);
    }
    if (ui_->comboViewerPhase->count() == 0) { viewer_.phase_id = 0; viewer_.property.clear(); ui_->comboViewerProperty->clear(); return; }
    if (ui_->comboViewerPhase->findData(viewer_.phase_id) < 0) viewer_.phase_id = ui_->comboViewerPhase->itemData(0).toUInt();
    ui_->comboViewerPhase->setCurrentIndex(ui_->comboViewerPhase->findData(viewer_.phase_id));
    ui_->comboViewerProperty->clear();
    for (const auto& field : fields) if (field.phase_id == viewer_.phase_id) ui_->comboViewerProperty->addItem(fieldProperty(field), field.index);
    int preferred = -1;
    for (int index = 0; index < ui_->comboViewerProperty->count(); ++index) {
        if (ui_->comboViewerProperty->itemText(index) == viewer_.property) preferred = index;
        if (preferred < 0 && ui_->comboViewerProperty->itemText(index) == QStringLiteral("T")) preferred = index;
    }
    if (preferred < 0) preferred = 0;
    ui_->comboViewerProperty->setCurrentIndex(preferred);
    viewer_.field_index = ui_->comboViewerProperty->currentData().toUInt();
    viewer_.property = ui_->comboViewerProperty->currentText();
}

void MainWindow::refreshViewerVertices() {
    QVector<CanvasVertex> vertices;
    TcqtProjectSummary summary{};
    if (!tcqt_project_summary(&summary).success || viewer_.grid_index >= summary.grid_count || viewer_.property.isEmpty()) {
        ui_->canvasTernary->setInspectionVertices(vertices);
        ui_->labelViewerCalculatedCount->setText(QStringLiteral("0"));
        ui_->labelViewerExtrapolatedCount->setText(QStringLiteral("0"));
        ui_->labelViewerCutOffCount->setText(QStringLiteral("0"));
        ui_->labelViewerMissingCount->setText(QStringLiteral("0"));
        ui_->labelViewerSelectedVertex->setText(tr("No vertex selected"));
        return;
    }
    TcqtGrid grid{};
    if (!tcqt_grid_at(viewer_.grid_index, &grid).success) return;
    std::array<int, 5> state_counts{};
    vertices.reserve(static_cast<qsizetype>(grid.row_count));
    for (std::uint32_t row = 0; row < grid.row_count; ++row) {
        TcqtRow composition{}; TcqtCell cell{};
        if (!tcqt_grid_row_at(viewer_.grid_index, row, &composition).success
            || !tcqt_grid_cell_at(viewer_.grid_index, viewer_.field_index, row, &cell).success) continue;
        if (cell.state < state_counts.size()) ++state_counts[cell.state];
        CanvasVertex vertex; vertex.composition = QPointF(composition.a, composition.b); vertex.row = row; vertex.state = cell.state;
        const auto note = text(cell.note);
        const auto token = cell.has_value ? QLocale::c().toString(cell.value, 'f', viewer_.label_decimals)
            : cell.state == 4 ? QStringLiteral("EX") : cell.state == 2 ? QStringLiteral("CO") : QStringLiteral("NA");
        vertex.label = note.isEmpty() ? token : token + QStringLiteral(":") + note;
        vertices.append(vertex);
    }
    ui_->labelViewerCalculatedCount->setText(QString::number(state_counts[0]));
    ui_->labelViewerExtrapolatedCount->setText(QString::number(state_counts[4]));
    ui_->labelViewerCutOffCount->setText(QString::number(state_counts[2]));
    ui_->labelViewerMissingCount->setText(QString::number(state_counts[3]));
    ui_->canvasTernary->setInspectionVertices(vertices);
    ui_->canvasTernary->setSourceVerticesVisible(viewer_.show_source_vertices);
    ui_->canvasTernary->setGridVisible(viewer_.show_regular_grid_edges && viewer_.show_sampling_grid);
    ui_->canvasTernary->setVertexVisibility(viewer_.show_calculated, viewer_.show_extrapolated, viewer_.show_cut_off, viewer_.show_missing);
    ui_->canvasTernary->setMarkerSize(viewer_.marker_size);
    ui_->canvasTernary->setVertexLabelSettings(viewer_.label_mode, viewer_.label_decimals, viewer_.labels_selected_only);
    ui_->canvasTernary->setSelectedRows(viewer_.selected_rows);
    updateViewerSelectionDetails();
}

bool MainWindow::refreshProjectionCanvas(bool accept_empty) {
    std::uint32_t count = 0;
    if (!tcqt_projection_record_count(&count).success) return false;
    // A dataset edit clears Rust-side records before the replacement worker completes.
    // Preserve the already rendered, last-valid scene until an accepted result arrives.
    if (count == 0 && viewer_.has_last_valid_projection && !accept_empty) return false;
    QMap<QString, CanvasPath> paths;
    for (std::uint32_t index = 0; index < count; ++index) {
        TcqtProjectionRecord record{};
        if (!tcqt_projection_record_at(index, &record).success) return false;
        // Raw and regularized variants intentionally share stable line IDs. Keep
        // their path source in the grouping key so Overlay retains both geometries.
        const auto key = QString::number(record.path_source) + QStringLiteral(":")
            + QString::number(record.line_type) + QStringLiteral(":")
            + text(record.phase_1) + QStringLiteral(":") + text(record.phase_2)
            + QStringLiteral(":") + text(record.line_id);
        auto& path = paths[key];
        path.type = record.line_type;
        path.rgba = record.rgba;
        path.stroke_width = record.stroke_width;
        path.marker_kind = record.marker_kind;
        path.path_source = record.path_source;
        path.line_id = text(record.line_id);
        const auto phase_1 = text(record.phase_1);
        const auto phase_2 = text(record.phase_2);
        path.phase_pair = phase_1.isEmpty() || phase_2.isEmpty()
            ? QString()
            : phase_1 + QStringLiteral(" / ") + phase_2;
        path.compositions.append(QPointF(record.a, record.b));
    }
    QVector<CanvasPath> output;
    output.reserve(paths.size());
    for (auto it = paths.cbegin(); it != paths.cend(); ++it) output.append(it.value());
    ui_->canvasTernary->setProjectionPaths(output);
    ui_->canvasTernary->setProjectionVisibility(viewer_.show_master_plot, viewer_.show_stable_isotherms,
        viewer_.show_stable_univariants, viewer_.show_binary_invariants, viewer_.show_interior_invariants);
    ui_->canvasTernary->setProjectionPathDisplayMode(viewer_.path_display_mode);
    ui_->canvasTernary->setProjectionAppearance(viewer_.line_width, viewer_.plot_marker_size);
    ui_->canvasTernary->setDiagnosticVisibility(viewer_.show_path_vertices, viewer_.show_contour_endpoints,
        viewer_.show_univariant_endpoints, viewer_.show_invariant_ids, viewer_.show_univariant_ids,
        viewer_.show_phase_pair_labels);
    return true;
}
void MainWindow::refreshViewerQueries() {
    auto* model = qobject_cast<QStandardItemModel*>(ui_->tableInterpolationResults->model());
    if (!model) return;
    model->removeRows(0, model->rowCount());
    for (auto& query : viewer_.queries) {
        const auto property = viewer_.property.toUtf8();
        TcqtInspectionResult result{};
        const auto status = tcqt_evaluate_field(viewer_.grid_index, viewer_.phase_id, property.constData(), &viewer_.options, query.a, query.b, query.c, query.id, &result);
        if (status.success) { query.grid_index = viewer_.grid_index; query.phase_id = viewer_.phase_id; query.property = viewer_.property; query.result = result; }
        QList<QStandardItem*> row;
        row << new QStandardItem(QString::number(query.id)) << new QStandardItem(QLocale::c().toString(query.a, 'g', 10)) << new QStandardItem(QLocale::c().toString(query.b, 'g', 10)) << new QStandardItem(QLocale::c().toString(query.c, 'g', 10));
        row << new QStandardItem(result.has_value ? QLocale::c().toString(result.value, 'g', 12) : (result.state == 3 ? QStringLiteral("CO") : QStringLiteral("NA")));
        const auto provenance = result.uses_extrapolated_sources
            ? tr("EX%1, %2; %3 source row%4")
                  .arg(result.maximum_extrapolation_layer)
                  .arg(text(result.extrapolation_methods))
                  .arg(result.extrapolated_source_row_count)
                  .arg(result.extrapolated_source_row_count == 1 ? QString() : QStringLiteral("s"))
            : tr("Calculated sources only");
        row << new QStandardItem(text(result.message)) << new QStandardItem(QString::number(viewer_.grid_index)) << new QStandardItem(QString::number(viewer_.phase_id)) << new QStandardItem(viewer_.property) << new QStandardItem(result.local_mode == 0 ? tr("Cubic") : result.local_mode == 1 ? tr("One-sided cubic") : result.local_mode == 2 ? tr("Linear") : tr("Undefined")) << new QStandardItem(provenance);
        model->appendRow(row);
    }
    const auto current = ui_->tableInterpolationResults->currentIndex();
    QVector<CanvasQuery> queries;
    queries.reserve(viewer_.queries.size());
    for (qsizetype index = 0; index < viewer_.queries.size(); ++index) {
        const auto& query = viewer_.queries.at(index);
        QPolygonF containing_triangle;
        if (query.result.has_source_rows) {
            for (const auto row_index : {query.result.source_row0, query.result.source_row1, query.result.source_row2}) {
                TcqtRow row{};
                if (tcqt_grid_row_at(viewer_.grid_index, row_index, &row).success) {
                    containing_triangle << QPointF(row.a, row.b);
                }
            }
        }
        queries.append({query.id, QPointF(query.a, query.b), query.result.state,
                        current.isValid() && current.row() == index, containing_triangle});
    }
    ui_->canvasTernary->setQueries(queries);
    ui_->canvasTernary->setContainingTriangleVisible(viewer_.show_containing_triangle);
}

void MainWindow::dispatchViewerWidgetCommand(ViewerWidgetCommand action) {
    if (synchronizing_) return;
    bool schedule_calculation = false;
    switch (action) {
    case ViewerWidgetCommand::SelectGrid:
        viewer_.grid_index = ui_->comboViewerGrid->currentData().toUInt(); viewer_.phase_id = 0; viewer_.property.clear(); viewer_.selected_rows.clear();
        refreshViewerFieldSelectors(); refreshViewerVertices(); refreshViewerQueries(); break;
    case ViewerWidgetCommand::SelectPhase:
        viewer_.phase_id = ui_->comboViewerPhase->currentData().toUInt(); viewer_.property.clear(); viewer_.selected_rows.clear();
        refreshViewerFieldSelectors(); refreshViewerVertices(); refreshViewerQueries(); break;
    case ViewerWidgetCommand::SelectProperty:
        viewer_.field_index = ui_->comboViewerProperty->currentData().toUInt(); viewer_.property = ui_->comboViewerProperty->currentText(); viewer_.selected_rows.clear();
        refreshViewerVertices(); refreshViewerQueries(); break;
    case ViewerWidgetCommand::SetInteractionMode:
        viewer_.interaction_mode = ui_->comboViewerMode->currentIndex(); ui_->canvasTernary->setInteractionMode(viewer_.interaction_mode); break;
    case ViewerWidgetCommand::SetVertexVisibility:
        viewer_.show_calculated = ui_->checkViewerCalculated->isChecked(); viewer_.show_extrapolated = ui_->checkViewerExtrapolated->isChecked();
        viewer_.show_cut_off = ui_->checkViewerCutOff->isChecked(); viewer_.show_missing = ui_->checkViewerMissing->isChecked(); refreshViewerVertices(); break;
    case ViewerWidgetCommand::SetRegularGridEdges:
        viewer_.show_regular_grid_edges = ui_->checkViewerRegularGridEdges->isChecked(); refreshViewerVertices(); break;
    case ViewerWidgetCommand::SetMarkerSize: viewer_.marker_size = ui_->spinViewerMarkerSize->value(); refreshViewerVertices(); break;
    case ViewerWidgetCommand::SetLabelMode: viewer_.label_mode = ui_->comboViewerLabelMode->currentIndex(); refreshViewerVertices(); break;
    case ViewerWidgetCommand::SetLabelDecimals: viewer_.label_decimals = ui_->spinViewerLabelDecimals->value(); refreshViewerVertices(); break;
    case ViewerWidgetCommand::SetLabelsSelectedOnly: viewer_.labels_selected_only = ui_->checkViewerLabelsSelectedOnly->isChecked(); refreshViewerVertices(); break;
    case ViewerWidgetCommand::SetAutomaticRange:
        viewer_.options.automatic_range = ui_->checkViewerAutomaticRange->isChecked(); schedule_calculation = commitViewerCalculationOptions(action); break;
    case ViewerWidgetCommand::CommitIsoMinimum:
    case ViewerWidgetCommand::CommitIsoMaximum:
    case ViewerWidgetCommand::CommitIsoStep:
    case ViewerWidgetCommand::SetSamplingSubdivisions:
    case ViewerWidgetCommand::SetSourceInterpolation:
    case ViewerWidgetCommand::SetCubicMethod:
    case ViewerWidgetCommand::SetPartialDomainPolicy:
    case ViewerWidgetCommand::SetContinuation:
    case ViewerWidgetCommand::SetRegularizationEnabled:
    case ViewerWidgetCommand::SetRegularizationSpacing:
        schedule_calculation = commitViewerCalculationOptions(action); break;
    case ViewerWidgetCommand::SetPathDisplayMode:
        viewer_.path_display_mode = ui_->comboViewerPathDisplay->currentIndex(); refreshProjectionCanvas(); break;
    case ViewerWidgetCommand::SetMasterPlotVisible: viewer_.show_master_plot = ui_->actionViewPlot->isChecked(); refreshProjectionCanvas(); break;
    case ViewerWidgetCommand::SetSamplingGridVisible: viewer_.show_sampling_grid = ui_->actionViewGrid->isChecked(); refreshViewerVertices(); break;
    case ViewerWidgetCommand::SetSourceVerticesVisible: viewer_.show_source_vertices = ui_->actionViewSourceVertices->isChecked(); refreshViewerVertices(); break;
    case ViewerWidgetCommand::SetQueryPointsVisible: viewer_.show_query_points = ui_->actionViewQueryPoints->isChecked(); ui_->canvasTernary->setQueryPointsVisible(viewer_.show_query_points); break;
    case ViewerWidgetCommand::SetResultsTableVisible: viewer_.show_results_table = ui_->actionViewResultsTable->isChecked(); ui_->tableInterpolationResults->setVisible(viewer_.show_results_table); break;
    case ViewerWidgetCommand::SetStableIsothermsVisible: viewer_.show_stable_isotherms = ui_->actionViewStableIsotherms->isChecked(); refreshProjectionCanvas(); break;
    case ViewerWidgetCommand::SetStableUnivariantsVisible: viewer_.show_stable_univariants = ui_->actionViewStableUnivariants->isChecked(); refreshProjectionCanvas(); break;
    case ViewerWidgetCommand::SetBinaryInvariantsVisible: viewer_.show_binary_invariants = ui_->actionViewBinaryInvariants->isChecked(); refreshProjectionCanvas(); break;
    case ViewerWidgetCommand::SetInteriorInvariantsVisible: viewer_.show_interior_invariants = ui_->actionViewInteriorInvariants->isChecked(); refreshProjectionCanvas(); break;
    case ViewerWidgetCommand::SetAxisLabelsVisible: viewer_.show_axis_labels = ui_->actionViewAxisLabels->isChecked(); ui_->canvasTernary->setAxisLabelsVisible(viewer_.show_axis_labels); break;
    case ViewerWidgetCommand::SetCornerNamesVisible: viewer_.show_corner_names = ui_->actionViewCornerNames->isChecked(); ui_->canvasTernary->setComponentNamesVisible(viewer_.show_corner_names); break;
    case ViewerWidgetCommand::SetLegendVisible: viewer_.show_legend = ui_->actionViewLegend->isChecked(); ui_->canvasTernary->setLegendVisible(viewer_.show_legend); break;
    case ViewerWidgetCommand::SetPathVerticesVisible: viewer_.show_path_vertices = ui_->checkViewerPathVertices->isChecked(); refreshProjectionCanvas(); break;
    case ViewerWidgetCommand::SetContourEndpointsVisible: viewer_.show_contour_endpoints = ui_->checkViewerContourEndpoints->isChecked(); refreshProjectionCanvas(); break;
    case ViewerWidgetCommand::SetUnivariantEndpointsVisible: viewer_.show_univariant_endpoints = ui_->checkViewerUnivariantEndpoints->isChecked(); refreshProjectionCanvas(); break;
    case ViewerWidgetCommand::SetInvariantIdsVisible: viewer_.show_invariant_ids = ui_->checkViewerInvariantIds->isChecked(); refreshProjectionCanvas(); break;
    case ViewerWidgetCommand::SetUnivariantIdsVisible: viewer_.show_univariant_ids = ui_->checkViewerUnivariantIds->isChecked(); refreshProjectionCanvas(); break;
    case ViewerWidgetCommand::SetPhasePairLabelsVisible: viewer_.show_phase_pair_labels = ui_->checkViewerPhasePairLabels->isChecked(); refreshProjectionCanvas(); break;
    case ViewerWidgetCommand::SetContainingTriangleVisible: viewer_.show_containing_triangle = ui_->checkViewerContainingTriangle->isChecked(); ui_->canvasTernary->setContainingTriangleVisible(viewer_.show_containing_triangle); break;
    case ViewerWidgetCommand::SetLineWidth: viewer_.line_width = ui_->spinViewerLineWidth->value(); refreshProjectionCanvas(); break;
    case ViewerWidgetCommand::SetPlotMarkerSize: viewer_.plot_marker_size = ui_->spinViewerPlotMarkerSize->value(); refreshProjectionCanvas(); break;
    case ViewerWidgetCommand::Fit: ui_->canvasTernary->fitTriangleToView(); break;
    case ViewerWidgetCommand::Reset: ui_->canvasTernary->resetView(); break;
    case ViewerWidgetCommand::RestoreLayout:
        ui_->splitterViewerOuter->setSizes({340, 940}); ui_->splitterViewerControls->setSizes({420, 420}); ui_->splitterViewerRight->setSizes({620, 250}); for (const auto& section : viewer_sections_) section->resetToDefault(); break;
    case ViewerWidgetCommand::RemoveSelectedQuery:
        if (const auto current = ui_->tableInterpolationResults->currentIndex(); current.isValid() && current.row() < viewer_.queries.size()) viewer_.queries.removeAt(current.row()); refreshViewerQueries(); break;
    case ViewerWidgetCommand::RemoveAllQueries: viewer_.queries.clear(); refreshViewerQueries(); break;
    case ViewerWidgetCommand::ResetAutomaticRange:
        viewer_.options.automatic_range = true; schedule_calculation = commitViewerCalculationOptions(action); break;
    default: break;
    }
    if (schedule_calculation) scheduleViewerCalculation();
    syncViewerPanelControls();
    updateViewerActionState();
}

bool MainWindow::commitViewerCalculationOptions(ViewerWidgetCommand source) {
    const auto finite_positive = [this](QLineEdit* editor, double* target, const QString& label) {
        if (!commitViewerNumber(editor, target, label) || *target <= 0.0) { editor->setToolTip(tr("%1 must be finite and positive.").arg(label)); return false; }
        return true;
    };
    if (source == ViewerWidgetCommand::CommitIsoMinimum && !commitViewerNumber(ui_->editViewerTmin, &viewer_.options.minimum, tr("Tmin"))) return false;
    if (source == ViewerWidgetCommand::CommitIsoMaximum && !commitViewerNumber(ui_->editViewerTmax, &viewer_.options.maximum, tr("Tmax"))) return false;
    if (source == ViewerWidgetCommand::CommitIsoStep && !finite_positive(ui_->editViewerStep, &viewer_.options.level_step, tr("Step"))) return false;
    if (source == ViewerWidgetCommand::SetRegularizationSpacing && !finite_positive(ui_->editViewerRegularizationSpacing, &viewer_.options.regularization_spacing, tr("Regularization spacing"))) return false;
    if (source == ViewerWidgetCommand::CommitIsoMinimum || source == ViewerWidgetCommand::CommitIsoMaximum || source == ViewerWidgetCommand::CommitIsoStep) viewer_.options.automatic_range = false;
    const auto source_interpolation = sourceInterpolationAbi(ui_->comboViewerSourceInterpolation->currentIndex());
    const auto cubic_method = cubicMethodAbi(ui_->comboViewerCubicMethod->currentIndex());
    const auto partial_domain = partialDomainAbi(ui_->comboViewerPartialDomain->currentIndex());
    const auto continuation = continuationAbi(ui_->comboViewerContinuation->currentIndex());
    if (source_interpolation == invalid_viewer_abi || cubic_method == invalid_viewer_abi
        || partial_domain == invalid_viewer_abi || continuation == invalid_viewer_abi) {
        reportBridgeStatus(tr("Unsupported Viewer option selection."), false);
        return false;
    }
    TcqtGrid selected_grid{};
    if (source_interpolation == 1
        && tcqt_grid_at(viewer_.grid_index, &selected_grid).success
        && selected_grid.kind != 0) {
        reportBridgeStatus(tr("Cubic alpha is unavailable for irregular grids. Select Linear source interpolation."), false);
        return false;
    }
    viewer_.options.sampling_subdivisions = static_cast<std::uint32_t>(ui_->spinViewerSamplingSubdivisions->value());
    viewer_.options.source_interpolation = source_interpolation;
    viewer_.options.cubic_method = cubic_method;
    viewer_.options.partial_domain_policy = partial_domain;
    viewer_.options.continuation = continuation;
    viewer_.options.regularize = ui_->checkViewerRegularizePaths->isChecked();
    const auto stored = tcqt_set_viewer_calculation_options(&viewer_.options);
    if (!stored.success) { reportBridgeStatus(statusText(stored), false); return false; }
    TcqtViewerCalculationState authoritative{};
    const auto current = tcqt_viewer_calculation_state(&authoritative);
    if (!current.success) {
        reportBridgeStatus(statusText(current), false);
        return false;
    }
    viewer_.options = authoritative.options;
    viewer_.options_revision = authoritative.options_revision;
    return true;
}

void MainWindow::syncViewerPanelControls() {
    TcqtViewerCalculationState authoritative{};
    if (tcqt_viewer_calculation_state(&authoritative).success) {
        viewer_.options = authoritative.options;
        viewer_.options_revision = authoritative.options_revision;
    }
    const auto set_checked = [](QAction* action, bool checked) { const QSignalBlocker blocker(action); action->setChecked(checked); };
    const auto set_panel = [](QCheckBox* panel, bool checked) { const QSignalBlocker blocker(panel); panel->setChecked(checked); };
    set_checked(ui_->actionViewPlot, viewer_.show_master_plot); set_checked(ui_->actionViewGrid, viewer_.show_sampling_grid);
    set_checked(ui_->actionViewSourceVertices, viewer_.show_source_vertices); set_checked(ui_->actionViewQueryPoints, viewer_.show_query_points);
    set_checked(ui_->actionViewResultsTable, viewer_.show_results_table); set_checked(ui_->actionViewStableIsotherms, viewer_.show_stable_isotherms);
    set_checked(ui_->actionViewStableUnivariants, viewer_.show_stable_univariants); set_checked(ui_->actionViewBinaryInvariants, viewer_.show_binary_invariants);
    set_checked(ui_->actionViewInteriorInvariants, viewer_.show_interior_invariants); set_checked(ui_->actionViewAxisLabels, viewer_.show_axis_labels);
    set_checked(ui_->actionViewCornerNames, viewer_.show_corner_names); set_checked(ui_->actionViewLegend, viewer_.show_legend);
    set_panel(ui_->checkViewerSamplingGrid, viewer_.show_sampling_grid); set_panel(ui_->checkViewerSourceVertices, viewer_.show_source_vertices);
    set_panel(ui_->checkViewerQueryPoints, viewer_.show_query_points); set_panel(ui_->checkViewerStableIsotherms, viewer_.show_stable_isotherms);
    set_panel(ui_->checkViewerStableUnivariants, viewer_.show_stable_univariants); set_panel(ui_->checkViewerBinaryInvariants, viewer_.show_binary_invariants);
    set_panel(ui_->checkViewerInteriorInvariants, viewer_.show_interior_invariants); set_panel(ui_->checkViewerAxisLabels, viewer_.show_axis_labels);
    set_panel(ui_->checkViewerCornerNames, viewer_.show_corner_names); set_panel(ui_->checkViewerLegend, viewer_.show_legend);
    set_panel(ui_->checkViewerAutomaticRange, viewer_.options.automatic_range);
    { const QSignalBlocker blocker(ui_->comboViewerMode); ui_->comboViewerMode->setCurrentIndex(viewer_.interaction_mode); }
    { const QSignalBlocker blocker(ui_->spinViewerSamplingSubdivisions); ui_->spinViewerSamplingSubdivisions->setValue(static_cast<int>(viewer_.options.sampling_subdivisions)); }
    { const QSignalBlocker blocker(ui_->comboViewerSourceInterpolation); ui_->comboViewerSourceInterpolation->setCurrentIndex(sourceInterpolationIndex(viewer_.options.source_interpolation)); }
    { const QSignalBlocker blocker(ui_->comboViewerCubicMethod); ui_->comboViewerCubicMethod->setCurrentIndex(cubicMethodIndex(viewer_.options.cubic_method)); }
    { const QSignalBlocker blocker(ui_->comboViewerPartialDomain); ui_->comboViewerPartialDomain->setCurrentIndex(partialDomainIndex(viewer_.options.partial_domain_policy)); }
    { const QSignalBlocker blocker(ui_->comboViewerContinuation); ui_->comboViewerContinuation->setCurrentIndex(continuationIndex(viewer_.options.continuation)); }
    { const QSignalBlocker blocker(ui_->checkViewerRegularizePaths); ui_->checkViewerRegularizePaths->setChecked(viewer_.options.regularize); }
    { const QSignalBlocker blocker(ui_->editViewerStep); ui_->editViewerStep->setText(QLocale::c().toString(viewer_.options.level_step, 'g', 10)); }
    { const QSignalBlocker blocker(ui_->editViewerRegularizationSpacing); ui_->editViewerRegularizationSpacing->setText(QLocale::c().toString(viewer_.options.regularization_spacing, 'g', 10)); }
}
void MainWindow::updateViewerActionState() {
    TcqtProjectSummary summary{};
    if (!tcqt_project_summary(&summary).success) return;
    const bool field_available = !viewer_.property.isEmpty() && viewer_.grid_index < summary.grid_count;
    const auto unavailable = summary.calculation_available ? QString() : tr("Calculation requires valid participating phases, grids, and finite Temperature values.");
    TcqtGrid inspection_grid{};
    const bool regular_inspection_grid = tcqt_grid_at(viewer_.grid_index, &inspection_grid).success && inspection_grid.kind == 0;
    bool has_eligible_missing = false;
    if (field_available && regular_inspection_grid) {
        for (std::uint32_t row = 0; row < inspection_grid.row_count; ++row) {
            TcqtCell cell{};
            if (tcqt_grid_cell_at(viewer_.grid_index, viewer_.field_index, row, &cell).success && cell.state == 3) {
                has_eligible_missing = true;
                break;
            }
        }
    }
    ui_->buttonViewerExtrapolatePhase->setEnabled(field_available && regular_inspection_grid && viewer_.interaction_mode == 0 && has_eligible_missing);
    ui_->buttonViewerExtrapolatePhase->setToolTip(!regular_inspection_grid
        ? tr("Automatic mesh extrapolation is currently available for regular grids only.")
        : viewer_.interaction_mode != 0
            ? tr("Return to Vertex mode to extrapolate source vertices.")
            : has_eligible_missing ? QString() : tr("The selected field contains no eligible missing values."));    ui_->comboViewerGrid->setEnabled(summary.grid_count > 0);
    ui_->comboViewerPhase->setEnabled(field_available); ui_->comboViewerProperty->setEnabled(field_available); ui_->comboViewerMode->setEnabled(field_available);
    for (QWidget* control : {static_cast<QWidget*>(ui_->checkViewerCalculated), static_cast<QWidget*>(ui_->checkViewerExtrapolated), static_cast<QWidget*>(ui_->checkViewerCutOff), static_cast<QWidget*>(ui_->checkViewerMissing), static_cast<QWidget*>(ui_->checkViewerRegularGridEdges), static_cast<QWidget*>(ui_->spinViewerMarkerSize), static_cast<QWidget*>(ui_->comboViewerLabelMode), static_cast<QWidget*>(ui_->spinViewerLabelDecimals), static_cast<QWidget*>(ui_->checkViewerLabelsSelectedOnly)}) control->setEnabled(field_available);
    ui_->actionViewSourceVertices->setEnabled(field_available); ui_->actionViewSourceVertices->setToolTip(field_available ? QString() : tr("Select a grid field first."));
    ui_->actionViewQueryPoints->setEnabled(!viewer_.queries.isEmpty());
    ui_->actionViewerClearSelectedQuery->setEnabled(ui_->tableInterpolationResults->currentIndex().isValid()); ui_->actionViewerClearAllQueries->setEnabled(!viewer_.queries.isEmpty());
    for (auto* action : {ui_->actionViewStableIsotherms, ui_->actionViewStableUnivariants, ui_->actionViewBinaryInvariants, ui_->actionViewInteriorInvariants}) { action->setEnabled(viewer_.has_last_valid_projection); action->setToolTip(viewer_.has_last_valid_projection ? QString() : unavailable); }
    ui_->buttonViewerResetAutomaticRange->setEnabled(summary.calculation_available);
    for (QWidget* control : {static_cast<QWidget*>(ui_->checkViewerAutomaticRange), static_cast<QWidget*>(ui_->editViewerTmin), static_cast<QWidget*>(ui_->editViewerTmax), static_cast<QWidget*>(ui_->editViewerStep), static_cast<QWidget*>(ui_->spinViewerSamplingSubdivisions), static_cast<QWidget*>(ui_->comboViewerSourceInterpolation), static_cast<QWidget*>(ui_->comboViewerCubicMethod), static_cast<QWidget*>(ui_->comboViewerPartialDomain), static_cast<QWidget*>(ui_->comboViewerContinuation), static_cast<QWidget*>(ui_->checkViewerRegularizePaths), static_cast<QWidget*>(ui_->editViewerRegularizationSpacing)}) control->setEnabled(summary.calculation_available);
    const bool cubic_selected = ui_->comboViewerSourceInterpolation->currentIndex() == 1;
    const bool cubic_available = summary.calculation_available && cubic_selected && regular_inspection_grid;
    const auto cubic_reason = !regular_inspection_grid
        ? tr("Cubic alpha is unavailable for irregular grids. Select Linear source interpolation.")
        : !cubic_selected ? tr("Select Cubic alpha to configure cubic controls.") : QString();
    for (QWidget* control : {static_cast<QWidget*>(ui_->comboViewerCubicMethod), static_cast<QWidget*>(ui_->comboViewerPartialDomain), static_cast<QWidget*>(ui_->comboViewerContinuation)}) {
        control->setEnabled(cubic_available);
        control->setToolTip(cubic_available ? QString() : cubic_reason);
    }
    ui_->comboViewerSourceInterpolation->setToolTip(!regular_inspection_grid
        ? tr("Irregular grids support Linear source interpolation only.") : QString());
    if (!summary.calculation_available) setViewerCalculationStatus(tr("Waiting for calculation-ready data"));
}

void MainWindow::scheduleViewerCalculation() {
    TcqtProjectSummary summary{};
    if (!tcqt_project_summary(&summary).success) return;
    if (!summary.calculation_available) {
        setViewerCalculationStatus(tr("Waiting for calculation-ready data"));
        return;
    }
    viewer_.projection_is_stale = viewer_.has_last_valid_projection;
    if (viewer_.calculation_running) {
        viewer_.pending_recalculation = true;
        setViewerCalculationStatus(tr("Settings changed - recalculation pending"));
        return;
    }
    runRustCalculation();
}

void MainWindow::setViewerCalculationStatus(const QString& message, bool error) {
    ui_->labelViewerCalculationStatus->setText(message);
    ui_->labelViewerCalculationStatus->setStyleSheet(error ? QStringLiteral("color: #b03030") : QString());
}

void MainWindow::updateViewerSelectionDetails() {
    if (viewer_.selected_rows.size() != 1 || viewer_.property.isEmpty()) { ui_->labelViewerSelectedVertex->setText(tr("No vertex selected")); return; }
    const auto row = *viewer_.selected_rows.cbegin(); TcqtRow composition{}; TcqtCell cell{};
    if (!tcqt_grid_row_at(viewer_.grid_index, row, &composition).success || !tcqt_grid_cell_at(viewer_.grid_index, viewer_.field_index, row, &cell).success) return;
    const auto state = cell.state == 0 ? tr("Calculated") : cell.state == 4 ? tr("Extrapolated") : cell.state == 2 ? tr("Cut-off") : tr("Missing");
    const auto value = cell.has_value ? QLocale::c().toString(cell.value, 'g', 12) : QStringLiteral("-");
    const auto note = text(cell.note);
    ui_->labelViewerSelectedVertex->setText(tr("Row %1\nA %2  B %3  C %4\nPhase %5 (stable ID %6) · %7\nState: %8\nValue: %9\nNote: %10")
        .arg(row + 1).arg(composition.a, 0, 'g', 10).arg(composition.b, 0, 'g', 10).arg(composition.c, 0, 'g', 10)
        .arg(ui_->comboViewerPhase->currentText()).arg(viewer_.phase_id).arg(viewer_.property).arg(state, value, note));
}
void MainWindow::setInterpolationPreview(const TcqtLocatedPoint& location) {
    CanvasInterpolationPreview preview;
    preview.composition = QPointF(location.a, location.b);
    for (const auto row_index : {location.source_row0, location.source_row1, location.source_row2}) {
        TcqtRow row{};
        if (tcqt_grid_row_at(viewer_.grid_index, row_index, &row).success) {
            preview.containing_triangle << QPointF(row.a, row.b);
            preview.source_rows.insert(row_index);
        }
    }
    ui_->canvasTernary->setInterpolationPreview(preview);
}

void MainWindow::clearInterpolationPreview() {
    ui_->canvasTernary->setInterpolationPreview(std::nullopt);
}

void MainWindow::addInterpolationQuery(double a, double b, double c) {
    if (viewer_.property.isEmpty()) {
        reportBridgeStatus(tr("Select a grid, phase, and property before adding a query."), false);
        return;
    }
    TcqtLocatedPoint initial{};
    const auto initial_status = tcqt_locate_grid_point(viewer_.grid_index, a, b, c, &initial);
    if (!initial_status.success) {
        reportBridgeStatus(statusText(initial_status), false);
        return;
    }
    TcqtProjectSummary summary{};
    if (!tcqt_project_summary(&summary).success) return;
    const QStringList components{text(summary.component_a), text(summary.component_b), text(summary.component_c)};
    setInterpolationPreview(initial);
    InterpolationPointDialog dialog(viewer_.grid_index, components, initial, this);
    connect(&dialog, &InterpolationPointDialog::previewLocationChanged, this,
            [this](const TcqtLocatedPoint& location) { setInterpolationPreview(location); });
    const auto accepted = dialog.exec() == QDialog::Accepted;
    clearInterpolationPreview();
    if (!accepted) return;
    const auto location = dialog.acceptedLocation();
    ViewerQuery query;
    query.id = viewer_.next_query_id++;
    query.a = location.a;
    query.b = location.b;
    query.c = location.c;
    viewer_.queries.append(query);
    refreshViewerQueries();
    updateViewerActionState();
}

void MainWindow::selectViewerVertex(std::uint32_t row, bool additive) {
    if (!additive) viewer_.selected_rows.clear();
    if (additive && viewer_.selected_rows.contains(row)) viewer_.selected_rows.remove(row); else viewer_.selected_rows.insert(row);
    ui_->canvasTernary->setSelectedRows(viewer_.selected_rows);
    updateViewerSelectionDetails();
    updateViewerActionState();
}

void MainWindow::editViewerVertex(std::uint32_t row, const QPoint& global_position) {
    if (viewer_.property.isEmpty()) return;
    TcqtCell existing{};
    if (!tcqt_grid_cell_at(viewer_.grid_index, viewer_.field_index, row, &existing).success) return;
    QDialog dialog(this, Qt::Tool);
    dialog.setWindowTitle(tr("Vertex %1 - %2 / %3").arg(row + 1).arg(viewer_.phase_id).arg(viewer_.property));
    auto* layout = new QFormLayout(&dialog);
    auto* state = new QComboBox(&dialog);
    state->addItem(tr("Calculated"), 0);
    state->addItem(tr("Extrapolated (EX)"), 4);
    state->addItem(tr("Missing (NA)"), 3);
    state->addItem(tr("Cut-off (CO)"), 2);
    state->setCurrentIndex(qMax(0, state->findData(existing.state)));
    auto* value = new QLineEdit(existing.has_value ? QLocale::c().toString(existing.value, 'g', 15) : QString(), &dialog);
    auto* note = new QLineEdit(text(existing.note), &dialog);
    auto* provenance = new QLabel(&dialog);
    provenance->setWordWrap(true);
    auto* error = new QLabel(&dialog);
    error->setStyleSheet(QStringLiteral("color: #b03030"));
    layout->addRow(tr("State"), state);
    layout->addRow(tr("Value"), value);
    layout->addRow(tr("Note"), note);
    layout->addRow(tr("Provenance"), provenance);
    layout->addRow(error);
    auto* actions = new QDialogButtonBox(QDialogButtonBox::Cancel, &dialog);
    auto* extrapolate = actions->addButton(tr("Extrapolate this vertex..."), QDialogButtonBox::ActionRole);
    TcqtGrid source_grid{};
    const bool regular_grid = tcqt_grid_at(viewer_.grid_index, &source_grid).success && source_grid.kind == 0;
    auto* calculated = actions->addButton(tr("Enter calculated value"), QDialogButtonBox::ActionRole);
    auto* cut_off = actions->addButton(tr("Set cut-off"), QDialogButtonBox::ActionRole);
    auto* clear = actions->addButton(tr("Clear to NA"), QDialogButtonBox::ActionRole);
    layout->addRow(actions);

    const auto update_state = [&] {
        const auto code = state->currentData().toUInt();
        const bool extrapolated = code == 4;
        value->setEnabled(code == 0);
        note->setEnabled(!extrapolated);
        extrapolate->setText(extrapolated ? tr("Re-extrapolate...") : tr("Extrapolate this vertex..."));
        extrapolate->setVisible(code == 3 || extrapolated || code == 2);
        extrapolate->setEnabled(regular_grid && (code == 3 || extrapolated));
        extrapolate->setToolTip(!regular_grid
            ? tr("Automatic mesh extrapolation is currently available for regular grids only.")
            : code == 2
                ? tr("Cut-off values are excluded from automatic mesh extrapolation. Convert this cell to NA first to make it eligible.")
                : QString());
        if (extrapolated) {
            provenance->setText(tr("EX%1 - %2; support %3; spread %4")
                .arg(existing.extrapolation_layer)
                .arg(existing.extrapolation_method == 0 ? tr("Akima") : existing.extrapolation_method == 1 ? tr("Makima") : existing.extrapolation_method == 2 ? tr("PCHIP") : tr("Steffen"))
                .arg(existing.extrapolation_support_count)
                .arg(QLocale::c().toString(existing.extrapolation_spread, 'g', 10)));
        } else if (code == 2) {
            provenance->setText(tr("Cut-off values are excluded from automatic mesh extrapolation. Convert to NA first to make this vertex eligible."));
        } else {
            provenance->clear();
        }
    };
    const auto commit = [&] {
        const auto code = state->currentData().toUInt();
        if (code == 4) {
            error->setText(tr("Extrapolated values are read-only. Re-extrapolate, convert to calculated, or clear to NA."));
            return;
        }
        QString token;
        if (code == 0) {
            bool ok = false;
            const auto scalar = QLocale::c().toDouble(value->text(), &ok);
            if (!ok || !std::isfinite(scalar)) { error->setText(tr("Calculated requires one finite numeric value.")); return; }
            token = QLocale::c().toString(scalar, 'g', 15);
        } else {
            token = code == 2 ? QStringLiteral("CO") : QStringLiteral("NA");
        }
        if (!note->text().trimmed().isEmpty()) token += QStringLiteral(":") + note->text().trimmed();
        const auto property = viewer_.property.toUtf8();
        const auto encoded = token.toUtf8();
        const auto result = tcqt_set_field_vertex(viewer_.grid_index, viewer_.phase_id, property.constData(), row, encoded.constData());
        if (!result.success) { error->setText(statusText(result)); return; }
        dialog.accept();
        rebuildFromRust(selected_grid_);
        scheduleViewerCalculation();
        reportBridgeStatus(tr("Updated vertex %1: %2").arg(row + 1).arg(token), true);
    };
    connect(value, &QLineEdit::returnPressed, &dialog, commit);
    connect(note, &QLineEdit::returnPressed, &dialog, commit);
    connect(state, qOverload<int>(&QComboBox::currentIndexChanged), &dialog, [=](int) { update_state(); });
    connect(calculated, &QPushButton::clicked, &dialog, [=] { state->setCurrentIndex(state->findData(0)); value->setFocus(); });
    connect(cut_off, &QPushButton::clicked, &dialog, [=] { state->setCurrentIndex(state->findData(2)); commit(); });
    connect(clear, &QPushButton::clicked, &dialog, [=] { state->setCurrentIndex(state->findData(3)); commit(); });
    connect(extrapolate, &QPushButton::clicked, &dialog, [this, &dialog, row] {
        dialog.reject();
        QTimer::singleShot(0, this, [this, row] { extrapolateViewerTargets({row}); });
    });
    connect(actions, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
    update_state();
    const auto screen = QGuiApplication::screenAt(global_position);
    const auto bounds = screen ? screen->availableGeometry() : geometry();
    dialog.adjustSize();
    auto position = global_position + QPoint(12, 12);
    position.setX(qBound(bounds.left(), position.x(), bounds.right() - dialog.width()));
    position.setY(qBound(bounds.top(), position.y(), bounds.bottom() - dialog.height()));
    dialog.move(position);
    if (existing.state == 0) value->setFocus();
    dialog.exec();
}
void MainWindow::showViewerVertexContextMenu(std::uint32_t row, const QPoint& global_position) {
    if (viewer_.property.isEmpty()) return;
    if (!viewer_.selected_rows.contains(row)) {
        viewer_.selected_rows = {row};
        ui_->canvasTernary->setSelectedRows(viewer_.selected_rows);
    }
    QVector<std::uint32_t> eligible;
    bool contains_calculated_or_cut_off = false;
    for (const auto selected : viewer_.selected_rows) {
        TcqtCell cell{};
        if (!tcqt_grid_cell_at(viewer_.grid_index, viewer_.field_index, selected, &cell).success) continue;
        if (cell.state == 3) eligible.append(selected);
        else if (cell.state == 0 || cell.state == 2) contains_calculated_or_cut_off = true;
    }
    QMenu menu(this);
    auto* extrapolate_one = menu.addAction(tr("Extrapolate selected vertex"));
    auto* extrapolate_many = menu.addAction(tr("Extrapolate selected vertices"));
    auto* extrapolate_phase = menu.addAction(tr("Extrapolate missing vertices for selected phase..."));
    auto* clear_extrapolated = menu.addAction(tr("Clear extrapolated values from selected phase..."));
    menu.addSeparator();
    auto* missing = menu.addAction(tr("Set selected to Missing (NA)"));
    auto* cut_off = menu.addAction(tr("Set selected to Cut-off (CO)"));
    auto* clear_notes = menu.addAction(tr("Clear notes"));
    extrapolate_one->setEnabled(eligible.size() == 1 && viewer_.selected_rows.size() == 1);
    extrapolate_many->setEnabled(!eligible.isEmpty());
    extrapolate_many->setToolTip(contains_calculated_or_cut_off
        ? tr("Calculated and cut-off rows will be skipped; only missing rows are candidates.")
        : QString());
    TcqtGrid grid{};
    const bool regular = tcqt_grid_at(viewer_.grid_index, &grid).success && grid.kind == 0;
    extrapolate_phase->setEnabled(regular);
    extrapolate_phase->setToolTip(regular ? QString() : tr("Automatic mesh extrapolation is currently available for regular grids only."));
    const auto selected = menu.exec(global_position);
    if (!selected) return;
    if (selected == extrapolate_one || selected == extrapolate_many) {
        extrapolateViewerTargets(eligible);
        return;
    }
    if (selected == extrapolate_phase) {
        extrapolateViewerPhase();
        return;
    }
    if (selected == clear_extrapolated) {
        const auto status = tcqt_clear_extrapolated_phase_values(viewer_.grid_index, viewer_.phase_id);
        reportBridgeStatus(statusText(status), status.success);
        if (status.success) { rebuildFromRust(selected_grid_); scheduleViewerCalculation(); }
        return;
    }
    const auto property = viewer_.property.toUtf8();
    QVector<std::uint32_t> rows = viewer_.selected_rows.values();
    TcqtStatus result{};
    if (selected == clear_notes) result = tcqt_clear_field_notes(viewer_.grid_index, viewer_.phase_id, property.constData(), rows.constData(), rows.size());
    else { const auto state = selected == cut_off ? 2U : 3U; result = tcqt_bulk_set_field_state(viewer_.grid_index, viewer_.phase_id, property.constData(), rows.constData(), rows.size(), state); }
    reportBridgeStatus(statusText(result), result.success);
    if (result.success) { rebuildFromRust(selected_grid_); scheduleViewerCalculation(); }
}
bool MainWindow::commitViewerNumber(QLineEdit* editor, double* target, const QString& label) {
    bool ok = false; const auto value = QLocale::c().toDouble(editor->text(), &ok); if (!ok || !std::isfinite(value)) { editor->setToolTip(tr("%1 must be a finite number.").arg(label)); return false; } *target = value; return true;
}
void MainWindow::closeEvent(QCloseEvent* event) { if (!confirmDocumentReplacement(tr("closing"))) { event->ignore(); return; } saveWindowLayout(); event->accept(); }
void MainWindow::restoreWindowLayout() {
    QSettings settings("evnekdev", "ternary-contours-qt");
    const auto geometry = settings.value("window/geometry").toByteArray();
    if (!geometry.isEmpty()) QMainWindow::restoreGeometry(geometry);
    const auto data = settings.value("splitter/data").toByteArray(); if (!data.isEmpty()) ui_->splitterData->restoreState(data);
    const auto outer = settings.value("splitter/viewer-outer").toByteArray();
    const auto controls = settings.value("splitter/viewer-controls").toByteArray();
    const auto right = settings.value("splitter/viewer-right").toByteArray();
    if (!outer.isEmpty()) ui_->splitterViewerOuter->restoreState(outer); else ui_->splitterViewerOuter->setSizes({340, 940});
    if (!controls.isEmpty()) ui_->splitterViewerControls->restoreState(controls); else ui_->splitterViewerControls->setSizes({420, 420});
    if (!right.isEmpty()) ui_->splitterViewerRight->restoreState(right); else ui_->splitterViewerRight->setSizes({620, 250});
    for (const auto& section : viewer_sections_) section->restore();
}
void MainWindow::saveWindowLayout() {
    QSettings settings("evnekdev", "ternary-contours-qt"); settings.setValue("window/geometry", QMainWindow::saveGeometry());
    settings.setValue("splitter/data", ui_->splitterData->saveState());
    settings.setValue("splitter/viewer-outer", ui_->splitterViewerOuter->saveState());
    settings.setValue("splitter/viewer-controls", ui_->splitterViewerControls->saveState());
    settings.setValue("splitter/viewer-right", ui_->splitterViewerRight->saveState());
}