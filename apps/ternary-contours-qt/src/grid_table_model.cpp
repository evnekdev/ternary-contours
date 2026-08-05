#include "grid_table_model.hpp"
#include "rust_bridge.hpp"

#include <QBrush>
#include <QLocale>

namespace {
QString cText(const char* value) { return QString::fromUtf8(value); }
QString statusText(const TcqtStatus& status) { return cText(status.message); }
}

GridTableModel::GridTableModel(QObject* parent) : QAbstractTableModel(parent) {}

void GridTableModel::clear() {
    beginResetModel(); headers_.clear(); rows_.clear(); endResetModel();
}

void GridTableModel::load(std::uint32_t grid_index, const QStringList& component_names) {
    TcqtGrid grid{};
    const auto grid_status = tcqt_grid_at(grid_index, &grid);
    if (!grid_status.success) { clear(); emit bridgeStatus(statusText(grid_status), false); return; }
    beginResetModel();
    grid_index_ = grid_index; regular_ = grid.kind == 0; component_names_ = component_names;
    headers_ = component_names_;
    for (std::uint32_t field_index = 0; field_index < grid.field_count; ++field_index) {
        TcqtField field{}; const auto field_status = tcqt_grid_field_at(grid_index, field_index, &field);
        headers_.append(field_status.success ? cText(field.column_name) : tr("Unavailable field"));
    }
    rows_.clear(); rows_.reserve(static_cast<qsizetype>(grid.row_count));
    for (std::uint32_t row_index = 0; row_index < grid.row_count; ++row_index) {
        TcqtRow source{}; const auto row_status = tcqt_grid_row_at(grid_index, row_index, &source);
        if (!row_status.success) continue;
        Row row{source.a, source.b, source.c, {}};
        for (std::uint32_t field_index = 0; field_index < grid.field_count; ++field_index) {
            TcqtCell cell{}; const auto cell_status = tcqt_grid_cell_at(grid_index, field_index, row_index, &cell);
            row.fields.append(cell_status.success ? tokenForCell(cell.state, cell.has_value, cell.value, cell.note) : tr("NA"));
        }
        rows_.append(std::move(row));
    }
    endResetModel();
}

std::uint32_t GridTableModel::gridIndex() const { return grid_index_; }
bool GridTableModel::isRegular() const { return regular_; }
int GridTableModel::rowCount(const QModelIndex& parent) const { return parent.isValid() ? 0 : rows_.size(); }
int GridTableModel::columnCount(const QModelIndex& parent) const { return parent.isValid() ? 0 : headers_.size(); }

QVariant GridTableModel::data(const QModelIndex& index, int role) const {
    if (!index.isValid() || index.row() >= rows_.size()) return {};
    const auto& row = rows_.at(index.row());
    const auto composition = index.column() < 3;
    if (role == Qt::DisplayRole || role == Qt::EditRole) {
        if (composition) return QLocale::c().toString(index.column() == 0 ? row.a : index.column() == 1 ? row.b : row.c, 'f', 6);
        return row.fields.value(index.column() - 3);
    }
    if (role == Qt::BackgroundRole && composition && regular_) return QBrush(Qt::lightGray);
    if (role == Qt::ToolTipRole && !composition) return tr("Classified scalar input: number, NA, NE, or CO[:note]");
    return {};
}

QVariant GridTableModel::headerData(int section, Qt::Orientation orientation, int role) const {
    if (orientation == Qt::Horizontal && role == Qt::DisplayRole) return headers_.value(section);
    if (orientation == Qt::Vertical && role == Qt::DisplayRole) return section + 1;
    return {};
}

Qt::ItemFlags GridTableModel::flags(const QModelIndex& index) const {
    if (!index.isValid()) return Qt::NoItemFlags;
    auto result = Qt::ItemIsEnabled | Qt::ItemIsSelectable;
    if (index.column() >= 3 || !regular_) result |= Qt::ItemIsEditable;
    return result;
}

bool GridTableModel::setData(const QModelIndex& index, const QVariant& value, int role) {
    if (!index.isValid() || role != Qt::EditRole) return false;
    TcqtStatus status{};
    const auto token = value.toString().toUtf8();
    if (index.column() >= 3) {
        status = tcqt_set_grid_cell(grid_index_, static_cast<std::uint32_t>(index.column() - 3), static_cast<std::uint32_t>(index.row()), token.constData());
    } else {
        bool ok = false; const auto numeric = QLocale::c().toDouble(value.toString(), &ok);
        if (!ok) { emit bridgeStatus(tr("Composition values must be finite numbers"), false); return false; }
        auto row = rows_.at(index.row());
        if (index.column() == 0) row.a = numeric; else if (index.column() == 1) row.b = numeric; else row.c = numeric;
        status = tcqt_set_irregular_composition(grid_index_, static_cast<std::uint32_t>(index.row()), row.a, row.b, row.c);
    }
    emit bridgeStatus(statusText(status), status.success);
    if (!status.success) return false;
    load(grid_index_, component_names_);
    return true;
}

QString GridTableModel::tokenForCell(std::uint32_t state, bool has_value, double value, const char* note) const {
    if (has_value) return QLocale::c().toString(value, 'g', 15);
    const auto suffix = cText(note).trimmed();
    const auto base = state == 1 ? QStringLiteral("NE") : state == 2 ? QStringLiteral("CO") : QStringLiteral("NA");
    return suffix.isEmpty() ? base : base + QStringLiteral(":") + suffix;
}