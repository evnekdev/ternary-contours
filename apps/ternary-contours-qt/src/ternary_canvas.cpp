#include "ternary_canvas.hpp"

#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QWheelEvent>
#include <algorithm>
#include <cmath>

namespace {
QPolygonF baseTriangle(const QRectF& bounds) {
    const auto side = std::min(bounds.width() * 0.84, bounds.height() * 0.76);
    const auto height = side * std::sqrt(3.0) / 2.0;
    const auto center = bounds.center();
    return {QPointF(center.x(), center.y() - height / 2.0), QPointF(center.x() - side / 2.0, center.y() + height / 2.0), QPointF(center.x() + side / 2.0, center.y() + height / 2.0)};
}
}

TernaryCanvas::TernaryCanvas(QWidget* parent) : QWidget(parent) { setAccessibleName("Unified ternary viewer"); setMinimumSize(280, 220); setMouseTracking(true); }
void TernaryCanvas::setPlotVisible(bool visible) { plot_visible_ = visible; update(); }
void TernaryCanvas::setGridVisible(bool visible) { grid_visible_ = visible; update(); }
void TernaryCanvas::setComponentNames(const QStringList& names) { if (names.size() == 3) component_names_ = names; update(); }
void TernaryCanvas::setSourceVertices(const QVector<QPointF>& compositions) { source_vertices_ = compositions; update(); }
void TernaryCanvas::fitTriangleToView() { pan_ = {}; zoom_ = 1.0; update(); }

QPolygonF TernaryCanvas::triangle() const {
    const auto base = baseTriangle(rect()); const auto center = rect().center(); QPolygonF result;
    for (const auto& point : base) result << center + (point - center) * zoom_ + pan_;
    return result;
}
QPointF TernaryCanvas::pointForComposition(double a, double b, double c) const { const auto t = triangle(); return t[0] * a + t[1] * b + t[2] * c; }
bool TernaryCanvas::compositionForPoint(const QPointF& point, double* a, double* b, double* c) const {
    const auto t = triangle(); const auto denominator = (t[1].y() - t[2].y()) * (t[0].x() - t[2].x()) + (t[2].x() - t[1].x()) * (t[0].y() - t[2].y());
    if (std::abs(denominator) < 1e-9) return false;
    *a = ((t[1].y() - t[2].y()) * (point.x() - t[2].x()) + (t[2].x() - t[1].x()) * (point.y() - t[2].y())) / denominator;
    *b = ((t[2].y() - t[0].y()) * (point.x() - t[2].x()) + (t[0].x() - t[2].x()) * (point.y() - t[2].y())) / denominator;
    *c = 1.0 - *a - *b; return *a >= -1e-9 && *b >= -1e-9 && *c >= -1e-9;
}
void TernaryCanvas::mousePressEvent(QMouseEvent* event) {
    if (event->button() == Qt::MiddleButton || event->button() == Qt::RightButton) { panning_ = true; pan_origin_ = event->position(); setCursor(Qt::ClosedHandCursor); event->accept(); return; }
    double a = 0.0, b = 0.0, c = 0.0; if (event->button() == Qt::LeftButton && compositionForPoint(event->position(), &a, &b, &c)) { selected_composition_ = QPointF(a, b); emit compositionSelected(a, b, c); update(); }
}
void TernaryCanvas::mouseMoveEvent(QMouseEvent* event) { if (!panning_) return; pan_ += event->position() - pan_origin_; pan_origin_ = event->position(); update(); }
void TernaryCanvas::mouseReleaseEvent(QMouseEvent* event) { if (panning_ && (event->button() == Qt::MiddleButton || event->button() == Qt::RightButton)) { panning_ = false; unsetCursor(); } }
void TernaryCanvas::wheelEvent(QWheelEvent* event) { const auto factor = event->angleDelta().y() > 0 ? 1.12 : 1.0 / 1.12; zoom_ = std::clamp(zoom_ * factor, 0.35, 8.0); update(); event->accept(); }

void TernaryCanvas::paintEvent(QPaintEvent*) {
    QPainter painter(this); painter.setRenderHint(QPainter::Antialiasing, true); painter.fillRect(rect(), palette().base());
    const auto t = triangle(); QPainterPath boundary; boundary.addPolygon(t); painter.setPen(QPen(palette().text().color(), 1.5)); painter.setBrush(Qt::NoBrush); painter.drawPath(boundary);
    if (grid_visible_) {
        painter.setPen(QPen(palette().mid().color(), 0.75, Qt::DashLine));
        for (int step = 1; step < 10; ++step) { const auto value = step / 10.0; painter.drawLine(pointForComposition(value, 1.0 - value, 0.0), pointForComposition(value, 0.0, 1.0 - value)); painter.drawLine(pointForComposition(1.0 - value, value, 0.0), pointForComposition(0.0, value, 1.0 - value)); painter.drawLine(pointForComposition(1.0 - value, 0.0, value), pointForComposition(0.0, 1.0 - value, value)); }
    }
    if (plot_visible_ && source_vertices_.isEmpty()) { painter.setPen(palette().mid().color()); painter.drawText(rect(), Qt::AlignCenter, tr("No calculated projection to display")); }
    painter.setPen(QPen(QColor(35, 110, 180), 1.2)); painter.setBrush(QColor(225, 242, 255));
    for (const auto& point : source_vertices_) { const auto a = point.x(); const auto b = point.y(); painter.drawEllipse(pointForComposition(a, b, 1.0 - a - b), 3.5, 3.5); }
    painter.setPen(palette().text().color()); painter.drawText(t[0] + QPointF(-22, -10), component_names_.value(0, QStringLiteral("A"))); painter.drawText(t[1] + QPointF(-30, 20), component_names_.value(1, QStringLiteral("B"))); painter.drawText(t[2] + QPointF(10, 20), component_names_.value(2, QStringLiteral("C")));
    if (selected_composition_.x() >= 0.0) { painter.setPen(QPen(QColor(200, 60, 40), 2)); painter.setBrush(QColor(255, 220, 210)); painter.drawEllipse(pointForComposition(selected_composition_.x(), selected_composition_.y(), 1.0 - selected_composition_.x() - selected_composition_.y()), 5, 5); }
}