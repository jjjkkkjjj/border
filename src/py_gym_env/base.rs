use std::fmt::Debug;
use std::marker::PhantomData;
use log::{trace};
use ndarray::{Array, IxDyn};
use pyo3::{PyErr, PyObject, PyResult, Python};
use pyo3::types::{PyTuple};
use numpy::{PyArrayDyn};
use crate::core::{Info, Step, Env};
use crate::py_gym_env::{PyNDArrayObs, PyGymEnvAct};

pub struct PyGymInfo {}

impl Info for PyGymInfo {}

/// Adapted from [tch-rs RL example](https://github.com/LaurentMazare/tch-rs/tree/master/examples/reinforcement-learning)
#[derive(Debug, Clone)]
pub struct PyGymEnv<A> {
    render: bool,
    env: PyObject,
    action_space: i64,
    observation_space: Vec<usize>,
    action_type: PhantomData<A>,
}

impl<A: PyGymEnvAct + Debug> PyGymEnv<A> {
    pub fn new(name: &str) -> PyResult<Self> {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let gym = py.import("gym")?;
        let env = gym.call("make", (name,), None)?;
        let _ = env.call_method("seed", (42,), None)?;
        let action_space = env.getattr("action_space")?;
        // println!("{:?}", action_space);
        let action_space = if let Ok(val) = action_space.getattr("n") {
            val.extract()?
        } else {
            let action_space: Vec<i64> = action_space.getattr("shape")?.extract()?;
            action_space[0]
        };
        let observation_space = env.getattr("observation_space")?;
        // println!("{:?}", observation_space);
        let observation_space = observation_space.getattr("shape")?.extract()?;
        Ok(PyGymEnv {
            render: false,
            env: env.into(),
            action_space,
            observation_space,
            action_type: PhantomData,
        })
    }

    pub fn set_render(&mut self, render: bool) {
        self.render = render;
    }
}

impl<A: PyGymEnvAct + Debug> Env for PyGymEnv<A> {
    type Obs = PyNDArrayObs;
    type Act = A;
    type Info = PyGymInfo;
    type ERR = PyErr;

    /// Resets the environment, returning the observation tensor.
    fn reset(&self) -> PyResult<PyNDArrayObs>  {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let obs = self.env.call_method0(py, "reset")?;
        Ok(PyNDArrayObs(
            Array::from_shape_vec(
                IxDyn(&self.observation_space),
                obs.extract::<Vec<f32>>(py)?.clone()
            ).unwrap()
        ))
    }

    fn step(&self, a: &A) -> Step<Self> {
        trace!("{:?}", &a);
        pyo3::Python::with_gil(|py| {
            if self.render {
                let _ = self.env.call_method0(py, "render");
            }
            let a_py = a.clone().into();
            let ret = self.env.call_method(py, "step", (a_py,), None).unwrap();

            let step: &PyTuple = ret.extract(py).unwrap();

            let obs1: &PyArrayDyn<f64> = step.get_item(0).extract().unwrap();
            // let obs1: &PyArrayDyn<u8> = step.get_item(0).extract().unwrap();
            let obs2 = obs1.readonly();
            let obs3 = obs2.as_array();
            let obs4 = obs3.mapv(|elem| elem as f32);

            let r: f32 = step.get_item(1).extract().unwrap();
            let is_done: bool = step.get_item(2).extract().unwrap();

            Step::new(PyNDArrayObs(obs4), a.clone(), r, is_done, PyGymInfo{})
        })
    }
}
