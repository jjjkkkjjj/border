use super::{ActorManager, ActorManagerConfig, AsyncTrainer, AsyncTrainerConfig, SyncModel};
use border_atari_env::util::test::*;
use border_core::{
    record::BufferedRecorder,
    replay_buffer::{
        SimpleReplayBuffer, SimpleReplayBufferConfig, SimpleStepProcessor,
        SimpleStepProcessorConfig,
    },
    Env as _,
};
use crossbeam_channel::unbounded;
use log::info;
use std::sync::{Arc, Mutex};
use test_log::test;

fn replay_buffer_config() -> SimpleReplayBufferConfig {
    SimpleReplayBufferConfig::default()
}

fn actor_man_config() -> ActorManagerConfig {
    ActorManagerConfig::default()
}

fn async_trainer_config() -> AsyncTrainerConfig {
    AsyncTrainerConfig {
        model_dir: Some("".to_string()),
        record_interval: 5,
        eval_interval: 105,
        max_train_steps: 15,
        save_interval: 5,
        sync_interval: 5,
        eval_episodes: 1,
    }
}

impl SyncModel for RandomAgent {
    type ModelInfo = ();

    fn model_info(&self) -> (usize, Self::ModelInfo) {
        info!("Returns the current model info");
        (self.n_opts_steps(), ())
    }

    fn sync_model(&mut self, _model_info: &Self::ModelInfo) {
        info!("Sync model");
    }
}

#[test]
fn test_async_trainer() {
    type Agent = RandomAgent;
    type StepProc = SimpleStepProcessor<Env, ObsBatch, ActBatch>;
    type ReplayBuffer = SimpleReplayBuffer<ObsBatch, ActBatch>;
    type ActorManager_ = ActorManager<RandomAgent, Env, ReplayBuffer, StepProc>;
    type AsyncTrainer_ = AsyncTrainer<Agent, Env, ReplayBuffer>;

    let env_config = env_config("pong".to_string());
    let env = Env::build(&env_config, 0).unwrap();
    let n_acts = env.get_num_actions_atari() as _;
    let agent_config = RandomAgentConfig { n_acts };
    let step_proc_config = SimpleStepProcessorConfig::default();
    let replay_buffer_config = replay_buffer_config();
    let actor_man_config = actor_man_config();
    let async_trainer_config = async_trainer_config();
    let agent_configs = vec![agent_config.clone(); 2];

    let mut recorder = BufferedRecorder::new();

    // Shared flag to stop actor threads
    let stop = Arc::new(Mutex::new(false));

    // Pushed items into replay buffer
    let (item_s, item_r) = unbounded();

    // Synchronizing model
    let (model_s, model_r) = unbounded();

    // Prevents simlutaneous initialization of env
    let guard_init_env = Arc::new(Mutex::new(true));

    let mut actors = ActorManager_::build(
        &actor_man_config,
        &agent_configs,
        &env_config,
        &step_proc_config,
        item_s,
        model_r,
        stop.clone(),
    );
    let mut trainer = AsyncTrainer_::build(
        &async_trainer_config,
        &agent_config,
        &env_config,
        &replay_buffer_config,
        item_r,
        model_s,
        stop,
    );

    actors.run(guard_init_env.clone());
    trainer.train(&mut recorder, guard_init_env);

    actors.stop_and_join();
}
