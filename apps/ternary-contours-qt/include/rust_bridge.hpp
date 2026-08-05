#pragma once

#include <cstdint>

extern "C" {
struct TcqtCalculationResult {
    bool success;
    std::uint64_t request_id;
    std::uint32_t vertex_count;
    char message[128];
};

TcqtCalculationResult tcqt_run_feasibility_calculation(
    std::uint32_t subdivisions,
    std::uint64_t dataset_revision);
}