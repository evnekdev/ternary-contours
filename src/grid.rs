#[cfg(test)]
use std::collections::BTreeSet;
use std::fmt;

use crate::{FieldError, GridEvaluationError};

/// Absolute tolerance used when accepting a nearly normalized composition.
///
/// Public point-location APIs accept finite components whose sum differs from
/// one by at most this amount, normalize that accepted input, then snap nearby
/// simplex and lattice boundaries. Inputs farther away are rejected rather
/// than silently projected into the simplex.
pub const POINT_LOCATION_TOLERANCE: f64 = 1.0e-10;

/// One elementary triangle and its canonical regular-grid vertices.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GridTriangle {
    /// Stable canonical elementary-triangle identifier.
    pub id: usize,
    /// Vertices in the triangle's canonical local barycentric order.
    pub vertices: [GridVertexId; 3],
}

/// Classification of a located point after tolerance snapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PointBoundaryLocation {
    /// The point is strictly inside its deterministically selected triangle.
    Interior,
    /// The point lies on an elementary-triangle edge.
    Edge,
    /// The point lies at a regular-grid vertex.
    Vertex,
}

/// A regular-grid triangle containing a normalized ternary composition.
///
/// `barycentric` is ordered to match [`Self::triangle`]'s vertices, contains
/// only non-negative values after snapping, and sums to one. The private grid
/// subdivision marker prevents accidentally using a location from an
/// incompatible regular grid while keeping the useful geometric fields public.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocatedTriangle {
    /// Deterministically owned elementary triangle.
    pub triangle: GridTriangle,
    /// Local barycentric coordinates in `triangle.vertices` order.
    pub barycentric: [f64; 3],
    /// Whether the located point is interior, on an edge, or at a vertex.
    pub boundary: PointBoundaryLocation,
    subdivisions: usize,
}

impl LocatedTriangle {
    pub(crate) const fn subdivisions(&self) -> usize {
        self.subdivisions
    }
}

/// Failure while locating a composition on a regular ternary grid.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum PointLocationError {
    /// One component of the requested composition was NaN or infinite.
    NonFiniteComposition { component: usize, value: f64 },
    /// The composition sum was not within [`POINT_LOCATION_TOLERANCE`] of one.
    InvalidCompositionSum { sum: f64 },
    /// A finite normalized component lies outside the ternary simplex.
    OutsideSimplex { composition: [f64; 3] },
    /// Checked grid-index arithmetic overflowed while selecting a triangle.
    GridIndexOverflow,
}

impl fmt::Display for PointLocationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteComposition { component, value } => write!(
                formatter,
                "composition component {component} is not finite: {value:?}"
            ),
            Self::InvalidCompositionSum { sum } => write!(
                formatter,
                "composition components must sum to one within {POINT_LOCATION_TOLERANCE:e}; received {sum:?}"
            ),
            Self::OutsideSimplex { composition } => write!(
                formatter,
                "composition {:?} lies outside the ternary simplex",
                composition
            ),
            Self::GridIndexOverflow => {
                write!(
                    formatter,
                    "regular-grid triangle index arithmetic overflowed"
                )
            }
        }
    }
}

impl std::error::Error for PointLocationError {}

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

    /// Lazily iterate scalar-field vertex identifiers and normalized compositions.
    ///
    /// Every identifier matches the canonical order expected by
    /// RegularTernaryScalarField::new.
    pub fn indexed_compositions(
        &self,
    ) -> impl ExactSizeIterator<Item = (GridVertexId, [f64; 3])> + Clone {
        self.compositions()
            .enumerate()
            .map(|(index, composition)| (GridVertexId(index), composition))
    }
}

impl RegularTernaryGrid {
    /// Number of elementary triangles in canonical order.
    pub fn triangle_count(&self) -> Result<usize, FieldError> {
        self.subdivisions
            .checked_mul(self.subdivisions)
            .ok_or(FieldError::AllocationOverflow)
    }

    /// Return the canonical vertex identifier for one lattice coordinate.
    pub fn vertex_id(&self, coordinate: LatticeCoordinate) -> Result<GridVertexId, FieldError> {
        if coordinate
            .i
            .checked_add(coordinate.j)
            .and_then(|value| value.checked_add(coordinate.k))
            != Some(self.subdivisions)
        {
            return Err(FieldError::InvalidLatticeCoordinate {
                i: coordinate.i,
                j: coordinate.j,
                k: coordinate.k,
                subdivisions: self.subdivisions,
            });
        }
        let prefix = coordinate
            .i
            .checked_mul(self.subdivisions + 1)
            .and_then(|value| {
                coordinate
                    .i
                    .checked_mul(coordinate.i.saturating_sub(1))
                    .map(|triangle| value - triangle / 2)
            })
            .ok_or(FieldError::AllocationOverflow)?;
        prefix
            .checked_add(coordinate.j)
            .map(GridVertexId)
            .ok_or(FieldError::AllocationOverflow)
    }

    /// Return the lattice coordinate for one canonical vertex identifier.
    pub fn lattice_coordinate(&self, id: GridVertexId) -> Result<LatticeCoordinate, FieldError> {
        if id.0 >= self.vertex_count {
            return Err(FieldError::InvalidVertexIndex {
                index: id.0,
                vertex_count: self.vertex_count,
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

    /// Return the normalized composition at one canonical vertex identifier.
    pub fn composition(&self, id: GridVertexId) -> Result<[f64; 3], FieldError> {
        Ok(composition_from_coordinate(
            self.lattice_coordinate(id)?,
            self.subdivisions as f64,
        ))
    }
}

impl RegularTernaryGrid {
    /// Generate elementary triangles in canonical regular-grid order.
    pub fn elementary_triangles(&self) -> Result<Vec<GridTriangle>, FieldError> {
        let mut triangles = Vec::with_capacity(self.triangle_count()?);
        for i in 0..self.subdivisions {
            for j in 0..(self.subdivisions - i) {
                triangles.push(self.upward_triangle(i, j)?);
                if i + j + 2 <= self.subdivisions {
                    triangles.push(self.downward_triangle(i, j)?);
                }
            }
        }
        debug_assert_eq!(triangles.len(), self.triangle_count()?);
        Ok(triangles)
    }

    pub(crate) fn triangle(&self, id: usize) -> Result<GridTriangle, FieldError> {
        let triangle_count = self.triangle_count()?;
        if id >= triangle_count {
            return Err(FieldError::InvalidVertexIndex {
                index: id,
                vertex_count: triangle_count,
            });
        }
        let mut row = 0usize;
        let mut row_start = 0usize;
        while row < self.subdivisions {
            let row_triangles = (self.subdivisions - row)
                .checked_mul(2)
                .and_then(|value| value.checked_sub(1))
                .ok_or(FieldError::AllocationOverflow)?;
            if id < row_start + row_triangles {
                let offset = id - row_start;
                let column = offset / 2;
                return if offset.is_multiple_of(2) {
                    self.upward_triangle(row, column)
                } else {
                    self.downward_triangle(row, column)
                };
            }
            row_start = row_start
                .checked_add(row_triangles)
                .ok_or(FieldError::AllocationOverflow)?;
            row += 1;
        }
        unreachable!("validated triangle id must occur in one row")
    }

    fn upward_triangle(&self, i: usize, j: usize) -> Result<GridTriangle, FieldError> {
        let k = self
            .subdivisions
            .checked_sub(i)
            .and_then(|value| value.checked_sub(j))
            .ok_or(FieldError::AllocationOverflow)?;
        if k == 0 {
            return Err(FieldError::InvalidLatticeCoordinate {
                i,
                j,
                k,
                subdivisions: self.subdivisions,
            });
        }
        let id = triangle_row_start(self.subdivisions, i)
            .and_then(|value| {
                j.checked_mul(2)
                    .and_then(|offset| value.checked_add(offset))
            })
            .ok_or(FieldError::AllocationOverflow)?;
        Ok(GridTriangle {
            id,
            vertices: [
                self.vertex_id(LatticeCoordinate { i, j, k })?,
                self.vertex_id(LatticeCoordinate {
                    i: i + 1,
                    j,
                    k: k - 1,
                })?,
                self.vertex_id(LatticeCoordinate {
                    i,
                    j: j + 1,
                    k: k - 1,
                })?,
            ],
        })
    }

    fn downward_triangle(&self, i: usize, j: usize) -> Result<GridTriangle, FieldError> {
        let k = self
            .subdivisions
            .checked_sub(i)
            .and_then(|value| value.checked_sub(j))
            .ok_or(FieldError::AllocationOverflow)?;
        if k < 2 {
            return Err(FieldError::InvalidLatticeCoordinate {
                i,
                j,
                k,
                subdivisions: self.subdivisions,
            });
        }
        let id = triangle_row_start(self.subdivisions, i)
            .and_then(|value| {
                j.checked_mul(2)
                    .and_then(|offset| value.checked_add(offset))
            })
            .and_then(|value| value.checked_add(1))
            .ok_or(FieldError::AllocationOverflow)?;
        Ok(GridTriangle {
            id,
            vertices: [
                self.vertex_id(LatticeCoordinate {
                    i: i + 1,
                    j,
                    k: k - 1,
                })?,
                self.vertex_id(LatticeCoordinate {
                    i: i + 1,
                    j: j + 1,
                    k: k - 2,
                })?,
                self.vertex_id(LatticeCoordinate {
                    i,
                    j: j + 1,
                    k: k - 1,
                })?,
            ],
        })
    }
}

impl RegularTernaryGrid {
    /// Locate a finite normalized composition in constant time with respect to
    /// the total grid triangle count.
    ///
    /// Input within [`POINT_LOCATION_TOLERANCE`] of normalized is normalized,
    /// then values close to simplex or lattice boundaries are snapped. Points
    /// on shared edges and vertices are owned by the lowest canonical triangle
    /// identifier among the bounded local candidate set.
    pub fn locate(&self, composition: [f64; 3]) -> Result<LocatedTriangle, PointLocationError> {
        let composition = normalized_composition(composition)?;
        let scale = self.subdivisions as f64;
        let scaled = composition.map(|value| snap_lattice(value * scale));
        let floor_i = floor_lattice_index(scaled[0], self.subdivisions)?;
        let floor_j = floor_lattice_index(scaled[1], self.subdivisions)?;
        let mut selected = None;

        for base_i in [floor_i.checked_sub(1), Some(floor_i)]
            .into_iter()
            .flatten()
        {
            for base_j in [floor_j.checked_sub(1), Some(floor_j)]
                .into_iter()
                .flatten()
            {
                if base_i
                    .checked_add(base_j)
                    .is_none_or(|sum| sum >= self.subdivisions)
                {
                    continue;
                }
                let delta_a = scaled[0] - base_i as f64;
                let delta_b = scaled[1] - base_j as f64;
                consider_location_candidate(
                    self.upward_triangle(base_i, base_j)
                        .map_err(|_| PointLocationError::GridIndexOverflow)?,
                    [1.0 - delta_a - delta_b, delta_a, delta_b],
                    scale,
                    &mut selected,
                );
                if base_i + base_j + 2 <= self.subdivisions {
                    consider_location_candidate(
                        self.downward_triangle(base_i, base_j)
                            .map_err(|_| PointLocationError::GridIndexOverflow)?,
                        [1.0 - delta_b, delta_a + delta_b - 1.0, 1.0 - delta_a],
                        scale,
                        &mut selected,
                    );
                }
            }
        }

        let Some((triangle, barycentric)) = selected else {
            return Err(PointLocationError::OutsideSimplex { composition });
        };
        let zeroes = barycentric
            .into_iter()
            .filter(|value| *value == 0.0)
            .count();
        let boundary = match zeroes {
            0 => PointBoundaryLocation::Interior,
            1 => PointBoundaryLocation::Edge,
            _ => PointBoundaryLocation::Vertex,
        };
        Ok(LocatedTriangle {
            triangle,
            barycentric,
            boundary,
            subdivisions: self.subdivisions,
        })
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
    grid: RegularTernaryGrid,
    values: Vec<f64>,
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
    /// Callback failures retain their source error, stable vertex identifier,
    /// and normalized composition in GridEvaluationError::Evaluation. Grid and
    /// field validation failures are returned as GridEvaluationError::Grid.
    ///
    /// Example:
    ///
    ///     use ternary_contours::{
    ///         GridEvaluationError, GridVertexId, RegularTernaryScalarField,
    ///     };
    ///
    ///     let error = RegularTernaryScalarField::try_from_fn(2, |composition| {
    ///         if composition == [0.0, 0.5, 0.5] {
    ///             Err("sample unavailable")
    ///         } else {
    ///             Ok(composition[0])
    ///         }
    ///     })
    ///     .unwrap_err();
    ///     assert!(matches!(
    ///         error,
    ///         GridEvaluationError::Evaluation {
    ///             index: GridVertexId(1),
    ///             source: "sample unavailable",
    ///             ..
    ///         }
    ///     ));
    pub fn try_from_fn<F, E>(
        subdivisions: usize,
        mut value_at: F,
    ) -> Result<Self, GridEvaluationError<E>>
    where
        F: FnMut([f64; 3]) -> Result<f64, E>,
    {
        let grid = RegularTernaryGrid::new(subdivisions).map_err(GridEvaluationError::Grid)?;
        let mut values = Vec::with_capacity(grid.vertex_count());
        for (index, composition) in grid.indexed_compositions() {
            let value =
                value_at(composition).map_err(|source| GridEvaluationError::Evaluation {
                    index,
                    composition,
                    source,
                })?;
            values.push(value);
        }
        Self::new(subdivisions, values).map_err(GridEvaluationError::Grid)
    }

    pub fn new(subdivisions: usize, values: Vec<f64>) -> Result<Self, FieldError> {
        let grid = RegularTernaryGrid::new(subdivisions)?;
        let expected = grid.vertex_count();
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
        Ok(Self { grid, values })
    }

    pub const fn subdivisions(&self) -> usize {
        self.grid.subdivisions()
    }
    /// Return the canonical regular grid on which this field is sampled.
    pub const fn grid(&self) -> RegularTernaryGrid {
        self.grid
    }
    pub fn values(&self) -> &[f64] {
        &self.values
    }
    pub const fn vertex_count(&self) -> usize {
        self.values.len()
    }
    pub fn triangle_count(&self) -> Result<usize, FieldError> {
        self.grid.triangle_count()
    }
    pub fn edge_count(&self) -> Result<usize, FieldError> {
        self.grid
            .subdivisions()
            .checked_mul(self.grid.subdivisions() + 1)
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
        self.grid.vertex_id(coordinate)
    }

    pub fn lattice_coordinate(&self, id: GridVertexId) -> Result<LatticeCoordinate, FieldError> {
        self.grid.lattice_coordinate(id)
    }

    pub fn composition(&self, id: GridVertexId) -> Result<[f64; 3], FieldError> {
        self.grid.composition(id)
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
        self.grid.elementary_triangles()
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

fn normalized_composition(composition: [f64; 3]) -> Result<[f64; 3], PointLocationError> {
    if let Some((component, value)) = composition
        .into_iter()
        .enumerate()
        .find(|(_, value)| !value.is_finite())
    {
        return Err(PointLocationError::NonFiniteComposition { component, value });
    }
    let sum = composition.into_iter().sum::<f64>();
    if (sum - 1.0).abs() > POINT_LOCATION_TOLERANCE {
        return Err(PointLocationError::InvalidCompositionSum { sum });
    }
    let mut normalized = composition.map(|value| value / sum);
    if normalized
        .into_iter()
        .any(|value| !(-POINT_LOCATION_TOLERANCE..=1.0 + POINT_LOCATION_TOLERANCE).contains(&value))
    {
        return Err(PointLocationError::OutsideSimplex { composition });
    }
    for value in &mut normalized {
        if value.abs() <= POINT_LOCATION_TOLERANCE {
            *value = 0.0;
        } else if (1.0 - *value).abs() <= POINT_LOCATION_TOLERANCE {
            *value = 1.0;
        }
    }
    let snapped_sum = normalized.into_iter().sum::<f64>();
    Ok(normalized.map(|value| value / snapped_sum))
}

fn snap_lattice(value: f64) -> f64 {
    let nearest = value.round();
    let tolerance = POINT_LOCATION_TOLERANCE * value.abs().max(1.0);
    if (value - nearest).abs() <= tolerance {
        nearest
    } else {
        value
    }
}

fn floor_lattice_index(value: f64, subdivisions: usize) -> Result<usize, PointLocationError> {
    if value < -POINT_LOCATION_TOLERANCE || value > subdivisions as f64 + POINT_LOCATION_TOLERANCE {
        return Err(PointLocationError::GridIndexOverflow);
    }
    if value >= subdivisions as f64 {
        Ok(subdivisions)
    } else {
        Ok(value.floor() as usize)
    }
}

fn canonical_barycentric(mut barycentric: [f64; 3], scale: f64) -> Option<[f64; 3]> {
    let tolerance = POINT_LOCATION_TOLERANCE * scale.max(1.0);
    if barycentric
        .into_iter()
        .any(|value| value < -tolerance || value > 1.0 + tolerance)
    {
        return None;
    }
    for value in &mut barycentric {
        if value.abs() <= tolerance {
            *value = 0.0;
        } else if (1.0 - *value).abs() <= tolerance {
            *value = 1.0;
        }
    }
    let sum = barycentric.into_iter().sum::<f64>();
    if sum <= 0.0 || !sum.is_finite() {
        return None;
    }
    Some(barycentric.map(|value| (value / sum).max(0.0)))
}

fn consider_location_candidate(
    triangle: GridTriangle,
    barycentric: [f64; 3],
    scale: f64,
    selected: &mut Option<(GridTriangle, [f64; 3])>,
) {
    let Some(barycentric) = canonical_barycentric(barycentric, scale) else {
        return;
    };
    if selected.is_none_or(|(current, _)| triangle.id < current.id) {
        *selected = Some((triangle, barycentric));
    }
}

fn triangle_row_start(subdivisions: usize, row: usize) -> Option<usize> {
    subdivisions
        .checked_mul(2)
        .and_then(|twice| twice.checked_sub(row))
        .and_then(|factor| row.checked_mul(factor))
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
                let index = id.0;
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
                Ok::<f64, &'static str>(2.0 * a - 3.0 * b + 5.0 * c)
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
            RegularTernaryScalarField::try_from_fn(0, |_| Ok::<f64, &'static str>(0.0)),
            Err(GridEvaluationError::Grid(FieldError::ZeroSubdivisions))
        ));
        assert!(matches!(
            RegularTernaryScalarField::from_fn(2, |_| f64::NAN),
            Err(FieldError::NonFiniteValue { index: 0, .. })
        ));
        assert!(matches!(
            RegularTernaryScalarField::try_from_fn(2, |_| { Ok::<f64, &'static str>(f64::NAN) }),
            Err(GridEvaluationError::Grid(FieldError::NonFiniteValue {
                index: 0,
                ..
            }))
        ));
    }

    #[test]
    fn fallible_evaluation_preserves_source_vertex_and_composition() {
        #[derive(Debug, PartialEq)]
        struct SampleUnavailable {
            reason: &'static str,
        }

        let mut calls = 0;
        let error = RegularTernaryScalarField::try_from_fn(3, |composition| {
            let index = calls;
            calls += 1;
            if index == 2 {
                Err(SampleUnavailable {
                    reason: "missing sample",
                })
            } else {
                Ok(composition[0] + composition[1])
            }
        })
        .unwrap_err();

        assert_eq!(calls, 3);
        assert_eq!(
            error,
            GridEvaluationError::Evaluation {
                index: GridVertexId(2),
                composition: [0.0, 2.0 / 3.0, 1.0 / 3.0],
                source: SampleUnavailable {
                    reason: "missing sample",
                },
            }
        );
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

    fn close(left: f64, right: f64) {
        assert!((left - right).abs() <= 2.0e-10, "{left:?} != {right:?}");
    }

    fn reference_locate(grid: &RegularTernaryGrid, point: [f64; 3]) -> LocatedTriangle {
        let point = normalized_composition(point).unwrap();
        for triangle in grid.elementary_triangles().unwrap() {
            let vertices = triangle.vertices.map(|id| grid.composition(id).unwrap());
            let determinant = (vertices[0][0] - vertices[2][0]) * (vertices[1][1] - vertices[2][1])
                - (vertices[1][0] - vertices[2][0]) * (vertices[0][1] - vertices[2][1]);
            let pa = point[0] - vertices[2][0];
            let pb = point[1] - vertices[2][1];
            let u = (pa * (vertices[1][1] - vertices[2][1])
                - (vertices[1][0] - vertices[2][0]) * pb)
                / determinant;
            let v = ((vertices[0][0] - vertices[2][0]) * pb
                - pa * (vertices[0][1] - vertices[2][1]))
                / determinant;
            if let Some(barycentric) =
                canonical_barycentric([u, v, 1.0 - u - v], grid.subdivisions() as f64)
            {
                let zeroes = barycentric.iter().filter(|value| **value == 0.0).count();
                return LocatedTriangle {
                    triangle,
                    barycentric,
                    boundary: match zeroes {
                        0 => PointBoundaryLocation::Interior,
                        1 => PointBoundaryLocation::Edge,
                        _ => PointBoundaryLocation::Vertex,
                    },
                    subdivisions: grid.subdivisions(),
                };
            }
        }
        panic!("reference locator did not find {point:?}")
    }

    #[test]
    fn direct_locator_matches_exhaustive_reference_and_reconstructs_points() {
        let mut state = 0x7e57_1a11_u64;
        let next = |state: &mut u64| {
            *state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            (*state >> 11) as f64 / ((u64::MAX >> 11) as f64)
        };
        for subdivisions in [1, 2, 3, 7, 31, 127] {
            let grid = RegularTernaryGrid::new(subdivisions).unwrap();
            for _ in 0..257 {
                let a = next(&mut state);
                let b = (1.0 - a) * next(&mut state);
                let point = [a, b, 1.0 - a - b];
                let direct = grid.locate(point).unwrap();
                let reference = reference_locate(&grid, point);
                assert_eq!(direct.triangle, reference.triangle);
                assert_eq!(direct.boundary, reference.boundary);
                for (actual, expected) in direct.barycentric.into_iter().zip(reference.barycentric)
                {
                    close(actual, expected);
                }
                let reconstructed = direct
                    .triangle
                    .vertices
                    .map(|id| grid.composition(id).unwrap());
                for component in 0..3 {
                    close(
                        direct
                            .barycentric
                            .into_iter()
                            .zip(reconstructed)
                            .map(|(weight, vertex)| weight * vertex[component])
                            .sum(),
                        point[component],
                    );
                }
            }
        }
    }

    #[test]
    fn direct_locator_handles_vertices_centres_edges_and_invalid_input() {
        for subdivisions in [1, 2, 5, 19] {
            let grid = RegularTernaryGrid::new(subdivisions).unwrap();
            for (id, point) in grid.indexed_compositions() {
                let location = grid.locate(point).unwrap();
                assert_eq!(location.boundary, PointBoundaryLocation::Vertex);
                assert!(location.triangle.vertices.contains(&id));
            }
            for triangle in grid.elementary_triangles().unwrap() {
                let vertices = triangle.vertices.map(|id| grid.composition(id).unwrap());
                let centre = vertices.map(|vertex| vertex.map(|value| value / 3.0));
                let point = [
                    centre.into_iter().map(|value| value[0]).sum(),
                    centre.into_iter().map(|value| value[1]).sum(),
                    centre.into_iter().map(|value| value[2]).sum(),
                ];
                let location = grid.locate(point).unwrap();
                assert_eq!(location.triangle, triangle);
                assert_eq!(location.boundary, PointBoundaryLocation::Interior);
            }
            for triangle in grid.elementary_triangles().unwrap() {
                for [left, right] in [[0, 1], [1, 2], [2, 0]] {
                    let a = grid.composition(triangle.vertices[left]).unwrap();
                    let b = grid.composition(triangle.vertices[right]).unwrap();
                    let point = [
                        (a[0] + b[0]) / 2.0,
                        (a[1] + b[1]) / 2.0,
                        (a[2] + b[2]) / 2.0,
                    ];
                    assert!(matches!(
                        grid.locate(point).unwrap().boundary,
                        PointBoundaryLocation::Edge
                    ));
                }
            }
        }
        let grid = RegularTernaryGrid::new(3).unwrap();
        for point in [[-0.01, 0.4, 0.61], [1.01, 0.0, -0.01]] {
            assert!(matches!(
                grid.locate(point),
                Err(PointLocationError::OutsideSimplex { .. })
                    | Err(PointLocationError::InvalidCompositionSum { .. })
            ));
        }
        for point in [[f64::NAN, 0.0, 1.0], [f64::INFINITY, 0.0, 1.0]] {
            assert!(matches!(
                grid.locate(point),
                Err(PointLocationError::NonFiniteComposition { .. })
            ));
        }
        assert!(matches!(
            grid.locate([0.2, 0.2, 0.2]),
            Err(PointLocationError::InvalidCompositionSum { .. })
        ));
    }
}
