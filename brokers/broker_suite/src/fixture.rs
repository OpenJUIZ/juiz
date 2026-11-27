//! 確認の相手にする System の設定と、その中の Process・Container の名前

use std::path::PathBuf;

use juiz_core::prelude::*;

/// 確認の相手の System に置いてある Process・Container の名前。
/// [`crate::api`] の確認はここに書いたものがあることを前提にする。
pub struct Fixture {
    /// Process の (名前, 型名)。どれも引数 arg1 に 1 を足して返す
    pub processes: &'static [(&'static str, &'static str)],
    /// Container の (Container 名, 加算する ContainerProcess 名, 加算の引数名, 値を返す ContainerProcess 名)
    pub containers: &'static [(&'static str, &'static str, &'static str, &'static str)],
    /// Pull で繋ぐ (出力側, 入力側) の Process 名。入力側は arg1 で受ける
    pub connect: (&'static str, &'static str),
}

/// EC の名前。どの Fixture も TimerEC をこの名前で 1 つ持つ
pub const TIMER_EC: &str = "timer0";

/// Rust の例のプラグインだけを読んだ System ([`rust_system_manifest`]) の中身
pub const RUST_ONLY: Fixture = Fixture {
    processes: &[("inc0", "increment_process"), ("inc1", "increment_process")],
    containers: &[("c0", "c0inc", "arg1", "c0get")],
    connect: ("inc0", "inc1"),
};

/// 例のプラグインと juiz コマンドの置き場所を変える環境変数。
/// juiz の外のリポジトリ (OpenJUIZ/juiz_zenoh_broker など) がこのクレートを git の依存で使うとき、
/// CARGO_MANIFEST_DIR は cargo の git のチェックアウトの中になり、そこには何もビルドされていないので、
/// juiz をビルドした target/debug をこれで渡す (issue #639)
pub const TARGET_DIR_ENV: &str = "JUIZ_BROKER_SUITE_TARGET_DIR";

/// 例のプラグインと juiz コマンドの置き場所。[`TARGET_DIR_ENV`] があればそれ、無ければワークスペースの target/debug
pub fn target_dir() -> PathBuf {
    match std::env::var_os(TARGET_DIR_ENV) {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug"),
    }
}

pub fn plugin_path() -> String {
    target_dir().to_string_lossy().into_owned()
}

/// Rust の例のプラグイン (increment_process, example_container, timer_ec) を読み、
/// [`RUST_ONLY`] の Process・Container・EC を置いた System の設定。
/// `brokers` はそのまま "brokers" に、`broker_factories` は "plugins.broker_factories" に入る。
/// HTTP ブローカーは起動しない (http_port は設定にだけ書く)。
pub fn rust_system_manifest(
    name: &str,
    http_port: i64,
    brokers: Value,
    broker_factories: Value,
) -> Value {
    let path = plugin_path();
    jvalue!({
        "name": name,
        "option": {
            "http_broker": { "start": false, "port": http_port }
        },
        "plugins": {
            "broker_factories": broker_factories,
            "process_factories": {
                "increment_process": { "path": path }
            },
            "container_factories": {
                "example_container": {
                    "path": path,
                    "processes": {
                        "example_container_increment": { "path": path },
                        "example_container_get": { "path": path }
                    }
                }
            },
            "ec_factories": {
                "timer_ec": { "path": path }
            }
        },
        "brokers": brokers,
        "processes": [
            { "type_name": "increment_process", "name": "inc0" },
            { "type_name": "increment_process", "name": "inc1" }
        ],
        "containers": [
            { "type_name": "example_container", "name": "c0",
              "processes": [
                  { "type_name": "example_container_increment", "name": "c0inc" },
                  { "type_name": "example_container_get", "name": "c0get" } ] }
        ],
        "ecs": [
            { "type_name": "TimerEC", "name": TIMER_EC, "rate": 100.0 }
        ]
    })
}
