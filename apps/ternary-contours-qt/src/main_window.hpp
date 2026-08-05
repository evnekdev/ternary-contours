#pragma once

#include "rust_bridge.hpp"

#include <QMainWindow>
#include <QPoint>
#include <QSet>
#include <QString>
#include <QVector>
#include <cstdint>
#include <memory>

class GridTableModel;
class QStandardItemModel;
class QCloseEvent;
class QModelIndex;
class QLineEdit;

namespace Ui { class MainWindow; }

enum class ViewerAction {
    SelectGrid, SelectPhase, SelectProperty, SetInteractionMode,
    SetSourceVertices, SetVertexFilter, SetMarkerSize, SetNumericalOptions,
    SetPlotLayer, SetGridLayer, SetQueryPoints, SetResultsVisible,
    Fit, Reset, RestoreLayout, AddQuery, RemoveSelectedQuery, RemoveAllQueries,
    ResetAutomaticRange, SelectVertex, CommitVertexEdit, CommitBulkState,
};

struct ViewerQuery {
    std::uint64_t id = 0;
    std::uint32_t grid_index = 0;
    std::uint32_t phase_id = 0;
    QString property;
    double a = 0.0;
    double b = 0.0;
    double c = 0.0;
    TcqtInspectionResult result{};
};

struct ViewerState {
    std::uint32_t grid_index = 0;
    std::uint32_t phase_id = 0;
    std::uint32_t field_index = 0;
    QString property;
    int interaction_mode = 0;
    bool show_source_vertices = true;
    bool show_calculated = true;
    bool show_non_existing = true;
    bool show_cut_off = true;
    bool show_missing = true;
    bool show_query_points = true;
    bool show_results_table = true;
    bool show_plot_layer = true;
    bool show_grid_layer = true;
    bool show_corner_names = true;
    int marker_size = 6;
    QSet<std::uint32_t> selected_rows;
    QVector<ViewerQuery> queries;
    std::uint64_t next_query_id = 1;
    std::uint64_t calculation_generation = 0;
    std::uint64_t numerical_revision = 1;
    bool calculation_running = false;
    bool has_last_valid_projection = false;
    TcqtViewerCalculationOptions options{true, 0.0, 0.0, 100.0, 0, true, 0.0, 0, 3, 2, 1};
};

class MainWindow final : public QMainWindow {
    Q_OBJECT
public:
    explicit MainWindow(QWidget* parent = nullptr);
    ~MainWindow() override;
protected:
    void closeEvent(QCloseEvent* event) override;
private slots:
    void newDocument();
    void openDocument();
    void saveDocument();
    void saveDocumentAs();
    void exportPng();
    void exportSvg();
    void exportLinesCsv();
    void addGrid(bool regular);
    void removeSelectedGrid();
    void duplicateSelectedGrid();
    void renameSelectedGrid();
    void addPhase();
    void removeSelectedPhase();
    void addProperty();
    void addIrregularRow();
    void runRustCalculation();
    void updateComposition(double a, double b, double c);
    void selectProjectNode(const QModelIndex& index);
    void commitTitle();
    void commitComponentA();
    void commitComponentB();
    void commitComponentC();
    void copyGridSelection();
    void pasteGridClipboard();
    void showGridContextMenu(const QPoint& position);
private:
    bool saveToPath(const QString& path);
    bool performSave(bool save_as);
    void commitPendingEditors();
    bool confirmDocumentReplacement(const QString& action);
    void rebuildFromRust(std::uint32_t preferred_grid = 0);
    void rebuildTree();
    void updateActionState();
    void updateViewerActionState();
    void updateWindowTitle();
    void updateDocumentPresentation();
    void reportBridgeStatus(const QString& message, bool success);
    void restoreWindowLayout();
    void saveWindowLayout();
    void dispatchViewerAction(ViewerAction action);
    void refreshViewerFieldSelectors();
    void refreshViewerVertices();
    void refreshViewerQueries();
    void refreshProjectionCanvas();
    void scheduleViewerCalculation();
    void addInterpolationQuery(double a, double b, double c);
    void selectViewerVertex(std::uint32_t row, bool additive);
    void editViewerVertex(std::uint32_t row, const QPoint& global_position);
    void showViewerVertexContextMenu(std::uint32_t row, const QPoint& global_position);
    bool commitViewerNumber(QLineEdit* editor, double* target, const QString& label);

    std::unique_ptr<Ui::MainWindow> ui_;
    QStandardItemModel* tree_model_ = nullptr;
    GridTableModel* grid_model_ = nullptr;
    std::uint32_t selected_grid_ = 0;
    std::uint32_t selected_phase_id_ = 0;
    ViewerState viewer_;
    bool synchronizing_ = false;
    bool editor_commit_failed_ = false;
};