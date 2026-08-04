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

/// How cubic-alpha construction handles local partial-domain stencils.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum CubicPartialDomainPolicy {
    /// Reject any undefined triangle or unavailable local stencil.
    Strict,
    /// Use one-sided cubic intervals, then leave insufficient intervals linear.
    #[default]
    OneSidedThenLinear,
    /// Use one-sided cubic intervals, but reject when those are unavailable.
    OneSided,
    /// Use linear intervals at every local domain boundary.
    LinearNearDomain,
}

/// Backend-independent cubic interval construction policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicAlphaBuildOptions {
    pub method: CubicAlphaMethod,
    pub boundary_policy: CubicBoundaryPolicy,
    pub partial_domain_policy: CubicPartialDomainPolicy,
    pub extrapolation: super::BinaryExtrapolation,
}
impl Default for CubicAlphaBuildOptions {
    fn default() -> Self {
        Self {
            method: CubicAlphaMethod::Steffen,
            boundary_policy: CubicBoundaryPolicy::LinearFallback,
            partial_domain_policy: CubicPartialDomainPolicy::OneSidedThenLinear,
            extrapolation: super::BinaryExtrapolation::Muggianu,
        }
    }
}

