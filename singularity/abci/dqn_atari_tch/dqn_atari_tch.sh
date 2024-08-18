#!/bin/bash

#$-l rt_G.small=1
#$-l h_rt=48:00:00
#$-j y
#$-cwd

source $HOME/.bashrc
PATH_TO_BORDER=$HOME/border
source /etc/profile.d/modules.sh
module load singularitypro
cd $PATH_TO_BORDER/singularity/abci
sh run.sh "mlflow server --host 127.0.0.1 --port 8080 & \
        sleep 5 && \
        ATARI_ROM=$HOME/atari_rom cargo run --release --example dqn_atari_tch --features=candle-tch -- $1 --mlflow"
