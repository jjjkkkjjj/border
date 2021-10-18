//! A generic implementation of [Batch](crate::Batch).
use super::SubBatch;
use crate::Batch as BatchBase;

/// A generic implementation of [Batch](crate::Batch).
pub struct Batch<O, A>
where
    O: SubBatch,
    A: SubBatch,
{
    pub(super) obs: O,
    pub(super) act: A,
    pub(super) next_obs: O,
    pub(super) reward: Vec<f32>,
    pub(super) is_done: Vec<i8>,
}

impl<O, A> BatchBase for Batch<O, A>
where
    O: SubBatch,
    A: SubBatch,
{
    type ObsBatch = O;
    type ActBatch = A;

    fn unpack(
        self,
    ) -> (
        Self::ObsBatch,
        Self::ActBatch,
        Self::ObsBatch,
        Vec<f32>,
        Vec<i8>,
    ) {
        (self.obs, self.act, self.next_obs, self.reward, self.is_done)
    }

    fn len(&self) -> usize {
        self.reward.len()
    }

    fn obs(&self) -> &Self::ObsBatch {
        &self.obs
    }

    fn act(&self) -> &Self::ActBatch {
        &self.act
    }

    fn next_obs(&self) -> &Self::ObsBatch {
        &self.next_obs
    }

    fn reward(&self) -> &Vec<f32> {
        &self.reward
    }

    fn is_done(&self) -> &Vec<i8> {
        &self.is_done
    }
}