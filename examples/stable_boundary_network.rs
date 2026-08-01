use ternary_contours::{
    PathRegularizationOptions, PreparedStablePhaseEnsemble, StableBoundaryOptions,
    StableContourQuantity, StableGridOptions, StableInvariantNode, StablePhaseEvaluation,
    StablePhaseId, StablePhaseSource, StablePhaseUndefinedReason, StableScalarSource,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let alpha = |[a, _b, _c]: [f64; 3]| partial_component(a);
    let beta = |[_a, b, _c]: [f64; 3]| partial_component(b);
    let gamma = |[_a, _b, c]: [f64; 3]| partial_component(c);
    let prepared = PreparedStablePhaseEnsemble::new(
        [
            StablePhaseSource::new(StablePhaseId(30), StableScalarSource::evaluator(&gamma)),
            StablePhaseSource::new(StablePhaseId(10), StableScalarSource::evaluator(&alpha)),
            StablePhaseSource::new(StablePhaseId(20), StableScalarSource::evaluator(&beta)),
        ],
        StableContourQuantity::Height,
        StableGridOptions {
            subdivisions: 18,
            ..StableGridOptions::default()
        },
    )?;

    let raw = prepared.stable_boundaries(StableBoundaryOptions::default())?;
    println!(
        "raw: {} nodes, {} boundary-connected univariants",
        raw.nodes.len(),
        raw.univariants.len()
    );
    for trace in &raw.binary_traces {
        println!(
            "{:?}: {} stable regions, {} invariant nodes",
            trace.boundary,
            trace.regions.len(),
            trace.invariants.len()
        );
    }
    for node in &raw.nodes {
        let kind = match node {
            StableInvariantNode::Binary(_) => "binary",
            StableInvariantNode::Interior(_) => "interior",
        };
        println!(
            "node {} ({kind}) {:?}, phases {:?}",
            node.id().0,
            node.point().as_array(),
            node.phases()
        );
    }

    let regularized = prepared.stable_boundaries(StableBoundaryOptions {
        regularization: Some(PathRegularizationOptions {
            spacing: 0.025,
            protected_endpoint_distance: 0.0,
            ..PathRegularizationOptions::default()
        }),
        ..StableBoundaryOptions::default()
    })?;
    for path in &regularized.univariants {
        let diagnostics = path
            .regularization
            .as_ref()
            .expect("regularization was requested");
        println!(
            "path {} {:?}: node {} -> {}, {} -> {} points, residual {:.3e}",
            path.id.0,
            path.phases,
            path.start.0,
            path.end.0,
            diagnostics.raw_point_count,
            diagnostics.final_point_count,
            diagnostics.maximum_pair_residual
        );
        assert_eq!(path.points[0], regularized.nodes[path.start.0].point());
        assert_eq!(
            path.points.last(),
            Some(&regularized.nodes[path.end.0].point())
        );
    }
    Ok(())
}

fn partial_component(component: f64) -> StablePhaseEvaluation {
    if component >= 0.04 {
        StablePhaseEvaluation::Defined { value: component }
    } else {
        StablePhaseEvaluation::Undefined {
            reason: StablePhaseUndefinedReason::OutsidePhaseDomain,
        }
    }
}
