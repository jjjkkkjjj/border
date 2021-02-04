use std::marker::PhantomData;
use log::trace;
use ndarray::{Array1, IxDyn};
use tch::Tensor;
use crate::agents::tch::{Shape, TchBuffer, util::try_from, util::concat_slices};
use crate::py_gym_env::act_c::{PyGymEnvContinuousAct, PyGymEnvContinuousActFilter};

impl<S: Shape, F: PyGymEnvContinuousActFilter> From<Tensor> for PyGymEnvContinuousAct<S, F> {
    /// The first dimension is the number of environments.
    fn from(t: Tensor) -> Self {
        trace!("TchPyGymEnvContinuousAct from Tensor: {:?}", t);
        let shape = t.size().iter().map(|x| *x as usize).collect::<Vec<_>>();
        let act: Vec<f32> = t.into();
        Self {
            act: Array1::<f32>::from(act).into_shape(IxDyn(&shape)).unwrap(),
            phantom: PhantomData,
        }
    }
}

pub struct TchPyGymEnvContinuousActBuffer<S: Shape, F: PyGymEnvContinuousActFilter> {
    act: Tensor,
    n_procs: i64,
    phantom: PhantomData<(S, F)>
}

impl<S: Shape, F: PyGymEnvContinuousActFilter> TchBuffer for TchPyGymEnvContinuousActBuffer<S, F> {
    type Item = PyGymEnvContinuousAct<S, F>;
    type SubBatch = Tensor;

    fn new(capacity: usize, n_procs: usize) -> Self {
        let capacity = capacity as _;
        let n_procs = n_procs as _;
        let shape = concat_slices(&[capacity, n_procs],
            S::shape().iter().map(|v| *v as i64).collect::<Vec<_>>().as_slice());
        Self {
            act: Tensor::zeros(&shape, tch::kind::FLOAT_CPU),
            n_procs,
            phantom: PhantomData,
        }
    }

    fn push(&mut self, index: i64, item: &Self::Item) {
        let act: Tensor = try_from(item.act.clone()).unwrap();
        debug_assert_eq!(act.size().as_slice(), &self.act.size()[1..]);
        self.act.get(index).copy_(&act);
    }

    /// Create minibatch.
    /// The second axis is squeezed, thus the batch size is
    /// `batch_indexes.len()` times `n_procs`.
    fn batch(&self, batch_indexes: &Tensor) -> Tensor {
        let batch = self.act.index_select(0, &batch_indexes);
        let batch = batch.flatten(0, 1);
        debug_assert_eq!(batch.size().as_slice()[0], batch_indexes.size()[0] * self.n_procs);
        batch
    }
}
