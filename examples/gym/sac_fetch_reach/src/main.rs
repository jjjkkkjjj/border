use anyhow::Result;
use border_candle_agent::{
    mlp::{Mlp, Mlp2, MlpConfig},
    opt::OptimizerConfig,
    sac::{EntCoefMode, Sac, SacConfig},
    util::{actor::GaussianActorConfig, critic::MultiCriticConfig, CriticLoss},
    Activation,
};
use border_core::{
    generic_replay_buffer::{
        SimpleReplayBuffer, SimpleReplayBufferConfig, SimpleStepProcessor,
        SimpleStepProcessorConfig,
    },
    record::Recorder,
    Agent, Configurable, DefaultEvaluator, Env as _, Evaluator as _, ReplayBufferBase,
    StepProcessor, Trainer, TrainerConfig,
};
use border_mlflow_tracking::MlflowTrackingClient;
use border_py_gym_env::{
    candle::{
        // tensor_converter::{TensorConverter, TensorConverterConfig},
        NdarrayDictObsConverter,
        NdarrayDictObsConverterConfig,
        TensorBatch,
    },
    GymEnv, GymEnvConfig,
};
use border_tensorboard::TensorboardRecorder;
use candle_core::Device;
use clap::Parser;
use serde::Serialize;

type Env = GymEnv<NdarrayDictObsConverter>;
type ReplayBuffer = SimpleReplayBuffer<TensorBatch, TensorBatch>;
type StepProc = SimpleStepProcessor<Env, TensorBatch, TensorBatch>;
type Evaluator = DefaultEvaluator<Env>;

const DIM_OBS: i64 = 16;
const DIM_ACT: i64 = 4;
const LR_ACTOR: f64 = 3e-4;
const LR_CRITIC: f64 = 3e-4;
const BATCH_SIZE: usize = 256;
const N_TRANSITIONS_WARMUP: usize = 1000;
const OPT_INTERVAL: usize = 1;
const MAX_OPTS: usize = 20_000_000;
const EVAL_INTERVAL: usize = 5_000;
const REPLAY_BUFFER_CAPACITY: usize = 131_072;
const N_EPISODES_PER_EVAL: usize = 5;
const N_CRITICS: usize = 2;
const TAU: f64 = 0.05;
const TARGET_ENTROPY: f64 = -(DIM_ACT as f64);
const LR_ENT_COEF: f64 = 3e-4;
const CRITIC_LOSS: CriticLoss = CriticLoss::SmoothL1;
const ENV_NAME: &str = "FetchReach-v4";
const MODEL_DIR: &str = "./model/candle/sac_fetch_reach/";
const MLFLOW_EXPERIMENT_NAME: &str = "Fetch";
const MLFLOW_RUN_NAME: &str = "sac-fetch-reach";
const MLFLOW_TAGS: &[(&str, &str)] = &[("env", "reach"), ("algo", "sac"), ("backend", "candle")];

/// Train/eval SAC agent in fetch reach environment
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Train SAC agent, not evaluate
    #[arg(short, long, default_value_t = false)]
    train: bool,

    /// Evaluate SAC agent, not train
    #[arg(short, long, default_value_t = false)]
    eval: bool,

    /// Log metrics with MLflow
    #[arg(short, long, default_value_t = false)]
    mlflow: bool,
}

fn create_env_config(render: bool) -> Result<GymEnvConfig<NdarrayDictObsConverter>> {
    let mut env_config = GymEnvConfig::default()
        .name(ENV_NAME.to_string())
        .converter_config(NdarrayDictObsConverterConfig {
            keys: vec![
                "observation".to_string(),
                "desired_goal".to_string(),
                "achieved_goal".to_string(),
            ],
        });

    if render {
        env_config = env_config
            .render_mode(Some("human".to_string()))
            .set_wait_in_millis(10);
    }

    Ok(env_config)
}

fn create_actor_config(in_dim: i64, out_dim: i64) -> GaussianActorConfig<MlpConfig> {
    GaussianActorConfig::default()
        .opt_config(OptimizerConfig::Adam { lr: LR_ACTOR })
        .out_dim(out_dim)
        // .action_limit(args.action_limit())
        .policy_config(MlpConfig::new(
            in_dim,
            vec![256, 256, 256],
            out_dim,
            Activation::None,
        ))
}

fn create_critic_config(in_dim: i64, out_dim: i64) -> MultiCriticConfig<MlpConfig> {
    MultiCriticConfig::default()
        .opt_config(OptimizerConfig::Adam { lr: LR_CRITIC })
        .q_config(MlpConfig::new(
            in_dim + out_dim,
            vec![256, 256, 256],
            1,
            Activation::None,
        ))
        .n_nets(N_CRITICS)
        .tau(TAU)
}

fn create_agent_config(in_dim: i64, out_dim: i64) -> Result<SacConfig<Mlp, Mlp2>> {
    let device = Device::cuda_if_available(0)?;
    let actor_config = create_actor_config(in_dim, out_dim);
    let critic_config = create_critic_config(in_dim, out_dim);
    let sac_config = SacConfig::default()
        .batch_size(BATCH_SIZE)
        .actor_config(actor_config)
        .critic_config(critic_config)
        .ent_coef_mode(EntCoefMode::Auto(TARGET_ENTROPY, LR_ENT_COEF))
        .critic_loss(CRITIC_LOSS)
        .device(device);

    Ok(sac_config)
}

// fn create_agent_config(in_dim: i64, out_dim: i64) -> Result<SacConfig<Mlp, Mlp2>> {
//     let target_ent = TARGET_ENTROPY;
//     let device = Device::cuda_if_available(0)?;
//     let actor_config = ActorConfig::default()
//         .opt_config(OptimizerConfig::default().learning_rate(LR_ACTOR))
//         .out_dim(out_dim)
//         .pi_config(MlpConfig::new(in_dim, vec![256, 256, 256], out_dim, Activation::None));
//     let critic_config = CriticConfig::default()
//         .opt_config(OptimizerConfig::default().learning_rate(LR_CRITIC))
//         .q_config(MlpConfig::new(
//             in_dim + out_dim,
//             vec![256, 256, 256],
//             1,
//             Activation::None,
//         ));
//     let sac_config = SacConfig::default()
//         .batch_size(BATCH_SIZE)
//         .actor_config(actor_config)
//         .critic_config(critic_config)
//         .tau(TAU)
//         .critic_loss(CRITIC_LOSS)
//         .n_critics(N_CRITICS)
//         .ent_coef_mode(EntCoefMode::Auto(target_ent, LR_ENT_COEF))
//         .device(device);

//     Ok(sac_config)
// }

/// `model_dir` - Directory where TFRecord and model parameters are saved with
///               [`TensorboardRecorder`].
/// `config` - Configuration parameters for a run of MLflow. These are used for
///            recording purpose only when a new run is created.
fn create_recorder(
    args: &Args,
    model_dir: &str,
    config: Option<&SacFetchReachConfig>,
) -> Result<Box<dyn Recorder<Env, ReplayBuffer>>> {
    match args.mlflow {
        true => {
            let client = MlflowTrackingClient::new("http://localhost:8080")
                .set_experiment(MLFLOW_EXPERIMENT_NAME)?;
            let recorder_run = client.create_recorder(MLFLOW_RUN_NAME)?;
            if let Some(config) = config {
                recorder_run.log_params(config)?;
                recorder_run.set_tags(MLFLOW_TAGS)?;
            }
            Ok(Box::new(recorder_run))
        }
        false => Ok(Box::new(TensorboardRecorder::new(
            model_dir, model_dir, false,
        ))),
    }
}

#[derive(Serialize)]
pub struct SacFetchReachConfig {
    pub env_config: GymEnvConfig<NdarrayDictObsConverter>,
    pub agent_config: SacConfig<Mlp, Mlp2>,
    pub trainer_config: TrainerConfig,
}

impl SacFetchReachConfig {
    pub fn new(in_dim: i64, out_dim: i64, max_opts: usize, eval_interval: usize) -> Result<Self> {
        let env_config = create_env_config(false)?;
        let agent_config = create_agent_config(in_dim, out_dim)?;
        let trainer_config = TrainerConfig::default()
            .max_opts(max_opts)
            .opt_interval(OPT_INTERVAL)
            .eval_interval(eval_interval)
            .record_agent_info_interval(EVAL_INTERVAL)
            .record_compute_cost_interval(EVAL_INTERVAL)
            .flush_record_interval(EVAL_INTERVAL)
            .save_interval(EVAL_INTERVAL)
            .warmup_period(N_TRANSITIONS_WARMUP);
        let config = Self {
            env_config,
            agent_config,
            trainer_config,
        };

        Ok(config)
    }
}

fn train(args: &Args, max_opts: usize, model_dir: &str, eval_interval: usize) -> Result<()> {
    let config = SacFetchReachConfig::new(DIM_OBS, DIM_ACT, max_opts, eval_interval)?;
    let step_proc_config = SimpleStepProcessorConfig {};
    let replay_buffer_config = SimpleReplayBufferConfig::default().capacity(REPLAY_BUFFER_CAPACITY);
    let mut recorder = create_recorder(&args, model_dir, Some(&config))?;
    let mut trainer = Trainer::build(config.trainer_config.clone());

    let env = Env::build(&config.env_config, 0)?;
    let step_proc = StepProc::build(&step_proc_config);
    let mut agent = Box::new(Sac::build(config.agent_config)) as _;
    let mut buffer = ReplayBuffer::build(&replay_buffer_config);
    let mut evaluator = Evaluator::new(&config.env_config, 0, N_EPISODES_PER_EVAL)?;

    trainer.train(
        env,
        step_proc,
        &mut agent,
        &mut buffer,
        &mut recorder,
        &mut evaluator,
    )?;

    Ok(())
}

fn eval(args: &Args, model_dir: &str, render: bool) -> Result<()> {
    let env_config = create_env_config(render)?;
    let mut agent: Box<dyn Agent<_, ReplayBuffer>> = {
        let agent_config = create_agent_config(DIM_OBS, DIM_ACT)?;
        let mut agent = Box::new(Sac::build(agent_config)) as _;
        let recorder = create_recorder(&args, model_dir, None)?;
        recorder.load_model("best".as_ref(), &mut agent)?;
        agent.eval();
        agent
    };
    let _ = Evaluator::new(&env_config, 0, 5)?.evaluate(&mut agent);

    Ok(())
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Args::parse();

    if args.train {
        train(&args, MAX_OPTS, MODEL_DIR, EVAL_INTERVAL)?;
    } else if args.eval {
        eval(&args, MODEL_DIR, true)?;
    } else {
        train(&args, MAX_OPTS, MODEL_DIR, EVAL_INTERVAL)?;
        eval(&args, MODEL_DIR, true)?;
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use tempdir::TempDir;

    #[test]
    fn test_sac_fetch_reach() -> Result<()> {
        let tmp_dir = TempDir::new("sac_fetch_reach")?;
        let model_dir = match tmp_dir.as_ref().to_str() {
            Some(s) => s,
            None => panic!("Failed to get string of temporary directory"),
        };
        let args = Args {
            train: false,
            eval: false,
            mlflow: false,
        };
        train(&args, 100, model_dir, 100)?;
        eval(&args, model_dir, false)?;
        Ok(())
    }
}
