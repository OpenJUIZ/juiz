//! language: "zig" の Container と ContainerProcess (zigjuiz の exportContainer / exportContainerProcess) を
//! 読み込んで呼べることを確かめるテスト (issue #574)。Haskell の plugins::haskell::container にそろえている。

use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;

use super::{plugin_file, skip_without_zig_examples};

const EXAMPLES: [&str; 4] = [
    "example_container_zig",
    "example_container_zig_increment",
    "example_container_zig_get",
    "mismatched_container_process_zig",
];

fn system_manifest() -> Value {
    jvalue!({
        "name": "zig_container_test",
        "plugins": {
            "container_factories": {
                "example_container_zig": {
                    "language": "zig",
                    "path": "../target",
                    "processes": {
                        "example_container_zig_increment": { "path": "../target" },
                        "example_container_zig_get": { "path": "../target" }
                    }
                }
            }
        },
        "containers": [
            {
                "type_name": "example_container_zig",
                "name": "c0",
                "value": 15,
                "processes": [
                    { "type_name": "example_container_zig_increment", "name": "inc0" },
                    { "type_name": "example_container_zig_get", "name": "get0" }
                ]
            },
            {
                // value を書かなければマニフェストの既定値 (0) になる
                "type_name": "example_container_zig",
                "name": "c1",
                "processes": [
                    { "type_name": "example_container_zig_increment", "name": "inc1" },
                    { "type_name": "example_container_zig_get", "name": "get1" }
                ]
            }
        ]
    })
}

fn find(cb: &CoreBroker, name: &str) -> JuizResult<ProcessIdentifier> {
    let ids = cb.container_process_list(false, None)?;
    Ok(ids
        .into_iter()
        .find(|id| id.name == name)
        .unwrap_or_else(|| panic!("container process {name} not found")))
}

fn call(cb: &CoreBroker, id: &ProcessIdentifier, args: CapsuleMap) -> JuizResult<Value> {
    Ok(cb.container_process_call(id, args)?.lock_as_value(|v| v.clone())?)
}

fn arg0(v: i64) -> CapsuleMap {
    vec![("arg0", jvalue!(v))].into()
}

fn container_id(cb: &CoreBroker, name: &str) -> JuizResult<ContainerIdentifier> {
    Ok(cb
        .container_list(false, None)?
        .into_iter()
        .find(|id| id.name == name)
        .unwrap_or_else(|| panic!("container {name} not found")))
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

/// Container の状態を ContainerProcess で読み書きでき、Container ごとに状態が分かれている
#[test]
fn zig_container_keeps_state() -> JuizResult<()> {
    crate::common::logger::init();
    if skip_without_zig_examples(&EXAMPLES) {
        return Ok(());
    }
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            let cb = system.core_broker().lock()?;
            let (inc0, get0) = (find(&cb, "inc0")?, find(&cb, "get0")?);
            let (inc1, get1) = (find(&cb, "inc1")?, find(&cb, "get1")?);

            // 設定ファイルの value が初期値になる。書かなければ既定値
            assert_eq!(call(&cb, &get0, CapsuleMap::new())?, jvalue!(15));
            assert_eq!(call(&cb, &get1, CapsuleMap::new())?, jvalue!(0));

            assert_eq!(call(&cb, &inc0, arg0(3))?, jvalue!(18));
            assert_eq!(call(&cb, &inc0, arg0(-20))?, jvalue!(-2));
            assert_eq!(call(&cb, &get0, CapsuleMap::new())?, jvalue!(-2));
            // 引数を省くとマニフェストの既定値 (1)
            assert_eq!(call(&cb, &inc0, CapsuleMap::new())?, jvalue!(-1));
            // c1 は c0 と別の状態
            assert_eq!(call(&cb, &inc1, arg0(5))?, jvalue!(5));
            assert_eq!(call(&cb, &get0, CapsuleMap::new())?, jvalue!(-1));

            // マニフェストは zigjuiz が組み立てたもの
            let profile = cb.container_profile(&container_id(&cb, "c0")?)?;
            assert_eq!(profile.type_name, "example_container_zig");
            assert_eq!(profile.language, "zig");
            assert_eq!(profile.arguments.len(), 1);
            assert_eq!(profile.arguments[0].name, "value");
            assert_eq!(profile.arguments[0].default, jvalue!(0));
            let p = cb.container_process_profile(&inc0)?;
            assert_eq!(p.type_name, "example_container_zig_increment");
            assert_eq!(p.language, "zig");
            assert_eq!(p.arguments[0].name, "arg0");
            assert_eq!(p.arguments[0].type_name.as_str(), "Int");
            assert_eq!(p.description.as_str(), "Adds arg0 to the value of the container (Zig).");
            assert!(!p.readonly);
            // 状態を *const で受ける ContainerProcess は readonly
            let p = cb.container_process_profile(&get0)?;
            assert!(p.readonly, "{p:?}");

            // 引数の型が違えば引数のエラー (-1102)、Zig のエラーなら -1101。どちらも状態は変わらない
            assert_zig_error(
                call(&cb, &inc0, vec![("arg0", jvalue!("not a number"))].into()),
                -1102,
                "argument 'arg0' of example_container_zig_increment",
            );
            assert_eq!(call(&cb, &inc0, arg0(i64::MAX))?, jvalue!(i64::MAX - 1));
            assert_zig_error(
                call(&cb, &inc0, arg0(2)),
                -1101,
                "example_container_zig_increment returned error.Overflow",
            );
            assert_eq!(call(&cb, &get0, CapsuleMap::new())?, jvalue!(i64::MAX - 1));
            Ok(())
        })
}

/// Container を削除すると、zigjuiz の解放関数が呼ばれて状態の deinit が動く
#[test]
fn zig_container_is_destroyed() -> JuizResult<()> {
    crate::common::logger::init();
    if skip_without_zig_examples(&EXAMPLES) {
        return Ok(());
    }
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            // juiz が開いたのと同じライブラリを開き、生きている Container の数を数える関数を取り出す
            let lib = unsafe { libloading::Library::new(plugin_file("example_container_zig"))? };
            let alive_count: libloading::Symbol<unsafe extern "C" fn() -> i64> =
                unsafe { lib.get(b"example_container_zig_alive_count")? };
            let alive = || unsafe { alive_count() };

            let mut cb = system.core_broker().lock_mut()?;
            let initial = alive();
            assert!(initial >= 2, "c0 と c1 が作られている: {initial}");
            let created = cb.container_create(
                &ContainerManifest::new("example_container_zig").name("c2"),
                vec![("value", jvalue!(7))].into(),
            )?;
            assert_eq!(alive(), initial + 1);

            cb.container_destroy(&created.identifier())?;
            assert_eq!(alive(), initial);
            Ok(())
        })
}

/// 状態の型が違う Container に結びつけても、juiz は落ちずに呼び出しがエラー (-1104) になる
#[test]
fn zig_container_process_rejects_other_containers() -> JuizResult<()> {
    crate::common::logger::init();
    if skip_without_zig_examples(&EXAMPLES) {
        return Ok(());
    }
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            let mut cb = system.core_broker().lock_mut()?;
            let path = plugin_file("mismatched_container_process_zig");
            cb.worker_mut()
                .load_container_process_factory("zig".to_owned(), path.display().to_string())?;

            // OtherState を期待する ContainerProcess を、Counter の Container に結びつける
            let c0 = container_id(&cb, "c0")?;
            let mm = cb.container_process_create(
                &c0,
                &ProcessManifest::new("mismatched_container_process_zig").name("mm0"),
            )?;
            assert_zig_error(
                call(&cb, &mm.identifier(), CapsuleMap::new()),
                -1104,
                "mismatched_container_process_zig: the container state is example_container_state.Counter, but the process expects",
            );

            // その後も正しい組み合わせは呼べる
            let get0 = find(&cb, "get0")?;
            assert_eq!(call(&cb, &get0, CapsuleMap::new())?, jvalue!(15));
            Ok(())
        })
}

/// C++ の Container に Zig の ContainerProcess を結びつけても、目印が違うので -1104 になる
/// (C++ の例 examples/cpp がビルドされているときだけ)
#[test]
fn zig_container_process_rejects_cpp_containers() -> JuizResult<()> {
    crate::common::logger::init();
    if skip_without_zig_examples(&EXAMPLES) {
        return Ok(());
    }
    if !plugin_file("example_container_cpp").exists() {
        eprintln!("skip: C++ の例 (example_container_cpp) がビルドされていません");
        return Ok(());
    }
    let mut manifest = system_manifest();
    manifest["plugins"]["container_factories"]["example_container_cpp"] =
        jvalue!({ "language": "c++", "path": "../target" });
    manifest["containers"]
        .as_array_mut()
        .unwrap()
        .push(jvalue!({ "type_name": "example_container_cpp", "name": "cpp0" }));
    System::new(manifest)?.setup()?.run_and_do_once(|system| {
        let mut cb = system.core_broker().lock_mut()?;
        let cpp0 = container_id(&cb, "cpp0")?;
        let get = cb.container_process_create(
            &cpp0,
            &ProcessManifest::new("example_container_zig_get").name("zigget_on_cpp"),
        )?;
        assert_zig_error(
            call(&cb, &get.identifier(), CapsuleMap::new()),
            -1104,
            "the container is not a Zig (zigjuiz) container",
        );
        Ok(())
    })
}
