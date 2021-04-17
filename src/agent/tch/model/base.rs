//! Definition of interfaces of neural networks.
use std::{path::Path, error::Error};
use tch::{Tensor, nn, nn::VarStore};

/// Base interface.
pub trait ModelBase {
    /// Trains the network given a loss.
    fn backward_step(&mut self, loss: &Tensor);

    /// Returns `var_store`.
    fn get_var_store(&mut self) -> &mut nn::VarStore;

    /// Save parameters of the neural network.
    fn save<T: AsRef<Path>>(&self, path: T) -> Result<(), Box<dyn Error>>;

    /// Load parameters of the neural network.
    fn load<T: AsRef<Path>>(&mut self, path: T) -> Result<(), Box<dyn Error>>;
}

/// Neural networks with a single input and a single output. 
pub trait Model1: ModelBase {
    /// The input of the neural network.
    type Input;
    /// The output of the neural network.
    type Output;

    /// Performs forward computation given an input.
    fn forward(&self, xs: &Self::Input) -> Self::Output;

    /// TODO: check places this method is used in code.
    fn in_shape(&self) -> &[usize];

    /// TODO: check places this method is used in code.
    fn out_dim(&self) -> usize;
}

/// Neural networks with double inputs and a single output.
pub trait Model2: ModelBase {
    /// An input of the neural network.
    type Input1;
    /// The other input of the neural network.
    type Input2;
    /// The output of the neural network.
    type Output;

    /// Performs forward computation given a pair of inputs.
    fn forward(&self, x1s: &Self::Input1, x2s: &Self::Input2) -> Self::Output;
}

/// Neural network model that can be initialized with [VarStore] and configuration.
///
/// The purpose of this trait is for modularity of neural network models.
/// Modules, which consists a neural network, should share [VarStore].
/// To do this, structs implementing this trait can be initialized with a given [VarStore].
/// This trait also provide the ability to clone with a given [VarStore].
/// The ability is useful when creating a target network, used in recent deep learning algorithms in common.
pub trait SubModel {
    /// Configuration from which [SubModel] is constructed.
    type Config;

    /// Input of the [SubModel].
    type Input;

    /// Output of the [SubModel].
    type Output;

    /// Builds [SubModel] with [VarStore] and [Self::Config].
    fn build(var_store: &VarStore, config: Self::Config) -> Self;

    /// Clones [SubModel] with [VarStore].
    fn clone_with_var_store(&self, var_store: &VarStore) -> Self;

    /// A generalized forward function.
    fn forward(&self, input: &Self::Input) -> Self::Output;
}
