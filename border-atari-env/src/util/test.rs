//! Utilities for test.
use crate::{
    BorderAtariAct, BorderAtariActRawFilter, BorderAtariEnv, BorderAtariEnvConfig, BorderAtariObs,
    BorderAtariObsRawFilter,
};
use anyhow::Result;
use border_core::{
    generic_replay_buffer::{BatchBase, SimpleReplayBuffer},
    record::Record,
    Agent as Agent_, Configurable, Policy, ReplayBufferBase,
};
use serde::Deserialize;
use std::ptr::copy;

pub type Obs = BorderAtariObs;
pub type Act = BorderAtariAct;
pub type ObsFilter = BorderAtariObsRawFilter<Obs>;
pub type ActFilter = BorderAtariActRawFilter<Act>;
pub type EnvConfig = BorderAtariEnvConfig<Obs, Act, ObsFilter, ActFilter>;
pub type ReplayBuffer = SimpleReplayBuffer<ObsBatch, ActBatch>;
pub type Env = BorderAtariEnv<Obs, Act, ObsFilter, ActFilter>;
pub type Agent = RandomAgent;

const FRAME_IN_BYTES: usize = 84 * 84;

/// Consists the observation part of a batch in [SimpleReplayBuffer].
pub struct ObsBatch {
    /// The number of samples in the batch.
    pub n: usize,

    /// The length of a sample in bytes.
    pub m: usize,

    /// The buffer.
    pub buf: Vec<u8>,
}

impl BatchBase for ObsBatch {
    fn new(capacity: usize) -> Self {
        let m = 4 * FRAME_IN_BYTES;
        Self {
            n: 0,
            m,
            buf: vec![0; capacity * m],
        }
    }

    #[inline]
    fn push(&mut self, i: usize, data: Self) {
        unsafe {
            let src: *const u8 = &data.buf[0];
            let dst: *mut u8 = &mut self.buf[i * self.m];
            copy(src, dst, self.m);
        }
    }

    fn sample(&self, ixs: &Vec<usize>) -> Self {
        let n = ixs.len();
        let m = self.m;
        let mut buf = vec![0; n];
        (0..n).enumerate().for_each(|(i, ix)| unsafe {
            let src: *const u8 = &self.buf[ix];
            let dst: *mut u8 = &mut buf[i * self.m];
            copy(src, dst, self.m);
        });

        Self { m, n, buf }
    }
}

impl From<Obs> for ObsBatch {
    fn from(obs: Obs) -> Self {
        Self {
            n: 1,
            m: 4 * FRAME_IN_BYTES,
            buf: obs.frames,
        }
    }
}

/// Consists the action part of a batch in [SimpleReplayBuffer].
pub struct ActBatch {
    /// The number of samples in the batch.
    pub n: usize,

    /// The length of a sample in bytes.
    pub m: usize,

    /// The buffer.
    pub buf: Vec<u8>,
}

impl BatchBase for ActBatch {
    fn new(capacity: usize) -> Self {
        let m = 1;
        Self {
            n: 0,
            m,
            buf: vec![0; capacity * m],
        }
    }

    #[inline]
    fn push(&mut self, i: usize, data: Self) {
        unsafe {
            let src: *const u8 = &data.buf[0];
            let dst: *mut u8 = &mut self.buf[i * self.m];
            copy(src, dst, self.m);
        }
    }

    fn sample(&self, ixs: &Vec<usize>) -> Self {
        let n = ixs.len();
        let m = self.m;
        let mut buf = vec![0; n];
        (0..n).enumerate().for_each(|(i, ix)| unsafe {
            let src: *const u8 = &self.buf[ix];
            let dst: *mut u8 = &mut buf[i * self.m];
            copy(src, dst, self.m);
        });

        Self { m, n, buf }
    }
}

impl From<Act> for ActBatch {
    fn from(act: Act) -> Self {
        Self {
            n: 1,
            m: 1,
            buf: vec![act.act],
        }
    }
}

#[derive(Clone, Deserialize)]
/// Configuration of [`RandomAgent``].
pub struct RandomAgentConfig {
    pub n_acts: usize,
}

/// A random policy.
pub struct RandomAgent {
    n_acts: usize,
    n_opts_steps: usize,
    train: bool,
}

impl Policy<Env> for RandomAgent {
    fn sample(&mut self, _: &Obs) -> Act {
        fastrand::u8(..self.n_acts as u8).into()
    }
}

impl Configurable for RandomAgent {
    type Config = RandomAgentConfig;

    fn build(config: Self::Config) -> Self {
        Self {
            n_acts: config.n_acts,
            n_opts_steps: 0,
            train: true,
        }
    }
}

impl<R: ReplayBufferBase> Agent_<Env, R> for RandomAgent {
    fn train(&mut self) {
        self.train = true;
    }

    fn eval(&mut self) {
        self.train = false;
    }

    fn is_train(&self) -> bool {
        self.train
    }

    fn opt_with_record(&mut self, _buffer: &mut R) -> border_core::record::Record {
        // Do nothing
        self.n_opts_steps += 1;
        Record::empty()
    }

    fn save_params(&self, _path: &std::path::Path) -> Result<Vec<std::path::PathBuf>> {
        println!("save() was invoked");
        Ok(vec![])
    }

    fn load_params(&mut self, _path: &std::path::Path) -> Result<()> {
        println!("load() was invoked");
        Ok(())
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn as_any_ref(&self) -> &dyn std::any::Any {
        self
    }
}

impl RandomAgent {
    /// Returns the number of optimization steps;
    pub fn n_opts_steps(&self) -> usize {
        self.n_opts_steps
    }
}

/// Returns the default configuration of [BorderAtariEnv].
pub fn env_config(name: String) -> EnvConfig {
    EnvConfig::default().name(name)
}

// fn main() -> Result<()> {
//     env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
//     fastrand::seed(42);

//     let env_config = env_config("pong".to_string());
//     let mut env = Env::build(&env_config, 42)?;
//     let mut recorder = BufferedRecorder::new();
//     let n_acts = env.get_num_actions_atari();
//     let policy_config = RandomPolicyConfig {
//         n_acts: n_acts as _,
//     };
//     let mut policy = RandomPolicy::build(policy_config);

//     env.open()?;
//     let _ = util::eval_with_recorder(&mut env, &mut policy, 5, &mut recorder)?;

//     Ok(())
// }
