//! language: "zig" の Component (zigjuiz の exportComponent で 1 つのライブラリにまとめた Process / Container /
//! ContainerProcess) を読み込んで呼べることを確かめるテスト (issue #575)。Haskell の plugins::haskell::component にそろえている。
//!
//! Component の中の間違い (型名の重なり、Component に無い Container の ContainerProcess、状態の型違い) は
//! zigjuiz がコンパイルエラーにするので、bindings/zigjuiz/tests/compile_errors/check.sh で確かめる。

use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;

use super::{plugin_file, skip_without_zig_examples};

const EXAMPLES: [&str; 2] = ["example_component_zig", "increment_process_zig"];

fn system_manifest() -> Value {
    jvalue!({
        "name": "zig_component_test",
        "plugins": {
            "components": {
                "example_component_zig": { "language": "zig", "path": "../target" }
            }
        },
        "processes": [
            { "type_name": "example_component_zig_increment", "name": "inc0" },
            { "type_name": "example_component_zig_greet", "name": "greet0" }
        ],
        "containers": [
            {
                "type_name": "example_component_zig_counter",
                "name": "c0",
                "value": 15,
                "processes": [
                    { "type_name": "example_component_zig_counter_increment", "name": "cinc0" },
                    { "type_name": "example_component_zig_counter_get", "name": "cget0" }
                ]
            },
            {
                // value を書かなければマニフェストの既定値 (0) になる
                "type_name": "example_component_zig_counter",
                "name": "c1",
                "processes": [
                    { "type_name": "example_component_zig_counter_get", "name": "cget1" }
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
fn zig_component_can_be_loaded_and_called() -> JuizResult<()> {
    crate::common::logger::init();
    if skip_without_zig_examples(&EXAMPLES) {
        return Ok(());
    }
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            let cb = system.core_broker().lock()?;

            let inc0 = find_process(&cb, "inc0")?;
            assert_eq!(
                call(&cb, &inc0, vec![("arg0", jvalue!(41))].into())?,
                jvalue!(42)
            );
            let greet0 = find_process(&cb, "greet0")?;
            assert_eq!(
                call(&cb, &greet0, vec![("name", jvalue!("Zig"))].into())?,
                jvalue!("Hello, Zig!")
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
            assert_eq!(call_cp(&cb, &cget0, CapsuleMap::new())?, jvalue!(18));
            // c1 は c0 と別の状態
            assert_eq!(call_cp(&cb, &cget1, CapsuleMap::new())?, jvalue!(0));

            // マニフェストは zigjuiz の exportComponent が組み立てたもの
            let p = cb.process_profile(&inc0)?;
            assert_eq!(p.type_name, "example_component_zig_increment");
            assert_eq!(p.description.as_str(), "Adds 1 to arg0.");
            assert_eq!(p.language, "zig");
            assert_eq!(p.arguments[0].name, "arg0");
            assert_eq!(p.arguments[0].default, jvalue!(1));
            let p = cb.container_process_profile(&cinc0)?;
            assert_eq!(p.type_name, "example_component_zig_counter_increment");
            assert_eq!(p.language, "zig");
            assert_eq!(p.arguments[0].name, "arg0");
            assert!(!p.readonly);
            // 状態を *const で受ける ContainerProcess は readonly
            assert!(cb.container_process_profile(&cget0)?.readonly);
            let c0 = cb
                .container_list(false, None)?
                .into_iter()
                .find(|id| id.name == "c0")
                .expect("container c0 not found");
            let c = cb.container_profile(&c0)?;
            assert_eq!(c.type_name, "example_component_zig_counter");
            assert_eq!(c.language, "zig");

            // 引数の型が違えば、プロセスは落ちずに呼び出しがエラーになる
            let r = cb.process_call(&greet0, vec![("name", jvalue!(1))].into());
            assert!(r.is_err(), "{r:?}");
            // Zig のエラー (i64 の最大値に 1 を足す) は -1101 の呼び出しエラーになる
            let r = cb.process_call(&inc0, vec![("arg0", jvalue!(i64::MAX))].into());
            assert!(r.is_err(), "{r:?}");
            Ok(())
        })
}

/// Component の Container を削除すると、<型名>_factory_destroy_entry_point が呼ばれて deinit が動く
#[test]
fn zig_component_container_is_destroyed() -> JuizResult<()> {
    crate::common::logger::init();
    if skip_without_zig_examples(&EXAMPLES) {
        return Ok(());
    }
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            let lib = unsafe { libloading::Library::new(plugin_file("example_component_zig"))? };
            let alive_count: libloading::Symbol<unsafe extern "C" fn() -> i64> =
                unsafe { lib.get(b"example_component_zig_alive_count")? };
            let alive = || unsafe { alive_count() };

            let mut cb = system.core_broker().lock_mut()?;
            let initial = alive();
            assert!(initial >= 2, "c0 と c1 が作られている: {initial}");

            let created = cb.container_create(
                &ContainerManifest::new("example_component_zig_counter").name("c2"),
                vec![("value", jvalue!(7))].into(),
            )?;
            assert_eq!(alive(), initial + 1);

            cb.container_destroy(&created.identifier())?;
            assert_eq!(alive(), initial);
            Ok(())
        })
}

/// Component でないライブラリ (exportProcess だけのもの) を components に書くと、読み込むときにエラーになる
#[test]
fn zig_library_without_component_is_rejected() -> JuizResult<()> {
    crate::common::logger::init();
    if skip_without_zig_examples(&EXAMPLES) {
        return Ok(());
    }
    let r = System::new(jvalue!({
        "name": "zig_not_component_test",
        "plugins": {
            "components": {
                "increment_process_zig": { "language": "zig", "path": "../target" }
            }
        }
    }))?
    .setup();
    assert!(r.is_err());
    Ok(())
}
