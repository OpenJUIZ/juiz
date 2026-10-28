//! language: "haskell" の Component (1 つのライブラリに Process / Container / ContainerProcess をまとめたもの) を
//! 読み込んで呼べることを確かめるテスト (issue #134)
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
    let missing: Vec<_> = ["example_component_hs", "broken_component_hs"]
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
        "name": "haskell_component_test",
        "plugins": {
            "components": {
                "example_component_hs": { "language": "haskell", "path": "../target" }
            }
        },
        "processes": [
            { "type_name": "example_component_hs_increment", "name": "inc0" },
            { "type_name": "example_component_hs_greet", "name": "greet0" }
        ],
        "containers": [
            {
                "type_name": "example_component_hs_counter",
                "name": "c0",
                "value": 15,
                "processes": [
                    { "type_name": "example_component_hs_counter_increment", "name": "cinc0" },
                    { "type_name": "example_component_hs_counter_get", "name": "cget0" }
                ]
            },
            {
                // value を書かなければマニフェストの既定値 (0) になる
                "type_name": "example_component_hs_counter",
                "name": "c1",
                "processes": [
                    { "type_name": "example_component_hs_counter_get", "name": "cget1" }
                ]
            }
        ]
    })
}

fn find_process(cb: &CoreBroker, name: &str) -> JuizResult<ProcessIdentifier> {
    Ok(cb
        .process_list(false, None)?
        .into_iter()
        .find(|id| id.name == name)
        .unwrap_or_else(|| panic!("process {name} not found")))
}

fn find_container_process(cb: &CoreBroker, name: &str) -> JuizResult<ProcessIdentifier> {
    Ok(cb
        .container_process_list(false, None)?
        .into_iter()
        .find(|id| id.name == name)
        .unwrap_or_else(|| panic!("container process {name} not found")))
}

fn call(cb: &CoreBroker, id: &ProcessIdentifier, args: CapsuleMap) -> JuizResult<Value> {
    Ok(cb.process_call(id, args)?.lock_as_value(|v| v.clone())?)
}

fn call_cp(cb: &CoreBroker, id: &ProcessIdentifier, args: CapsuleMap) -> JuizResult<Value> {
    Ok(cb
        .container_process_call(id, args)?
        .lock_as_value(|v| v.clone())?)
}

/// 1 つのライブラリにまとめた Process / Container / ContainerProcess を、それぞれ作って呼べる
#[test]
fn haskell_component_can_be_loaded_and_called() -> JuizResult<()> {
    crate::common::logger::init();
    if skip_without_haskell_examples() {
        return Ok(());
    }
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            // GHC の major GC の後も関数と状態が残っていることを確かめるため、途中で GC を起こす
            let lib = unsafe { libloading::Library::new(plugin_file("example_component_hs"))? };
            let gc: libloading::Symbol<unsafe extern "C" fn()> =
                unsafe { lib.get(b"performMajorGC")? };
            let cb = system.core_broker().lock()?;

            let inc0 = find_process(&cb, "inc0")?;
            assert_eq!(
                call(&cb, &inc0, vec![("arg0", jvalue!(41))].into())?,
                jvalue!(42)
            );
            let greet0 = find_process(&cb, "greet0")?;
            assert_eq!(
                call(&cb, &greet0, vec![("name", jvalue!("Haskell"))].into())?,
                jvalue!("Hello, Haskell!")
            );

            let (cinc0, cget0) = (
                find_container_process(&cb, "cinc0")?,
                find_container_process(&cb, "cget0")?,
            );
            let cget1 = find_container_process(&cb, "cget1")?;
            assert_eq!(call_cp(&cb, &cget0, CapsuleMap::new())?, jvalue!(15));
            assert_eq!(call_cp(&cb, &cget1, CapsuleMap::new())?, jvalue!(0));
            assert_eq!(
                call_cp(&cb, &cinc0, vec![("arg0", jvalue!(3))].into())?,
                jvalue!(18)
            );
            unsafe { gc() };
            assert_eq!(call_cp(&cb, &cget0, CapsuleMap::new())?, jvalue!(18));
            assert_eq!(call_cp(&cb, &cget1, CapsuleMap::new())?, jvalue!(0));
            assert_eq!(
                call(&cb, &inc0, vec![("arg0", jvalue!(-1))].into())?,
                jvalue!(0)
            );

            // マニフェストは Haskell の ComponentManifest で組み立てたもの
            let p = cb.process_profile_full(&inc0)?;
            assert_eq!(p.type_name, "example_component_hs_increment");
            assert_eq!(p.description.as_str(), "Adds 1 to arg0.");
            let p = cb.container_process_profile_full(&cinc0)?;
            assert_eq!(p.type_name, "example_component_hs_counter_increment");
            assert_eq!(p.arguments[0].name, "arg0");

            // 引数の型が違えば、プロセスは落ちずに呼び出しがエラーになる
            let r = cb.process_call(&greet0, vec![("name", jvalue!(1))].into());
            assert!(r.is_err(), "{r:?}");
            Ok(())
        })
}

/// Component の Container を削除すると、<関数名>_factory_destroy_entry_point が呼ばれて destroy が動く
#[test]
fn haskell_component_container_is_destroyed() -> JuizResult<()> {
    crate::common::logger::init();
    if skip_without_haskell_examples() {
        return Ok(());
    }
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            let lib = unsafe { libloading::Library::new(plugin_file("example_component_hs"))? };
            let alive_count: libloading::Symbol<unsafe extern "C" fn() -> i64> =
                unsafe { lib.get(b"example_component_hs_alive_count")? };
            let alive = || unsafe { alive_count() };

            let mut cb = system.core_broker().lock_mut()?;
            let initial = alive();
            assert!(initial >= 2, "c0 と c1 が作られている: {initial}");

            let created = cb.container_create(
                &ContainerManifest::new("example_component_hs_counter").name("c2"),
                vec![("value", jvalue!(7))].into(),
            )?;
            assert_eq!(alive(), initial + 1);

            cb.container_destroy(&created.identifier())?;
            assert_eq!(alive(), initial);
            Ok(())
        })
}

/// Component に無い Container の ContainerProcess を入れた Component は、読み込むときにエラーになる
#[test]
fn haskell_broken_component_is_rejected() -> JuizResult<()> {
    crate::common::logger::init();
    if skip_without_haskell_examples() {
        return Ok(());
    }
    let r = System::new(jvalue!({
        "name": "haskell_broken_component_test",
        "plugins": {
            "components": {
                "broken_component_hs": { "language": "haskell", "path": "../target" }
            }
        }
    }))?
    .setup();
    assert!(r.is_err());
    Ok(())
}
