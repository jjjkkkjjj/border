use std::error::Error;
use lrr::core::{Trainer, Agent, util};
use lrr::py_gym_env::{
    PyGymEnv,
    obs::{PyGymEnvObs, PyGymEnvObsRawFilter},
    act_d::{PyGymEnvDiscreteAct, PyGymEnvDiscreteActRawFilter}
};
use lrr::agents::{
    OptInterval,
    tch::{
        Shape, DQN, QNetwork, ReplayBuffer,
        py_gym_env::{
            obs::TchPyGymEnvObsBuffer,
            act_d::TchPyGymEnvDiscreteActBuffer
        }
    }
};

#[derive(Debug, Clone)]
struct ObsShape {}

impl Shape for ObsShape {
    fn shape() -> &'static [usize] {
        &[4]
    }
}

type ObsFilter = PyGymEnvObsRawFilter<ObsShape, f64>;
type ActFilter = PyGymEnvDiscreteActRawFilter;
type Obs = PyGymEnvObs<ObsShape, f64>;
type Act = PyGymEnvDiscreteAct;
type Env = PyGymEnv<Obs, Act, ObsFilter, ActFilter>;
type ObsBuffer = TchPyGymEnvObsBuffer<ObsShape, f64>;
type ActBuffer = TchPyGymEnvDiscreteActBuffer;

fn create_agent() -> impl Agent<Env> {
    let qnet = QNetwork::new(4, 2, 0.001);
    let replay_buffer = ReplayBuffer::<Env, ObsBuffer, ActBuffer>::new(10000, 1);
    let agent: DQN<Env, _, _, _> = DQN::new(
        qnet,
        replay_buffer)
        .opt_interval(OptInterval::Steps(50))
        .n_updates_per_opt(1)
        .min_transitions_warmup(100)
        .batch_size(64)
        .discount_factor(0.99)
        .tau(0.005);
    agent
}

fn create_env() -> Env {
    let obs_filter = ObsFilter::new();
    let act_filter = ActFilter::new();
    Env::new("CartPole-v0", obs_filter, act_filter).unwrap()
}

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    tch::manual_seed(42);

    let env = create_env();
    let env_eval = create_env();
    let agent = create_agent();
    let mut trainer = Trainer::new(
        env,
        env_eval,
        agent)
        .max_opts(1000)
        .n_opts_per_eval(50)
        .n_episodes_per_eval(5);

    trainer.train();
    trainer.get_agent().save("./examples/model/dqn_cartpole")?;

    let mut env = create_env();
    let mut agent = create_agent();
    env.set_render(true);
    agent.load("./examples/model/dqn_cartpole")?;
    agent.eval();
    util::eval(&mut env, &mut agent, 5, None);

    Ok(())
}
