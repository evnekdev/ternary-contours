#pragma once

#include "rust_bridge.hpp"

#include <QDialog>
#include <QStringList>
#include <array>
#include <memory>

class QGroupBox;
class QLineEdit;
class QLabel;
class QPushButton;

namespace Ui { class InterpolationPointDialog; }

class InterpolationPointDialog final : public QDialog {
    Q_OBJECT
public:
    enum class CoordinateTripletState { Synchronized, EditedUnnormalized, Invalid };
    enum class CoordinateAuthority { Global, Local };

    InterpolationPointDialog(std::uint32_t grid_index, const QStringList& component_names,
                             const TcqtLocatedPoint& initial_location, QWidget* parent = nullptr);
    ~InterpolationPointDialog() override;

    [[nodiscard]] TcqtLocatedPoint acceptedLocation() const;
    [[nodiscard]] bool coordinatesSynchronized() const;

signals:
    /// Emitted only after Rust has successfully normalized, transformed, and
    /// located a new preview point. It never creates an interpolation query.
    void previewLocationChanged(const TcqtLocatedPoint& location);

private:
    using Triplet = std::array<double, 3>;

    void markGlobalEdited();
    void markLocalEdited();
    void validateGlobalOnFocusLoss();
    void validateLocalOnFocusLoss();
    void normalizeGlobalFromEditors();
    void normalizeLocalFromEditors();
    void handleOk();
    [[nodiscard]] bool normalizeAuthoritativeTriplet();
    [[nodiscard]] bool parseTriplet(const std::array<QLineEdit*, 3>& editors, Triplet* output,
                                    QString* error) const;
    void setLocation(const TcqtLocatedPoint& location, bool emit_preview);
    void updateTriangleDetails();
    void updateTripletFeedback(bool global, bool valid, double sum, const QString& message);
    void clearFeedback();
    void setStatus(const QString& message, bool error);
    [[nodiscard]] std::array<QLineEdit*, 3> globalEditors() const;
    [[nodiscard]] std::array<QLineEdit*, 3> localEditors() const;

    std::unique_ptr<Ui::InterpolationPointDialog> ui_;
    std::uint32_t grid_index_ = 0;
    QStringList component_names_;
    TcqtLocatedPoint location_{};
    CoordinateTripletState global_state_ = CoordinateTripletState::Synchronized;
    CoordinateTripletState local_state_ = CoordinateTripletState::Synchronized;
    CoordinateAuthority authority_ = CoordinateAuthority::Global;
    bool writing_editors_ = false;
};