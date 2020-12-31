use pyo3::{Python};
use lrr::core::{Trainer};
use lrr::py_gym_env::{PyGymEnv, PyGymDiscreteAct};
use lrr::py_gym_env::adapter::{PyNDArrayObsAdapter, PyGymDiscreteActAdapter};
use lrr::agents::{DQN, dqn::QNetwork, ReplayBuffer};

fn main() {
    std::env::set_var("RUST_LOG", "trace");
    env_logger::init();

    let mut env = match PyGymEnv::<PyGymDiscreteAct>::new("CartPole-v0") {
        Ok(env) => env,
        Err(e) => {
            let gil = Python::acquire_gil();
            let py = gil.python();
            println!("{:?}", e.ptype(py));
            println!("{:?}", e.pvalue(py));
            panic!();
        }
    };
    env.set_render(false);
    let env_eval = env.clone();
    let qnet = QNetwork::new(4, 2, 0.01);
    let from_obs = PyNDArrayObsAdapter::new(&[4]);
    let into_act = PyGymDiscreteActAdapter::new(&[1]);
    let replay_buffer = ReplayBuffer::<PyGymEnv<PyGymDiscreteAct>, _, _>::new(1000, &from_obs, &into_act);
    let dqn: DQN<PyGymEnv<PyGymDiscreteAct>, _, _, _> = DQN::new(
        qnet,
        replay_buffer,
        from_obs,
        into_act)
        .n_samples_per_opt(10)
        .n_updates_per_opt(1)
        .n_opts_per_soft_update(1)
        .min_transitions_warmup(8)
        .batch_size(8)
        .discount_factor(0.99)
        .tau(0.005);
    let mut trainer = Trainer::new(
        env,
        env_eval,
        dqn,
        2,
    );

    trainer.train();
}
