#!/bin/bash

export PYTHONPATH=`pwd`/../../../../bindings/pyjuiz
export PWD=`pwd`
export DYLIB=$PWD/../python_talker.py 

cd ../../../../
# export RUST_LOG=
#export RUST_LOG=juiz_core=debug
# export RUST_LOG=juiz_core::brokers=debug #,juiz_sdk::connections::connection_manifest=debug
# export RUST_LOG=$RUST_LOG,juiz_core::processes=debug
# export RUST_LOG=juiz_core::processes::implementation=trace
# export RUST_LOG=rust_talker=info
cargo run -p juiz_app -- --process $DYLIB -l python -1 -r 1.0 -d


#export PATH=$PATH:~/.cargo/bin
#juiz --process ./target/debug/librust_talker.dylib -1 -r 1.0 