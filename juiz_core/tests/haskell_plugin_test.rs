//! language: "haskell" のプラグインを読み込んで呼べることを確かめるテスト (issue #131, #132, #133)
//!
//! Haskell の例 (examples/haskell) は scripts/test.sh (CI も同じ) で ../target にビルドされている前提。
//! GHC が無い環境では scripts/test.sh がビルドを飛ばすので、ライブラリが無ければテストも飛ばす。
//! CI では JUIZ_REQUIRE_HASKELL=1 にして、飛ばさずに失敗させる。

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

/// Haskell の例がビルドされていなければ true (テストを飛ばす)
fn skip_without_haskell_examples() -> bool {
    let missing: Vec<_> = ["increment_process_hs", "decrement_process_hs", "value_types_process_hs"]
        .iter()
        .map(|n| plugin_file(n))
        .filter(|p| !p.exists())
        .collect();
    if missing.is_empty() {
        return false;
    }
    if std::env::var("JUIZ_REQUIRE_HASKELL").map_or(false, |v| v == "1") {
        panic!("Haskell の例がビルドされていません: {missing:?}");
    }
    eprintln!(
        "skip: Haskell の例がビルドされていません ({missing:?})。GHC と cabal を入れて scripts/test.sh を実行してください。"
    );
    true
}

fn system_manifest() -> Value {
    jvalue!({
        "name": "haskell_plugin_test",
        "plugins": {
            "process_factories": {
                "increment_process_hs": { "language": "haskell", "path": "../target" },
                "decrement_process_hs": { "language": "haskell", "path": "../target" },
                "value_types_process_hs": { "language": "haskell", "path": "../target" }
            }
        },
        "processes": [
            { "type_name": "increment_process_hs", "name": "inc0" },
            { "type_name": "decrement_process_hs", "name": "dec0" },
            { "type_name": "value_types_process_hs", "name": "types0" }
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

fn call(cb: &CoreBroker, id: &ProcessIdentifier, arg1: i64) -> Value {
    cb.process_call(id, vec![("arg1", jvalue!(arg1))].into())
        .expect("call failed")
        .lock_as_value(|v| v.clone())
        .unwrap()
}

/// 二つの Haskell ライブラリを同じプロセスに読み込み、どちらも呼べる
#[test]
fn haskell_processes_can_be_loaded_and_called() -> JuizResult<()> {
    let _ = env_logger::try_init();
    if skip_without_haskell_examples() {
        return Ok(());
    }
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            let cb = system.core_broker().lock()?;
            let inc = find(&cb, "inc0")?;
            let dec = find(&cb, "dec0")?;
            for v in [-3, 0, 41, i64::MAX - 1] {
                assert_eq!(call(&cb, &inc, v), jvalue!(v + 1), "increment({v})");
                assert_eq!(call(&cb, &dec, v), jvalue!(v - 1), "decrement({v})");
            }

            // マニフェストは Haskell 側で組み立てたもの (Process のプロファイルの language は今は C++ と同じく "rust" になる)
            let profile = cb.process_profile_full(&inc)?;
            assert_eq!(profile.type_name, "increment_process_hs");
            assert_eq!(profile.arguments[0].name, "arg1");

            // 引数の型が違えば Haskell の例外ではなくエラーコードで返り、juiz はエラーにする
            let r = cb.process_call(&inc, vec![("arg1", jvalue!("not a number"))].into());
            assert!(r.is_err(), "{r:?}");
            Ok(())
        })
}

fn value_types_args(name: &str, items: Value) -> CapsuleMap {
    vec![
        ("flag", jvalue!(true)),
        ("count", jvalue!(21)),
        ("ratio", jvalue!(0.25)),
        ("name", jvalue!(name)),
        ("items", items),
        ("extra", jvalue!({"a": 1, "b": {"c": [true, null]}})),
    ]
    .into()
}

/// hsjuiz の FromCapsule / ToCapsule で、いろいろな型の引数と、入れ子のオブジェクトの戻り値をやりとりできる
#[test]
fn haskell_process_converts_value_types() -> JuizResult<()> {
    let _ = env_logger::try_init();
    if skip_without_haskell_examples() {
        return Ok(());
    }
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            let cb = system.core_broker().lock()?;
            let id = find(&cb, "types0")?;

            // マニフェストは hsjuiz の ProcessManifest ビルダーで組み立てたもの。引数は足した順
            let profile = cb.process_profile_full(&id)?;
            let names: Vec<_> = profile.arguments.iter().map(|a| a.name.as_str()).collect();
            assert_eq!(names, ["flag", "count", "ratio", "name", "items", "extra"]);
            assert_eq!(profile.description.as_str(), "Echoes its arguments as an object.");

            let out = cb
                .process_call(&id, value_types_args("ロボット juiz", jvalue!([1, 2, 3])))?
                .lock_as_value(|v| v.clone())
                .unwrap();
            assert_eq!(
                out,
                jvalue!({
                    "flag": false,
                    "count": 42,
                    "ratio": 0.5,
                    "name": "ロボット JUIZ",
                    "items": [3, 2, 1],
                    "sum": 6,
                    "extra": {"a": 1, "b": {"c": [true, null]}, "seen": true},
                    "nothing": null,
                    "nested": [{"k": "v"}, [null, 1.5]]
                })
            );

            // 型が合わない引数、Haskell の error、JuizError は、どれもプロセスを落とさずエラーになる
            for (name, items) in [
                ("ok", jvalue!(["not", "ints"])),
                ("throw", jvalue!([])),
                ("juiz_error", jvalue!([])),
            ] {
                let r = cb.process_call(&id, value_types_args(name, items.clone()));
                assert!(r.is_err(), "{name} {items}: {r:?}");
            }
            // エラーの後も呼べる
            let out = cb
                .process_call(&id, value_types_args("again", jvalue!([])))?
                .lock_as_value(|v| v.clone())
                .unwrap();
            assert_eq!(out["name"], jvalue!("AGAIN"));
            Ok(())
        })
}

/// juiz が使うのと同じ C ABI で、複数のスレッドから同時に Haskell の関数を呼べる (-threaded の RTS)
#[test]
fn haskell_process_can_be_called_from_many_threads() -> JuizResult<()> {
    let _ = env_logger::try_init();
    if skip_without_haskell_examples() {
        return Ok(());
    }
    type ProcessFunction = unsafe extern "C" fn(*mut CapsuleMap, *mut Capsule) -> i64;
    // juiz と同じく、ライブラリを開いて juiz_hs_init を呼ぶのは一つのスレッドで順に行う
    let functions: Vec<(&str, i64, ProcessFunction)> =
        [("increment_process_hs", 1), ("decrement_process_hs", -1)]
            .into_iter()
            .map(|(name, delta)| unsafe {
                let lib = libloading::Library::new(plugin_file(name)).unwrap();
                let init: libloading::Symbol<unsafe extern "C" fn() -> i64> =
                    lib.get(b"juiz_hs_init").unwrap();
                assert_eq!(init(), 0);
                let entry: libloading::Symbol<unsafe extern "C" fn() -> ProcessFunction> =
                    lib.get(b"process_factory_entry_point").unwrap();
                let f = entry();
                // RTS を動かしたまま dlclose しないよう、juiz と同じくライブラリは閉じない
                std::mem::forget(lib);
                (name, delta, f)
            })
            .collect();
    let handles: Vec<_> = functions
        .into_iter()
        .flat_map(|(name, delta, f)| (0..4i64).map(move |t| (name, delta, f, t)))
        .map(|(name, delta, f, t)| {
            std::thread::spawn(move || {
                for i in 0..200i64 {
                    let arg = t * 1000 + i;
                    let mut args: CapsuleMap = vec![("arg1", jvalue!(arg))].into();
                    let mut out = Capsule::empty();
                    assert_eq!(unsafe { f(&mut args, &mut out) }, 0);
                    let v = out.as_value().unwrap().clone();
                    assert_eq!(v, jvalue!(arg + delta), "{name}({arg})");
                }
            })
        })
        .collect();
    for h in handles {
        h.join().expect("thread panicked");
    }
    Ok(())
}

/// GHC の major GC の後も、2 つ目以降に読み込んだ Haskell のライブラリを呼べる (issue #133)。
/// hs_init の後に開いたライブラリの foreign export は GC の根に登録されないので、hsjuiz の juiz_hs_init が
/// setKeepCAFs で CAF を残している。残さないと、生成したエントリポイント (CAF) が回収されてプロセスが落ちる
#[test]
fn haskell_processes_survive_major_gc() -> JuizResult<()> {
    let _ = env_logger::try_init();
    if skip_without_haskell_examples() {
        return Ok(());
    }
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            // juiz が開いたのと同じライブラリから、共有の RTS の performMajorGC を取り出す
            let lib = unsafe { libloading::Library::new(plugin_file("increment_process_hs"))? };
            let gc: libloading::Symbol<unsafe extern "C" fn()> = unsafe { lib.get(b"performMajorGC")? };
            let cb = system.core_broker().lock()?;
            let inc = find(&cb, "inc0")?;
            let dec = find(&cb, "dec0")?;
            let types = find(&cb, "types0")?;
            for v in 0..3 {
                assert_eq!(call(&cb, &inc, v), jvalue!(v + 1));
                assert_eq!(call(&cb, &dec, v), jvalue!(v - 1));
                let out = cb
                    .process_call(&types, value_types_args("gc", jvalue!([v])))?
                    .lock_as_value(|v| v.clone())
                    .unwrap();
                assert_eq!(out["sum"], jvalue!(v));
                unsafe { gc() };
            }
            Ok(())
        })
}

/// juiz_hs_init を公開していないライブラリを language: "haskell" で読み込むとエラーになる
#[test]
fn haskell_plugin_without_init_function_is_rejected() -> JuizResult<()> {
    let _ = env_logger::try_init();
    // C++ の例は juiz_hs_init を持たない
    let manifest = jvalue!({
        "name": "haskell_plugin_without_init_test",
        "plugins": {
            "process_factories": {
                "increment_process_cpp": { "language": "haskell", "path": "../target" }
            }
        }
    });
    let r = System::new(manifest)?.setup();
    let err = r.err().expect("setup must fail");
    let msg = format!("{err:?}");
    assert!(msg.contains("juiz_hs_init"), "{msg}");
    Ok(())
}
