//! Wrapper of gym environments implemented in Python.
#![allow(clippy::float_cmp)]
use crate::{AtariWrapper, GymEnvConfig};
use anyhow::Result;
use border_core::{
    record::{Record, RecordValue::Scalar},
    Act, Env, Info, Obs, Step,
};
use log::{info, trace};
// use pyo3::IntoPy;
use pyo3::types::{IntoPyDict, PyTuple};
use pyo3::{types::PyModule, PyObject, Python, ToPyObject};
use serde::{de::DeserializeOwned, Serialize};
use std::marker::PhantomData;
use std::{fmt::Debug, time::Duration};

/// Information given at every step of the interaction with the environment.
///
/// Currently, it is empty and used to match the type signature.
pub struct GymInfo {}

impl Info for GymInfo {}

/// Convert [`PyObject`] to [`GymEnv`]::Obs with a preprocessing.
///
/// [`PyObject`]: https://docs.rs/pyo3/0.14.5/pyo3/type.PyObject.html
pub trait GymObsFilter<O: Obs> {
    /// Configuration.
    type Config: Clone + Default + Serialize + DeserializeOwned;

    /// Build filter.
    fn build(config: &Self::Config) -> Result<Self>
    where
        Self: Sized;

    /// Convert PyObject into observation with filtering.
    fn filt(&mut self, obs: PyObject) -> (O, Record);

    /// Called when resetting the environment.
    ///
    /// This method is useful for stateful filters.
    fn reset(&mut self, obs: PyObject) -> O {
        let (obs, _) = self.filt(obs);
        obs
    }

    /// Returns default configuration.
    fn default_config() -> Self::Config {
        Self::Config::default()
    }
}

/// Convert [`GymEnv`]::Act to [`PyObject`] with a preprocessing.
///
/// [`PyObject`]: https://docs.rs/pyo3/0.14.5/pyo3/type.PyObject.html
pub trait GymActFilter<A: Act> {
    /// Configuration.
    type Config: Clone + Default + Serialize + DeserializeOwned;

    /// Build filter.
    fn build(config: &Self::Config) -> Result<Self>
    where
        Self: Sized;

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
    fn reset(&mut self, _is_done: &Option<&Vec<i8>>) {}

    /// Returns default configuration.
    fn default_config() -> Self::Config {
        Self::Config::default()
    }
}

/// An wrapper of [Gymnasium](https://gymnasium.farama.org).
#[derive(Debug)]
pub struct GymEnv<O, A, OF, AF>
where
    O: Obs,
    A: Act,
    OF: GymObsFilter<O>,
    AF: GymActFilter<A>,
{
    render: bool,

    env: PyObject,

    count_steps: usize,

    max_steps: Option<usize>,

    obs_filter: OF,

    act_filter: AF,

    wait: Duration,

    pybullet: bool,

    pybullet_state: Option<PyObject>,

    /// Initial seed.
    ///
    /// This value will be used at the first call of the reset method.
    initial_seed: Option<i64>,

    phantom: PhantomData<(O, A)>,
}

impl<O, A, OF, AF> GymEnv<O, A, OF, AF>
where
    O: Obs,
    A: Act,
    OF: GymObsFilter<O>,
    AF: GymActFilter<A>,
{
    /// Set rendering mode.
    ///
    /// If `true`, it renders the state at every step.
    pub fn set_render(&mut self, render: bool) {
        self.render = render;
        if self.pybullet {
            pyo3::Python::with_gil(|py| {
                // self.env.call_method0(py, "render").unwrap();
                self.env
                    .call_method(py, "render", ("human",), None)
                    .unwrap();
            });
        }
    }

    /// Set the maximum number of steps in the environment.
    pub fn max_steps(mut self, v: Option<usize>) -> Self {
        self.max_steps = v;
        self
    }

    /// Set wait time at every interaction steps.
    pub fn set_wait(&mut self, d: Duration) {
        self.wait = d;
    }

    /// Get the number of available actions of atari environments
    pub fn get_num_actions_atari(&self) -> i64 {
        pyo3::Python::with_gil(|py| {
            let act_space = self.env.getattr(py, "action_space").unwrap();
            act_space.getattr(py, "n").unwrap().extract(py).unwrap()
        })
    }
}

impl<O, A, OF, AF> Env for GymEnv<O, A, OF, AF>
where
    O: Obs,
    A: Act + Debug,
    OF: GymObsFilter<O>,
    AF: GymActFilter<A>,
{
    type Obs = O;
    type Act = A;
    type Info = GymInfo;
    type Config = GymEnvConfig<O, A, OF, AF>;

    /// Resets the environment and returns an observation.
    ///
    /// This method also resets the [`GymObsFilter`] adn [`GymActFilter`].
    ///
    /// In this environment, `is_done` should be None.
    ///
    /// [`GymObsFilter`]: crate::GymObsFilter
    /// [`GymActFilter`]: crate::GymActFilter
    fn reset(&mut self, is_done: Option<&Vec<i8>>) -> Result<O> {
        trace!("PyGymEnv::reset()");
        assert_eq!(is_done, None);

        // Reset the action filter, required for stateful filters.
        self.act_filter.reset(&is_done);

        // Initial observation
        let ret = pyo3::Python::with_gil(|py| {
            let obs = {
                let ret_values = if let Some(seed) = self.initial_seed {
                    self.initial_seed = None;
                    let kwargs = match self.pybullet {
                        true => None,
                        false => Some(vec![("seed", seed)].into_py_dict(py)),
                    };
                    self.env.call_method(py, "reset", (), kwargs)?
                } else {
                    self.env.call_method0(py, "reset")?
                };
                let ret_values_: &PyTuple = ret_values.extract(py).unwrap();
                ret_values_.get_item(0).extract().unwrap()
            };

            if self.pybullet && self.render {
                let floor: &PyModule = self.pybullet_state.as_ref().unwrap().extract(py).unwrap();
                floor.getattr("add_floor")?.call1((&self.env,)).unwrap();
            }
            Ok(self.obs_filter.reset(obs))
        });

        // Rendering
        if self.pybullet && self.render {
            pyo3::Python::with_gil(|py| {
                // self.env.call_method0(py, "render").unwrap();
                self.env
                    .call_method(py, "render", ("human",), None)
                    .unwrap();
            });
        }

        ret
    }

    /// Resets the environment with the given index.
    ///
    /// Specifically, env.reset(seed=ix) is called in the Python interpreter.
    fn reset_with_index(&mut self, ix: usize) -> Result<Self::Obs> {
        self.initial_seed = Some(ix as _);
        self.reset(None)
    }

    /// Runs a step of the environment's dynamics.
    ///
    /// It returns [`Step`] and [`Record`] objects.
    /// The [`Record`] is composed of [`Record`]s constructed in [`GymObsFilter`] and
    /// [`GymActFilter`].
    fn step(&mut self, act: &A) -> (Step<Self>, Record) {
        fn is_done(step: &PyTuple) -> (i8, i8) {
            // terminated or truncated
            let is_terminated = match step.get_item(2).extract().unwrap() {
                true => 1,
                false => 0,
            };
            let is_truncated = match step.get_item(3).extract().unwrap() {
                true => 1,
                false => 0,
            };

            (is_terminated, is_truncated)
        }

        trace!("PyGymEnv::step()");

        pyo3::Python::with_gil(|py| {
            if self.render {
                if !self.pybullet {
                    let _ = self.env.call_method0(py, "render");
                } else {
                    let cam: &PyModule = self.pybullet_state.as_ref().unwrap().extract(py).unwrap();
                    // cam.call1("update_camera_pos", (&self.env,)).unwrap();
                    cam.getattr("update_camera_pos")
                        .unwrap()
                        .call1((&self.env,))
                        .unwrap();
                }
                std::thread::sleep(self.wait);
            }

            // State transition
            let (
                act,
                next_obs,
                reward,
                is_terminated,
                mut is_truncated,
                mut record,
                info,
                init_obs,
            ) = {
                let (a_py, record_a) = self.act_filter.filt(act.clone());
                let ret = self.env.call_method(py, "step", (a_py,), None).unwrap();
                let step: &PyTuple = ret.extract(py).unwrap();
                let next_obs = step.get_item(0).to_owned();
                let (next_obs, record_o) = self.obs_filter.filt(next_obs.to_object(py));
                let reward: Vec<f32> = vec![step.get_item(1).extract().unwrap()];
                let (is_terminated, is_truncated) = is_done(step);
                let is_terminated = vec![is_terminated];
                let is_truncated = vec![is_truncated];
                let record = record_o.merge(record_a);
                let info = GymInfo {};
                let init_obs = None;
                let act = act.clone();

                (
                    act,
                    next_obs,
                    reward,
                    is_terminated,
                    is_truncated,
                    record,
                    info,
                    init_obs,
                )
            };

            self.count_steps += 1; //.replace(c + 1);

            // Terminated or truncated
            if let Some(max_steps) = self.max_steps {
                if self.count_steps >= max_steps {
                    is_truncated[0] = 1;
                }
            };

            if (is_terminated[0] | is_truncated[0]) == 1 {
                record.insert("episode_length", Scalar(self.count_steps as _));
                self.count_steps = 0;
            }

            (
                Step::new(
                    next_obs,
                    act,
                    reward,
                    is_terminated,
                    is_truncated,
                    info,
                    init_obs,
                ),
                record,
            )
        })
    }

    /// Constructs [`GymEnv`].
    ///
    /// * `seed` - The seed value of the random number generator.
    ///   This value will be used at the first call of the reset method.
    fn build(config: &Self::Config, seed: i64) -> Result<Self> {
        let gil = Python::acquire_gil();
        let py = gil.python();

        // sys.argv is used by pyglet library, which is responsible for rendering.
        // Depending on the python interpreter, however, sys.argv can be empty.
        // For that case, sys argv is set here.
        // See https://github.com/PyO3/pyo3/issues/1241#issuecomment-715952517
        let locals = [("sys", py.import("sys")?)].into_py_dict(py);
        let _ = py.eval("sys.argv.insert(0, 'PyGymEnv')", None, Some(&locals))?;
        let path = py.eval("sys.path", None, Some(&locals)).unwrap();
        let ver = py.eval("sys.version", None, Some(&locals)).unwrap();
        info!("Initialize PyGymEnv");
        info!("{}", path);
        info!("Python version = {}", ver);

        // import pybullet-gym if it exists
        if py.import("pybulletgym").is_ok() {}

        // For some unknown reason, Mujoco requires this import
        if py.import("IPython").is_ok() {}

        let name = config.name.as_str();
        let (env, render) = if let Some(mode) = config.atari_wrapper.as_ref() {
            let mode = match mode {
                AtariWrapper::Train => true,
                AtariWrapper::Eval => false,
            };
            let gym = py.import("atari_wrappers")?;
            let env = gym
                .getattr("make_env_single_proc")?
                .call((name, true, mode), None)?;
            (env, false)
        } else if !config.pybullet {
            let gym = py.import("f32_wrapper")?;
            let render = config.render_mode.is_some();
            let env = {
                let kwargs = if let Some(render_mode) = config.render_mode.clone() {
                    Some(vec![("render_mode", render_mode)].into_py_dict(py))
                } else {
                    None
                };
                gym.getattr("make_f32")?.call((name,), kwargs)?
            };

            (env, render)
        } else {
            let gym = py.import("f32_wrapper")?;
            let kwargs = None;
            let env = gym.getattr("make_f32")?.call((name,), kwargs)?;
            if config.render_mode.is_some() {
                env.call_method("render", ("human",), None).unwrap();
                (env, true)
            } else {
                (env, false)
            }
        };

        // TODO: consider removing action_space and observation_space.
        // Act/obs types are specified by type parameters.
        let action_space = env.getattr("action_space")?;
        println!("Action space = {:?}", action_space);
        let observation_space = env.getattr("observation_space")?;
        println!("Observation space = {:?}", observation_space);

        let pybullet_state = if !config.pybullet {
            None
        } else {
            let pybullet_state = Python::with_gil(|py| {
                PyModule::from_code(
                    py,
                    r#"
_torsoId = None
_floor = False

def unwrap(env):
    while True:
        if hasattr(env, "_p"):
            return env
        else:
            env = env.env

def add_floor(env):
    global _floor
    if not _floor:
        env = unwrap(env)
        p = env._p
        import pybullet_data
        p.setAdditionalSearchPath(pybullet_data.getDataPath())
        p.loadURDF("plane.urdf")
        _floor = True
        env.stateId = p.saveState()

def get_torso_id(p):
    global _torsoId
    if _torsoId is None:
        torsoId = -1
        for i in range(p.getNumBodies()):
            print(p.getBodyInfo(i))
            if p.getBodyInfo(i)[0].decode() == "torso":
                torsoId = i
                print("found torso")
        _torsoId = torsoId
    
    return _torsoId

def update_camera_pos(env):
    env = unwrap(env)
    p = env._p
    torsoId = get_torso_id(p)
    if torsoId >= 0:
        distance = 5
        yaw = 0
        humanPos, humanOrn = p.getBasePositionAndOrientation(torsoId)
        p.resetDebugVisualizerCamera(distance, yaw, -20, humanPos)

            "#,
                    "pybullet_state.py",
                    "pybullet_state",
                )
                .unwrap()
                .to_object(py)
            });
            Some(pybullet_state)
        };

        Ok(GymEnv {
            env: env.into(),
            obs_filter: OF::build(&config.obs_filter_config.as_ref().unwrap())?,
            act_filter: AF::build(&config.act_filter_config.as_ref().unwrap())?,
            render,
            count_steps: 0,
            wait: config.wait,
            max_steps: config.max_steps,
            pybullet: config.pybullet,
            pybullet_state,
            initial_seed: Some(seed),
            phantom: PhantomData,
        })
    }
}
