use std::collections::{BTreeMap, BTreeSet};

use crate::TernaryCoordinate;

use super::ContourError;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ContourSegment {
    pub start: TernaryCoordinate,
    pub end: TernaryCoordinate,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContourPath {
    /// Ordered semantic A/B/C coordinates with no duplicate closing endpoint.
    pub points: Vec<TernaryCoordinate>,
    /// Whether the final point connects periodically back to the first.
    pub closed: bool,
}

pub(crate) fn join_segments(
    segments: Vec<ContourSegment>,
    tolerance: f64,
) -> Result<Vec<ContourPath>, ContourError> {
    let mut nodes: Vec<TernaryCoordinate> = Vec::new();
    let mut buckets = BTreeMap::<[i64; 3], Vec<usize>>::new();
    let mut edges: Vec<(usize, usize)> = Vec::new();
    let mut unique_edges = BTreeSet::new();
    for segment in segments {
        if close(segment.start, segment.end, tolerance) {
            continue;
        }
        let start = node_for(&mut nodes, &mut buckets, segment.start, tolerance);
        let end = node_for(&mut nodes, &mut buckets, segment.end, tolerance);
        let key = if start < end {
            (start, end)
        } else {
            (end, start)
        };
        if start != end && unique_edges.insert(key) {
            edges.push((start, end));
        }
    }
    let mut adjacency = vec![Vec::<(usize, usize)>::new(); nodes.len()];
    for (edge_index, &(left, right)) in edges.iter().enumerate() {
        adjacency[left].push((right, edge_index));
        adjacency[right].push((left, edge_index));
    }
    if let Some(degree) = adjacency.iter().map(Vec::len).find(|degree| *degree > 2) {
        return Err(ContourError::BranchingTopology { degree });
    }
    let mut used = vec![false; edges.len()];
    let mut paths = Vec::new();
    let mut starts: Vec<_> = adjacency
        .iter()
        .enumerate()
        .filter(|(_, edges)| edges.len() == 1)
        .map(|(index, _)| index)
        .collect();
    starts.sort_by(|&a, &b| lex_cmp(nodes[a], nodes[b]));
    for start in starts {
        if adjacency[start].iter().all(|(_, edge)| used[*edge]) {
            continue;
        }
        paths.push(walk(start, &nodes, &adjacency, &mut used, false)?);
    }
    while let Some(edge_index) = used.iter().position(|used| !*used) {
        let (a, b) = edges[edge_index];
        let start = if lex_cmp(nodes[a], nodes[b]).is_le() {
            a
        } else {
            b
        };
        paths.push(walk(start, &nodes, &adjacency, &mut used, true)?);
    }
    paths.sort_by(|left, right| {
        lex_cmp(left.points[0], right.points[0])
            .then_with(|| left.points.len().cmp(&right.points.len()))
    });
    Ok(paths)
}

fn walk(
    start: usize,
    nodes: &[TernaryCoordinate],
    adjacency: &[Vec<(usize, usize)>],
    used: &mut [bool],
    expect_closed: bool,
) -> Result<ContourPath, ContourError> {
    let mut points = vec![nodes[start]];
    let mut current = start;
    let mut previous = None;
    loop {
        let next = adjacency[current]
            .iter()
            .filter(|(_, edge)| !used[*edge])
            .min_by(|(a, _), (b, _)| lex_cmp(nodes[*a], nodes[*b]))
            .copied();
        let Some((node, edge)) = next else { break };
        used[edge] = true;
        previous = Some(current);
        current = node;
        if current == start {
            break;
        }
        points.push(nodes[current]);
        if points.len() > used.len() + 1 {
            return Err(ContourError::BranchingTopology {
                degree: adjacency[current].len(),
            });
        }
    }
    let closed = current == start;
    if expect_closed && !closed {
        return Err(ContourError::InvalidClosedLoop);
    }
    if closed && points.len() < 3 {
        return Err(ContourError::InvalidClosedLoop);
    }
    if points.len() < 2 {
        return Err(ContourError::ZeroLengthPath);
    }
    let _ = previous;
    if !closed && lex_cmp(*points.last().unwrap(), points[0]).is_lt() {
        points.reverse();
    }
    if closed {
        let minimum = (0..points.len())
            .min_by(|&a, &b| lex_cmp(points[a], points[b]))
            .unwrap();
        points.rotate_left(minimum);
        if points.len() > 2 && lex_cmp(points[points.len() - 1], points[1]).is_lt() {
            points[1..].reverse();
        }
    }
    Ok(ContourPath { points, closed })
}
fn node_for(
    nodes: &mut Vec<TernaryCoordinate>,
    buckets: &mut BTreeMap<[i64; 3], Vec<usize>>,
    point: TernaryCoordinate,
    tolerance: f64,
) -> usize {
    let key = bucket_key(point, tolerance);
    let mut matched = None;
    for da in -1_i64..=1 {
        for db in -1_i64..=1 {
            for dc in -1_i64..=1 {
                let neighbour = [
                    key[0].saturating_add(da),
                    key[1].saturating_add(db),
                    key[2].saturating_add(dc),
                ];
                if let Some(indices) = buckets.get(&neighbour) {
                    for &index in indices {
                        if close(nodes[index], point, tolerance) {
                            matched =
                                Some(matched.map_or(index, |current: usize| current.min(index)));
                        }
                    }
                }
            }
        }
    }
    if let Some(index) = matched {
        return index;
    }
    let index = nodes.len();
    nodes.push(point);
    buckets.entry(key).or_default().push(index);
    index
}

fn bucket_key(point: TernaryCoordinate, tolerance: f64) -> [i64; 3] {
    point.as_array().map(|component| {
        let scaled = (component / tolerance).floor();
        if scaled <= i64::MIN as f64 {
            i64::MIN
        } else if scaled >= i64::MAX as f64 {
            i64::MAX
        } else {
            scaled as i64
        }
    })
}
fn close(left: TernaryCoordinate, right: TernaryCoordinate, tolerance: f64) -> bool {
    left.as_array()
        .into_iter()
        .zip(right.as_array())
        .all(|(a, b)| (a - b).abs() <= tolerance)
}
fn lex_cmp(left: TernaryCoordinate, right: TernaryCoordinate) -> std::cmp::Ordering {
    for (a, b) in left.as_array().into_iter().zip(right.as_array()) {
        let order = a.total_cmp(&b);
        if !order.is_eq() {
            return order;
        }
    }
    std::cmp::Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(a: f64, b: f64) -> TernaryCoordinate {
        TernaryCoordinate::new(a, b, 1.0 - a - b)
    }
    #[test]
    fn open_and_closed_paths_join_deterministically() {
        let open = join_segments(
            vec![
                ContourSegment {
                    start: p(0.5, 0.4),
                    end: p(0.3, 0.4),
                },
                ContourSegment {
                    start: p(0.3, 0.4),
                    end: p(0.1, 0.4),
                },
            ],
            1e-9,
        )
        .unwrap();
        assert_eq!(open.len(), 1);
        assert!(!open[0].closed);
        assert_eq!(open[0].points[0], p(0.1, 0.4));
        let closed = join_segments(
            vec![
                ContourSegment {
                    start: p(0.2, 0.2),
                    end: p(0.5, 0.2),
                },
                ContourSegment {
                    start: p(0.5, 0.2),
                    end: p(0.3, 0.5),
                },
                ContourSegment {
                    start: p(0.3, 0.5),
                    end: p(0.2, 0.2),
                },
            ],
            1e-9,
        )
        .unwrap();
        assert_eq!(closed.len(), 1);
        assert!(closed[0].closed);
        assert_eq!(closed[0].points.len(), 3);
    }
    #[test]
    fn branching_is_reported() {
        let centre = p(0.3, 0.3);
        let segments = [p(0.1, 0.3), p(0.5, 0.3), p(0.3, 0.5)]
            .map(|end| ContourSegment { start: centre, end });
        assert!(matches!(
            join_segments(segments.to_vec(), 1e-9),
            Err(ContourError::BranchingTopology { degree: 3 })
        ));
    }
}
