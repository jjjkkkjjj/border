#![allow(clippy::float_cmp)]
use std::cell::RefCell;
use log::info;
use crate::{core::{Obs, Step, Env, Policy}};

/// The agent take an action and apply it to the environment.
/// Then return [crate::core::base::Step] object.
pub fn sample<E: Env, P: Policy<E>>(env: &E, policy: &mut P, obs_prev: &RefCell<Option<E::Obs>>) -> Step<E> {
    let obs = obs_prev.replace(None)
        .expect("Observation buffer is not initialized.");
    let a = policy.sample(&obs);
    let step = env.step(&a);

    // Replace observations if the corresponding environments are resetted.
    let obs_reset = env.reset(Some(&step.is_done)).unwrap();
    let obs_reset = step.obs.clone().merge(obs_reset, &step.is_done);
    obs_prev.replace(Some(obs_reset));

    step
}

/// This method assumes that `n_proc`=1.
pub fn eval<E: Env, P: Policy<E>>(env: &E, policy: &mut P, n_episodes_per_eval: usize, count_opts: Option<usize>) {
    // TODO: check the maximum number of steps of the environment for evaluation.
    // If it is infinite, the number of evaluation steps should be given in place of
    // n_episodes_per_eval.
    let mut rs = Vec::new();
    let obs = env.reset(None).unwrap();
    let obs_prev = RefCell::new(Some(obs));

    for _ in 0..n_episodes_per_eval {
        let mut r_sum = 0.0;
        loop {
            let step = sample(env, policy, &obs_prev);
            r_sum += &step.reward[0];
            if step.is_done[0] == 1.0 as f32 { break; }
        }
        rs.push(r_sum);
    }

    let mean: f32 = rs.iter().sum::<f32>() / n_episodes_per_eval as f32;
    let min = rs.iter().fold(f32::NAN, |m, v| v.min(m));
    let max = rs.iter().fold(f32::NAN, |m, v| v.max(m));
    if let Some(c) = count_opts {
        info!("Opt step {}, Eval (mean, min, max) of r_sum: {}, {}, {}",
            c, mean, min, max);
    }
    else {
        info!("Eval (mean, min, max) of r_sum: {}, {}, {}", mean, min, max);
    }
}
