#[cfg(feature = "cubic-alpha")]
use crate::RegularTernaryPartialScalarField;
use crate::{FieldInterpolation, RegularTernaryScalarField};
#[cfg(feature = "irregular-delaunay")]
use crate::{IrregularFieldInterpolation, IrregularTernaryScalarField};

/// Stable user-defined identifier for one phase in an ensemble.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StablePhaseId(pub u32);
/// Expected outcome of evaluating one phase-specific scalar field.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum StablePhaseEvaluation {
    /// The phase field is defined at the requested composition.
    Defined { value: f64 },
    /// The phase field is intentionally unavailable at this composition.
    Undefined { reason: StablePhaseUndefinedReason },
}

/// Why a phase-specific scalar field is undefined at one composition.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum StablePhaseUndefinedReason {
    TargetSearchDidNotConverge,
    NoSinglePhaseLiquidus,
    OutsidePhaseDomain,
    NonFiniteResult,
    SourceEvaluationFailure,
    /// The table explicitly classifies this phase as non-existing.
    ClassifiedNonExisting,
    /// The table explicitly records a high-temperature calculation cut-off.
    ClassifiedCutOff,
    /// The table has no calculated or classified scalar for this point.
    MissingTabulatedInput,
}

/// Black-box phase evaluator supporting explicit partial-domain semantics.
pub trait StablePhaseEvaluator {
    /// Evaluate one normalized semantic A/B/C composition.
    fn evaluate(&self, composition: [f64; 3]) -> StablePhaseEvaluation;
}

impl<F> StablePhaseEvaluator for F
where
    F: Fn([f64; 3]) -> StablePhaseEvaluation,
{
    fn evaluate(&self, composition: [f64; 3]) -> StablePhaseEvaluation {
        self(composition)
    }
}

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
#[derive(Clone, Copy)]
#[non_exhaustive]
pub enum StableScalarSource<'a> {
    /// A regular scalar field with linear or optional cubic-alpha sampling.
    Regular {
        /// Sampled regular source field.
        field: &'a RegularTernaryScalarField,
        /// Interpolation used while resampling.
        interpolation: FieldInterpolation,
    },
    /// A regular source with unavailable vertices and local cubic fallbacks.
    #[cfg(feature = "cubic-alpha")]
    PartialRegular {
        /// Partial regular source field.
        field: &'a RegularTernaryPartialScalarField,
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
    /// Direct evaluator with explicit defined/undefined semantics.
    Evaluator {
        /// Borrowed deterministic evaluator.
        evaluator: &'a dyn StablePhaseEvaluator,
    },
}

impl core::fmt::Debug for StableScalarSource<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Regular {
                field,
                interpolation,
            } => formatter
                .debug_struct("Regular")
                .field("subdivisions", &field.subdivisions())
                .field("interpolation", interpolation)
                .finish(),
            #[cfg(feature = "cubic-alpha")]
            Self::PartialRegular {
                field,
                interpolation,
            } => formatter
                .debug_struct("PartialRegular")
                .field("subdivisions", &field.subdivisions())
                .field("interpolation", interpolation)
                .finish(),
            #[cfg(feature = "irregular-delaunay")]
            Self::Irregular {
                field,
                interpolation,
            } => formatter
                .debug_struct("Irregular")
                .field("vertex_count", &field.mesh().vertex_count())
                .field("interpolation", interpolation)
                .finish(),
            Self::Evaluator { .. } => formatter.write_str("Evaluator(..)"),
        }
    }
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

    /// Construct a regular source with unavailable vertices.
    #[cfg(feature = "cubic-alpha")]
    pub const fn partial_regular(
        field: &'a RegularTernaryPartialScalarField,
        interpolation: FieldInterpolation,
    ) -> Self {
        Self::PartialRegular {
            field,
            interpolation,
        }
    }

    /// Construct a direct partial-domain evaluator source.
    pub const fn evaluator(evaluator: &'a dyn StablePhaseEvaluator) -> Self {
        Self::Evaluator { evaluator }
    }

    pub(crate) fn geometry_key(self) -> SourceGeometryKey<'a> {
        match self {
            Self::Regular { field, .. } => SourceGeometryKey::regular(field.subdivisions()),
            #[cfg(feature = "cubic-alpha")]
            Self::PartialRegular { field, .. } => {
                SourceGeometryKey::PartialRegular(field.subdivisions(), core::marker::PhantomData)
            }
            #[cfg(feature = "irregular-delaunay")]
            Self::Irregular { field, .. } => SourceGeometryKey::Irregular(field.mesh()),
            Self::Evaluator { evaluator } => SourceGeometryKey::Evaluator(
                evaluator as *const dyn StablePhaseEvaluator as *const () as usize,
                core::marker::PhantomData,
            ),
        }
    }

    pub(crate) fn has_same_topology(self, other: Self) -> bool {
        match (self, other) {
            (Self::Regular { field: left, .. }, Self::Regular { field: right, .. }) => {
                left.subdivisions() == right.subdivisions()
            }
            #[cfg(feature = "cubic-alpha")]
            (
                Self::PartialRegular { field: left, .. },
                Self::PartialRegular { field: right, .. },
            ) => left.subdivisions() == right.subdivisions(),
            #[cfg(feature = "irregular-delaunay")]
            (Self::Irregular { field: left, .. }, Self::Irregular { field: right, .. }) => {
                left.mesh().has_same_identity(right.mesh())
            }
            (Self::Evaluator { evaluator: left }, Self::Evaluator { evaluator: right }) => {
                core::ptr::eq(left, right)
            }
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
    #[cfg(feature = "cubic-alpha")]
    PartialRegular(usize, core::marker::PhantomData<&'a ()>),
    #[cfg(feature = "irregular-delaunay")]
    Irregular(&'a crate::IrregularTernaryMesh),
    Evaluator(usize, core::marker::PhantomData<&'a ()>),
}

impl<'a> SourceGeometryKey<'a> {
    pub(crate) const fn regular(subdivisions: usize) -> Self {
        Self::Regular(subdivisions, core::marker::PhantomData)
    }

    pub(crate) fn matches(self, other: Self) -> bool {
        match (self, other) {
            (Self::Regular(left, _), Self::Regular(right, _)) => left == right,
            #[cfg(feature = "cubic-alpha")]
            (Self::PartialRegular(left, _), Self::PartialRegular(right, _)) => left == right,
            #[cfg(feature = "irregular-delaunay")]
            (Self::Irregular(left), Self::Irregular(right)) => left.has_same_identity(right),
            (Self::Evaluator(left, _), Self::Evaluator(right, _)) => left == right,
            _ => false,
        }
    }

    pub(crate) const fn is_regular(self) -> bool {
        matches!(self, Self::Regular(_, _)) || {
            #[cfg(feature = "cubic-alpha")]
            {
                matches!(self, Self::PartialRegular(_, _))
            }
            #[cfg(not(feature = "cubic-alpha"))]
            {
                false
            }
        }
    }

    pub(crate) const fn is_irregular(self) -> bool {
        #[cfg(feature = "irregular-delaunay")]
        {
            matches!(self, Self::Irregular(_))
        }
        #[cfg(not(feature = "irregular-delaunay"))]
        {
            false
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScalarRole {
    Height,
    Secondary,
}
