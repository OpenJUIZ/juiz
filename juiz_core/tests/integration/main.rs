//! juiz_core の統合テスト
//!
//! テストファイルごとに別のバイナリにすると、それぞれが juiz_core と依存クレートを丸ごとリンクして
//! (1 本 ~110MB)、lib を少し変えただけでも再リンクに時間がかかる。そこで tests/integration/ の下の
//! テストをすべてこの 1 本のバイナリにまとめている。新しいテストはこのディレクトリにファイルを置き、
//! 下に `mod` を足すこと(tests/ 直下に置くと別のバイナリになる)。
//!
//! 一部だけ実行するときはモジュール名で絞り込む。
//!
//! ```sh
//! scripts/test.sh -p juiz_core --test integration -- system_test
//! ```
//!
//! 1 つのプロセスで全テストが走るので、logger の設定は `common::logger::init()` に揃えること。
//! Python のプラグインなどは tests/python と tests/confs から読む(パスは変わらない)。

mod broker_api;
mod common;

mod composite_api_test;
mod composite_bench;
mod composite_conf_test;
mod composite_container_test;
mod composite_display_test;
mod composite_process_test;
mod composite_type_check_test;
mod connection_test;
mod core_broker_api_test;
mod core_broker_integration_test;
mod core_broker_test;
mod cpp_container_destroy_test;
mod cpp_return_type_test;
mod cpp_typed_test;
mod cpp_value_types_test;
mod ec_bind_test;
mod haskell_component_test;
mod haskell_container_test;
mod haskell_plugin_test;
mod haskell_typed_test;
mod http_broker_api_test;
mod http_router_test;
mod identifier_test;
mod lazy_eval_test;
mod local_broker_api_test;
mod messenger_broker_proxy_test;
mod namespace_broker_test;
mod namespace_test;
mod process_factory_test;
mod process_test;
mod pyjuiz_proxy_test;
mod python_datatype_test;
mod python_plugin_path_test;
mod rust_native_test;
mod subsystem_test;
mod system_test;
mod type_check_test;
mod viewer_api_test;
mod webui_test;
