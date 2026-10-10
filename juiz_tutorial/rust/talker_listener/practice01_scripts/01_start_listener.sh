#!/bin/bash
#export PATH=$PATH:$HOME/.cargo/bin
#juiz --process ./target/debug/librust_listener.dylib -1 -d

export PWD=`pwd`
export DYLIB=$PWD/../target/debug/librust_listener.dylib 
# export RUST_LOG=juiz_core=debug


cd ../../../../
cargo run -p juiz_app -- --process $DYLIB -1 -d
