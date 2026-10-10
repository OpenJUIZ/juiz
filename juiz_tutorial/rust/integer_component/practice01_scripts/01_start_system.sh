#!/bin/bash
#export PATH=$PATH:$HOME/.cargo/bin
#juiz --process ./target/debug/librust_listener.dylib -1 -d
export FILEPATH=$(pwd)/juiz.conf

#export PWD=`pwd`
# export RUST_LOG=juiz_core=debug
# export RUST_LOG=juiz_core::core::system=trace,juiz_core::plugin=trace,juiz_core::core=trace
cd ../../../../
cargo run -p juiz_app -- -f $FILEPATH -d
