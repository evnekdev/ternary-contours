//! Evaluate a regular ternary scalar field at arbitrary compositions.

use ternary_contours::{FieldInterpolation, InterpolatedTernaryField, RegularTernaryScalarField};

#[cfg(feature = "cubic-alpha")]
use ternary_contours::{
    BinaryExtrapolation, CubicAlphaBuildOptions, CubicAlphaMethod, CubicBoundaryPolicy,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let field = RegularTernaryScalarField::from_fn(12, |[a, b, c]| {
        2.0 * a - 3.0 * b + 5.0 * c + 0.4 * a * b
    })?;

    let linear = InterpolatedTernaryField::new(&field, FieldInterpolation::Linear)?;
    let point = [0.23, 0.31, 0.46];
    let sample = linear.evaluate(point)?;
    println!("linear value: {}", sample.value);
    println!("global (a, b) gradient: {:?}", sample.gradient_ab);
    println!("triangle: {:?}", sample.location.triangle);
    println!("barycentric: {:?}", sample.location.barycentric);

    let points = [[0.2, 0.3, 0.5], [0.5, 0.25, 0.25], [0.0, 1.0, 0.0]];
    let values: Result<Vec<_>, _> = linear.values(points).collect();
    println!("linear batch: {:?}", values?);

    #[cfg(feature = "cubic-alpha")]
    {
        let cubic = InterpolatedTernaryField::new(
            &field,
            FieldInterpolation::CubicAlpha(CubicAlphaBuildOptions {
                method: CubicAlphaMethod::Pchip,
                boundary_policy: CubicBoundaryPolicy::LinearFallback,
                partial_domain_policy: Default::default(),
                // Muggianu and Kohler are interior continuation policies within
                // this one cubic-alpha interpolation family.
                extrapolation: BinaryExtrapolation::Kohler,
            }),
        )?;
        let sample = cubic.evaluate(point)?;
        println!("PCHIP/Kohler cubic value: {}", sample.value);
        println!("PCHIP/Kohler gradient: {:?}", sample.gradient_ab);

        let mut output = vec![0.0; points.len()];
        cubic.values_into(&points, &mut output)?;
        println!("cubic batch: {output:?}");
    }

    #[cfg(not(feature = "cubic-alpha"))]
    println!("enable `cubic-alpha` to run the PCHIP/Kohler cubic example");

    Ok(())
}
