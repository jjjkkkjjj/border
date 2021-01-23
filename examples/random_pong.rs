use std::error::Error;
use lrr::core::{Policy, util};
use lrr::py_gym_env::PyGymEnv;
use lrr::agents::tch::Shape;
use lrr::agents::tch::py_gym_env::obs::{TchPyGymEnvObs, TchPyGymEnvObsRawFilter};
use lrr::agents::tch::py_gym_env::act_d::{
    TchPyGymEnvDiscreteAct, TchPyGymEnvDiscreteActRawFilter
};

#[derive(Debug, Clone)]
struct ObsShape {}

impl Shape for ObsShape {
    fn shape() -> &'static [usize] {
        &[210, 160, 3]
    }
}

type ObsFilter = TchPyGymEnvObsRawFilter<ObsShape, u8>;
type ActFilter = TchPyGymEnvDiscreteActRawFilter;
type Obs = TchPyGymEnvObs<ObsShape, u8>;
type Act = TchPyGymEnvDiscreteAct<ActFilter>;
type Env = PyGymEnv<Obs, Act, ObsFilter>;

struct RandomPolicy {}

impl Policy<Env> for RandomPolicy {
    fn sample(&mut self, _: &Obs) -> Act {
        let v = fastrand::u32(..=5);
        Act::new(vec![v as i32])
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    tch::manual_seed(42);
    fastrand::seed(42);

    let obs_filter = ObsFilter::new();
    let mut env = Env::new("Pong-v0", obs_filter, false)?;
    env.set_render(true);
    let mut policy = RandomPolicy{};
    util::eval(&env, &mut policy, 5, None);

    Ok(())
}
