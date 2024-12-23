use anyhow::Result;
use border_candle_agent::{
    dqn::{Dqn, DqnConfig, DqnModelConfig},
    mlp::{Mlp, MlpConfig},
    opt::OptimizerConfig,
    util::{arrayd_to_tensor, vec_to_tensor, CriticLoss},
    TensorBatch,
};
use border_core::{
    generic_replay_buffer::{
        BatchBase, SimpleReplayBuffer, SimpleReplayBufferConfig, SimpleStepProcessor,
        SimpleStepProcessorConfig,
    },
    record::Recorder,
    Agent, Configurable, DefaultEvaluator, Env as _, Evaluator as _, ReplayBufferBase,
    StepProcessor, Trainer, TrainerConfig,
};
use border_mlflow_tracking::MlflowTrackingClient;
use border_py_gym_env::{
    ArrayObsFilter, DiscreteActFilter, GymActFilter, GymEnv, GymEnvConfig, GymObsFilter,
};
use border_tensorboard::TensorboardRecorder;
use candle_core::{Device, Tensor};
use clap::Parser;
use ndarray::ArrayD;
use serde::Serialize;

const DIM_OBS: i64 = 4;
const DIM_ACT: i64 = 2;
const LR_CRITIC: f64 = 0.001;
const DISCOUNT_FACTOR: f64 = 0.99;
const BATCH_SIZE: usize = 64;
const WARMUP_PERIOD: usize = 100;
const N_UPDATES_PER_OPT: usize = 1;
const TAU: f64 = 0.01;
const OPT_INTERVAL: usize = 1;
const MAX_OPTS: usize = 10000;
const EVAL_INTERVAL: usize = 1000;
const REPLAY_BUFFER_CAPACITY: usize = 10000;
const N_EPISODES_PER_EVAL: usize = 5;
const CRITIC_LOSS: CriticLoss = CriticLoss::Mse;
const ENV_NAME: &str = "CartPole-v0";
const MODEL_DIR: &str = "./border/examples/gym/model/candle/dqn_cartpole";
const MLFLOW_EXPERIMENT_NAME: &str = "Gym";
const MLFLOW_RUN_NAME: &str = "dqn_cartpole_candle";
const MLFLOW_TAGS: &[(&str, &str)] = &[("env", "cartpole"), ("algo", "dqn"), ("backend", "candle")];

mod obs_act_types {
    use super::*;

    #[derive(Clone, Debug)]
    pub struct Obs(ArrayD<f32>);

    impl border_core::Obs for Obs {
        fn len(&self) -> usize {
            self.0.shape()[0]
        }
    }

    impl From<ArrayD<f32>> for Obs {
        fn from(obs: ArrayD<f32>) -> Self {
            Obs(obs)
        }
    }

    impl From<Obs> for Tensor {
        fn from(obs: Obs) -> Tensor {
            arrayd_to_tensor::<_, f32>(obs.0, false).unwrap()
        }
    }

    pub struct ObsBatch(TensorBatch);

    impl BatchBase for ObsBatch {
        fn new(capacity: usize) -> Self {
            Self(TensorBatch::new(capacity))
        }

        fn push(&mut self, i: usize, data: Self) {
            self.0.push(i, data.0)
        }

        fn sample(&self, ixs: &Vec<usize>) -> Self {
            let buf = self.0.sample(ixs);
            Self(buf)
        }
    }

    impl From<Obs> for ObsBatch {
        fn from(obs: Obs) -> Self {
            let tensor = obs.into();
            Self(TensorBatch::from_tensor(tensor))
        }
    }

    impl From<ObsBatch> for Tensor {
        fn from(b: ObsBatch) -> Self {
            b.0.into()
        }
    }

    #[derive(Clone, Debug)]
    pub struct Act(Vec<i32>);

    impl border_core::Act for Act {}

    impl From<Act> for Vec<i32> {
        fn from(value: Act) -> Self {
            value.0
        }
    }

    impl From<Tensor> for Act {
        // `t` must be a 1-dimentional tensor of `i64`
        fn from(t: Tensor) -> Self {
            let data = t.to_vec1::<i64>().expect("Failed to convert Tensor to Act");
            let data = data.iter().map(|&e| e as i32).collect();
            Self(data)
        }
    }

    pub struct ActBatch(TensorBatch);

    impl BatchBase for ActBatch {
        fn new(capacity: usize) -> Self {
            Self(TensorBatch::new(capacity))
        }

        fn push(&mut self, i: usize, data: Self) {
            self.0.push(i, data.0)
        }

        fn sample(&self, ixs: &Vec<usize>) -> Self {
            let buf = self.0.sample(ixs);
            Self(buf)
        }
    }

    impl From<Act> for ActBatch {
        fn from(act: Act) -> Self {
            let t =
                vec_to_tensor::<_, i64>(act.0, true).expect("Failed to convert Act to ActBatch");
            Self(TensorBatch::from_tensor(t))
        }
    }

    // Required by Dqn
    impl From<ActBatch> for Tensor {
        fn from(act: ActBatch) -> Self {
            act.0.into()
        }
    }

    type PyObsDtype = f32;
    pub type ObsFilter = ArrayObsFilter<PyObsDtype, f32, Obs>;
    pub type ActFilter = DiscreteActFilter<Act>;
    pub type EnvConfig = GymEnvConfig<Obs, Act, ObsFilter, ActFilter>;
    pub type Env = GymEnv<Obs, Act, ObsFilter, ActFilter>;
    pub type StepProc = SimpleStepProcessor<Env, ObsBatch, ActBatch>;
    pub type ReplayBuffer = SimpleReplayBuffer<ObsBatch, ActBatch>;
    pub type Evaluator = DefaultEvaluator<Env>;
}

use obs_act_types::*;

mod config {
    use super::*;

    #[derive(Serialize)]
    pub struct DqnCartpoleConfig {
        pub env_config: EnvConfig,
        pub agent_config: DqnConfig<Mlp>,
        pub trainer_config: TrainerConfig,
    }

    impl DqnCartpoleConfig {
        pub fn new(in_dim: i64, out_dim: i64, max_opts: usize, eval_interval: usize) -> Self {
            let env_config = create_env_config(false);
            let agent_config = create_agent_config(in_dim, out_dim);
            let trainer_config = TrainerConfig::default()
                .max_opts(max_opts)
                .opt_interval(OPT_INTERVAL)
                .eval_interval(eval_interval)
                .record_agent_info_interval(EVAL_INTERVAL)
                .record_compute_cost_interval(EVAL_INTERVAL)
                .flush_record_interval(EVAL_INTERVAL)
                .save_interval(EVAL_INTERVAL)
                .warmup_period(WARMUP_PERIOD);
            Self {
                env_config,
                agent_config,
                trainer_config,
            }
        }
    }

    pub fn create_env_config(render: bool) -> EnvConfig {
        let mut env_config = EnvConfig::default()
            .name(ENV_NAME.to_string())
            .obs_filter_config(ObsFilter::default_config())
            .act_filter_config(ActFilter::default_config());

        if render {
            env_config = env_config
                .render_mode(Some("human".to_string()))
                .set_wait_in_millis(10);
        }
        env_config
    }

    pub fn create_agent_config(in_dim: i64, out_dim: i64) -> DqnConfig<Mlp> {
        let device = Device::cuda_if_available(0).unwrap();
        let opt_config = OptimizerConfig::default().learning_rate(LR_CRITIC);
        let mlp_config = MlpConfig::new(in_dim, vec![256, 256], out_dim, false);
        let model_config = DqnModelConfig::default()
            .q_config(mlp_config)
            .out_dim(out_dim)
            .opt_config(opt_config);
        DqnConfig::default()
            .n_updates_per_opt(N_UPDATES_PER_OPT)
            .batch_size(BATCH_SIZE)
            .discount_factor(DISCOUNT_FACTOR)
            .tau(TAU)
            .model_config(model_config)
            .device(device)
            .critic_loss(CRITIC_LOSS)
    }
}

use config::{create_agent_config, create_env_config, DqnCartpoleConfig};

/// `model_dir` - Directory where TFRecord and model parameters are saved with
///               [`TensorboardRecorder`].
/// `config` - Configuration parameters for a run of MLflow. These are used for
///            recording purpose only when a new run is created.
fn create_recorder(
    args: &Args,
    model_dir: &str,
    config: Option<&DqnCartpoleConfig>,
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

/// Train/eval DQN agent in cartpole environment
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Train DQN agent, not evaluate
    #[arg(short, long, default_value_t = false)]
    train: bool,

    /// Evaluate DQN agent, not train
    #[arg(short, long, default_value_t = false)]
    eval: bool,

    /// Log metrics with MLflow
    #[arg(short, long, default_value_t = false)]
    mlflow: bool,
}

fn train(args: &Args, max_opts: usize, model_dir: &str, eval_interval: usize) -> Result<()> {
    let config = DqnCartpoleConfig::new(DIM_OBS, DIM_ACT, max_opts, eval_interval);
    let step_proc_config = SimpleStepProcessorConfig {};
    let replay_buffer_config = SimpleReplayBufferConfig::default().capacity(REPLAY_BUFFER_CAPACITY);
    let mut recorder = create_recorder(&args, model_dir, Some(&config))?;
    let mut trainer = Trainer::build(config.trainer_config.clone());

    let env = Env::build(&config.env_config, 0)?;
    let step_proc = StepProc::build(&step_proc_config);
    let mut agent = Box::new(Dqn::build(config.agent_config)) as _;
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
    let env_config = create_env_config(render);
    let mut agent: Box<dyn Agent<_, ReplayBuffer>> = {
        let agent_config = create_agent_config(DIM_OBS, DIM_ACT);
        let mut agent = Box::new(Dqn::build(agent_config)) as _;
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
    // TODO: set seed

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
mod tests {
    use super::*;
    use tempdir::TempDir;

    #[test]
    fn test_dqn_cartpole() -> Result<()> {
        let tmp_dir = TempDir::new("dqn_cartpole")?;
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
