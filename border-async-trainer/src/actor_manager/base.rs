use crate::{Actor, ActorManagerConfig, ReplayBufferProxyConfig};
use border_core::{Agent, Env, ReplayBufferBase, StepProcessorBase};
use std::{marker::PhantomData, sync::{Arc, Mutex}};

/// Manages [Actor]s.
///
/// This struct handles the following requests:
/// * From the [LearnerManager]() for updating the latest model info, stored in this struct.
/// * From the [Actor]s for getting the latest model info.
/// * From the [Actor]s for pushing sample batch to the `LearnerManager`.
pub struct ActorManager<A, E, P, R>
where
    A: Agent<E, R>,
    E: Env,
    P: StepProcessorBase<E>,
    R: ReplayBufferBase<PushedItem = P::Output>,
{
    /// The number of [Actor]s.
    n_actors: usize,

    /// Configuration of [Agent].
    agent_config: A::Config,

    /// Configuration of [Env].
    env_config: E::Config,

    /// Configuration of a `StepProcessor`.
    step_proc_config: P::Config,

    /// Number of samples to be buffered in each actor before being pushed to the replay buffer.
    ///
    /// At the same time, [Actor] asks for [ActorManager] to get the model parameters.
    samples_per_push: usize,

    /// Flag to stop training
    stop: Arc<Mutex<bool>>,

    phantom: PhantomData<R>
}

impl<A, E, P, R> ActorManager<A, E, P, R>
where
    A: Agent<E, R>,
    E: Env,
    P: StepProcessorBase<E>,
    R: ReplayBufferBase<PushedItem = P::Output>,
    A::Config: Send + 'static,
    E::Config: Send + 'static,
    P::Config: Send + 'static,
{
    /// Builds a [ActorManager].
    pub fn build(config: &ActorManagerConfig<A, E, P, R>) -> Self {
        Self {
            n_actors: config.n_actors,
            agent_config: config.agent_config.clone(),
            env_config: config.env_config.clone(),
            step_proc_config: config.step_proc_config.clone(),
            samples_per_push: config.samples_per_push,
            stop: Arc::new(Mutex::new(false)),
            phantom: PhantomData,
        }
    }

    /// Runs [Actor]s.
    pub fn run(&self) {
        // 

        // Runs sampling processes
        (0..self.n_actors).for_each(|seed| {
            let replay_buffer_proxy_config = ReplayBufferProxyConfig {};
            let agent_config = self.agent_config.clone();
            let env_config = self.env_config.clone();
            let step_proc_config = self.step_proc_config.clone();
            let samples_per_push = self.samples_per_push;
            let stop = self.stop.clone();

            std::thread::spawn(move || {
                Actor::<A, E, P, R>::build(
                    agent_config,
                    env_config,
                    step_proc_config,
                    replay_buffer_proxy_config,
                    samples_per_push,
                    stop,
                    seed as i64,
                ).run();
            });
        });
    }
}
