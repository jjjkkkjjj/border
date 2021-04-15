//! Utilities used by tch agents.
use log::trace;
use tch::{Tensor, nn};

use crate::agent::tch::model::ModelBase;

pub mod quantile_loss;
pub use quantile_loss::quantile_huber_loss;

/// Apply soft update on a model.
///
/// Variables are identified by their names.
pub fn track<M: ModelBase>(dest: &mut M, src: &mut M, tau: f64) {
    let src = &mut src.get_var_store().variables();
    let dest = &mut dest.get_var_store().variables();
    debug_assert_eq!(src.len(), dest.len());

    let names = src.keys();
    tch::no_grad(|| {
        for name in names {
            let src = src.get(name).unwrap();
            let dest = dest.get_mut(name).unwrap();
            dest.copy_(&(tau * src + (1.0 - tau) * &*dest));
        }
    });
    trace!("soft update");
}

/// Concatenates slices.
pub fn concat_slices(s1: &[i64], s2: &[i64]) -> Vec<i64> {
    let mut v = Vec::from(s1);
    v.append(&mut Vec::from(s2));
    v
}

/// Builds feature extractor
pub trait FeatureExtractorBuilder {
    /// [FeatureExtractor] constructed by this builder.
    type F: FeatureExtractor;

    /// Constructs [FeatureExtractor].
    fn build(self, p: &nn::Path) -> Self::F;
}

/// Feature extractor that output [tch::Tensor].
pub trait FeatureExtractor {
    /// Input of the model.
    type Input;

    /// Convert the input to a feature vector.
    fn feature(&self, x: &Self::Input) -> Tensor;

    /// Clone [FeatureExtractor] with [`tch::nn::VarStore`].
    fn clone_with_var_store(&self, var_store: &nn::VarStore) -> Self;
}
