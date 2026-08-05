#pragma once

#include <cstdint>

extern "C" {
struct TcqtStatus { bool success; char message[512]; };
struct TcqtCalculationResult { bool success; std::uint64_t request_id; std::uint32_t vertex_count; char message[128]; };
struct TcqtProjectSummary {
    char title[128]; char path[512]; char component_a[128]; char component_b[128]; char component_c[128];
    std::uint32_t phase_count; std::uint32_t property_count; std::uint32_t grid_count;
    bool dirty; std::uint64_t revision;
};
struct TcqtPhase { std::uint32_t id; char name[128]; };
struct TcqtProperty { std::uint32_t ordinal; bool required; char name[128]; char unit[128]; };
struct TcqtGrid { std::uint32_t index; std::uint32_t kind; std::uint32_t subdivisions; std::uint32_t row_count; std::uint32_t field_count; char name[128]; };
struct TcqtField { std::uint32_t index; std::uint32_t phase_id; char property[128]; char column_name[128]; };
struct TcqtRow { double a; double b; double c; };
struct TcqtCell { std::uint32_t state; bool has_value; double value; char note[128]; };
struct TcqtProjectionRecord { double a; double b; double c; std::uint32_t point_index; std::uint32_t line_type; char line_id[128]; };

TcqtStatus tcqt_new_document();
TcqtStatus tcqt_open_document(const char* path);
TcqtStatus tcqt_save_document(const char* path);
TcqtStatus tcqt_project_summary(TcqtProjectSummary* output);
TcqtStatus tcqt_phase_at(std::uint32_t index, TcqtPhase* output);
TcqtStatus tcqt_property_at(std::uint32_t index, TcqtProperty* output);
TcqtStatus tcqt_grid_at(std::uint32_t index, TcqtGrid* output);
TcqtStatus tcqt_grid_field_at(std::uint32_t grid_index, std::uint32_t field_index, TcqtField* output);
TcqtStatus tcqt_grid_row_at(std::uint32_t grid_index, std::uint32_t row_index, TcqtRow* output);
TcqtStatus tcqt_grid_cell_at(std::uint32_t grid_index, std::uint32_t field_index, std::uint32_t row_index, TcqtCell* output);
TcqtStatus tcqt_set_title(const char* value);
TcqtStatus tcqt_set_component(std::uint32_t index, const char* value);
TcqtStatus tcqt_add_phase(const char* name);
TcqtStatus tcqt_remove_phase(std::uint32_t id);
TcqtStatus tcqt_add_property(const char* name, const char* unit, bool required);
TcqtStatus tcqt_remove_property(std::uint32_t ordinal);
TcqtStatus tcqt_add_regular_grid(const char* name, std::uint32_t subdivisions);
TcqtStatus tcqt_add_irregular_grid(const char* name);
TcqtStatus tcqt_remove_grid(std::uint32_t index);
TcqtStatus tcqt_rename_grid(std::uint32_t index, const char* name);
TcqtStatus tcqt_duplicate_grid(std::uint32_t index);
TcqtStatus tcqt_set_grid_cell(std::uint32_t grid_index, std::uint32_t field_index, std::uint32_t row_index, const char* token);
TcqtStatus tcqt_add_irregular_row(std::uint32_t grid_index);
TcqtStatus tcqt_set_irregular_composition(std::uint32_t grid_index, std::uint32_t row_index, double a, double b, double c);
TcqtStatus tcqt_undo();
TcqtStatus tcqt_redo();
TcqtCalculationResult tcqt_calculate_current();
TcqtStatus tcqt_projection_record_count(std::uint32_t* output);
TcqtStatus tcqt_projection_record_at(std::uint32_t index, TcqtProjectionRecord* output);
TcqtStatus tcqt_export_plot(const char* path, std::uint32_t format);
TcqtStatus tcqt_export_lines_csv(const char* path);
TcqtCalculationResult tcqt_run_feasibility_calculation(std::uint32_t subdivisions, std::uint64_t dataset_revision);
}