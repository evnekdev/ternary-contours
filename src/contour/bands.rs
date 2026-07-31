use std::collections::{BTreeMap, BTreeSet};

use crate::{RegularTernaryScalarField, TernaryCoordinate};

use super::{ContourError, ContourInterpolation};

/// Backend-independent options for piecewise-linear filled contour bands.
///
/// Geometric rings use the same tolerance model as ContourOptions. Bands are
/// numerically classified as lower-inclusive and upper-exclusive; unbounded
/// extreme bands own their respective infinities. Polygon boundaries remain
/// closed geometrically, so adjacent bands may share a boundary without
/// overlapping in positive area.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContourBandOptions {
    /// Interpolation used to construct the regions. Only linear interpolation
    /// is currently supported.
    pub interpolation: ContourInterpolation,
    /// Finite positive scalar equality tolerance.
    pub value_tolerance: f64,
    /// Finite positive composition-space cleanup tolerance.
    pub geometry_tolerance: f64,
    /// Include the unbounded band below the first break.
    pub include_lower_extreme: bool,
    /// Include the unbounded band above the last break.
    pub include_upper_extreme: bool,
}

impl ContourBandOptions {
    /// Construct options for the always-available piecewise-linear band model.
    pub const fn linear() -> Self {
        Self {
            interpolation: ContourInterpolation::Linear,
            value_tolerance: 1.0e-10,
            geometry_tolerance: 1.0e-8,
            include_lower_extreme: true,
            include_upper_extreme: true,
        }
    }

    fn validate(self) -> Result<(), ContourError> {
        if !self.value_tolerance.is_finite()
            || self.value_tolerance <= 0.0
            || !self.geometry_tolerance.is_finite()
            || self.geometry_tolerance <= 0.0
        {
            return Err(ContourError::InvalidTolerance {
                value_tolerance: self.value_tolerance,
                geometry_tolerance: self.geometry_tolerance,
            });
        }
        if !matches!(self.interpolation, ContourInterpolation::Linear) {
            return Err(ContourError::UnsupportedFilledInterpolation);
        }
        Ok(())
    }
}

impl Default for ContourBandOptions {
    fn default() -> Self {
        Self::linear()
    }
}

/// One scalar interval of a ContourBandSet.
///
/// None is an unbounded end. The semantic ownership rule is lower-inclusive
/// and upper-exclusive, with the unbounded upper band retaining its maximum
/// boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct ContourBand {
    /// Inclusive lower scalar bound, or no lower bound.
    pub lower: Option<f64>,
    /// Exclusive upper scalar bound, or no upper bound.
    pub upper: Option<f64>,
    /// Deterministically ordered disconnected filled regions.
    pub regions: Vec<ContourRegion>,
    fragments: Vec<ContourFragment>,
}

impl ContourBand {
    /// Return deterministic, non-overlapping simple fragments for this band.
    ///
    /// This representation is useful to render a band without painting its
    /// holes: draw the fragments and leave all complementary geometry
    /// untouched. It has the same positive-area coverage as Self::regions.
    pub fn fragments(&self) -> &[ContourFragment] {
        &self.fragments
    }
}

/// One simple, open polygon fragment of a contour band.
///
/// Fragments are in canonical ternary composition coordinates. They have no
/// holes and do not overlap each other in positive area.
#[derive(Clone, Debug, PartialEq)]
pub struct ContourFragment {
    vertices: Vec<TernaryCoordinate>,
}

impl ContourFragment {
    /// Return the open polygon ring. The final edge is implicit.
    pub fn vertices(&self) -> &[TernaryCoordinate] {
        &self.vertices
    }
}

/// One connected filled-band region.
///
/// Rings are open: the final edge back to the first point is implicit.
/// Exterior rings are counter-clockwise in semantic (a, b) coordinates;
/// holes are clockwise.
#[derive(Clone, Debug, PartialEq)]
pub struct ContourRegion {
    /// Counter-clockwise exterior ring in semantic A/B/C coordinates.
    pub exterior: Vec<TernaryCoordinate>,
    /// Clockwise interior hole rings.
    pub holes: Vec<Vec<TernaryCoordinate>>,
}

impl ContourRegion {
    /// Signed semantic area of this region, excluding holes.
    pub fn area(&self) -> f64 {
        ring_area(&self.exterior).abs()
            - self
                .holes
                .iter()
                .map(|ring| ring_area(ring).abs())
                .sum::<f64>()
    }
}

/// Complete deterministic piecewise-linear isoband geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct ContourBandSet {
    /// Bands in ascending scalar order.
    pub bands: Vec<ContourBand>,
}

impl ContourBandSet {
    /// Compute finite, ordered piecewise-linear scalar bands.
    ///
    /// Callers must supply finite, strictly increasing breaks. Semantic scalar
    /// ownership is f < l0, li <= f < li+1, and f >= lm; closed polygon
    /// boundaries may still be shared by adjacent bands without a positive-area
    /// overlap. The field is clipped in composition space before rendering.
    pub fn compute(
        field: &RegularTernaryScalarField,
        breaks: &[f64],
        options: ContourBandOptions,
    ) -> Result<Self, ContourError> {
        options.validate()?;
        let breaks = validated_breaks(breaks, options.value_tolerance)?;
        let ranges = band_ranges(&breaks, options);
        let triangles = field.elementary_triangles()?;
        let mut bands = Vec::with_capacity(ranges.len());
        for (lower, upper) in ranges {
            let mut fragments = Vec::new();
            for triangle in &triangles {
                let mut polygon = triangle
                    .vertices
                    .map(|vertex| {
                        Ok(BandVertex {
                            coordinate: field.composition(vertex)?.into(),
                            value: field.value(vertex)?,
                        })
                    })
                    .into_iter()
                    .collect::<Result<Vec<_>, crate::FieldError>>()?;
                if let Some(value) = lower {
                    polygon = clip_lower(&polygon, value, options.value_tolerance);
                }
                if let Some(value) = upper {
                    polygon = clip_upper(&polygon, value, options.value_tolerance);
                }
                cleanup_ring(&mut polygon, options.geometry_tolerance);
                if polygon.len() >= 3
                    && ring_area_vertices(&polygon).abs() > options.geometry_tolerance
                {
                    fragments.push(
                        polygon
                            .into_iter()
                            .map(|vertex| vertex.coordinate)
                            .collect(),
                    );
                }
            }
            let fragments = fragments
                .into_iter()
                .map(|vertices| ContourFragment { vertices })
                .collect::<Vec<_>>();
            bands.push(ContourBand {
                lower,
                upper,
                regions: assemble_regions(
                    fragments
                        .iter()
                        .map(|fragment| fragment.vertices.clone())
                        .collect(),
                    options.geometry_tolerance,
                )?,
                fragments,
            });
        }
        Ok(Self { bands })
    }

    /// Return the semantically owning band for a finite scalar value.
    ///
    /// Unlike geometric clipping this performs no tolerance expansion: a value
    /// exactly equal to a break belongs to the band starting at that break.
    pub fn band_index_for(&self, value: f64) -> Option<usize> {
        if !value.is_finite() {
            return None;
        }
        self.bands.iter().position(|band| {
            band.lower.is_none_or(|lower| value >= lower)
                && band.upper.is_none_or(|upper| value < upper)
        })
    }
}

#[derive(Clone, Copy)]
struct BandVertex {
    coordinate: TernaryCoordinate,
    value: f64,
}

fn validated_breaks(breaks: &[f64], tolerance: f64) -> Result<Vec<f64>, ContourError> {
    for (index, value) in breaks.iter().copied().enumerate() {
        if !value.is_finite() {
            return Err(ContourError::NonFiniteBandBreak { index, value });
        }
    }
    for (index, pair) in breaks.windows(2).enumerate() {
        let previous = pair[0];
        let value = pair[1];
        if value <= previous {
            if (previous - value).abs() <= tolerance {
                return Err(ContourError::DuplicateBandBreak {
                    first: index,
                    second: index + 1,
                    value,
                });
            }
            return Err(ContourError::UnorderedBandBreak {
                previous_index: index,
                index: index + 1,
                previous,
                value,
            });
        }
        if value - previous <= tolerance {
            return Err(ContourError::DuplicateBandBreak {
                first: index,
                second: index + 1,
                value,
            });
        }
    }
    Ok(breaks.to_vec())
}

fn band_ranges(breaks: &[f64], options: ContourBandOptions) -> Vec<(Option<f64>, Option<f64>)> {
    if breaks.is_empty() {
        return vec![(None, None)];
    }
    let mut ranges = Vec::with_capacity(breaks.len() + 1);
    if options.include_lower_extreme {
        ranges.push((None, Some(breaks[0])));
    }
    ranges.extend(breaks.windows(2).map(|pair| (Some(pair[0]), Some(pair[1]))));
    if options.include_upper_extreme {
        ranges.push((Some(*breaks.last().expect("nonempty breaks")), None));
    }
    ranges
}

fn clip_lower(input: &[BandVertex], threshold: f64, tolerance: f64) -> Vec<BandVertex> {
    // Closed geometry is intentional: neighbouring bands share only the
    // threshold curve. Semantic half-open ownership is exposed separately.
    clip(input, threshold, tolerance, |delta| delta >= 0.0)
}

fn clip_upper(input: &[BandVertex], threshold: f64, tolerance: f64) -> Vec<BandVertex> {
    clip(input, threshold, tolerance, |delta| delta <= 0.0)
}

fn clip(
    input: &[BandVertex],
    threshold: f64,
    tolerance: f64,
    is_inside: impl Fn(f64) -> bool,
) -> Vec<BandVertex> {
    let mut output = Vec::new();
    let Some(mut previous) = input.last().copied() else {
        return output;
    };
    let mut previous_inside = is_inside(previous.value - threshold);
    for current in input.iter().copied() {
        let current_inside = is_inside(current.value - threshold);
        if current_inside != previous_inside {
            push_vertex(
                &mut output,
                interpolate(previous, current, threshold, tolerance),
                tolerance,
            );
        }
        if current_inside {
            push_vertex(&mut output, current, tolerance);
        }
        previous = current;
        previous_inside = current_inside;
    }
    output
}

fn interpolate(start: BandVertex, end: BandVertex, threshold: f64, tolerance: f64) -> BandVertex {
    let denominator = end.value - start.value;
    let mut t = if denominator.abs() <= tolerance {
        0.0
    } else {
        (threshold - start.value) / denominator
    };
    if t.abs() <= tolerance {
        t = 0.0;
    } else if (1.0 - t).abs() <= tolerance {
        t = 1.0;
    }
    let left = start.coordinate.as_array();
    let right = end.coordinate.as_array();
    BandVertex {
        coordinate: TernaryCoordinate::new(
            left[0] + (right[0] - left[0]) * t,
            left[1] + (right[1] - left[1]) * t,
            left[2] + (right[2] - left[2]) * t,
        ),
        value: threshold,
    }
}

fn cleanup_ring(vertices: &mut Vec<BandVertex>, tolerance: f64) {
    let mut clean = Vec::with_capacity(vertices.len());
    for vertex in vertices.drain(..) {
        push_vertex(&mut clean, vertex, tolerance);
    }
    if clean.len() > 1
        && points_close(
            clean[0].coordinate,
            clean[clean.len() - 1].coordinate,
            tolerance,
        )
    {
        clean.pop();
    }
    *vertices = clean;
}

fn push_vertex(output: &mut Vec<BandVertex>, vertex: BandVertex, tolerance: f64) {
    if output
        .last()
        .is_none_or(|previous| !points_close(previous.coordinate, vertex.coordinate, tolerance))
    {
        output.push(vertex);
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct NodeKey(i64, i64);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct DirectedEdgeKey(usize, usize);

struct NodeCanonicalizer {
    tolerance: f64,
    buckets: BTreeMap<NodeKey, Vec<usize>>,
    points: Vec<TernaryCoordinate>,
}

impl NodeCanonicalizer {
    fn new(tolerance: f64) -> Self {
        Self {
            // A tolerance below machine-scale cannot identify distinct computed
            // intersections reliably. This guard only affects node joining.
            tolerance: tolerance.max(64.0 * f64::EPSILON),
            buckets: BTreeMap::new(),
            points: Vec::new(),
        }
    }

    fn node_for(&mut self, point: TernaryCoordinate) -> usize {
        let key = self.key(point);
        let mut existing: Option<usize> = None;
        for da in -1..=1 {
            for db in -1..=1 {
                let neighbour = NodeKey(key.0.saturating_add(da), key.1.saturating_add(db));
                if let Some(candidates) = self.buckets.get(&neighbour) {
                    for &candidate in candidates {
                        if points_close(self.points[candidate], point, self.tolerance) {
                            existing = Some(existing.map_or(candidate, |old| old.min(candidate)));
                        }
                    }
                }
            }
        }
        if let Some(node) = existing {
            return node;
        }
        let node = self.points.len();
        self.points.push(point);
        self.buckets.entry(key).or_default().push(node);
        node
    }

    fn key(&self, point: TernaryCoordinate) -> NodeKey {
        let [a, b, _] = point.as_array();
        NodeKey(bucket(a, self.tolerance), bucket(b, self.tolerance))
    }
}

fn bucket(value: f64, tolerance: f64) -> i64 {
    let scaled = (value / tolerance).floor();
    if scaled <= i64::MIN as f64 {
        i64::MIN
    } else if scaled >= i64::MAX as f64 {
        i64::MAX
    } else {
        scaled as i64
    }
}

fn assemble_regions(
    fragments: Vec<Vec<TernaryCoordinate>>,
    tolerance: f64,
) -> Result<Vec<ContourRegion>, ContourError> {
    let mut nodes = NodeCanonicalizer::new(tolerance);
    let mut edges = BTreeSet::<DirectedEdgeKey>::new();
    for fragment in fragments {
        for (start, end) in fragment
            .iter()
            .copied()
            .zip(fragment.iter().copied().cycle().skip(1))
            .take(fragment.len())
        {
            let start = nodes.node_for(start);
            let end = nodes.node_for(end);
            if start == end {
                continue;
            }
            let key = DirectedEdgeKey(start, end);
            let reverse = DirectedEdgeKey(end, start);
            if !edges.remove(&reverse) {
                edges.insert(key);
            }
        }
    }

    let mut outgoing = BTreeMap::<usize, BTreeSet<DirectedEdgeKey>>::new();
    for key in edges.iter().copied() {
        outgoing.entry(key.0).or_default().insert(key);
    }
    let mut unused = edges.clone();
    let mut rings = Vec::new();
    while let Some(start) = unused.iter().next().copied() {
        let mut ring = Vec::new();
        let mut current = start;
        let mut guard = 0usize;
        loop {
            guard += 1;
            if guard > edges.len().saturating_add(1) || !unused.remove(&current) {
                return Err(ContourError::UnclosedBandBoundary);
            }
            let from = nodes.points[current.0];
            let to = nodes.points[current.1];
            if ring.is_empty() {
                ring.push(from);
            }
            ring.push(to);
            if current.1 == start.0 {
                break;
            }
            let candidates = outgoing
                .get(&current.1)
                .into_iter()
                .flatten()
                .copied()
                .filter(|key| unused.contains(key))
                .collect::<Vec<_>>();
            let Some(next) = select_next(current, &candidates, &nodes.points) else {
                return Err(ContourError::UnclosedBandBoundary);
            };
            current = next;
        }
        ring.pop();
        if ring.len() >= 3 && ring_area(&ring).abs() > tolerance {
            rings.push(ring);
        }
    }
    Ok(build_regions(rings))
}

fn select_next(
    incoming: DirectedEdgeKey,
    candidates: &[DirectedEdgeKey],
    nodes: &[TernaryCoordinate],
) -> Option<DirectedEdgeKey> {
    let from = nodes[incoming.0].as_array();
    let middle = nodes[incoming.1].as_array();
    candidates.iter().copied().min_by(|left, right| {
        let l = turn_angle(from, middle, nodes[left.1].as_array());
        let r = turn_angle(from, middle, nodes[right.1].as_array());
        l.total_cmp(&r).then_with(|| left.cmp(right))
    })
}

fn turn_angle(from: [f64; 3], middle: [f64; 3], to: [f64; 3]) -> f64 {
    let incoming = (middle[0] - from[0], middle[1] - from[1]);
    let outgoing = (to[0] - middle[0], to[1] - middle[1]);
    let angle = (incoming.0 * outgoing.1 - incoming.1 * outgoing.0)
        .atan2(incoming.0 * outgoing.0 + incoming.1 * outgoing.1);
    if angle <= 0.0 {
        angle + std::f64::consts::TAU
    } else {
        angle
    }
}

fn build_regions(mut rings: Vec<Vec<TernaryCoordinate>>) -> Vec<ContourRegion> {
    rings.sort_by(|left, right| compare_ring_keys(ring_sort_key(left), ring_sort_key(right)));
    let samples = rings
        .iter()
        .map(|ring| interior_sample(ring))
        .collect::<Vec<_>>();
    let areas = rings
        .iter()
        .map(|ring| ring_area(ring).abs())
        .collect::<Vec<_>>();
    let parents = (0..rings.len())
        .map(|child| {
            (0..rings.len())
                .filter(|&candidate| candidate != child && areas[candidate] > areas[child])
                .filter(|&candidate| point_in_ring(samples[child], &rings[candidate]))
                .min_by(|&left, &right| areas[left].total_cmp(&areas[right]))
        })
        .collect::<Vec<_>>();
    let depths = (0..rings.len())
        .map(|index| {
            let mut depth = 0usize;
            let mut parent = parents[index];
            while let Some(next) = parent {
                depth += 1;
                parent = parents[next];
            }
            depth
        })
        .collect::<Vec<_>>();

    let mut exterior_result = BTreeMap::<usize, usize>::new();
    let mut regions = Vec::new();
    for index in 0..rings.len() {
        if depths[index] % 2 == 0 {
            let mut exterior = rings[index].clone();
            if ring_area(&exterior) < 0.0 {
                exterior.reverse();
            }
            exterior_result.insert(index, regions.len());
            regions.push(ContourRegion {
                exterior,
                holes: Vec::new(),
            });
        }
    }
    for index in 0..rings.len() {
        if depths[index] % 2 == 1 {
            let mut hole = rings[index].clone();
            if ring_area(&hole) > 0.0 {
                hole.reverse();
            }
            let mut owner = parents[index];
            while let Some(parent) = owner {
                if depths[parent] % 2 == 0 {
                    if let Some(&region) = exterior_result.get(&parent) {
                        regions[region].holes.push(hole);
                    }
                    break;
                }
                owner = parents[parent];
            }
        }
    }
    for region in &mut regions {
        region
            .holes
            .sort_by(|left, right| compare_ring_keys(ring_sort_key(left), ring_sort_key(right)));
    }
    regions.sort_by(|left, right| {
        compare_ring_keys(
            ring_sort_key(&left.exterior),
            ring_sort_key(&right.exterior),
        )
    });
    regions
}

fn interior_sample(ring: &[TernaryCoordinate]) -> TernaryCoordinate {
    let signed_area = ring_area(ring);
    if signed_area.abs() > f64::EPSILON {
        let (a, b) = ring
            .iter()
            .zip(ring.iter().cycle().skip(1))
            .take(ring.len())
            .fold((0.0, 0.0), |(a_sum, b_sum), (left, right)| {
                let [x0, y0, _] = left.as_array();
                let [x1, y1, _] = right.as_array();
                let cross = x0 * y1 - x1 * y0;
                (a_sum + (x0 + x1) * cross, b_sum + (y0 + y1) * cross)
            });
        let sample = TernaryCoordinate::new(a / (6.0 * signed_area), b / (6.0 * signed_area), 0.0);
        if point_in_ring(sample, ring) {
            return sample;
        }
    }
    let origin = ring[0].as_array();
    for pair in ring[1..].windows(2) {
        let left = pair[0].as_array();
        let right = pair[1].as_array();
        let sample = TernaryCoordinate::new(
            (origin[0] + left[0] + right[0]) / 3.0,
            (origin[1] + left[1] + right[1]) / 3.0,
            0.0,
        );
        if point_in_ring(sample, ring) {
            return sample;
        }
    }
    // Only reached for malformed rings, which are filtered before assembly.
    ring[0]
}

fn compare_ring_keys(left: (f64, f64, usize), right: (f64, f64, usize)) -> std::cmp::Ordering {
    left.0
        .total_cmp(&right.0)
        .then_with(|| left.1.total_cmp(&right.1))
        .then_with(|| left.2.cmp(&right.2))
}

fn ring_sort_key(ring: &[TernaryCoordinate]) -> (f64, f64, usize) {
    let point = ring
        .iter()
        .map(|point| point.as_array())
        .min_by(|left, right| {
            left[0]
                .total_cmp(&right[0])
                .then_with(|| left[1].total_cmp(&right[1]))
        })
        .expect("nonempty ring");
    (point[0], point[1], ring.len())
}

fn point_in_ring(point: TernaryCoordinate, ring: &[TernaryCoordinate]) -> bool {
    let [x, y, _] = point.as_array();
    ring.iter()
        .zip(ring.iter().cycle().skip(1))
        .take(ring.len())
        .fold(false, |inside, (left, right)| {
            let [x0, y0, _] = left.as_array();
            let [x1, y1, _] = right.as_array();
            if (y0 > y) != (y1 > y) && x < (x1 - x0) * (y - y0) / (y1 - y0) + x0 {
                !inside
            } else {
                inside
            }
        })
}

fn ring_area_vertices(vertices: &[BandVertex]) -> f64 {
    let coordinates = vertices
        .iter()
        .map(|vertex| vertex.coordinate)
        .collect::<Vec<_>>();
    ring_area(&coordinates)
}

fn ring_area(vertices: &[TernaryCoordinate]) -> f64 {
    vertices
        .iter()
        .zip(vertices.iter().cycle().skip(1))
        .take(vertices.len())
        .map(|(left, right)| {
            let left = left.as_array();
            let right = right.as_array();
            left[0] * right[1] - left[1] * right[0]
        })
        .sum::<f64>()
        * 0.5
}

fn points_close(left: TernaryCoordinate, right: TernaryCoordinate, tolerance: f64) -> bool {
    left.as_array()
        .into_iter()
        .zip(right.as_array())
        .all(|(a, b)| (a - b).abs() <= tolerance)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(n: usize, value: impl Fn(f64, f64, f64) -> f64) -> RegularTernaryScalarField {
        RegularTernaryScalarField::from_fn(n, |[a, b, c]| value(a, b, c)).unwrap()
    }
    fn area(bands: &ContourBandSet) -> f64 {
        bands
            .bands
            .iter()
            .flat_map(|band| &band.regions)
            .map(ContourRegion::area)
            .sum()
    }

    #[test]
    fn one_triangle_cut_by_threshold_conserves_simplex_area() {
        let field = field(1, |a, _, _| a);
        let bands = ContourBandSet::compute(&field, &[0.5], ContourBandOptions::linear()).unwrap();
        assert_eq!(bands.bands.len(), 2);
        assert!((area(&bands) - 0.5).abs() < 1e-10);
        assert!(bands.bands.iter().all(|band| !band.regions.is_empty()));
    }

    #[test]
    fn two_thresholds_and_extremes_are_ordered() {
        let field = field(1, |a, _, _| a);
        let bands =
            ContourBandSet::compute(&field, &[0.25, 0.75], ContourBandOptions::linear()).unwrap();
        assert_eq!(
            bands
                .bands
                .iter()
                .map(|band| (band.lower, band.upper))
                .collect::<Vec<_>>(),
            vec![
                (None, Some(0.25)),
                (Some(0.25), Some(0.75)),
                (Some(0.75), None)
            ]
        );
        assert!((area(&bands) - 0.5).abs() < 1e-10);
    }

    #[test]
    fn exact_vertex_and_edge_breaks_are_deterministic() {
        let field = field(2, |a, _, _| a);
        let first = ContourBandSet::compute(&field, &[0.5], ContourBandOptions::linear()).unwrap();
        let second = ContourBandSet::compute(&field, &[0.5], ContourBandOptions::linear()).unwrap();
        assert_eq!(first, second);
        assert!((area(&first) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn outer_band_can_retain_an_interior_hole() {
        let field = field(18, |a, b, c| {
            (a - 1.0 / 3.0).powi(2) + (b - 1.0 / 3.0).powi(2) + (c - 1.0 / 3.0).powi(2)
        });
        let bands =
            ContourBandSet::compute(&field, &[0.025, 0.10], ContourBandOptions::linear()).unwrap();
        assert!(
            bands.bands[2]
                .regions
                .iter()
                .any(|region| !region.holes.is_empty())
        );
        assert!((area(&bands) - 0.5).abs() < 1.0e-7);
    }
    #[test]
    fn invalid_breaks_and_cubic_mode_are_explicit() {
        let field = field(1, |a, _, _| a);
        assert!(matches!(
            ContourBandSet::compute(&field, &[f64::NAN], ContourBandOptions::linear()),
            Err(ContourError::NonFiniteBandBreak { .. })
        ));
        assert!(matches!(
            ContourBandSet::compute(&field, &[0.5, 0.5], ContourBandOptions::linear()),
            Err(ContourError::DuplicateBandBreak { .. })
        ));
        let mut options = ContourBandOptions::linear();
        options.interpolation = ContourInterpolation::CubicAlpha(Default::default());
        assert!(matches!(
            ContourBandSet::compute(&field, &[0.5], options),
            Err(ContourError::UnsupportedFilledInterpolation)
        ));
    }

    #[test]
    fn all_bands_cover_disconnected_linear_regions_without_zero_area_rings() {
        let field = field(8, |a, b, c| {
            (a - 0.2).powi(2) + (b - 0.25).powi(2) + (c - 0.55).powi(2)
        });
        let bands =
            ContourBandSet::compute(&field, &[0.04, 0.12], ContourBandOptions::linear()).unwrap();
        assert!((area(&bands) - 0.5).abs() < 1e-7);
        assert!(
            bands
                .bands
                .iter()
                .flat_map(|band| &band.regions)
                .all(|region| region.area() > 1e-10)
        );
    }
    #[test]
    fn breaks_are_strictly_increasing_and_scalar_ownership_is_half_open() {
        let field = field(1, |a, _, _| a);
        let options = ContourBandOptions::linear();
        let bands = ContourBandSet::compute(&field, &[0.25, 0.75], options).unwrap();
        assert_eq!(bands.band_index_for(0.249_999_999), Some(0));
        assert_eq!(bands.band_index_for(0.25), Some(1));
        assert_eq!(bands.band_index_for(0.75), Some(2));
        assert_eq!(bands.band_index_for(f64::NAN), None);
        assert!(matches!(
            ContourBandSet::compute(&field, &[0.75, 0.25], options),
            Err(ContourError::UnorderedBandBreak { .. })
        ));
        assert!(matches!(
            ContourBandSet::compute(&field, &[0.25, 0.25 + 1.0e-12], options),
            Err(ContourError::DuplicateBandBreak { .. })
        ));
        assert!(ContourBandSet::compute(&field, &[], options).is_ok());
    }

    #[test]
    fn containment_assigns_holes_and_nested_islands_without_orientation_assumptions() {
        let coordinate = |a, b| TernaryCoordinate::new(a, b, 1.0 - a - b);
        let outer = vec![
            coordinate(0.05, 0.05),
            coordinate(0.85, 0.05),
            coordinate(0.85, 0.85),
            coordinate(0.05, 0.85),
        ];
        let hole = vec![
            coordinate(0.25, 0.25),
            coordinate(0.65, 0.25),
            coordinate(0.65, 0.65),
            coordinate(0.25, 0.65),
        ];
        let island = vec![
            coordinate(0.35, 0.35),
            coordinate(0.55, 0.35),
            coordinate(0.55, 0.55),
            coordinate(0.35, 0.55),
        ];
        let regions = build_regions(vec![outer, hole, island]);
        assert_eq!(regions.len(), 2);
        assert_eq!(
            regions
                .iter()
                .filter(|region| !region.holes.is_empty())
                .count(),
            1
        );
        assert!(
            regions
                .iter()
                .all(|region| ring_area(&region.exterior) > 0.0)
        );
        assert!(
            regions
                .iter()
                .flat_map(|region| &region.holes)
                .all(|ring| ring_area(ring) < 0.0)
        );
    }

    #[test]
    fn neighbour_cell_canonicalisation_cancels_shared_fragment_edges_once() {
        let coordinate = |a, b| TernaryCoordinate::new(a, b, 1.0 - a - b);
        let epsilon = 0.25e-8;
        let regions = assemble_regions(
            vec![
                vec![
                    coordinate(0.0, 0.0),
                    coordinate(0.5, 0.0),
                    coordinate(0.5, 0.5),
                ],
                vec![
                    coordinate(0.5 + epsilon, 0.0),
                    coordinate(1.0, 0.0),
                    coordinate(0.5 + epsilon, 0.5),
                ],
            ],
            1.0e-8,
        )
        .unwrap();
        assert_eq!(regions.len(), 1);
        assert!(regions[0].area() > 0.2);
    }

    #[test]
    fn assembled_regions_and_fragments_have_matching_coverage() {
        let field = field(9, |a, b, c| 0.4 * a - 0.3 * b + 0.8 * c);
        let bands =
            ContourBandSet::compute(&field, &[0.05, 0.25, 0.45], ContourBandOptions::linear())
                .unwrap();
        let fragment_area = bands
            .bands
            .iter()
            .flat_map(|band| band.fragments())
            .map(|fragment| ring_area(fragment.vertices()).abs())
            .sum::<f64>();
        assert!((fragment_area - area(&bands)).abs() < 1.0e-8);
        for i in 1..20 {
            for j in 1..(20 - i) {
                let a = i as f64 / 20.0;
                let b = j as f64 / 20.0;
                let value = 0.4 * a - 0.3 * b + 0.8 * (1.0 - a - b);
                if [0.05_f64, 0.25, 0.45]
                    .iter()
                    .any(|break_value| (value - break_value).abs() < 1.0e-9)
                {
                    continue;
                }
                let expected = bands.band_index_for(value).unwrap();
                let point = TernaryCoordinate::new(a, b, 1.0 - a - b);
                let covered = bands
                    .bands
                    .iter()
                    .enumerate()
                    .filter(|(_, band)| {
                        band.fragments()
                            .iter()
                            .any(|fragment| point_in_ring(point, fragment.vertices()))
                    })
                    .map(|(index, _)| index)
                    .collect::<Vec<_>>();
                assert_eq!(covered, vec![expected]);
            }
        }
    }
}
