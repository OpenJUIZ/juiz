//! language: "haskell" の Container と ContainerProcess を読み込んで呼べることを確かめるテスト (issue #133)
//!
//! Haskell の例 (examples/haskell) と C++ の例 (examples/cpp) は scripts/test.sh (CI も同じ) で ../target に
//! ビルドされている前提。GHC が無い環境では scripts/test.sh がビルドを飛ばすので、ライブラリが無ければテストも飛ばす。
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
    let missing: Vec<_> = [
        "example_container_hs",
        "example_container_hs_increment",
        "example_container_hs_get",
        "mismatched_container_process_hs",
    ]
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
        "name": "haskell_container_test",
        "plugins": {
            "container_factories": {
                "example_container_hs": {
                    "language": "haskell",
                    "path": "../target",
                    "processes": {
                        "example_container_hs_increment": { "path": "../target" },
                        "example_container_hs_get": { "path": "../target" }
                    }
                },
                "example_container_cpp": { "language": "c++", "path": "../target" }
            }
        },
        "containers": [
            {
                "type_name": "example_container_hs",
                "name": "c0",
                "value": 15,
                "processes": [
                    { "type_name": "example_container_hs_increment", "name": "inc0" },
                    { "type_name": "example_container_hs_get", "name": "get0" }
                ]
            },
            {
                // value を書かなければマニフェストの既定値 (0) になる
                "type_name": "example_container_hs",
                "name": "c1",
                "processes": [
                    { "type_name": "example_container_hs_increment", "name": "inc1" },
                    { "type_name": "example_container_hs_get", "name": "get1" }
                ]
            },
            { "type_name": "example_container_cpp", "name": "cpp0" }
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
    Ok(cb
        .container_process_call(id, args)?
        .lock_as_value(|v| v.clone())?)
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

/// Windows では Haskell の DLL ごとに GHC の RTS が別にあり、別の DLL で作った Container の状態は読めないので、
/// 別の DLL の ContainerProcess の呼び出しはエラーになる (issue #137。同じ DLL にまとめる Component は
/// haskell_component_test で確かめる)
#[cfg(target_os = "windows")]
#[test]
fn haskell_container_process_in_another_dll_is_rejected_on_windows() -> JuizResult<()> {
    crate::common::logger::init();
    if skip_without_haskell_examples() {
        return Ok(());
    }
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            let cb = system.core_broker().lock()?;
            let (inc0, get0) = (find(&cb, "inc0")?, find(&cb, "get0")?);
            let r = cb.container_process_call(&get0, CapsuleMap::new());
            assert!(r.is_err(), "{r:?}");
            let r = cb.container_process_call(&inc0, arg0(3));
            assert!(r.is_err(), "{r:?}");
            Ok(())
        })
}

/// Container の状態を ContainerProcess で読み書きでき、Container ごとに状態が分かれている
#[cfg(not(target_os = "windows"))]
#[test]
fn haskell_container_keeps_state() -> JuizResult<()> {
    crate::common::logger::init();
    if skip_without_haskell_examples() {
        return Ok(());
    }
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            // GHC の major GC の後も状態と関数が残っていることを確かめるため、途中で GC を起こす
            let lib = unsafe { libloading::Library::new(plugin_file("example_container_hs"))? };
            let gc: libloading::Symbol<unsafe extern "C" fn()> = unsafe { lib.get(b"performMajorGC")? };
            let cb = system.core_broker().lock()?;
            let (inc0, get0) = (find(&cb, "inc0")?, find(&cb, "get0")?);
            let (inc1, get1) = (find(&cb, "inc1")?, find(&cb, "get1")?);

            // 設定ファイルの value が初期値になる。書かなければ既定値
            assert_eq!(call(&cb, &get0, CapsuleMap::new())?, jvalue!(15));
            assert_eq!(call(&cb, &get1, CapsuleMap::new())?, jvalue!(0));

            assert_eq!(call(&cb, &inc0, arg0(3))?, jvalue!(18));
            unsafe { gc() };
            assert_eq!(call(&cb, &inc0, arg0(-20))?, jvalue!(-2));
            unsafe { gc() };
            assert_eq!(call(&cb, &get0, CapsuleMap::new())?, jvalue!(-2));
            // c1 は c0 と別の状態
            assert_eq!(call(&cb, &inc1, arg0(5))?, jvalue!(5));
            assert_eq!(call(&cb, &get0, CapsuleMap::new())?, jvalue!(-2));

            // マニフェストは Haskell 側で組み立てたもの
            let profile = cb.container_profile_full(&container_id(&cb, "c0")?)?;
            assert_eq!(profile.type_name, "example_container_hs");
            let p = cb.container_process_profile_full(&inc0)?;
            assert_eq!(p.type_name, "example_container_hs_increment");
            assert_eq!(p.arguments[0].name, "arg0");
            assert_eq!(p.description.as_str(), "Adds arg0 to the value of the container.");

            // 引数の型が違えば Haskell の例外ではなくエラーコードで返り、juiz はエラーにする。状態は変わらない
            let r = cb.container_process_call(&inc0, vec![("arg0", jvalue!("not a number"))].into());
            assert!(r.is_err(), "{r:?}");
            assert_eq!(call(&cb, &get0, CapsuleMap::new())?, jvalue!(-2));
            Ok(())
        })
}

/// Container を削除すると、hsjuiz の解放関数が呼ばれて juizContainerWithDestroy の destroy が動く
#[test]
fn haskell_container_is_destroyed() -> JuizResult<()> {
    crate::common::logger::init();
    if skip_without_haskell_examples() {
        return Ok(());
    }
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            // juiz が開いて RTS を初期化したのと同じライブラリを開き、生きている Container の数を数える関数を取り出す
            // (juiz は Haskell のライブラリを閉じないので、同じものが返る)
            let lib = unsafe { libloading::Library::new(plugin_file("example_container_hs"))? };
            let alive_count: libloading::Symbol<unsafe extern "C" fn() -> i64> =
                unsafe { lib.get(b"example_container_hs_alive_count")? };
            let alive = || unsafe { alive_count() };

            let mut cb = system.core_broker().lock_mut()?;
            let initial = alive();
            assert!(initial >= 2, "c0 と c1 が作られている: {initial}");

            let created = cb.container_create(
                &ContainerManifest::new("example_container_hs").name("c2"),
                vec![("value", jvalue!(7))].into(),
            )?;
            assert_eq!(alive(), initial + 1);

            cb.container_destroy(&created.identifier())?;
            assert_eq!(alive(), initial);
            Ok(())
        })
}

/// 状態の型が違う Container (別の型の Haskell の Container、C++ の Container) に結びつけても、
/// プロセスは落ちずに呼び出しがエラーになる
#[test]
fn haskell_container_process_rejects_other_containers() -> JuizResult<()> {
    crate::common::logger::init();
    if skip_without_haskell_examples() {
        return Ok(());
    }
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            let mut cb = system.core_broker().lock_mut()?;
            let path = plugin_file("mismatched_container_process_hs");
            cb.worker_mut().load_container_process_factory(
                "haskell".to_owned(),
                path.display().to_string(),
            )?;

            // IORef Double を期待する ContainerProcess を、ExampleContainer の Container に結びつける
            let c0 = container_id(&cb, "c0")?;
            let mm = cb.container_process_create(
                &c0,
                &ProcessManifest::new("mismatched_container_process_hs").name("mm0"),
            )?;
            let r = cb.container_process_call(&mm.identifier(), CapsuleMap::new());
            assert!(r.is_err(), "{r:?}");

            // Haskell の ContainerProcess を C++ の Container に結びつける
            let cpp0 = container_id(&cb, "cpp0")?;
            let get = cb.container_process_create(
                &cpp0,
                &ProcessManifest::new("example_container_hs_get").name("hsget_on_cpp"),
            )?;
            let r = cb.container_process_call(&get.identifier(), CapsuleMap::new());
            assert!(r.is_err(), "{r:?}");

            // その後も正しい組み合わせは呼べる (Windows では別の DLL の Container なので呼べない)
            #[cfg(not(target_os = "windows"))]
            {
                let get0 = find(&cb, "get0")?;
                assert_eq!(call(&cb, &get0, CapsuleMap::new())?, jvalue!(15));
            }
            Ok(())
        })
}
