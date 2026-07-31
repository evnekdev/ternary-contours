#[cfg(test)]
use std::collections::BTreeSet;

use crate::FieldError;

/// Stable identifier in the field's canonical value ordering.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GridVertexId(pub usize);

/// Integer ternary lattice coordinate satisfying `i+j+k=n`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LatticeCoordinate {
    pub i: usize,
    pub j: usize,
    pub k: usize,
}

/// A reusable view of the canonical regular ternary composition lattice.
///
/// The grid visits lattice coordinates in the exact order used by
/// RegularTernaryScalarField: i increases first, then j, with
/// k = subdivisions - i - j. Its iterators are lazy and do not allocate.
///
/// Example:
///
///     use ternary_contours::{LatticeCoordinate, RegularTernaryGrid};
///
///     let grid = RegularTernaryGrid::new(2)?;
///     let coordinates: Vec<_> = grid.lattice_coordinates().collect();
///     assert_eq!(coordinates[0], LatticeCoordinate { i: 0, j: 0, k: 2 });
///     # Ok::<(), ternary_contours::FieldError>(())
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegularTernaryGrid {
    subdivisions: usize,
    vertex_count: usize,
}

impl RegularTernaryGrid {
    /// Create a regular ternary lattice with subdivisions intervals per edge.
    ///
    /// The subdivision count must be positive. Its vertex count is
    /// (n + 1) * (n + 2) / 2 and is checked for overflow.
    pub fn new(subdivisions: usize) -> Result<Self, FieldError> {
        if subdivisions == 0 {
            return Err(FieldError::ZeroSubdivisions);
        }
        Ok(Self {
            subdivisions,
            vertex_count: canonical_vertex_count(subdivisions)?,
        })
    }

    /// Number of intervals on each binary edge.
    pub const fn subdivisions(&self) -> usize {
        self.subdivisions
    }

    /// Number of vertices in canonical scalar-field order.
    pub const fn vertex_count(&self) -> usize {
        self.vertex_count
    }

    /// Lazily iterate canonical integer lattice coordinates.
    ///
    /// The iterator is exact-sized and yields i = 0..=n, j = 0..=n-i,
    /// and k = n-i-j.
    pub fn lattice_coordinates(&self) -> impl ExactSizeIterator<Item = LatticeCoordinate> + Clone {
        CanonicalLatticeCoordinates {
            subdivisions: self.subdivisions,
            i: 0,
            j: 0,
            remaining: self.vertex_count,
        }
    }

    /// Lazily iterate normalized semantic compositions in canonical field order.
    ///
    /// Each item is [i/n, j/n, k/n] in A/B/C component order.
    ///
    /// Example:
    ///
    ///     use ternary_contours::RegularTernaryGrid;
    ///
    ///     let grid = RegularTernaryGrid::new(2)?;
    ///     assert_eq!(grid.compositions().next(), Some([0.0, 0.0, 1.0]));
    ///     # Ok::<(), ternary_contours::FieldError>(())
    pub fn compositions(&self) -> impl ExactSizeIterator<Item = [f64; 3]> + Clone {
        let denominator = self.subdivisions as f64;
        self.lattice_coordinates()
            .map(move |coordinate| composition_from_coordinate(coordinate, denominator))
    }

    /// Lazily iterate scalar-field value indices and normalized compositions.
    ///
    /// Every index matches the canonical order expected by
    /// RegularTernaryScalarField::new.
    pub fn indexed_compositions(&self) -> impl ExactSizeIterator<Item = (usize, [f64; 3])> + Clone {
        self.compositions().enumerate()
    }
}

#[derive(Clone, Debug)]
struct CanonicalLatticeCoordinates {
    subdivisions: usize,
    i: usize,
    j: usize,
    remaining: usize,
}

impl Iterator for CanonicalLatticeCoordinates {
    type Item = LatticeCoordinate;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let coordinate = LatticeCoordinate {
            i: self.i,
            j: self.j,
            k: self.subdivisions - self.i - self.j,
        };
        self.remaining -= 1;

        if self.j == self.subdivisions - self.i {
            self.i += 1;
            self.j = 0;
        } else {
            self.j += 1;
        }

        Some(coordinate)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl ExactSizeIterator for CanonicalLatticeCoordinates {}

/// A scalar field sampled on the regular ternary lattice `i+j+k=n`.
///
/// Values use row-major `(i,j)` ordering: `i` increases from zero to `n`, and
/// for each `i`, `j` increases from zero to `n-i`; `k=n-i-j`.
#[derive(Clone, Debug, PartialEq)]
pub struct RegularTernaryScalarField {
    subdivisions: usize,
    values: Vec<f64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GridTriangle {
    pub id: usize,
    pub vertices: [GridVertexId; 3],
}

#[cfg(feature = "cubic-alpha")]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct GridEdgeKey {
    pub start: GridVertexId,
    pub end: GridVertexId,
}

#[cfg(feature = "cubic-alpha")]
impl GridEdgeKey {
    pub const fn new(left: GridVertexId, right: GridVertexId) -> Self {
        if left.0 < right.0 {
            Self {
                start: left,
                end: right,
            }
        } else {
            Self {
                start: right,
                end: left,
            }
        }
    }
}

impl RegularTernaryScalarField {
    /// Construct a field by evaluating one scalar value at every canonical grid
    /// composition.
    ///
    /// The callback receives normalized [a, b, c] compositions in the same
    /// order as RegularTernaryGrid::compositions. The produced values are
    /// validated exactly as they are by Self::new.
    ///
    /// Example:
    ///
    ///     use ternary_contours::RegularTernaryScalarField;
    ///
    ///     let field = RegularTernaryScalarField::from_fn(2, |[a, b, c]| {
    ///         2.0 * a - 3.0 * b + 5.0 * c
    ///     })?;
    ///     assert_eq!(field.vertex_count(), 6);
    ///     # Ok::<(), ternary_contours::FieldError>(())
    pub fn from_fn<F>(subdivisions: usize, value_at: F) -> Result<Self, FieldError>
    where
        F: FnMut([f64; 3]) -> f64,
    {
        let grid = RegularTernaryGrid::new(subdivisions)?;
        let values = grid.compositions().map(value_at).collect();
        Self::new(subdivisions, values)
    }

    /// Fallibly construct a field by evaluating canonical grid compositions.
    ///
    /// Callback failures use the existing FieldError type and are returned
    /// unchanged; this avoids introducing a second generic error layer for this
    /// focused convenience constructor.
    pub fn try_from_fn<F>(subdivisions: usize, mut value_at: F) -> Result<Self, FieldError>
    where
        F: FnMut([f64; 3]) -> Result<f64, FieldError>,
    {
        let grid = RegularTernaryGrid::new(subdivisions)?;
        let mut values = Vec::with_capacity(grid.vertex_count());
        for composition in grid.compositions() {
            values.push(value_at(composition)?);
        }
        Self::new(subdivisions, values)
    }

    pub fn new(subdivisions: usize, values: Vec<f64>) -> Result<Self, FieldError> {
        if subdivisions == 0 {
            return Err(FieldError::ZeroSubdivisions);
        }
        let expected = canonical_vertex_count(subdivisions)?;
        if values.len() != expected {
            return Err(FieldError::IncorrectValueCount {
                expected,
                actual: values.len(),
            });
        }
        if let Some((index, value)) = values
            .iter()
            .copied()
            .enumerate()
            .find(|(_, value)| !value.is_finite())
        {
            return Err(FieldError::NonFiniteValue { index, value });
        }
        Ok(Self {
            subdivisions,
            values,
        })
    }

    pub const fn subdivisions(&self) -> usize {
        self.subdivisions
    }
    pub fn values(&self) -> &[f64] {
        &self.values
    }
    pub const fn vertex_count(&self) -> usize {
        self.values.len()
    }
    pub fn triangle_count(&self) -> Result<usize, FieldError> {
        self.subdivisions
            .checked_mul(self.subdivisions)
            .ok_or(FieldError::AllocationOverflow)
    }
    pub fn edge_count(&self) -> Result<usize, FieldError> {
        self.subdivisions
            .checked_mul(self.subdivisions + 1)
            .and_then(|value| value.checked_mul(3))
            .map(|value| value / 2)
            .ok_or(FieldError::AllocationOverflow)
    }

    pub fn value(&self, id: GridVertexId) -> Result<f64, FieldError> {
        self.values
            .get(id.0)
            .copied()
            .ok_or(FieldError::InvalidVertexIndex {
                index: id.0,
                vertex_count: self.values.len(),
            })
    }

    pub fn vertex_id(&self, coordinate: LatticeCoordinate) -> Result<GridVertexId, FieldError> {
        if coordinate
            .i
            .checked_add(coordinate.j)
            .and_then(|v| v.checked_add(coordinate.k))
            != Some(self.subdivisions)
        {
            return Err(FieldError::InvalidLatticeCoordinate {
                i: coordinate.i,
                j: coordinate.j,
                k: coordinate.k,
                subdivisions: self.subdivisions,
            });
        }
        let i = coordinate.i;
        let prefix = i
            .checked_mul(self.subdivisions + 1)
            .and_then(|v| i.checked_mul(i.saturating_sub(1)).map(|tri| v - tri / 2))
            .ok_or(FieldError::AllocationOverflow)?;
        Ok(GridVertexId(prefix + coordinate.j))
    }

    pub fn lattice_coordinate(&self, id: GridVertexId) -> Result<LatticeCoordinate, FieldError> {
        if id.0 >= self.values.len() {
            return Err(FieldError::InvalidVertexIndex {
                index: id.0,
                vertex_count: self.values.len(),
            });
        }
        let mut offset = 0;
        for i in 0..=self.subdivisions {
            let row = self.subdivisions - i + 1;
            if id.0 < offset + row {
                let j = id.0 - offset;
                return Ok(LatticeCoordinate {
                    i,
                    j,
                    k: self.subdivisions - i - j,
                });
            }
            offset += row;
        }
        unreachable!("validated vertex id must occur in one row")
    }

    pub fn composition(&self, id: GridVertexId) -> Result<[f64; 3], FieldError> {
        let coordinate = self.lattice_coordinate(id)?;
        Ok(composition_from_coordinate(
            coordinate,
            self.subdivisions as f64,
        ))
    }

    pub fn index_of(&self, i: usize, j: usize, k: usize) -> Result<usize, FieldError> {
        Ok(self.vertex_id(LatticeCoordinate { i, j, k })?.0)
    }

    pub fn coordinate_of(&self, index: usize) -> Result<LatticeCoordinate, FieldError> {
        self.lattice_coordinate(GridVertexId(index))
    }

    pub fn composition_at(&self, index: usize) -> Result<[f64; 3], FieldError> {
        self.composition(GridVertexId(index))
    }

    pub fn elementary_triangles(&self) -> Result<Vec<GridTriangle>, FieldError> {
        let capacity = self.triangle_count()?;
        let mut triangles = Vec::with_capacity(capacity);
        for i in 0..self.subdivisions {
            for j in 0..(self.subdivisions - i) {
                let k = self.subdivisions - i - j;
                let p = self.vertex_id(LatticeCoordinate { i, j, k })?;
                let q = self.vertex_id(LatticeCoordinate {
                    i: i + 1,
                    j,
                    k: k - 1,
                })?;
                let r = self.vertex_id(LatticeCoordinate {
                    i,
                    j: j + 1,
                    k: k - 1,
                })?;
                triangles.push(GridTriangle {
                    id: triangles.len(),
                    vertices: [p, q, r],
                });
                if k >= 2 {
                    let s = self.vertex_id(LatticeCoordinate {
                        i: i + 1,
                        j: j + 1,
                        k: k - 2,
                    })?;
                    triangles.push(GridTriangle {
                        id: triangles.len(),
                        vertices: [q, s, r],
                    });
                }
            }
        }
        debug_assert_eq!(triangles.len(), capacity);
        Ok(triangles)
    }

    #[cfg(test)]
    pub(crate) fn unique_edges(
        &self,
    ) -> Result<BTreeSet<(GridVertexId, GridVertexId)>, FieldError> {
        let mut edges = BTreeSet::new();
        for triangle in self.elementary_triangles()? {
            for [left, right] in [[0, 1], [1, 2], [2, 0]] {
                let a = triangle.vertices[left];
                let b = triangle.vertices[right];
                edges.insert(if a < b { (a, b) } else { (b, a) });
            }
        }
        Ok(edges)
    }
}

fn canonical_vertex_count(subdivisions: usize) -> Result<usize, FieldError> {
    subdivisions
        .checked_add(1)
        .and_then(|left| {
            subdivisions
                .checked_add(2)
                .and_then(|right| left.checked_mul(right))
        })
        .map(|value| value / 2)
        .ok_or(FieldError::AllocationOverflow)
}

fn composition_from_coordinate(coordinate: LatticeCoordinate, denominator: f64) -> [f64; 3] {
    [
        coordinate.i as f64 / denominator,
        coordinate.j as f64 / denominator,
        coordinate.k as f64 / denominator,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(n: usize) -> RegularTernaryScalarField {
        let count = (n + 1) * (n + 2) / 2;
        RegularTernaryScalarField::new(n, (0..count).map(|value| value as f64).collect()).unwrap()
    }

    fn requires_exact_size<I: ExactSizeIterator>(_iterator: I) {}

    #[test]
    fn grid_iterators_match_field_order_and_accessors() {
        for subdivisions in [1, 2, 3, 5] {
            let grid = RegularTernaryGrid::new(subdivisions).unwrap();
            assert_eq!(grid.subdivisions(), subdivisions);
            assert_eq!(
                grid.vertex_count(),
                (subdivisions + 1) * (subdivisions + 2) / 2
            );
            requires_exact_size(grid.lattice_coordinates());
            requires_exact_size(grid.compositions());
            requires_exact_size(grid.indexed_compositions());

            let mut coordinates = grid.lattice_coordinates();
            assert_eq!(coordinates.len(), grid.vertex_count());
            let first = coordinates.next().unwrap();
            assert_eq!(
                first,
                LatticeCoordinate {
                    i: 0,
                    j: 0,
                    k: subdivisions
                }
            );
            assert_eq!(coordinates.len(), grid.vertex_count() - 1);

            let expected_coordinates: Vec<_> = (0..=subdivisions)
                .flat_map(|i| {
                    (0..=subdivisions - i).map(move |j| LatticeCoordinate {
                        i,
                        j,
                        k: subdivisions - i - j,
                    })
                })
                .collect();
            assert_eq!(
                grid.lattice_coordinates().collect::<Vec<_>>(),
                expected_coordinates
            );

            let values: Vec<_> = grid
                .compositions()
                .map(|[a, b, c]| 2.0 * a - 3.0 * b + 5.0 * c)
                .collect();
            let field = RegularTernaryScalarField::new(subdivisions, values.clone()).unwrap();
            assert_eq!(field.vertex_count(), grid.vertex_count());

            for ((id, composition), coordinate) in
                grid.indexed_compositions().zip(grid.lattice_coordinates())
            {
                let index = id;
                assert_eq!(
                    field
                        .index_of(coordinate.i, coordinate.j, coordinate.k)
                        .unwrap(),
                    index
                );
                assert_eq!(field.coordinate_of(index).unwrap(), coordinate);
                assert_eq!(field.composition_at(index).unwrap(), composition);
                assert_eq!(
                    composition,
                    [
                        coordinate.i as f64 / subdivisions as f64,
                        coordinate.j as f64 / subdivisions as f64,
                        coordinate.k as f64 / subdivisions as f64,
                    ]
                );
            }

            let generated = RegularTernaryScalarField::from_fn(subdivisions, |[a, b, c]| {
                2.0 * a - 3.0 * b + 5.0 * c
            })
            .unwrap();
            assert_eq!(generated.values(), values);

            let fallible = RegularTernaryScalarField::try_from_fn(subdivisions, |[a, b, c]| {
                Ok::<f64, FieldError>(2.0 * a - 3.0 * b + 5.0 * c)
            })
            .unwrap();
            assert_eq!(fallible, generated);
        }
    }

    #[test]
    fn grid_and_function_constructors_preserve_typed_errors() {
        assert!(matches!(
            RegularTernaryGrid::new(0),
            Err(FieldError::ZeroSubdivisions)
        ));
        assert!(matches!(
            RegularTernaryScalarField::from_fn(0, |_| 0.0),
            Err(FieldError::ZeroSubdivisions)
        ));
        assert!(matches!(
            RegularTernaryScalarField::try_from_fn(0, |_| Ok::<f64, FieldError>(0.0)),
            Err(FieldError::ZeroSubdivisions)
        ));
        assert!(matches!(
            RegularTernaryScalarField::from_fn(2, |_| f64::NAN),
            Err(FieldError::NonFiniteValue { index: 0, .. })
        ));
    }

    #[test]
    fn canonical_order_and_index_round_trips_are_stable() {
        let field = field(4);
        assert_eq!(field.vertex_count(), 15);
        let expected = [
            (0, 0, 4),
            (0, 1, 3),
            (0, 2, 2),
            (0, 3, 1),
            (0, 4, 0),
            (1, 0, 3),
            (1, 1, 2),
            (1, 2, 1),
            (1, 3, 0),
            (2, 0, 2),
            (2, 1, 1),
            (2, 2, 0),
            (3, 0, 1),
            (3, 1, 0),
            (4, 0, 0),
        ];
        for (index, &(i, j, k)) in expected.iter().enumerate() {
            assert_eq!(
                field.coordinate_of(index).unwrap(),
                LatticeCoordinate { i, j, k }
            );
            assert_eq!(field.index_of(i, j, k).unwrap(), index);
            let point = field.composition_at(index).unwrap();
            assert!((point.into_iter().sum::<f64>() - 1.0).abs() < 1.0e-12);
        }
    }

    #[test]
    fn triangle_and_unique_edge_counts_match_regular_lattice_formulae() {
        for n in 1..=12 {
            let field = field(n);
            assert_eq!(field.elementary_triangles().unwrap().len(), n * n);
            assert_eq!(field.unique_edges().unwrap().len(), 3 * n * (n + 1) / 2);
            assert_eq!(field.edge_count().unwrap(), 3 * n * (n + 1) / 2);
        }
    }

    #[test]
    fn construction_rejects_invalid_sizes_values_and_overflow() {
        assert!(matches!(
            RegularTernaryScalarField::new(0, vec![]),
            Err(FieldError::ZeroSubdivisions)
        ));
        assert!(matches!(
            RegularTernaryScalarField::new(2, vec![0.0; 5]),
            Err(FieldError::IncorrectValueCount {
                expected: 6,
                actual: 5
            })
        ));
        let mut values = vec![0.0; 6];
        values[3] = f64::NAN;
        assert!(matches!(
            RegularTernaryScalarField::new(2, values),
            Err(FieldError::NonFiniteValue { index: 3, .. })
        ));
        assert!(matches!(
            RegularTernaryScalarField::new(usize::MAX, Vec::new()),
            Err(FieldError::AllocationOverflow)
        ));
    }
}
