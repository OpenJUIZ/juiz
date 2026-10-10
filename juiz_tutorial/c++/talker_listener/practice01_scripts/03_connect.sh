#!/bin/bash

export RUST_LOG=juiz_core::processes=trace,juiz_core::connections=trace,juiz_core::brokers=trace,juiz_sdk::connections::connection_manifest=debug
cd ../../../../
cargo run -p juiz_app -- connection create -t PUSH "http://127.0.0.1:8001/process/cpp_talker0::cpp_talker" arg1 "http://127.0.0.1:8000/process/cpp_listener0::cpp_listener" 