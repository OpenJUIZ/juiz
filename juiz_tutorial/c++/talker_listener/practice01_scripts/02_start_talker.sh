#!/bin/bash

export PWD=`pwd`
export DYLIB=$PWD/../build/cpp_talker/libcpp_talker.dylib 
cd ../../../../
#export RUST_LOG=juiz_core=debug
# export RUST_BACKTRACE=0
cargo run -p juiz_app  -- --process $DYLIB -1 -r 1.0 -d -l cpp
