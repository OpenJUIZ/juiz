//! language: "zig" のプラグインを読み込んで呼べることを確かめるテスト (issue #572)。
//! Zig の例は zigjuiz (bindings/zigjuiz) で書いてあり、zigjuiz の値の読み書き・エラーのコード化も確かめる (issue #573)。
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
                "increment_process_zig": { "language": "zig", "path": "../target" },
                "greet_process_zig": { "language": "zig", "path": "../target" },
                "echo_types_process_zig": { "language": "zig", "path": "../target" },
                "fail_process_zig": { "language": "zig", "path": "../target" }
            }
        },
        "processes": [
            { "type_name": "increment_process_zig", "name": "inc0" },
            { "type_name": "greet_process_zig", "name": "greet0" },
            { "type_name": "echo_types_process_zig", "name": "echo0" },
            { "type_name": "fail_process_zig", "name": "fail0" }
        ]
    })
}

fn call(cb: &CoreBroker, name: &str, args: Value) -> JuizResult<Value> {
    let id = find(cb, name)?;
    let out = cb.process_call(&id, args.try_into()?)?;
    Ok(out.lock_as_value(|v| v.clone()).unwrap())
}

/// エラーが ZigPluginError で、コードと message が期待どおりか
fn assert_zig_error(result: JuizResult<Value>, expected_code: i64, expected_message: &str) {
    let e = result.expect_err("the call should fail");
    match e.downcast_ref::<JuizError>() {
        Some(JuizError::ZigPluginError { return_value, message, .. }) => {
            assert_eq!(*return_value, expected_code, "message={message:?}");
            assert!(message.contains(expected_message), "message={message:?}, expected {expected_message:?}");
        }
        _ => panic!("unexpected error: {e:#}"),
    }
}

fn find(cb: &CoreBroker, name: &str) -> JuizResult<ProcessIdentifier> {
    let ids = cb.process_list(false, None)?;
    Ok(ids
        .into_iter()
        .find(|id| id.name == name)
        .unwrap_or_else(|| panic!("process {name} not found")))
}

/// zigjuiz の exportProcess で書いた Zig の Process を読み込み、呼べる
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

            // マニフェストは zigjuiz が引数の struct から comptime に組み立てたもの
            let profile = cb.process_profile(&inc)?;
            assert_eq!(profile.type_name, "increment_process_zig");
            assert_eq!(profile.language, "zig");
            assert_eq!(profile.description.as_str(), "Adds 1 to arg1 (Zig).");
            assert_eq!(profile.arguments.len(), 1);
            assert_eq!(profile.arguments[0].name, "arg1");
            assert_eq!(profile.arguments[0].type_name.as_str(), "Int");
            assert_eq!(profile.arguments[0].description.as_str(), "value to be incremented");
            assert_eq!(profile.arguments[0].default, jvalue!(1));

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

/// 引数の型が違うときや、Zig の関数がエラーを返したときは、juiz のエラーになる (juiz は落ちない)
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
            let r = call(&cb, "inc0", jvalue!({ "arg1": "not a number" }));
            assert!(r.is_err(), "{r:?}");
            // i64 の最大値に 1 を足すと std.math.add が error.Overflow を返す。zigjuiz が -1101 とエラー文にする
            assert_zig_error(
                call(&cb, "inc0", jvalue!({ "arg1": i64::MAX })),
                -1101,
                "increment_process_zig returned error.Overflow",
            );
            // 自分で定義したエラー
            assert_eq!(call(&cb, "fail0", jvalue!({ "fail": false }))?, jvalue!(true));
            assert_zig_error(
                call(&cb, "fail0", jvalue!({ "fail": true })),
                -1101,
                "fail_process_zig returned error.RequestedFailure",
            );
            // 引数の struct のフィールドの型 (u8) に収まらない値は、引数のエラー (-1102)
            assert_zig_error(
                call(&cb, "echo0", jvalue!({ "small": 256 })),
                -1102,
                "argument 'small' of echo_types_process_zig: value does not fit in u8",
            );
            assert_zig_error(
                call(&cb, "echo0", jvalue!({ "u": -1 })),
                -1102,
                "argument 'u' of echo_types_process_zig: value does not fit in u64",
            );
            // エラーの後も続けて呼べる
            assert_eq!(call(&cb, "inc0", jvalue!({ "arg1": 1 }))?, jvalue!(2));
            Ok(())
        })
}

/// zigjuiz が読み書きできる型 (整数・浮動小数点・bool・文字列・juiz.Value) と、マニフェストの型名・既定値
#[test]
fn zigjuiz_reads_and_writes_values() -> JuizResult<()> {
    crate::common::logger::init();
    if skip_without_zig_examples() {
        return Ok(());
    }
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            let cb = system.core_broker().lock()?;

            // 既定値だけで呼ぶ
            let out = call(&cb, "echo0", jvalue!({}))?;
            assert_eq!(
                out,
                jvalue!({
                    "i": -1, "u": 2, "small": 3, "f": 0.5, "g": 1.5, "b": true, "s": "zig",
                    "v": { "k": 1 }, "v_k": 1
                })
            );

            // 値を渡す。辞書・配列は入れ子のまま往復し、Float の引数は整数の値も受ける
            let out = call(
                &cb,
                "echo0",
                jvalue!({
                    "i": i64::MIN, "u": u64::MAX, "small": 255, "f": 3, "g": 0.25, "b": false,
                    "s": "日本語も通る",
                    "v": { "k": "x", "list": [1, -2, 3.5, null, true, { "deep": [] }], "empty": {} }
                }),
            )?;
            assert_eq!(out["i"], jvalue!(i64::MIN));
            assert_eq!(out["u"], jvalue!(u64::MAX));
            assert_eq!(out["small"], jvalue!(255));
            assert_eq!(out["f"], jvalue!(3.0));
            assert_eq!(out["g"], jvalue!(0.25));
            assert_eq!(out["b"], jvalue!(false));
            assert_eq!(out["s"], jvalue!("日本語も通る"));
            assert_eq!(
                out["v"],
                jvalue!({ "k": "x", "list": [1, -2, 3.5, null, true, { "deep": [] }], "empty": {} })
            );
            assert_eq!(out["v_k"], jvalue!("x"));

            // マニフェストの型名と既定値
            let profile = cb.process_profile(&find(&cb, "echo0")?)?;
            let args: Vec<(String, String, Value)> = profile
                .arguments
                .iter()
                .map(|a| (a.name.clone(), a.type_name.as_str().to_owned(), a.default.clone()))
                .collect();
            assert_eq!(
                args,
                vec![
                    ("i".to_owned(), "Int".to_owned(), jvalue!(-1)),
                    ("u".to_owned(), "Int".to_owned(), jvalue!(2)),
                    ("small".to_owned(), "Int".to_owned(), jvalue!(3)),
                    ("f".to_owned(), "Float".to_owned(), jvalue!(0.5)),
                    ("g".to_owned(), "Float".to_owned(), jvalue!(1.5)),
                    ("b".to_owned(), "Bool".to_owned(), jvalue!(true)),
                    ("s".to_owned(), "String".to_owned(), jvalue!("zig")),
                    ("v".to_owned(), "Object".to_owned(), jvalue!({ "k": 1 })),
                ]
            );

            // juiz.Context の arena で作った文字列を返す
            assert_eq!(call(&cb, "greet0", jvalue!({}))?, jvalue!("Hello, juiz!"));
            assert_eq!(
                call(&cb, "greet0", jvalue!({ "name": "Zig", "times": 2 }))?,
                jvalue!("Hello, Zig! Hello, Zig!")
            );
            assert_eq!(call(&cb, "greet0", jvalue!({ "times": 0 }))?, jvalue!(""));
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
