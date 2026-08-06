#pragma once

#include <cstdint>

extern "C" {
struct TcqtStatus { bool success; char message[512]; };
struct TcqtCalculationResult { bool success; std::uint64_t request_id; std::uint64_t dataset_revision; std::uint64_t options_revision; std::uint32_t vertex_count; char message[128]; };
struct TcqtViewerCalculationOptions {
    bool automatic_range; double minimum; double maximum; double level_step;
    std::uint32_t sampling_subdivisions; bool regularize; double regularization_spacing;
    std::uint32_t source_interpolation; std::uint32_t cubic_method;
    std::uint32_t partial_domain_policy; std::uint32_t continuation;
};
struct TcqtViewerCalculationState { TcqtViewerCalculationOptions options; std::uint64_t options_revision; };
struct TcqtProjectionSummary {
    bool available; double source_minimum; double source_maximum; double automatic_minimum;
    bool automatic_used_invariant; std::uint32_t level_count; std::uint32_t invariant_count;
    std::uint32_t binary_invariant_count; std::uint32_t interior_invariant_count;
    std::uint32_t univariant_count; std::uint32_t contour_path_count;
    bool effective_automatic_range; double effective_minimum; double effective_maximum; double effective_level_step;
    std::uint32_t effective_sampling_subdivisions; bool effective_regularize; double effective_regularization_spacing;
    std::uint32_t effective_source_interpolation; std::uint32_t effective_cubic_method;
    std::uint32_t effective_partial_domain_policy; std::uint32_t effective_continuation;
    std::uint64_t dataset_revision; std::uint64_t options_revision; std::uint64_t request_id;
    bool raw_projection_available; bool regularized_projection_available; bool selected_projection_regularized; std::uint32_t domain_truncated_univariant_count; char message[512];
};
struct TcqtInspectionResult {
    bool success; std::uint32_t state; bool has_value; double value;
    double a; double b; double c; std::uint32_t triangle_index;
    bool has_local_barycentric; double lambda0; double lambda1; double lambda2;
    bool has_contributions; double linear_part; double excess_part;
    bool has_source_rows; std::uint32_t source_row0; std::uint32_t source_row1; std::uint32_t source_row2;
    std::uint32_t local_mode; bool uses_extrapolated_sources; std::uint32_t maximum_extrapolation_layer;
    char extrapolation_methods[128]; std::uint32_t extrapolated_source_row_count; std::uint64_t options_revision; std::uint32_t effective_source_interpolation; std::uint32_t effective_cubic_method; std::uint32_t effective_partial_domain_policy; std::uint32_t effective_continuation; char unit[128]; char message[512];
};
struct TcqtLocatedPoint {
    bool success;
    double a; double b; double c;
    std::uint32_t triangle_index;
    std::uint32_t source_row0; std::uint32_t source_row1; std::uint32_t source_row2;
    double lambda0; double lambda1; double lambda2;
    char message[512];
};
struct TcqtProjectSummary {
    char title[128]; char path[512]; char component_a[128]; char component_b[128]; char component_c[128];
    std::uint32_t phase_count; std::uint32_t property_count; std::uint32_t grid_count;
    bool dirty; std::uint64_t revision; std::uint64_t saved_revision;
    std::uint32_t validity; bool saveable; bool calculation_available; char blocking_reason[512];
};
struct TcqtSaveResult {
    std::uint32_t outcome; char message[512]; char path[512];
};
struct TcqtPasteResult {
    bool success;
    std::uint32_t rows_pasted;
    std::uint32_t columns_pasted;
    std::uint32_t rows_appended;
    bool header_skipped;
    std::uint32_t clipboard_row;
    std::uint32_t clipboard_column;
    std::uint32_t target_row;
    std::uint32_t target_column;
    char message[512];
};
struct TcqtPhase { std::uint32_t id; char name[128]; };
struct TcqtProperty { std::uint32_t ordinal; bool required; char name[128]; char unit[128]; };
struct TcqtGrid { std::uint32_t index; std::uint32_t kind; std::uint32_t subdivisions; std::uint32_t row_count; std::uint32_t field_count; char name[128]; };
struct TcqtField { std::uint32_t index; std::uint32_t phase_id; char property[128]; char column_name[128]; };
struct TcqtRow { double a; double b; double c; };
struct TcqtMeshExtrapolationOptions {
    std::uint32_t grid_index;
    std::uint32_t field_index;
    std::uint32_t phase_id;
    // 0 = field, 1 = phase, 2 = selected canonical target rows.
    std::uint32_t scope;
    bool all_phase_properties;
    const std::uint32_t* target_rows;
    std::uint32_t target_row_count;
    std::uint32_t method;
    std::uint32_t maximum_layers;
    std::uint32_t minimum_directional_support;
    bool has_maximum_directional_spread;
    double maximum_directional_spread;
    bool has_minimum_value;
    double minimum_value;
    bool has_maximum_value;
    double maximum_value;
};struct TcqtMeshExtrapolationSummary {
    bool success; std::uint32_t fields_processed; std::uint32_t values_proposed;
    std::uint32_t values_remaining; std::uint32_t maximum_layer; char message[512];
};
struct TcqtMeshExtrapolationPreviewRow {
    std::uint32_t field_index;
    std::uint32_t phase_id;
    std::uint32_t row_index;
    double a;
    double b;
    double c;
    std::uint32_t old_state;
    // 0 = requested target, 1 = dependency, 2 = field candidate, 3 = rejected.
    std::uint32_t status;
    bool has_value;
    double value;
    std::uint32_t layer;
    std::uint32_t method;
    std::uint32_t support_count;
    double spread;
    char property[128];
    char reason[512];
    char directional_estimates[512];
};
struct TcqtCell { std::uint32_t state; bool has_value; double value; std::uint32_t extrapolation_layer; std::uint32_t extrapolation_method; std::uint32_t extrapolation_support_count; double extrapolation_spread; char note[128]; };
struct TcqtProjectionRecord { double a; double b; double c; std::uint32_t point_index; std::uint32_t line_type; std::uint32_t rgba; double stroke_width; std::uint32_t marker_kind; std::uint32_t path_source; char phase_1[128]; char phase_2[128]; char line_id[128]; };

TcqtStatus tcqt_new_document();
TcqtStatus tcqt_open_document(const char* path);
TcqtSaveResult tcqt_save_document(const char* path);
TcqtPasteResult tcqt_paste_grid_tsv(std::uint32_t grid_index, std::uint32_t start_row, std::uint32_t start_column, const char* clipboard);
TcqtStatus tcqt_project_summary(TcqtProjectSummary* output);
TcqtStatus tcqt_phase_at(std::uint32_t index, TcqtPhase* output);
TcqtStatus tcqt_property_at(std::uint32_t index, TcqtProperty* output);
TcqtStatus tcqt_grid_at(std::uint32_t index, TcqtGrid* output);
TcqtStatus tcqt_grid_field_at(std::uint32_t grid_index, std::uint32_t field_index, TcqtField* output);
TcqtStatus tcqt_grid_row_at(std::uint32_t grid_index, std::uint32_t row_index, TcqtRow* output);
TcqtStatus tcqt_grid_cell_at(std::uint32_t grid_index, std::uint32_t field_index, std::uint32_t row_index, TcqtCell* output);
TcqtStatus tcqt_preview_regular_mesh_extrapolation(const TcqtMeshExtrapolationOptions* options, TcqtMeshExtrapolationSummary* output);
TcqtStatus tcqt_materialize_regular_mesh_extrapolation(TcqtMeshExtrapolationSummary* output);
TcqtStatus tcqt_mesh_extrapolation_preview_row_count(std::uint32_t* output);
TcqtStatus tcqt_mesh_extrapolation_preview_row_at(std::uint32_t index, TcqtMeshExtrapolationPreviewRow* output);
TcqtStatus tcqt_clear_extrapolated_grid_values(std::uint32_t grid_index, std::uint32_t field_index);
TcqtStatus tcqt_clear_extrapolated_phase_values(std::uint32_t grid_index, std::uint32_t phase_id);
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
TcqtStatus tcqt_set_viewer_calculation_options(const TcqtViewerCalculationOptions* options);
TcqtStatus tcqt_set_numerical_trace(std::uint32_t level, const char* destination);
TcqtStatus tcqt_viewer_calculation_options(TcqtViewerCalculationOptions* output);
TcqtStatus tcqt_viewer_calculation_state(TcqtViewerCalculationState* output);
TcqtCalculationResult tcqt_calculate_viewer(const TcqtViewerCalculationOptions* options, std::uint64_t expected_revision, std::uint64_t expected_options_revision, std::uint64_t request_id);
TcqtStatus tcqt_projection_summary(TcqtProjectionSummary* output);
TcqtStatus tcqt_validate_coordinate_triplet(double a, double b, double c);
TcqtStatus tcqt_locate_grid_point(std::uint32_t grid_index, double a, double b, double c, TcqtLocatedPoint* output);
TcqtStatus tcqt_locate_grid_local_point(std::uint32_t grid_index, std::uint32_t triangle_index, double lambda0, double lambda1, double lambda2, TcqtLocatedPoint* output);
TcqtStatus tcqt_evaluate_field_current(std::uint32_t grid_index, std::uint32_t phase_id, const char* property, std::uint64_t expected_options_revision, double a, double b, double c, std::uint64_t query_index, TcqtInspectionResult* output);
TcqtStatus tcqt_set_field_vertex(std::uint32_t grid_index, std::uint32_t phase_id, const char* property, std::uint32_t row_index, const char* token);
TcqtStatus tcqt_bulk_set_field_state(std::uint32_t grid_index, std::uint32_t phase_id, const char* property, const std::uint32_t* rows, std::uint32_t row_count, std::uint32_t state_code);
TcqtStatus tcqt_clear_field_notes(std::uint32_t grid_index, std::uint32_t phase_id, const char* property, const std::uint32_t* rows, std::uint32_t row_count);
TcqtStatus tcqt_projection_record_count(std::uint32_t* output);
TcqtStatus tcqt_projection_record_at(std::uint32_t index, TcqtProjectionRecord* output);
TcqtStatus tcqt_export_plot(const char* path, std::uint32_t format);
TcqtStatus tcqt_export_lines_csv(const char* path);
TcqtCalculationResult tcqt_run_feasibility_calculation(std::uint32_t subdivisions, std::uint64_t dataset_revision);
}