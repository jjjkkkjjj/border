use std::marker::PhantomData;
use log::trace;
use pyo3::{PyObject};
use ndarray::{ArrayD, Axis, IxDyn};
use numpy::PyArrayDyn;
use tch::Tensor;
use crate::core::Obs;
use crate::agents::tch::{Shape, TchBuffer, util::try_from, util::concat_slices};

fn any(is_done: &[f32]) -> bool {
    is_done.iter().fold(0, |x, v| x + *v as i32) > 0
}

/// Represents observation.
/// Currently, it supports 1-dimentional vector only.
#[derive(Clone, Debug)]
pub struct TchPyGymEnvObs<S: Shape, D> where
    D: Clone + std::fmt::Debug
{
    obs: ArrayD<f32>,
    phantom: PhantomData<(S, D)>
}

impl<S: Shape, D> Obs for TchPyGymEnvObs<S, D> where
    D: Clone + std::fmt::Debug
{
    fn zero(n_procs: usize) -> Self {
        let shape = &mut S::shape().to_vec();
        shape.insert(0, n_procs as _);
        trace!("Shape of TchPyGymEnvObs: {:?}", shape);
        Self {
            obs: ArrayD::zeros(IxDyn(&shape[..])),
            phantom: PhantomData
        }
    }

    fn merge(mut self, obs_reset: Self, is_done: &[f32]) -> Self {
        if any(is_done) {
            for (i, is_done_i) in is_done.iter().enumerate() {
                if *is_done_i != 0.0 as f32 {
                    self.obs.index_axis_mut(Axis(0), i)
                        .assign(&obs_reset.obs.index_axis(Axis(0), i));
                }
            }
        };
        self
    }
}

impl<S: Shape> From<PyObject> for TchPyGymEnvObs<S, f32>
{
    fn from(obs: PyObject) -> Self {
        pyo3::Python::with_gil(|py| {
            // let obs: &PyArrayDyn<f64> = obs.extract(py).unwrap();
            let obs: &PyArrayDyn<f32> = obs.extract(py).unwrap();
            let obs = obs.to_owned_array();
            let obs = obs.mapv(|elem| elem as f32);
            let obs = {
                if obs.shape().len() == S::shape().len() + 1 {
                    // In this case obs has a dimension for n_procs
                    obs
                }
                else if obs.shape().len() == S::shape().len() {
                    // add dimension for n_procs
                    obs.insert_axis(Axis(0))
                }
                else {
                    panic!();
                }
            };
            Self {
                obs,
                phantom: PhantomData,
            }
        })
    }
}

impl<S: Shape> From<PyObject> for TchPyGymEnvObs<S, f64>
{
    fn from(obs: PyObject) -> Self {
        pyo3::Python::with_gil(|py| {
            // let obs: &PyArrayDyn<f64> = obs.extract(py).unwrap();
            let obs: &PyArrayDyn<f64> = obs.extract(py).unwrap();
            let obs = obs.to_owned_array();
            let obs = obs.mapv(|elem| elem as f32);
            let obs = {
                if obs.shape().len() == S::shape().len() + 1 {
                    // In this case obs has a dimension for n_procs
                    obs
                }
                else if obs.shape().len() == S::shape().len() {
                    // add dimension for n_procs
                    obs.insert_axis(Axis(0))
                }
                else {
                    panic!();
                }
            };
            Self {
                obs,
                phantom: PhantomData,
            }
        })
    }
}

impl<S: Shape> From<PyObject> for TchPyGymEnvObs<S, u8>
{
    fn from(obs: PyObject) -> Self {
        pyo3::Python::with_gil(|py| {
            let obs: &PyArrayDyn<u8> = obs.extract(py).unwrap();
            let obs = obs.to_owned_array();
            let obs = obs.mapv(|elem| elem as f32);
            let obs = {
                if obs.shape().len() == S::shape().len() + 1 {
                    // In this case obs has a dimension for n_procs
                    obs
                }
                else if obs.shape().len() == S::shape().len() {
                    // add dimension for n_procs
                    obs.insert_axis(Axis(0))
                }
                else {
                    println!("{:?}", obs.shape());
                    panic!();
                }
            };
            Self {
                obs,
                phantom: PhantomData,
            }
        })
    }
}

impl<S: Shape, D> Into<Tensor> for TchPyGymEnvObs<S, D> where
    D: Clone + std::fmt::Debug
{
    fn into(self) -> Tensor {
        try_from(self.obs).unwrap()
    }
}

pub struct TchPyGymEnvObsBuffer<S, D> where
    D: Clone + std::fmt::Debug
{
    obs: Tensor,
    phantom: PhantomData<(S, D)>,
}

impl<S: Shape, D> TchBuffer for TchPyGymEnvObsBuffer<S, D> where
    D: Clone + std::fmt::Debug
{
    type Item = TchPyGymEnvObs<S, D>;
    type SubBatch = Tensor;

    fn new(capacity: usize, n_procs: usize) -> Self {
        let capacity = capacity as _;
        let n_procs = n_procs as _;
        let shape = concat_slices(&[capacity, n_procs],
            S::shape().iter().map(|v| *v as i64).collect::<Vec<_>>().as_slice());
        Self {
            obs: Tensor::zeros(&shape, tch::kind::FLOAT_CPU),
            phantom: PhantomData
        }
    }

    fn push(&mut self, index: i64, item: &Self::Item) {
        let obs = item.clone().into();
        self.obs.get(index).copy_(&obs);
    }

    /// Create minibatch.
    /// The second axis is squeezed, thus the batch size is
    /// `batch_indexes.len()` times `n_procs`.
    fn batch(&self, batch_indexes: &Tensor) -> Tensor {
        let batch = self.obs.index_select(0, &batch_indexes);
        batch.flatten(0, 1)
    }
}
