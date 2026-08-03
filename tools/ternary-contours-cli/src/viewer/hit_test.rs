use plotters_ternary::{
    EQUILATERAL_TRIANGLE_HEIGHT, Normalization, TernaryCartesian, TernaryGeometry, TernaryPoint,
    Tolerance,
};

use crate::{LiquidusProjection, RenderOptions, TabulatedTernaryDataset};

use super::state::PathDisplayMode;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkSource {
    Raw,
    Regularized,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectedFeature {
    Invariant {
        node_index: usize,
    },
    Univariant {
        id: usize,
        source: NetworkSource,
    },
    Isotherm {
        level_index: usize,
        path_index: usize,
        nearest_point: usize,
    },
    SourceSample {
        grid_index: usize,
        point_index: usize,
    },
}

#[derive(Clone, Debug)]
struct LogicalPath {
    points: Vec<TernaryCartesian>,
    feature: SelectedFeature,
    closed: bool,
}

#[derive(Clone, Debug)]
struct LogicalNode {
    point: TernaryCartesian,
    feature: SelectedFeature,
}

/// Centralized conversion between composition, logical equilateral space, bitmap, and screen.
#[derive(Clone, Copy, Debug)]
pub struct ViewerTransform {
    bitmap_width: f64,
    bitmap_height: f64,
    image_min: [f64; 2],
    image_size: [f64; 2],
}

impl ViewerTransform {
    pub fn new(
        bitmap_width: u32,
        bitmap_height: u32,
        image_min: [f64; 2],
        image_size: [f64; 2],
    ) -> Self {
        Self {
            bitmap_width: f64::from(bitmap_width),
            bitmap_height: f64::from(bitmap_height),
            image_min,
            image_size,
        }
    }

    pub fn composition_to_logical(&self, composition: [f64; 3]) -> Option<TernaryCartesian> {
        TernaryGeometry::default()
            .project(
                TernaryPoint::from(composition),
                Normalization::RequireUnitSum,
                Tolerance::default(),
            )
            .ok()
    }

    pub fn logical_to_composition(&self, point: TernaryCartesian) -> Option<[f64; 3]> {
        TernaryGeometry::default()
            .unproject(point, Tolerance::default())
            .ok()
            .map(TernaryPoint::as_array)
    }

    pub fn logical_to_screen(&self, point: TernaryCartesian) -> [f64; 2] {
        let bitmap = self.logical_to_bitmap(point);
        [
            self.image_min[0] + bitmap[0] * self.image_size[0] / self.bitmap_width,
            self.image_min[1] + bitmap[1] * self.image_size[1] / self.bitmap_height,
        ]
    }

    pub fn screen_to_logical(&self, screen: [f64; 2]) -> Option<TernaryCartesian> {
        if self.image_size[0] <= 0.0
            || self.image_size[1] <= 0.0
            || self.bitmap_width <= 0.0
            || self.bitmap_height <= 0.0
        {
            return None;
        }
        let bitmap = [
            (screen[0] - self.image_min[0]) * self.bitmap_width / self.image_size[0],
            (screen[1] - self.image_min[1]) * self.bitmap_height / self.image_size[1],
        ];
        self.bitmap_to_logical(bitmap)
    }

    fn logical_to_bitmap(&self, point: TernaryCartesian) -> [f64; 2] {
        let (left, _top, side, base_y) = self.triangle_layout();
        [left + point.x * side, base_y - point.y * side]
    }

    fn bitmap_to_logical(&self, point: [f64; 2]) -> Option<TernaryCartesian> {
        let (left, _top, side, base_y) = self.triangle_layout();
        if side <= 0.0 {
            return None;
        }
        Some(TernaryCartesian::new(
            (point[0] - left) / side,
            (base_y - point[1]) / side,
        ))
    }

    fn triangle_layout(&self) -> (f64, f64, f64, f64) {
        const MARGIN: f64 = 30.0;
        const TITLE_HEIGHT: f64 = 46.0;
        let available_width = (self.bitmap_width - 2.0 * MARGIN).max(1.0);
        let available_height = (self.bitmap_height - TITLE_HEIGHT - 2.0 * MARGIN).max(1.0);
        let side = available_width.min(available_height / EQUILATERAL_TRIANGLE_HEIGHT);
        let left = MARGIN + (available_width - side) * 0.5;
        let top =
            TITLE_HEIGHT + MARGIN + (available_height - side * EQUILATERAL_TRIANGLE_HEIGHT) * 0.5;
        (left, top, side, top + side * EQUILATERAL_TRIANGLE_HEIGHT)
    }
}

#[derive(Clone, Debug, Default)]
pub struct HitGeometry {
    nodes: Vec<LogicalNode>,
    univariants: Vec<LogicalPath>,
    isotherms: Vec<LogicalPath>,
    samples: Vec<LogicalNode>,
}

impl HitGeometry {
    pub fn build(
        dataset: &TabulatedTernaryDataset,
        projection: &LiquidusProjection,
        raw_projection: Option<&LiquidusProjection>,
        options: &RenderOptions,
        path_display: PathDisplayMode,
    ) -> Self {
        let transform = ViewerTransform::new(options.width, options.height, [0.0, 0.0], [1.0, 1.0]);
        let mut geometry = Self::default();
        if options.show_invariants || options.show_binary_invariants {
            for (node_index, node) in projection.stable_boundaries.nodes.iter().enumerate() {
                let visible = match node {
                    ternary_contours::StableInvariantNode::Binary(_) => {
                        options.show_binary_invariants
                    }
                    ternary_contours::StableInvariantNode::Interior(_) => options.show_invariants,
                };
                if visible {
                    if let Some(point) = transform.composition_to_logical(node.point().as_array()) {
                        geometry.nodes.push(LogicalNode {
                            point,
                            feature: SelectedFeature::Invariant { node_index },
                        });
                    }
                }
            }
        }
        if options.show_univariants {
            let raw = raw_projection.unwrap_or(projection);
            match path_display {
                PathDisplayMode::Raw => {
                    geometry.add_univariants(&transform, raw, NetworkSource::Raw)
                }
                PathDisplayMode::Regularized => {
                    geometry.add_univariants(&transform, projection, NetworkSource::Regularized)
                }
                PathDisplayMode::Overlay => {
                    geometry.add_univariants(&transform, raw, NetworkSource::Raw);
                    geometry.add_univariants(&transform, projection, NetworkSource::Regularized);
                }
            }
        }
        if options.show_isotherms {
            for (level_index, level) in projection.stable_contours.levels.iter().enumerate() {
                for (path_index, path) in level.paths.iter().enumerate() {
                    geometry.isotherms.push(LogicalPath {
                        points: path
                            .points
                            .iter()
                            .filter_map(|point| transform.composition_to_logical(point.as_array()))
                            .collect(),
                        feature: SelectedFeature::Isotherm {
                            level_index,
                            path_index,
                            nearest_point: 0,
                        },
                        closed: path.closed,
                    });
                }
            }
        }
        if options.show_grid || options.show_samples {
            for (grid_index, grid) in dataset.grids.iter().enumerate() {
                for (point_index, composition) in grid.compositions().iter().copied().enumerate() {
                    if let Some(point) = transform.composition_to_logical(composition) {
                        geometry.samples.push(LogicalNode {
                            point,
                            feature: SelectedFeature::SourceSample {
                                grid_index,
                                point_index,
                            },
                        });
                    }
                }
            }
        }
        geometry
    }

    fn add_univariants(
        &mut self,
        transform: &ViewerTransform,
        projection: &LiquidusProjection,
        source: NetworkSource,
    ) {
        for path in &projection.stable_boundaries.univariants {
            self.univariants.push(LogicalPath {
                points: path
                    .points
                    .iter()
                    .filter_map(|point| transform.composition_to_logical(point.as_array()))
                    .collect(),
                feature: SelectedFeature::Univariant {
                    id: path.id.0,
                    source,
                },
                closed: false,
            });
        }
    }

    pub fn hit_test(
        &self,
        transform: &ViewerTransform,
        screen: [f64; 2],
        threshold: f64,
    ) -> Option<SelectedFeature> {
        self.closest_node(&self.nodes, transform, screen, threshold)
            .or_else(|| self.closest_path(&self.univariants, transform, screen, threshold))
            .or_else(|| self.closest_path(&self.isotherms, transform, screen, threshold))
            .or_else(|| self.closest_node(&self.samples, transform, screen, threshold))
    }

    pub fn selected_anchor(&self, selection: &SelectedFeature) -> Option<TernaryCartesian> {
        self.nodes
            .iter()
            .chain(&self.samples)
            .find(|node| node.feature == *selection)
            .map(|node| node.point)
            .or_else(|| {
                self.univariants
                    .iter()
                    .chain(&self.isotherms)
                    .find(|path| path.feature == *selection)
                    .and_then(|path| path.points.first().copied())
            })
    }

    pub fn paths(&self) -> impl Iterator<Item = (&[TernaryCartesian], &SelectedFeature, bool)> {
        self.univariants
            .iter()
            .chain(&self.isotherms)
            .map(|path| (path.points.as_slice(), &path.feature, path.closed))
    }

    pub fn nodes(&self) -> impl Iterator<Item = (TernaryCartesian, &SelectedFeature)> {
        self.nodes
            .iter()
            .chain(&self.samples)
            .map(|node| (node.point, &node.feature))
    }

    fn closest_node(
        &self,
        nodes: &[LogicalNode],
        transform: &ViewerTransform,
        screen: [f64; 2],
        threshold: f64,
    ) -> Option<SelectedFeature> {
        nodes
            .iter()
            .filter_map(|node| {
                let distance = distance(screen, transform.logical_to_screen(node.point));
                (distance <= threshold).then_some((distance, node.feature.clone()))
            })
            .min_by(|left, right| left.0.total_cmp(&right.0))
            .map(|(_, feature)| feature)
    }

    fn closest_path(
        &self,
        paths: &[LogicalPath],
        transform: &ViewerTransform,
        screen: [f64; 2],
        threshold: f64,
    ) -> Option<SelectedFeature> {
        paths
            .iter()
            .filter_map(|path| {
                let (distance, nearest_point) = path_distance(path, transform, screen)?;
                (distance <= threshold).then(|| {
                    let feature = match path.feature {
                        SelectedFeature::Isotherm {
                            level_index,
                            path_index,
                            ..
                        } => SelectedFeature::Isotherm {
                            level_index,
                            path_index,
                            nearest_point,
                        },
                        _ => path.feature.clone(),
                    };
                    (distance, feature)
                })
            })
            .min_by(|left, right| left.0.total_cmp(&right.0))
            .map(|(_, feature)| feature)
    }
}

fn path_distance(
    path: &LogicalPath,
    transform: &ViewerTransform,
    screen: [f64; 2],
) -> Option<(f64, usize)> {
    if path.points.len() == 1 {
        return Some((
            distance(screen, transform.logical_to_screen(path.points[0])),
            0,
        ));
    }
    let mut best: Option<(f64, usize)> = None;
    for (index, pair) in path.points.windows(2).enumerate() {
        let value = point_to_segment_distance(
            screen,
            transform.logical_to_screen(pair[0]),
            transform.logical_to_screen(pair[1]),
        );
        if best.is_none_or(|current| value < current.0) {
            best = Some((value, index));
        }
    }
    if path.closed && path.points.len() > 2 {
        let last = path.points.len() - 1;
        let value = point_to_segment_distance(
            screen,
            transform.logical_to_screen(path.points[last]),
            transform.logical_to_screen(path.points[0]),
        );
        if best.is_none_or(|current| value < current.0) {
            best = Some((value, last));
        }
    }
    best
}

fn distance(left: [f64; 2], right: [f64; 2]) -> f64 {
    let dx = left[0] - right[0];
    let dy = left[1] - right[1];
    dx.hypot(dy)
}

/// Euclidean distance from a point to a finite segment in screen space.
pub fn point_to_segment_distance(point: [f64; 2], start: [f64; 2], end: [f64; 2]) -> f64 {
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let length_squared = dx * dx + dy * dy;
    if length_squared <= f64::EPSILON {
        return distance(point, start);
    }
    let fraction = (((point[0] - start[0]) * dx + (point[1] - start[1]) * dy) / length_squared)
        .clamp(0.0, 1.0);
    distance(point, [start[0] + fraction * dx, start[1] + fraction * dy])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ternary_logical_screen_round_trip_is_stable() {
        let transform = ViewerTransform::new(1_200, 950, [40.0, 20.0], [720.0, 570.0]);
        let composition = [0.2, 0.3, 0.5];
        let logical = transform.composition_to_logical(composition).unwrap();
        let recovered = transform.logical_to_composition(logical).unwrap();
        for (actual, expected) in recovered.into_iter().zip(composition) {
            assert!((actual - expected).abs() < 1.0e-12);
        }
        let screen = transform.logical_to_screen(logical);
        let round_trip = transform.screen_to_logical(screen).unwrap();
        assert!((round_trip.x - logical.x).abs() < 1.0e-12);
        assert!((round_trip.y - logical.y).abs() < 1.0e-12);
    }

    #[test]
    fn point_to_segment_distance_handles_projection_and_endpoints() {
        assert!(
            (point_to_segment_distance([0.5, 1.0], [0.0, 0.0], [1.0, 0.0]) - 1.0).abs() < 1.0e-12
        );
        assert!(
            (point_to_segment_distance([-1.0, 0.0], [0.0, 0.0], [1.0, 0.0]) - 1.0).abs() < 1.0e-12
        );
    }

    #[test]
    fn invariants_take_priority_over_paths_and_samples() {
        let point = TernaryCartesian::new(0.5, 0.4);
        let geometry = HitGeometry {
            nodes: vec![LogicalNode {
                point,
                feature: SelectedFeature::Invariant { node_index: 1 },
            }],
            univariants: vec![LogicalPath {
                points: vec![
                    TernaryCartesian::new(0.0, 0.4),
                    TernaryCartesian::new(1.0, 0.4),
                ],
                feature: SelectedFeature::Univariant {
                    id: 2,
                    source: NetworkSource::Raw,
                },
                closed: false,
            }],
            isotherms: Vec::new(),
            samples: vec![LogicalNode {
                point,
                feature: SelectedFeature::SourceSample {
                    grid_index: 0,
                    point_index: 0,
                },
            }],
        };
        let transform = ViewerTransform::new(1_200, 950, [0.0, 0.0], [1_200.0, 950.0]);
        let screen = transform.logical_to_screen(point);
        assert_eq!(
            geometry.hit_test(&transform, screen, 12.0),
            Some(SelectedFeature::Invariant { node_index: 1 })
        );
    }
}
