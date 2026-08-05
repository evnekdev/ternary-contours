#include "main_window.hpp"
#include "rust_bridge.hpp"
#include "ternary_canvas.hpp"
#include "ui_main_window.h"

#include <QAction>
#include <QApplication>
#include <QByteArray>
#include <QFileDialog>
#include <QFutureWatcher>
#include <QSettings>
#include <QStandardItemModel>
#include <QtConcurrentRun>
#include <algorithm>

namespace {
QString fromMessage(const TcqtCalculationResult& result) {
    return QString::fromUtf8(result.message);
}

TcqtCalculationResult fallbackCalculation() {
    TcqtCalculationResult result{};
    result.success = false;
    const auto message = QByteArray("Rust bridge is not linked; build rust-bridge and pass TCQT_RUST_BRIDGE_LIBRARY.");
    std::copy_n(message.constData(), std::min<int>(message.size(), 127), result.message);
    return result;
}

TcqtCalculationResult calculateOnWorker() {
#ifdef TCQT_WITH_RUST_BRIDGE
    return tcqt_run_feasibility_calculation(10, 1);
#else
    return fallbackCalculation();
#endif
}
}

MainWindow::MainWindow(QWidget* parent)
    : QMainWindow(parent), ui_(std::make_unique<Ui::MainWindow>()) {
    ui_->setupUi(this);

    auto* tree_model = new QStandardItemModel(ui_->treeProject);
    tree_model->setHorizontalHeaderLabels({tr("Project")});
    auto* project = new QStandardItem(tr("Project"));
    project->appendRow(new QStandardItem(tr("Title")));
    auto* corners = new QStandardItem(tr("Corners"));
    corners->appendRow(new QStandardItem(tr("A corner")));
    corners->appendRow(new QStandardItem(tr("B corner")));
    corners->appendRow(new QStandardItem(tr("C corner")));
    project->appendRow(corners);
    project->appendRow(new QStandardItem(tr("Phases")));
    project->appendRow(new QStandardItem(tr("Properties")));
    project->appendRow(new QStandardItem(tr("Grids")));
    tree_model->appendRow(project);
    ui_->treeProject->setModel(tree_model);
    ui_->treeProject->expandAll();

    auto* grid_model = new QStandardItemModel(0, 3, ui_->tableGridValues);
    grid_model->setHorizontalHeaderLabels({tr("A"), tr("B"), tr("C")});
    ui_->tableGridValues->setModel(grid_model);
    auto* results_model = new QStandardItemModel(0, 6, ui_->tableInterpolationResults);
    results_model->setHorizontalHeaderLabels({tr("Index"), tr("A"), tr("B"), tr("C"), tr("Value"), tr("State")});
    ui_->tableInterpolationResults->setModel(results_model);

    for (auto* action : {ui_->actionGridDuplicate, ui_->actionGridRename, ui_->actionGridRemove,
             ui_->actionGridAddPhaseField, ui_->actionGridModifyPhaseField, ui_->actionGridRemovePhaseField,
             ui_->actionGridValidate, ui_->actionGridRecalculate, ui_->actionGridCopy, ui_->actionGridPaste}) {
        action->setEnabled(false);
    }
    ui_->statusMain->showMessage(tr("Ready - Qt 6 feasibility prototype"));

    connect(ui_->actionFileOpen, &QAction::triggered, this, &MainWindow::openDocument);
    connect(ui_->actionQuit, &QAction::triggered, this, &QWidget::close);
    connect(ui_->actionAboutQt, &QAction::triggered, qApp, &QApplication::aboutQt);
    connect(ui_->buttonRunRustCalculation, &QPushButton::clicked, this, &MainWindow::runRustCalculation);
    connect(ui_->actionViewPlot, &QAction::toggled, ui_->canvasTernary, &TernaryCanvas::setPlotVisible);
    connect(ui_->actionViewGrid, &QAction::toggled, ui_->canvasTernary, &TernaryCanvas::setGridVisible);
    connect(ui_->canvasTernary, &TernaryCanvas::compositionSelected, this, &MainWindow::updateComposition);
    restoreWindowLayout();
}

MainWindow::~MainWindow() { saveWindowLayout(); }

void MainWindow::openDocument() {
    const auto path = QFileDialog::getOpenFileName(
        this,
        tr("Open Ternary Contour Table"),
        {},
        tr("Ternary Contour Table (*.tct)"));
    if (!path.isEmpty()) ui_->statusMain->showMessage(tr("Open requested: %1").arg(path));
}

void MainWindow::runRustCalculation() {
    ui_->statusMain->showMessage(tr("Calculating on Rust worker..."));
    auto* watcher = new QFutureWatcher<TcqtCalculationResult>(this);
    connect(watcher, &QFutureWatcher<TcqtCalculationResult>::finished, this, [this, watcher] {
        const auto result = watcher->result();
        ui_->statusMain->showMessage(fromMessage(result));
        watcher->deleteLater();
    });
    watcher->setFuture(QtConcurrent::run(calculateOnWorker));
}

void MainWindow::updateComposition(double a, double b, double c) {
    ui_->statusMain->showMessage(
        tr("A=%1  B=%2  C=%3").arg(a, 0, 'f', 4).arg(b, 0, 'f', 4).arg(c, 0, 'f', 4));
}

void MainWindow::restoreWindowLayout() {
    QSettings settings("evnekdev", "ternary-contours-qt");
    const auto geometry = settings.value("window/geometry").toByteArray();
    if (!geometry.isEmpty()) {
        QMainWindow::restoreGeometry(geometry);
    }
    if (const auto data_state = settings.value("splitter/data").toByteArray(); !data_state.isEmpty()) {
        ui_->splitterData->restoreState(data_state);
    }
    if (const auto viewer_state = settings.value("splitter/viewer").toByteArray(); !viewer_state.isEmpty()) {
        ui_->splitterViewer->restoreState(viewer_state);
    }
}

void MainWindow::saveWindowLayout() {
    QSettings settings("evnekdev", "ternary-contours-qt");
    settings.setValue("window/geometry", QMainWindow::saveGeometry());
    settings.setValue("splitter/data", ui_->splitterData->saveState());
    settings.setValue("splitter/viewer", ui_->splitterViewer->saveState());
}