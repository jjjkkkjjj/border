use std::{path::Path, error::Error, fmt, fmt::{Formatter, Debug}};
use log::{info, trace};
use tch::{Tensor, nn, nn::Module, Device, nn::OptimizerConfig};
use crate::agents::tch::model::{ModelBase, Model1};

pub struct Model1_2 {
    var_store: nn::VarStore,
    network_fn: fn(&nn::Path, usize, usize) -> nn::Sequential,
    network: nn::Sequential,
    device: Device,
    opt: nn::Optimizer<nn::Adam>,
    head_mean: nn::Linear,
    head_lstd: nn::Linear,
    in_dim: usize,
    hidden_dim: usize,
    out_dim: usize,
    learning_rate: f64
}

// TODO: implement this
impl Debug for Model1_2 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result { Ok(()) }
}

impl Clone for Model1_2 {
    fn clone(&self) -> Self {
        let mut new = Self::new(self.in_dim, self.hidden_dim, self.out_dim,
                                self.learning_rate, self.network_fn);
        new.var_store.copy(&self.var_store).unwrap();
        new
    }
}

impl Model1_2 {
    pub fn new(in_dim: usize, hidden_dim: usize, out_dim: usize, learning_rate: f64,
        network_fn: fn(&nn::Path, usize, usize) -> nn::Sequential) -> Self {
        let vs = nn::VarStore::new(tch::Device::Cpu);
        let p = &vs.root();
        let network = network_fn(p, in_dim, hidden_dim);
        let head_mean = nn::linear(p / "ml", hidden_dim as _, out_dim as _, Default::default());
        let head_lstd = nn::linear(p / "sl", hidden_dim as _, out_dim as _, Default::default());

        let opt = nn::Adam::default().build(&vs, learning_rate).unwrap();
        Self {
            network,
            network_fn,
            device: p.device(),
            var_store: vs,
            in_dim,
            hidden_dim,
            out_dim,
            head_mean,
            head_lstd,
            opt,
            learning_rate,
        }
    }
}

impl ModelBase for Model1_2 {
    fn backward_step(&mut self, loss: &Tensor) {
        self.opt.backward_step(loss);
    }

    fn get_var_store(&mut self) -> &mut nn::VarStore {
        &mut self.var_store
    }

    fn save<T: AsRef<Path>>(&self, path: T) -> Result<(), Box<dyn Error>> {
        self.var_store.save(&path)?;
        info!("Save qnet to {:?}", path.as_ref());
        let vs = self.var_store.variables();
        for (name, _) in vs.iter() {
            trace!("Save variable {}", name);
        };
        Ok(())
    }

    fn load<T: AsRef<Path>>(&mut self, path: T) -> Result<(), Box<dyn Error>> {
        self.var_store.load(&path)?;
        info!("Load qnet from {:?}", path.as_ref());
        Ok(())
    }
}

impl Model1 for Model1_2 {
    type Input = Tensor;
    type Output = (Tensor, Tensor);

    fn forward(&self, xs: &Tensor) -> Self::Output {
        trace!("Model1_2.forward()");
        trace!("xs.size() = {:?}", xs.size());
        let xs = self.network.forward(xs);
        trace!("network.forward(xs).size() = {:?}", xs.size());
        (xs.apply(&self.head_mean), xs.apply(&self.head_lstd).exp())
    }
}