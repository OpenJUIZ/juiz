#!/bin/bash
#export PATH=$PATH:$HOME/.cargo/bin
#juiz --process ./target/debug/librust_listener.dylib -1 -d
export FILEPATH=$(pwd)/juiz.conf

#export PWD=`pwd`
# export RUST_LOG=juiz_core=debug
# export RUST_LOG=juiz_core::core::system=trace,juiz_core::plugin=trace,juiz_core::core=trace
cd ../../../../

echo START SYSTEM TEST
echo '---------------------------------'
echo FIRST VALUE MUST BE 0.
cargo run -p juiz_app -- container-process call http://localhost:8000/container_process/get0::python_integer_container_get:container0 {} --print

echo '---------------------------------'
echo THEN SET 1.
cargo run -p juiz_app -- container-process call http://localhost:8000/container_process/set0::python_integer_container_set:container0 '{"value": 1}' 

echo THEN GET. VALUE MUST BE 1.
cargo run -p juiz_app -- container-process call http://localhost:8000/container_process/get0::python_integer_container_get:container0 '{}' --print

echo '---------------------------------'
echo THEN SET 2.
cargo run -p juiz_app -- container-process call http://localhost:8000/container_process/set0::python_integer_container_set:container0 '{"value": 2}' 

echo THEN GET. VALUE MUST BE 2.
cargo run -p juiz_app -- container-process call http://localhost:8000/container_process/get0::python_integer_container_get:container0 '{}' --print

echo '---------------------------------'
echo THEN SET 3.
cargo run -p juiz_app -- container-process call http://localhost:8000/container_process/set0::python_integer_container_set:container0 '{"value": 3}' 

echo THEN GET. VALUE MUST BE 3.
cargo run -p juiz_app -- container-process call http://localhost:8000/container_process/get0::python_integer_container_get:container0 '{}'  --print

echo '---------------------------------'
echo TEST_SYSTEM END.