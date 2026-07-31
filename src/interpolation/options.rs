/// Shape-preserving one-dimensional method used to derive cubic-alpha intervals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CubicAlphaMethod {
    Akima,
    Makima,
    Pchip,
    Steffen,
}

/// Behaviour when a regular-lattice line has only two samples.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum CubicBoundaryPolicy {
    /// Use a zero-alpha (linear) interval.
    #[default]
    LinearFallback,
    /// Reject construction because no cubic stencil is available.
    Error,
}

/// Backend-independent cubic interval construction policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicAlphaBuildOptions {
    pub method: CubicAlphaMethod,
    pub boundary_policy: CubicBoundaryPolicy,
    pub extrapolation: super::BinaryExtrapolation,
}
impl Default for CubicAlphaBuildOptions {
    fn default() -> Self {
        Self {
            method: CubicAlphaMethod::Steffen,
            boundary_policy: CubicBoundaryPolicy::LinearFallback,
            extrapolation: super::BinaryExtrapolation::Muggianu,
        }
    }
}
