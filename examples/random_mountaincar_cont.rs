use std::error::Error;
use lrr::core::{Policy, util};
use ndarray::Array;
use lrr::py_gym_env::PyGymEnv;
use lrr::agents::tch::Shape;
use lrr::agents::tch::py_gym_env::{TchPyGymEnvObs, TchPyGymEnvContinuousAct};

#[derive(Debug, Clone)]
struct MountainCarObsShape {}

impl Shape for MountainCarObsShape {
    fn shape() -> &'static [usize] {
        &[2]
    }
}

#[derive(Debug, Clone)]
struct MountainCarActShape {}

impl Shape for MountainCarActShape {
    fn shape() -> &'static [usize] {
        &[1]
    }
}

type O = TchPyGymEnvObs<MountainCarObsShape, f64>;
type A = TchPyGymEnvContinuousAct<MountainCarActShape>;
type E = PyGymEnv<O, A>;

struct RandomPolicy {}

impl Policy<E> for RandomPolicy {
    fn sample(&mut self, _: &O) -> A {
        A::new(Array::from(vec![2.0 * fastrand::f32() - 1.0]).into_dyn())
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    tch::manual_seed(42);
    fastrand::seed(42);

    let mut env = E::new("MountainCarContinuous-v0", true)?;
    env.set_render(true);
    let mut policy = RandomPolicy{};
    util::eval(&env, &mut policy, 5, None);

    Ok(())
}
