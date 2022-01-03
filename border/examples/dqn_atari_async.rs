mod util_dqn_atari;
use anyhow::Result;
use border_async_trainer::{ActorManager, ActorManagerConfig, AsyncTrainer, AsyncTrainerConfig};
use border_atari_env::{
    BorderAtariAct, BorderAtariActRawFilter, BorderAtariEnv, BorderAtariEnvConfig, BorderAtariObs,
    BorderAtariObsRawFilter,
};
use border_core::{
    record::TensorboardRecorder,
    replay_buffer::{
        SimpleReplayBuffer, SimpleReplayBufferConfig, SimpleStepProcessor,
        SimpleStepProcessorConfig,
    },
    shape, Env as _,
};
use border_derive::{Act, SubBatch};
use border_tch_agent::{
    cnn::CNN,
    dqn::{DQNConfig, DQN as DQN_},
    TensorSubBatch,
};
use clap::{App, Arg, ArgMatches};
use std::{sync::{Arc, Mutex}, default::Default};
use util_dqn_atari::{model_dir as model_dir_, Params};
use crossbeam_channel::unbounded;

type ObsDtype = u8;
shape!(ObsShape, [4, 1, 84, 84]);

// #[derive(Debug, Clone, Obs)]
// struct Obs(BorderAtariObs);
type Obs = BorderAtariObs;

#[derive(Clone, SubBatch)]
struct ObsBatch(TensorSubBatch<ObsShape, ObsDtype>);

impl From<Obs> for ObsBatch {
    fn from(obs: Obs) -> Self {
        let tensor = obs.into();
        Self(TensorSubBatch::from_tensor(tensor))
    }
}

shape!(ActShape, [1]);

#[derive(SubBatch)]
struct ActBatch(TensorSubBatch<ActShape, i64>);

impl From<Act> for ActBatch {
    fn from(act: Act) -> Self {
        let tensor = act.into();
        Self(TensorSubBatch::from_tensor(tensor))
    }
}

// Wrap `BorderAtariAct` to make a new type.
// Act also implements Into<Tensor>.
// TODO: Consider to implement Into<Tensor> on BorderAtariAct when feature=tch.
#[derive(Debug, Clone, Act)]
struct Act(BorderAtariAct);

type ObsFilter = BorderAtariObsRawFilter<Obs>;
type ActFilter = BorderAtariActRawFilter<Act>;
type EnvConfig = BorderAtariEnvConfig<Obs, Act, ObsFilter, ActFilter>;
type Env_ = BorderAtariEnv<Obs, Act, ObsFilter, ActFilter>;
type StepProc_ = SimpleStepProcessor<Env_, ObsBatch, ActBatch>;
type ReplayBuffer_ = SimpleReplayBuffer<ObsBatch, ActBatch>;
type Agent_ = DQN_<Env_, CNN, ReplayBuffer_>;
type ActorManager_ = ActorManager<Agent_, Env_, ReplayBuffer_, StepProc_>;
type AsyncTrainer_ = AsyncTrainer<Agent_, Env_, ReplayBuffer_>;

fn env_config(name: impl Into<String>) -> EnvConfig {
    BorderAtariEnvConfig::default().name(name.into())
}

fn init<'a>() -> ArgMatches<'a> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    tch::manual_seed(42);

    let matches = App::new("dqn_atari_async")
        .version("0.1.0")
        .author("Taku Yoshioka <taku.yoshioka.4096@gmail.com>")
        .arg(
            Arg::with_name("name")
                .long("name")
                .takes_value(true)
                .required(true)
                .index(1)
                .help("The name of the atari environment (e.g., PongNoFrameskip-v4)"),
        )
        .arg(
            Arg::with_name("per")
                .long("per")
                .takes_value(false)
                .help("Train/play with prioritized experience replay"),
        )
        .arg(
            Arg::with_name("ddqn")
                .long("ddqn")
                .takes_value(false)
                .help("Train/play with double DQN"),
        )
        .arg(
            Arg::with_name("debug")
                .long("debug")
                .takes_value(false)
                .help("Run with debug configuration"),
        )
        .arg(
            Arg::with_name("show-config")
                .long("show-config")
                .takes_value(false)
                .help("Showing configuration loaded from files"),
        )
        .get_matches();

    matches
}

fn show_config(
    env_config: &EnvConfig,
    agent_config: &DQNConfig<CNN>,
    trainer_config: &AsyncTrainerConfig,
) {
    println!("Device: {:?}", tch::Device::cuda_if_available());
    println!("{}", serde_yaml::to_string(&env_config).unwrap());
    println!("{}", serde_yaml::to_string(&agent_config).unwrap());
    println!("{}", serde_yaml::to_string(&trainer_config).unwrap());
}

fn model_dir(matches: &ArgMatches) -> Result<String> {
    let name = matches
        .value_of("name")
        .expect("The name of the environment was not given")
        .to_string();
    let mut params = Params::default();

    if matches.is_present("ddqn") {
        params = params.ddqn();
    }

    if matches.is_present("per") {
        params = params.per();
    }

    if matches.is_present("debug") {
        params = params.debug();
    }

    let model_dir = model_dir_(name, &params)?;

    Ok(model_dir + "_async")
}

fn n_actions(env_config: &EnvConfig) -> Result<usize> {
    Ok(Env_::build(env_config, 0)?.get_num_actions_atari() as usize)
}

fn load_dqn_config<'a>(model_dir: impl Into<&'a str>) -> Result<DQNConfig<CNN>> {
    let config_path = format!("{}/agent.yaml", model_dir.into());
    DQNConfig::<CNN>::load(config_path)
}

fn load_async_trainer_config<'a>(model_dir: impl Into<&'a str>) -> Result<AsyncTrainerConfig> {
    let config_path = format!("{}/trainer.yaml", model_dir.into());
    // TrainerConfig::load(config_path)
    unimplemented!();
}

fn load_replay_buffer_config<'a>(
    model_dir: impl Into<&'a str>,
) -> Result<SimpleReplayBufferConfig> {
    let config_path = format!("{}/replay_buffer.yaml", model_dir.into());
    SimpleReplayBufferConfig::load(config_path)
}

fn train(matches: ArgMatches) -> Result<()> {
    let name = matches.value_of("name").unwrap();
    let model_dir = model_dir(&matches)?;
    let env_config_train = env_config(name);
    let n_actions = n_actions(&env_config_train)?;

    // Configurations
    let agent_config = load_dqn_config(model_dir.as_str())?.out_dim(n_actions as _);
    let agent_configs = vec![agent_config.clone(); 4];
    let env_config_eval = env_config(name).eval();
    let replay_buffer_config = load_replay_buffer_config(model_dir.as_str())?;
    let step_proc_config = SimpleStepProcessorConfig::default();
    let actor_man_config = ActorManagerConfig::default();
    let async_trainer_config = load_async_trainer_config(model_dir.as_str())?;

    if matches.is_present("show-config") {
        show_config(
            &env_config_train,
            &agent_config,
            // &actor_man_config,
            &async_trainer_config,
        );
    } else {
        let mut recorder = TensorboardRecorder::new(model_dir);

        // Shared flag to stop actor threads
        let stop = Arc::new(Mutex::new(false));

        // Creates channels
        let (item_s, item_r) = unbounded(); // items pushed to replay buffer
        let (model_s, model_r) = unbounded(); // model_info

        // guard for initialization of envs in multiple threads
        let guard_init_env = Arc::new(Mutex::new(true));

        // Actor manager and async trainer
        let mut actors = ActorManager_::build(
            &actor_man_config,
            &agent_configs,
            &env_config_train,
            &step_proc_config,
            item_s,
            model_r,
            stop.clone(),
        );
        let mut trainer = AsyncTrainer_::build(
            &async_trainer_config,
            &agent_config,
            &env_config_eval,
            &replay_buffer_config,
            item_r,
            model_s,
            stop.clone(),
        );

        // Starts sampling and training
        actors.run(guard_init_env.clone());
        trainer.train(&mut recorder, guard_init_env);

        actors.stop_and_join();
    }

    Ok(())
}

fn main() -> Result<()> {
    let matches = init();

    train(matches)?;

    Ok(())
}
