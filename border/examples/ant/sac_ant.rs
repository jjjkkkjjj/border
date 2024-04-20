use anyhow::Result;
use border::util::get_model_from_url;
use border_candle_agent::{
    mlp::{Mlp, Mlp2, MlpConfig},
    opt::OptimizerConfig,
    sac::{ActorConfig, CriticConfig, EntCoefMode, Sac, SacConfig},
    util::CriticLoss,
    TensorSubBatch,
};
use border_core::{
    record::AggregateRecorder,
    replay_buffer::{
        SimpleReplayBuffer, SimpleReplayBufferConfig, SimpleStepProcessor,
        SimpleStepProcessorConfig,
    },
    Agent, DefaultEvaluator, Evaluator as _, Policy, Trainer, TrainerConfig,
};
use border_derive::SubBatch;
use border_mlflow_tracking::MlflowTrackingClient;
use border_py_gym_env::{
    util::{arrayd_to_tensor, tensor_to_arrayd},
    ArrayObsFilter, ContinuousActFilter, GymActFilter, GymEnv, GymEnvConfig, GymObsFilter,
};
use border_tensorboard::TensorboardRecorder;
use candle_core::Tensor;
use clap::{App, Arg, ArgMatches};
use log::info;
use ndarray::{ArrayD, IxDyn};

const DIM_OBS: i64 = 27;
const DIM_ACT: i64 = 8;
const LR_ACTOR: f64 = 3e-4;
const LR_CRITIC: f64 = 3e-4;
const BATCH_SIZE: usize = 256;
const WARMUP_PERIOD: usize = 10_000;
const OPT_INTERVAL: usize = 1;
const MAX_OPTS: usize = 3_000_000;
const EVAL_INTERVAL: usize = 5_000;
const REPLAY_BUFFER_CAPACITY: usize = 300_000;
const N_EPISODES_PER_EVAL: usize = 5;
const N_CRITICS: usize = 2;
const TAU: f64 = 0.02;
const TARGET_ENTROPY: f64 = -(DIM_ACT as f64);
const LR_ENT_COEF: f64 = 3e-4;
const CRITIC_LOSS: CriticLoss = CriticLoss::SmoothL1;
const MODEL_DIR: &str = "./border/examples/ant/model/candle";

fn cuda_if_available() -> candle_core::Device {
    candle_core::Device::cuda_if_available(0).unwrap()
}

mod obs_act_types {
    use super::*;

    #[derive(Clone, Debug)]
    pub struct Obs(ArrayD<f32>);

    #[derive(Clone, SubBatch)]
    pub struct ObsBatch(TensorSubBatch);

    impl border_core::Obs for Obs {
        fn dummy(_n: usize) -> Self {
            Self(ArrayD::zeros(IxDyn(&[0])))
        }

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

    impl From<Obs> for ObsBatch {
        fn from(obs: Obs) -> Self {
            let tensor = obs.into();
            Self(TensorSubBatch::from_tensor(tensor))
        }
    }

    #[derive(Clone, Debug)]
    pub struct Act(ArrayD<f32>);

    impl border_core::Act for Act {}

    impl From<Act> for ArrayD<f32> {
        fn from(value: Act) -> Self {
            value.0
        }
    }

    impl From<Tensor> for Act {
        fn from(t: Tensor) -> Self {
            Self(tensor_to_arrayd(t, true).unwrap())
        }
    }

    // Required by Sac
    impl From<Act> for Tensor {
        fn from(value: Act) -> Self {
            arrayd_to_tensor::<_, f32>(value.0, true).unwrap()
        }
    }

    #[derive(SubBatch)]
    pub struct ActBatch(TensorSubBatch);

    impl From<Act> for ActBatch {
        fn from(act: Act) -> Self {
            let tensor = act.into();
            Self(TensorSubBatch::from_tensor(tensor))
        }
    }

    type PyObsDtype = f32;
    pub type ObsFilter = ArrayObsFilter<PyObsDtype, f32, Obs>;
    pub type ActFilter = ContinuousActFilter<Act>;
    pub type EnvConfig = GymEnvConfig<Obs, Act, ObsFilter, ActFilter>;
    pub type Env = GymEnv<Obs, Act, ObsFilter, ActFilter>;
    pub type StepProc = SimpleStepProcessor<Env, ObsBatch, ActBatch>;
    pub type ReplayBuffer = SimpleReplayBuffer<ObsBatch, ActBatch>;
    pub type Evaluator = DefaultEvaluator<Env, Sac<Env, Mlp, Mlp2, ReplayBuffer>>;
}

use obs_act_types::*;

mod config {
    use serde::Serialize;

    use super::*;

    #[derive(Serialize)]
    pub struct SacAntConfig {
        pub trainer: TrainerConfig,
        pub replay_buffer: SimpleReplayBufferConfig,
        pub agent: SacConfig<Mlp, Mlp2>,
    }

    pub fn env_config() -> EnvConfig {
        GymEnvConfig::<Obs, Act, ObsFilter, ActFilter>::default()
            .name("Ant-v4".to_string())
            .obs_filter_config(ObsFilter::default_config())
            .act_filter_config(ActFilter::default_config())
    }

    pub fn create_trainer_config() -> TrainerConfig {
        TrainerConfig::default()
            .max_opts(MAX_OPTS)
            .opt_interval(OPT_INTERVAL)
            .eval_interval(EVAL_INTERVAL)
            .record_agent_info_interval(EVAL_INTERVAL)
            .record_compute_cost_interval(EVAL_INTERVAL)
            .flush_record_interval(EVAL_INTERVAL)
            .save_interval(EVAL_INTERVAL)
            .warmup_period(WARMUP_PERIOD)
            .model_dir(MODEL_DIR)
    }

    pub fn create_sac_config() -> SacConfig<Mlp, Mlp2> {
        let device = cuda_if_available();
        let actor_config = ActorConfig::default()
            .opt_config(OptimizerConfig::Adam { lr: LR_ACTOR })
            .out_dim(DIM_ACT)
            .pi_config(MlpConfig::new(DIM_OBS, vec![400, 300], DIM_ACT, false));
        let critic_config = CriticConfig::default()
            .opt_config(OptimizerConfig::Adam { lr: LR_CRITIC })
            .q_config(MlpConfig::new(DIM_OBS + DIM_ACT, vec![400, 300], 1, false));

        SacConfig::default()
            .batch_size(BATCH_SIZE)
            .actor_config(actor_config)
            .critic_config(critic_config)
            .tau(TAU)
            .critic_loss(CRITIC_LOSS)
            .n_critics(N_CRITICS)
            .ent_coef_mode(EntCoefMode::Auto(TARGET_ENTROPY, LR_ENT_COEF))
            .device(device)
    }
}

mod utils {
    use super::*;

    pub fn create_recorder(
        matches: &ArgMatches,
        config: &config::SacAntConfig,
    ) -> Result<Box<dyn AggregateRecorder>> {
        match matches.is_present("mlflow") {
            true => {
                let client =
                    MlflowTrackingClient::new("http://localhost:8080").set_experiment_id("Gym")?;
                let recorder_run = client.create_recorder("")?;
                recorder_run.log_params(&config)?;
                recorder_run.set_tag("env", "ant")?;
                recorder_run.set_tag("algo", "dqn")?;
                recorder_run.set_tag("backend", "candle")?;
                Ok(Box::new(recorder_run))
            }
            false => Ok(Box::new(TensorboardRecorder::new(MODEL_DIR))),
        }
    }

    pub fn create_matches<'a>() -> ArgMatches<'a> {
        App::new("sac_ant_tch")
            .version("0.1.0")
            .author("Taku Yoshioka <yoshioka@laboro.ai>")
            .arg(
                Arg::with_name("play")
                    .long("play")
                    .takes_value(true)
                    .help("Play with the trained model of the given path"),
            )
            .arg(
                Arg::with_name("play-gdrive")
                    .long("play-gdrive")
                    .takes_value(false)
                    .help("Play with the trained model downloaded from google drive"),
            )
            .arg(
                Arg::with_name("wait")
                    .long("wait")
                    .takes_value(true)
                    .default_value("25")
                    .help("Waiting time in milliseconds between frames when playing"),
            )
            .arg(
                Arg::with_name("mlflow")
                    .long("mlflow")
                    .takes_value(false)
                    .help("User mlflow tracking"),
            )
            .get_matches()
    }
}

fn train(matches: ArgMatches) -> Result<()> {
    let env_config = config::env_config();
    let agent_config = config::create_sac_config();
    let trainer_config = config::create_trainer_config();
    let replay_buffer_config = SimpleReplayBufferConfig::default().capacity(REPLAY_BUFFER_CAPACITY);
    let step_proc_config = SimpleStepProcessorConfig {};

    let config = config::SacAntConfig {
        trainer: trainer_config.clone(),
        replay_buffer: replay_buffer_config.clone(),
        agent: agent_config.clone(),
    };
    let mut agent = Sac::build(agent_config);
    let mut recorder = utils::create_recorder(&matches, &config)?;
    let mut evaluator = Evaluator::new(&env_config, 0, N_EPISODES_PER_EVAL)?;
    let mut trainer = Trainer::<Env, StepProc, ReplayBuffer>::build(
        trainer_config,
        env_config,
        step_proc_config,
        replay_buffer_config,
    );

    trainer.train(&mut agent, &mut recorder, &mut evaluator)?;

    Ok(())
}

fn eval(model_dir: &str, render: bool, wait: u64) -> Result<()> {
    let env_config = {
        let mut env_config = config::env_config();
        if render {
            env_config = env_config
                .render_mode(Some("human".to_string()))
                .set_wait_in_millis(wait);
        };
        env_config
    };
    let mut agent = {
        let agent_config = config::create_sac_config();
        let mut agent = Sac::build(agent_config);
        agent.load(model_dir)?;
        agent.eval();
        agent
    };
    // let mut recorder = BufferedRecorder::new();

    let _ = Evaluator::new(&env_config, 0, N_EPISODES_PER_EVAL)?.evaluate(&mut agent);

    Ok(())
}

fn eval1(matches: ArgMatches) -> Result<()> {
    let model_dir = {
        let model_dir = matches
            .value_of("play")
            .expect("Failed to parse model directory");
        format!("{}{}", model_dir, "/best").to_owned()
    };
    let render = true;
    let wait = matches.value_of("wait").unwrap().parse().unwrap();
    eval(&model_dir, render, wait)
}

fn eval2(matches: ArgMatches) -> Result<()> {
    let model_dir = {
        let file_base = "sac_ant_20210324_ec2_smoothl1";
        let url =
            "https://drive.google.com/uc?export=download&id=1XvFi2nJD5OhpTvs-Et3YREuoqy8c3Vkq";
        let model_dir = get_model_from_url(url, file_base)?;
        info!("Download the model in {:?}", model_dir.as_ref().to_str());
        model_dir.as_ref().to_str().unwrap().to_string()
    };
    let render = true;
    let wait = matches.value_of("wait").unwrap().parse().unwrap();
    eval(&model_dir, render, wait)
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    fastrand::seed(42);

    let matches = utils::create_matches();

    if matches.is_present("play") {
        eval1(matches)?;
    } else if matches.is_present("play-gdrive") {
        eval2(matches)?;
    } else {
        train(matches)?;
    }

    Ok(())
}
