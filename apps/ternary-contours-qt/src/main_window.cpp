#include "main_window.hpp"

#include "grid_table_model.hpp"
#include "rust_bridge.hpp"
#include "ternary_canvas.hpp"
#include "ui_add_grid_dialog.h"
#include "ui_main_window.h"

#include <QAbstractItemView>
#include <limits>
#include <QAction>
#include <QApplication>
#include <QCloseEvent>
#include <QFileDialog>
#include <QFileInfo>
#include <QFutureWatcher>
#include <QInputDialog>
#include <QItemSelectionModel>
#include <QMessageBox>
#include <QSettings>

#include <QLocale>
#include <QPushButton>
#include <QStandardItem>
#include <QStandardItemModel>
#include <QtConcurrentRun>

namespace {
constexpr auto node_kind_role = Qt::UserRole;
constexpr auto node_id_role = Qt::UserRole + 1;
enum class NodeKind { Project, Title, Corner, Phase, Property, Grid, Field };
constexpr int min_regular_subdivisions = 1;
constexpr int max_regular_subdivisions = 50;
constexpr int default_regular_subdivisions = 10;
QString text(const char* value) { return QString::fromUtf8(value); }
QString statusText(const TcqtStatus& status) { return text(status.message); }
QString calculationText(const TcqtCalculationResult& result) { return text(result.message); }
QStandardItem* node(const QString& label, NodeKind kind, std::uint32_t id = 0) {
    auto* item = new QStandardItem(label);
    item->setEditable(false);
    item->setData(static_cast<int>(kind), node_kind_role);
    item->setData(id, node_id_role);
    return item;
}
TcqtCalculationResult calculateOnWorker() { return tcqt_calculate_current(); }
}

MainWindow::MainWindow(QWidget* parent) : QMainWindow(parent), ui_(std::make_unique<Ui::MainWindow>()) {
    ui_->setupUi(this);
    tree_model_ = new QStandardItemModel(ui_->treeProject);
    tree_model_->setHorizontalHeaderLabels({tr("Project")});
    ui_->treeProject->setModel(tree_model_);
    ui_->treeProject->setEditTriggers(QAbstractItemView::NoEditTriggers);
    grid_model_ = new GridTableModel(ui_->tableGridValues);
    ui_->tableGridValues->setModel(grid_model_);
    auto* results_model = new QStandardItemModel(0, 10, ui_->tableInterpolationResults);
    results_model->setHorizontalHeaderLabels({tr("Index"), tr("A"), tr("B"), tr("C"), tr("Value"), tr("State"), tr("Grid"), tr("Phase"), tr("Property"), tr("Method")});
    ui_->tableInterpolationResults->setModel(results_model);
    ui_->buttonRunRustCalculation->setText(tr("Calculate projection"));

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
    connect(ui_->buttonAddPhase, &QPushButton::clicked, this, &MainWindow::addPhase);
    connect(ui_->buttonRemovePhase, &QPushButton::clicked, this, &MainWindow::removeSelectedPhase);
    connect(ui_->buttonAddProperty, &QPushButton::clicked, this, &MainWindow::addProperty);
    connect(ui_->buttonAddIrregularRow, &QPushButton::clicked, this, &MainWindow::addIrregularRow);
    connect(ui_->buttonRunRustCalculation, &QPushButton::clicked, this, &MainWindow::runRustCalculation);
    connect(ui_->actionViewPlot, &QAction::toggled, ui_->canvasTernary, &TernaryCanvas::setPlotVisible);
    connect(ui_->actionViewGrid, &QAction::toggled, ui_->canvasTernary, &TernaryCanvas::setGridVisible);
    connect(ui_->actionViewFit, &QAction::triggered, ui_->canvasTernary, &TernaryCanvas::fitTriangleToView);
    connect(ui_->actionViewReset, &QAction::triggered, ui_->canvasTernary, &TernaryCanvas::fitTriangleToView);
    connect(ui_->canvasTernary, &TernaryCanvas::compositionSelected, this, &MainWindow::updateComposition);
    connect(ui_->treeProject->selectionModel(), &QItemSelectionModel::currentChanged, this, &MainWindow::selectProjectNode);
    connect(grid_model_, &GridTableModel::bridgeStatus, this, &MainWindow::reportBridgeStatus);
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
    rebuildTree(); updateWindowTitle(); updateActionState();
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
}

void MainWindow::rebuildTree() {
    TcqtProjectSummary summary{}; if (!tcqt_project_summary(&summary).success) return;
    tree_model_->clear(); tree_model_->setHorizontalHeaderLabels({tr("Project")});
    auto* project = node(text(summary.title), NodeKind::Project);
    project->appendRow(node(tr("Title: %1").arg(text(summary.title)), NodeKind::Title));
    auto* corners = node(tr("Corners"), NodeKind::Project);
    corners->appendRow(node(tr("A: %1").arg(text(summary.component_a)), NodeKind::Corner, 0));
    corners->appendRow(node(tr("B: %1").arg(text(summary.component_b)), NodeKind::Corner, 1));
    corners->appendRow(node(tr("C: %1").arg(text(summary.component_c)), NodeKind::Corner, 2)); project->appendRow(corners);
    auto* phases = node(tr("Phases"), NodeKind::Project);
    for (std::uint32_t index = 0; index < summary.phase_count; ++index) { TcqtPhase phase{}; if (tcqt_phase_at(index, &phase).success) phases->appendRow(node(tr("[%1] %2").arg(phase.id).arg(text(phase.name)), NodeKind::Phase, phase.id)); }
    project->appendRow(phases);
    auto* properties = node(tr("Properties"), NodeKind::Project);
    for (std::uint32_t index = 0; index < summary.property_count; ++index) { TcqtProperty property{}; if (tcqt_property_at(index, &property).success) properties->appendRow(node(tr("%1%2 (%3)").arg(text(property.name), property.required ? tr(" required") : QString(), text(property.unit)), NodeKind::Property, index)); }
    project->appendRow(properties);
    auto* grids = node(tr("Grids"), NodeKind::Project);
    for (std::uint32_t index = 0; index < summary.grid_count; ++index) {
        TcqtGrid grid{}; if (!tcqt_grid_at(index, &grid).success) continue;
        auto* grid_node = node(tr("%1 — %2 (%3 rows)").arg(grid.kind == 0 ? tr("Regular") : tr("Irregular"), text(grid.name)).arg(grid.row_count), NodeKind::Grid, index);
        for (std::uint32_t field_index = 0; field_index < grid.field_count; ++field_index) { TcqtField field{}; if (tcqt_grid_field_at(index, field_index, &field).success) grid_node->appendRow(node(text(field.column_name), NodeKind::Field, index)); }
        grids->appendRow(grid_node);
    }
    project->appendRow(grids); tree_model_->appendRow(project); ui_->treeProject->expandAll();
}
void MainWindow::updateActionState() {
    TcqtProjectSummary summary{};
    if (!tcqt_project_summary(&summary).success) return;
    const auto selected = ui_->treeProject->currentIndex();
    const auto kind = selected.data(node_kind_role).toInt();
    const bool grid_selected = kind == static_cast<int>(NodeKind::Grid);
    const bool phase_selected = kind == static_cast<int>(NodeKind::Phase);
    ui_->actionGridRemove->setEnabled(grid_selected);
    ui_->actionGridDuplicate->setEnabled(grid_selected);
    ui_->actionGridRename->setEnabled(grid_selected);
    ui_->buttonRemovePhase->setEnabled(phase_selected);
    ui_->buttonAddIrregularRow->setEnabled(grid_selected && !grid_model_->isRegular());
    ui_->actionGridValidate->setEnabled(grid_selected);
    ui_->actionGridRecalculate->setEnabled(grid_selected);
    ui_->actionGridCopy->setEnabled(grid_selected);
    ui_->actionGridPaste->setEnabled(grid_selected);
    ui_->buttonRunRustCalculation->setEnabled(summary.phase_count > 0 && summary.grid_count > 0);
}void MainWindow::updateWindowTitle() {
    TcqtProjectSummary summary{}; if (!tcqt_project_summary(&summary).success) return;
    const auto path = text(summary.path); const auto document_name = path.isEmpty() ? tr("Untitled") : QFileInfo(path).fileName();
    setWindowTitle(tr("Ternary Contours — %1%2").arg(document_name, summary.dirty ? QStringLiteral(" *") : QString()));
}

bool MainWindow::confirmDocumentReplacement(const QString& action) {
    TcqtProjectSummary summary{}; if (!tcqt_project_summary(&summary).success || !summary.dirty) return true;
    const auto choice = QMessageBox::warning(this, tr("Unsaved changes"), tr("The current document has unsaved changes.\n\nSave before %1?").arg(action), QMessageBox::Save | QMessageBox::Discard | QMessageBox::Cancel, QMessageBox::Save);
    if (choice == QMessageBox::Cancel) return false;
    if (choice == QMessageBox::Save) { saveDocument(); TcqtProjectSummary after{}; return tcqt_project_summary(&after).success && !after.dirty; }
    return true;
}

void MainWindow::newDocument() {
    if (!confirmDocumentReplacement(tr("creating a new project"))) return;
    const auto result = tcqt_new_document(); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust();
}

void MainWindow::openDocument() {
    if (!confirmDocumentReplacement(tr("opening another project"))) return;
    TcqtProjectSummary summary{}; tcqt_project_summary(&summary); const auto initial = text(summary.path);
    const auto path = QFileDialog::getOpenFileName(this, tr("Open Ternary Contour Table"), initial.isEmpty() ? QString() : QFileInfo(initial).absolutePath(), tr("Ternary Contour Table (*.tct)"));
    if (path.isEmpty()) return;
    const auto encoded = path.toUtf8(); const auto result = tcqt_open_document(encoded.constData()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust();
}

bool MainWindow::saveToPath(const QString& path) {
    if (path.isEmpty()) return false;
    const auto encoded = path.toUtf8(); const auto result = tcqt_save_document(encoded.constData()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_); return result.success;
}
void MainWindow::saveDocument() {
    TcqtProjectSummary summary{}; if (!tcqt_project_summary(&summary).success) return;
    const auto path = text(summary.path); if (path.isEmpty()) { saveDocumentAs(); return; } saveToPath(path);
}
void MainWindow::saveDocumentAs() {
    TcqtProjectSummary summary{}; tcqt_project_summary(&summary); const auto path = QFileDialog::getSaveFileName(this, tr("Save Ternary Contour Table"), text(summary.path), tr("Ternary Contour Table (*.tct)"));
    if (!path.isEmpty()) saveToPath(path);
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
    const auto index = ui_->treeProject->currentIndex(); if (index.data(node_kind_role).toInt() != static_cast<int>(NodeKind::Grid)) return;
    if (QMessageBox::question(this, tr("Remove grid"), tr("Remove the selected grid and its values?")) != QMessageBox::Yes) return;
    const auto result = tcqt_remove_grid(index.data(node_id_role).toUInt()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust();
}
void MainWindow::duplicateSelectedGrid() {
    const auto index = ui_->treeProject->currentIndex(); if (index.data(node_kind_role).toInt() != static_cast<int>(NodeKind::Grid)) return;
    const auto result = tcqt_duplicate_grid(index.data(node_id_role).toUInt()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_);
}
void MainWindow::renameSelectedGrid() {
    const auto index = ui_->treeProject->currentIndex(); if (index.data(node_kind_role).toInt() != static_cast<int>(NodeKind::Grid)) return;
    bool accepted = false; const auto name = QInputDialog::getText(this, tr("Rename grid"), tr("Grid name:"), QLineEdit::Normal, index.data(Qt::DisplayRole).toString().section(QStringLiteral(" — "), 1).section(QStringLiteral(" ("), 0, 0), &accepted); if (!accepted) return;
    const auto encoded = name.toUtf8(); const auto result = tcqt_rename_grid(index.data(node_id_role).toUInt(), encoded.constData()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_);
}void MainWindow::addPhase() {
    bool accepted = false; const auto name = QInputDialog::getText(this, tr("Add phase"), tr("Phase name:"), QLineEdit::Normal, {}, &accepted); if (!accepted) return;
    const auto encoded = name.toUtf8(); const auto result = tcqt_add_phase(encoded.constData()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_);
}
void MainWindow::removeSelectedPhase() {
    const auto index = ui_->treeProject->currentIndex(); if (index.data(node_kind_role).toInt() != static_cast<int>(NodeKind::Phase)) return;
    if (QMessageBox::question(this, tr("Remove phase"), tr("Removing this phase also removes its grid fields and values. Continue?")) != QMessageBox::Yes) return;
    const auto result = tcqt_remove_phase(index.data(node_id_role).toUInt()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_);
}
void MainWindow::addProperty() {
    bool accepted = false; const auto name = QInputDialog::getText(this, tr("Add property"), tr("Property name:"), QLineEdit::Normal, {}, &accepted); if (!accepted) return;
    const auto unit = QInputDialog::getText(this, tr("Property unit"), tr("Unit:"), QLineEdit::Normal, tr("1"), &accepted); if (!accepted) return;
    const auto name_encoded = name.toUtf8(); const auto unit_encoded = unit.toUtf8(); const auto result = tcqt_add_property(name_encoded.constData(), unit_encoded.constData(), false); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_);
}
void MainWindow::addIrregularRow() { const auto result = tcqt_add_irregular_row(selected_grid_); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_); }

void MainWindow::selectProjectNode(const QModelIndex& index) {
    if (!index.isValid() || synchronizing_) return;
    const auto kind = index.data(node_kind_role).toInt();
    if (kind == static_cast<int>(NodeKind::Grid) || kind == static_cast<int>(NodeKind::Field)) { selected_grid_ = index.data(node_id_role).toUInt(); rebuildFromRust(selected_grid_); }
    if (kind == static_cast<int>(NodeKind::Phase)) selected_phase_id_ = index.data(node_id_role).toUInt();
    updateActionState();
}

void MainWindow::commitTitle() { if (synchronizing_) return; const auto value = ui_->editProjectTitle->text().toUtf8(); const auto result = tcqt_set_title(value.constData()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_); }
void MainWindow::commitComponentA() { if (synchronizing_) return; const auto value = ui_->editCornerA->text().toUtf8(); const auto result = tcqt_set_component(0, value.constData()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_); }
void MainWindow::commitComponentB() { if (synchronizing_) return; const auto value = ui_->editCornerB->text().toUtf8(); const auto result = tcqt_set_component(1, value.constData()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_); }
void MainWindow::commitComponentC() { if (synchronizing_) return; const auto value = ui_->editCornerC->text().toUtf8(); const auto result = tcqt_set_component(2, value.constData()); reportBridgeStatus(statusText(result), result.success); if (result.success) rebuildFromRust(selected_grid_); }
void MainWindow::runRustCalculation() {
    TcqtProjectSummary summary{}; if (!tcqt_project_summary(&summary).success) return;
    const auto revision = summary.revision; ui_->buttonRunRustCalculation->setEnabled(false); ui_->statusMain->showMessage(tr("Calculating projection on Rust worker…"));
    auto* watcher = new QFutureWatcher<TcqtCalculationResult>(this);
    connect(watcher, &QFutureWatcher<TcqtCalculationResult>::finished, this, [this, watcher, revision] {
        TcqtProjectSummary latest{}; tcqt_project_summary(&latest); const auto result = watcher->result();
        if (latest.revision != revision) reportBridgeStatus(tr("Ignored stale calculation result; the project changed."), false);
        else reportBridgeStatus(calculationText(result), result.success);
        ui_->buttonRunRustCalculation->setEnabled(true); watcher->deleteLater();
    });
    watcher->setFuture(QtConcurrent::run(calculateOnWorker));
}
void MainWindow::updateComposition(double a, double b, double c) { ui_->statusMain->showMessage(tr("A=%1  B=%2  C=%3").arg(a, 0, 'f', 4).arg(b, 0, 'f', 4).arg(c, 0, 'f', 4)); }

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