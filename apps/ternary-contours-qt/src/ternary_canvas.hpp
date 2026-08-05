#pragma once

#include <QPointF>
#include <QWidget>

class TernaryCanvas final : public QWidget {
    Q_OBJECT

public:
    explicit TernaryCanvas(QWidget* parent = nullptr);
    void setPlotVisible(bool visible);
    void setGridVisible(bool visible);

signals:
    void compositionSelected(double a, double b, double c);

protected:
    void mousePressEvent(QMouseEvent* event) override;
    void paintEvent(QPaintEvent* event) override;

private:
    QPointF pointForComposition(double a, double b, double c) const;
    bool plot_visible_ = true;
    bool grid_visible_ = true;
    QPointF selected_composition_{-1.0, -1.0};
};