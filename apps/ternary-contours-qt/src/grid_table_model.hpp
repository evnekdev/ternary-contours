#pragma once

#include <QAbstractTableModel>
#include <QStringList>
#include <QVector>

class GridTableModel final : public QAbstractTableModel {
    Q_OBJECT

public:
    explicit GridTableModel(QObject* parent = nullptr);
    void load(std::uint32_t grid_index, const QStringList& component_names);
    void clear();
    std::uint32_t gridIndex() const;
    bool isRegular() const;

    int rowCount(const QModelIndex& parent = {}) const override;
    int columnCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    QVariant headerData(int section, Qt::Orientation orientation, int role = Qt::DisplayRole) const override;
    Qt::ItemFlags flags(const QModelIndex& index) const override;
    bool setData(const QModelIndex& index, const QVariant& value, int role = Qt::EditRole) override;

signals:
    void bridgeStatus(const QString& message, bool success);

private:
    struct Row { double a; double b; double c; QVector<QString> fields; };
    QString tokenForCell(std::uint32_t state, bool has_value, double value, const char* note) const;

    std::uint32_t grid_index_ = 0;
    bool regular_ = true;
    QStringList component_names_;
    QStringList headers_;
    QVector<Row> rows_;
};