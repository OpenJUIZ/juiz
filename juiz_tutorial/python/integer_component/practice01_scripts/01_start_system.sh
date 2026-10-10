#!/bin/bash
#export PATH=$PATH:$HOME/.cargo/bin
#juiz --process ./target/debug/librust_listener.dylib -1 -d

export PYTHONPATH=`pwd`/../../../../bindings/pyjuiz
export PWD=`pwd`

export FILEPATH=$(pwd)/juiz.conf
export RUST_LOG=juiz_core=info
#export RUST_BACKTRACE=1

cd ../../../../
cargo run -p juiz_app -- -f $FILEPATH -d
# cargo run -p juiz_app -- --process $DYLIB -l python -1 -d
