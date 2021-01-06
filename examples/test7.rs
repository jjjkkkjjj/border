use std::error::Error;
use ndarray::ArrayD;
use pyo3::{PyObject, IntoPy};
use numpy::PyArrayDyn;
use tch::Tensor;
use lrr::core::{Obs, Act, Trainer, Agent, util};
use lrr::py_gym_env::PyGymEnv;
use lrr::agents::tch::{PPODiscrete, TchBufferableActInfo, TchBufferableObsInfo};

fn main() {}

// #[derive(Clone, Debug)]
// pub struct CartPoleObs (pub ArrayD<f32>);

// impl Obs for CartPoleObs {}

// impl TchBufferableObsInfo for CartPoleObs {
//     fn shape() -> Vec<i64> {
//         vec![4]
//     }

//     fn tch_kind() -> (tch::Kind, tch::Device) {
//         tch::kind::FLOAT_CPU
//     }
// }

// impl From<PyObject> for CartPoleObs {
//     fn from(obs: PyObject) -> Self {
//         pyo3::Python::with_gil(|py| {
//             let obs: &PyArrayDyn<f64> = obs.extract(py).unwrap();
//             let obs = obs.to_owned_array();
//             let obs = obs.mapv(|elem| elem as f32);
//             Self {
//                 0: obs
//             }
//         })
//     }
// }

// impl Into<Tensor> for CartPoleObs {
//     fn into(self) -> Tensor {
//         let obs = self.0.view().to_slice().unwrap();
//         Tensor::of_slice(obs)
//     }
// }

// #[derive(Clone, Debug)]
// pub struct CartPoleAct (u32);

// impl Act for CartPoleAct {}

// impl CartPoleAct {
//     pub fn new(v: u32) -> Self {
//         CartPoleAct { 0: v }
//     }
// }

// impl Into<PyObject> for CartPoleAct {
//     fn into(self) -> PyObject {
//         pyo3::Python::with_gil(|py| {
//             self.0.into_py(py)
//         })
//     }
// }

// impl Into<Tensor> for CartPoleAct {
//     fn into(self) -> Tensor {
//         (self.0 as i32).into()
//     }
// }

// impl From<Tensor> for CartPoleAct {
//     fn from(t: Tensor) -> Self {
//         let a: i32 = t.into();
//         Self::new(a as u32)
//     }
// }

// impl TchBufferableActInfo for CartPoleAct {
//     fn shape() -> Vec<i64> {
//         vec![1]
//     }

//     fn tch_kind() -> (tch::Kind, tch::Device) {
//         tch::kind::INT64_CPU
//     }
// }

// fn create_agent() -> impl Agent<PyGymEnv<CartPoleObs, CartPoleAct>> {
//     let mh_model = QNetwork::new(4, 2, 0.001);
//     let agent: PPODiscrete<PyGymEnv<CartPoleObs, CartPoleAct>, _> = PPODiscrete::new(
//         mh_model, 200)
//         .n_updates_per_opt(1)
//         .batch_size(64)
//         .discount_factor(0.99);
//     agent
// }

// fn main() -> Result<(), Box<dyn Error>> {
//     std::env::set_var("RUST_LOG", "info");
//     env_logger::init();
//     tch::manual_seed(42);

//     let env = PyGymEnv::<CartPoleObs, CartPoleAct>::new("CartPole-v0")?;
//     let env_eval = PyGymEnv::<CartPoleObs, CartPoleAct>::new("CartPole-v0")?;
//     let agent = create_agent();
//     let mut trainer = Trainer::new(
//         env,
//         env_eval,
//         agent)
//         .max_opts(1000)
//         .n_opts_per_eval(50)
//         .n_episodes_per_eval(5);

//     trainer.train();
//     trainer.get_agent().save("./examples/test5")?;

//     let mut env = PyGymEnv::<CartPoleObs, CartPoleAct>::new("CartPole-v0")?;
//     let mut agent = create_agent();
//     env.set_render(true);
//     agent.load("./examples/test5")?;
//     agent.eval();
//     util::eval(&env, &agent, 5, None);

//     Ok(())
// }
