use crate::args::Args;
use crate::types::*;
// use anyhow::Result;
use border_async_trainer::AsyncTrainerConfig;
use border_atari_env::BorderAtariEnvConfig;
use border_core::generic_replay_buffer::SimpleReplayBufferConfig;
use border_tch_agent::{
    cnn::{AtariCnn, AtariCnnConfig},
    dqn::{DqnConfig, DqnExplorer, DqnModelConfig, EpsilonGreedy},
    opt::OptimizerConfig,
    util::CriticLoss,
    Device,
};
use serde::Serialize;
use std::marker::PhantomData;
use tch::Device as TchDevice;

#[derive(Clone, Serialize)]
pub struct DqnAtariAsyncConfig {
    pub args: Args,
    pub env_config: EnvConfig,
    pub replay_buffer_config: SimpleReplayBufferConfig,
    pub agent_config: DqnConfig<AtariCnn>,
    pub trainer_config: AsyncTrainerConfig,
}

impl From<Args> for DqnAtariAsyncConfig {
    fn from(args: Args) -> DqnAtariAsyncConfig {
        let name = args.name.clone();
        let env_config = BorderAtariEnvConfig::default().name(name);
        let replay_buffer_config = create_replay_buffer_config(&args);
        let agent_config = create_agent_config(&args);
        let trainer_config = create_trainer_config(&args);

        DqnAtariAsyncConfig {
            args,
            env_config,
            replay_buffer_config,
            agent_config,
            trainer_config,
        }
    }
}

impl DqnAtariAsyncConfig {
    pub fn clone_env_config(&self) -> EnvConfig {
        self.env_config.clone()
    }

    pub fn clone_replay_buffer_config(&self) -> SimpleReplayBufferConfig {
        self.replay_buffer_config.clone()
    }

    pub fn clone_trainer_config(&self) -> AsyncTrainerConfig {
        self.trainer_config.clone()
    }

    pub fn model_dir(&self) -> String {
        format!("./model/{}", self.args.name)
    }
}

fn create_replay_buffer_config(_args: &Args) -> SimpleReplayBufferConfig {
    SimpleReplayBufferConfig {
        capacity: 262144,
        seed: 42,
        per_config: None,
    }
}

fn create_agent_config(args: &Args) -> DqnConfig<AtariCnn> {
    let device = if let Some(device) = &args.device {
        match device.as_str() {
            "Cpu" => Device::Cpu,
            _ => Device::Cuda(0),
        }
    } else {
        Device::from(TchDevice::cuda_if_available())
    };

    DqnConfig {
        model_config: DqnModelConfig {
            q_config: Some(AtariCnnConfig {
                n_stack: 4,
                out_dim: 0,
                skip_linear: false,
            }),
            opt_config: OptimizerConfig::Adam { lr: 0.0001 },
        },
        soft_update_interval: 10000,
        n_updates_per_opt: 1,
        batch_size: 32,
        discount_factor: 0.99,
        tau: 1.0,
        train: false,
        explorer: DqnExplorer::EpsilonGreedy(EpsilonGreedy {
            n_opts: 0,
            eps_start: 1.0,
            eps_final: 0.02,
            final_step: 1000000,
        }),
        clip_reward: Some(1.0),
        double_dqn: false,
        clip_td_err: None,
        critic_loss: CriticLoss::Mse,
        record_verbose_level: 0,
        device: Some(device),
        phantom: PhantomData,
    }
}

fn create_trainer_config(_args: &Args) -> AsyncTrainerConfig {
    AsyncTrainerConfig {
        max_opts: 3000000,
        eval_interval: 3000,
        flush_record_interval: 3000,
        record_compute_cost_interval: 3000,
        save_interval: 300000,
        sync_interval: 1,
        warmup_period: 32,
    }
}
