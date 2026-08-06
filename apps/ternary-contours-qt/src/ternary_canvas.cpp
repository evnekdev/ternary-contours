#include "ternary_canvas.hpp"
#include "scalar_state_appearance.hpp"

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
    return {QPointF(center.x(), center.y() - height / 2.0),
            QPointF(center.x() - side / 2.0, center.y() + height / 2.0),
            QPointF(center.x() + side / 2.0, center.y() + height / 2.0)};
}
}

TernaryCanvas::TernaryCanvas(QWidget* parent) : QWidget(parent) {
    setAccessibleName("Unified ternary viewer");
    setMinimumSize(280, 220);
    setMouseTracking(true);
}
void TernaryCanvas::setPlotVisible(bool visible) { plot_visible_ = visible; update(); }
void TernaryCanvas::setGridVisible(bool visible) { grid_visible_ = visible; update(); }
void TernaryCanvas::setSourceVerticesVisible(bool visible) { source_vertices_visible_ = visible; update(); }
void TernaryCanvas::setQueryPointsVisible(bool visible) { query_points_visible_ = visible; update(); }
void TernaryCanvas::setComponentNamesVisible(bool visible) { component_names_visible_ = visible; update(); }
void TernaryCanvas::setAxisLabelsVisible(bool visible) { axis_labels_visible_ = visible; update(); }
void TernaryCanvas::setLegendVisible(bool visible) { legend_visible_ = visible; update(); }
void TernaryCanvas::setComponentNames(const QStringList& names) { if (names.size() == 3) component_names_ = names; update(); }
void TernaryCanvas::setSourceVertices(const QVector<QPointF>& compositions) { source_vertices_ = compositions; update(); }
void TernaryCanvas::setInspectionVertices(const QVector<CanvasVertex>& vertices) { inspection_vertices_ = vertices; update(); }
void TernaryCanvas::setProjectionPaths(const QVector<CanvasPath>& paths) { projection_paths_ = paths; update(); }
void TernaryCanvas::setProjectionVisibility(bool master, bool isotherms, bool univariants,
                                            bool binary_invariants, bool interior_invariants) {
    plot_visible_ = master;
    show_isotherms_ = isotherms;
    show_univariants_ = univariants;
    show_binary_invariants_ = binary_invariants;
    show_interior_invariants_ = interior_invariants;
    update();
}
void TernaryCanvas::setProjectionPathDisplayMode(int mode) { path_display_mode_ = std::clamp(mode, 0, 2); update(); }
void TernaryCanvas::setProjectionAppearance(int line_width, int invariant_marker_size) {
    line_width_ = std::clamp(line_width, 1, 8);
    invariant_marker_size_ = std::clamp(invariant_marker_size, 2, 20);
    update();
}
void TernaryCanvas::setDiagnosticVisibility(bool path_vertices, bool contour_endpoints,
                                            bool univariant_endpoints, bool invariant_ids,
                                            bool univariant_ids, bool phase_pair_labels) {
    show_path_vertices_ = path_vertices;
    show_contour_endpoints_ = contour_endpoints;
    show_univariant_endpoints_ = univariant_endpoints;
    show_invariant_ids_ = invariant_ids;
    show_univariant_ids_ = univariant_ids;
    show_phase_pair_labels_ = phase_pair_labels;
    update();
}
void TernaryCanvas::setVertexLabelSettings(int mode, int decimals, bool selected_only) {
    label_mode_ = std::clamp(mode, 0, 4);
    label_decimals_ = std::clamp(decimals, 0, 12);
    labels_selected_only_ = selected_only;
    update();
}
void TernaryCanvas::setQueries(const QVector<CanvasQuery>& queries) { queries_ = queries; update(); }
void TernaryCanvas::setInterpolationPreview(const std::optional<CanvasInterpolationPreview>& preview) { interpolation_preview_ = preview; update(); }
void TernaryCanvas::setContainingTriangleVisible(bool visible) { show_containing_triangle_ = visible; update(); }
void TernaryCanvas::setMarkerSize(int size) { marker_size_ = std::clamp(size, 2, 20); update(); }
void TernaryCanvas::setVertexVisibility(bool calculated, bool extrapolated, bool cut_off, bool missing) {
    show_calculated_ = calculated; show_extrapolated_ = extrapolated; show_cut_off_ = cut_off; show_missing_ = missing; update();
}
void TernaryCanvas::setInteractionMode(int mode) { interaction_mode_ = mode; update(); }
void TernaryCanvas::setSelectedRows(const QSet<std::uint32_t>& rows) { selected_rows_ = rows; update(); }
void TernaryCanvas::fitTriangleToView() { pan_ = {}; zoom_ = 1.0; update(); }
void TernaryCanvas::resetView() { fitTriangleToView(); }

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
    *c = 1.0 - *a - *b;
    return *a >= -1e-9 && *b >= -1e-9 && *c >= -1e-9;
}
std::optional<std::uint32_t> TernaryCanvas::vertexAt(const QPointF& point) const {
    const auto radius = static_cast<qreal>(marker_size_ + 4);
    std::optional<std::uint32_t> best; auto best_distance = radius * radius;
    for (const auto& vertex : inspection_vertices_) {
        if (!visibleState(vertex.state)) continue;
        const auto screen = pointForComposition(vertex.composition.x(), vertex.composition.y(), 1.0 - vertex.composition.x() - vertex.composition.y());
        const auto delta = screen - point; const auto distance = QPointF::dotProduct(delta, delta);
        if (distance <= best_distance) { best = vertex.row; best_distance = distance; }
    }
    return best;
}
QColor TernaryCanvas::colorForState(std::uint32_t state) const {
    const auto appearance = scalarStateAppearance(state);
    return appearance.filled ? appearance.fill : appearance.outline;
}
bool TernaryCanvas::visibleState(std::uint32_t state) const {
    return state == 0 ? show_calculated_ : state == 4 ? show_extrapolated_ : state == 2 ? show_cut_off_ : show_missing_;
}
void TernaryCanvas::mousePressEvent(QMouseEvent* event) {
    if (event->button() == Qt::MiddleButton) { panning_ = true; pan_origin_ = event->position(); setCursor(Qt::ClosedHandCursor); event->accept(); return; }
    if (event->button() == Qt::RightButton) {
        if (const auto row = vertexAt(event->position())) emit vertexContextRequested(*row, event->globalPosition().toPoint());
        else { panning_ = true; pan_origin_ = event->position(); setCursor(Qt::ClosedHandCursor); }
        event->accept(); return;
    }
    double a = 0.0, b = 0.0, c = 0.0;
    if (event->button() == Qt::LeftButton && compositionForPoint(event->position(), &a, &b, &c)) {
        if (interaction_mode_ == 0) {
            selected_composition_ = QPointF(a, b);
            if (const auto row = vertexAt(event->position())) emit vertexSelected(*row, event->modifiers().testFlag(Qt::ShiftModifier));
            else emit compositionSelected(a, b, c);
        }
        update();
    }
}
void TernaryCanvas::mouseDoubleClickEvent(QMouseEvent* event) {
    if (event->button() != Qt::LeftButton) return;
    if (interaction_mode_ == 0) {
        if (const auto row = vertexAt(event->position())) emit vertexDoubleClicked(*row, event->globalPosition().toPoint());
        return;
    }
    double a = 0.0, b = 0.0, c = 0.0;
    if (compositionForPoint(event->position(), &a, &b, &c)) emit interpolationRequested(a, b, c);
}
void TernaryCanvas::mouseMoveEvent(QMouseEvent* event) { if (!panning_) return; pan_ += event->position() - pan_origin_; pan_origin_ = event->position(); update(); }
void TernaryCanvas::mouseReleaseEvent(QMouseEvent* event) {
    if (panning_ && (event->button() == Qt::MiddleButton || event->button() == Qt::RightButton)) { panning_ = false; unsetCursor(); }
}
void TernaryCanvas::wheelEvent(QWheelEvent* event) { const auto factor = event->angleDelta().y() > 0 ? 1.12 : 1.0 / 1.12; zoom_ = std::clamp(zoom_ * factor, 0.35, 8.0); update(); event->accept(); }

void TernaryCanvas::paintEvent(QPaintEvent*) {
    QPainter painter(this); painter.setRenderHint(QPainter::Antialiasing, true); painter.fillRect(rect(), palette().base());
    const auto t = triangle(); QPainterPath boundary; boundary.addPolygon(t); boundary.closeSubpath(); painter.setPen(QPen(palette().text().color(), 1.5)); painter.setBrush(Qt::NoBrush); painter.drawPath(boundary);
    if (grid_visible_) {
        painter.setPen(QPen(palette().mid().color(), 0.75, Qt::DashLine));
        for (int step = 1; step < 10; ++step) { const auto value = step / 10.0; painter.drawLine(pointForComposition(value, 1.0 - value, 0.0), pointForComposition(value, 0.0, 1.0 - value)); painter.drawLine(pointForComposition(1.0 - value, value, 0.0), pointForComposition(0.0, value, 1.0 - value)); painter.drawLine(pointForComposition(1.0 - value, 0.0, value), pointForComposition(0.0, 1.0 - value, value)); }
    }
    if (plot_visible_) {
        for (const auto& path : projection_paths_) {
            const bool visible = path.type == 0 ? show_isotherms_
                : path.type == 1 ? show_univariants_
                : path.type == 2 ? show_binary_invariants_
                : path.type == 3 ? show_interior_invariants_ : true;
            const bool source_visible = path_display_mode_ == 2 || path.path_source == static_cast<std::uint32_t>(path_display_mode_);
            if (!visible || !source_visible || path.compositions.isEmpty()) continue;
            const QColor color = QColor::fromRgba(path.rgba);
            if (path.compositions.size() == 1) {
                const auto source = path.compositions.front();
                const auto point = pointForComposition(source.x(), source.y(), 1.0 - source.x() - source.y());
                painter.setPen(QPen(color, 1.5)); painter.setBrush(color);
                if (path.marker_kind == 1) painter.drawRect(QRectF(point.x() - invariant_marker_size_ / 2.0, point.y() - invariant_marker_size_ / 2.0, invariant_marker_size_, invariant_marker_size_));
                else painter.drawEllipse(point, invariant_marker_size_ / 2.0, invariant_marker_size_ / 2.0);
                if ((path.type == 2 || path.type == 3) && show_invariant_ids_) {
                    painter.drawText(point + QPointF(invariant_marker_size_ + 2, -2), path.line_id);
                }
                continue;
            }
            QPainterPath curve; const auto first = path.compositions.front(); curve.moveTo(pointForComposition(first.x(), first.y(), 1.0 - first.x() - first.y()));
            for (qsizetype index = 1; index < path.compositions.size(); ++index) { const auto point = path.compositions.at(index); curve.lineTo(pointForComposition(point.x(), point.y(), 1.0 - point.x() - point.y())); }
            painter.setPen(QPen(color, path.stroke_width * line_width_ / 2.0)); painter.setBrush(Qt::NoBrush); painter.drawPath(curve);
            if (show_path_vertices_) { painter.setBrush(color); for (const auto& source : path.compositions) painter.drawEllipse(pointForComposition(source.x(), source.y(), 1.0 - source.x() - source.y()), 1.8, 1.8); }
            if ((path.type == 0 && show_contour_endpoints_) || (path.type == 1 && show_univariant_endpoints_)) {
                painter.setBrush(color); for (const auto& source : {path.compositions.front(), path.compositions.back()}) painter.drawEllipse(pointForComposition(source.x(), source.y(), 1.0 - source.x() - source.y()), 3.0, 3.0);
            }
            if (path.type == 1 && (show_univariant_ids_ || show_phase_pair_labels_)) {
                const auto middle = path.compositions.at(path.compositions.size() / 2);
                const auto label_point = pointForComposition(middle.x(), middle.y(), 1.0 - middle.x() - middle.y());
                painter.setPen(color);
                const auto label = show_phase_pair_labels_ && !path.phase_pair.isEmpty()
                    ? path.phase_pair
                    : path.line_id;
                painter.drawText(label_point + QPointF(5.0, -5.0), label);
            }
        }
        if (projection_paths_.isEmpty() && source_vertices_.isEmpty()) { painter.setPen(palette().mid().color()); painter.drawText(rect(), Qt::AlignCenter, tr("No calculated projection to display")); }
    }
    if (source_vertices_visible_) {
        for (const auto& vertex : inspection_vertices_) {
            if (!visibleState(vertex.state)) continue;
            const auto point = pointForComposition(vertex.composition.x(), vertex.composition.y(), 1.0 - vertex.composition.x() - vertex.composition.y());
            const auto selected = selected_rows_.contains(vertex.row);
            const auto appearance = scalarStateAppearance(vertex.state);
            painter.setPen(QPen(selected ? QColor(30, 90, 210) : appearance.outline, selected ? 2.5 : 1.4));
            painter.setBrush(appearance.filled ? QBrush(appearance.fill) : Qt::NoBrush);
            if (appearance.marker == ScalarMarkerShape::Square) {
                painter.drawRect(QRectF(point.x() - marker_size_ * 0.5, point.y() - marker_size_ * 0.5, marker_size_, marker_size_));
            } else if (appearance.marker == ScalarMarkerShape::Triangle) {
                QPolygonF triangle_marker;
                triangle_marker << point + QPointF(0, -marker_size_) << point + QPointF(marker_size_, marker_size_) << point + QPointF(-marker_size_, marker_size_);
                painter.drawPolygon(triangle_marker);
            } else {
                painter.drawEllipse(point, marker_size_ * 0.55, marker_size_ * 0.55);
            }
            if (label_mode_ != 0 && (!labels_selected_only_ || selected)) {
                QString label;
                const auto state = vertex.state == 0 ? tr("Calculated") : vertex.state == 4 ? tr("EX") : vertex.state == 2 ? tr("CO") : tr("NA");
                if (label_mode_ == 1) label = vertex.label.section(QLatin1Char(':'), 0, 0);
                else if (label_mode_ == 2) label = state;
                else if (label_mode_ == 3) label = QString::number(vertex.row + 1);
                else label = vertex.label.section(QLatin1Char(':'), 0, 0) + QStringLiteral(" (") + state + QStringLiteral(")");
                painter.setPen(palette().text().color()); painter.drawText(point + QPointF(marker_size_ + 2, -2), label);
            }
        }
    }
    if (interpolation_preview_) {
        painter.setPen(QPen(QColor(146, 68, 173), 2.2, Qt::DashLine));
        painter.setBrush(QColor(146, 68, 173, 34));
        QPolygonF preview_triangle;
        for (const auto& source : interpolation_preview_->containing_triangle) {
            preview_triangle << pointForComposition(source.x(), source.y(), 1.0 - source.x() - source.y());
        }
        if (preview_triangle.size() == 3) painter.drawPolygon(preview_triangle);
        for (const auto& vertex : inspection_vertices_) {
            if (!interpolation_preview_->source_rows.contains(vertex.row)) continue;
            const auto point = pointForComposition(vertex.composition.x(), vertex.composition.y(), 1.0 - vertex.composition.x() - vertex.composition.y());
            painter.setBrush(Qt::NoBrush);
            painter.drawEllipse(point, marker_size_ + 3.0, marker_size_ + 3.0);
        }
        const auto point = pointForComposition(interpolation_preview_->composition.x(), interpolation_preview_->composition.y(),
                                                1.0 - interpolation_preview_->composition.x() - interpolation_preview_->composition.y());
        painter.setPen(QPen(QColor(146, 68, 173), 2.5));
        painter.setBrush(QColor(255, 255, 255, 120));
        painter.drawEllipse(point, 7.0, 7.0);
        painter.drawLine(point + QPointF(-10, 0), point + QPointF(10, 0));
        painter.drawLine(point + QPointF(0, -10), point + QPointF(0, 10));
    }
    if (show_containing_triangle_) {
        painter.setPen(QPen(QColor(185, 60, 185), 1.8, Qt::DashLine));
        painter.setBrush(QColor(185, 60, 185, 28));
        for (const auto& query : queries_) {
            if (!query.selected || query.containing_triangle.size() != 3) continue;
            QPolygonF triangle;
            for (const auto& source : query.containing_triangle) {
                triangle << pointForComposition(source.x(), source.y(), 1.0 - source.x() - source.y());
            }
            painter.drawPolygon(triangle);
        }
    }
    if (query_points_visible_) for (const auto& query : queries_) { const auto point = pointForComposition(query.composition.x(), query.composition.y(), 1.0 - query.composition.x() - query.composition.y()); painter.setPen(QPen(query.selected ? QColor(220, 50, 50) : QColor(70, 70, 210), query.selected ? 2.5 : 1.4)); painter.setBrush(Qt::NoBrush); painter.drawEllipse(point, query.selected ? 6.0 : 4.0, query.selected ? 6.0 : 4.0); }
    if (component_names_visible_) { painter.setPen(palette().text().color()); painter.drawText(t[0] + QPointF(-22, -10), component_names_.value(0, QStringLiteral("A"))); painter.drawText(t[1] + QPointF(-30, 20), component_names_.value(1, QStringLiteral("B"))); painter.drawText(t[2] + QPointF(10, 20), component_names_.value(2, QStringLiteral("C"))); }    if (axis_labels_visible_) {
        painter.setPen(palette().mid().color());
        painter.drawText(QRectF(8.0, height() - 24.0, width() - 16.0, 18.0), Qt::AlignCenter, tr("A / B / C composition fractions"));
    }
    if (legend_visible_) {
        const QRectF legend(10.0, 10.0, 148.0, 76.0);
        painter.setPen(QPen(palette().mid().color())); painter.setBrush(palette().base()); painter.drawRect(legend);
        painter.setPen(palette().text().color());
        painter.drawText(legend.adjusted(7.0, 5.0, -7.0, -5.0), tr("Calculated (circle)\nExtrapolated (square)\nCut-off (triangle)\nMissing (hollow circle)"));
    }
    if (selected_composition_.x() >= 0.0) { painter.setPen(QPen(QColor(200, 60, 40), 2)); painter.setBrush(Qt::NoBrush); painter.drawEllipse(pointForComposition(selected_composition_.x(), selected_composition_.y(), 1.0 - selected_composition_.x() - selected_composition_.y()), 6, 6); }
}