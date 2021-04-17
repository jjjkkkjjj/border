//! IQN agent implemented with tch-rs.
use log::trace;
use std::{error::Error, cell::RefCell, marker::PhantomData, path::Path, fs};
use tch::{Tensor, no_grad, Device};

use crate::{
    core::{
        Policy, Agent, Step, Env, Obs,
        record::{Record, RecordValue}
    },
    agent::{
        OptIntervalCounter,
        tch::{
            ReplayBuffer, TchBuffer, model::{ModelBase, SubModel}, TchBatch,
            iqn::{IQNModel, IQNExplorer, model::{IQNSample, average}},
            util::{quantile_huber_loss, track}
        }
    },
};

#[allow(clippy::upper_case_acronyms)]
/// IQN agent implemented with tch-rs.
///
/// The type parameter `M` is a feature extractor, which takes
/// `M::Input` and returns feature vectors.
pub struct IQN<E, F, M, O, A> where
    E: Env,
    F: SubModel,
    M: SubModel,
    E::Obs: Into<F::Input>,
    E::Act: From<Tensor>,
    O: TchBuffer<Item = E::Obs, SubBatch = F::Input>,
    A: TchBuffer<Item = E::Act, SubBatch = Tensor>,
{
    pub(super) opt_interval_counter: OptIntervalCounter,
    pub(super) soft_update_interval: usize,
    pub(super) soft_update_counter: usize,
    pub(super) n_updates_per_opt: usize,
    pub(super) min_transitions_warmup: usize,
    pub(super) batch_size: usize,
    pub(super) iqn: IQNModel<F, M>,
    pub(super) iqn_tgt: IQNModel<F, M>,
    pub(super) train: bool,
    pub(super) phantom: PhantomData<E>,
    pub(super) prev_obs: RefCell<Option<E::Obs>>,
    pub(super) replay_buffer: ReplayBuffer<E, O, A>,
    pub(super) discount_factor: f64,
    pub(super) tau: f64,
    pub(super) n_prob_samples: usize,
    pub(super) explorer: IQNExplorer,
    pub(super) device: Device,
}

impl<E, F, M, O, A> IQN<E, F, M, O, A> where
    E: Env,
    F: SubModel<Output = Tensor>,
    M: SubModel<Input = Tensor, Output = Tensor>,
    E::Obs: Into<F::Input>,
    E::Act: From<Tensor>,
    O: TchBuffer<Item = E::Obs, SubBatch = F::Input>,
    A: TchBuffer<Item = E::Act, SubBatch = Tensor>,
{
    fn push_transition(&mut self, step: Step<E>) {
        trace!("IQN::push_transition()");

        let next_obs = step.obs;
        let obs = self.prev_obs.replace(None).unwrap();
        let reward = Tensor::of_slice(&step.reward[..]);
        let not_done = Tensor::from(1f32) - Tensor::of_slice(&step.is_done[..]);
        self.replay_buffer.push(
            &obs,
            &step.act,
            &reward,
            &next_obs,
            &not_done,
        );
        let _ = self.prev_obs.replace(Some(next_obs));
    }

    fn update_critic(&mut self, batch: TchBatch<E, O, A>) -> f32 {
        trace!("IQN::update_critic()");

        let obs = batch.obs;
        let a = batch.actions.to(self.device);
        let r = batch.rewards.to(self.device);
        let next_obs = batch.next_obs;
        let not_done = batch.not_dones.to(self.device);
        trace!("a.shape        = {:?}", a.size());

        let batch_size = self.batch_size as _;
        let n_percent_points = self.n_prob_samples as _;

        debug_assert_eq!(r.size().as_slice(), &[batch_size, 1]);
        debug_assert_eq!(not_done.size().as_slice(), &[batch_size, 1]);

        let loss = {
            // predictions of z(s, a), where a is from minibatch
            // pred.size() == [batch_size, 1, n_percent_points]
            let (pred, tau) = {
                // percent points
                let tau = IQNSample::Uniform10.sample().to(self.device);
                debug_assert_eq!(tau.size().as_slice(), &[n_percent_points]);

                // predictions for all actions
                let x = self.iqn.forward(&obs, &tau);
                let n_actions = x.size()[x.size().len() - 1];
                debug_assert_eq!(x.size().as_slice(), &[batch_size, n_percent_points, n_actions]);

                // takes z(s, a) with a from minibatch
                let x = x.gather(-1, &a.unsqueeze(1).repeat(&[1, n_percent_points, 1]), false)
                    .squeeze1(-1).unsqueeze(1);
                debug_assert_eq!(x.size().as_slice(), &[batch_size, 1, n_percent_points]);
                (x, tau)
            };

            // target values with max_a q(s, a)
            // tgt.size() == [batch_size, n_percent_points, 1]
            // in theory, n_percent_points can be different with that for predictions
            let tgt = no_grad(|| {
                // percent points
                let tau = IQNSample::Uniform10.sample().to(self.device);
                debug_assert_eq!(tau.size().as_slice(), &[n_percent_points]);

                // target values for all actions
                let x = self.iqn_tgt.forward(&next_obs, &tau);
                let n_actions = x.size()[x.size().len() - 1];
                debug_assert_eq!(x.size().as_slice(), &[batch_size, n_percent_points, n_actions]);

                // argmax_a q(s,a), where z are averaged over tau
                let y = x.copy().mean1(&[1], false, tch::Kind::Float);
                let a = y.argmax(-1, false).unsqueeze(-1).unsqueeze(-1)
                    .repeat(&[1, n_percent_points, 1]);
                debug_assert_eq!(a.size(), &[batch_size, n_percent_points, 1]);

                // takes z(s, a)
                let x = x.gather(2, &a, false);
                debug_assert_eq!(x.size().as_slice(), &[batch_size, n_percent_points, 1]);

                // target value
                let r = r.unsqueeze(-1);
                let not_done = not_done.unsqueeze(-1);
                let tgt = r + not_done * self.discount_factor * x;
                debug_assert_eq!(tgt.size().as_slice(), &[batch_size, n_percent_points, 1]);

                tgt
            });

            let diff = pred - tgt;
            debug_assert_eq!(diff.size().as_slice(),
                &[batch_size, n_percent_points, n_percent_points]
            );

            // flatten the axes of minibatch and percent points of the target values
            let diff = diff.flatten(0, 1);
            debug_assert_eq!(diff.size().as_slice(),
                &[batch_size * n_percent_points, n_percent_points]
            );

            quantile_huber_loss(&diff.transpose(0, 1), &tau).mean(tch::Kind::Float)
        };

        self.iqn.backward_step(&loss);

        f32::from(loss)
    }

    fn soft_update(&mut self) {
        trace!("IQN::soft_update()");
        track(&mut self.iqn_tgt, &mut self.iqn, self.tau);
    }
}

impl<E, F, M, O, A> Policy<E> for IQN<E, F, M, O, A> where
    E: Env,
    F: SubModel<Output = Tensor>,
    M: SubModel<Input = Tensor, Output = Tensor>,
    E::Obs: Into<F::Input>,
    E::Act: From<Tensor>,
    O: TchBuffer<Item = E::Obs, SubBatch = F::Input>,
    A: TchBuffer<Item = E::Act, SubBatch = Tensor>,
{
    fn sample(&mut self, obs: &E::Obs) -> E::Act {
        let a = no_grad(|| {
            if self.train {
                let iqn = &self.iqn;
                let device = self.device;
                let n_procs = obs.n_procs();
                let obs = obs.clone().into();
                let q_fn = || {
                    let a = average(&obs, iqn, IQNSample::Uniform10, device);
                    a.argmax(-1, true)
                };
                let shape = (n_procs as u32, self.iqn.out_dim as u32);
                match &mut self.explorer {
                    IQNExplorer::EpsilonGreedy(egreedy) => egreedy.action(shape, q_fn),
                }
            } else {
                let obs = obs.clone().into();
                let a = average(&obs, &self.iqn, IQNSample::Uniform10, self.device);
                a.argmax(-1, true)
            }
        });
        a.into()
    }
}

impl<E, F, M, O, A> Agent<E> for IQN<E, F, M, O, A> where
    E: Env,
    F: SubModel<Output = Tensor>,
    M: SubModel<Input = Tensor, Output = Tensor>,
    E::Obs: Into<F::Input>,
    E::Act: From<Tensor>,
    O: TchBuffer<Item = E::Obs, SubBatch = F::Input>,
    A: TchBuffer<Item = E::Act, SubBatch = Tensor>,
{
    fn train(&mut self) {
        self.train = true;
    }

    fn eval(&mut self) {
        self.train = false;
    }

    fn is_train(&self) -> bool {
        self.train
    }

    fn push_obs(&self, obs: &E::Obs) {
        self.prev_obs.replace(Some(obs.clone()));
    }

    /// Update model parameters.
    ///
    /// When the return value is `Some(Record)`, it includes:
    /// * `loss_critic`: Loss of critic
    fn observe(&mut self, step: Step<E>) -> Option<Record> {
        trace!("DQN::observe()");

        // Check if doing optimization
        let do_optimize = self.opt_interval_counter.do_optimize(&step.is_done)
            && self.replay_buffer.len() + 1 >= self.min_transitions_warmup;

        // Push transition to the replay buffer
        self.push_transition(step);
        trace!("Push transition");

        // Do optimization
        if do_optimize {
            let mut loss_critic = 0f32;

            for _ in 0..self.n_updates_per_opt {
                let batch = self.replay_buffer.random_batch(self.batch_size).unwrap();
                trace!("Sample random batch");

                loss_critic += self.update_critic(batch);
            };

            self.soft_update_counter += 1;
            if self.soft_update_counter == self.soft_update_interval {
                self.soft_update_counter = 0;
                self.soft_update();
                trace!("Update target network");
            }

            loss_critic /= self.n_updates_per_opt as f32;

            Some(Record::from_slice(&[
                ("loss_critic", RecordValue::Scalar(loss_critic)),
            ]))
        }
        else {
            None
        }
    }

    fn save<T: AsRef<Path>>(&self, path: T) -> Result<(), Box<dyn Error>> {
        // TODO: consider to rename the path if it already exists
        fs::create_dir_all(&path)?;
        self.iqn.save(&path.as_ref().join("iqn.pt").as_path())?;
        self.iqn_tgt.save(&path.as_ref().join("iqn_tgt.pt").as_path())?;
        Ok(())
    }

    fn load<T: AsRef<Path>>(&mut self, path: T) -> Result<(), Box<dyn Error>> {
        self.iqn.load(&path.as_ref().join("iqn.pt").as_path())?;
        self.iqn_tgt.load(&path.as_ref().join("iqn_tgt.pt").as_path())?;
        Ok(())
    }
}
