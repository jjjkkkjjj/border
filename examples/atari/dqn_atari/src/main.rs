mod args;
mod config;
mod types;
use anyhow::Result;
use args::Args;
use border_core::{
    generic_replay_buffer::SimpleStepProcessorConfig, record::Recorder, Agent, Configurable,
    Env as _, Evaluator as _, ReplayBufferBase, StepProcessor, Trainer,
};
use border_mlflow_tracking::MlflowTrackingClient;
use border_tensorboard::TensorboardRecorder;
use clap::Parser;
use config::DqnAtariConfig;
use types::*;

const MODEL_DIR: &str = "./model";
const MLFLOW_EXPERIMENT_NAME: &str = "Atari";
const MLFLOW_TAGS: &[(&str, &str)] = &[("algo", "dqn"), ("backend", "candle")];

fn create_agent(config: &DqnAtariConfig) -> Result<Box<dyn Agent<Env, ReplayBuffer>>> {
    let n_actions = Env::build(&config.env_config, 0)?.get_num_actions_atari() as i64;
    let agent_config = config.agent_config.clone().out_dim(n_actions);
    Ok(Box::new(Dqn::build(agent_config)))
}

fn create_recorder(
    args: &Args,
    config: Option<&DqnAtariConfig>,
) -> Result<Box<dyn Recorder<Env, ReplayBuffer>>> {
    if let Some(mlflow_run_name) = &args.mlflow_run_name {
        let client = MlflowTrackingClient::new("http://localhost:8080")
            .set_experiment(MLFLOW_EXPERIMENT_NAME)?;
        let recorder_run = client.create_recorder(format!("{}_candle", mlflow_run_name))?;
        if let Some(config) = config {
            recorder_run.log_params(&config)?;
            recorder_run.set_tag("env", &args.name)?;
            recorder_run.set_tags(MLFLOW_TAGS)?;
        }
        Ok(Box::new(recorder_run))
    } else {
        let model_dir = format!("{}/{}", MODEL_DIR, &args.name);
        Ok(Box::new(TensorboardRecorder::new(
            &model_dir, &model_dir, false,
        )))
    }
}

fn train(config: &DqnAtariConfig) -> Result<()> {
    let env_config_train = config.clone_env_config();
    let env_config_eval = config.clone_env_config().eval();
    let step_proc_config = SimpleStepProcessorConfig {};

    let mut trainer = Trainer::build(config.clone_trainer_config());
    let env = Env::build(&env_config_train, 0)?;
    let step_proc = StepProc::build(&step_proc_config);
    let mut agent = create_agent(config)?;
    let mut buffer = ReplayBuffer::build(&config.clone_replay_buffer_config());
    let mut recorder = create_recorder(&config.args, Some(config))?;
    let mut evaluator = Evaluator::new(&env_config_eval, 0, 1)?;

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

fn eval(config: &DqnAtariConfig) -> Result<()> {
    let env_config = config.clone_env_config();
    let mut agent = create_agent(config)?;
    let mut evaluator = Evaluator::new(&env_config, 0, 5)?;

    // recorder is used to load model parameters
    let recorder = create_recorder(&config.args, None)?;
    recorder.load_model("best".as_ref(), &mut agent)?;

    let _ = evaluator.evaluate(&mut agent);

    Ok(())
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let config: DqnAtariConfig = Args::parse().into();

    match config.args.mode.as_str() {
        "train" => train(&config)?,
        "eval" => eval(&config)?,
        _ => panic!("mode must be either 'train' or 'eval'"),
    }

    Ok(())
}
