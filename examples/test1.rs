use std::cell::RefCell;
use ndarray::Array2;

trait Obs {}

trait Info {}

trait Env {
    type Obs: Obs;
    type Act;
    type Info: Info;

    fn step(&self, a: &Self::Act) -> (Self::Obs, f32, bool, Self::Info);
}

trait Policy<E: Env> {
    fn sample(&self, obs: &E::Obs) -> E::Act;
}

// struct Sampler<E: Env, P: Policy> {
//     env: E,
//     pi : P,
//     obs: RefCell<E::Obs>,
// }

// impl<E: Env, P: Policy> Sampler<E, P> {
//     fn sample(&self) {
//         let o = self.obs.into_inner();
//         let a = self.pi.sample(&o);
//         // self.env.step()
//     }
// }

// ----------

struct MyAct (f32);

struct MyObs (f32);

struct MyInfo ();

struct MyPolicy ();

struct MyEnv {
    state: RefCell<Array2::<f32>>
}

impl Obs for MyObs {}

impl Info for MyInfo {}

impl Policy<MyEnv> for MyPolicy {
    fn sample(&self, _: &MyObs) -> MyAct {
        MyAct(1.0)
    }
}

impl Env for MyEnv {
    type Obs  = MyObs;
    type Act  = MyAct;
    type Info = MyInfo;

    fn step(&self, a: &MyAct) -> (MyObs, f32, bool, MyInfo) {
        (MyObs(0.1), 0.0, true, MyInfo{})
    }
}

impl MyEnv {
    fn new() -> Self {
        MyEnv {
            state: RefCell::new(Array2::<f32>::zeros((3, 4)))
        }
    }
}

// ----------

fn main() {
    let env = MyEnv::new();
    let act = MyAct(1.0);
    let ret = env.step(&act);

    println!("finished!");
}
