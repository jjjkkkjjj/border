use std::{convert::TryFrom, fs::File, iter::FromIterator};
use clap::{Arg, App};
use serde::Serialize;
use anyhow::Result;
use csv::WriterBuilder;
use tch::nn;

use lrr::{
    core::{
        Agent, Trainer,
        record::{BufferedRecorder, Record, RecordValue, TensorboardRecorder},
        util::eval_with_recorder,
    },
    env::py_gym_env::{
        Shape, PyGymEnv,
        obs::{PyGymEnvObs, PyGymEnvObsRawFilter},
        act_c::{PyGymEnvContinuousAct, PyGymEnvContinuousActRawFilter},
        tch::{
            obs::TchPyGymEnvObsBuffer,
            act_c::TchPyGymEnvContinuousActBuffer
        }
    },
    agent::{
        OptInterval,
        tch::{
            SAC, ReplayBuffer,
            model::{Model1_2, Model2_1},
        }
    }
};

#[derive(Debug, Clone)]
struct ObsShape {}

impl Shape for ObsShape {
    fn shape() -> &'static [usize] {
        &[8]
    }
}

#[derive(Debug, Clone)]
struct ActShape {}

impl Shape for ActShape {
    fn shape() -> &'static [usize] {
        &[2]
    }

    fn squeeze_first_dim() -> bool {
        true
    }
}

fn create_actor() -> Model1_2 {
    let network_fn = |p: &nn::Path, in_dim, hidden_dim| nn::seq()
        .add(nn::linear(p / "al1", in_dim as _, 64, Default::default()))
        .add_fn(|xs| xs.relu())
        .add(nn::linear(p / "al2", 64, 64, Default::default()))
        .add_fn(|xs| xs.relu())
        .add(nn::linear(p / "al3", 64, hidden_dim as _, Default::default()));
    Model1_2::new(8, 64, 2, 3e-4, network_fn)
}

fn create_critic() -> Model2_1 {
    let network_fn = |p: &nn::Path, in_dim, out_dim| nn::seq()
        .add(nn::linear(p / "cl1", in_dim as _, 64, Default::default()))
        .add_fn(|xs| xs.relu())
        .add(nn::linear(p / "cl2", 64, 64, Default::default()))
        .add_fn(|xs| xs.relu())
        .add(nn::linear(p / "cl3", 64, out_dim as _, Default::default()));
        Model2_1::new(10, 1, 3e-4, network_fn)
}

type ObsFilter = PyGymEnvObsRawFilter<ObsShape, f32>;
type ActFilter = PyGymEnvContinuousActRawFilter;
type Obs = PyGymEnvObs<ObsShape, f32>;
type Act = PyGymEnvContinuousAct<ActShape>;
type Env = PyGymEnv<Obs, Act, ObsFilter, ActFilter>;
type ObsBuffer = TchPyGymEnvObsBuffer<ObsShape, f32>;
type ActBuffer = TchPyGymEnvContinuousActBuffer<ActShape>;

fn create_agent() -> impl Agent<Env> {
    let actor = create_actor();
    let critic = create_critic();
    let replay_buffer = ReplayBuffer::<Env, ObsBuffer, ActBuffer>::new(100_000, 1);
    let agent: SAC<Env, _, _, _, _> = SAC::new(
        critic,
        actor,
        replay_buffer)
        .opt_interval(OptInterval::Steps(1))
        .n_updates_per_opt(1)
        .min_transitions_warmup(1000)
        .batch_size(128)
        .discount_factor(0.99)
        .tau(0.001)
        .alpha(0.5);
    agent
}

fn create_env() -> Env {
    let obs_filter = ObsFilter::default(); //new();
    let act_filter = ActFilter::default(); //new();
    Env::new("LunarLanderContinuous-v2", obs_filter, act_filter)
        .unwrap()
        .max_steps(Some(1000))
}

#[derive(Debug, Serialize)]
struct LunarlanderRecord {
    episode: usize,
    step: usize,
    reward: f32,
    obs: Vec<f32>,
    act: Vec<f32>,
}

impl TryFrom<&Record> for LunarlanderRecord {
    type Error = anyhow::Error;

    fn try_from(record: &Record) -> Result<Self> {
        Ok(Self {
            episode: record.get_scalar("episode")? as _,
            step: record.get_scalar("step")? as _,
            reward: record.get_scalar("reward")?,
            obs: Vec::from_iter(record.get_array1("obs")?.iter().cloned()),
            act: Vec::from_iter(record.get_array1("act")?.iter().cloned()),
        })
    }
}

fn main() -> Result<()> {
    let matches = App::new("sac_lunarlander_cont")
    .version("0.1.0")
    .author("Taku Yoshioka <taku.yoshioka.4096@gmail.com>")
    .arg(Arg::with_name("skip training")
        .long("skip_training")
        .takes_value(false)
        .help("Skip training"))
    .get_matches();

    env_logger::init();
    tch::manual_seed(42);

    if !matches.is_present("skip training") {
        let env = create_env();
        let env_eval = create_env();
        let agent = create_agent();
        let mut trainer = Trainer::new(
            env,
            env_eval,
            agent)
            .max_opts(200_000)
            .eval_interval(10_000)
            .n_episodes_per_eval(5);
        let mut recorder = TensorboardRecorder::new("./examples/model/sac_lunarlander_cont");
    
        trainer.train(&mut recorder);
        trainer.get_agent().save("./examples/model/sac_lunarlander_cont").unwrap();    
    }

    let mut env = create_env();
    let mut agent = create_agent();
    let mut recorder = BufferedRecorder::new();
    env.set_render(true);
    agent.load("./examples/model/sac_lunarlander_cont").unwrap();
    agent.eval();

    eval_with_recorder(&mut env, &mut agent, 5, &mut recorder);

    // Vec<_> field in a struct does not support writing a header in csv crate, so disable it.
    let mut wtr = csv::WriterBuilder::new().has_headers(false)
        .from_writer(File::create("examples/model/sac_lunarlander_eval.csv")?);
    for record in recorder.iter() {
        wtr.serialize(LunarlanderRecord::try_from(record)?)?;
    }

    Ok(())
}
