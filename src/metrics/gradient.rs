//! Invariant gradient coordinates for the two-dimensional ternary simplex.

/// A finite or user-constructed gradient on the ternary composition plane.
///
/// `reduced_ab` stores the existing reduced semantic derivatives with
/// `c = 1-a-b`. `logical_xy` and [`Self::norm`] use the canonical equilateral
/// logical plane `A=(0,0)`, `B=(1,0)`, `C=(1/2,sqrt(3)/2)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TernaryGradient {
    reduced_ab: [f64; 2],
}

impl TernaryGradient {
    /// Construct from reduced semantic derivatives `(df/da, df/db)`.
    ///
    /// Field evaluators construct this value only after rejecting non-finite
    /// derived gradients. [`Self::is_finite`] is provided for callers creating
    /// a gradient directly from external data.
    pub const fn from_reduced_ab(reduced_ab: [f64; 2]) -> Self {
        Self { reduced_ab }
    }

    /// Return reduced semantic derivatives with `c=1-a-b`.
    pub const fn reduced_ab(self) -> [f64; 2] {
        self.reduced_ab
    }

    /// Construct from a gradient in canonical logical `(x, y)` coordinates.
    pub fn from_logical_xy([g_x, g_y]: [f64; 2]) -> Self {
        let sum = -3.0_f64.sqrt() * g_y;
        Self::from_reduced_ab([(sum - g_x) / 2.0, (sum + g_x) / 2.0])
    }

    /// Return the gradient in canonical logical `(x,y)` coordinates.
    ///
    /// For reduced derivatives `[g_a, g_b]`, this is
    /// `[g_b-g_a, -(g_a+g_b)/sqrt(3)]`.
    pub fn logical_xy(self) -> [f64; 2] {
        let [g_a, g_b] = self.reduced_ab;
        [g_b - g_a, -(g_a + g_b) / 3.0_f64.sqrt()]
    }

    /// Return the invariant gradient norm per unit canonical logical distance.
    ///
    /// This is `sqrt((4/3) * (g_a² - g_a*g_b + g_b²))`, not
    /// `sqrt(g_a² + g_b²)`.
    pub fn norm(self) -> f64 {
        let [g_a, g_b] = self.reduced_ab;
        ((4.0 / 3.0) * (g_a.mul_add(g_a, g_b.mul_add(g_b, -g_a * g_b)))).sqrt()
    }

    /// Return a unit logical direction, or `[0,0]` for a zero gradient.
    pub fn direction(self) -> [f64; 2] {
        self.direction_if_nonzero().unwrap_or([0.0, 0.0])
    }

    /// Return a unit logical direction when the gradient has nonzero finite norm.
    pub fn direction_if_nonzero(self) -> Option<[f64; 2]> {
        let logical = self.logical_xy();
        let norm = self.norm();
        (norm.is_finite() && norm > 0.0 && logical.into_iter().all(f64::is_finite))
            .then_some([logical[0] / norm, logical[1] / norm])
    }

    /// Return the derivative along an arbitrary logical-plane direction vector.
    ///
    /// Pass a unit vector to obtain a directional derivative per unit logical
    /// distance. A non-finite direction returns `None`.
    pub fn directional_derivative(self, direction_xy: [f64; 2]) -> Option<f64> {
        if !direction_xy.into_iter().all(f64::is_finite) {
            return None;
        }
        let logical = self.logical_xy();
        let derivative = logical[0].mul_add(direction_xy[0], logical[1] * direction_xy[1]);
        derivative.is_finite().then_some(derivative)
    }

    /// Whether both reduced and derived logical components are finite.
    pub fn is_finite(self) -> bool {
        self.reduced_ab.into_iter().all(f64::is_finite)
            && self.logical_xy().into_iter().all(f64::is_finite)
            && self.norm().is_finite()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduced_and_logical_gradient_formulae_agree() {
        let gradient = TernaryGradient::from_reduced_ab([2.0, -1.0]);
        assert_eq!(gradient.logical_xy(), [-3.0, -1.0 / 3.0_f64.sqrt()]);
        let expected = ((4.0_f64 / 3.0) * (4.0 + 2.0 + 1.0)).sqrt();
        assert!((gradient.norm() - expected).abs() < 1.0e-14);
        assert_eq!(gradient.direction().len(), 2);
        assert!(gradient.direction_if_nonzero().is_some());
    }

    #[test]
    fn zero_gradient_has_explicit_direction_semantics() {
        let gradient = TernaryGradient::from_reduced_ab([0.0, 0.0]);
        assert_eq!(gradient.direction(), [0.0, 0.0]);
        assert_eq!(gradient.direction_if_nonzero(), None);
        assert_eq!(gradient.directional_derivative([1.0, 0.0]), Some(0.0));
    }
}
