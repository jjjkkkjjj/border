mod util_dqn_atari;
use anyhow::Result;
use border_async_trainer::AsyncTrainerConfig;
use border_core::{
    replay_buffer::{PerConfig, SimpleReplayBufferConfig},
    TrainerConfig,
};
use border_tch_agent::{
    cnn::{CnnConfig, Cnn},
    dqn::{DqnConfig, DqnModelConfig}, //, EpsilonGreedy, DQNExplorer},
    opt::OptimizerConfig,
};
use std::{default::Default, path::Path};
use util_dqn_atari::{model_dir, model_dir_async, Params};

fn make_dqn_config(params: &Params) -> DqnConfig<Cnn> {
    let n_stack = 4;
    let out_dim = 0; // Set before training/evaluation
    let lr = if params.per {
        params.lr / 4.0
    } else {
        params.lr
    };
    let clip_td_err = if params.per { Some((-1.0, 1.0)) } else { None };

    let model_config = DqnModelConfig::default()
        .q_config(CnnConfig::new(n_stack, out_dim))
        .out_dim(out_dim)
        .opt_config(OptimizerConfig::Adam { lr });

    DqnConfig::default()
        .model_config(model_config)
        .batch_size(params.batch_size)
        .discount_factor(params.discount_factor)
        .double_dqn(params.double_dqn)
        .min_transitions_warmup(params.min_transition_warmup)
        .soft_update_interval(params.soft_update_interval)
        .clip_reward(params.clip_reward)
        .tau(params.tau)
        .explorer(params.explorer.clone())
        .clip_td_err(clip_td_err)
}

fn make_replay_buffer_config(params: &Params) -> SimpleReplayBufferConfig {
    let mut config = SimpleReplayBufferConfig::default().capacity(params.replay_buffer_capacity);

    if params.per {
        config = config.per_config(Some(PerConfig::default().n_opts_final(200_000_000)));
    }

    config
}

fn make_trainer_config(env_name: String, params: &Params) -> Result<TrainerConfig> {
    let model_dir = model_dir(env_name, params)?;

    let (max_opts, opt_interval, record_interval) = if !params.debug {
        (params.max_opts, params.opt_interval, params.record_interval)
    } else {
        (1000, 100, 100)
    };

    Ok(TrainerConfig::default()
        .max_opts(max_opts)
        .opt_interval(opt_interval)
        .record_interval(record_interval)
        .eval_interval(params.eval_interval)
        .model_dir(model_dir)
        .save_interval(params.save_interval))
}

fn make_async_trainer_config(env_name: String, params: &Params) -> Result<AsyncTrainerConfig> {
    let model_dir = model_dir(env_name, params)? + "_async/";

    let (max_opts, record_interval) = if !params.debug {
        (params.max_opts, params.record_interval)
    } else {
        (1000, 100)
    };

    Ok(AsyncTrainerConfig {
        model_dir: Some(model_dir),
        record_interval,
        eval_interval: params.eval_interval,
        max_train_steps: max_opts,
        save_interval: params.save_interval,
        sync_interval: 100,
    })
}

fn make_cfg(env_name: impl Into<String> + Clone, params: &Params) -> Result<()> {
    let agent_config = make_dqn_config(params);
    let replay_buffer_config = make_replay_buffer_config(params);
    let trainer_config = make_trainer_config(env_name.clone().into(), params)?;

    let model_dir = model_dir(env_name.into(), params)?;
    let agent_path = Path::new(&model_dir).join("agent.yaml");
    let replay_buffer_path = Path::new(&model_dir).join("replay_buffer.yaml");
    let trainer_path = Path::new(&model_dir).join("trainer.yaml");

    agent_config.save(&agent_path)?;
    replay_buffer_config.save(&replay_buffer_path)?;
    trainer_config.save(&trainer_path)?;

    println!("{:?}", agent_path);
    println!("{:?}", replay_buffer_path);
    println!("{:?}", trainer_path);

    Ok(())
}

fn make_cfg_async(env_name: impl Into<String> + Clone, params: &Params) -> Result<()> {
    let agent_config = make_dqn_config(params);
    let replay_buffer_config = make_replay_buffer_config(params);
    let trainer_config = make_async_trainer_config(env_name.clone().into(), params)?;

    let model_dir = model_dir_async(env_name.into(), params)?;
    let agent_path = Path::new(&model_dir).join("agent.yaml");
    let replay_buffer_path = Path::new(&model_dir).join("replay_buffer.yaml");
    let trainer_path = Path::new(&model_dir).join("trainer.yaml");

    agent_config.save(&agent_path)?;
    replay_buffer_config.save(&replay_buffer_path)?;
    trainer_config.save(&trainer_path)?;

    println!("{:?}", agent_path);
    println!("{:?}", replay_buffer_path);
    println!("{:?}", trainer_path);

    Ok(())
}

fn main() -> Result<()> {
    // Pong
    let params = Params::default().replay_buffer_capacity(65536);
    make_cfg("pong", &params)?;
    make_cfg("PongNoFrameskip-v4", &params)?;
    make_cfg("PongNoFrameskip-v4", &params.clone().debug())?;
    make_cfg("pong", &params.clone().ddqn().per())?;
    make_cfg_async("pong", &params)?;

    // Hero
    let params = Params::default()
        .max_opts(50_000_000)
        .eval_interval(500_000)
        .save_interval(10_000_000)
        .replay_buffer_capacity(1048576)
        .optimizer("adam");
    make_cfg("HeroNoFrameskip-v4", &params)?;
    make_cfg("hero", &params)?;
    make_cfg("hero", &params.clone().ddqn())?;
    make_cfg("hero", &params.clone().ddqn().per())?;

    Ok(())
}
