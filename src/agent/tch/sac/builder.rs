use std::{marker::PhantomData, cell::RefCell};
use tch::Tensor;

use crate::{
    core::Env,
    agent::{
        OptInterval, OptIntervalCounter,
        tch::{
            ReplayBuffer, TchBuffer,
            model::{Model1, Model2},
            sac::SAC,
        }
    }
};

type ActionValue = Tensor;
type ActMean = Tensor;
type ActStd = Tensor;

/// SAC builder.
pub struct SACBuilder {
    gamma: f64,
    tau: f64,
    alpha: f64,
    epsilon: f64,
    min_std: f64,
    max_std: f64,
    opt_interval_counter: OptIntervalCounter,
    n_updates_per_opt: usize,
    min_transitions_warmup: usize,
    batch_size: usize,
    train: bool,
}

impl Default for SACBuilder {
    fn default() -> Self {
        Self {
            gamma: 0.99,
            tau: 0.005,
            alpha: 0.1,
            epsilon: 1e-4,
            min_std: 1e-3,
            max_std: 2.0,
            opt_interval_counter: OptInterval::Steps(1).counter(),
            n_updates_per_opt: 1,
            min_transitions_warmup: 1,
            batch_size: 1,
            train: false,
        }
    }
}

impl SACBuilder{
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
    pub fn alpha(mut self, v: f64) -> Self {
        self.alpha = v;
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

    /// Constructs SAC.
    pub fn build<E, Q, P, O, A>(self, critic: Q, policy: P, replay_buffer: ReplayBuffer<E, O, A>)
        -> SAC<E, Q, P, O, A> where
        E: Env,
        Q: Model2<Input1 = O::SubBatch, Input2 = A::SubBatch, Output = ActionValue> + Clone,
        P: Model1<Output = (ActMean, ActStd)> + Clone,
        E::Obs :Into<O::SubBatch>,
        E::Act :From<Tensor>,
        O: TchBuffer<Item = E::Obs, SubBatch = P::Input>,
        A: TchBuffer<Item = E::Act, SubBatch = Tensor>,
    {
        let critic_tgt = critic.clone();
        SAC {
            qnet: critic,
            qnet_tgt: critic_tgt,
            pi: policy,
            replay_buffer,
            gamma: self.gamma,
            tau: self.tau,
            alpha: self.alpha,
            epsilon: self.epsilon,
            min_std: self.min_std,
            max_std: self.max_std,
            opt_interval_counter: self.opt_interval_counter,
            n_updates_per_opt: self.n_updates_per_opt,
            min_transitions_warmup: self.min_transitions_warmup,
            batch_size: self.batch_size,
            train: self.train,
            prev_obs: RefCell::new(None),
            phantom: PhantomData,       
        }
    }
}
