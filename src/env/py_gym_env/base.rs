#![allow(clippy::float_cmp)]
use std::{fmt::Debug, error::Error};
use std::marker::PhantomData;
use std::cell::RefCell;
use log::{trace};
use pyo3::{PyObject, PyResult, Python, ToPyObject};
use pyo3::types::{PyTuple, IntoPyDict};

use crate::core::{Act, Env, Info, Obs, Step, record::Record};

/// Information given at every step of the interaction with the environment.
///
/// Currently, it is empty and used to match the type signature.
pub struct PyGymInfo {}

impl Info for PyGymInfo {}

/// Shape of observation or action.
pub trait Shape: Clone + Debug {
    /// Returns the shape of Shape of observation or action.
    ///
    /// This trait is used for conversion of PyObject in [`super::obs::pyobj_to_arrayd`] and
    fn shape() -> &'static [usize];

    /// Returns `true` if you would like to squeeze the first dimension of the array
    /// before conversion into an numpy array in Python. The first dimension may
    /// correspond to process indices for vectorized environments.
    /// This method is used in
    /// [`super::act_c::to_pyobj`] and [`super::act_c::PyGymEnvContinuousActRawFilter::filt`].
    fn squeeze_first_dim() -> bool {
        false
    }
}

/// Convert PyObject to PyGymEnv::Obs.
pub trait PyGymEnvObsFilter<O: Obs> {
    /// Convert PyObject into observation with filtering.
    fn filt(&mut self, obs: PyObject) -> (O, Record);

    /// Called when resetting the environment.
    ///
    /// This method is useful for stateful filters.
    fn reset(&mut self, obs: PyObject) -> O {
        let (obs, _) = self.filt(obs);
        obs
    }
}

/// Convert PyGymEnv::Act to PyObject.
///
/// This trait should support vectorized environments.
pub trait PyGymEnvActFilter<A: Act> {
    /// Filter action and convert it to PyObject.
    ///
    /// For vectorized environments, `act` should have actions for all environments in
    /// the vectorized environment. The return values will be a `PyList` object, each
    /// element is an action of the corresponding environment.
    fn filt(&mut self, act: A) -> (PyObject, Record);

    /// Called when resetting the environment.
    ///
    /// This method is useful for stateful filters.
    /// This method support vectorized environment
    fn reset(&mut self, _is_done: &Option<&Vec<f32>>) {}
}

/// Represents an environment in [OpenAI gym](https://github.com/openai/gym).
/// The code is adapted from [tch-rs RL example](https://github.com/LaurentMazare/tch-rs/tree/master/examples/reinforcement-learning).
#[derive(Debug, Clone)]
pub struct PyGymEnv<O, A, OF, AF> where
    O: Obs,
    A: Act,
    OF: PyGymEnvObsFilter<O>,
    AF: PyGymEnvActFilter<A>
{
    render: bool,
    env: PyObject,
    action_space: i64,
    observation_space: Vec<usize>,
    count_steps: RefCell<usize>,
    max_steps: Option<usize>,
    obs_filter: OF,
    act_filter: AF,
    phantom: PhantomData<(O, A)>,
}

impl<O, A, OF, AF> PyGymEnv<O, A, OF, AF> where
    O: Obs,
    A: Act,
    OF: PyGymEnvObsFilter<O>,
    AF: PyGymEnvActFilter<A>
{
    /// Constructs an environment.
    ///
    /// `name` is the name of the environment, which is implemented in OpenAI gym.
    pub fn new(name: &str, obs_filter: OF, act_filter: AF, atari_wrapper: bool) -> PyResult<Self> {
        let gil = Python::acquire_gil();
        let py = gil.python();

        // sys.argv is used by pyglet library, which is responsible for rendering.
        // Depending on the environment, however, sys.argv can be empty.
        // For that case, sys argv is set here.
        // See https://github.com/PyO3/pyo3/issues/1241#issuecomment-715952517
        let locals = [("sys", py.import("sys")?)].into_py_dict(py);
        let _ = py.eval("sys.argv.insert(0, 'PyGymEnv')", None, Some(&locals))?;

        let env = if atari_wrapper {
            unimplemented!()
        }
        else {
            let gym = py.import("gym")?;
            let env = gym.call("make", (name,), None)?;
            let _ = env.call_method("seed", (42,), None)?;
            env
        };

        // TODO: consider removing action_space and observation_space.
        // Act/obs types are specified by type parameters.
        let action_space = env.getattr("action_space")?;
        let action_space = if let Ok(val) = action_space.getattr("n") {
            val.extract()?
        } else {
            let action_space: Vec<i64> = action_space.getattr("shape")?.extract()?;
            action_space[0]
        };
        let observation_space = env.getattr("observation_space")?;
        let observation_space = observation_space.getattr("shape")?.extract()?;

        Ok(PyGymEnv {
            render: false,
            env: env.into(),
            action_space,
            observation_space,
            // TODO: consider remove RefCell, raw value instead
            count_steps: RefCell::new(0),
            max_steps: None,
            obs_filter,
            act_filter,
            phantom: PhantomData,
        })
    }

    /// Set rendering mode.
    ///
    /// If `true`, it renders the state at every step.
    pub fn set_render(&mut self, render: bool) {
        self.render = render;
    }

    /// Set the maximum number of steps in the environment.
    pub fn max_steps(mut self, v: Option<usize>) -> Self {
        self.max_steps = v;
        self
    }    
}

impl<O, A, OF, AF> Env for PyGymEnv<O, A, OF, AF> where
    O: Obs,
    A: Act + Debug,
    OF: PyGymEnvObsFilter<O>,
    AF: PyGymEnvActFilter<A>
{
    type Obs = O;
    type Act = A;
    type Info = PyGymInfo;

    /// Resets the environment, the obs/act filters and returns the observation tensor.
    ///
    /// In this environment, the length of `is_done` is assumed to be 1.
    ///
    /// TODO: defines appropriate error for the method and returns it.
    fn reset(&mut self, is_done: Option<&Vec<f32>>) -> Result<O, Box<dyn Error>>  {
        trace!("PyGymEnv::reset()");

        // Reset the action filter, required for stateful filters.
        self.act_filter.reset(&is_done);

        // Reset the environment
        let reset = match is_done {
            None => true,
            Some(v) => {
                debug_assert_eq!(v.len(), 1);
                !(v[0] == 0.0 as f32)
            }
        };

        if !reset {
            Ok(O::dummy(1))
        }
        else {
            pyo3::Python::with_gil(|py| {
                let obs = self.env.call_method0(py, "reset")?;
                Ok(self.obs_filter.reset(obs))
            })
        }
    }

    /// Runs a step of the environment's dynamics.
    ///
    /// It returns [`Step`] and [`Record`] objects.
    /// The [`Record`] is composed of [`Record`]s constructed in [`ObsFilter`] and [`ActFilter`].
    fn step(&mut self, a: &A) -> (Step<Self>, Record) {
        trace!("PyGymEnv::step()");

        pyo3::Python::with_gil(|py| {
            if self.render {
                let _ = self.env.call_method0(py, "render");
            }

            let (a_py, record_a) = self.act_filter.filt(a.clone());
            let ret = self.env.call_method(py, "step", (a_py,), None).unwrap();
            let step: &PyTuple = ret.extract(py).unwrap();
            let obs = step.get_item(0).to_owned();
            let (obs, record_o) = self.obs_filter.filt(obs.to_object(py));
            let reward: Vec<f32> = vec![step.get_item(1).extract().unwrap()];
            let mut is_done: Vec<f32> = vec![
                if step.get_item(2).extract().unwrap() {1.0} else {0.0}
            ];

            let c = *self.count_steps.borrow();
            self.count_steps.replace(c + 1);
            if let Some(max_steps) = self.max_steps {
                if *self.count_steps.borrow() >= max_steps {
                    is_done[0] = 1.0;
                }
            };

            (
                Step::<Self>::new(obs, a.clone(), reward, is_done, PyGymInfo{}),
                record_o.merge(record_a)
            )
        })
    }
}
