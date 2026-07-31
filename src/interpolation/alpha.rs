/// Normalized endpoint coefficients for one directed cubic interval.
///
/// The interval value is
/// `y0*(1-t) + y1*t + (1-t)*t*(alpha0 + alpha1*t)`, with `t=0` at
/// the first endpoint and `t=1` at the second.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AlphaInterval {
    pub alpha0: f64,
    pub alpha1: f64,
}

impl AlphaInterval {
    pub const fn new(alpha0: f64, alpha1: f64) -> Self {
        Self { alpha0, alpha1 }
    }

    /// Evaluate the normalized directed interval.
    pub fn value(self, y0: f64, y1: f64, t: f64) -> f64 {
        y0 * (1.0 - t) + y1 * t + (1.0 - t) * t * (self.alpha0 + self.alpha1 * t)
    }

    /// Reverse the directed interval without changing its geometric curve.
    ///
    /// `(alpha0, alpha1) -> (alpha0 + alpha1, -alpha1)`.
    pub const fn reversed(self) -> Self {
        Self {
            alpha0: self.alpha0 + self.alpha1,
            alpha1: -self.alpha1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(left: f64, right: f64) {
        assert!((left - right).abs() < 1.0e-11, "{left} != {right}");
    }

    #[test]
    fn asymmetric_polynomial_proves_alpha1_multiplies_t() {
        let interval = AlphaInterval::new(2.5, -4.0);
        for t in [0.0, 0.1, 0.25, 0.5, 0.8, 1.0] {
            let expected = 1.25 * (1.0 - t) - 0.75 * t + (1.0 - t) * t * (2.5 - 4.0 * t);
            close(interval.value(1.25, -0.75, t), expected);
            if t > 0.0 && t < 1.0 && (t - 0.5_f64).abs() > 1.0e-12 {
                let wrong = 1.25 * (1.0 - t) - 0.75 * t + (1.0 - t) * t * (2.5 - 4.0 * (1.0 - t));
                assert!((expected - wrong).abs() > 1.0e-4);
            }
        }
        assert_eq!(interval.value(1.25, -0.75, 0.0), 1.25);
        assert_eq!(interval.value(1.25, -0.75, 1.0), -0.75);
    }

    #[test]
    fn reversal_identity_holds() {
        let interval = AlphaInterval::new(2.5, -4.0);
        let reversed = interval.reversed();
        assert_eq!(reversed, AlphaInterval::new(-1.5, 4.0));
        for t in [0.0, 0.1, 0.25, 0.5, 0.8, 1.0] {
            close(
                interval.value(1.25, -0.75, t),
                reversed.value(-0.75, 1.25, 1.0 - t),
            );
        }
    }

    #[cfg(feature = "cubic-alpha")]
    #[test]
    fn direct_coefficient_round_trip_on_non_unit_interval() {
        let coefficients = [0.375, -1.25, 2.75, -0.5];
        let x0 = 2.0;
        let x1 = 5.0;
        let h = x1 - x0;
        let eval = |dx: f64| {
            ((coefficients[0] * dx + coefficients[1]) * dx + coefficients[2]) * dx + coefficients[3]
        };
        let y0 = eval(0.0);
        let y1 = eval(h);
        let alpha = spline1d::cubic_coeffs_to_alpha(coefficients, h);
        let interval = AlphaInterval::new(alpha[0], alpha[1]);
        let roundtrip =
            spline1d::alpha_to_cubic_coeffs(x0, y0, x1, y1, interval.alpha0, interval.alpha1);
        for (actual, expected) in roundtrip.into_iter().zip(coefficients) {
            close(actual, expected);
        }
        for t in [0.0, 0.13, 0.37, 0.71, 1.0] {
            close(interval.value(y0, y1, t), eval(t * h));
        }
    }
}
