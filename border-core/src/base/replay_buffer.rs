//! Replay buffer.
use anyhow::Result;

/// Interface of buffers of experiences from environments.
///
/// You can push items, which has an arbitrary type.
/// This trait is usually required by processes sampling experiences.
pub trait ExperienceBufferBase {
    /// Items pushed into the buffer.
    type Item;

    /// Pushes a transition into the buffer.
    fn push(&mut self, tr: Self::Item) -> Result<()>;

    /// The number of samples in the buffer.
    fn len(&self) -> usize;
}

/// Interface of replay buffers.
///
/// Ones implementing this trait generates a [`ReplayBufferBase::Batch`],
/// which is used to train agents.
pub trait ReplayBufferBase {
    /// Configuration of the replay buffer.
    type Config: Clone;

    /// Batch generated from the buffer.
    type Batch;

    /// Build a replay buffer from [Self::Config].
    fn build(config: &Self::Config) -> Self;

    /// Constructs a batch.
    ///
    /// `beta` - The exponent for priority.
    fn batch(&mut self, size: usize) -> Result<Self::Batch>;

    /// Updates priority.
    ///
    /// Priority is commonly based on TD error.
    ///
    /// TODO: Consider to move this method to another trait.
    /// There are cases where prioritization is not required.
    fn update_priority(&mut self, ixs: &Option<Vec<usize>>, td_err: &Option<Vec<f32>>);
}

/// A dummy replay buffer.
///
/// This struct is used to wrap a [`Policy`] with an [`Agent`].
///
/// [`Policy`]: crate::Policy
/// [`Agent`]: crate::Agent
pub struct NullReplayBuffer;

impl ReplayBufferBase for NullReplayBuffer {
    type Batch = ();
    type Config = ();

    #[allow(unused_variables)]
    fn build(config: &Self::Config) -> Self {
        Self
    }

    #[allow(unused_variables)]
    fn batch(&mut self, size: usize) -> Result<Self::Batch> {
        unimplemented!();
    }

    #[allow(unused_variables)]
    fn update_priority(&mut self, ixs: &Option<Vec<usize>>, td_err: &Option<Vec<f32>>) {
        unimplemented!();
    }
}
