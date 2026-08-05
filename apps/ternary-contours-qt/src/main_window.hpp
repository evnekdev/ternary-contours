#pragma once

#include <QMainWindow>
#include <memory>

namespace Ui {
class MainWindow;
}

class MainWindow final : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(QWidget* parent = nullptr);
    ~MainWindow() override;

private slots:
    void openDocument();
    void saveGeometry();
    void runRustCalculation();
    void updateComposition(double a, double b, double c);

private:
    void restoreGeometry();
    std::unique_ptr<Ui::MainWindow> ui_;
};