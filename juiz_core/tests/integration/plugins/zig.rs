//! language: "zig" のプラグインを読み込んで呼べることを確かめるテスト (issue #572)
//!
//! Zig の例 (examples/zig) は scripts/test.sh (CI も同じ) で ../target にビルドされている前提。
//! zig が無い環境では scripts/test.sh がビルドを飛ばすので、ライブラリが無ければテストも飛ばす。
//! CI では JUIZ_REQUIRE_ZIG=1 にして、飛ばさずに失敗させる。

use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;

fn plugin_file(name: &str) -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    let file = format!("lib{name}.dylib");
    #[cfg(target_os = "linux")]
    let file = format!("lib{name}.so");
    #[cfg(target_os = "windows")]
    let file = format!("{name}.dll");
    std::path::PathBuf::from("../target").join(file)
}

/// Zig の例がビルドされていなければ true (テストを飛ばす)
fn skip_without_zig_examples() -> bool {
    let path = plugin_file("increment_process_zig");
    if path.exists() {
        return false;
    }
    if std::env::var("JUIZ_REQUIRE_ZIG").map_or(false, |v| v == "1") {
        panic!("Zig の例がビルドされていません: {path:?}");
    }
    eprintln!(
        "skip: Zig の例がビルドされていません ({path:?})。Zig 0.17 を入れて scripts/test.sh を実行してください。"
    );
    true
}

fn system_manifest() -> Value {
    jvalue!({
        "name": "zig_plugin_test",
        "plugins": {
            "process_factories": {
                "increment_process_zig": { "language": "zig", "path": "../target" }
            }
        },
        "processes": [
            { "type_name": "increment_process_zig", "name": "inc0" }
        ]
    })
}

fn find(cb: &CoreBroker, name: &str) -> JuizResult<ProcessIdentifier> {
    let ids = cb.process_list(false, None)?;
    Ok(ids
        .into_iter()
        .find(|id| id.name == name)
        .unwrap_or_else(|| panic!("process {name} not found")))
}

/// zigjuiz を使わずに C ABI の関数を手で書いた Zig の Process を読み込み、呼べる
#[test]
fn zig_process_can_be_loaded_and_called() -> JuizResult<()> {
    crate::common::logger::init();
    if skip_without_zig_examples() {
        return Ok(());
    }
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            let cb = system.core_broker().lock()?;
            let inc = find(&cb, "inc0")?;
            for v in [-3, 0, 41, i64::MAX - 1] {
                let out = cb
                    .process_call(&inc, vec![("arg1", jvalue!(v))].into())?
                    .lock_as_value(|v| v.clone())
                    .unwrap();
                assert_eq!(out, jvalue!(v + 1), "increment({v})");
            }

            // マニフェストは Zig 側で組み立てたもの
            let profile = cb.process_profile(&inc)?;
            assert_eq!(profile.type_name, "increment_process_zig");
            assert_eq!(profile.language, "zig");
            assert_eq!(profile.arguments[0].name, "arg1");

            // Factory の profile のプラグインの言語は zig (CAbiLanguage::Zig)
            let pf = cb
                .worker()
                .store()
                .processes
                .factory("increment_process_zig")?
                .lock()?
                .profile_full()?;
            assert_eq!(pf["plugin"]["language"], "zig", "{pf}");

            // 引数を省くとマニフェストの既定値 (1) で呼ばれる
            let out = cb
                .process_call(&inc, CapsuleMap::new())?
                .lock_as_value(|v| v.clone())
                .unwrap();
            assert_eq!(out, jvalue!(2));
            Ok(())
        })
}

/// 引数の型が違うときや、Zig 側でエラーを返したときは、juiz のエラーになる (juiz は落ちない)
#[test]
fn zig_process_errors_are_returned_as_juiz_errors() -> JuizResult<()> {
    crate::common::logger::init();
    if skip_without_zig_examples() {
        return Ok(());
    }
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            let cb = system.core_broker().lock()?;
            let inc = find(&cb, "inc0")?;
            let r = cb.process_call(&inc, vec![("arg1", jvalue!("not a number"))].into());
            assert!(r.is_err(), "{r:?}");
            // i64 の最大値に 1 を足すと桁あふれ。例は panic せずにエラーコードで返す
            let r = cb.process_call(&inc, vec![("arg1", jvalue!(i64::MAX))].into());
            assert!(r.is_err(), "{r:?}");
            // エラーの後も続けて呼べる
            let out = cb
                .process_call(&inc, vec![("arg1", jvalue!(1))].into())?
                .lock_as_value(|v| v.clone())
                .unwrap();
            assert_eq!(out, jvalue!(2));
            Ok(())
        })
}

/// ライブラリが無いとき、Zig のビルドの手順を案内する
#[test]
fn zig_plugin_not_found_shows_build_hint() -> JuizResult<()> {
    crate::common::logger::init();
    let manifest = jvalue!({
        "name": "zig_plugin_not_found_test",
        "plugins": {
            "process_factories": {
                "no_such_process_zig": { "language": "zig", "path": "../target" }
            }
        }
    });
    let r = System::new(manifest)?.setup();
    let msg = format!("{:?}", r.err().expect("setup must fail"));
    assert!(msg.contains("zig build"), "{msg}");
    Ok(())
}
