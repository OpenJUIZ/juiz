//! C++ の Container を削除すると、プラグインの解放関数 (container_factory_destroy_entry_point) が呼ばれて
//! C++ のオブジェクトが delete されることを確かめるテスト (issue #130)
//!
//! C++ の例は scripts/test.sh (CI も同じ) で ../target にビルドされている前提。

use juiz_core::prelude::*;

fn plugin_file() -> std::path::PathBuf {
    let name = "destroy_check_container_cpp";
    #[cfg(target_os = "macos")]
    let file = format!("lib{name}.dylib");
    #[cfg(target_os = "linux")]
    let file = format!("lib{name}.so");
    #[cfg(target_os = "windows")]
    let file = format!("{name}.dll");
    std::path::PathBuf::from("../target").join(file)
}

fn system_manifest() -> Value {
    jvalue!({
        "name": "cpp_container_destroy_test",
        "plugins": {
            "container_factories": {
                "destroy_check_container_cpp": { "language": "c++", "path": "../target" }
            }
        },
        "containers": [
            { "type_name": "destroy_check_container_cpp", "name": "c0" }
        ]
    })
}

#[test]
fn cpp_container_is_deleted_when_destroyed() -> JuizResult<()> {
    crate::common::logger::init();
    // juiz が読み込むのと同じライブラリを開き、生きている C++ オブジェクトの数を数える関数を取り出す
    let lib = unsafe { libloading::Library::new(plugin_file())? };
    let alive_count: libloading::Symbol<unsafe extern "C" fn() -> i64> =
        unsafe { lib.get(b"destroy_check_container_cpp_alive_count")? };
    let alive = || unsafe { alive_count() };
    let initial = alive();

    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            let mut cb = system.core_broker().lock_mut()?;
            // 設定ファイルの c0 が作られている
            assert_eq!(alive(), initial + 1);

            let created = cb.container_create(
                &ContainerManifest::new("destroy_check_container_cpp").name("c1"),
                CapsuleMap::new(),
            )?;
            assert_eq!(alive(), initial + 2);

            cb.container_destroy(&created.identifier())?;
            assert_eq!(alive(), initial + 1);
            Ok(())
        })?;
    Ok(())
}
