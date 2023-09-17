use anyhow::Result;
use border_core::{
    record::{/*BufferedRecorder,*/ Record, RecordValue, TensorboardRecorder},
    replay_buffer::{
        SimpleReplayBuffer, SimpleReplayBufferConfig, SimpleStepProcessor,
        SimpleStepProcessorConfig,
    },
    Agent, DefaultEvaluator, Evaluator as _, Policy, Trainer, TrainerConfig,
};
use border_derive::SubBatch;
use border_py_gym_env::{
    util::{arrayd_to_pyobj, arrayd_to_tensor, tensor_to_arrayd, ArrayType},
    ArrayDictObsFilter, GymActFilter, GymEnv, GymEnvConfig, GymObsFilter,
};
use border_tch_agent::{
    mlp::{Mlp, Mlp2, MlpConfig},
    opt::OptimizerConfig,
    sac::{ActorConfig, CriticConfig, EntCoefMode, Sac, SacConfig},
    TensorSubBatch,
    util::CriticLoss,
};
use clap::{App, Arg};
// use csv::WriterBuilder;
use ndarray::ArrayD;
use pyo3::PyObject;
// use serde::Serialize;
use std::convert::TryFrom;
use tch::Tensor;

const DIM_OBS: i64 = 16;
const DIM_ACT: i64 = 4;
const LR_ACTOR: f64 = 3e-4;
const LR_CRITIC: f64 = 3e-4;
const BATCH_SIZE: usize = 128;
const N_TRANSITIONS_WARMUP: usize = 1000;
const OPT_INTERVAL: usize = 1;
const MAX_OPTS: usize = 20_000_000;
const EVAL_INTERVAL: usize = 2_000;
const REPLAY_BUFFER_CAPACITY: usize = 100_000;
const N_EPISODES_PER_EVAL: usize = 5;
const N_CRITICS: usize = 2;
const TAU: f64 = 0.02;
const TARGET_ENTROPY: f64 = -(DIM_ACT as f64);
const LR_ENT_COEF: f64 = 3e-4;
const CRITIC_LOSS: CriticLoss = CriticLoss::SmoothL1;

mod obs {
    use super::*;
    use border_py_gym_env::util::Array;

    #[derive(Clone, Debug)]
    pub struct Obs(Vec<(String, Array)>);

    #[derive(Clone, SubBatch)]
    pub struct ObsBatch(TensorSubBatch);

    impl border_core::Obs for Obs {
        fn dummy(_n: usize) -> Self {
            Self(vec![("".to_string(), Array::Empty)])
        }

        fn len(&self) -> usize {
            match self.0.get(0) {
                None => 0,
                Some(v) => v.1.len(),
            }
        }
    }

    impl From<Vec<(String, Array)>> for Obs {
        fn from(obs: Vec<(String, Array)>) -> Self {
            Obs(obs)
        }
    }

    impl From<Obs> for Tensor {
        fn from(obs: Obs) -> Tensor {
            let arrays = obs.0.into_iter().map(|e| e.1).collect::<Vec<_>>();
            let array = Array::hstack(arrays);
            Tensor::try_from(&array.as_f32_array()).unwrap()
        }
    }

    impl From<Obs> for ObsBatch {
        fn from(obs: Obs) -> Self {
            let tensor = obs.into();
            Self(TensorSubBatch::from_tensor(tensor))
        }
    }
}

mod act {
    use super::*;

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
            Self(tensor_to_arrayd(t, true))
        }
    }

    // Required by Sac
    impl From<Act> for Tensor {
        fn from(value: Act) -> Self {
            arrayd_to_tensor::<_, f32>(value.0, true)
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

    // Custom activation filter
    #[derive(Clone, Debug)]
    pub struct ActFilter {}

    impl GymActFilter<Act> for ActFilter {
        type Config = ();

        fn build(_config: &Self::Config) -> Result<Self>
        where
            Self: Sized,
        {
            Ok(Self {})
        }

        fn filt(&mut self, act: Act) -> (PyObject, Record) {
            let act_filt = 2f32 * &act.0;
            let record = Record::from_slice(&[
                (
                    "act_org",
                    RecordValue::Array1(act.0.iter().cloned().collect()),
                ),
                (
                    "act_filt",
                    RecordValue::Array1(act_filt.iter().cloned().collect()),
                ),
            ]);
            (arrayd_to_pyobj(act_filt), record)
        }
    }
}

use act::{Act, ActBatch, ActFilter};
use obs::{Obs, ObsBatch};

type ObsFilter = ArrayDictObsFilter<Obs>;
type Env = GymEnv<Obs, Act, ObsFilter, ActFilter>;
type StepProc = SimpleStepProcessor<Env, ObsBatch, ActBatch>;
type ReplayBuffer = SimpleReplayBuffer<ObsBatch, ActBatch>;
type Evaluator = DefaultEvaluator<Env, Sac<Env, Mlp, Mlp2, ReplayBuffer>>;

fn create_agent(in_dim: i64, out_dim: i64) -> Sac<Env, Mlp, Mlp2, ReplayBuffer> {
    let device = tch::Device::cuda_if_available();
    let actor_config = ActorConfig::default()
        .opt_config(OptimizerConfig::Adam { lr: LR_ACTOR })
        .out_dim(out_dim)
        .pi_config(MlpConfig::new(in_dim, vec![64, 64], out_dim, true));
    let critic_config = CriticConfig::default()
        .opt_config(OptimizerConfig::Adam { lr: LR_CRITIC })
        .q_config(MlpConfig::new(in_dim + out_dim, vec![64, 64], 1, true));
    let sac_config = SacConfig::default()
        .batch_size(BATCH_SIZE)
        .min_transitions_warmup(N_TRANSITIONS_WARMUP)
        .actor_config(actor_config)
        .critic_config(critic_config)
        .tau(TAU)
        .critic_loss(CRITIC_LOSS)
        .n_critics(N_CRITICS)
        .ent_coef_mode(EntCoefMode::Auto(TARGET_ENTROPY, LR_ENT_COEF))
        .device(device);
    Sac::build(sac_config)
}

fn env_config() -> GymEnvConfig<Obs, Act, ObsFilter, ActFilter> {
    GymEnvConfig::<Obs, Act, ObsFilter, ActFilter>::default()
        .name("FetchReach-v2".to_string())
        .obs_filter_config(ObsFilter::default_config().add_key_and_types(vec![
            ("observation", ArrayType::F32Array),
            ("desired_goal", ArrayType::F32Array),
            ("achieved_goal", ArrayType::F32Array),
        ]))
        .act_filter_config(ActFilter::default_config())
}

fn train(max_opts: usize, model_dir: &str, eval_interval: usize) -> Result<()> {
    let mut trainer = {
        let env_config = env_config();
        let step_proc_config = SimpleStepProcessorConfig {};
        let replay_buffer_config =
            SimpleReplayBufferConfig::default().capacity(REPLAY_BUFFER_CAPACITY);
        let config = TrainerConfig::default()
            .max_opts(max_opts)
            .opt_interval(OPT_INTERVAL)
            .eval_interval(eval_interval)
            .record_interval(eval_interval)
            .save_interval(eval_interval)
            .model_dir(model_dir);
        let trainer = Trainer::<Env, StepProc, ReplayBuffer>::build(
            config,
            env_config,
            step_proc_config,
            replay_buffer_config,
        );

        trainer
    };
    let mut agent = create_agent(DIM_OBS, DIM_ACT);
    let mut recorder = TensorboardRecorder::new(model_dir);
    let mut evaluator = Evaluator::new(&env_config(), 0, N_EPISODES_PER_EVAL)?;

    trainer.train(&mut agent, &mut recorder, &mut evaluator)?;

    Ok(())
}

fn eval(n_episodes: usize, render: bool, model_dir: &str) -> Result<()> {
    let env_config = {
        let mut env_config = env_config();
        if render {
            env_config = env_config
                .render_mode(Some("human".to_string()))
                .set_wait_in_millis(10);
        };
        env_config
    };
    let mut agent = {
        let mut agent = create_agent(DIM_OBS, DIM_ACT);
        agent.load(model_dir)?;
        agent.eval();
        agent
    };
    // let mut recorder = BufferedRecorder::new();

    let _ = Evaluator::new(&env_config, 0, n_episodes)?.evaluate(&mut agent);

    Ok(())
}



fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    tch::manual_seed(42);

    let matches = App::new("sac_fetch_reach")
        .version("0.1.0")
        .author("Taku Yoshioka <yoshioka@laboro.ai>")
        .arg(
            Arg::with_name("train")
                .long("train")
                .takes_value(false)
                .help("Do training only"),
        )
        .arg(
            Arg::with_name("eval")
                .long("eval")
                .takes_value(false)
                .help("Do evaluation only"),
        )
        .get_matches();

    let do_train = matches.is_present("train");
    let do_eval = matches.is_present("eval");

    if !do_train && !do_eval {
        println!("You need to give either --train or --eval in the command line argument.");
        return Ok(());
    }

    if do_train {
        train(
            MAX_OPTS,
            "./border/examples/model/sac_fetch_reach",
            EVAL_INTERVAL,
        )?;
    }
    if do_eval {
        eval(5, true, "./border/examples/model/sac_fetch_reach/best")?;
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use tempdir::TempDir;

    #[test]
    fn test_sac_fetch_reach() -> Result<()> {
        tch::manual_seed(42);

        let model_dir = TempDir::new("sac_fetch_reach")?;
        let model_dir = model_dir.path().to_str().unwrap();
        train(100, model_dir, 100)?;
        eval(1, false, (model_dir.to_string() + "/best").as_str())?;

        Ok(())
    }
}
