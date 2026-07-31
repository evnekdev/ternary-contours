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
    pub fn new(subdivisions: usize, values: Vec<f64>) -> Result<Self, FieldError> {
        if subdivisions == 0 {
            return Err(FieldError::ZeroSubdivisions);
        }
        let expected = Self::vertex_count_for(subdivisions)?;
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
        let denominator = self.subdivisions as f64;
        Ok([
            coordinate.i as f64 / denominator,
            coordinate.j as f64 / denominator,
            coordinate.k as f64 / denominator,
        ])
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

    fn vertex_count_for(subdivisions: usize) -> Result<usize, FieldError> {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(n: usize) -> RegularTernaryScalarField {
        let count = (n + 1) * (n + 2) / 2;
        RegularTernaryScalarField::new(n, (0..count).map(|value| value as f64).collect()).unwrap()
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
