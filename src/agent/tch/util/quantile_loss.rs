//! Quantile loss.
use tch::Tensor;

/// Returns the quantile huber loss.
///
/// The first dimension of `x` corresponds to quantiles `tau`.
pub fn quantile_huber_loss(x: &Tensor, tau: &Tensor) -> Tensor {
    debug_assert_eq!(x.size().as_slice(), tau.size().as_slice());

    // let x_abs = &x.abs();
    // let x_abs_gt_1 = &x_abs.gt(1.0);
    let lt_0 = &x.lt(0.0);
    // let loss = (x_abs - 0.5).where1(x_abs_gt_1, &(0.5 * x.pow(2.0)));
    let loss = x.smooth_l1_loss(&Tensor::zeros_like(x), tch::Reduction::None, 1.0);
    (tau - Tensor::where4(lt_0, 1., 0.)).abs() * loss
}
