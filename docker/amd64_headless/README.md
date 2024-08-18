# Docker container for training

This directory contains scripts to build and run a docker container for training.

## Build a Docker image

```bash
cd $REPO/docker/aarch64_headless
sh build.sh
```

## Run

The following commands runs a program for training an agent.
The trained model will be saved in `$REPO/border/examples/model` directory,
which is mounted in the container.

### DQN

* Cartpole

  ```bash
  cd $REPO/docker/aarch64_headless
  sh run.sh "source /home/ubuntu/venv/bin/activate && cargo run --example dqn_cartpole --features='tch' -- --train"
  ```

  * Use a directory, not mounted on the host, as a cargo target directory,
    making compile faster on Mac, where access to mounted directories is slow.

    ```bash
    cd $REPO/docker/aarch64_headless
    sh run.sh "source /home/ubuntu/venv/bin/activate && CARGO_TARGET_DIR=/home/ubuntu/target cargo run --example dqn_cartpole --features='tch' -- --train"
    ```
