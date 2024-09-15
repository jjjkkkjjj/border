use anyhow::Result;
use border_core::Env;
use border_minari::{d4rl::ndarray::KitchenNdarrayConverter, MinariDataset};
use numpy::convert;
use core::panic;
use std::num;

fn main() -> Result<()> {
    let dataset = MinariDataset::load_dataset("D4RL/kitchen/complete-v1", true)?;

    // The number of transitions over all episodes
    let num_transitions = dataset.get_num_transitions(None)?;
    println!("{:?}", num_transitions);

    // Converter for observation and action
    let converter = KitchenNdarrayConverter {};

    // Create replay buffer for the sixth episode
    let replay_buffer = dataset.create_replay_buffer(&converter, Some(vec![5]))?;

    // Recover the environment from the dataset
    let mut env = dataset.recover_environment(converter, false, "human")?;

    // Sequence of actions in the episode
    let actions = replay_buffer.whole_actions();

    // Apply the actions to the environment
    env.reset(None)?;

    for ix in 220..actions.action.shape()[0] {
        let act = actions.get(ix);
        let _ = env.step(&act);
    }

    Ok(())
}
