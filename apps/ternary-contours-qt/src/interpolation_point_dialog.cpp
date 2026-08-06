#include "interpolation_point_dialog.hpp"

#include "ui_interpolation_point_dialog.h"

#include <QAbstractButton>
#include <QDialogButtonBox>
#include <QGroupBox>
#include <QLabel>
#include <QLocale>
#include <QLineEdit>
#include <QPushButton>
#include <QSignalBlocker>

#include <cmath>

namespace {
constexpr double coordinate_tolerance = 1.0e-12;

QString bridgeMessage(const TcqtStatus& status) {
    return QString::fromUtf8(status.message);
}

QString number(double value) {
    return QLocale::c().toString(value, 'g', 17);
}

QString sumText(double sum, bool normalized) {
    return normalized ? QStringLiteral("Σ = 1")
                      : QStringLiteral("Σ = %1 — Not normalized").arg(number(sum));
}
}

InterpolationPointDialog::InterpolationPointDialog(
    std::uint32_t grid_index, const QStringList& component_names,
    const TcqtLocatedPoint& initial_location, QWidget* parent)
    : QDialog(parent), ui_(std::make_unique<Ui::InterpolationPointDialog>()),
      grid_index_(grid_index), component_names_(component_names) {
    ui_->setupUi(this);
    ui_->labelGlobalA->setText(component_names_.value(0, QStringLiteral("A")));
    ui_->labelGlobalB->setText(component_names_.value(1, QStringLiteral("B")));
    ui_->labelGlobalC->setText(component_names_.value(2, QStringLiteral("C")));

    auto* ok_button = ui_->buttonBoxInterpolationPoint->button(QDialogButtonBox::Ok);
    ok_button->setAutoDefault(false);
    ok_button->setDefault(false);
    connect(ui_->buttonBoxInterpolationPoint, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(ui_->buttonBoxInterpolationPoint, &QDialogButtonBox::clicked, this,
            [this, ok_button](QAbstractButton* button) {
                if (button == ok_button) handleOk();
            });

    for (auto* editor : globalEditors()) {
        connect(editor, &QLineEdit::textChanged, this, [this] { markGlobalEdited(); });
        connect(editor, &QLineEdit::editingFinished, this, &InterpolationPointDialog::validateGlobalOnFocusLoss);
        connect(editor, &QLineEdit::returnPressed, this, &InterpolationPointDialog::normalizeGlobalFromEditors);
    }
    for (auto* editor : localEditors()) {
        connect(editor, &QLineEdit::textChanged, this, [this] { markLocalEdited(); });
        connect(editor, &QLineEdit::editingFinished, this, &InterpolationPointDialog::validateLocalOnFocusLoss);
        connect(editor, &QLineEdit::returnPressed, this, &InterpolationPointDialog::normalizeLocalFromEditors);
    }

    setLocation(initial_location, false);
}

InterpolationPointDialog::~InterpolationPointDialog() = default;

TcqtLocatedPoint InterpolationPointDialog::acceptedLocation() const { return location_; }

bool InterpolationPointDialog::coordinatesSynchronized() const {
    return global_state_ == CoordinateTripletState::Synchronized
        && local_state_ == CoordinateTripletState::Synchronized;
}

std::array<QLineEdit*, 3> InterpolationPointDialog::globalEditors() const {
    return {ui_->editGlobalA, ui_->editGlobalB, ui_->editGlobalC};
}

std::array<QLineEdit*, 3> InterpolationPointDialog::localEditors() const {
    return {ui_->editLocal0, ui_->editLocal1, ui_->editLocal2};
}

void InterpolationPointDialog::markGlobalEdited() {
    if (writing_editors_) return;
    global_state_ = CoordinateTripletState::EditedUnnormalized;
    authority_ = CoordinateAuthority::Global;
}

void InterpolationPointDialog::markLocalEdited() {
    if (writing_editors_) return;
    local_state_ = CoordinateTripletState::EditedUnnormalized;
    authority_ = CoordinateAuthority::Local;
}

bool InterpolationPointDialog::parseTriplet(const std::array<QLineEdit*, 3>& editors,
                                             Triplet* output, QString* error) const {
    for (int index = 0; index < 3; ++index) {
        const auto text = editors[index]->text().trimmed();
        if (text.isEmpty()) {
            *error = tr("Coordinate %1 is blank.").arg(index + 1);
            return false;
        }
        bool parsed = false;
        const auto value = QLocale::c().toDouble(text, &parsed);
        if (!parsed) {
            *error = tr("Coordinate %1 is not a C-locale number.").arg(index + 1);
            return false;
        }
        (*output)[index] = value;
    }
    return true;
}

void InterpolationPointDialog::setStatus(const QString& message, bool error) {
    ui_->labelCoordinateStatus->setText(message);
    ui_->labelCoordinateStatus->setAccessibleDescription(message);
    ui_->labelCoordinateStatus->setStyleSheet(error ? QStringLiteral("color: #b03030;") : QString());
}

void InterpolationPointDialog::updateTripletFeedback(bool global, bool valid, double sum,
                                                      const QString& message) {
    const auto editors = global ? globalEditors() : localEditors();
    const auto normalized = valid && std::abs(sum - 1.0) <= coordinate_tolerance;
    for (auto* editor : editors) {
        editor->setProperty("coordinateWarning", !valid || !normalized);
        editor->setStyleSheet(!valid || !normalized ? QStringLiteral("background: #fff1d6;") : QString());
    }
    auto* sum_label = global ? ui_->labelGlobalSumValue : ui_->labelLocalSumValue;
    sum_label->setText(valid ? sumText(sum, normalized) : QStringLiteral("Σ = —"));
    sum_label->setAccessibleDescription(valid ? sum_label->text() : message);
    setStatus(message, !valid || !normalized);
}

void InterpolationPointDialog::clearFeedback() {
    for (auto* editor : globalEditors()) {
        editor->setProperty("coordinateWarning", false);
        editor->setStyleSheet(QString());
    }
    for (auto* editor : localEditors()) {
        editor->setProperty("coordinateWarning", false);
        editor->setStyleSheet(QString());
    }
    ui_->labelGlobalSumValue->setText(QStringLiteral("Σ = 1"));
    ui_->labelLocalSumValue->setText(QStringLiteral("Σ = 1"));
    setStatus(tr("Coordinates are synchronized."), false);
}

void InterpolationPointDialog::validateGlobalOnFocusLoss() {
    if (writing_editors_ || global_state_ == CoordinateTripletState::Synchronized) return;
    Triplet values{};
    QString error;
    if (!parseTriplet(globalEditors(), &values, &error)) {
        global_state_ = CoordinateTripletState::Invalid;
        updateTripletFeedback(true, false, 0.0, error);
        return;
    }
    const auto status = tcqt_validate_coordinate_triplet(values[0], values[1], values[2]);
    if (!status.success) {
        global_state_ = CoordinateTripletState::Invalid;
        updateTripletFeedback(true, false, 0.0, bridgeMessage(status));
        return;
    }
    global_state_ = CoordinateTripletState::EditedUnnormalized;
    const auto sum = values[0] + values[1] + values[2];
    const auto message = std::abs(sum - 1.0) <= coordinate_tolerance
        ? tr("Global coordinates are edited; press Enter or OK to synchronize.")
        : tr("Coordinates are not normalized; press Enter or OK to normalize.");
    updateTripletFeedback(true, true, sum, message);
}

void InterpolationPointDialog::validateLocalOnFocusLoss() {
    if (writing_editors_ || local_state_ == CoordinateTripletState::Synchronized) return;
    Triplet values{};
    QString error;
    if (!parseTriplet(localEditors(), &values, &error)) {
        local_state_ = CoordinateTripletState::Invalid;
        updateTripletFeedback(false, false, 0.0, error);
        return;
    }
    const auto status = tcqt_validate_coordinate_triplet(values[0], values[1], values[2]);
    if (!status.success) {
        local_state_ = CoordinateTripletState::Invalid;
        updateTripletFeedback(false, false, 0.0, bridgeMessage(status));
        return;
    }
    local_state_ = CoordinateTripletState::EditedUnnormalized;
    const auto sum = values[0] + values[1] + values[2];
    const auto message = std::abs(sum - 1.0) <= coordinate_tolerance
        ? tr("Local coordinates are edited; press Enter or OK to synchronize.")
        : tr("Coordinates are not normalized; press Enter or OK to normalize.");
    updateTripletFeedback(false, true, sum, message);
}

void InterpolationPointDialog::normalizeGlobalFromEditors() {
    Triplet values{};
    QString error;
    if (!parseTriplet(globalEditors(), &values, &error)) {
        global_state_ = CoordinateTripletState::Invalid;
        updateTripletFeedback(true, false, 0.0, error);
        return;
    }
    TcqtLocatedPoint location{};
    const auto status = tcqt_locate_grid_point(grid_index_, values[0], values[1], values[2], &location);
    if (!status.success) {
        global_state_ = CoordinateTripletState::Invalid;
        updateTripletFeedback(true, false, 0.0, bridgeMessage(status));
        return;
    }
    setLocation(location, true);
}

void InterpolationPointDialog::normalizeLocalFromEditors() {
    Triplet values{};
    QString error;
    if (!parseTriplet(localEditors(), &values, &error)) {
        local_state_ = CoordinateTripletState::Invalid;
        updateTripletFeedback(false, false, 0.0, error);
        return;
    }
    TcqtLocatedPoint location{};
    const auto status = tcqt_locate_grid_local_point(
        grid_index_, location_.triangle_index, values[0], values[1], values[2], &location);
    if (!status.success) {
        local_state_ = CoordinateTripletState::Invalid;
        updateTripletFeedback(false, false, 0.0, bridgeMessage(status));
        return;
    }
    setLocation(location, true);
}

bool InterpolationPointDialog::normalizeAuthoritativeTriplet() {
    if (authority_ == CoordinateAuthority::Global) {
        normalizeGlobalFromEditors();
    } else {
        normalizeLocalFromEditors();
    }
    return coordinatesSynchronized() && location_.success;
}

void InterpolationPointDialog::handleOk() {
    if (coordinatesSynchronized()) {
        accept();
        return;
    }
    if (!normalizeAuthoritativeTriplet()) return;
    // A first OK is deliberately only a correction/synchronization action.
    // It leaves the dialog open and retains focus on OK for the second click.
    ui_->buttonBoxInterpolationPoint->button(QDialogButtonBox::Ok)->setFocus();
}

void InterpolationPointDialog::setLocation(const TcqtLocatedPoint& location, bool emit_preview) {
    writing_editors_ = true;
    location_ = location;
    const auto globals = globalEditors();
    const auto locals = localEditors();
    const std::array<double, 3> global_values{location.a, location.b, location.c};
    const std::array<double, 3> local_values{location.lambda0, location.lambda1, location.lambda2};
    for (int index = 0; index < 3; ++index) {
        const QSignalBlocker global_blocker(globals[index]);
        const QSignalBlocker local_blocker(locals[index]);
        globals[index]->setText(number(global_values[index]));
        locals[index]->setText(number(local_values[index]));
    }
    writing_editors_ = false;
    global_state_ = CoordinateTripletState::Synchronized;
    local_state_ = CoordinateTripletState::Synchronized;
    updateTriangleDetails();
    clearFeedback();
    if (emit_preview) emit previewLocationChanged(location_);
}

void InterpolationPointDialog::updateTriangleDetails() {
    ui_->labelTriangleIdValue->setText(QString::number(location_.triangle_index));
    ui_->groupLocalCoordinates->setTitle(tr("Local coordinates in triangle %1").arg(location_.triangle_index));
    const std::array<std::uint32_t, 3> rows{location_.source_row0, location_.source_row1, location_.source_row2};
    const std::array<QLabel*, 3> labels{ui_->labelTriangleRow0, ui_->labelTriangleRow1, ui_->labelTriangleRow2};
    const std::array<QLabel*, 3> lambda_labels{ui_->labelLocal0, ui_->labelLocal1, ui_->labelLocal2};
    for (int index = 0; index < 3; ++index) {
        TcqtRow row{};
        const auto status = tcqt_grid_row_at(grid_index_, rows[index], &row);
        const auto row_number = rows[index] + 1;
        if (status.success) {
            labels[index]->setText(tr("Source row %1: (%2, %3, %4)")
                .arg(row_number).arg(number(row.a), number(row.b), number(row.c)));
        } else {
            labels[index]->setText(tr("Source row %1 is unavailable").arg(row_number));
        }
        lambda_labels[index]->setText(tr("λ%1 — row %2").arg(index).arg(row_number));
        lambda_labels[index]->setToolTip(labels[index]->text());
    }
}