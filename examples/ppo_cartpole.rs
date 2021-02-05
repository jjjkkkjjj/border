use std::error::Error;
use lrr::{
    core::{Trainer, Agent, util},
    py_gym_env::{
        PyGymEnv,
        obs::{PyGymEnvObs, PyGymEnvObsRawFilter},
        act_d::{PyGymEnvDiscreteAct, PyGymEnvDiscreteActRawFilter}
    },
    agents::{
        OptInterval,
        tch::{
            Shape, PPODiscrete, ReplayBuffer,
            model::StateValueAndDiscreteActProb,
            py_gym_env::{
                obs::TchPyGymEnvObsBuffer,
                act_d::TchPyGymEnvDiscreteActBuffer
            }
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
    let mh_model = StateValueAndDiscreteActProb::new(4, 2, 0.0001);
    let replay_buffer = ReplayBuffer::new(200, 1);
    let agent: PPODiscrete<Env, _, ObsBuffer, ActBuffer> = PPODiscrete::new(
        mh_model, OptInterval::Steps(200), replay_buffer)
        .n_updates_per_opt(1)
        .batch_size(200)
        .discount_factor(0.99);
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
        .max_opts(250)
        .n_opts_per_eval(10)
        .n_episodes_per_eval(5);

    trainer.train();
    trainer.get_agent().save("./examples/model/ppo_cartpole")?;

    let mut env = create_env();
    let mut agent = create_agent();
    env.set_render(true);
    agent.load("./examples/model/ppo_cartpole")?;
    agent.eval();
    util::eval(&mut env, &mut agent, 5, None);

    Ok(())
}
