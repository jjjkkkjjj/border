//! Exploration strategies of IQN.
use serde::{Deserialize, Serialize};
use std::default::Default;
use tch::Tensor;

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
/// Explorers for IQN.
pub enum IqnExplorer {
    /// Softmax action selection.
    Softmax(Softmax),
    /// Epsilon-greedy action selection.
    EpsilonGreedy(EpsilonGreedy),
}

/// Softmax explorer for IQN.
#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct Softmax {}

#[allow(clippy::new_without_default)]
impl Softmax {
    /// Constructs softmax explorer.
    pub fn new() -> Self {
        Self {}
    }

    /// Takes an action based on the observation and the critic.
    pub fn action(&mut self, a: &Tensor) -> Tensor {
        a.softmax(-1, tch::Kind::Float).multinomial(1, true)
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
/// Epsilon-greedy explorer for IQN.
pub struct EpsilonGreedy {
    n_opts: usize,
    eps_start: f64,
    eps_final: f64,
    final_step: usize,
}

impl Default for EpsilonGreedy {
    fn default() -> Self {
        Self {
            n_opts: 0,
            eps_start: 1.0,
            eps_final: 0.02,
            final_step: 100_000,
        }
    }
}

#[allow(clippy::new_without_default)]
impl EpsilonGreedy {
    /// Constructs epsilon-greedy explorer.
    pub fn with_params(eps_start: f64, eps_final: f64, final_step: usize) -> IqnExplorer {
        IqnExplorer::EpsilonGreedy(Self {
            n_opts: 0,
            eps_start,
            eps_final,
            final_step,
        })
    }

    /// Constructs epsilon-greedy explorer.
    ///
    /// TODO: improve interface.
    pub fn with_final_step(final_step: usize) -> IqnExplorer {
        IqnExplorer::EpsilonGreedy(Self {
            n_opts: 0,
            eps_start: 1.0,
            eps_final: 0.02,
            final_step,
        })
    }

    /// Takes an action based on the observation and the critic.
    pub fn action(&mut self, action_value: Tensor) -> Tensor {
        let d = (self.eps_start - self.eps_final) / (self.final_step as f64);
        let eps = (self.eps_start - d * self.n_opts as f64).max(self.eps_final);
        let r = fastrand::f64();
        let is_random = r < eps;
        self.n_opts += 1;

        if is_random {
            let batch_size = action_value.size()[0];
            let n_actions = action_value.size()[1] as u32;
            Tensor::of_slice(
                (0..batch_size)
                    .map(|_| fastrand::u32(..n_actions) as i32)
                    .collect::<Vec<_>>()
                    .as_slice(),
            )
        } else {
            action_value.argmax(-1, true)
        }
    }
}
