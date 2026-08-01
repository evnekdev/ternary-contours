use crate::{FieldInterpolation, RegularTernaryScalarField};
#[cfg(feature = "irregular-delaunay")]
use crate::{IrregularFieldInterpolation, IrregularTernaryScalarField};

/// Stable user-defined identifier for one phase in an ensemble.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StablePhaseId(pub u32);

/// Quantity traced after height-defined stable regions have been constructed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum StableContourQuantity {
    /// Trace the stable phase height itself.
    Height,
    /// Trace each phase's secondary scalar inside its height-stable region.
    Secondary,
}

/// One scalar source used only to sample the virtual regular sampling grid.
///
/// The selected source interpolation is prepared once. Regardless of this
/// choice, the resulting sampling-grid representation is affine per sampling-grid
/// triangle.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum StableScalarSource<'a> {
    /// A regular scalar field with linear or optional cubic-alpha sampling.
    Regular {
        /// Sampled regular source field.
        field: &'a RegularTernaryScalarField,
        /// Interpolation used while resampling.
        interpolation: FieldInterpolation,
    },
    /// An irregular Delaunay field with linear or optional cubic-alpha sampling.
    #[cfg(feature = "irregular-delaunay")]
    Irregular {
        /// Sampled irregular source field.
        field: &'a IrregularTernaryScalarField,
        /// Interpolation used while resampling.
        interpolation: IrregularFieldInterpolation,
    },
}

impl<'a> StableScalarSource<'a> {
    /// Construct a regular source layer.
    pub const fn regular(
        field: &'a RegularTernaryScalarField,
        interpolation: FieldInterpolation,
    ) -> Self {
        Self::Regular {
            field,
            interpolation,
        }
    }

    /// Construct an irregular source layer.
    #[cfg(feature = "irregular-delaunay")]
    pub const fn irregular(
        field: &'a IrregularTernaryScalarField,
        interpolation: IrregularFieldInterpolation,
    ) -> Self {
        Self::Irregular {
            field,
            interpolation,
        }
    }

    pub(crate) fn geometry_key(self) -> SourceGeometryKey<'a> {
        match self {
            Self::Regular { field, .. } => SourceGeometryKey::regular(field.subdivisions()),
            #[cfg(feature = "irregular-delaunay")]
            Self::Irregular { field, .. } => SourceGeometryKey::Irregular(field.mesh()),
        }
    }

    pub(crate) fn has_same_topology(self, other: Self) -> bool {
        match (self, other) {
            (Self::Regular { field: left, .. }, Self::Regular { field: right, .. }) => {
                left.subdivisions() == right.subdivisions()
            }
            #[cfg(feature = "irregular-delaunay")]
            (Self::Irregular { field: left, .. }, Self::Irregular { field: right, .. }) => {
                left.mesh().has_same_identity(right.mesh())
            }
            #[cfg(feature = "irregular-delaunay")]
            _ => false,
        }
    }
}

/// Height and optional secondary source for one stable phase.
#[derive(Clone, Copy, Debug)]
pub struct StablePhaseSource<'a> {
    phase: StablePhaseId,
    height: StableScalarSource<'a>,
    secondary: Option<StableScalarSource<'a>>,
}

impl<'a> StablePhaseSource<'a> {
    /// Construct a height-only phase source.
    pub const fn new(phase: StablePhaseId, height: StableScalarSource<'a>) -> Self {
        Self {
            phase,
            height,
            secondary: None,
        }
    }

    /// Attach the phase's topology-compatible secondary scalar source.
    pub const fn with_secondary(mut self, secondary: StableScalarSource<'a>) -> Self {
        self.secondary = Some(secondary);
        self
    }

    /// Return this phase's stable identifier.
    pub const fn phase(&self) -> StablePhaseId {
        self.phase
    }

    /// Return the required height source.
    pub const fn height(&self) -> StableScalarSource<'a> {
        self.height
    }

    /// Return the optional secondary source.
    pub const fn secondary(&self) -> Option<StableScalarSource<'a>> {
        self.secondary
    }
}

#[derive(Clone, Copy)]
pub(crate) enum SourceGeometryKey<'a> {
    Regular(usize, core::marker::PhantomData<&'a ()>),
    #[cfg(feature = "irregular-delaunay")]
    Irregular(&'a crate::IrregularTernaryMesh),
}

impl<'a> SourceGeometryKey<'a> {
    pub(crate) const fn regular(subdivisions: usize) -> Self {
        Self::Regular(subdivisions, core::marker::PhantomData)
    }

    pub(crate) fn matches(self, other: Self) -> bool {
        match (self, other) {
            (Self::Regular(left, _), Self::Regular(right, _)) => left == right,
            #[cfg(feature = "irregular-delaunay")]
            (Self::Irregular(left), Self::Irregular(right)) => left.has_same_identity(right),
            #[cfg(feature = "irregular-delaunay")]
            _ => false,
        }
    }

    pub(crate) const fn is_regular(self) -> bool {
        matches!(self, Self::Regular(_, _))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScalarRole {
    Height,
    Secondary,
}
