//! Narrow C ABI used only by the Qt feasibility shell.
//!
//! The production application will use the same framework-neutral reducer and
//! request/revision types through a generated CXX boundary. The explicit ABI
//! keeps the feasibility prototype buildable independently from Qt.

use ternary_contours::RegularTernaryGrid;
use ternary_contours_gui_core::{GuiContractState, Revision, UiAction, UiEffect, update};

#[repr(C)]
pub struct TcqtCalculationResult {
    pub success: bool,
    pub request_id: u64,
    pub vertex_count: u32,
    pub message: [u8; 128],
}

fn message(text: &str) -> [u8; 128] {
    let mut bytes = [0; 128];
    let text = text.as_bytes();
    let count = text.len().min(bytes.len() - 1);
    bytes[..count].copy_from_slice(&text[..count]);
    bytes
}

/// Exercises the real regular-grid constructor and the shared reducer.
/// The Qt caller runs this function on a worker thread, never its GUI thread.
#[unsafe(no_mangle)]
pub extern "C" fn tcqt_run_feasibility_calculation(
    subdivisions: u32,
    dataset_revision: u64,
) -> TcqtCalculationResult {
    let Ok(grid) = RegularTernaryGrid::new(subdivisions as usize) else {
        return TcqtCalculationResult {
            success: false,
            request_id: 0,
            vertex_count: 0,
            message: message("subdivisions must be positive"),
        };
    };
    let mut state = GuiContractState::default();
    state.revisions.dataset = Revision(dataset_revision);
    let effects = update(&mut state, UiAction::RecalculateRequested);
    let request_id = effects
        .iter()
        .find_map(|effect| match effect {
            UiEffect::RecalculateProjection { request, .. } => Some(request.0),
            _ => None,
        })
        .unwrap_or_default();
    TcqtCalculationResult {
        success: request_id != 0,
        request_id,
        vertex_count: grid.vertex_count() as u32,
        message: message("Rust grid calculation request accepted"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feasibility_bridge_uses_the_grid_and_reducer() {
        let result = tcqt_run_feasibility_calculation(10, 7);
        assert!(result.success);
        assert_eq!(result.vertex_count, 66);
        assert_ne!(result.request_id, 0);
    }
}
