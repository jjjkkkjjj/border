//! SubBatch, which consists [`BatchBase`](`crate::BatchBase`).

/// Represents a SubBatch, which consists [`BatchBase`](`crate::BatchBase`).
pub trait SubBatch {
    /// Builds a subbatch with a capacity.
    fn new(capacity: usize) -> Self;

    /// Pushes the samples in `data`.
    fn push(&mut self, i: usize, data: &Self);

    /// Takes samples in the batch.
    fn sample(&self, ixs: &Vec<usize>) -> Self;
}
