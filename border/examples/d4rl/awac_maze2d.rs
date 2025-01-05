use anyhow::Result;
use border_candle_agent::{
    awac::{ActorConfig, Awac, AwacConfig, CriticConfig},
    mlp::{Mlp, Mlp2, MlpConfig},
    opt::OptimizerConfig,
    Activation,
};
use border_core::{
    generic_replay_buffer::{BatchBase, SimpleReplayBuffer},
    record::Recorder,
    Agent, Configurable, Env, Evaluator, ExperienceBufferBase, ReplayBufferBase, Trainer,
    TrainerConfig, TransitionBatch,
};
use border_minari::{
    d4rl::pointmaze::{
        candle::{PointMazeConverter, PointMazeConverterConfig},
        PointMazeEvaluator,
    },
    MinariConverter, MinariDataset, MinariEnv,
};
use border_mlflow_tracking::MlflowTrackingClient;
use border_tensorboard::TensorboardRecorder;
use candle_core::{Device, Tensor};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

const MODEL_DIR: &str = "border/examples/d4rl/model/candle/awac_maze2d";
const ENV_NAME: &str = "D4RL/pointmaze/umaze-v2";
const MLFLOW_EXPERIMENT_NAME: &str = "D4RL";
const MLFLOW_RUN_NAME: &str = "awac_maze2d_candle";
const MLFLOW_TAGS: &[(&str, &str)] = &[
    ("env", "pointmaze/umaze-v2"),
    ("algo", "awac"),
    ("backend", "candle"),
];

/// Train AWAC agent in maze2d environment
#[derive(Parser, Debug, Serialize, Deserialize)]
#[command(version, about)]
struct Args {
    /// train or eval
    #[arg(long)]
    mode: String,

    // /// Waiting time in milliseconds between frames when evaluation
    // #[arg(long, default_value_t = 25)]
    // wait: u64,
    /// Log metrics with MLflow
    #[arg(long, default_value_t = false)]
    mlflow: bool,

    /// The number of optimization steps
    #[arg(long, default_value_t = 1000000)]
    max_opts: usize,

    /// Interval of evaluation
    #[arg(long, default_value_t = 10000)]
    eval_interval: usize,

    /// The number of evaluation episodes
    #[arg(long, default_value_t = 5)]
    eval_episodes: usize,

    /// If true, goal position is included in observation
    #[arg(long, default_value_t = false)]
    include_goal: bool,

    /// Batch size
    #[arg(long, default_value_t = 256)]
    batch_size: usize,
}

#[derive(Serialize)]
struct AwacMaze2dConfig {
    args: Args,
    trainer_config: TrainerConfig,
    agent_config: AwacConfig<Mlp, Mlp2>,
}

impl AwacMaze2dConfig {
    fn new(args: Args) -> Self {
        let trainer_config = TrainerConfig::default()
            .max_opts(args.max_opts)
            .eval_interval(args.eval_interval)
            .flush_record_interval(args.eval_interval)
            .record_agent_info_interval(args.eval_interval);
        let agent_config = create_awac_config(&args).unwrap();
        Self {
            args,
            trainer_config,
            agent_config,
        }
    }
}

fn create_awac_config(args: &Args) -> Result<AwacConfig<Mlp, Mlp2>> {
    // Actor/Critic learning rate
    let lr = 0.0003;

    // Dimensions of observation and action
    let dim_obs = match args.include_goal {
        true => 6,
        false => 4,
    };
    let dim_act = 2;

    // Actor/critic configs
    let actor_config = ActorConfig::default()
        .opt_config(OptimizerConfig::default().learning_rate(lr))
        .out_dim(dim_act)
        .pi_config(MlpConfig::new(
            dim_obs,
            vec![256, 256],
            dim_act,
            Activation::None,
        ));
    let critic_config = CriticConfig::default()
        .opt_config(OptimizerConfig::default().learning_rate(lr))
        .q_config(MlpConfig::new(
            dim_obs + dim_act,
            vec![256, 256],
            1,
            Activation::None,
        ));

    // AWAC config
    let awac_config = AwacConfig::<Mlp, Mlp2>::default()
        .actor_config(actor_config)
        .critic_config(critic_config)
        .device(Device::cuda_if_available(0)?)
        .batch_size(args.batch_size);
    Ok(awac_config)
}

fn create_trainer(config: &AwacMaze2dConfig) -> Trainer {
    log::info!("Create trainer");
    Trainer::build(config.trainer_config.clone())
}

fn create_agent<E, R>(config: &AwacMaze2dConfig) -> Box<dyn Agent<E, R>>
where
    E: Env + 'static,
    E::Obs: Into<Tensor>,
    E::Act: From<Tensor> + Into<Tensor>,
    R: ReplayBufferBase + 'static,
    R::Batch: TransitionBatch,
    <R::Batch as TransitionBatch>::ObsBatch: Into<Tensor> + Clone,
    <R::Batch as TransitionBatch>::ActBatch: Into<Tensor> + Clone,
{
    log::info!("Create agent");
    Box::new(Awac::build(config.agent_config.clone()))
}

fn create_replay_buffer<T>(
    converter: &T,
    dataset: &MinariDataset,
) -> Result<SimpleReplayBuffer<T::ObsBatch, T::ActBatch>>
where
    T: MinariConverter,
    T::ObsBatch: BatchBase + Debug + Into<Tensor>,
    T::ActBatch: BatchBase + Debug + Into<Tensor>,
{
    log::info!("Create replay buffer");
    let buffer = dataset.create_replay_buffer(converter, None)?;
    log::info!("{} samples", buffer.len());
    Ok(buffer)
}

fn create_recorder<E, R>(config: &AwacMaze2dConfig) -> Result<Box<dyn Recorder<E, R>>>
where
    E: Env + 'static,
    R: ReplayBufferBase + 'static,
{
    log::info!("Create recorder");
    match config.args.mlflow {
        true => {
            let client = MlflowTrackingClient::new("http://localhost:8080")
                .set_experiment(MLFLOW_EXPERIMENT_NAME)?;
            let recorder_run = client.create_recorder(MLFLOW_RUN_NAME)?;
            recorder_run.log_params(config)?;
            recorder_run.set_tags(MLFLOW_TAGS)?;
            Ok(Box::new(recorder_run))
        }
        false => Ok(Box::new(TensorboardRecorder::new(
            MODEL_DIR, MODEL_DIR, false,
        ))),
    }
}

fn create_evaluator<T>(
    args: &Args,
    converter: T,
    dataset: &MinariDataset,
) -> Result<impl Evaluator<MinariEnv<T>>>
where
    T: MinariConverter,
{
    // Create evaluator
    log::info!("Create evaluator");
    let env = dataset.recover_environment(converter, true, None)?;
    PointMazeEvaluator::new(env, args.eval_episodes)
}

fn train<T>(config: AwacMaze2dConfig, dataset: MinariDataset, converter: T) -> Result<()>
where
    T: MinariConverter + 'static,
    T::Obs: std::fmt::Debug + Into<Tensor>,
    T::Act: std::fmt::Debug + From<Tensor> + Into<Tensor>,
    T::ObsBatch: std::fmt::Debug + Into<Tensor> + 'static + Clone,
    T::ActBatch: std::fmt::Debug + Into<Tensor> + 'static + Clone,
{
    let mut trainer = create_trainer(&config);
    let mut agent = create_agent(&config);
    let mut buffer = create_replay_buffer(&converter, &dataset)?;
    let mut recorder = create_recorder(&config)?;
    let mut evaluator = create_evaluator(&config.args, converter, &dataset)?;

    log::info!("Start training");
    let _ = trainer.train_offline(&mut agent, &mut buffer, &mut recorder, &mut evaluator);

    Ok(())
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    let config = AwacMaze2dConfig::new(args);
    let dataset = MinariDataset::load_dataset(ENV_NAME, true)?;
    let converter = PointMazeConverter::new(PointMazeConverterConfig {
        // Include goal position in observation
        include_goal: config.args.include_goal,
    });

    train(config, dataset, converter)
}
