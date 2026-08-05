#pragma once

#include <QMainWindow>
#include <QPoint>
#include <cstdint>
#include <memory>

class GridTableModel;
class QStandardItemModel;
class QCloseEvent;
class QModelIndex;

namespace Ui { class MainWindow; }

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
    void updateWindowTitle();
    void updateDocumentPresentation();
    void reportBridgeStatus(const QString& message, bool success);
    void restoreWindowLayout();
    void saveWindowLayout();

    std::unique_ptr<Ui::MainWindow> ui_;
    QStandardItemModel* tree_model_ = nullptr;
    GridTableModel* grid_model_ = nullptr;
    std::uint32_t selected_grid_ = 0;
    std::uint32_t selected_phase_id_ = 0;
    bool synchronizing_ = false;
    bool editor_commit_failed_ = false;
};
