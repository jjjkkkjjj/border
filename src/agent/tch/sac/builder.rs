//! Builder of SAC agent.
use std::{cell::RefCell, marker::PhantomData};
use tch::Tensor;

use crate::{
    agent::{
        tch::{
            model::{Model1, Model2},
            sac::{
                ent_coef::{EntCoef, EntCoefMode},
                SAC,
            },
            ReplayBuffer, TchBuffer,
        },
        CriticLoss, OptInterval, OptIntervalCounter,
    },
    core::Env,
};

type ActionValue = Tensor;
type ActMean = Tensor;
type ActStd = Tensor;

/// SAC builder.
#[allow(clippy::upper_case_acronyms)]
pub struct SACBuilder {
    gamma: f64,
    tau: f64,
    ent_coef_mode: EntCoefMode,
    epsilon: f64,
    min_lstd: f64,
    max_lstd: f64,
    opt_interval_counter: OptIntervalCounter,
    n_updates_per_opt: usize,
    min_transitions_warmup: usize,
    batch_size: usize,
    train: bool,
    critic_loss: CriticLoss,
    reward_scale: f32,
}

impl Default for SACBuilder {
    fn default() -> Self {
        Self {
            gamma: 0.99,
            tau: 0.005,
            ent_coef_mode: EntCoefMode::Fix(1.0),
            epsilon: 1e-4,
            min_lstd: -20.0,
            max_lstd: 2.0,
            opt_interval_counter: OptInterval::Steps(1).counter(),
            n_updates_per_opt: 1,
            min_transitions_warmup: 1,
            batch_size: 1,
            train: false,
            critic_loss: CriticLoss::MSE,
            reward_scale: 1.0,
        }
    }
}

impl SACBuilder {
    /// Discount factor.
    pub fn discount_factor(mut self, v: f64) -> Self {
        self.gamma = v;
        self
    }

    /// Soft update coefficient.
    pub fn tau(mut self, v: f64) -> Self {
        self.tau = v;
        self
    }

    /// SAC-alpha.
    pub fn ent_coef_mode(mut self, v: EntCoefMode) -> Self {
        self.ent_coef_mode = v;
        self
    }

    /// Set optimization interval.
    pub fn opt_interval(mut self, v: OptInterval) -> Self {
        self.opt_interval_counter = v.counter();
        self
    }

    /// Set numper of parameter update steps per optimization step.
    pub fn n_updates_per_opt(mut self, v: usize) -> Self {
        self.n_updates_per_opt = v;
        self
    }

    /// Interval before starting optimization.
    pub fn min_transitions_warmup(mut self, v: usize) -> Self {
        self.min_transitions_warmup = v;
        self
    }

    /// Batch size.
    pub fn batch_size(mut self, v: usize) -> Self {
        self.batch_size = v;
        self
    }

    /// Reward scale.
    ///
    /// It works for obtaining target values, not the values in logs.
    pub fn reward_scale(mut self, v: f32) -> Self {
        self.reward_scale = v;
        self
    }

    /// Critic loss.
    pub fn critic_loss(mut self, v: CriticLoss) -> Self {
        self.critic_loss = v;
        self
    }

    /// Constructs SAC.
    pub fn build<E, Q, P, O, A>(
        self,
        critics: Vec<Q>,
        policy: P,
        replay_buffer: ReplayBuffer<E, O, A>,
        device: tch::Device,
    ) -> SAC<E, Q, P, O, A>
    where
        E: Env,
        Q: Model2<Input1 = O::SubBatch, Input2 = A::SubBatch, Output = ActionValue> + Clone,
        P: Model1<Output = (ActMean, ActStd)> + Clone,
        E::Obs: Into<O::SubBatch>,
        E::Act: From<Tensor>,
        O: TchBuffer<Item = E::Obs, SubBatch = P::Input>,
        A: TchBuffer<Item = E::Act, SubBatch = Tensor>,
    {
        // let critics_tgt = critics.iter().map(|c| c.clone()).collect();
        let critics_tgt = critics.to_vec();

        SAC {
            qnets: critics,
            qnets_tgt: critics_tgt,
            pi: policy,
            replay_buffer,
            gamma: self.gamma,
            tau: self.tau,
            ent_coef: EntCoef::new(self.ent_coef_mode, device),
            epsilon: self.epsilon,
            min_lstd: self.min_lstd,
            max_lstd: self.max_lstd,
            opt_interval_counter: self.opt_interval_counter,
            n_updates_per_opt: self.n_updates_per_opt,
            min_transitions_warmup: self.min_transitions_warmup,
            batch_size: self.batch_size,
            train: self.train,
            reward_scale: self.reward_scale,
            critic_loss: self.critic_loss,
            prev_obs: RefCell::new(None),
            device,
            phantom: PhantomData,
        }
    }
}
