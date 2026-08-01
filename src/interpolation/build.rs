//! Shared one-dimensional cubic-alpha construction helpers.

#[cfg(feature = "irregular-cubic-alpha")]
use super::AlphaInterval;
#[cfg(feature = "cubic-alpha")]
use super::CubicAlphaMethod;

#[cfg(feature = "irregular-cubic-alpha")]
pub(crate) fn alpha_from_uniform_four_values(
    method: CubicAlphaMethod,
    values: [f64; 4],
) -> AlphaInterval {
    use spline1d::cubic_single_middle_alpha;

    let alpha = cubic_single_middle_alpha(
        cubic_method_kind(method),
        -1.0,
        values[0],
        0.0,
        values[1],
        1.0,
        values[2],
        2.0,
        values[3],
    );
    AlphaInterval::new(alpha[0], alpha[1])
}

#[cfg(feature = "cubic-alpha")]
pub(crate) fn cubic_method_kind(method: CubicAlphaMethod) -> spline1d::InterpolationType<f64> {
    match method {
        CubicAlphaMethod::Akima => spline1d::InterpolationType::AKIMA,
        CubicAlphaMethod::Makima => spline1d::InterpolationType::MAKIMA,
        CubicAlphaMethod::Pchip => spline1d::InterpolationType::PCHIP,
        CubicAlphaMethod::Steffen => spline1d::InterpolationType::STEFFEN,
    }
}
