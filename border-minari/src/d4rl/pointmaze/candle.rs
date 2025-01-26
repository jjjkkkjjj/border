//! Observation, action types and corresponding converters for the Point Maze environment implemented with candle.
use crate::{
    util::ndarray::{arrayd_to_pyobj, pyobj_to_arrayd},
    MinariConverter, MinariDataset,
};
use anyhow::Result;
use border_core::generic_replay_buffer::BatchBase;
use candle_core::{DType, Device, Tensor};
use ndarray::{ArrayBase, ArrayD, Axis, Slice};
use pyo3::{types::PyIterator, PyAny, PyObject, Python};
use std::convert::{TryFrom, TryInto};

mod obs {
    use super::Tensor;

    /// Observation of the Point Maze environment stored as [`Tensor`].
    ///
    /// To create of batch of observations, this struct can be converted into [`PointMazeObsBatch`].
    ///
    /// [`Tensor`]: candle_core::Tensor
    #[derive(Clone, Debug)]
    pub struct PointMazeObs {
        pub(super) obs: Tensor,
    }

    impl border_core::Obs for PointMazeObs {
        fn len(&self) -> usize {
            self.obs.dims()[0]
        }
    }

    /// Converts [`PointMazeObs`] to Tensor.
    impl Into<Tensor> for PointMazeObs {
        fn into(self) -> Tensor {
            self.obs
        }
    }
}

mod obs_batch {
    use super::{BatchBase, DType, Device, PointMazeObs, Tensor};

    /// Batch of observations.
    ///
    /// It can be converted from [`PointMazeObs`].
    ///
    /// It can be converted into [`Tensor`].　This allows a batch of observations to be fed into a neural network.
    ///
    /// [`Tensor`]: candle_core::Tensor
    #[derive(Clone, Debug)]
    pub struct PointMazeObsBatch {
        pub(super) capacity: usize,
        pub(super) obs: Option<Tensor>,
    }

    impl BatchBase for PointMazeObsBatch {
        fn new(capacity: usize) -> Self {
            Self {
                capacity,
                obs: None,
                // obs: ,
            }
        }

        fn push(&mut self, ix: usize, data: Self) {
            // Push samples when data is not empty
            if let Some(obs) = &data.obs {
                // Lazy creation of the internal buffer
                if self.obs.is_none() {
                    let dim = obs.dims()[1];
                    self.obs = Some(
                        Tensor::zeros((self.capacity, dim), DType::F32, &Device::Cpu).unwrap(),
                    );
                }

                // Push samples to the internal buffer
                self.obs.as_mut().unwrap().slice_set(&obs, 0, ix).unwrap();
            }
        }

        fn sample(&self, ixs: &Vec<usize>) -> Self {
            let capacity = ixs.len();
            let ixs = Tensor::from_vec(
                ixs.iter().map(|&ix| ix as u32).collect::<Vec<u32>>(),
                (ixs.len(),),
                &Device::Cpu,
            )
            .unwrap();
            Self {
                capacity,
                obs: Some(self.obs.as_ref().unwrap().index_select(&ixs, 0).unwrap()),
            }
        }
    }

    impl From<PointMazeObs> for PointMazeObsBatch {
        fn from(obs: PointMazeObs) -> Self {
            // Size of obs = [batch_size, dim_of_obs_vec]
            assert_eq!(obs.obs.dims().len(), 2);

            Self {
                capacity: obs.obs.dims()[0],
                obs: Some(obs.obs),
            }
        }
    }

    impl Into<Tensor> for PointMazeObsBatch {
        fn into(self) -> Tensor {
            self.obs.unwrap()
        }
    }
}

mod act {
    use super::Tensor;

    /// Action of the Point Maze environment stored as [`Tensor`].
    ///
    /// It can be converted from a [`Tensor`] and can be converted into a [`PyObject`].
    /// It allows the action to inferred from the neural network and be passed to the Python interpreter.
    ///
    /// To create a batch of actions, this struct can be converted into [`PointMazeActBatch`].
    ///
    /// [`Tensor`]: candle_core::Tensor
    #[derive(Clone, Debug)]
    pub struct PointMazeAct {
        pub(super) action: Tensor,
    }

    impl border_core::Act for PointMazeAct {}

    impl From<Tensor> for PointMazeAct {
        fn from(action: Tensor) -> Self {
            Self { action }
        }
    }

    impl Into<Tensor> for PointMazeAct {
        fn into(self) -> Tensor {
            self.action
        }
    }
}

mod act_batch {
    use super::{BatchBase, DType, Device, PointMazeAct, Tensor, TryInto};

    /// Batch of actions.
    ///
    /// It can be converted into [`Tensor`] for handling with neural networks.
    ///
    /// [`Tensor`]: candle_core::Tensor
    #[derive(Clone, Debug)]
    pub struct PointMazeActBatch {
        pub(super) action: Tensor,
    }

    impl PointMazeActBatch {
        /// Returns an action at the specified index in the batch.
        pub fn get(&self, ix: usize) -> PointMazeAct {
            PointMazeAct {
                action: self
                    .action
                    .index_select(&(ix as u32).try_into().unwrap(), 0)
                    .unwrap()
                    .copy()
                    .unwrap(),
            }
        }
    }

    impl BatchBase for PointMazeActBatch {
        fn new(capacity: usize) -> Self {
            Self {
                // Dimension of action vector should be 2
                action: Tensor::zeros((capacity, 2), DType::F32, &Device::Cpu).unwrap(),
            }
        }

        fn push(&mut self, ix: usize, data: Self) {
            self.action.slice_set(&data.action, 0, ix).unwrap();
        }

        fn sample(&self, ixs: &Vec<usize>) -> Self {
            let action = {
                let ixs = Tensor::from_vec(
                    ixs.iter().map(|&ix| ix as u32).collect::<Vec<u32>>(),
                    (ixs.len(),),
                    &Device::Cpu,
                )
                .unwrap();
                self.action.index_select(&ixs, 0).unwrap().copy().unwrap()
            };

            Self { action }
        }
    }

    impl From<PointMazeAct> for PointMazeActBatch {
        fn from(act: PointMazeAct) -> Self {
            Self { action: act.action }
        }
    }

    impl Into<Tensor> for PointMazeActBatch {
        fn into(self) -> Tensor {
            self.action
        }
    }
}

pub use act::PointMazeAct;
pub use act_batch::PointMazeActBatch;
pub use obs::PointMazeObs;
pub use obs_batch::PointMazeObsBatch;

/// Configuration of [`PointMazeConverter`].
pub struct PointMazeConverterConfig {
    /// If `true`, the observation vectors will include the x and y positions in the last two dimensions.
    /// Default is `false`.
    pub include_goal: bool,
}

impl Default for PointMazeConverterConfig {
    fn default() -> Self {
        Self {
            include_goal: false,
        }
    }
}

impl PointMazeConverterConfig {
    pub fn include_goal(self, value: bool) -> Self {
        let mut config = self;
        config.include_goal = value;
        config
    }
}

/// Converter for the Point Maze environment implemented with candle.
///
/// This struct normalizes observations based on the statistics
/// of the observations in the dataset.
pub struct PointMazeConverter {
    include_goal: bool,
    mean: Tensor, // for normalizing observation
    std: Tensor,  // for normalizing observation
}

impl PointMazeConverter {
    /// Creates a new Point Maze converter.
    ///
    /// `dataset` is used to calculate the mean and standard deviation of the observations.
    pub fn new(config: PointMazeConverterConfig, dataset: &MinariDataset) -> Result<Self> {
        let (mean, std) = Python::with_gil(|py| -> Result<(Tensor, Tensor)> {
            // Iterate all episodes
            let episodes = dataset
                .dataset
                .call_method1(py, "iterate_episodes", (None::<i32>,))?;
            let mut all_obs = Tensor::zeros(&[0, 4], DType::F32, &Device::Cpu)?;

            // Collect all observations for calculating mean and std
            for ep in PyIterator::from_object(py, &episodes)? {
                // ep is minari.dataset.episode_data.EpisodeData
                let ep = ep?;
                let obj = ep.getattr("observations")?;

                let obs_batch = pyobj_to_tensor1(obj, "observation")?;
                all_obs = Tensor::cat(&[all_obs, obs_batch], 0)?;
            }

            // Calculate mean and std
            let mean = all_obs.mean(0)?.unsqueeze(0)?;
            let std = all_obs.var(0)?.sqrt()?.unsqueeze(0)?;
            debug_assert_eq!(mean.dims(), &[1, 4]);

            Ok((mean, std))
        })?;

        Ok(Self {
            include_goal: config.include_goal,
            mean,
            std,
        })
    }

    fn normalize_observation(&self, obs: &Tensor) -> Result<Tensor> {
        Ok(obs.broadcast_sub(&self.mean)?.broadcast_div(&self.std)?)
    }
}

impl MinariConverter for PointMazeConverter {
    type Obs = PointMazeObs;
    type Act = PointMazeAct;
    type ObsBatch = PointMazeObsBatch;
    type ActBatch = PointMazeActBatch;

    fn convert_observation(&self, obj: &PyAny) -> Result<Self::Obs> {
        match self.include_goal {
            false => {
                let obs = obj.get_item("observation")?.extract()?;
                let obs = arrayd_to_tensor(pyobj_to_arrayd::<f64, f32>(obs), Some(&[1, 4]))?;
                Ok(PointMazeObs {
                    obs: self.normalize_observation(&obs)?,
                })
            }
            true => {
                let obs = obj.get_item("observation")?.extract()?;
                let obs = arrayd_to_tensor(pyobj_to_arrayd::<f64, f32>(obs), Some(&[1, 4]))?;
                let obs = self.normalize_observation(&obs)?;
                let goal = obj.get_item("desired_goal")?.extract()?;
                let goal = arrayd_to_tensor(pyobj_to_arrayd::<f64, f32>(goal), Some(&[1, 2]))?;
                Ok(PointMazeObs {
                    obs: Tensor::cat(&[obs, goal], candle_core::D::Minus1)?,
                })
            }
        }
    }

    fn convert_action(&self, act: Self::Act) -> Result<PyObject> {
        Ok(arrayd_to_pyobj(tensor_to_arrayd(act.action)?))
    }

    fn convert_observation_batch(&self, obj: &PyAny) -> Result<Self::ObsBatch> {
        match self.include_goal {
            false => {
                let obs = pyobj_to_tensor1(obj, "observation")?;
                let obs = self.normalize_observation(&obs)?;

                // Check tensor size: expects [batch_size, obs_vec_dim]
                let batch_size = obs.dims()[0];
                debug_assert_eq!(obs.dims(), &[batch_size, 4]);

                Ok(PointMazeObsBatch {
                    capacity: batch_size,
                    obs: Some(obs),
                })
            }
            true => {
                let obs = pyobj_to_tensor1(obj, "observation")?;
                let goal = pyobj_to_tensor1(obj, "desired_goal")?;

                // Drop the last dim
                let obs = obs.squeeze(candle_core::D::Minus1)?;
                let goal = goal.squeeze(candle_core::D::Minus1)?;

                // Normalize obs (keep goal unchanged)
                let obs = self.normalize_observation(&obs)?;

                // Check tensor size: expects [batch_size, obs_vec_dim]
                let batch_size = obs.dims()[0];
                debug_assert_eq!(obs.dims(), &[batch_size, 4]);
                debug_assert_eq!(goal.dims(), &[batch_size, 2]);

                // Concat obs and goal
                let obs = Tensor::cat(&[obs, goal], candle_core::D::Minus1)?;

                Ok(PointMazeObsBatch {
                    capacity: batch_size,
                    obs: Some(obs),
                })
            }
        }
    }

    fn convert_observation_batch_next(&self, obj: &PyAny) -> Result<Self::ObsBatch> {
        match self.include_goal {
            false => {
                let obs = pyobj_to_tensor2(obj, "observation")?;
                let obs = self.normalize_observation(&obs)?;

                // Check tensor size: expects [batch_size, obs_vec_dim]
                let batch_size = obs.dims()[0];
                debug_assert_eq!(obs.dims(), &[batch_size, 4]);

                Ok(PointMazeObsBatch {
                    capacity: batch_size,
                    obs: Some(obs),
                })
            }
            true => {
                let obs = pyobj_to_tensor2(obj, "observation")?;
                let goal = pyobj_to_tensor2(obj, "desired_goal")?;

                // Drop the last dim
                let obs = obs.squeeze(candle_core::D::Minus1)?;
                let goal = goal.squeeze(candle_core::D::Minus1)?;

                // Normalize
                let obs = self.normalize_observation(&obs)?;

                // Check tensor size: expects [batch_size, obs_vec_dim]
                let batch_size = obs.dims()[0];
                debug_assert_eq!(obs.dims(), &[batch_size, 4]);
                debug_assert_eq!(goal.dims(), &[batch_size, 2]);

                // Concat obs and goal
                let obs = Tensor::cat(&[obs, goal], candle_core::D::Minus1)?;

                Ok(PointMazeObsBatch {
                    capacity: batch_size,
                    obs: Some(obs),
                })
            }
        }
    }

    fn convert_action_batch(&self, obj: &PyAny) -> Result<Self::ActBatch> {
        Ok(PointMazeActBatch {
            action: {
                let arr = pyobj_to_arrayd::<f32, f32>(obj.into());
                arrayd_to_tensor(arr, None)?
            },
        })
    }

    fn env_params(&self) -> Vec<(&str, Option<&str>)> {
        // not override the original parameters in Minari
        // https://github.com/Farama-Foundation/minari-dataset-generation-scripts/blob/cc54b71147650b310f5a84c642dd6dc127f333a1/scripts/pointmaze/create_pointmaze_dataset.py#L157-L159
        vec![]

        // When want to override the parameters, comment in the following code:
        // vec![
        //     ("continuing_task", Some("True")),
        //     ("reset_target", Some("True")),
        // ]
    }
}

/// Converts PyObject to [`candle_core::Tensor`] and drop the last row.
fn pyobj_to_tensor1(obj: &PyAny, name: &str) -> Result<Tensor> {
    // From python object to ndarray
    let arr = pyobj_to_arrayd::<f64, f32>(obj.get_item(name)?.extract()?);

    // Drop the last row
    let arr = arr.slice_axis(Axis(0), Slice::from(..-1)).to_owned();

    // Convert to Tensor
    Ok(arrayd_to_tensor(arr, None)?)
}

/// Converts PyObject to Tensor and drop the first row.
fn pyobj_to_tensor2(obj: &PyAny, name: &str) -> Result<Tensor> {
    // From python object to ndarray
    let arr = pyobj_to_arrayd::<f64, f32>(obj.get_item(name)?.extract()?);

    // Drop the first row
    let arr = arr.slice_axis(Axis(0), Slice::from(1..)).to_owned();

    // Convert to Tensor
    Ok(arrayd_to_tensor(arr, None)?)
}

/// Converts ArrayD to tensor.
///
/// When `shape` is None, its shape will be the result of `arr.shape()`.
fn arrayd_to_tensor(arr: ArrayD<f32>, shape: Option<&[usize]>) -> Result<Tensor> {
    let shape = match shape {
        Some(shape) => shape,
        None => arr.shape(),
    };
    let tensor = Tensor::try_from(arr.as_slice().expect("Slice of ndarray"))?.reshape(shape)?;
    Ok(tensor)
}

/// Converts tensor to ArrayD.
fn tensor_to_arrayd(tensor: Tensor) -> Result<ArrayD<f32>> {
    let shape = tensor
        .dims()
        .iter()
        .map(|&x| x as usize)
        .collect::<Vec<usize>>();
    let arr = ArrayBase::from_vec(tensor.flatten_all()?.to_vec1()?).into_shape(shape)?;
    Ok(arr)
}
