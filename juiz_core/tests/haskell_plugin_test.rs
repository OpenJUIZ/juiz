//! language: "haskell" のプラグインを読み込んで呼べることを確かめるテスト (issue #131)
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
    let missing: Vec<_> = ["increment_process_hs", "decrement_process_hs"]
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
                "decrement_process_hs": { "language": "haskell", "path": "../target" }
            }
        },
        "processes": [
            { "type_name": "increment_process_hs", "name": "inc0" },
            { "type_name": "decrement_process_hs", "name": "dec0" }
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
