//! Derived pointwise quantities reusing prepared field evaluators.

use crate::{FieldEvaluationError, InterpolatedTernaryField, LocatedTriangle};

use super::TernaryGradient;

/// A scalar quantity derived from a prepared ternary field sample.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum DerivedFieldQuantity {
    #[default]
    Value,
    GradientReducedA,
    GradientReducedB,
    GradientLogicalX,
    GradientLogicalY,
    GradientNorm,
}

impl DerivedFieldQuantity {
    fn select(self, value: f64, reduced_ab: [f64; 2]) -> Option<f64> {
        let gradient = TernaryGradient::from_reduced_ab(reduced_ab);
        let result = match self {
            Self::Value => value,
            Self::GradientReducedA => reduced_ab[0],
            Self::GradientReducedB => reduced_ab[1],
            Self::GradientLogicalX => gradient.logical_xy()[0],
            Self::GradientLogicalY => gradient.logical_xy()[1],
            Self::GradientNorm => gradient.norm(),
        };
        result.is_finite().then_some(result)
    }
}

/// One derived scalar sample retaining the source field's deterministic location.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DerivedFieldSample<Location> {
    pub value: f64,
    pub location: Location,
}

/// A prepared derived quantity over a regular ternary field.
///
/// This adapter never rebuilds interpolation intervals or repeats point location
/// when [`Self::evaluate_at_location`] is used.
pub struct DerivedRegularTernaryField<'e, 'f> {
    evaluator: &'e InterpolatedTernaryField<'f>,
    quantity: DerivedFieldQuantity,
}

impl<'e, 'f> DerivedRegularTernaryField<'e, 'f> {
    pub const fn new(
        evaluator: &'e InterpolatedTernaryField<'f>,
        quantity: DerivedFieldQuantity,
    ) -> Self {
        Self {
            evaluator,
            quantity,
        }
    }

    pub const fn quantity(&self) -> DerivedFieldQuantity {
        self.quantity
    }

    pub fn value(&self, composition: [f64; 3]) -> Result<f64, FieldEvaluationError> {
        Ok(self.evaluate(composition)?.value)
    }

    pub fn evaluate(
        &self,
        composition: [f64; 3],
    ) -> Result<DerivedFieldSample<LocatedTriangle>, FieldEvaluationError> {
        let sample = self.evaluator.evaluate(composition)?;
        Ok(DerivedFieldSample {
            value: self
                .quantity
                .select(sample.value, sample.gradient_ab)
                .ok_or(FieldEvaluationError::NonFiniteEvaluation)?,
            location: sample.location,
        })
    }

    pub fn value_at_location(
        &self,
        location: &LocatedTriangle,
    ) -> Result<f64, FieldEvaluationError> {
        Ok(self.evaluate_at_location(location)?.value)
    }

    pub fn evaluate_at_location(
        &self,
        location: &LocatedTriangle,
    ) -> Result<DerivedFieldSample<LocatedTriangle>, FieldEvaluationError> {
        let sample = self.evaluator.evaluate_at_location(location)?;
        Ok(DerivedFieldSample {
            value: self
                .quantity
                .select(sample.value, sample.gradient_ab)
                .ok_or(FieldEvaluationError::NonFiniteEvaluation)?,
            location: sample.location,
        })
    }

    pub fn values<'a, I>(
        &'a self,
        compositions: I,
    ) -> impl Iterator<Item = Result<f64, FieldEvaluationError>> + 'a
    where
        I: IntoIterator<Item = [f64; 3]>,
        I::IntoIter: 'a,
    {
        compositions
            .into_iter()
            .map(|composition| self.value(composition))
    }

    pub fn values_into(
        &self,
        compositions: &[[f64; 3]],
        output: &mut [f64],
    ) -> Result<(), FieldEvaluationError> {
        if compositions.len() != output.len() {
            return Err(FieldEvaluationError::OutputSizeMismatch {
                expected: compositions.len(),
                actual: output.len(),
            });
        }
        for (composition, value) in compositions.iter().copied().zip(output) {
            *value = self.value(composition)?;
        }
        Ok(())
    }
}

#[cfg(feature = "irregular-delaunay")]
use crate::{
    InterpolatedIrregularTernaryField, IrregularFieldEvaluationError, LocatedIrregularTriangle,
};

/// A prepared derived quantity over an irregular ternary field.
#[cfg(feature = "irregular-delaunay")]
pub struct DerivedIrregularTernaryField<'e, 'f> {
    evaluator: &'e InterpolatedIrregularTernaryField<'f>,
    quantity: DerivedFieldQuantity,
}

#[cfg(feature = "irregular-delaunay")]
impl<'e, 'f> DerivedIrregularTernaryField<'e, 'f> {
    pub const fn new(
        evaluator: &'e InterpolatedIrregularTernaryField<'f>,
        quantity: DerivedFieldQuantity,
    ) -> Self {
        Self {
            evaluator,
            quantity,
        }
    }

    pub const fn quantity(&self) -> DerivedFieldQuantity {
        self.quantity
    }

    pub fn value(&self, composition: [f64; 3]) -> Result<f64, IrregularFieldEvaluationError> {
        Ok(self.evaluate(composition)?.value)
    }

    pub fn evaluate(
        &self,
        composition: [f64; 3],
    ) -> Result<DerivedFieldSample<LocatedIrregularTriangle>, IrregularFieldEvaluationError> {
        let sample = self.evaluator.evaluate(composition)?;
        Ok(DerivedFieldSample {
            value: self
                .quantity
                .select(sample.value, sample.gradient_ab)
                .ok_or(IrregularFieldEvaluationError::NonFiniteEvaluation)?,
            location: sample.location,
        })
    }

    pub fn value_at_location(
        &self,
        location: &LocatedIrregularTriangle,
    ) -> Result<f64, IrregularFieldEvaluationError> {
        Ok(self.evaluate_at_location(location)?.value)
    }

    pub fn evaluate_at_location(
        &self,
        location: &LocatedIrregularTriangle,
    ) -> Result<DerivedFieldSample<LocatedIrregularTriangle>, IrregularFieldEvaluationError> {
        let sample = self.evaluator.evaluate_at_location(location)?;
        Ok(DerivedFieldSample {
            value: self
                .quantity
                .select(sample.value, sample.gradient_ab)
                .ok_or(IrregularFieldEvaluationError::NonFiniteEvaluation)?,
            location: sample.location,
        })
    }

    pub fn values<'a, I>(
        &'a self,
        compositions: I,
    ) -> impl Iterator<Item = Result<f64, IrregularFieldEvaluationError>> + 'a
    where
        I: IntoIterator<Item = [f64; 3]>,
        I::IntoIter: 'a,
    {
        compositions
            .into_iter()
            .map(|composition| self.value(composition))
    }

    pub fn values_into(
        &self,
        compositions: &[[f64; 3]],
        output: &mut [f64],
    ) -> Result<(), IrregularFieldEvaluationError> {
        if compositions.len() != output.len() {
            return Err(IrregularFieldEvaluationError::OutputSizeMismatch {
                expected: compositions.len(),
                actual: output.len(),
            });
        }
        for (composition, value) in compositions.iter().copied().zip(output) {
            *value = self.value(composition)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FieldInterpolation, RegularTernaryScalarField};

    #[test]
    fn regular_derived_scalar_and_batch_reuse_prepared_gradient() {
        let field =
            RegularTernaryScalarField::from_fn(4, |[a, b, c]| 2.0 * a - 3.0 * b + c).unwrap();
        let evaluator = InterpolatedTernaryField::new(&field, FieldInterpolation::Linear).unwrap();
        let derived = evaluator.derived(DerivedFieldQuantity::GradientNorm);
        let point = [0.2, 0.3, 0.5];
        let expected = evaluator.evaluate(point).unwrap().gradient_norm();
        assert!((derived.value(point).unwrap() - expected).abs() < 1.0e-14);
        let points = [point, [0.4, 0.1, 0.5]];
        let lazy = derived
            .values(points)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let mut output = vec![0.0; points.len()];
        derived.values_into(&points, &mut output).unwrap();
        assert_eq!(lazy, output);
    }

    #[cfg(feature = "irregular-delaunay")]
    #[test]
    fn irregular_derived_scalar_and_batch_reuse_prepared_gradient() {
        use crate::{
            InterpolatedIrregularTernaryField, IrregularFieldInterpolation, IrregularTernaryMesh,
            IrregularTernaryScalarField,
        };
        let mesh = IrregularTernaryMesh::new([
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.57, 0.28, 0.15],
            [0.18, 0.61, 0.21],
            [0.23, 0.16, 0.61],
            [0.31, 0.42, 0.27],
        ])
        .unwrap();
        let field =
            IrregularTernaryScalarField::from_fn(mesh, |[a, b, c]| 2.0 * a - 3.0 * b + c).unwrap();
        let evaluator =
            InterpolatedIrregularTernaryField::new(&field, IrregularFieldInterpolation::Linear)
                .unwrap();
        let derived = evaluator.derived(DerivedFieldQuantity::GradientLogicalX);
        let point = [0.23, 0.31, 0.46];
        let expected = evaluator.evaluate(point).unwrap().gradient_logical_xy()[0];
        assert!((derived.value(point).unwrap() - expected).abs() < 1.0e-14);
        let points = [point, [0.31, 0.34, 0.35]];
        let lazy = derived
            .values(points)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let mut output = vec![0.0; points.len()];
        derived.values_into(&points, &mut output).unwrap();
        assert_eq!(lazy, output);
    }
}
