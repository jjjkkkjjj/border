use std::marker::PhantomData;
use tch::Tensor;

use crate::agent::tch::model::Model1;

/// Explorers for DQN.
pub trait DQNExplorer<M>
{
    /// Takes actions.
    ///
    /// The first dimension of `obs` can be the number of processes.
    fn action(&mut self, qnet: &M, obs: &Tensor) -> Tensor;
}

/// Softmax explorer for DQN.
#[derive(Clone)]
pub struct Softmax<M> {
    phantom: PhantomData<M>
}

#[allow(clippy::new_without_default)]
impl<M> Softmax<M> {
    /// Constructs softmax explorer.
    pub fn new() -> Self {
        Self { phantom: PhantomData }
    }
}

impl<M> DQNExplorer<M> for Softmax<M> where
    M: Model1<Input=Tensor, Output=Tensor>
{
    fn action(&mut self, qnet: &M, obs: &Tensor) -> Tensor {
        let a = qnet.forward(obs);
        a.softmax(-1, tch::Kind::Float).multinomial(1, true)
    }
}

/// Epsilon-greedy explorer for DQN.
#[derive(Clone)]
pub struct EpsilonGreedy<M> {
    n_opts: usize,
    eps_start: f64,
    eps_final: f64,
    final_step: usize,
    phantom: PhantomData<M>
}

#[allow(clippy::new_without_default)]
impl<M> EpsilonGreedy<M> {
    /// Constructs softmax explorer.
    pub fn new() -> Self {
        Self {
            n_opts: 0,
            eps_start: 1.0,
            eps_final: 0.02,
            final_step: 100_000,
            phantom: PhantomData
        }
    }
}

impl<M> DQNExplorer<M> for EpsilonGreedy<M> where
    M: Model1<Input=Tensor, Output=Tensor>
{
    fn action(&mut self, qnet: &M, obs: &Tensor) -> Tensor {
        let d = (self.eps_start - self.eps_final) / (self.final_step as f64);
        let eps = (self.eps_start - d * self.n_opts as f64).max(self.eps_final);
        let r = fastrand::f64();
        let is_random = r < eps;
        self.n_opts += 1;

        if is_random {
            let n_procs = obs.size()[0] as u32;
            let n_actions = qnet.out_dim() as u32;
            Tensor::of_slice(
                (0..n_procs).map(|_| fastrand::u32(..n_actions) as i32).collect::<Vec<_>>()
                .as_slice()
            )
        }
        else {
            let a = qnet.forward(&obs);
            a.argmax(-1, true)
        }
    }
}