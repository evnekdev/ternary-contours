#include "main_window.hpp"

#include "grid_table_model.hpp"
#include "rust_bridge.hpp"
#include "ternary_canvas.hpp"
#include "ui_add_grid_dialog.h"
#include <algorithm>
#include <functional>
#include "ui_main_window.h"

#include <QAbstractItemView>
#include <limits>
#include <QAction>
#include <QApplication>
#include <QCloseEvent>
#include <QComboBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QGuiApplication>
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
#include <QMessageBox>
#include <QSettings>

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
QString text(const char* value) { return QString::fromUtf8(value); }
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
TcqtCalculationResult calculateOnWorker() { return tcqt_calculate_current(); }
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
    auto* results_model = new QStandardItemModel(0, 10, ui_->tableInterpolationResults);
    results_model->setHorizontalHeaderLabels({tr("Index"), tr("A"), tr("B"), tr("C"), tr("Value"), tr("State"), tr("Grid"), tr("Phase"), tr("Property"), tr("Method")});
    ui_->tableInterpolationResults->setModel(results_model);
    ui_->buttonRunRustCalculation->setText(tr("Calculate projection"));
    ui_->actionViewSourceVertices->setChecked(true);

    connect(ui_->actionFileNew, &QAction::triggered, this, &MainWindow::newDocument);
    connect(ui_->actionFileOpen, &QAction::triggered, this, &MainWindow::openDocument);
    connect(ui_->actionFileSave, &QAction::triggered, this, &MainWindow::saveDocument);
    connect(ui_->actionFileSaveAs, &QAction::triggered, this, &MainWindow::saveDocumentAs);
    connect(ui_->actionExportPng, &QAction::triggered, this, &MainWindow::exportPng);
    connect(ui_->actionExportSvg, &QAction::triggered, this, &MainWindow::exportSvg);
    connect(ui_->actionExportLinesCsv, &QAction::triggered, this, &MainWindow::exportLinesCsv);
    connect(ui_->actionQuit, &QAction::triggered, this, &QWidget::close);
    connect(ui_->actionAboutQt, &QAction::triggered, qApp, &QApplication::aboutQt);
    connect(ui_->actionGridAddRegular, &QAction::triggered, this, [this] { addGrid(true); });
    connect(ui_->actionGridAddIrregular, &QAction::triggered, this, [this] { addGrid(false); });
    connect(ui_->actionGridRemove, &QAction::triggered, this, &MainWindow::removeSelectedGrid);
    connect(ui_->actionGridDuplicate, &QAction::triggered, this, &MainWindow::duplicateSelectedGrid);
    connect(ui_->actionGridRename, &QAction::triggered, this, &MainWindow::renameSelectedGrid);
    connect(ui_->actionGridCopy, &QAction::triggered, this, &MainWindow::copyGridSelection);
    connect(ui_->actionGridPaste, &QAction::triggered, this, &MainWindow::pasteGridClipboard);
    connect(ui_->tableGridValues, &QTableView::customContextMenuRequested, this, &MainWindow::showGridContextMenu);
    connect(ui_->tableGridValues->selectionModel(), &QItemSelectionModel::selectionChanged, this, [this] { updateActionState(); });
    connect(QApplication::clipboard(), &QClipboard::dataChanged, this, [this] { updateActionState(); });
    connect(ui_->buttonAddPhase, &QPushButton::clicked, this, &MainWindow::addPhase);
    connect(ui_->buttonRemovePhase, &QPushButton::clicked, this, &MainWindow::removeSelectedPhase);
    connect(ui_->buttonAddProperty, &QPushButton::clicked, this, &MainWindow::addProperty);
    connect(ui_->buttonAddIrregularRow, &QPushButton::clicked, this, &MainWindow::addIrregularRow);
    connect(ui_->buttonRunRustCalculation, &QPushButton::clicked, this, &MainWindow::runRustCalculation);
    connect(ui_->actionViewPlot, &QAction::toggled, this, [this] { dispatchViewerAction(ViewerAction::SetPlotLayer); });
    connect(ui_->actionViewGrid, &QAction::toggled, this, [this] { dispatchViewerAction(ViewerAction::SetGridLayer); });
    connect(ui_->actionViewSourceVertices, &QAction::toggled, this, [this] { dispatchViewerAction(ViewerAction::SetSourceVertices); });
    connect(ui_->actionViewQueryPoints, &QAction::toggled, this, [this] { dispatchViewerAction(ViewerAction::SetQueryPoints); });
    connect(ui_->actionViewResultsTable, &QAction::toggled, this, [this] { dispatchViewerAction(ViewerAction::SetResultsVisible); });
    connect(ui_->actionViewFit, &QAction::triggered, this, [this] { dispatchViewerAction(ViewerAction::Fit); });
    connect(ui_->actionViewReset, &QAction::triggered, this, [this] { dispatchViewerAction(ViewerAction::Reset); });
    connect(ui_->actionViewRestoreLayout, &QAction::triggered, this, [this] { dispatchViewerAction(ViewerAction::RestoreLayout); });
    connect(ui_->actionViewerClearSelectedQuery, &QAction::triggered, this, [this] { dispatchViewerAction(ViewerAction::RemoveSelectedQuery); });
    connect(ui_->actionViewerClearAllQueries, &QAction::triggered, this, [this] { dispatchViewerAction(ViewerAction::RemoveAllQueries); });
    connect(ui_->actionViewerResetAutomaticRange, &QAction::triggered, this, [this] { dispatchViewerAction(ViewerAction::ResetAutomaticRange); });
    for (auto* action : {ui_->actionViewStableIsotherms, ui_->actionViewStableUnivariants, ui_->actionViewBinaryInvariants, ui_->actionViewInteriorInvariants, ui_->actionViewAxisLabels, ui_->actionViewCornerNames, ui_->actionViewLegend}) {
        connect(action, &QAction::toggled, this, [this] { dispatchViewerAction(ViewerAction::SetPlotLayer); });
    }
    connect(ui_->comboViewerGrid, qOverload<int>(&QComboBox::currentIndexChanged), this, [this] { dispatchViewerAction(ViewerAction::SelectGrid); });
    connect(ui_->comboViewerPhase, qOverload<int>(&QComboBox::currentIndexChanged), this, [this] { dispatchViewerAction(ViewerAction::SelectPhase); });
    connect(ui_->comboViewerProperty, qOverload<int>(&QComboBox::currentIndexChanged), this, [this] { dispatchViewerAction(ViewerAction::SelectProperty); });
    connect(ui_->comboViewerMode, qOverload<int>(&QComboBox::currentIndexChanged), this, [this] { dispatchViewerAction(ViewerAction::SetInteractionMode); });
    for (auto* toggle : {ui_->checkViewerCalculated, ui_->checkViewerNonExisting, ui_->checkViewerCutOff, ui_->checkViewerMissing}) {
        connect(toggle, &QCheckBox::toggled, this, [this] { dispatchViewerAction(ViewerAction::SetVertexFilter); });
    }
    connect(ui_->spinViewerMarkerSize, qOverload<int>(&QSpinBox::valueChanged), this, [this] { dispatchViewerAction(ViewerAction::SetMarkerSize); });
    connect(ui_->canvasTernary, &TernaryCanvas::compositionSelected, this, &MainWindow::updateComposition);
    connect(ui_->canvasTernary, &TernaryCanvas::vertexSelected, this, &MainWindow::selectViewerVertex);
    connect(ui_->canvasTernary, &TernaryCanvas::vertexDoubleClicked, this, &MainWindow::editViewerVertex);
    connect(ui_->canvasTernary, &TernaryCanvas::vertexContextRequested, this, &MainWindow::showViewerVertexContextMenu);
    connect(ui_->canvasTernary, &TernaryCanvas::interpolationRequested, this, &MainWindow::addInterpolationQuery);
    connect(ui_->treeProject->selectionModel(), &QItemSelectionModel::currentChanged, this, &MainWindow::selectProjectNode);
    connect(grid_model_, &GridTableModel::bridgeStatus, this, [this](const QString& message, bool success) { reportBridgeStatus(message, success); if (!success) editor_commit_failed_ = true; });
    connect(grid_model_, &GridTableModel::documentMutated, this, [this] { rebuildFromRust(selected_grid_); scheduleViewerCalculation(); });
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
    refreshProjectionCanvas();
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
    ui_->buttonRunRustCalculation->setEnabled(summary.calculation_available);
    ui_->buttonRunRustCalculation->setToolTip(summary.calculation_available
        ? tr("Calculate the current liquidus projection.")
        : tr("Calculation is unavailable: %1\nThe draft can still be saved.").arg(text(summary.blocking_reason)));
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
    if (result.success) rebuildFromRust();
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
                tr("Grid points: %1\nAllowed subdivisions: %2–%3")
                    .arg(point_text)
                    .arg(min_regular_subdivisions)
                    .arg(max_regular_subdivisions));
        } else {
            form.labelAddGridStepValue->setText(tr("Step size: —"));
            form.labelAddGridPointsValue->setText(tr("Grid points: —\nAllowed subdivisions: %1–%2").arg(min_regular_subdivisions).arg(max_regular_subdivisions));
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
    if (result.success) rebuildFromRust();
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

void MainWindow::showGridContextMenu(const QPoint& position) {
    updateActionState();
    QMenu menu(this);
    menu.addAction(ui_->actionGridCopy);
    menu.addAction(ui_->actionGridPaste);
    menu.exec(ui_->tableGridValues->mapToGlobal(position));
}

void MainWindow::runRustCalculation() {
    TcqtProjectSummary summary{};
    if (!tcqt_project_summary(&summary).success || !summary.calculation_available || viewer_.calculation_running) return;
    const auto revision = summary.revision;
    const auto generation = ++viewer_.calculation_generation;
    const auto options = viewer_.options;
    viewer_.calculation_running = true;
    updateViewerActionState();
    ui_->statusMain->showMessage(tr("Recalculating topology and iso-plots on the Rust worker…"));
    auto* watcher = new QFutureWatcher<TcqtCalculationResult>(this);
    connect(watcher, &QFutureWatcher<TcqtCalculationResult>::finished, this, [this, watcher, revision, generation] {
        TcqtProjectSummary latest{}; tcqt_project_summary(&latest); const auto result = watcher->result();
        viewer_.calculation_running = false;
        if (latest.revision != revision || generation != viewer_.calculation_generation) {
            reportBridgeStatus(tr("Ignored stale calculation result."), false);
        } else if (result.success) {
            viewer_.has_last_valid_projection = true;
            refreshProjectionCanvas();
            TcqtProjectionSummary projection{};
            if (tcqt_projection_summary(&projection).success && projection.available) {
                const auto source = QLocale::c();
                ui_->statusMain->showMessage(
                    tr("%1 Automatic range: %2–%3, %4 levels.")
                        .arg(calculationText(result), source.toString(projection.automatic_minimum, 'g', 8), source.toString(projection.source_maximum, 'g', 8))
                        .arg(projection.level_count), 10000);
            } else reportBridgeStatus(calculationText(result), true);
            refreshViewerQueries();
        } else {
            reportBridgeStatus(tr("%1 Previous valid plot remains visible.").arg(calculationText(result)), false);
        }
        updateViewerActionState(); watcher->deleteLater();
    });
    watcher->setFuture(QtConcurrent::run([options, revision, generation] { return tcqt_calculate_viewer(&options, revision, generation); }));
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
        ui_->canvasTernary->setInspectionVertices(vertices); return;
    }
    TcqtGrid grid{};
    if (!tcqt_grid_at(viewer_.grid_index, &grid).success) return;
    vertices.reserve(static_cast<qsizetype>(grid.row_count));
    for (std::uint32_t row = 0; row < grid.row_count; ++row) {
        TcqtRow composition{}; TcqtCell cell{};
        if (!tcqt_grid_row_at(viewer_.grid_index, row, &composition).success
            || !tcqt_grid_cell_at(viewer_.grid_index, viewer_.field_index, row, &cell).success) continue;
        CanvasVertex vertex; vertex.composition = QPointF(composition.a, composition.b); vertex.row = row; vertex.state = cell.state;
        const auto note = text(cell.note); const auto token = cell.has_value ? QLocale::c().toString(cell.value, 'g', 8) : cell.state == 1 ? QStringLiteral("NE") : cell.state == 2 ? QStringLiteral("CO") : QStringLiteral("NA");
        vertex.label = note.isEmpty() ? token : token + QStringLiteral(":") + note;
        vertices.append(vertex);
    }
    ui_->canvasTernary->setInspectionVertices(vertices);
    ui_->canvasTernary->setSourceVerticesVisible(viewer_.show_source_vertices);
    ui_->canvasTernary->setVertexVisibility(viewer_.show_calculated, viewer_.show_non_existing, viewer_.show_cut_off, viewer_.show_missing);
    ui_->canvasTernary->setMarkerSize(viewer_.marker_size);
    ui_->canvasTernary->setSelectedRows(viewer_.selected_rows);
}

void MainWindow::refreshProjectionCanvas() {
    std::uint32_t count = 0;
    if (!tcqt_projection_record_count(&count).success) return;
    QMap<QString, CanvasPath> paths;
    for (std::uint32_t index = 0; index < count; ++index) {
        TcqtProjectionRecord record{};
        if (!tcqt_projection_record_at(index, &record).success) continue;
        if ((record.line_type == 0 && !ui_->actionViewStableIsotherms->isChecked())
            || (record.line_type == 1 && !ui_->actionViewStableUnivariants->isChecked())
            || (record.line_type == 2 && !ui_->actionViewBinaryInvariants->isChecked())
            || (record.line_type == 3 && !ui_->actionViewInteriorInvariants->isChecked())) continue;
        const auto key = QString::number(record.line_type) + QStringLiteral(":") + text(record.line_id);
        auto& path = paths[key]; path.type = record.line_type; path.compositions.append(QPointF(record.a, record.b));
    }
    QVector<CanvasPath> output;
    for (auto it = paths.cbegin(); it != paths.cend(); ++it) output.append(it.value());
    ui_->canvasTernary->setProjectionPaths(output);
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
        row << new QStandardItem(result.has_value ? QLocale::c().toString(result.value, 'g', 12) : (result.state == 2 ? QStringLiteral("NE") : result.state == 3 ? QStringLiteral("CO") : QStringLiteral("NA")));
        row << new QStandardItem(text(result.message)) << new QStandardItem(QString::number(viewer_.grid_index)) << new QStandardItem(QString::number(viewer_.phase_id)) << new QStandardItem(viewer_.property) << new QStandardItem(result.local_mode == 0 ? tr("Cubic") : result.local_mode == 1 ? tr("One-sided cubic") : result.local_mode == 2 ? tr("Linear") : tr("Undefined"));
        model->appendRow(row);
    }
    QVector<CanvasQuery> queries;
    for (const auto& query : viewer_.queries) queries.append({query.id, QPointF(query.a, query.b), query.result.state, false});
    ui_->canvasTernary->setQueries(queries);
}

void MainWindow::dispatchViewerAction(ViewerAction action) {
    if (synchronizing_) return;
    switch (action) {
    case ViewerAction::SelectGrid:
        viewer_.grid_index = ui_->comboViewerGrid->currentData().toUInt(); viewer_.phase_id = 0; viewer_.property.clear(); viewer_.selected_rows.clear(); refreshViewerFieldSelectors(); refreshViewerVertices(); refreshViewerQueries(); break;
    case ViewerAction::SelectPhase:
        viewer_.phase_id = ui_->comboViewerPhase->currentData().toUInt(); viewer_.property.clear(); viewer_.selected_rows.clear(); refreshViewerFieldSelectors(); refreshViewerVertices(); refreshViewerQueries(); break;
    case ViewerAction::SelectProperty:
        viewer_.field_index = ui_->comboViewerProperty->currentData().toUInt(); viewer_.property = ui_->comboViewerProperty->currentText(); viewer_.selected_rows.clear(); refreshViewerVertices(); refreshViewerQueries(); break;
    case ViewerAction::SetInteractionMode:
        viewer_.interaction_mode = ui_->comboViewerMode->currentIndex(); ui_->canvasTernary->setInteractionMode(viewer_.interaction_mode); break;    case ViewerAction::SetVertexFilter:
        viewer_.show_calculated = ui_->checkViewerCalculated->isChecked();
        viewer_.show_non_existing = ui_->checkViewerNonExisting->isChecked();
        viewer_.show_cut_off = ui_->checkViewerCutOff->isChecked();
        viewer_.show_missing = ui_->checkViewerMissing->isChecked();
        refreshViewerVertices();
        break;
    case ViewerAction::SetMarkerSize:
        viewer_.marker_size = ui_->spinViewerMarkerSize->value();
        ui_->canvasTernary->setMarkerSize(viewer_.marker_size);
        break;
    case ViewerAction::SetPlotLayer:
        viewer_.show_plot_layer = ui_->actionViewPlot->isChecked(); ui_->canvasTernary->setPlotVisible(viewer_.show_plot_layer); ui_->canvasTernary->setComponentNamesVisible(ui_->actionViewCornerNames->isChecked()); ui_->canvasTernary->setAxisLabelsVisible(ui_->actionViewAxisLabels->isChecked()); ui_->canvasTernary->setLegendVisible(ui_->actionViewLegend->isChecked()); refreshProjectionCanvas(); break;
    case ViewerAction::SetGridLayer:
        viewer_.show_grid_layer = ui_->actionViewGrid->isChecked(); ui_->canvasTernary->setGridVisible(viewer_.show_grid_layer); break;
    case ViewerAction::SetSourceVertices:
        viewer_.show_source_vertices = ui_->actionViewSourceVertices->isChecked(); ui_->canvasTernary->setSourceVerticesVisible(viewer_.show_source_vertices); break;
    case ViewerAction::SetQueryPoints:
        viewer_.show_query_points = ui_->actionViewQueryPoints->isChecked(); ui_->canvasTernary->setQueryPointsVisible(viewer_.show_query_points); break;
    case ViewerAction::SetResultsVisible:
        viewer_.show_results_table = ui_->actionViewResultsTable->isChecked(); ui_->tableInterpolationResults->setVisible(viewer_.show_results_table); break;
    case ViewerAction::Fit: ui_->canvasTernary->fitTriangleToView(); break;
    case ViewerAction::Reset: ui_->canvasTernary->resetView(); break;
    case ViewerAction::RestoreLayout: restoreWindowLayout(); break;
    case ViewerAction::RemoveSelectedQuery:
        if (const auto current = ui_->tableInterpolationResults->currentIndex(); current.isValid()) viewer_.queries.removeAt(current.row()); refreshViewerQueries(); break;
    case ViewerAction::RemoveAllQueries: viewer_.queries.clear(); refreshViewerQueries(); break;
    case ViewerAction::ResetAutomaticRange: viewer_.options.automatic_range = true; scheduleViewerCalculation(); break;
    default: break;
    }
    updateViewerActionState();
}

void MainWindow::updateViewerActionState() {
    TcqtProjectSummary summary{};
    if (!tcqt_project_summary(&summary).success) return;
    const bool field_available = !viewer_.property.isEmpty() && viewer_.grid_index < summary.grid_count;
    ui_->comboViewerGrid->setEnabled(summary.grid_count > 0);
    ui_->comboViewerPhase->setEnabled(field_available);
    ui_->comboViewerProperty->setEnabled(field_available);
    ui_->comboViewerMode->setEnabled(field_available);
    ui_->actionViewSourceVertices->setEnabled(field_available);
    ui_->actionViewQueryPoints->setEnabled(!viewer_.queries.isEmpty());
    ui_->actionViewerClearSelectedQuery->setEnabled(ui_->tableInterpolationResults->currentIndex().isValid());
    ui_->actionViewerClearAllQueries->setEnabled(!viewer_.queries.isEmpty());
    ui_->actionViewStableIsotherms->setEnabled(summary.calculation_available);
    ui_->actionViewStableUnivariants->setEnabled(summary.calculation_available);
    ui_->actionViewBinaryInvariants->setEnabled(summary.calculation_available);
    ui_->actionViewInteriorInvariants->setEnabled(summary.calculation_available);
    ui_->buttonRunRustCalculation->setEnabled(summary.calculation_available && !viewer_.calculation_running);
    ui_->buttonRunRustCalculation->setToolTip(summary.calculation_available ? tr("Calculate automatically with current Viewer numerical settings.") : tr("Calculation requires valid participating phases, grids, and finite Temperature values."));
}

void MainWindow::scheduleViewerCalculation() {
    TcqtProjectSummary summary{};
    if (tcqt_project_summary(&summary).success && summary.calculation_available && !viewer_.calculation_running) runRustCalculation();
}

void MainWindow::addInterpolationQuery(double a, double b, double c) {
    if (viewer_.property.isEmpty()) { reportBridgeStatus(tr("Select a grid, phase, and property before adding a query."), false); return; }
    ViewerQuery query; query.id = viewer_.next_query_id++; query.a = a; query.b = b; query.c = c;
    viewer_.queries.append(query); refreshViewerQueries(); updateViewerActionState();
}

void MainWindow::selectViewerVertex(std::uint32_t row, bool additive) {
    if (!additive) viewer_.selected_rows.clear();
    if (additive && viewer_.selected_rows.contains(row)) viewer_.selected_rows.remove(row); else viewer_.selected_rows.insert(row);
    ui_->canvasTernary->setSelectedRows(viewer_.selected_rows);
    updateViewerActionState();
}

void MainWindow::editViewerVertex(std::uint32_t row, const QPoint& global_position) {
    if (viewer_.property.isEmpty()) return;
    TcqtCell existing{};
    if (!tcqt_grid_cell_at(viewer_.grid_index, viewer_.field_index, row, &existing).success) return;
    QDialog dialog(this, Qt::Tool); dialog.setWindowTitle(tr("Vertex %1 — %2 / %3").arg(row + 1).arg(viewer_.phase_id).arg(viewer_.property));
    auto* layout = new QFormLayout(&dialog); auto* state = new QComboBox(&dialog); state->addItem(tr("Calculated"), 0); state->addItem(tr("Missing (NA)"), 3); state->addItem(tr("Non-existing (NE)"), 1); state->addItem(tr("Cut-off (CO)"), 2); state->setCurrentIndex(state->findData(existing.state));
    auto* value = new QLineEdit(existing.has_value ? QLocale::c().toString(existing.value, 'g', 15) : QString(), &dialog); auto* note = new QLineEdit(text(existing.note), &dialog); auto* error = new QLabel(&dialog); error->setStyleSheet(QStringLiteral("color: #b03030"));
    layout->addRow(tr("State"), state); layout->addRow(tr("Value"), value); layout->addRow(tr("Note"), note); layout->addRow(error);
    auto commit = [&]() {
        const auto code = state->currentData().toUInt(); QString token;
        if (code == 0) { bool ok = false; const auto scalar = QLocale::c().toDouble(value->text(), &ok); if (!ok || !std::isfinite(scalar)) { error->setText(tr("Calculated requires one finite numeric value.")); return; } token = QLocale::c().toString(scalar, 'g', 15); }
        else token = code == 1 ? QStringLiteral("NE") : code == 2 ? QStringLiteral("CO") : QStringLiteral("NA");
        if (!note->text().trimmed().isEmpty()) token += QStringLiteral(":") + note->text().trimmed();
        const auto property = viewer_.property.toUtf8(); const auto encoded = token.toUtf8(); const auto result = tcqt_set_field_vertex(viewer_.grid_index, viewer_.phase_id, property.constData(), row, encoded.constData());
        if (!result.success) { error->setText(statusText(result)); return; }
        dialog.accept(); rebuildFromRust(selected_grid_); scheduleViewerCalculation(); reportBridgeStatus(tr("Updated vertex %1: %2").arg(row + 1).arg(token), true);
    };
    connect(value, &QLineEdit::returnPressed, &dialog, commit); connect(note, &QLineEdit::returnPressed, &dialog, commit); connect(state, qOverload<int>(&QComboBox::currentIndexChanged), &dialog, [=](int) { value->setEnabled(state->currentData().toUInt() == 0); });
    value->setEnabled(existing.state == 0); const auto screen = QGuiApplication::screenAt(global_position); const auto bounds = screen ? screen->availableGeometry() : geometry(); dialog.adjustSize(); auto pos = global_position + QPoint(12, 12); pos.setX(qBound(bounds.left(), pos.x(), bounds.right() - dialog.width())); pos.setY(qBound(bounds.top(), pos.y(), bounds.bottom() - dialog.height())); dialog.move(pos); value->setFocus(); dialog.exec();
}

void MainWindow::showViewerVertexContextMenu(std::uint32_t row, const QPoint& global_position) {
    if (viewer_.property.isEmpty()) return; if (!viewer_.selected_rows.contains(row)) { viewer_.selected_rows = {row}; ui_->canvasTernary->setSelectedRows(viewer_.selected_rows); }
    QMenu menu(this); auto* missing = menu.addAction(tr("Set selected to Missing (NA)")); auto* non_existing = menu.addAction(tr("Set selected to Non-existing (NE)")); auto* cut_off = menu.addAction(tr("Set selected to Cut-off (CO)")); auto* clear_notes = menu.addAction(tr("Clear notes"));
    const auto selected = menu.exec(global_position); if (!selected) return; const auto property = viewer_.property.toUtf8(); QVector<std::uint32_t> rows = viewer_.selected_rows.values(); TcqtStatus result{};
    if (selected == clear_notes) result = tcqt_clear_field_notes(viewer_.grid_index, viewer_.phase_id, property.constData(), rows.constData(), rows.size());
    else { const auto state = selected == non_existing ? 1U : selected == cut_off ? 2U : 3U; result = tcqt_bulk_set_field_state(viewer_.grid_index, viewer_.phase_id, property.constData(), rows.constData(), rows.size(), state); }
    reportBridgeStatus(statusText(result), result.success); if (result.success) { rebuildFromRust(selected_grid_); scheduleViewerCalculation(); }
}

bool MainWindow::commitViewerNumber(QLineEdit* editor, double* target, const QString& label) {
    bool ok = false; const auto value = QLocale::c().toDouble(editor->text(), &ok); if (!ok || !std::isfinite(value)) { editor->setToolTip(tr("%1 must be a finite number.").arg(label)); return false; } *target = value; return true;
}
void MainWindow::closeEvent(QCloseEvent* event) { if (!confirmDocumentReplacement(tr("closing"))) { event->ignore(); return; } saveWindowLayout(); event->accept(); }
void MainWindow::restoreWindowLayout() {
    QSettings settings("evnekdev", "ternary-contours-qt"); const auto geometry = settings.value("window/geometry").toByteArray();
    if (!geometry.isEmpty()) QMainWindow::restoreGeometry(geometry);
    const auto data = settings.value("splitter/data").toByteArray(); if (!data.isEmpty()) ui_->splitterData->restoreState(data);
    const auto viewer = settings.value("splitter/viewer").toByteArray(); if (!viewer.isEmpty()) ui_->splitterViewer->restoreState(viewer);
}
void MainWindow::saveWindowLayout() {
    QSettings settings("evnekdev", "ternary-contours-qt"); settings.setValue("window/geometry", QMainWindow::saveGeometry());
    settings.setValue("splitter/data", ui_->splitterData->saveState()); settings.setValue("splitter/viewer", ui_->splitterViewer->saveState());
}