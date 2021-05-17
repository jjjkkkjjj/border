#![warn(missing_docs)]
//! Border is a library for reinforcement learning (RL).
pub mod core;
pub mod error;
pub use crate::core::{
    base::{Act, Agent, Env, Obs, Policy, Step, Info},
    trainer::{Trainer, TrainerBuilder},
    util::eval,
    util,
    record,
};
