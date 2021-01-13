use log::trace;
use std::{error::Error, cell::RefCell, marker::PhantomData, path::Path, fs};
use tch::{no_grad, Kind::Float, Tensor};
use crate::core::{Policy, Agent, Step, Env};
use crate::agents::tch::{ReplayBuffer, TchBuffer, TchBatch};
use crate::agents::tch::model::Model;
use crate::agents::tch::util::track;

type ActionValue = Tensor;
type ActMean = Tensor;
type ActStd = Tensor;

pub struct SAC<E, Q, P, O, A> where
    E: Env,
    Q: Model<Input = (O::SubBatch, A::SubBatch), Output = ActionValue> + Clone,
    P: Model<Output = (ActMean, ActStd)>,
    E::Obs :Into<O::SubBatch>,
    E::Act :From<Tensor>,
    O: TchBuffer<Item = E::Obs, SubBatch = P::Input>,
    A: TchBuffer<Item = E::Act>,
{
    qnet: Q,
    qnet_tgt: Q,
    pi: P,
    prev_obs: RefCell<Option<E::Obs>>,
    phantom: PhantomData<E>
}

impl<E, Q, P, O, A> SAC<E, Q, P, O, A> where
    E: Env,
    Q: Model<Input = (O::SubBatch, A::SubBatch), Output = ActionValue> + Clone,
    P: Model<Output = (ActMean, ActStd)>,
    E::Obs :Into<O::SubBatch>,
    E::Act :From<Tensor>,
    O: TchBuffer<Item = E::Obs, SubBatch = P::Input>,
    A: TchBuffer<Item = E::Act>,
{
    pub fn new(qnet: M, replay_buffer: ReplayBuffer<E, O, A>)
            -> Self {
        let qnet_tgt = qnet.clone();
        SAC {
            qnet,
            qnet_tgt,
            replay_buffer,
            n_samples_per_opt: 1,
            n_updates_per_opt: 1,
            n_opts_per_soft_update: 1,
            min_transitions_warmup: 1,
            batch_size: 1,
            discount_factor: 0.99,
            tau: 0.005,
            count_samples_per_opt: 0,
            count_opts_per_soft_update: 0,
            train: false,
            prev_obs: RefCell::new(None),
            phantom: PhantomData,
        }
    }

    // Adapted from dqn.rs
    fn push_transition(&mut self, step: Step<E>) {
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

    fn action_logp(&self, o: &Tensor) -> (Tensor, Tensor) {
        let (m, s) = self.pi.forward(&o);
        let z = Tensor::randn(m.size().into(), tch::kind::FLOAT_CPU);
        let next_a = (&s * &z + &m).tanh();
        let log_p = Normal::logp(&z, &s) - (1 - &a.pow(2) + self.epsilon);

        (next_a, log_p)
    }

    fn update_critic(&mut self, batch: TchBatch<E, O, A>) {
        trace!("Start sac.update_critic()");

        let o = batch.obs;
        let a = batch.actions;
        let r = batch.rewards;
        let next_o = batch.next_obs;
        let not_done = batch.not_dones;
        trace!("obs.shape      = {:?}", o.size());
        trace!("next_obs.shape = {:?}", next_o.size());
        trace!("act.shape      = {:?}", a.size());
        trace!("reward.shape   = {:?}", r.size());
        trace!("not_done.shape = {:?}", not_done.size());

        let loss = {
            let pred = self.qnet.forward((&o, &a));
            let tgt = {
                let next_q = tch::no_grad(|| {
                    let (next_a, log_p) = self.action_logp(&next_o);
                    let next_q = self.qnet_tgt.forward(&next_o, &next_a);
                    next_q - self.alpha * log_p
                });
                r + not_done * self.gamma * next_q
            };
            0.5 * pred.mse_loss(&tgt, tch::Reduction::Mean)
        };

        self.qnet.backward_step(&loss);
    }
}

impl<E, Q, P, O, A> Policy<E> for SAC<E, Q, P, O, A> where
    E: Env,
    Q: Model<Input = (O::SubBatch, A::SubBatch), Output = ActionValue> + Clone,
    P: Model<Output = (ActMean, ActStd)>,
    E::Obs :Into<O::SubBatch>,
    E::Act :From<Tensor>,
    O: TchBuffer<Item = E::Obs, SubBatch = P::Input>,
    A: TchBuffer<Item = E::Act>,
{
    fn sample(&self, obs: &E::Obs) -> E::Act {
        let obs = obs.clone().into();
        let (m, s) = self.pi.forward(&obs);
        let act = if self.train {
            s * Tensor::randn(&m.size(), kind::FLOAT_CPU) + m
        }
        else {
            m
        };
        a.tanh().into()
    }
}

impl<E, Q, P, O, A> Agent<E> for SAC<E, Q, P, O, A> where
    E: Env,
    Q: Model<Input = (O::SubBatch, A::SubBatch), Output = ActionValue> + Clone,
    P: Model<Output = (ActMean, ActStd)>,
    E::Obs :Into<O::SubBatch>,
    E::Act :From<Tensor>,
    O: TchBuffer<Item = E::Obs, SubBatch = P::Input>,
    A: TchBuffer<Item = E::Act>,
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

    fn observe(&mut self, step: Step<E>) -> bool {
        trace!("Start dqn.observe()");

        // Push transition to the replay buffer
        self.push_transition(step);
        trace!("Push transition");

        // Do optimization 1 step
        self.count_samples_per_opt += 1;
        if self.count_samples_per_opt == self.n_samples_per_opt {
            self.count_samples_per_opt = 0;

            if self.replay_buffer.len() >= self.min_transitions_warmup {
                for _ in 0..self.n_updates_per_opt {
                    let batch = self.replay_buffer.random_batch(self.batch_size).unwrap();
                    trace!("Sample random batch");

                    self.update_critic(batch);
                    self.update_actor(batch);
                    trace!("Update models");
                };

                self.count_opts_per_soft_update += 1;
                if self.count_opts_per_soft_update == self.n_opts_per_soft_update {
                    self.count_opts_per_soft_update = 0;
                    self.soft_update_qnet_tgt();
                }
                return true;
            }
        }
        false
    }

    fn save<T: AsRef<Path>>(&self, path: T) -> Result<(), Box<dyn Error>> {
        fs::create_dir(&path)?;
        self.qnet.save(&path.as_ref().join("qnet.pt").as_path())?;
        self.qnet_tgt.save(&path.as_ref().join("qnet_tgt.pt").as_path())?;
        Ok(())
    }

    fn load<T: AsRef<Path>>(&mut self, path: T) -> Result<(), Box<dyn Error>> {
        self.qnet.load(&path.as_ref().join("qnet.pt").as_path())?;
        self.qnet_tgt.load(&path.as_ref().join("qnet_tgt.pt").as_path())?;
        Ok(())
    }
}
