//! Core functionalities.
mod agent;
mod batch;
mod env;
mod policy;
mod replay_buffer;
mod step;
pub use agent::Agent;
pub use batch::TransitionBatch;
pub use env::Env;
pub use policy::{Configurable, Policy};
pub use replay_buffer::{ExperienceBufferBase, NullReplayBuffer, ReplayBufferBase};
use std::fmt::Debug;
pub use step::{Info, Step, StepProcessor};

/// A set of observations of an environment.
///
/// Old versions of the library support vectorized environment and
/// [`Obs`] was able to handle multiple observations.
/// In the current version, no vectorized environment is implemented.
/// Thus, [`Obs`]`::len()` always returns 1.
pub trait Obs: Clone + Debug {
    /// Returns the number of observations in the object.
    fn len(&self) -> usize;
}

/// A set of actions of the environment.
pub trait Act: Clone + Debug {
    /// Returns the number of actions in the object.
    ///
    /// TODO: Consider to delete.
    fn len(&self) -> usize {
        unimplemented!();
    }
}
